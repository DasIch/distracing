use std::borrow::Cow;
use std::collections::HashMap;
use std::time::SystemTime;

pub type Key = Cow<'static, str>;

#[derive(Clone, Debug)]
pub enum Value {
    String(String),
    Bool(bool),

    // Numeric types, some tracer implementations may not be able to cope with all of these and may
    // have to cast them to a string.
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    F32(f32),
    F64(f64),
    USize(usize),
    ISize(isize),
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<u8> for Value {
    fn from(value: u8) -> Self {
        Value::U8(value)
    }
}

impl From<u16> for Value {
    fn from(value: u16) -> Self {
        Value::U16(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Value::U32(value)
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Value::U64(value)
    }
}

impl From<u128> for Value {
    fn from(value: u128) -> Self {
        Value::U128(value)
    }
}

impl From<i8> for Value {
    fn from(value: i8) -> Self {
        Value::I8(value)
    }
}

impl From<i16> for Value {
    fn from(value: i16) -> Self {
        Value::I16(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::I32(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::I64(value)
    }
}

impl From<i128> for Value {
    fn from(value: i128) -> Self {
        Value::I128(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::F32(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::F64(value)
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Value::USize(value)
    }
}

impl From<isize> for Value {
    fn from(value: isize) -> Self {
        Value::ISize(value)
    }
}

#[derive(Clone, Debug)]
pub enum ReferenceType {
    ChildOf,
    FollowsFrom,
}

pub struct Reference<SC: SpanContext> {
    pub rtype: ReferenceType,
    pub to: SC,
}

#[derive(Clone, Debug)]
pub struct Event {
    key: Key,
    value: Value,
}

impl Event {
    pub fn new<K, V>(key: K, value: V) -> Event
    where
        K: Into<Key>,
        V: Into<Value>,
    {
        Event {
            key: key.into(),
            value: value.into(),
        }
    }
}

pub trait SpanContext: Clone {
    // The OpenTracing specification calls for being able to iterate through all baggage items.
    // I don't see a reason to restrict it just to that, it would only add unnecessary complexity
    fn baggage_items(&self) -> &HashMap<Key, String>;
}

pub trait Span: Sized {
    type SpanContext: SpanContext;
    type FinishedSpan: FinishedSpan;

    fn span_context(&self) -> &Self::SpanContext;

    fn set_operation_name(&mut self, new_operation_name: &str);

    fn finish(self) -> Self::FinishedSpan {
        self.finish_with_timestamp(SystemTime::now())
    }

    fn finish_with_timestamp(self, finish_timestamp: SystemTime) -> Self::FinishedSpan;

    fn set_tag<K>(&mut self, key: K, value: String)
    where
        K: Into<Key>;

    fn log_with_timestamp(&mut self, events: &[Event], timestamp: SystemTime);

    fn log(&mut self, events: &[Event]) {
        self.log_with_timestamp(events, SystemTime::now())
    }

    fn set_baggage_item<K>(&mut self, key: K, value: String)
    where
        K: Into<Key>;

    fn baggage_item<K>(&self, key: K) -> Option<&str>
    where
        K: Into<Key>;
}

pub trait FinishedSpan {
    type SpanContext: SpanContext;

    fn span_context(&self) -> &Self::SpanContext;
}

pub trait SpanBuilder {
    type SpanContext: SpanContext;
    type Span: Span;

    fn new(operation_name: &str) -> Self;

    fn references(&mut self, references: &[Reference<Self::SpanContext>]) -> &mut Self;

    fn child_of(&mut self, context: &Self::SpanContext) -> &mut Self;

    fn follows_from(&mut self, context: &Self::SpanContext) -> &mut Self;

    fn set_start_timestamp(&mut self, start_timestamp: SystemTime) -> &mut Self;

    fn set_tag<K>(&mut self, key: K, value: String) -> &mut Self
    where
        K: Into<Key>;

    fn start(self) -> Self::Span;
}

pub trait Tracer {
    type SpanBuilder: SpanBuilder;

    fn span(&self, operation_name: &str) -> Self::SpanBuilder;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_event() {
        Event::new("blasters", 123);
        Event::new("frobnicating".to_owned(), true);
        Event::new("foo", 123.456);
    }
}
