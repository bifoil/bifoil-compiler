use super::{Origin, State};
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn sparse_local_joins_match_pointwise_callable_origins() {
    let origins = [
        Origin::Unknown,
        Origin::empty(),
        Origin::function("main.one"),
        Origin::function("main.two"),
    ];
    let names = ["first", "second", "third"];
    for left in 0..64 {
        for right in 0..64 {
            let mut current = State::default();
            let mut incoming = State::default();
            let mut expected = Vec::new();
            let mut changed = false;
            for (index, name) in names.iter().enumerate() {
                let a = &origins[(left >> (index * 2)) & 3];
                let b = &origins[(right >> (index * 2)) & 3];
                current.bind(name, a.clone());
                incoming.bind(name, b.clone());
                let joined = match (a, b) {
                    (Origin::Known(a), Origin::Known(b)) => {
                        let union: BTreeSet<_> = a.union(b).cloned().collect();
                        Origin::Known(Arc::new(union))
                    }
                    _ => Origin::Unknown,
                };
                changed |= &joined != a;
                expected.push(joined);
            }
            assert_eq!(current.join(&incoming).unwrap(), changed, "join {left}, {right}");
            for (name, expected) in names.iter().zip(&expected) {
                assert_eq!(current.locals.get(*name).unwrap_or(&Origin::Unknown), expected);
            }
            assert!(current.locals.values().all(|origin| matches!(origin, Origin::Known(_))));
        }
    }
}
