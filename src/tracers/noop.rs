use crate::span::{FinishedSpan, Reporter, Span, SpanContext, SpanContextState, SpanOptions};
use crate::tracer::{CarrierMap, SpanBuilder, SpanContextCorrupted, Tracer};
use std::sync::Arc;

#[derive(Debug)]
struct NoopReporter {}

impl Reporter for NoopReporter {
    fn report(&self, _finished_span: FinishedSpan) {}
}

/// A tracer that does nothing at all.
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
struct NoopSpanContextState {}

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

    fn inject_into_text_map(&self, _span_context: &SpanContext, _carrier: &mut dyn CarrierMap) {}

    fn extract_from_text_map(
        &self,
        _carrier: &dyn CarrierMap,
    ) -> Result<SpanContext, SpanContextCorrupted> {
        Ok(SpanContext::new(Box::new(NoopSpanContextState {})))
    }

    fn inject_into_binary(&self, _span_context: &SpanContext) -> Vec<u8> {
        vec![]
    }

    fn extract_from_binary(&self, _carrier: &[u8]) -> Result<SpanContext, SpanContextCorrupted> {
        Ok(SpanContext::new(Box::new(NoopSpanContextState {})))
    }
}
