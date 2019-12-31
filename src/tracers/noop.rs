use crate::api::{
    Event, FinishedSpan as FinishedSpanTrait, Key, Reference, Span as SpanTrait,
    SpanBuilder as SpanBuilderTrait, SpanContext as SpanContextTrait, Tracer as TracerTrait,
};
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct SpanContext {
    baggage_items: HashMap<Key, String>,
}

impl SpanContextTrait for SpanContext {
    fn baggage_items(&self) -> &HashMap<Key, String> {
        &self.baggage_items
    }
}

#[derive(Clone, Debug)]
pub struct FinishedSpan {
    context: SpanContext,
}

impl FinishedSpanTrait for FinishedSpan {
    type SpanContext = SpanContext;

    fn span_context(&self) -> &Self::SpanContext {
        &self.context
    }
}

#[derive(Clone, Debug)]
pub struct Span {
    context: SpanContext,
}

impl SpanTrait for Span {
    type SpanContext = SpanContext;
    type FinishedSpan = FinishedSpan;

    fn span_context(&self) -> &Self::SpanContext {
        &self.context
    }

    fn set_operation_name(&mut self, _new_operation_name: &str) {}

    fn set_tag<K>(&mut self, _key: K, _value: String)
    where
        K: Into<Key>,
    {
    }

    fn set_baggage_item<K>(&mut self, _key: K, _value: String)
    where
        K: Into<Key>,
    {
    }

    fn baggage_item<K>(&self, _key: K) -> Option<&str>
    where
        K: Into<Key>,
    {
        None
    }

    fn log_with_timestamp(&mut self, _events: &[Event], _timestamp: SystemTime) {}

    fn finish_with_timestamp(self, _finish_timestamp: SystemTime) -> Self::FinishedSpan {
        FinishedSpan {
            context: self.context,
        }
    }
}

#[derive(Debug)]
pub struct SpanBuilder {}

impl SpanBuilderTrait for SpanBuilder {
    type SpanContext = SpanContext;
    type Span = Span;

    fn new(_operation_name: &str) -> Self {
        SpanBuilder {}
    }

    fn references(&mut self, _references: &[Reference<Self::SpanContext>]) -> &mut Self {
        self
    }

    fn child_of(&mut self, _context: &Self::SpanContext) -> &mut Self {
        self
    }

    fn follows_from(&mut self, _context: &Self::SpanContext) -> &mut Self {
        self
    }

    fn set_start_timestamp(&mut self, _start_timestamp: SystemTime) -> &mut Self {
        self
    }

    fn set_tag<K>(&mut self, _key: K, _value: String) -> &mut Self
    where
        K: Into<Key>,
    {
        self
    }

    fn start(self) -> Self::Span {
        let context = SpanContext {
            baggage_items: HashMap::new(),
        };
        Span { context }
    }
}

#[derive(Clone, Debug)]
pub struct Tracer {}

impl TracerTrait for Tracer {
    type SpanBuilder = SpanBuilder;

    fn span(&self, _operation_name: &str) -> Self::SpanBuilder {
        SpanBuilder {}
    }
}
