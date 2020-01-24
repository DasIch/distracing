use super::proto::collector;
use crate::span::{FinishedSpan, Reporter};
use log::{error, info, warn};
use prost::Message;
use reqwest::blocking::ClientBuilder as ReqwestClientBuilder;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

const LIGHTSTEP_TRACER_PLATFORM_VERSION: &str = env!("RUSTC_VERSION");
const LIGHTSTEP_TRACER_VERSION: &str = env!("CARGO_PKG_VERSION");

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

fn create_reporter(config: &super::LightStepConfig) -> collector::Reporter {
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
    let tags: Vec<collector::KeyValue> =
        config.tags.clone().into_iter().map(|t| t.into()).collect();
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

#[derive(Debug)]
pub struct LightStepReporter {
    pub config: super::LightStepConfig,
    finished_spans: Arc<Mutex<Vec<FinishedSpan>>>,
    join_handle: std::thread::JoinHandle<()>,
    dropped_spans: AtomicUsize,
    is_running: Weak<AtomicBool>,
    is_sending: Arc<AtomicBool>,
}

impl LightStepReporter {
    pub fn new(config: super::LightStepConfig) -> Self {
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

    pub fn is_running(&self) -> bool {
        match self.is_running.upgrade() {
            Some(b) => b.load(Ordering::SeqCst),
            None => false,
        }
    }

    pub fn has_pending_spans(&self) -> bool {
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
    let serialized_spans: Vec<collector::Span> = spans.into_iter().map(|s| s.into()).collect();
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
