use crate::api::{FinishedSpan, Reporter, SpanBuilder, SpanContext, SpanContextState, Tracer};
use std::sync::atomic::{AtomicUsize, Ordering};
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
struct LightStepReporter {
    finished_spans_incoming: crossbeam_channel::Sender<FinishedSpan>,
    finished_spans_outgoing: crossbeam_channel::Receiver<FinishedSpan>,
    dropped_spans: AtomicUsize,
}

impl LightStepReporter {
    fn new(buffer_capacity: usize) -> Self {
        let (s, r) = crossbeam_channel::bounded(buffer_capacity);
        LightStepReporter {
            finished_spans_incoming: s,
            finished_spans_outgoing: r,
            dropped_spans: AtomicUsize::new(0),
        }
    }
}

impl Reporter for LightStepReporter {
    fn report(&self, mut finished_span: FinishedSpan) {
        // Add the finished_span to the channel, if that fails because the channel is full drop the
        // oldest span from the channel and try again.
        loop {
            finished_span = match self.finished_spans_incoming.try_send(finished_span) {
                Ok(()) => return,
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    // Reporting thread is dead. We are going to have to drop this span.
                    self.dropped_spans.fetch_add(1, Ordering::SeqCst);
                    return;
                }
                Err(crossbeam_channel::TrySendError::Full(finished_span)) => finished_span,
            };

            // We don't care about the result of this call because:
            // 1. If it's Ok, we're good.
            // 2. If it's Empty, we're also good.
            // 3. It can't be disconnected because we hold a sender.
            let _ = self.finished_spans_outgoing.try_recv();
            self.dropped_spans.fetch_add(1, Ordering::SeqCst);
        }
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
            reporter: Arc::new(LightStepReporter::new(64)),
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
