use crate::api::{SpanBuilder, SpanContext, SpanContextState, Tracer};

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
    fn span(&self, operation_name: &str) -> SpanBuilder {
        SpanBuilder::new(
            SpanContext::new(Box::new(NoopSpanContextState {})),
            operation_name,
        )
    }
}
