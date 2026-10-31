//! Bifoil semantic analysis and instruction lowering.
//!
//! Each child owns one analysis or lowering responsibility. Module compilation
//! is the public entry point; per-function state lives in `context`.

use crate::model::Function;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

mod assignments;
mod bindings;
mod calls;
mod capture_initialization;
mod captures;
mod choices;
mod conditionals;
mod constants;
mod context;
mod enum_aliases;
mod expressions;
mod field_proofs;
mod flow;
mod functions;
mod inference;
mod legacy;
mod local_functions;
mod loops;
mod members;
mod modules;
mod operators;
mod overloads;
mod patterns;
mod prose;
mod reachability;
mod result_types;
mod return_inference;
mod shared;
mod single_use_initializers;
mod statements;
mod values;

use bindings::{Binding, ScopeStack, insert, local_function_id, lookup, symbol_mut};
use captures::{captured_names, expression_requirements};
use constants::constant_number;
use context::{FunctionContext, LoopState};
use flow::{
    check_loop_initialization, invalidate_narrowing, merge_env, narrow_case, narrow_condition, pure_expression,
};
use inference::{infer, infer_as, pending_type};
use members::{access_member, constant_case, enum_shorthand, expression_path, index_types, member_type, static_symbol};
use operators::{binary_type, numeric};
use overloads::specialize_signature;
use patterns::{case_bindings, when_exhaustive};
use return_inference::infer_returns;
use values::{infer_block_value, lambda_definition, yieldize};

pub(crate) use bindings::same_parameter_signature;
pub(crate) use functions::signature;
pub(crate) use modules::{compile, compile_group_member, declaration_interface, interface, seed};

struct Compiler {
    empty_dependencies: captures::EmptyDependencies,
    empty_publications: captures::EmptyDependencies,
    initialized_captures: BTreeMap<String, Vec<capture_initialization::Target>>,
    single_use_initializers: BTreeMap<String, single_use_initializers::Plan>,
    immutable_iterables: loops::nonempty::ImmutableValues,
    functions: BTreeMap<String, Function>,
    file: Arc<str>,
    started: Instant,
}
