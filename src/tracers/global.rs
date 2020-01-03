use crate::api::Tracer;
use crate::tracers::noop::NoopTracer;
use std::sync::{Arc, RwLock};

lazy_static::lazy_static! {
    static ref GLOBAL_TRACER: RwLock<Arc<dyn Tracer>> = RwLock::new(Arc::new(NoopTracer::new()));
}

pub fn tracer() -> Arc<dyn Tracer> {
    GLOBAL_TRACER
        .read()
        .expect("GLOBAL_TRACER RwLock poisoned")
        .clone()
}

pub fn set_tracer<S, T>(new_tracer: T)
where
    T: Tracer + 'static,
{
    let mut global_tracer = GLOBAL_TRACER
        .write()
        .expect("GLOBAL_TRACER RwLock poisoned");
    *global_tracer = Arc::new(new_tracer);
}
