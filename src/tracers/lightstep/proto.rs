use crate::span;
use std::convert::TryInto;
use std::time::SystemTime;

/// Generated code based on lightstep-tracer-common/collector.proto
pub mod collector {
    include!(concat!(env!("OUT_DIR"), "/lightstep.collector.rs"));
}

/// Generated code based on lightstep-tracer-common/lightstep.proto
pub mod carrier {
    include!(concat!(env!("OUT_DIR"), "/lightstep.rs"));
}

impl From<span::SpanContext> for collector::SpanContext {
    fn from(span_context: span::SpanContext) -> collector::SpanContext {
        // The unwrap is safe under the assumption that this is only called on Spans created by the
        // LightStepTracer. As long as nobody makes this function `pub` this should be a reasonable
        // assumption.
        let state = span_context
            .state
            .as_any()
            .downcast_ref::<super::LightStepSpanContextState>()
            .unwrap();
        collector::SpanContext {
            trace_id: state.trace_id,
            span_id: state.span_id,
            baggage: span_context.baggage_items.clone(),
        }
    }
}

impl From<span::Reference> for collector::Reference {
    fn from(reference: span::Reference) -> collector::Reference {
        let relationship = match reference.rtype {
            span::ReferenceType::ChildOf => collector::reference::Relationship::ChildOf,
            span::ReferenceType::FollowsFrom => collector::reference::Relationship::FollowsFrom,
        };
        collector::Reference {
            // for some reason this is stored as an i32 ¯\_(ツ)_/¯
            relationship: relationship.into(),
            span_context: Some(reference.to.into()),
        }
    }
}

impl From<span::Value> for collector::key_value::Value {
    fn from(value: span::Value) -> collector::key_value::Value {
        use collector::key_value;
        match value {
            span::Value::String(s) => key_value::Value::StringValue(s),
            span::Value::Bool(b) => key_value::Value::BoolValue(b),
            span::Value::F32(n) => key_value::Value::DoubleValue(n as f64),
            span::Value::F64(n) => key_value::Value::DoubleValue(n),
            span::Value::U8(n) => serialize_numeric_to_value(n),
            span::Value::U16(n) => serialize_numeric_to_value(n),
            span::Value::U32(n) => serialize_numeric_to_value(n),
            span::Value::U64(n) => serialize_numeric_to_value(n),
            span::Value::U128(n) => serialize_numeric_to_value(n),
            span::Value::I8(n) => serialize_numeric_to_value(n),
            span::Value::I16(n) => serialize_numeric_to_value(n),
            span::Value::I32(n) => serialize_numeric_to_value(n),
            span::Value::I64(n) => serialize_numeric_to_value(n),
            span::Value::I128(n) => serialize_numeric_to_value(n),
            span::Value::USize(n) => serialize_numeric_to_value(n),
            span::Value::ISize(n) => serialize_numeric_to_value(n),
        }
    }
}

fn serialize_numeric_to_value<N: Copy + TryInto<i64> + std::string::ToString>(
    n: N,
) -> collector::key_value::Value {
    match n.try_into() {
        Ok(n) => collector::key_value::Value::IntValue(n),
        Err(_) => collector::key_value::Value::StringValue(n.to_string()),
    }
}

impl From<(span::Key, span::Value)> for collector::KeyValue {
    fn from((key, value): (span::Key, span::Value)) -> collector::KeyValue {
        let key = key;
        collector::KeyValue {
            key,
            value: Some(value.into()),
        }
    }
}

impl From<(SystemTime, Vec<span::Event>)> for collector::Log {
    fn from((timestamp, events): (SystemTime, Vec<span::Event>)) -> collector::Log {
        collector::Log {
            timestamp: Some(timestamp.into()),
            fields: events
                .into_iter()
                .map(|e| collector::KeyValue {
                    key: e.key,
                    value: Some(e.value.into()),
                })
                .collect(),
        }
    }
}

impl From<span::FinishedSpan> for collector::Span {
    fn from(span: span::FinishedSpan) -> collector::Span {
        // Either Span.finish or Drop.drop sets the duration. Accordingly we should be able to fairly
        // safely assume that it's set here.
        let duration = span
            .data
            .duration
            .expect("BUG: FinishedSpan duration not set");
        collector::Span {
            span_context: Some(span.data.span_context.into()),
            operation_name: span.data.operation_name,
            references: span.data.references.into_iter().map(|r| r.into()).collect(),
            start_timestamp: Some(span.data.start_timestamp.into()),
            duration_micros: duration.as_micros() as u64,
            tags: span.data.tags.into_iter().map(|t| t.into()).collect(),
            logs: span.data.log.into_iter().map(|l| l.into()).collect(),
        }
    }
}
