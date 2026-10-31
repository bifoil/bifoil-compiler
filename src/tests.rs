//! Compiler semantic regressions using readable original source programs.

use crate::model::Module;
use crate::{compiler, parser};
use std::collections::BTreeMap;
use std::time::Instant;

fn compile(text: &str) -> Result<Module, String> {
    compiler::compile(
        parser::parse(text, "src/test.bfl")?,
        "test",
        "src/test.bfl",
        &BTreeMap::new(),
        Instant::now(),
    )
}

mod assertions;
mod ast_parameters;
mod audit_regressions;
mod binding_aliases;
mod call_publication;
mod callable_graphs;
mod capture_initialization;
mod capture_scopes;
mod captured_writes;
mod captures;
mod casts;
mod collection_captures;
mod collection_queries;
mod compound_reads;
mod constant_keys;
mod dynamic_keys;
mod entry_reads;
mod enum_aliases;
mod escaped_cells;
mod exclusive_initialization;
mod expressions;
mod field_proofs;
mod flow;
mod forward_initialization;
mod generic_bounds;
mod generic_inference;
mod generic_references;
mod generic_widening;
mod generic_width;
mod immediate_references;
mod indexed_proofs;
mod inference;
mod legacy_enums;
mod loop_exit_initialization;
mod loop_initialization;
mod map_proofs;
mod members;
mod modules;
mod mutable_field_proofs;
mod mutable_fields;
mod mutable_indexes;
mod nonempty_values;
mod normalization_scopes;
mod overload_captures;
mod private_members;
mod projected_inputs;
mod range_initialization;
mod repeated_initialization;
mod safe_paths;
mod shared_callbacks;
mod shared_metadata;
mod signature_parameters;
mod single_use_initializers;
mod source_locations;
mod stored_progressions;
mod syntax;
mod transitive_defaults;
mod type_graphs;
mod type_storage;
mod types;
mod unit_bindings;
mod write_captures;
