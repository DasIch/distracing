use crate::api::{
    FinishedSpan, Reporter, Span, SpanBuilder, SpanContext, SpanContextState, SpanOptions, Tracer,
};
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

impl SpanContextState for NoopSpanContextState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Tracer for NoopTracer {
    fn span<'a>(&'a self, operation_name: &str) -> SpanBuilder<'a> {
        SpanBuilder::new(Box::new(self), operation_name)
    }

    fn span_with_options(&self, options: SpanOptions) -> Span {
        Span::new(
            SpanContext::new(Box::new(NoopSpanContextState {})),
            self.reporter.clone(),
            options,
        )
    }
}
