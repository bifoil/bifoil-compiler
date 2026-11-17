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

mod accepted_clarifications;
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
mod comparison_views;
mod compound_reads;
mod constant_keys;
mod constant_variants;
mod covariant_inference;
mod coverage_artifact_boundaries;
mod coverage_legacy_boundaries;
mod coverage_source_mutations;
mod coverage_syntax_failures;
mod coverage_type_relations;
mod dependent_bounds;
mod directional_inference;
mod dynamic_keys;
mod entry_reads;
mod enum_aliases;
mod equality_contracts;
mod escaped_cells;
mod exclusive_initialization;
mod expressions;
mod field_proofs;
mod flow;
mod forward_initialization;
mod generic_bounds;
mod generic_inference;
mod generic_references;
mod generic_result_context;
mod generic_widening;
mod generic_width;
mod immediate_references;
mod indexed_proofs;
mod inference;
mod interpolation_blocks;
mod lambda_inference;
mod legacy_enums;
mod loop_exit_initialization;
mod loop_initialization;
mod map_proofs;
mod members;
mod modules;
mod mutable_field_proofs;
mod mutable_fields;
mod mutable_indexes;
mod namespace_separation;
mod nonempty_values;
mod normalization_scopes;
mod overload_captures;
mod overloaded_callbacks;
mod private_members;
mod projected_field_writes;
mod projected_inputs;
mod prose_callables;
mod range_initialization;
mod range_membership;
mod repeated_initialization;
mod safe_paths;
mod scoped_shared;
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
