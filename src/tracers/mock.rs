use crate::api::{
    CarrierMap, FinishedSpan, Reporter, Span, SpanBuilder, SpanContext, SpanContextCorrupted,
    SpanContextState, SpanOptions, Tracer,
};
use std::sync::{Arc, RwLock};

#[derive(Debug)]
struct MockReporter {
    finished_spans: RwLock<Vec<FinishedSpan>>,
}

impl Reporter for MockReporter {
    fn report(&self, finished_span: FinishedSpan) {
        let mut finished_spans = self
            .finished_spans
            .write()
            .expect("MockReporter.finished_spans RwLock poisoned");
        finished_spans.push(finished_span);
    }
}

#[derive(Clone, Debug)]
pub struct MockTracer {
    reporter: Arc<MockReporter>,
}

impl MockTracer {
    pub fn new() -> Self {
        MockTracer {
            reporter: Arc::new(MockReporter {
                finished_spans: RwLock::new(vec![]),
            }),
        }
    }

    pub fn finished_spans(&self) -> std::sync::RwLockReadGuard<'_, std::vec::Vec<FinishedSpan>> {
        self.reporter
            .finished_spans
            .read()
            .expect("MockReporter.finished_spans RwLock poisoned")
    }
}

#[derive(Clone, Debug)]
pub struct MockSpanContextState {}

impl SpanContextState for MockSpanContextState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Tracer for MockTracer {
    fn span<'a>(&'a self, operation_name: &str) -> SpanBuilder<'a> {
        SpanBuilder::new(Box::new(self), operation_name)
    }

    fn span_with_options(&self, options: SpanOptions) -> Span {
        Span::new(
            SpanContext::new(Box::new(MockSpanContextState {})),
            self.reporter.clone(),
            options,
        )
    }

    fn inject_into_text_map(&self, _span_context: &SpanContext, _carrier: &mut dyn CarrierMap) {}

    fn extract_from_text_map(
        &self,
        _carrier: &dyn CarrierMap,
    ) -> Result<SpanContext, SpanContextCorrupted> {
        Ok(SpanContext::new(Box::new(MockSpanContextState {})))
    }

    fn inject_into_binary(&self, _span_context: &SpanContext) -> Vec<u8> {
        vec![]
    }

    fn extract_from_binary(&self, _carrier: &[u8]) -> Result<SpanContext, SpanContextCorrupted> {
        Ok(SpanContext::new(Box::new(MockSpanContextState {})))
    }
}
