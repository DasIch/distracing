use crate::api::{FinishedSpan, Reporter, SpanBuilder, SpanContext, SpanContextState, Tracer};
use std::sync::Arc;

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

impl SpanContextState for LightStepSpanContextState {}

#[derive(Debug)]
struct LightStepReporter {}

impl Reporter for LightStepReporter {
    fn report(&self, _finished_span: FinishedSpan) {}
}

#[derive(Clone, Debug)]
pub struct LightStepTracer {
    reporter: Arc<LightStepReporter>,
}

impl LightStepTracer {
    pub fn new() -> Self {
        LightStepTracer {
            reporter: Arc::new(LightStepReporter {}),
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
