use std::sync::Arc;
use tracing::field::{Field, Visit};

use crate::packet::{self, DebugAnnotation};

pub(crate) struct SpanAttributesExt {
    pub(crate) info: Arc<Vec<FieldValue>>,
}

#[derive(Debug)]
pub enum SpanValue {
    Bool(bool),
    U64(u64),
    I64(i64),
    Str(String),
    F64(f64),
}

#[derive(Debug)]
pub struct FieldValue {
    pub(crate) name: &'static str,
    pub(crate) value: SpanValue,
}

// TODO: Use custom type here with `&'static str` for name, and custom enum for
// values. Then interning can be handled in the writer.
#[derive(Debug)]
pub(crate) struct SpanAttributeVisitor {
    pub(crate) infos: Vec<FieldValue>,
}

impl Visit for SpanAttributeVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.infos.push(FieldValue {
            name: field.name(),
            value: SpanValue::Str(format!("{:?}", value)),
        })
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.infos.push(FieldValue {
            name: field.name(),
            value: SpanValue::Bool(value),
        })
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.infos.push(FieldValue {
            name: field.name(),
            value: SpanValue::U64(value),
        })
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.infos.push(FieldValue {
            name: field.name(),
            value: SpanValue::Str(value.to_owned()),
        })
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.infos.push(FieldValue {
            name: field.name(),
            value: SpanValue::I64(value),
        })
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.infos.push(FieldValue {
            name: field.name(),
            value: SpanValue::F64(value),
        })
    }
}
