//! Completed dependency queries shared by initializers with common call targets.
//! Only successful, context-independent results are cached; recursive walks keep
//! their own visited set. Borrowed binding names never outlive their summaries.

use crate::model::FunctionSummary;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

type Bindings<'a> = BTreeSet<&'a str>;
const MAX_CACHED_BINDINGS: usize = 32768;

pub(super) struct Query<'a> {
    summaries: &'a BTreeMap<&'a str, &'a FunctionSummary>,
    possible_targets: BTreeSet<&'a str>,
    repeated: BTreeSet<&'a str>,
    cached: BTreeMap<&'a str, Arc<Bindings<'a>>>,
    cached_bindings: usize,
    indirect: Option<Arc<Bindings<'a>>>,
    started: Instant,
}
impl<'a> Query<'a> {
    pub(super) fn new(
        summaries: &'a BTreeMap<&'a str, &'a FunctionSummary>,
        possible_targets: BTreeSet<&'a str>,
        started: Instant,
    ) -> Result<Self, String> {
        let mut incoming = BTreeMap::<&str, usize>::new();
        for summary in summaries.values() {
            crate::budget(started)?;
            for target in &summary.calls {
                *incoming.entry(target.as_str()).or_default() += 1;
            }
        }
        Ok(Self {
            summaries,
            possible_targets,
            repeated: incoming
                .into_iter()
                .filter_map(|(id, count)| (count > 1).then_some(id))
                .collect(),
            cached: BTreeMap::new(),
            cached_bindings: 0,
            indirect: None,
            started,
        })
    }

