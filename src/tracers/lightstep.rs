use crate::api::{
    Event, FinishedSpan, Key, Reference, ReferenceType, Reporter, Span, SpanBuilder, SpanContext,
    SpanContextState, SpanOptions, Tracer, Value,
};
use log::{error, info, warn};
use prost::Message;
use reqwest::blocking::ClientBuilder as ReqwestClientBuilder;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// Generated code based on lightstep-tracer-common/collector.proto
mod collector {
    include!(concat!(env!("OUT_DIR"), "/lightstep.collector.rs"));
}

/// Generated code based on lightstep-tracer-common/lightstep.proto
mod carrier {
    include!(concat!(env!("OUT_DIR"), "/lightstep.rs"));
}

const LIGHTSTEP_TRACER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
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
}

impl LightStepReporter {
    fn new(config: LightStepConfig) -> Self {
        let finished_spans = Arc::new(Mutex::new(Vec::with_capacity(config.buffer_size)));

        let config_for_thread = config.clone();
        let finished_spans_for_thread = finished_spans.clone();
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
                .unwrap_or("".to_string());
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
                .expect("LightStepReporter reqwest::blocking::Client creation failed");
            loop {
                let mut spans = Vec::with_capacity(config_for_thread.buffer_size);
                {
                    let mut current_finished_spans = finished_spans_for_thread
                        .lock()
                        .expect("LightStepReporter.finished_spans RwLock poisoned");
                    std::mem::swap(&mut spans, &mut *current_finished_spans);
                }
                if spans.len() > 0 {
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
                std::thread::sleep(config_for_thread.send_period);
            }
        });
        LightStepReporter {
            config,
            finished_spans: finished_spans,
            join_handle,
            dropped_spans: AtomicUsize::new(0),
        }
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
        .expect("Buffer for ReportRequest has insufficient capacity");
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
        let mut finished_spans = self
            .finished_spans
            .lock()
            .expect("LightStepReporter.finished_spans Mutex poisoned");

        if finished_spans.len() > self.config.buffer_size {
            self.dropped_spans.fetch_add(1, Ordering::SeqCst);
            return;
        }
        finished_spans.push(finished_span);
    }
}

#[derive(Clone, Debug)]
pub struct LightStepTracer {
    reporter: Arc<LightStepReporter>,
}

impl LightStepTracer {
    pub fn new() -> Self {
        Self::build().build()
    }

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

    pub fn access_token<S: Into<String>>(&mut self, access_token: S) -> &mut Self {
        self.access_token = Some(access_token.into());
        self
    }

    pub fn component_name<S: Into<String>>(&mut self, component_name: S) -> &mut Self {
        self.component_name = Some(component_name.into());
        self
    }

    pub fn tag<K: Into<Key>, V: Into<Value>>(&mut self, key: K, value: V) -> &mut Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    pub fn collector_host<S: Into<String>>(&mut self, collector_host: S) -> &mut Self {
        self.collector_host = collector_host.into();
        self
    }

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

    pub fn buffer_size(&mut self, buffer_size: usize) -> &mut Self {
        self.buffer_size = buffer_size;
        self
    }

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

    // TODO: Add "lightstep.tracer_platform_version" Should be the rust compiler version

    // Normally this tag has the language as value. However I don't want to force LightStep to
    // break convention, should they provide a native tracer implementation
    config.tag("lightstep.tracer_platform", "rust (distracing)");
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
        if options.references.len() > 0 {
            // Unfortunately multiple references won't really work out :/ At least the Python
            // LightStep tracer has the same problem and it's not clear to me how we might be able
            // to address this.
            //
            // Downcasting could break, if someone passes a span context created from a different
            // tracer but that seems unlikely.
            state.trace_id = options.references[0]
                .to
                .state
                .as_any()
                .downcast_ref::<LightStepSpanContextState>()
                .expect("Span references SpanContext created with a different Tracer")
                .trace_id;
        }
        Span::new(
            SpanContext::new(Box::new(state)),
            self.reporter.clone(),
            options,
        )
    }
}
