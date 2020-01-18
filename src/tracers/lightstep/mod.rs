mod proto;
mod reporter;

use crate::api::{
    CarrierMap, Key, Span, SpanBuilder, SpanContext, SpanContextCorrupted, SpanContextState,
    SpanOptions, Tracer, Value,
};
use prost::Message;
use proto::carrier;
use reporter::LightStepReporter;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

const TEXT_MAP_TRACE_ID_FIELD: &str = "ot-tracer-traceid";
const TEXT_MAP_SPAN_ID_FIELD: &str = "ot-tracer-spanid";
const TEXT_MAP_SAMPLED_FIELD: &str = "ot-tracer-sampled";
const TEXT_MAP_PREFIX_BAGGAGE: &str = "ot-baggage-";

#[derive(Debug, Clone, PartialEq, Eq)]
struct LightStepSpanContextState {
    trace_id: u64,
    span_id: u64,
}

impl LightStepSpanContextState {
    fn new() -> Self {
        use rand::prelude::*;
        LightStepSpanContextState {
            trace_id: rand::thread_rng().gen(),
            span_id: rand::thread_rng().gen(),
        }
    }

    #[allow(clippy::borrowed_box)]
    fn from_trait_object(
        span_context_state: &Box<dyn SpanContextState>,
    ) -> &LightStepSpanContextState {
        span_context_state
            .as_any()
            .downcast_ref::<Self>()
            .expect("SpanContextState created with different Tracer")
    }
}

