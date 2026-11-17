//! Share checked cyclic identities without retaining discarded member lists.

use crate::model::ModuleGroup;
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

/// Normal loaded components share identity storage. Other views still require
/// content equality; pointer sharing never establishes an otherwise false proof.
pub(super) fn same_identity(left: &Arc<ModuleGroup>, right: &Arc<ModuleGroup>) -> bool {
    Arc::ptr_eq(left, right) || left == right
}

/// A fingerprint selects a candidate, never a substitute for content validation.
/// Conflicting lists stay distinct so normal component validation can reject them.
/// Weak group handles leave no live member vector after its last owner is dropped.
#[derive(Default)]
pub(super) struct Memberships {
    groups: BTreeMap<String, Weak<ModuleGroup>>,
}
impl Memberships {
    pub(super) fn share(&mut self, group: &mut Arc<ModuleGroup>) {
        if let Some(cached) = self.groups.get_mut(&group.digest) {
            if let Some(candidate) = cached.upgrade() {
                if candidate.as_ref() == group.as_ref() {
                    *group = candidate;
                }
            } else {
                *cached = Arc::downgrade(group);
            }
        } else {
            self.groups.insert(group.digest.clone(), Arc::downgrade(group));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(names: &[&str], digest: char) -> Arc<ModuleGroup> {
        Arc::new(ModuleGroup {
            members: names.iter().map(|name| (*name).to_owned()).collect(),
            digest: digest.to_string().repeat(64),
        })
    }

    #[test]
    fn independently_decoded_groups_share_storage_without_changing_canonical_json() {
        let original = identity(&["alpha", "beta"], 'a');
        let expected = format!(r#"{{"members":["alpha","beta"],"digest":"{}"}}"#, "a".repeat(64));
        let bytes = serde_json::to_vec(&original).unwrap();
        assert_eq!(bytes, expected.as_bytes());
        let mut first: Arc<ModuleGroup> = serde_json::from_slice(&bytes).unwrap();
        let mut second: Arc<ModuleGroup> = serde_json::from_slice(&bytes).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(same_identity(&first, &second));
        let mut memberships = Memberships::default();
        memberships.share(&mut first);
        memberships.share(&mut second);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(serde_json::to_vec(&first).unwrap(), bytes);
        assert_eq!(serde_json::to_vec(&second).unwrap(), bytes);
    }

    #[test]
    fn conflicting_members_cannot_acquire_an_equal_identity_from_a_claimed_fingerprint() {
        let mut first = identity(&["alpha", "beta"], 'a');
        let mut conflict = identity(&["alpha", "gamma"], 'a');
        let expected = serde_json::to_vec(&conflict).unwrap();
        let mut memberships = Memberships::default();
        memberships.share(&mut first);
        memberships.share(&mut conflict);
        assert!(!Arc::ptr_eq(&first, &conflict));
        assert!(!same_identity(&first, &conflict));
        assert_eq!(serde_json::to_vec(&conflict).unwrap(), expected);
        let mut matching = identity(&["alpha", "beta"], 'a');
        memberships.share(&mut matching);
        assert!(Arc::ptr_eq(&first, &matching));
    }

    #[test]
    fn matching_members_do_not_conceal_different_component_content() {
        let mut first = identity(&["alpha", "beta"], 'a');
        let mut changed = identity(&["alpha", "beta"], 'b');
        let mut memberships = Memberships::default();
        memberships.share(&mut first);
        memberships.share(&mut changed);
        assert!(!Arc::ptr_eq(&first, &changed));
        assert!(!same_identity(&first, &changed));
    }

    #[test]
    fn discarded_components_release_their_identity_and_replacements_can_share() {
        let mut memberships = Memberships::default();
        let mut first = identity(&["alpha", "beta"], 'a');
        memberships.share(&mut first);
        let previous = Arc::downgrade(&first);
        drop(first);
        assert!(previous.upgrade().is_none());
        let mut replacement = identity(&["alpha", "beta"], 'a');
        let mut another = identity(&["alpha", "beta"], 'a');
        memberships.share(&mut replacement);
        memberships.share(&mut another);
        assert!(Arc::ptr_eq(&replacement, &another));
        assert!(previous.upgrade().is_none());
    }
}
