#![allow(clippy::new_without_default)]
//! This crate implements the [OpenTracing](https://opentracing.io/) API and
//! provides accompanying tracers.
//!
//! Currently implemented are the following tracers:
//!
//! - NoopTracer (default) – Does nothing and should have minimal overhead.
//! - MockTracer – Stores finished spans and provides an API to retrieve them.
//!   useful for testing your instrumentation.
//! - LightStepTracer (requires `lightstep` feature) – A tracer that sends
//!   spans to [LightStep](https://lightstep.com/).
//!
//! ## Example
//!
//! ```rust
//! fn frobnicate(foo: i32) {
//!     let span = distracing::tracer()
//!         .span("frobnicate")
//!         .set_tag("foo", foo)
//!         .start();
//!
//!     // do your thing
//!     // span is implicitly finished at the end of the scope
//! }
//! ```
extern crate lazy_static;

#[cfg(feature = "lightstep")]
extern crate bytes;
#[cfg(feature = "lightstep")]
extern crate log;
#[cfg(feature = "lightstep")]
extern crate prost;
#[cfg(feature = "lightstep")]
extern crate prost_types;
#[cfg(feature = "lightstep")]
extern crate rand;
#[cfg(any(feature = "lightstep", feature = "reqwest"))]
extern crate reqwest;

pub use span::Event;
pub use tracer::Tracer;
pub use tracers::global::{set_tracer, tracer};
#[cfg(feature = "lightstep")]
pub use tracers::lightstep::LightStepTracer;
pub use tracers::mock::MockTracer;
pub use tracers::noop::NoopTracer;

pub mod span;
pub mod tracer;
pub mod tracers;