impl SpanContextState for LightStepSpanContextState {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A tracer that sends finished spans to a [LightStep](https://lightstep.com/) collector.
#[derive(Clone, Debug)]
pub struct LightStepTracer {
    reporter: Arc<LightStepReporter>,
}

impl LightStepTracer {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self::build().build()
    }

    /// Create an instance of the LightStep tracer.
    ///
    /// Example:
    ///
    /// ```rust
    /// # use distracing::LightStepTracer;
    /// distracing::set_tracer(LightStepTracer::build()
    ///     .access_token("<your-lightstep-access-token")
    ///     .component_name("my-application")
    ///     .tag("team", "my-team")
    ///     .build()
    /// )
    /// ```
    ///
    /// Further options are documented as part of the `LightStepConfig` struct.
    pub fn build() -> LightStepConfig {
        LightStepConfig::new()
    }

    fn new_with_config(config: LightStepConfig) -> Self {
        Self {
            reporter: Arc::new(LightStepReporter::new(config)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LightStepConfig {
    access_token: Option<String>,
    component_name: Option<String>,
    tags: HashMap<Key, Value>,
    collector_host: String,
    collector_port: usize,
    buffer_size: usize,
    send_period: Duration,
    send_timeout: Duration,
}

impl LightStepConfig {
    fn new() -> Self {
        LightStepConfig {
            access_token: None,
            component_name: None,
            tags: HashMap::new(),
            collector_host: "collector.lightstep.com".to_string(),
            collector_port: 443,

            // Copied from LightStep's own tracer implementations
            // Ref: https://github.com/lightstep/lightstep-tracer-python/blob/e146b1cad82c0b4c783a3a77872d816156c06dde/lightstep/constants.py#L6
            buffer_size: 1000,

            // Copied from LightStep's own tracer implementations
            // Ref: https://github.com/lightstep/lightstep-tracer-python/blob/e146b1cad82c0b4c783a3a77872d816156c06dde/lightstep/constants.py#L5
            send_period: Duration::from_millis(2_500), // 2.5s copied from LightStep's own tracer implementations

            // Copied from LightStep's own tracer implementations
            // Ref: https://github.com/lightstep/lightstep-tracer-python/blob/b8a47e25f085d58b46fa9bd6ee093e77aed5d62c/lightstep/recorder.py#L50
            send_timeout: Duration::from_secs(30),
        }
    }

    /// Sets the LightStep access token for your collector.
    pub fn access_token<S: Into<String>>(&mut self, access_token: S) -> &mut Self {
        self.access_token = Some(access_token.into());
        self
    }

    /// Sets the component name.
    pub fn component_name<S: Into<String>>(&mut self, component_name: S) -> &mut Self {
        self.component_name = Some(component_name.into());
        self
    }

    /// Add a global tag that applies to all spans.
    ///
    /// Tags set this way will show up in the "Additional Details" section
    /// of the trace view.
    pub fn tag<K: Into<Key>, V: Into<Value>>(&mut self, key: K, value: V) -> &mut Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Specify the hostname of the collector you are using.
    ///
    /// Default: `collector.lightstep.com`.
    pub fn collector_host<S: Into<String>>(&mut self, collector_host: S) -> &mut Self {
        self.collector_host = collector_host.into();
        self
    }

    /// Specify the port of the collector you are using.
    ///
    /// This implementation uses Protocol Buffers over HTTP, so you should use
    /// the port corresponding to that.
    ///
    /// Default: 443
    pub fn collector_port(&mut self, collector_port: usize) -> &mut Self {
        self.collector_port = collector_port;
        self
    }

    fn collector_url(&self) -> String {
        format!(
            "https://{}:{}/api/v2/reports",
            self.collector_host, self.collector_port
        )
    }

    /// Finished spans are buffered in memory and send in batches to the
    /// collector. This sets the size of that buffer in the number of spans.
    ///
    /// Default: 1000
    pub fn buffer_size(&mut self, buffer_size: usize) -> &mut Self {
        self.buffer_size = buffer_size;
        self
    }

    /// Finished spans are buffered in memory and send in batches to the
    /// collector periodically. This sets the duration for which the tracer
    /// sleeps inbetween sending spans.
    ///
    /// Default: 2.5s
    pub fn send_period(&mut self, send_period: Duration) -> &mut Self {
        self.send_period = send_period;
        self
    }

    pub fn build(&mut self) -> LightStepTracer {
        LightStepTracer::new_with_config(self.clone())
    }
}

impl Tracer for LightStepTracer {
    fn span<'a>(&'a self, operation_name: &str) -> SpanBuilder<'a> {
        SpanBuilder::new(Box::new(self), operation_name)
    }

    fn span_with_options(&self, options: SpanOptions) -> Span {
        let mut state = LightStepSpanContextState::new();
        let span_context = if !options.references.is_empty() {
            let parent_span_context = &options.references[0].to;
            let parent_state =
                LightStepSpanContextState::from_trait_object(&parent_span_context.state);
            state.trace_id = parent_state.trace_id;
            let mut span_context = SpanContext::new(Box::new(state));
            span_context.baggage_items = parent_span_context.baggage_items.clone();
            span_context
        } else {
            SpanContext::new(Box::new(state))
        };
        Span::new(span_context, self.reporter.clone(), options)
    }

    fn inject_into_text_map(&self, span_context: &SpanContext, carrier: &mut dyn CarrierMap) {
        let state = LightStepSpanContextState::from_trait_object(&span_context.state);

        carrier.set(TEXT_MAP_TRACE_ID_FIELD, &format!("{:x}", state.trace_id));
        carrier.set(TEXT_MAP_SPAN_ID_FIELD, &format!("{:x}", state.span_id));
        carrier.set(TEXT_MAP_SAMPLED_FIELD, "false");
    }

    fn extract_from_text_map(
        &self,
        carrier: &dyn CarrierMap,
    ) -> Result<SpanContext, SpanContextCorrupted> {
        let mut trace_id: Option<u64> = None;
        let mut span_id: Option<u64> = None;
        let mut baggage_items: HashMap<Cow<'static, str>, String> = HashMap::new();

        for key in carrier.keys() {
            if key == TEXT_MAP_TRACE_ID_FIELD {
                match u64::from_str_radix(carrier.get(&key).unwrap(), 16) {
                    Ok(tid) => trace_id = Some(tid),
                    Err(_) => {
                        return Err(SpanContextCorrupted {
                            message: format!(
                                "{} is not a hexadecimal u64",
                                TEXT_MAP_TRACE_ID_FIELD
                            ),
                        })
                    }
                };
            } else if key == TEXT_MAP_SPAN_ID_FIELD {
                match u64::from_str_radix(carrier.get(&key).unwrap(), 16) {
                    Ok(sid) => span_id = Some(sid),
                    Err(_) => {
                        return Err(SpanContextCorrupted {
                            message: format!("{} is not a hexadecimal u64", TEXT_MAP_SPAN_ID_FIELD),
                        })
                    }
                };
            } else if key.starts_with(TEXT_MAP_PREFIX_BAGGAGE) {
                let baggage_key = &key[TEXT_MAP_PREFIX_BAGGAGE.len()..];
                let baggage_value = carrier.get(baggage_key).unwrap();
                baggage_items.insert(
                    Cow::Owned(baggage_key.to_string()),
                    baggage_value.to_string(),
                );
            }
        }

        if trace_id.is_none() {
            return Err(SpanContextCorrupted {
                message: format!("{} is missing", TEXT_MAP_TRACE_ID_FIELD),
            });
        }
        if span_id.is_none() {
            return Err(SpanContextCorrupted {
                message: format!("{} is missing", TEXT_MAP_SPAN_ID_FIELD),
            });
        }

        Ok(SpanContext {
            state: Box::new(LightStepSpanContextState {
                trace_id: trace_id.unwrap(),
                span_id: span_id.unwrap(),
            }),
            baggage_items,
        })
    }

    fn inject_into_binary(&self, span_context: &SpanContext) -> Vec<u8> {
        let state = LightStepSpanContextState::from_trait_object(&span_context.state);
        let carrier = carrier::BinaryCarrier {
            deprecated_text_ctx: vec![],
            basic_ctx: Some(carrier::BasicTracerCarrier {
                trace_id: state.trace_id,
                span_id: state.span_id,
                sampled: false,
                baggage_items: span_context
                    .baggage_items
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k.into_owned(), v))
                    .collect(),
            }),
        };
        let mut buffer: Vec<u8> = Vec::with_capacity(carrier.encoded_len());
        // Ignore the result because the only possible failure is the buffer running out of
        // capacity and it's not like that is going to happen.
        let _ = carrier.encode(&mut buffer);
        base64::encode(&buffer).as_bytes().to_vec()
    }

    fn extract_from_binary(&self, carrier: &[u8]) -> Result<SpanContext, SpanContextCorrupted> {
        let carrier = carrier::BinaryCarrier::decode(base64::decode(carrier)?)?;
        let basic_ctx = match carrier.basic_ctx {
            Some(basic_ctx) => basic_ctx,
            None => {
                return Err(SpanContextCorrupted {
                    message: "missing basic_ctx".to_string(),
                })
            }
        };
        Ok(SpanContext {
            state: Box::new(LightStepSpanContextState {
                trace_id: basic_ctx.trace_id,
                span_id: basic_ctx.span_id,
            }),
            baggage_items: basic_ctx
                .baggage_items
                .into_iter()
                .map(|(k, v)| (Cow::Owned(k), v))
                .collect(),
        })
    }

    fn flush(&self) {
        while self.reporter.is_running() && self.reporter.has_pending_spans() {
            std::thread::sleep(self.reporter.config.send_period);
        }
    }
}

impl From<base64::DecodeError> for SpanContextCorrupted {
    fn from(err: base64::DecodeError) -> Self {
        Self {
            message: format!("{}", err),
        }
    }
}

impl From<prost::DecodeError> for SpanContextCorrupted {
    fn from(err: prost::DecodeError) -> Self {
        Self {
            message: format!("{}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LightStepSpanContextState, LightStepTracer};
    use crate::api::{SpanContext, Tracer};
    use std::collections::HashMap;

    #[test]
    fn test_child_span_inherits_from_parent_span() {
        let tracer = LightStepTracer::new();
        let mut parent_span = tracer.span("parent").start();
        parent_span.set_baggage_item("baggage", "item");
        let child_span = tracer
            .span("child")
            .child_of(parent_span.span_context())
            .start();
        assert_eq!(child_span.baggage_item("baggage"), Some("item"));

        let parent_state =
            LightStepSpanContextState::from_trait_object(&parent_span.span_context().state);
        let child_state =
            LightStepSpanContextState::from_trait_object(&child_span.span_context().state);
        assert_eq!(parent_state.trace_id, child_state.trace_id);
    }

    fn assert_span_context_eq(a: &SpanContext, b: &SpanContext) {
        assert_eq!(a.baggage_items, b.baggage_items);
        let a_state = LightStepSpanContextState::from_trait_object(&a.state);
        let b_state = LightStepSpanContextState::from_trait_object(&b.state);
        assert_eq!(a_state, b_state);
    }

    #[test]
    fn test_text_map_inject_and_extract() {
        let tracer = LightStepTracer::new();
        let span = tracer.span("foo").start();
        let span_context = span.span_context();
        let mut text_map: HashMap<String, String> = HashMap::new();
        tracer.inject_into_text_map(span_context, &mut text_map);
        let extracted_span_context = tracer.extract_from_text_map(&text_map).unwrap();
        assert_span_context_eq(span_context, &extracted_span_context);
    }

    #[test]
    fn test_binary_inject_and_extract() {
        let tracer = LightStepTracer::new();
        let span = tracer.span("foo").start();
        let span_context = span.span_context();
        let binary = tracer.inject_into_binary(span_context);
        let extracted_span_context = tracer.extract_from_binary(&binary).unwrap();
        assert_span_context_eq(span_context, &extracted_span_context);
    }
}
