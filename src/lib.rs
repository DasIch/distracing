#![allow(clippy::new_without_default)]

extern crate lazy_static;

#[cfg(feature = "lightstep")]
extern crate bytes;
#[cfg(feature = "lightstep")]
extern crate prost;
#[cfg(feature = "lightstep")]
extern crate prost_types;
#[cfg(feature = "lightstep")]
extern crate rand;

pub use api::Tracer;
pub use tracers::global::{set_tracer, tracer};
#[cfg(feature = "lightstep")]
pub use tracers::lightstep::LightStepTracer;
pub use tracers::mock::MockTracer;
pub use tracers::noop::NoopTracer;

mod api;
mod tracers;
