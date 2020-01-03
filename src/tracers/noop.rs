use crate::api::{FinishedSpan, Reporter, SpanBuilder, SpanContext, SpanContextState, Tracer};
use std::sync::Arc;

#[derive(Debug)]
struct NoopReporter {}

impl Reporter for NoopReporter {
    fn report(&self, _finished_span: FinishedSpan) {}
}

#[derive(Clone, Debug)]
pub struct NoopTracer {
    reporter: Arc<NoopReporter>,
}

impl NoopTracer {
    pub fn new() -> Self {
        NoopTracer {
            reporter: Arc::new(NoopReporter {}),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NoopSpanContextState {}

impl SpanContextState for NoopSpanContextState {}

impl Tracer for NoopTracer {
    fn span(&self, operation_name: &str) -> SpanBuilder {
        SpanBuilder::new(
            SpanContext::new(Box::new(NoopSpanContextState {})),
            self.reporter.clone(),
            operation_name,
        )
    }
}
