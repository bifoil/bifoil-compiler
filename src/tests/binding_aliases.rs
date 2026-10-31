//! Writes through shared aliases invalidate all views of the same cell.

use super::compile;
use crate::{compiler, parser};
use std::collections::BTreeMap;
use std::time::Instant;

const STATE: &str = r#"export shared var name: String? = "Ada"
export shared var dynamic: Any? = "Ada"
export shared var values: Map<String, String?> = {"name": "Ada"}
export shared var key: String = "name"
export shared val fixed: String? = "Ada"
"#;

fn client(source: &str) -> Result<crate::model::Module, String> {
    let state = compile(STATE)?;
    compiler::compile(
        parser::parse(source, "src/client.bfl")?,
        "client",
        "src/client.bfl",
        &BTreeMap::from([("test".into(), &state)]),
        Instant::now(),
    )
}

#[test]
fn shared_alias_writes_discard_null_type_and_lookup_facts_in_lowering_and_inference() {
    for source in [
        r#"import test.name as first
import test.name as second
scene start():
    if first != null:
        second = null
        val length = first.length
"#,
        r#"import test.values as first
import test.values as second
scene start():
    first["name"]!!
    second = {"name": null}
    val length = first["name"].length
"#,
        r#"import test.dynamic as first
import test.dynamic as second
scene start():
    if first is String:
        second = 7
        val length = first.length
"#,
        r#"import test.name as first
import test.name as second
fun read():
    if first != null:
        second = null
        return first.length
    return 0
scene start():
    Unit
"#,
    ] {
        client(source).expect_err("every alias observes the shared cell's new contents");
    }
}

#[test]
fn shared_aliases_can_be_checked_again_and_local_snapshots_keep_their_values() {
    client(
        r#"import test.name as first
import test.name as second
scene start():
    if first != null:
        val snapshot = first
        var local: String? = first
        local!!
        second = null
        val a = snapshot.length
        val b = local.length
        second = "Bifoil"
        if first != null:
            val c = first.length
"#,
    )
    .unwrap();
}

#[test]
fn unaliased_globals_keep_independent_facts_and_local_bindings_are_not_source_aliases() {
    client(
        r#"import test.name
import test.values
scene start():
    name!!
    var copy = name
    copy!!
    values = {"name": null}
    val before = name.length
    name = null
    val retained = copy.length
"#,
    )
    .unwrap();
}

#[test]
fn shared_alias_guards_apply_to_other_names_for_the_same_current_value() {
    client(
        r#"import test.name as first
import test.name as second
import test.dynamic as dynamicFirst
import test.dynamic as dynamicSecond
scene start():
    if first != null:
        val a = second.length
    if dynamicFirst is String:
        val b = dynamicSecond.length
"#,
    )
    .unwrap();
}

#[test]
fn shared_alias_paths_and_index_keys_share_only_their_current_cell_proofs() {
    client(
        r#"import test.values as first
import test.values as second
import test.key as keyFirst
import test.key as keySecond
scene start():
    first["name"]!!
    val a = second["name"].length
    val local: Map<String, String?> = {"name": "Ada"}
    local[keyFirst]!!
    val b = local[keySecond].length
"#,
    )
    .unwrap();
    let error = client(
        r#"import test.values as first
import test.values as second
scene start():
    first["name"]!!
    second = {"name": null}
    val a = first["name"].length
"#,
    )
    .unwrap_err();
    assert!(error.contains("nullable"), "{error}");
}

#[test]
fn readonly_global_aliases_retain_stable_choice_guard_proofs() {
    client(
        r#"import test.fixed as first
import test.fixed as second
scene start():
    choice:
        option "Read fixed alias" if first != null:
            val length = second.length
"#,
    )
    .unwrap();
}
