use crate::api::{
    CarrierMap, Event, FinishedSpan, Key, Reference, ReferenceType, Reporter, Span, SpanBuilder,
    SpanContext, SpanContextCorrupted, SpanContextState, SpanOptions, Tracer, Value,
};
use log::{error, info, warn};
use prost::Message;
use reqwest::blocking::ClientBuilder as ReqwestClientBuilder;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

/// Generated code based on lightstep-tracer-common/collector.proto
mod collector {
    include!(concat!(env!("OUT_DIR"), "/lightstep.collector.rs"));
}

/// Generated code based on lightstep-tracer-common/lightstep.proto
mod carrier {
    include!(concat!(env!("OUT_DIR"), "/lightstep.rs"));
}

const LIGHTSTEP_TRACER_PLATFORM_VERSION: &str = env!("RUSTC_VERSION");
const LIGHTSTEP_TRACER_VERSION: &str = env!("CARGO_PKG_VERSION");

const TEXT_MAP_TRACE_ID_FIELD: &str = "ot-tracer-traceid";
const TEXT_MAP_SPAN_ID_FIELD: &str = "ot-tracer-spanid";
const TEXT_MAP_SAMPLED_FIELD: &str = "ot-tracer-sampled";
const TEXT_MAP_PREFIX_BAGGAGE: &str = "ot-baggage-";

#[derive(Debug, Clone, PartialEq, Eq)]
struct LightStepSpanContextState {
    trace_id: u64,
    span_id: u64,
}

impl LightStepSpanContextState {
    fn new() -> Self {
        use rand::prelude::*;
        LightStepSpanContextState {
            trace_id: rand::thread_rng().gen(),
            span_id: rand::thread_rng().gen(),
        }
    }

    #[allow(clippy::borrowed_box)]
    fn from_trait_object(
        span_context_state: &Box<dyn SpanContextState>,
    ) -> &LightStepSpanContextState {
        span_context_state
            .as_any()
            .downcast_ref::<Self>()
            .expect("SpanContextState created with different Tracer")
    }
}

impl SpanContextState for LightStepSpanContextState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug)]
enum ReportRequestError {
    ReqwestError(reqwest::Error),
    InvalidResponseBody(prost::DecodeError),
}

impl From<reqwest::Error> for ReportRequestError {
    fn from(err: reqwest::Error) -> Self {
        Self::ReqwestError(err)
    }
}

impl From<prost::DecodeError> for ReportRequestError {
    fn from(err: prost::DecodeError) -> Self {
        Self::InvalidResponseBody(err)
    }
}

impl std::fmt::Display for ReportRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::ReqwestError(e) => e.fmt(f),
            Self::InvalidResponseBody(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ReportRequestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReqwestError(e) => Some(e),
            Self::InvalidResponseBody(e) => Some(e),
        }
    }
}

#[derive(Debug)]
struct LightStepReporter {
    config: LightStepConfig,
    finished_spans: Arc<Mutex<Vec<FinishedSpan>>>,
    join_handle: std::thread::JoinHandle<()>,
    dropped_spans: AtomicUsize,
    is_running: Weak<AtomicBool>,
    is_sending: Arc<AtomicBool>,
}

