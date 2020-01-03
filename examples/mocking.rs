fn main() {
    let tracer = distracing::MockTracer::new();
    distracing::set_tracer(tracer.clone());
    {
        distracing::tracer().span("foo").start();
    }

    println!("Finished Spans: {}", tracer.finished_spans().len());
}
