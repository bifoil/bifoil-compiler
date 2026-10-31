mod substitution;
pub use substitution::{signature, substitute};

mod constraints;
pub(crate) use constraints::widen_arguments;

mod inference;
pub(crate) use inference::{infer, refine_nonnull_arguments};

mod bounds;
pub(crate) use bounds::{member_view, operation_view};

use crate::model::{GenericParam, Type};
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    static PARAMETERS: RefCell<BTreeMap<String, GenericParam>> = const { RefCell::new(BTreeMap::new()) };
}
pub fn install(p: &GenericParam) {
    PARAMETERS.with(|ps| {
        ps.borrow_mut().insert(p.name.clone(), p.clone());
    });
}
/// Defer semantic bound checks until all local nominal interfaces exist.
/// Type validation treats registered parameter references as recursion leaves.
pub(crate) fn validate_installed_bounds() -> Result<(), String> {
    let parameters = PARAMETERS.with(|parameters| parameters.borrow().values().cloned().collect::<Vec<_>>());
    for parameter in parameters {
        for bound in &parameter.bounds {
            crate::declarations::validate_type(bound)?;
        }
    }
    Ok(())
}

pub fn parameter(name: &str) -> Option<GenericParam> {
    PARAMETERS.with(|ps| ps.borrow().get(name).cloned())
}
pub fn check(parameters: &[GenericParam], args: &BTreeMap<String, Type>) -> Result<(), String> {
    for p in parameters {
        let actual = args.get(&p.name).ok_or_else(|| {
            format!(
                "cannot infer type argument {}; supply it explicitly",
                p.name.rsplit('#').next().unwrap()
            )
        })?;
        if actual == &Type::Star {
            return Err("star is a projection, not a concrete call type argument".into());
        }
        for bound in &p.bounds {
            let bound = substitute(bound, args);
            let supported = match &bound {
                Type::Named { name, .. } if name == "Equatable" => actual.equatable(),
                Type::Named { name, .. } if name == "StableKey" => actual.stable_key(),
                _ => actual.subtype_of(&bound),
            };
            if !supported {
                return Err(format!("type argument {actual} does not satisfy {bound}"));
            }
        }
    }
    Ok(())
}
pub fn capability(name: &str, stable: bool) -> bool {
    crate::declarations::parameter_capability(name, stable)
}
pub fn nonnull(name: &str) -> bool {
    parameter(name).is_some_and(|p| {
        p.bounds
            .iter()
            .any(|b| !matches!(b,Type::Named{name,..}if name=="Equatable"||name=="StableKey") && !b.permits_null())
    })
}

pub(crate) struct ParameterSnapshot(BTreeMap<String, GenericParam>);

pub(crate) fn take_parameters() -> ParameterSnapshot {
    PARAMETERS.with(|parameters| ParameterSnapshot(std::mem::take(&mut *parameters.borrow_mut())))
}

pub(crate) fn restore_parameters(snapshot: ParameterSnapshot) {
    PARAMETERS.with(|parameters| *parameters.borrow_mut() = snapshot.0);
}

/// Direct upper-bound cycles have no useful bound and would recurse in queries.
/// Recursive occurrences behind a nominal type are permitted.
pub fn validate_bound_cycles(parameters: &[GenericParam]) -> Result<(), String> {
    fn visit(
        name: &str,
        path: &mut Vec<String>,
        complete: &mut std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        if complete.contains(name) {
            return Ok(());
        }
        if path.iter().any(|other| other == name) {
            return Err("cyclic generic upper bounds".into());
        }
        path.push(name.into());
        if let Some(parameter) = parameter(name) {
            for bound in parameter.bounds {
                if let Type::Parameter { name, .. } = bound.present() {
                    visit(name, path, complete)?;
                }
            }
        }
        path.pop();
        complete.insert(name.into());
        Ok(())
    }
    let mut complete = std::collections::BTreeSet::new();
    for parameter in parameters {
        visit(&parameter.name, &mut vec![], &mut complete)?;
    }
    Ok(())
}
