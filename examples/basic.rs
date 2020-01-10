#![allow(clippy::blacklisted_name)]
#[macro_use]
extern crate log;

use distracing::{Event, Tracer};

fn main() {
    env_logger::init();
    info!("Start");
    let lightstep_tracer = distracing::LightStepTracer::build()
        .component_name("distracing-dev")
        .collector_host("tracing.stups.zalan.do")
        .collector_port(8443)
        .access_token(
            std::env::var("LIGHTSTEP_ACCESS_TOKEN")
                .expect("LIGHTSTEP_ACCESS_TOKEN environment variable not found"),
        )
        .build();
    distracing::set_tracer(lightstep_tracer.clone());
    {
        let foo = distracing::tracer()
            .span("foo")
            .set_tag("spam", "bar")
            .set_tag("bla", 123)
            .start();
        {
            let mut bar = distracing::tracer()
                .span("bar")
                .child_of(foo.span_context())
                .start();
            bar.log(&[Event::new("is_frobnicating", true)]);
        }
    }
    lightstep_tracer.flush();
}
