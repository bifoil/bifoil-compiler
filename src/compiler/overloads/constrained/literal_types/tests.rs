use super::*;

fn collection(family: &str, mutable: bool, entries: Vec<(Option<Expr>, Expr)>) -> Expr {
    Expr::Collection {
        family: Some(family.into()),
        mutable,
        entries,
    }
}

#[test]
fn literal_candidates_keep_scalar_inference_and_decline_unresolved_names() {
    let env = vec![];
    assert_eq!(options(&Expr::Int(3), &env, &[], None, 0).unwrap(), vec![Type::Int]);
    assert!(
        options(&Expr::Name("missing".into()), &env, &[], None, 0)
            .unwrap()
            .is_empty()
    );
    assert!(
        options(&Expr::Int(3), &env, &[], Some(&Type::Int), 0)
            .unwrap()
            .is_empty()
    );
    assert!(options(&Expr::Block(vec![]), &env, &[], None, 0).unwrap().is_empty());
}

#[test]
fn collection_hints_preserve_family_and_choose_only_complete_mutability_contexts() {
    let env = vec![];
    let literal = collection("List", false, vec![(None, Expr::Int(3))]);
    for (hint, expected) in [
        (
            Type::named("MutableList", vec![Type::Int]),
            Type::named("MutableList", vec![Type::Int]),
        ),
        (
            Type::named("List", vec![Type::Int]),
            Type::named("List", vec![Type::Int]),
        ),
        (
            Type::named("Set", vec![Type::Int]),
            Type::named("List", vec![Type::Int]),
        ),
    ] {
        assert_eq!(options(&literal, &env, &[], Some(&hint), 0).unwrap(), vec![expected]);
    }
    assert_eq!(
        options(
            &collection("Set", false, vec![(None, Expr::Int(3))]),
            &env,
            &[],
            Some(&Type::Pending),
            0,
        )
        .unwrap(),
        vec![Type::named("Set", vec![Type::Int])]
    );
    assert_eq!(
        options(
            &collection(
                "Map",
                false,
                vec![(Some(Expr::String("Original".into())), Expr::Int(3))]
            ),
            &env,
            &[],
            Some(&Type::Pending),
            0,
        )
        .unwrap(),
        vec![Type::named("Map", vec![Type::String, Type::Int])]
    );
    let parameter = Type::Parameter {
        name: "T".into(),
        nonnull: false,
    };
    let hint = Type::named("List", vec![parameter]);
    assert_eq!(
        options(&literal, &env, &["T".into()], Some(&hint), 0).unwrap(),
        vec![Type::named("List", vec![Type::Int])]
    );
    assert_eq!(
        options(
            &collection("List", true, vec![(None, Expr::Int(3))]),
            &env,
            &[],
            Some(&Type::named("List", vec![Type::Int])),
            0
        )
        .unwrap(),
        vec![Type::named("MutableList", vec![Type::Int])]
    );
}

#[test]
fn malformed_and_unstable_literal_slots_cannot_become_callable_candidates() {
    let env = vec![];
    let mutable_list = collection("List", true, vec![(None, Expr::Int(1))]);
    for literal in [
        collection("Map", false, vec![(None, Expr::Int(1))]),
        collection("List", false, vec![(None, Expr::Null)]),
        collection("Map", false, vec![(Some(Expr::Null), Expr::Int(1))]),
        collection("Set", false, vec![(None, mutable_list.clone())]),
        collection("Map", false, vec![(Some(mutable_list), Expr::Int(1))]),
    ] {
        assert!(
            options(&literal, &env, &[], Some(&Type::Pending), 0)
                .unwrap()
                .is_empty()
        );
    }
    let callable = Type::Function {
        params: vec![].into(),
        result: Type::Int.into(),
        suspend: false,
        vararg: false,
    };
    for key in [callable, Type::Any] {
        let hint = std::hint::black_box(Type::named("Map", vec![key, Type::Int]));
        assert!(
            options(&collection("Map", false, vec![]), &env, &[], Some(&hint), 0)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn incompatible_literal_joins_are_discarded_without_changing_existing_slots() {
    let prefix = Slots {
        key: None,
        value: Some(Type::Boolean),
    };
    assert!(extend(vec![prefix.clone()], &[Type::String], false).unwrap().is_empty());
    let retained = extend(vec![prefix], &[Type::Boolean], false).unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].value, Some(Type::Boolean));
}

#[test]
fn literal_alternatives_deduplicate_before_charging_the_existing_budget() {
    let candidates = (0..MAX_LITERAL_ALTERNATIVES)
        .map(|index| Type::named(&format!("Candidate{index}"), vec![]))
        .collect::<Vec<_>>();
    let mut duplicate = candidates.clone();
    duplicate.push(candidates[0].clone());
    let values = extend(vec![Slots::default()], &duplicate, false).unwrap();
    assert_eq!(values.len(), MAX_LITERAL_ALTERNATIVES);
    assert_eq!(
        values
            .iter()
            .map(|slot| slot.value.clone().unwrap())
            .collect::<Vec<_>>(),
        candidates
    );
    let mut excessive = candidates;
    excessive.push(Type::named("OneMoreCandidate", vec![]));
    assert_eq!(
        extend(vec![Slots::default()], &excessive, false).err().unwrap(),
        Failure::Budget.to_string()
    );
}

#[test]
fn literal_traversal_and_constructed_types_keep_their_existing_depth_limits() {
    let env = vec![];
    assert_eq!(
        options(&Expr::Int(1), &env, &[], None, 33).unwrap_err(),
        Failure::Budget.to_string()
    );
    let mut deep = Type::Int;
    for _ in 0..40 {
        deep = Type::named("List", vec![deep]);
    }
    let hint = Type::named("List", vec![deep]);
    assert_eq!(
        options(&collection("List", false, vec![]), &env, &[], Some(&hint), 0).unwrap_err(),
        Failure::Budget.to_string()
    );
}

#[test]
fn converging_literal_joins_share_one_candidate_without_collapsing_distinct_keys() {
    let prefixes = vec![
        Slots {
            key: Some(Type::String),
            value: Some(Type::Int),
        },
        Slots {
            key: Some(Type::String),
            value: Some(Type::Float),
        },
        Slots {
            key: Some(Type::Char),
            value: Some(Type::Float),
        },
    ];
    let values = extend(prefixes, &[Type::Float], false).unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].key, Some(Type::String));
    assert_eq!(values[1].key, Some(Type::Char));
    assert!(values.iter().all(|slots| slots.value == Some(Type::Float)));
}
