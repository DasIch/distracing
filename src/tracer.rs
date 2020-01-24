use crate::span::{Key, Reference, ReferenceType, Span, SpanContext, SpanOptions, Value};
use std::collections::HashMap;

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