impl LightStepReporter {
    fn new(config: LightStepConfig) -> Self {
        let finished_spans = Arc::new(Mutex::new(Vec::with_capacity(config.buffer_size)));
        let finished_spans_for_thread = finished_spans.clone();

        let config_for_thread = config.clone();

        let is_running_for_thread = Arc::new(AtomicBool::new(true));
        let is_running = Arc::downgrade(&is_running_for_thread);

        let is_sending = Arc::new(AtomicBool::new(false));
        let is_sending_for_thread = is_sending.clone();

        let join_handle = std::thread::spawn(move || {
            let reporter = create_reporter(&config_for_thread);

            // The Python implementation defaults to send an empty string instead of `None` even
            // though the protocol buffer schema would allow the latter. In order to be on the safe
            // side let's copy what LightStep is doing in their implementation.
            //
            // Default: https://github.com/lightstep/lightstep-tracer-python/blob/b8a47e25f085d58b46fa9bd6ee093e77aed5d62c/lightstep/recorder.py#L39
            // Fail on non-str: https://github.com/lightstep/lightstep-tracer-python/blob/b8a47e25f085d58b46fa9bd6ee093e77aed5d62c/lightstep/recorder.py#L53-L54
            // Override of None in report with empty string: https://github.com/lightstep/lightstep-tracer-python/blob/b8a47e25f085d58b46fa9bd6ee093e77aed5d62c/lightstep/http_connection.py#L32
            let access_token = config_for_thread
                .access_token
                .clone()
                .unwrap_or_else(|| "".to_string());
            let auth = Some(collector::Auth {
                access_token: access_token.clone(),
            });
            let collector_url = config_for_thread.collector_url();
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("Content-Type", "application/octet-stream".parse().unwrap());
            headers.insert("Accept", "application/octet-stream".parse().unwrap());
            headers.insert("Lightstep-Access-Token", access_token.parse().unwrap());
            let client = ReqwestClientBuilder::new()
                .connect_timeout(config_for_thread.send_timeout)
                .timeout(config_for_thread.send_timeout)
                .default_headers(headers)
                .build()
                .expect("BUG: LightStepReporter reqwest::blocking::Client creation failed");
            while is_running_for_thread.load(Ordering::SeqCst) {
                let mut spans = Vec::with_capacity(config_for_thread.buffer_size);
                {
                    // If the lock is poisoned, we cannot continue so we might just as well panic.
                    let mut current_finished_spans = finished_spans_for_thread
                        .lock()
                        .expect("LightStepReporter.finished_spans RwLock poisoned");
                    std::mem::swap(&mut spans, &mut *current_finished_spans);
                    if !spans.is_empty() {
                        is_sending_for_thread.store(true, Ordering::SeqCst);
                    }
                }
                if !spans.is_empty() {
                    let response =
                        perform_report_request(&client, &collector_url, &reporter, &auth, spans);
                    match response {
                        Ok(r) => {
                            log_report_response(&r);
                        }
                        Err(err) => {
                            error!("Sending requests to LightStep collector failed: {}", err);
                        }
                    }
                }
                is_sending_for_thread.store(false, Ordering::SeqCst);
                std::thread::sleep(config_for_thread.send_period);
            }
        });
        LightStepReporter {
            config,
            finished_spans,
            join_handle,
            dropped_spans: AtomicUsize::new(0),
            is_running,
            is_sending,
        }
    }

    fn is_running(&self) -> bool {
        match self.is_running.upgrade() {
            Some(b) => b.load(Ordering::SeqCst),
            None => false,
        }
    }

    fn has_pending_spans(&self) -> bool {
        let has_buffered_spans = match self.finished_spans.lock() {
            Ok(finished_spans) => finished_spans.len() > 0,
            Err(_) => {
                // This can only happen, if for whatever reason the reporter thread panicked while
                // it held the lock. Any spans that might have been pending at that point won't be
                // send anymore now. So we default to...
                false
            }
        };
        let is_sending = self.is_sending.load(Ordering::SeqCst);
        has_buffered_spans || is_sending
    }
}

fn log_report_response(response: &collector::ReportResponse) {
    for message in &response.errors {
        error!("Received message from LightStep: {}", message);
    }
    for message in &response.warnings {
        warn!("Received message from LightStep: {}", message);
    }
    for message in &response.infos {
        info!("Received message from LightStep: {}", message);
    }
}

fn perform_report_request(
    client: &reqwest::blocking::Client,
    collector_url: &str,
    reporter: &collector::Reporter,
    auth: &Option<collector::Auth>,
    spans: Vec<FinishedSpan>,
) -> Result<collector::ReportResponse, ReportRequestError> {
    // TODO: Improve resilience!
    let request_body = create_report_request_body(&reporter, &auth, spans);
    let mut http_response = client.post(collector_url).body(request_body).send()?;
    parse_response(&mut http_response)
}

fn create_report_request_body(
    reporter: &collector::Reporter,
    auth: &Option<collector::Auth>,
    spans: Vec<FinishedSpan>,
) -> Vec<u8> {
    let serialized_spans: Vec<collector::Span> = spans.into_iter().map(serialize_span).collect();
    let request = collector::ReportRequest {
        reporter: Some(reporter.clone()),
        auth: auth.clone(),
        spans: serialized_spans,
        timestamp_offset_micros: 0,
        internal_metrics: None,
    };
    let mut body = Vec::with_capacity(request.encoded_len());
    request
        .encode(&mut body)
        .expect("BUG: Buffer for ReportRequest has insufficient capacity");
    body
}

fn parse_response(
    http_response: &mut reqwest::blocking::Response,
) -> Result<collector::ReportResponse, ReportRequestError> {
    let mut body: Vec<u8> = vec![];
    http_response.copy_to(&mut body)?;
    let response = collector::ReportResponse::decode(body)?;
    Ok(response)
}

