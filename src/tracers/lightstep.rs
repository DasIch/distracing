use crate::api::{
    Event, FinishedSpan, Key, Reference, ReferenceType, Reporter, SpanBuilder, SpanContext,
    SpanContextState, Tracer, Value,
};
use std::convert::TryInto;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Generated code based on lightstep-tracer-common/collector.proto
mod collector {
    include!(concat!(env!("OUT_DIR"), "/lightstep.collector.rs"));
}

/// Generated code based on lightstep-tracer-common/lightstep.proto
mod carrier {
    include!(concat!(env!("OUT_DIR"), "/lightstep.rs"));
}

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
struct LightStepReporter {
    finished_spans: Arc<Mutex<Vec<FinishedSpan>>>,
    max_finished_spans: usize,
    join_handle: std::thread::JoinHandle<()>,
    dropped_spans: AtomicUsize,
}

impl LightStepReporter {
    fn new(max_finished_spans: usize) -> Self {
        let finished_spans = Arc::new(Mutex::new(vec![]));
        let finished_spans_for_thread = finished_spans.clone();
        let join_handle = std::thread::spawn(move || loop {
            let mut spans = vec![];
            {
                let mut current_finished_spans = finished_spans_for_thread
                    .lock()
                    .expect("LightStepReporter.finished_spans RwLock poisoned");
                std::mem::swap(&mut spans, &mut *current_finished_spans);
            }
            if spans.len() > 0 {
                let serialized_spans: Vec<collector::Span> =
                    spans.into_iter().map(serialize_span).collect();
                println!("Serialized: {:?}", serialized_spans);
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        });
        LightStepReporter {
            finished_spans: finished_spans,
            max_finished_spans,
            join_handle,
            dropped_spans: AtomicUsize::new(0),
        }
    }
}

fn serialize_span(span: FinishedSpan) -> collector::Span {
    // TODO: Don't panic, if the wallclock time changes to the past.
    // Could be done by storing an Instant in addition to the start timestamp, may require some
    // hacking to convert SystemTime to Instance, in case a user actually sets a timestamp. See
    // https://stackoverflow.com/questions/35282308/convert-between-c11-clocks for how that might
    // work.
    let duration = span
        .data
        .finish_timestamp
        .expect("BUG: finish_timestamp not set")
        .duration_since(span.data.start_timestamp)
        .expect("Wallclock time moved back into the past :/");
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
        tags: span.data.tags.into_iter().map(serialize_tag).collect(),
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

        if finished_spans.len() > self.max_finished_spans {
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
        // TODO: configuration
        LightStepTracer {
            reporter: Arc::new(LightStepReporter::new(1000)),
        }
    }
}

impl Tracer for LightStepTracer {
    fn span(&self, operation_name: &str) -> SpanBuilder {
        SpanBuilder::new(
            SpanContext::new(Box::new(LightStepSpanContextState::new())),
            self.reporter.clone(),
            operation_name,
        )
    }
}
