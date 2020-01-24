use crate::tracer::Tracer;
use crate::tracers::noop::NoopTracer;
use std::sync::{Arc, RwLock};

lazy_static::lazy_static! {
    static ref GLOBAL_TRACER: RwLock<Arc<dyn Tracer>> = RwLock::new(Arc::new(NoopTracer::new()));
}

/// Returns the tracer set by `set_tracer`. If `set_tracer` has not been called,
/// an instance of the `NoopTracer` will be returned`.
pub fn tracer() -> Arc<dyn Tracer> {
    GLOBAL_TRACER
        .read()
        .expect("GLOBAL_TRACER RwLock poisoned")
        .clone()
}

/// Sets the global tracer.
pub fn set_tracer<T>(new_tracer: T)
where
    T: Tracer + 'static,
{
    let mut global_tracer = GLOBAL_TRACER
        .write()
        .expect("GLOBAL_TRACER RwLock poisoned");
    *global_tracer = Arc::new(new_tracer);
}