fn serialize_span(span: FinishedSpan) -> collector::Span {
    // Either Span.finish or Drop.drop sets the duration. Accordingly we should be able to fairly
    // safely assume that it's set here.
    let duration = span
        .data
        .duration
        .expect("BUG: FinishedSpan duration not set");
    collector::Span {
        span_context: Some(serialize_span_context(span.data.span_context)),
        operation_name: span.data.operation_name,
        references: span
            .data
            .references
            .into_iter()
            .map(serialize_reference)
            .collect(),
        start_timestamp: Some(span.data.start_timestamp.into()),
        duration_micros: duration.as_micros() as u64,
        tags: serialize_tags(span.data.tags),
        logs: span.data.log.into_iter().map(serialize_log_entry).collect(),
    }
}

fn serialize_span_context(span_context: SpanContext) -> collector::SpanContext {
    // The unwrap is safe under the assumption that this is only called on Spans created by the
    // LightStepTracer. As long as nobody makes this function `pub` this should be a reasonable
    // assumption.
    let state = span_context
        .state
        .as_any()
        .downcast_ref::<LightStepSpanContextState>()
        .unwrap();
    collector::SpanContext {
        trace_id: state.trace_id,
        span_id: state.span_id,
        baggage: span_context
            .baggage_items
            .into_iter()
            .map(|(k, v)| (k.into_owned(), v))
            .collect(),
    }
}

fn serialize_reference(reference: Reference) -> collector::Reference {
    let relationship = match reference.rtype {
        ReferenceType::ChildOf => collector::reference::Relationship::ChildOf,
        ReferenceType::FollowsFrom => collector::reference::Relationship::FollowsFrom,
    };
    collector::Reference {
        // for some reason this is stored as an i32 ¯\_(ツ)_/¯
        relationship: relationship.into(),
        span_context: Some(serialize_span_context(reference.to)),
    }
}

fn serialize_tags(tags: HashMap<Key, Value>) -> Vec<collector::KeyValue> {
    tags.into_iter().map(serialize_tag).collect()
}

fn serialize_tag((key, value): (Key, Value)) -> collector::KeyValue {
    let key = key.into_owned();
    collector::KeyValue {
        key,
        value: Some(serialize_value(value)),
    }
}

fn serialize_value(value: Value) -> collector::key_value::Value {
    use collector::key_value;
    match value {
        Value::String(s) => key_value::Value::StringValue(s),
        Value::Bool(b) => key_value::Value::BoolValue(b),
        Value::F32(n) => key_value::Value::DoubleValue(n as f64),
        Value::F64(n) => key_value::Value::DoubleValue(n),
        Value::U8(n) => serialize_numeric_to_value(n),
        Value::U16(n) => serialize_numeric_to_value(n),
        Value::U32(n) => serialize_numeric_to_value(n),
        Value::U64(n) => serialize_numeric_to_value(n),
        Value::U128(n) => serialize_numeric_to_value(n),
        Value::I8(n) => serialize_numeric_to_value(n),
        Value::I16(n) => serialize_numeric_to_value(n),
        Value::I32(n) => serialize_numeric_to_value(n),
        Value::I64(n) => serialize_numeric_to_value(n),
        Value::I128(n) => serialize_numeric_to_value(n),
        Value::USize(n) => serialize_numeric_to_value(n),
        Value::ISize(n) => serialize_numeric_to_value(n),
    }
}

fn serialize_numeric_to_value<N: Copy + TryInto<i64> + std::string::ToString>(
    n: N,
) -> collector::key_value::Value {
    match n.try_into() {
        Ok(n) => collector::key_value::Value::IntValue(n),
        Err(_) => collector::key_value::Value::StringValue(n.to_string()),
    }
}

fn serialize_log_entry((timestamp, events): (SystemTime, Vec<Event>)) -> collector::Log {
    collector::Log {
        timestamp: Some(timestamp.into()),
        fields: events
            .into_iter()
            .map(|e| collector::KeyValue {
                key: e.key.into_owned(),
                value: Some(serialize_value(e.value)),
            })
            .collect(),
    }
}

