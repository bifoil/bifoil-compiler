//! Repeated collection graphs retain value conversion and inference rules.

use crate::model::Type;

fn repeated(levels: usize, leaf: Type) -> Type {
    let mut ty = leaf;
    for _ in 0..levels {
        ty = Type::named("Map", vec![ty.clone(), ty]);
    }
    ty
}

#[test]
fn shared_collection_widening_has_bounded_analysis_work() {
    let integers = repeated(28, Type::Int);
    let floats = repeated(28, Type::Float);
    assert_eq!(floats.conversion_from(&integers), Some(1));
    assert_eq!(integers.conversion_from(&floats), None);
    assert!(!integers.subtype_of(&floats));
    let characters = repeated(28, Type::Char);
    let strings = repeated(28, Type::String);
    assert_eq!(strings.conversion_from(&characters), Some(1));
    assert_eq!(strings.nullable().conversion_from(&characters.nullable()), Some(1));
}

#[test]
fn shared_collection_joins_retain_compact_converted_results() {
    let integers = repeated(28, Type::Int);
    let floats = repeated(28, Type::Float);
    assert_eq!(integers.join(&floats), Some(floats.clone()));
    assert_eq!(floats.join(&integers), Some(floats.clone()));
    assert_eq!(integers.nullable().join(&floats), Some(floats.nullable()));
    assert_eq!(
        repeated(28, Type::Char).join(&repeated(28, Type::String)),
        Some(repeated(28, Type::String))
    );
    assert!(integers.join(&repeated(28, Type::String)).is_none());
}

#[test]
fn shared_conversion_queries_preserve_copy_ranks_and_mutable_invariance() {
    let integers = repeated(28, Type::Int);
    let floats = repeated(28, Type::Float);
    let immutable = Type::named("List", vec![integers.clone()]);
    let mutable = Type::named("MutableList", vec![integers.clone()]);
    let widened = Type::named("List", vec![floats.clone()]);
    let mutable_widened = Type::named("MutableList", vec![floats]);
    assert_eq!(immutable.conversion_from(&mutable), Some(2));
    assert_eq!(widened.conversion_from(&mutable), Some(3));
    assert_eq!(mutable_widened.conversion_from(&immutable), Some(3));
    assert_eq!(mutable_widened.conversion_from(&mutable), None);
    assert!(immutable.join(&mutable).is_none());
    // An immutable outer layer cannot silently copy its mutable child.
    assert_eq!(
        Type::named("List", vec![immutable]).conversion_from(&Type::named("List", vec![mutable])),
        None
    );
}
