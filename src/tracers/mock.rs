use crate::api::{FinishedSpan, Reporter, SpanBuilder, SpanContext, SpanContextState, Tracer};
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

impl SpanContextState for MockSpanContextState {}

impl Tracer for MockTracer {
    fn span(&self, operation_name: &str) -> SpanBuilder {
        SpanBuilder::new(
            SpanContext::new(Box::new(MockSpanContextState {})),
            self.reporter.clone(),
            operation_name,
        )
    }
}