impl Reporter for LightStepReporter {
    fn report(&self, finished_span: FinishedSpan) {
        if let Ok(mut finished_spans) = self.finished_spans.lock() {
            if finished_spans.len() >= self.config.buffer_size {
                // We've reached the buffer size, let's drop this span.
                self.dropped_spans.fetch_add(1, Ordering::SeqCst);
            } else {
                finished_spans.push(finished_span);
            }
        } else {
            // The thread panicked so this span gets dropped.
            self.dropped_spans.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// A tracer that sends finished spans to a [LightStep](https://lightstep.com/) collector.
#[derive(Clone, Debug)]
pub struct LightStepTracer {
    reporter: Arc<LightStepReporter>,
}

impl LightStepTracer {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self::build().build()
    }

    /// Create an instance of the LightStep tracer.
    ///
    /// Example:
    ///
    /// ```rust
    /// # use distracing::LightStepTracer;
    /// distracing::set_tracer(LightStepTracer::build()
    ///     .access_token("<your-lightstep-access-token")
    ///     .component_name("my-application")
    ///     .tag("team", "my-team")
    ///     .build()
    /// )
    /// ```
    ///
    /// Further options are documented as part of the `LightStepConfig` struct.
    pub fn build() -> LightStepConfig {
        LightStepConfig::new()
    }

    fn new_with_config(config: LightStepConfig) -> Self {
        Self {
            reporter: Arc::new(LightStepReporter::new(config)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LightStepConfig {
    access_token: Option<String>,
    component_name: Option<String>,
    tags: HashMap<Key, Value>,
    collector_host: String,
    collector_port: usize,
    buffer_size: usize,
    send_period: Duration,
    send_timeout: Duration,
}

impl LightStepConfig {
    fn new() -> Self {
        LightStepConfig {
            access_token: None,
            component_name: None,
            tags: HashMap::new(),
            collector_host: "collector.lightstep.com".to_string(),
            collector_port: 443,

            // Copied from LightStep's own tracer implementations
            // Ref: https://github.com/lightstep/lightstep-tracer-python/blob/e146b1cad82c0b4c783a3a77872d816156c06dde/lightstep/constants.py#L6
            buffer_size: 1000,

            // Copied from LightStep's own tracer implementations
            // Ref: https://github.com/lightstep/lightstep-tracer-python/blob/e146b1cad82c0b4c783a3a77872d816156c06dde/lightstep/constants.py#L5
            send_period: Duration::from_millis(2_500), // 2.5s copied from LightStep's own tracer implementations

            // Copied from LightStep's own tracer implementations
            // Ref: https://github.com/lightstep/lightstep-tracer-python/blob/b8a47e25f085d58b46fa9bd6ee093e77aed5d62c/lightstep/recorder.py#L50
            send_timeout: Duration::from_secs(30),
        }
    }

    /// Sets the LightStep access token for your collector.
    pub fn access_token<S: Into<String>>(&mut self, access_token: S) -> &mut Self {
        self.access_token = Some(access_token.into());
        self
    }

    /// Sets the component name.
    pub fn component_name<S: Into<String>>(&mut self, component_name: S) -> &mut Self {
        self.component_name = Some(component_name.into());
        self
    }

    /// Add a global tag that applies to all spans.
    ///
    /// Tags set this way will show up in the "Additional Details" section
    /// of the trace view.
    pub fn tag<K: Into<Key>, V: Into<Value>>(&mut self, key: K, value: V) -> &mut Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Specify the hostname of the collector you are using.
    ///
    /// Default: `collector.lightstep.com`.
    pub fn collector_host<S: Into<String>>(&mut self, collector_host: S) -> &mut Self {
        self.collector_host = collector_host.into();
        self
    }

    /// Specify the port of the collector you are using.
    ///
    /// This implementation uses Protocol Buffers over HTTP, so you should use
    /// the port corresponding to that.
    ///
    /// Default: 443
    pub fn collector_port(&mut self, collector_port: usize) -> &mut Self {
        self.collector_port = collector_port;
        self
    }

    fn collector_url(&self) -> String {
        format!(
            "https://{}:{}/api/v2/reports",
            self.collector_host, self.collector_port
        )
    }

    /// Finished spans are buffered in memory and send in batches to the
    /// collector. This sets the size of that buffer in the number of spans.
    ///
    /// Default: 1000
    pub fn buffer_size(&mut self, buffer_size: usize) -> &mut Self {
        self.buffer_size = buffer_size;
        self
    }

    /// Finished spans are buffered in memory and send in batches to the
    /// collector periodically. This sets the duration for which the tracer
    /// sleeps inbetween sending spans.
    ///
    /// Default: 2.5s
    pub fn send_period(&mut self, send_period: Duration) -> &mut Self {
        self.send_period = send_period;
        self
    }

    pub fn build(&mut self) -> LightStepTracer {
        LightStepTracer::new_with_config(self.clone())
    }
}

fn create_reporter(config: &LightStepConfig) -> collector::Reporter {
    use rand::prelude::*;
    let reporter_id: u64 = rand::thread_rng().gen();
    let mut config = config.clone();

    // Normally this tag has the language as value. However I don't want to force LightStep to
    // break convention, should they provide a native tracer implementation
    config.tag("lightstep.tracer_platform", "rust (distracing)");
    config.tag(
        "lightstep.tracer_platform_version",
        LIGHTSTEP_TRACER_PLATFORM_VERSION,
    );
    config.tag("lightstep.tracer_version", LIGHTSTEP_TRACER_VERSION);
    let component_name = config
        .component_name
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or_else(|| "None")
        .to_string();
    config.tag("lightstep.component_name", component_name);
    config.tag("lightstep.guid", reporter_id);
    let tags = serialize_tags(config.tags.clone());
    let tags = tags
        .into_iter()
        .map(|kv| collector::KeyValue {
            key: kv.key,
            value: kv.value.map(|v| match v {
                collector::key_value::Value::BoolValue(b) => {
                    collector::key_value::Value::StringValue(b.to_string())
                }
                collector::key_value::Value::IntValue(n) => {
                    collector::key_value::Value::StringValue(n.to_string())
                }
                collector::key_value::Value::DoubleValue(n) => {
                    collector::key_value::Value::StringValue(n.to_string())
                }
                collector::key_value::Value::JsonValue(s) => {
                    collector::key_value::Value::StringValue(s)
                }
                collector::key_value::Value::StringValue(s) => {
                    collector::key_value::Value::StringValue(s)
                }
            }),
        })
        .collect();

    // LightStep provided tracer implementations actually force values to be Value::StringValue. I
    // don't know whether this is actually necessary but let's do it to be safe until I get an a
    // clear answer from them.
    collector::Reporter { reporter_id, tags }
}

impl Tracer for LightStepTracer {
    fn span<'a>(&'a self, operation_name: &str) -> SpanBuilder<'a> {
        SpanBuilder::new(Box::new(self), operation_name)
    }

    fn span_with_options(&self, options: SpanOptions) -> Span {
        let mut state = LightStepSpanContextState::new();
        if !options.references.is_empty() {
            // Unfortunately multiple references won't really work out :/ At least the Python
            // LightStep tracer has the same problem and it's not clear to me how we might be able
            // to address this.
            //
            // Downcasting could break, if someone passes a span context created from a different
            // tracer but that seems unlikely.
            state.trace_id =
                LightStepSpanContextState::from_trait_object(&options.references[0].to.state)
                    .trace_id;
        }
        Span::new(
            SpanContext::new(Box::new(state)),
            self.reporter.clone(),
            options,
        )
    }

    fn inject_into_text_map(&self, span_context: &SpanContext, carrier: &mut dyn CarrierMap) {
        let state = LightStepSpanContextState::from_trait_object(&span_context.state);

        carrier.set(TEXT_MAP_TRACE_ID_FIELD, &format!("{:x}", state.trace_id));
        carrier.set(TEXT_MAP_SPAN_ID_FIELD, &format!("{:x}", state.span_id));
        carrier.set(TEXT_MAP_SAMPLED_FIELD, "false");
    }

    fn extract_from_text_map(
        &self,
        carrier: &dyn CarrierMap,
    ) -> Result<SpanContext, SpanContextCorrupted> {
        let mut trace_id: Option<u64> = None;
        let mut span_id: Option<u64> = None;
        let mut baggage_items: HashMap<Cow<'static, str>, String> = HashMap::new();

        for key in carrier.keys() {
            if key == TEXT_MAP_TRACE_ID_FIELD {
                match u64::from_str_radix(carrier.get(&key).unwrap(), 16) {
                    Ok(tid) => trace_id = Some(tid),
                    Err(_) => {
                        return Err(SpanContextCorrupted {
                            message: format!(
                                "{} is not a hexadecimal u64",
                                TEXT_MAP_TRACE_ID_FIELD
                            ),
                        })
                    }
                };
            } else if key == TEXT_MAP_SPAN_ID_FIELD {
                match u64::from_str_radix(carrier.get(&key).unwrap(), 16) {
                    Ok(sid) => span_id = Some(sid),
                    Err(_) => {
                        return Err(SpanContextCorrupted {
                            message: format!("{} is not a hexadecimal u64", TEXT_MAP_SPAN_ID_FIELD),
                        })
                    }
                };
            } else if key.starts_with(TEXT_MAP_PREFIX_BAGGAGE) {
                let baggage_key = &key[TEXT_MAP_PREFIX_BAGGAGE.len()..];
                let baggage_value = carrier.get(baggage_key).unwrap();
                baggage_items.insert(
                    Cow::Owned(baggage_key.to_string()),
                    baggage_value.to_string(),
                );
            }
        }

        if trace_id.is_none() {
            return Err(SpanContextCorrupted {
                message: format!("{} is missing", TEXT_MAP_TRACE_ID_FIELD),
            });
        }
        if span_id.is_none() {
            return Err(SpanContextCorrupted {
                message: format!("{} is missing", TEXT_MAP_SPAN_ID_FIELD),
            });
        }

        Ok(SpanContext {
            state: Box::new(LightStepSpanContextState {
                trace_id: trace_id.unwrap(),
                span_id: span_id.unwrap(),
            }),
            baggage_items,
        })
    }

    fn inject_into_binary(&self, span_context: &SpanContext) -> Vec<u8> {
        let state = LightStepSpanContextState::from_trait_object(&span_context.state);
        let carrier = carrier::BinaryCarrier {
            deprecated_text_ctx: vec![],
            basic_ctx: Some(carrier::BasicTracerCarrier {
                trace_id: state.trace_id,
                span_id: state.span_id,
                sampled: false,
                baggage_items: span_context
                    .baggage_items
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k.into_owned(), v))
                    .collect(),
            }),
        };
        let mut buffer: Vec<u8> = Vec::with_capacity(carrier.encoded_len());
        // Ignore the result because the only possible failure is the buffer running out of
        // capacity and it's not like that is going to happen.
        let _ = carrier.encode(&mut buffer);
        base64::encode(&buffer).as_bytes().to_vec()
    }

    fn extract_from_binary(&self, carrier: &[u8]) -> Result<SpanContext, SpanContextCorrupted> {
        let carrier = carrier::BinaryCarrier::decode(base64::decode(carrier)?)?;
        let basic_ctx = match carrier.basic_ctx {
            Some(basic_ctx) => basic_ctx,
            None => {
                return Err(SpanContextCorrupted {
                    message: "missing basic_ctx".to_string(),
                })
            }
        };
        Ok(SpanContext {
            state: Box::new(LightStepSpanContextState {
                trace_id: basic_ctx.trace_id,
                span_id: basic_ctx.span_id,
            }),
            baggage_items: basic_ctx
                .baggage_items
                .into_iter()
                .map(|(k, v)| (Cow::Owned(k), v))
                .collect(),
        })
    }

    fn flush(&self) {
        while self.reporter.is_running() && self.reporter.has_pending_spans() {
            std::thread::sleep(self.reporter.config.send_period);
        }
    }
}

impl From<base64::DecodeError> for SpanContextCorrupted {
    fn from(err: base64::DecodeError) -> Self {
        Self {
            message: format!("{}", err),
        }
    }
}

impl From<prost::DecodeError> for SpanContextCorrupted {
    fn from(err: prost::DecodeError) -> Self {
        Self {
            message: format!("{}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LightStepSpanContextState, LightStepTracer};
    use crate::api::{SpanContext, Tracer};
    use std::collections::HashMap;

    fn assert_span_context_eq(a: &SpanContext, b: &SpanContext) {
        assert_eq!(a.baggage_items, b.baggage_items);
        let a_state = LightStepSpanContextState::from_trait_object(&a.state);
        let b_state = LightStepSpanContextState::from_trait_object(&b.state);
        assert_eq!(a_state, b_state);
    }

    #[test]
    fn test_text_map_inject_and_extract() {
        let tracer = LightStepTracer::new();
        let span = tracer.span("foo").start();
        let span_context = span.span_context();
        let mut text_map: HashMap<String, String> = HashMap::new();
        tracer.inject_into_text_map(span_context, &mut text_map);
        let extracted_span_context = tracer.extract_from_text_map(&text_map).unwrap();
        assert_span_context_eq(span_context, &extracted_span_context);
    }

    #[test]
    fn test_binary_inject_and_extract() {
        let tracer = LightStepTracer::new();
        let span = tracer.span("foo").start();
        let span_context = span.span_context();
        let binary = tracer.inject_into_binary(span_context);
        let extracted_span_context = tracer.extract_from_binary(&binary).unwrap();
        assert_span_context_eq(span_context, &extracted_span_context);
    }
}
