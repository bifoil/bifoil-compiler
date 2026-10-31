//! Conservative shared-initializer dependencies and deterministic startup order.
//!
//! Callable-origin analysis produces compact summaries while bodies are available.
//! Dependency resolution then needs signatures and summaries only, including after
//! completed modules have moved their instruction bodies to temporary disk storage.

mod dependencies;
mod flow;
mod scheduler;
mod signatures;

use crate::model::{Function, FunctionSummary, SharedDef};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

pub(crate) use scheduler::order;

/// Compute summaries against complete signature and shared-binding metadata.
/// Other instruction bodies are not read by the origin analysis.
pub(crate) fn summarize(
    own_functions: &BTreeMap<String, Function>,
    own_shared: &BTreeMap<String, SharedDef>,
    modules: &crate::model::ModuleView<'_>,
    started: Instant,
) -> Result<BTreeMap<String, Arc<FunctionSummary>>, String> {
    let context = SummaryContext::new(own_functions, own_shared, modules, started);
    let mut summaries = BTreeMap::new();
    for (id, function) in own_functions {
        if let Some(summary) = context.analyze(function)? {
            summaries.insert(id.clone(), summary);
        }
    }
    Ok(summaries)
}

/// Other functions contribute signatures only, so one stored body can be
/// analyzed and released before reading its successor.
pub(crate) struct SummaryContext<'a> {
    functions: signatures::View<'a>,
    shared: BTreeMap<&'a str, &'a SharedDef>,
    started: Instant,
}
impl<'a> SummaryContext<'a> {
    pub(crate) fn new(
        own_functions: &'a BTreeMap<String, Function>,
        own_shared: &'a BTreeMap<String, SharedDef>,
        modules: &'a crate::model::ModuleView<'_>,
        started: Instant,
    ) -> Self {
        let functions = signatures::View::new(own_functions, modules);
        let mut shared = BTreeMap::new();
        for module in modules.values() {
            shared.extend(module.shared.iter().map(|(id, definition)| (id.as_str(), definition)));
        }
        shared.extend(own_shared.iter().map(|(id, definition)| (id.as_str(), definition)));
        Self {
            functions,
            shared,
            started,
        }
    }

    /// Scene, startup, and suspending bodies cannot participate in ordinary
    /// shared-initializer calls. Their signatures suffice for this analysis.
    pub(crate) fn requires_body(function: &Function) -> bool {
        !function.init && !function.signature.scene && !function.signature.suspend
    }

    pub(crate) fn analyze(&self, function: &Function) -> Result<Option<Arc<FunctionSummary>>, String> {
        crate::budget(self.started)?;
        if !Self::requires_body(function) {
            return Ok(None);
        }
        let analysis = flow::analyze(function, &self.functions, &self.shared, self.started);
        crate::budget(self.started)?;
        let summary = analysis.unwrap_or_else(|error| FunctionSummary {
            error: Some(error),
            ..FunctionSummary::default()
        });
        Ok(Some(summary.shared()))
    }
}

/// The source-module pass uses completed dependency summaries and real own bodies.
/// The linker repeats resolution against the whole program's possible call targets.
pub(crate) fn discover(
    own_functions: &BTreeMap<String, Function>,
    own_shared: &mut BTreeMap<String, SharedDef>,
    modules: &crate::model::ModuleView<'_>,
    started: Instant,
) -> Result<BTreeMap<String, Arc<FunctionSummary>>, String> {
    let own_summaries = summarize(own_functions, own_shared, modules, started)?;
    // Callers still need these summaries even when this module has no shared
    // initializers. Only dependency resolution can be skipped in that case.
    if own_shared.is_empty() {
        return Ok(own_summaries);
    }
    let mut summaries = BTreeMap::new();
    let mut initializer_ids = BTreeSet::new();
    for module in modules.values() {
        summaries.extend(
            module
                .summaries
                .iter()
                .map(|(id, summary)| (id.as_str(), summary.as_ref())),
        );
        initializer_ids.extend(module.shared.values().map(|definition| definition.initializer.clone()));
    }
    summaries.extend(
        own_summaries
            .iter()
            .map(|(id, summary)| (id.as_str(), summary.as_ref())),
    );
    initializer_ids.extend(own_shared.values().map(|definition| definition.initializer.clone()));
    let functions = modules
        .values()
        .flat_map(|module| module.functions.values())
        .chain(own_functions.values());
    resolve(own_shared, &initializer_ids, functions, &summaries, started)?;
    Ok(own_summaries)
}

/// Revisit raw summaries with the final ordinary-function candidate set. Unknown
/// indirect calls conservatively include every candidate; no body is loaded here.
pub(crate) fn resolve<'a>(
    own_shared: &mut BTreeMap<String, SharedDef>,
    initializer_ids: &BTreeSet<String>,
    functions: impl IntoIterator<Item = &'a Function>,
    summaries: &BTreeMap<&str, &FunctionSummary>,
    started: Instant,
) -> Result<(), String> {
    if own_shared.is_empty() {
        return Ok(());
    }
    let needs_targets = summaries.values().any(|summary| summary.indirect_call);
    let possible_targets: BTreeSet<_> = functions
        .into_iter()
        .filter(|_| needs_targets)
        .filter(|function| {
            !function.init
                && !function.signature.scene
                && !function.signature.suspend
                && !initializer_ids.contains(&function.signature.id)
        })
        .map(|function| function.signature.id.as_str())
        .collect();
    let mut query = dependencies::Query::new(summaries, possible_targets, started)?;
    let mut lists = crate::shared_dependencies::Interner::default();
    for definition in own_shared.values_mut() {
        let dependencies = query
            .initializer(&definition.initializer)?
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        definition.dependencies = lists.intern(dependencies.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_summaries_share_storage_without_losing_real_initializer_dependencies() {
        let source = "shared val base: Int = 7\nfun first() -> Int = 1\nfun second() -> Int = 2\nfun read() -> Int = base\nshared val derived: Int = read()\n";
        let module = crate::compiler::compile(
            crate::parser::parse(source, "src/shared.bfl").unwrap(),
            "shared",
            "src/shared.bfl",
            &BTreeMap::new(),
            Instant::now(),
        )
        .unwrap();
        let first = &module.summaries["shared.first"];
        let second = &module.summaries["shared.second"];
        let read = &module.summaries["shared.read"];
        assert!(Arc::ptr_eq(first, second));
        assert!(!Arc::ptr_eq(first, read));
        assert!(read.bindings.contains("shared.base"));
        assert_eq!(module.shared["shared.derived"].dependencies.as_ref(), ["shared.base"]);
    }
}
