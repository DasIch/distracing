use crate::api::{SpanContextState, Tracer};
use crate::tracers::noop::NoopTracer;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
struct BoxedSpanContextState(Box<dyn SpanContextState>);

impl SpanContextState for BoxedSpanContextState {}

trait GenericTracer: Send + Sync {
    fn new_span_context_state_boxed(&self) -> Box<dyn SpanContextState>;
}

impl<S: SpanContextState + Clone + 'static> GenericTracer
    for Box<dyn Tracer<SpanContextState = S>>
{
    fn new_span_context_state_boxed(&self) -> Box<dyn SpanContextState> {
        Box::new(self.new_span_context_state())
    }
}

impl Tracer for dyn GenericTracer {
    type SpanContextState = BoxedSpanContextState;

    fn new_span_context_state(&self) -> Self::SpanContextState {
        BoxedSpanContextState(self.new_span_context_state_boxed())
    }
}

pub struct GlobalTracer {
    tracer: Box<dyn GenericTracer>,
}

impl GlobalTracer {
    fn new<S, T>(tracer: T) -> Self
    where
        S: SpanContextState + Clone + 'static,
        T: Tracer<SpanContextState = S> + 'static,
    {
        let generic_tracer: Box<dyn Tracer<SpanContextState = S>> = Box::new(tracer);
        GlobalTracer {
            tracer: Box::new(generic_tracer),
        }
    }
}

lazy_static::lazy_static! {
    static ref GLOBAL_TRACER: RwLock<Arc<GlobalTracer>> = RwLock::new(Arc::new(GlobalTracer::new(NoopTracer::new())));
}

pub fn tracer() -> Arc<GlobalTracer> {
    GLOBAL_TRACER
        .read()
        .expect("GLOBAL_TRACER RwLock poisoned")
        .clone()
}

pub fn set_tracer<S, T>(new_tracer: T)
where
    S: SpanContextState + Clone + 'static,
    T: Tracer<SpanContextState = S> + 'static,
{
    let mut global_tracer = GLOBAL_TRACER
        .write()
        .expect("GLOBAL_TRACER RwLock poisoned");
    *global_tracer = Arc::new(GlobalTracer::new(new_tracer));
}
