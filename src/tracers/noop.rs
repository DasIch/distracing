use crate::api::{SpanContextState, Tracer};

#[derive(Clone, Debug)]
pub struct NoopTracer {}

impl NoopTracer {
    pub fn new() -> Self {
        NoopTracer {}
    }
}

#[derive(Clone, Debug)]
pub struct NoopSpanContextState {}

impl SpanContextState for NoopSpanContextState {}

impl Tracer for NoopTracer {
    type SpanContextState = NoopSpanContextState;

    fn new_span_context_state(&self) -> Self::SpanContextState {
        NoopSpanContextState {}
    }
}
