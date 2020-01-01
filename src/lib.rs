extern crate lazy_static;

#[cfg(feature = "lightstep")]
extern crate bytes;
#[cfg(feature = "lightstep")]
extern crate prost;
#[cfg(feature = "lightstep")]
extern crate prost_types;

pub use api::Tracer;
pub use tracers::global::{set_tracer, tracer};
pub use tracers::noop::NoopTracer;

mod api;
mod tracers;