    pub(super) fn initializer(&mut self, id: &str) -> Result<Bindings<'a>, String> {
        let (id, _) = self.summaries.get_key_value(id).ok_or_else(|| missing(id))?;
        self.walk(vec![*id], true)
    }

    fn remember(&mut self, id: &'a str, bindings: Arc<Bindings<'a>>) {
        if self.cached_bindings + bindings.len() <= MAX_CACHED_BINDINGS {
            self.cached_bindings += bindings.len();
            self.cached.insert(id, bindings);
        }
    }

    fn walk(&mut self, mut pending: Vec<&'a str>, cache_new: bool) -> Result<Bindings<'a>, String> {
        let mut visited = BTreeSet::new();
        let mut dependencies = BTreeSet::new();
        let mut expanded_indirect = false;
        while let Some(id) = pending.pop() {
            crate::budget(self.started)?;
            if !visited.insert(id) {
                continue;
            }
            if let Some(bindings) = self.cached.get(id) {
                dependencies.extend(bindings.iter().copied());
                continue;
            }
            if cache_new && self.repeated.contains(id) {
                // The inner walk may consume completed results, but cannot
                // recursively start another new query. Native stack depth stays
                // bounded even for a long or mutually recursive call chain.
                let bindings = Arc::new(self.walk(vec![id], false)?);
                dependencies.extend(bindings.iter().copied());
                self.remember(id, bindings);
                continue;
            }
            let summary = *self.summaries.get(id).ok_or_else(|| missing(id))?;
            if let Some(error) = &summary.error {
                return Err(format!("cannot prove shared initialization through {id}: {error}"));
            }
            dependencies.extend(summary.bindings.iter().map(String::as_str));
            pending.extend(summary.calls.iter().map(String::as_str));
            if summary.indirect_call && !expanded_indirect {
                expanded_indirect = true;
                if let Some(bindings) = &self.indirect {
                    dependencies.extend(bindings.iter().copied());
                } else if cache_new {
                    let targets = self.possible_targets.iter().copied().collect();
                    let bindings = Arc::new(self.walk(targets, false)?);
                    dependencies.extend(bindings.iter().copied());
                    // At most one additional set covers the conservative target
                    // group, bounded by the program's shared binding inventory.
                    self.indirect = Some(bindings);
                } else {
                    pending.extend(self.possible_targets.iter().copied());
                }
            }
        }
        Ok(dependencies)
    }
}
fn missing(id: &str) -> String {
    format!("missing initializer call summary {id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference<'a>(
        root: &'a str,
        summaries: &'a BTreeMap<String, FunctionSummary>,
        targets: &BTreeSet<&'a str>,
    ) -> Bindings<'a> {
        let mut pending = vec![root];
        let mut seen = BTreeSet::new();
        let mut result = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let summary = &summaries[id];
            result.extend(summary.bindings.iter().map(String::as_str));
            pending.extend(summary.calls.iter().map(String::as_str));
            if summary.indirect_call {
                pending.extend(targets.iter().copied());
            }
        }
        result
    }

    #[test]
    fn completed_queries_match_plain_walks_through_cycles_and_indirect_calls() {
        for seed in 0..96 {
            let mut summaries = BTreeMap::new();
            for index in 0..12 {
                let calls = (0..12)
                    .filter(|next| (seed * 17 + index * 29 + next * 11 + index * next * 3) % 31 < 2)
                    .map(|next| format!("f{next}"))
                    .collect();
                summaries.insert(
                    format!("f{index}"),
                    FunctionSummary {
                        calls,
                        bindings: BTreeSet::from([format!("value{index}")]),
                        indirect_call: (index + seed) % 19 == 0,
                        error: None,
                    },
                );
            }
            let targets = BTreeSet::from(["f1", "f4", "f8"]);
            let view = summaries.iter().map(|(id, summary)| (id.as_str(), summary)).collect();
            let mut query = Query::new(&view, targets.clone(), Instant::now()).unwrap();
            for root in summaries.keys() {
                assert_eq!(query.initializer(root).unwrap(), reference(root, &summaries, &targets));
            }
        }
    }

    #[test]
    fn unreachable_failures_stay_deferred_and_failed_queries_are_not_cached() {
        let mut summaries = BTreeMap::from([
            ("good".to_owned(), FunctionSummary::default()),
            (
                "bad".to_owned(),
                FunctionSummary {
                    calls: BTreeSet::from(["missing".into()]),
                    ..Default::default()
                },
            ),
            (
                "first".to_owned(),
                FunctionSummary {
                    calls: BTreeSet::from(["bad".into()]),
                    ..Default::default()
                },
            ),
            (
                "second".to_owned(),
                FunctionSummary {
                    calls: BTreeSet::from(["bad".into()]),
                    ..Default::default()
                },
            ),
        ]);
        let view = summaries.iter().map(|(id, summary)| (id.as_str(), summary)).collect();
        let mut query = Query::new(&view, BTreeSet::new(), Instant::now()).unwrap();
        assert!(query.initializer("good").unwrap().is_empty());
        for root in ["first", "second"] {
            assert_eq!(
                query.initializer(root).unwrap_err(),
                "missing initializer call summary missing"
            );
        }
        drop(query);
        summaries.get_mut("bad").unwrap().error = Some("original analysis failure".into());
        let view = summaries.iter().map(|(id, summary)| (id.as_str(), summary)).collect();
        let mut query = Query::new(&view, BTreeSet::new(), Instant::now()).unwrap();
        assert_eq!(
            query.initializer("first").unwrap_err(),
            "cannot prove shared initialization through bad: original analysis failure"
        );
    }

    #[test]
    fn long_reused_call_chains_keep_native_stack_depth_bounded() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let mut summaries = BTreeMap::new();
                for index in 0..3000 {
                    summaries.insert(
                        format!("f{index}"),
                        FunctionSummary {
                            calls: if index == 0 {
                                BTreeSet::new()
                            } else {
                                BTreeSet::from([format!("f{}", index - 1)])
                            },
                            bindings: if index == 0 {
                                BTreeSet::from(["value".into()])
                            } else {
                                BTreeSet::new()
                            },
                            ..Default::default()
                        },
                    );
                }
                for root in ["first", "second"] {
                    summaries.insert(
                        root.into(),
                        FunctionSummary {
                            calls: BTreeSet::from(["f2999".into()]),
                            ..Default::default()
                        },
                    );
                }
                let view = summaries.iter().map(|(id, summary)| (id.as_str(), summary)).collect();
                let mut query = Query::new(&view, BTreeSet::new(), Instant::now()).unwrap();
                for root in ["first", "second"] {
                    assert_eq!(query.initializer(root).unwrap(), BTreeSet::from(["value"]));
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
