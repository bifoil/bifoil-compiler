//! Constant-size sharing for empty headers and common immutable type shapes.

use crate::model::{Param, Type};
use serde::{Deserialize, Deserializer};
use std::sync::{Arc, LazyLock};

static EMPTY_TYPES: LazyLock<Arc<[Type]>> = LazyLock::new(|| Arc::from([]));
static EMPTY_PARAMETERS: LazyLock<Arc<[Param]>> = LazyLock::new(|| Arc::from([]));
static INT_RESULT: LazyLock<Arc<Type>> = LazyLock::new(|| Arc::new(Type::Int));
static UNIT_RESULT: LazyLock<Arc<Type>> = LazyLock::new(|| Arc::new(Type::Unit));
static STRING_RESULT: LazyLock<Arc<Type>> = LazyLock::new(|| Arc::new(Type::String));

pub(crate) fn type_list(values: Vec<Type>) -> Arc<[Type]> {
    if values.is_empty() {
        Arc::clone(&EMPTY_TYPES)
    } else {
        values.into()
    }
}

pub(crate) fn parameter_list(values: Vec<Param>) -> Arc<[Param]> {
    if values.is_empty() {
        Arc::clone(&EMPTY_PARAMETERS)
    } else {
        values.into()
    }
}

/// Read the current shape each time: inferred or specialized headers may change.
/// Complex results retain their own outer allocation and ordinary inner sharing.
pub(crate) fn result(value: &Type) -> Arc<Type> {
    owned_result(value.clone())
}

fn owned_result(value: Type) -> Arc<Type> {
    match value {
        Type::Int => Arc::clone(&INT_RESULT),
        Type::Unit => Arc::clone(&UNIT_RESULT),
        Type::String => Arc::clone(&STRING_RESULT),
        other => Arc::new(other),
    }
}

pub(crate) fn deserialize_types<'de, D: Deserializer<'de>>(input: D) -> Result<Arc<[Type]>, D::Error> {
    Vec::deserialize(input).map(type_list)
}

pub(crate) fn deserialize_parameters<'de, D: Deserializer<'de>>(input: D) -> Result<Arc<[Param]>, D::Error> {
    Vec::deserialize(input).map(parameter_list)
}

pub(crate) fn deserialize_result<'de, D: Deserializer<'de>>(input: D) -> Result<Arc<Type>, D::Error> {
    Type::deserialize(input).map(owned_result)
}
