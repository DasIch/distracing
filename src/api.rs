use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

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
    pub(crate) key: Key,
    pub(crate) value: Value,
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

pub(crate) trait SpanContextState: SpanContextClone + Send + Sync + std::fmt::Debug {
    fn as_any(&self) -> &dyn std::any::Any;
}

pub(crate) trait SpanContextClone {
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

/// Identifies a span.
#[derive(Clone, Debug)]
pub struct SpanContext {
    pub(crate) state: Box<dyn SpanContextState>,
    pub baggage_items: HashMap<Key, String>,
}

impl SpanContext {
    pub(crate) fn new(state: Box<dyn SpanContextState>) -> Self {
        SpanContext {
            state,
            baggage_items: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ReferenceType {
    ChildOf,
    FollowsFrom,
}

#[derive(Clone, Debug)]
pub(crate) struct Reference {
    pub rtype: ReferenceType,
    pub to: SpanContext,
}

#[derive(Clone, Debug)]
pub(crate) struct SpanData {
    pub(crate) span_context: SpanContext,
    pub(crate) start_timestamp: SystemTime,
    pub(crate) finish_timestamp: Option<SystemTime>,
    pub(crate) start_instant: Instant,
    pub(crate) duration: Option<Duration>,
    pub(crate) operation_name: String,
    pub(crate) references: Vec<Reference>,
    pub(crate) tags: HashMap<Key, Value>,
    pub(crate) log: Vec<(SystemTime, Vec<Event>)>,
}

/// Represents a span that has not been finished.
#[derive(Debug)]
pub struct Span {
    data: SpanData,
    reporter: Arc<dyn Reporter>,
}

/// Represents a finished span.
#[derive(Debug)]
pub struct FinishedSpan {
    pub(crate) data: SpanData,
}

impl FinishedSpan {
    pub fn span_context(&self) -> &SpanContext {
        &self.data.span_context
    }
}

impl Span {
    pub(crate) fn new(
        span_context: SpanContext,
        reporter: Arc<dyn Reporter>,
        options: SpanOptions,
    ) -> Self {
        Span {
            data: SpanData {
                span_context,
                start_timestamp: SystemTime::now(),
                finish_timestamp: None,
                start_instant: Instant::now(),
                duration: None,
                operation_name: options.operation_name,
                references: options.references,
                tags: options.tags,
                log: vec![],
            },
            reporter,
        }
    }

    pub fn span_context(&self) -> &SpanContext {
        &self.data.span_context
    }

    /// Change the operation name to something different.
    pub fn set_operation_name(&mut self, new_operation_name: &str) {
        self.data.operation_name = new_operation_name.to_owned();
    }

    /// Add/Update a tag.
    pub fn set_tag<K: Into<Key>, V: Into<Value>>(&mut self, key: K, value: V) {
        self.data.tags.insert(key.into(), value.into());
    }

    /// Log one or more events that happened right now.
    ///
    /// Example:
    ///
    /// ```rust
    /// # use distracing::Event;
    /// # let mut span = distracing::tracer().span("foo").start();
    /// span.log(&[Event::new("key", "value"), Event::new("bla", 123)]);
    /// ```
    pub fn log(&mut self, events: &[Event]) {
        self.log_with_timestamp(events, SystemTime::now());
    }

    /// Like `.log()` but allows you to specify a timestamp explicitly.
    pub fn log_with_timestamp(&mut self, events: &[Event], timestamp: SystemTime) {
        self.data.log.push((timestamp, events.to_vec()));
    }

    /// Retrieves a baggage item from the associated span context.
    pub fn baggage_item<K: Into<Key>>(&self, key: K) -> Option<&str> {
        self.data
            .span_context
            .baggage_items
            .get(&key.into())
            .map(|v| v.as_str())
    }

    /// Add/Update an item to the baggage context.
    pub fn set_baggage_item<K: Into<Key>>(&mut self, key: K, value: &str) {
        self.data
            .span_context
            .baggage_items
            .insert(key.into(), value.to_owned());
    }

    /// Finish the span explicitly.
    ///
    /// Normally you shouldn't have to call this as spans are implicitly
    /// finished when they are dropped. This may come in useful though, should
    /// you wish to finish a span before the end of the scope.
    pub fn finish(mut self) -> FinishedSpan {
        self.data.duration = Some(self.data.start_instant.elapsed());
        self.data.finish_timestamp = Some(SystemTime::now());
        FinishedSpan {
            data: self.data.clone(),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if self.data.duration.is_none() {
            self.data.duration = Some(self.data.start_instant.elapsed());
        }
        if self.data.finish_timestamp.is_none() {
            self.data.finish_timestamp = Some(SystemTime::now());
        }
        self.reporter.report(FinishedSpan {
            data: self.data.clone(),
        })
    }
}

#[doc(hidden)]
pub struct SpanOptions {
    pub(crate) operation_name: String,
    pub(crate) references: Vec<Reference>,
    pub(crate) tags: HashMap<Key, Value>,
}

impl SpanOptions {
    pub fn new(operation_name: &str) -> Self {
        Self {
            operation_name: operation_name.to_string(),
            references: vec![],
            tags: HashMap::new(),
        }
    }
}

pub struct SpanBuilder<'a> {
    tracer: Box<&'a dyn Tracer>,
    options: SpanOptions,
}

impl<'a> SpanBuilder<'a> {
    pub(crate) fn new(tracer: Box<&'a dyn Tracer>, operation_name: &str) -> Self {
        SpanBuilder {
            tracer,
            options: SpanOptions::new(operation_name),
        }
    }

    /// Add a "child of" reference to the span.
    pub fn child_of(mut self, span_context: &SpanContext) -> Self {
        self.options.references.push(Reference {
            rtype: ReferenceType::ChildOf,
            to: span_context.clone(),
        });
        self
    }

    /// Add a "follows from" reference to the span.
    pub fn follows_from(mut self, span_context: &SpanContext) -> Self {
        self.options.references.push(Reference {
            rtype: ReferenceType::FollowsFrom,
            to: span_context.clone(),
        });
        self
    }

    /// Add a tag.
    pub fn set_tag<K: Into<Key>, V: Into<Value>>(mut self, key: K, value: V) -> Self {
        self.options.tags.insert(key.into(), value.into());
        self
    }

    /// Create the span.
    pub fn start(self) -> Span {
        self.tracer.span_with_options(self.options)
    }
}

pub(crate) trait Reporter: std::fmt::Debug {
    fn report(&self, finished_span: FinishedSpan);
}

#[derive(Debug)]
pub struct SpanContextCorrupted {
    pub message: String,
}

impl std::fmt::Display for SpanContextCorrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SpanContextCorrupted {}

pub trait Tracer: Send + Sync {
    fn span<'a>(&'a self, operation_name: &str) -> SpanBuilder<'a>;

    #[doc(hidden)]
    fn span_with_options(&self, options: SpanOptions) -> Span;

    /// Inject the given span context into the carrier in text map format.
    ///
    /// May panic, if the span context was created by a different tracer.
    fn inject_into_text_map(&self, span_context: &SpanContext, carrier: &mut dyn CarrierMap);

    /// Extract the given span context from the carrier in text map format.
    ///
    /// An error will occur, if the carrier contains no span context
    /// information or the span context was serialized in a way the
    /// tracer doesn't understand.
    fn extract_from_text_map(
        &self,
        carrier: &dyn CarrierMap,
    ) -> Result<SpanContext, SpanContextCorrupted>;

    /// Inject the given span context from the carrier in HTTP header format.
    ///
    /// May panic, if the span context was created by a different tracer.
    fn inject_into_http_headers(&self, span_context: &SpanContext, carrier: &mut dyn CarrierMap) {
        self.inject_into_text_map(span_context, carrier)
    }

    /// Extract the given span context into the carrier in HTTP header format.
    ///
    /// An error will occur, if the carrier contains no span context
    /// information or the span context was serialized in a way the
    /// tracer doesn't understand.
    fn extract_from_http_headers(
        &self,
        carrier: &dyn CarrierMap,
    ) -> Result<SpanContext, SpanContextCorrupted> {
        self.extract_from_text_map(carrier)
    }

    /// Serialize the span context into a binary format.
    ///
    /// May panic, if the span context was created by a different tracer.
    fn inject_into_binary(&self, span_context: &SpanContext) -> Vec<u8>;

    /// Extract the given span context from the carrier in binary format.
    ///
    /// An error will occur, if the carrier contains no span context
    /// information or the span context was serialized in a way the
    /// tracer doesn't understand.
    fn extract_from_binary(&self, carrier: &[u8]) -> Result<SpanContext, SpanContextCorrupted>;

    /// Blocks until any spans that have been finished are processed by the
    /// tracer.
    ///
    /// This can be useful for implementations that perform I/O involving spans.
    fn flush(&self) {}
}

/// Trait for text map and http header carriers.
pub trait CarrierMap {
    fn keys<'a>(&'a self) -> Box<dyn Iterator<Item = String> + 'a>;

    fn get(&self, key: &str) -> Option<&str>;

    fn set(&mut self, key: &str, value: &str);
}

impl<S: ::std::hash::BuildHasher> CarrierMap for HashMap<String, String, S> {
    fn keys<'a>(&'a self) -> Box<dyn Iterator<Item = String> + 'a> {
        Box::new(self.keys().cloned())
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.get(key).map(|v| v.as_str())
    }

    fn set(&mut self, key: &str, value: &str) {
        self.insert(key.to_string(), value.to_string());
    }
}

#[cfg(feature = "reqwest")]
impl CarrierMap for reqwest::header::HeaderMap {
    fn keys<'a>(&'a self) -> Box<dyn Iterator<Item = String> + 'a> {
        Box::new(self.keys().map(|name| name.to_string()))
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.to_str().ok())
    }

    fn set(&mut self, key: &str, value: &str) {
        let name: reqwest::header::HeaderName = key.parse().unwrap();
        self.insert(name, value.parse().unwrap());
    }
}
