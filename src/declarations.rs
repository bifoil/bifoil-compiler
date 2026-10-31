//! Declaration preparation and the nominal type catalog.

mod capabilities;
mod catalog;
pub(crate) use capabilities::{capability, parameter_capability};
mod generated;
mod normalization;
mod preparation;
mod projections;
mod resolution;
mod type_names;
mod variance;
mod well_formed;
pub(crate) use variance::{argument_variance, validate_variance};
pub(crate) use well_formed::validate_type;

pub(crate) use catalog::{
    all_types, check_method_access, definition, field, field_can_change, install, method, methods, owned_types, parent,
    receiver, same_nominal_declaration, update_method,
};
use generated::{ConstructorDefaults, ConstructorSpec, generate_constructor};
use normalization::{normalize_block, normalize_expression, normalize_function};
pub(crate) use preparation::prepare;
use resolution::install_imported_alias;
use resolution::{register_type_aliases, resolve_generic_parameters, resolve_type};

/// Owns the temporary catalog used by Type's relationship queries.
///
/// The current type helpers access thread-local metadata. Keeping the lifetime
/// here prevents one module compilation, failure, or nested session from
/// leaving stale aliases or bounds behind for the next compilation.
pub(crate) struct AnalysisSession {
    catalog: Option<catalog::CatalogSnapshot>,
    aliases: Option<resolution::AliasSnapshot>,
    parameters: Option<crate::generics::ParameterSnapshot>,
}

impl AnalysisSession {
    pub(crate) fn enter() -> Self {
        Self {
            catalog: Some(catalog::take_catalog()),
            aliases: Some(resolution::take_aliases()),
            parameters: Some(crate::generics::take_parameters()),
        }
    }
}

impl Drop for AnalysisSession {
    fn drop(&mut self) {
        if let Some(catalog) = self.catalog.take() {
            catalog::restore_catalog(catalog);
        }
        if let Some(aliases) = self.aliases.take() {
            resolution::restore_aliases(aliases);
        }
        if let Some(parameters) = self.parameters.take() {
            crate::generics::restore_parameters(parameters);
        }
    }
}
