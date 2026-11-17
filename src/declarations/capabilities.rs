//! On-demand capability states for recursive nominal types.
//!
//! A state records a declaration and the capability of each argument, rather
//! than enumerating all sufficient argument combinations. Queries solve only
//! reached states and cache completed results within the current type catalog.

mod query;

use super::catalog::Catalog;
use crate::model::Type;
use query::Query;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    name: String,
    arguments: Vec<bool>,
}

type Results = BTreeMap<State, bool>;
thread_local! {
    static CACHE: RefCell<[Results; 2]> = RefCell::new(std::array::from_fn(|_| Results::new()));
}

pub(super) fn invalidate() {
    CACHE.with(|cache| *cache.borrow_mut() = std::array::from_fn(|_| Results::new()));
}

fn check(ty: &Type, stable: bool, definitions: &Catalog) -> bool {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let confirmed = &mut cache[usize::from(stable)];
        let mut query = Query::new(definitions, confirmed, stable);
        let result = query.solve(ty);
        confirmed.extend(query.completed());
        result
    })
}

pub(crate) fn capability(ty: &Type, stable: bool) -> bool {
    super::catalog::with_types(|definitions| check(ty, stable, definitions))
}

pub(crate) fn parameter_capability(name: &str, stable: bool) -> bool {
    capability(&Type::parameter(name), stable)
}
