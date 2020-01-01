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

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_owned())
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

pub trait SpanContextState: SpanContextClone {}

pub trait SpanContextClone {
    fn clone_box(&self) -> Box<dyn SpanContextState>;
}

impl<T: 'static + SpanContextState + Clone> SpanContextClone for T {
    fn clone_box(&self) -> Box<dyn SpanContextState> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn SpanContextState> {
    fn clone(&self) -> Box<dyn SpanContextState> {
        self.clone_box()
    }
}

#[derive(Clone)]
pub struct SpanContext<S: SpanContextState> {
    pub(crate) state: S,
    pub baggage_items: HashMap<Key, String>,
}

#[derive(Clone, Debug)]
pub enum ReferenceType {
    ChildOf,
    FollowsFrom,
}

pub struct Reference<S: SpanContextState + Clone> {
    pub rtype: ReferenceType,
    pub to: SpanContext<S>,
}

pub struct Span<S: SpanContextState + Clone> {
    span_context: SpanContext<S>,
    pub(crate) start_timestamp: SystemTime,
    pub(crate) operation_name: String,
    pub(crate) references: Vec<Reference<S>>,
    pub(crate) tags: HashMap<Key, Value>,
    pub(crate) log: Vec<(SystemTime, Vec<Event>)>,
}

pub struct FinishedSpan<S: SpanContextState + Clone> {
    pub(crate) span: Span<S>,
    pub(crate) finish_timestamp: SystemTime,
}

impl<S: SpanContextState + Clone> FinishedSpan<S> {
    pub fn span_context(&self) -> &SpanContext<S> {
        &self.span.span_context
    }
}

impl<S: SpanContextState + Clone> Span<S> {
    pub fn span_context(&self) -> &SpanContext<S> {
        &self.span_context
    }

    pub fn set_operation_name(&mut self, new_operation_name: &str) {
        self.operation_name = new_operation_name.to_owned();
    }

    pub fn set_tag<K: Into<Key>, V: Into<Value>>(&mut self, key: K, value: V) {
        self.tags.insert(key.into(), value.into());
    }

    pub fn log(&mut self, events: &[Event]) {
        self.log_with_timestamp(events, SystemTime::now());
    }

    pub fn log_with_timestamp(&mut self, events: &[Event], timestamp: SystemTime) {
        self.log.push((timestamp, events.to_vec()));
    }

    pub fn baggage_item<K: Into<Key>>(&self, key: K) -> Option<&str> {
        self.span_context
            .baggage_items
            .get(&key.into())
            .map(|v| v.as_str())
    }

    pub fn set_baggage_item<K: Into<Key>>(&mut self, key: K, value: &str) {
        self.span_context
            .baggage_items
            .insert(key.into(), value.to_owned());
    }

    pub fn finish(self) -> FinishedSpan<S> {
        self.finish_with_timestamp(SystemTime::now())
    }

    pub fn finish_with_timestamp(self, timestamp: SystemTime) -> FinishedSpan<S> {
        FinishedSpan {
            span: self,
            finish_timestamp: timestamp,
        }
    }
}

pub struct SpanBuilder<S: SpanContextState + Clone> {
    span_context: SpanContext<S>,
    start_timestamp: Option<SystemTime>,
    operation_name: String,
    references: Vec<Reference<S>>,
    tags: HashMap<Key, Value>,
}

impl<S: SpanContextState + Clone> SpanBuilder<S> {
    pub(crate) fn new(span_context: SpanContext<S>, operation_name: &str) -> Self {
        SpanBuilder {
            span_context,
            start_timestamp: None,
            operation_name: operation_name.to_owned(),
            references: vec![],
            tags: HashMap::new(),
        }
    }

    pub fn child_of(mut self, span_context: &SpanContext<S>) -> Self {
        self.references.push(Reference {
            rtype: ReferenceType::ChildOf,
            to: span_context.clone(),
        });
        self
    }

    pub fn follows_from(mut self, span_context: &SpanContext<S>) -> Self {
        self.references.push(Reference {
            rtype: ReferenceType::FollowsFrom,
            to: span_context.clone(),
        });
        self
    }

    pub fn set_start_timestamp(mut self, start_timestamp: SystemTime) -> Self {
        self.start_timestamp = Some(start_timestamp);
        self
    }

    pub fn set_tag<K: Into<Key>, V: Into<Value>>(mut self, key: K, value: V) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    pub fn start(self) -> Span<S> {
        Span {
            span_context: self.span_context,
            start_timestamp: self.start_timestamp.unwrap_or_else(SystemTime::now),
            operation_name: self.operation_name,
            references: self.references,
            tags: self.tags,
            log: vec![],
        }
    }
}

pub trait Tracer: Send + Sync {
    type SpanContextState: SpanContextState + Clone;

    fn new_span_context_state(&self) -> Self::SpanContextState;

    fn span(&self, operation_name: &str) -> SpanBuilder<Self::SpanContextState> {
        let span_context = SpanContext {
            state: self.new_span_context_state(),
            baggage_items: HashMap::new(),
        };
        SpanBuilder::new(span_context, operation_name)
    }
}
