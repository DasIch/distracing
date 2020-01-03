fn main() {
    let foo = distracing::tracer()
        .span("foo")
        .set_tag("spam", "bar")
        .start();
    {
        let _bar = distracing::tracer()
            .span("bar")
            .child_of(foo.span_context())
            .start();
    }
}
