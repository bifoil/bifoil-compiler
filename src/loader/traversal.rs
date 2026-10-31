//! Dependency-first strongly connected components without recursive calls.
//! A discovery request must be entered before traversal resumes.

use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Mark {
    index: usize,
    lowlink: usize,
    active: bool,
}

struct Frame {
    name: String,
    dependencies: std::vec::IntoIter<String>,
}

pub(super) enum Step {
    Discover(String),
    Component(Vec<String>),
}

#[derive(Default)]
pub(super) struct Traversal {
    marks: BTreeMap<String, Mark>,
    frames: Vec<Frame>,
    unfinished: Vec<String>,
}

impl Traversal {
    pub(super) fn contains(&self, name: &str) -> bool {
        self.marks.contains_key(name)
    }

    pub(super) fn len(&self) -> usize {
        self.marks.len()
    }

    pub(super) fn enter(&mut self, name: String, dependencies: Vec<String>) {
        let index = self.marks.len();
        let previous = self.marks.insert(
            name.clone(),
            Mark {
                index,
                lowlink: index,
                active: true,
            },
        );
        assert!(previous.is_none(), "a dependency may only be discovered once");
        self.unfinished.push(name.clone());
        self.frames.push(Frame {
            name,
            dependencies: dependencies.into_iter(),
        });
    }

    pub(super) fn next(&mut self) -> Option<Step> {
        while let Some(frame) = self.frames.last_mut() {
            if let Some(dependency) = frame.dependencies.next() {
                let Some(target) = self.marks.get(&dependency).copied() else {
                    return Some(Step::Discover(dependency));
                };
                if target.active {
                    let current = self.marks.get_mut(&frame.name).unwrap();
                    current.lowlink = current.lowlink.min(target.index);
                }
                continue;
            }
            let completed = self.frames.pop().unwrap();
            let mark = self.marks[&completed.name];
            if let Some(parent) = self.frames.last() {
                let parent = self.marks.get_mut(&parent.name).unwrap();
                parent.lowlink = parent.lowlink.min(mark.lowlink);
            }
            if mark.lowlink == mark.index {
                let mut members = Vec::new();
                loop {
                    let name = self.unfinished.pop().unwrap();
                    self.marks.get_mut(&name).unwrap().active = false;
                    let root = name == completed.name;
                    members.push(name);
                    if root {
                        break;
                    }
                }
                members.sort();
                return Some(Step::Component(members));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_cross_edges_and_completed_roots_retain_dependency_order() {
        let graph = BTreeMap::from([
            ("a", vec!["b", "c", "d"]),
            ("b", vec!["e"]),
            ("e", vec!["b"]),
            ("c", vec!["f", "b"]),
            ("f", vec!["c"]),
            ("d", vec!["e", "g"]),
            ("g", vec!["g"]),
            ("z", vec!["a"]),
        ]);
        let mut traversal = Traversal::default();
        let enter = |traversal: &mut Traversal, name: &str| {
            traversal.enter(name.into(), graph[name].iter().map(|name| (*name).into()).collect());
        };
        enter(&mut traversal, "a");
        let mut components = Vec::new();
        while let Some(step) = traversal.next() {
            match step {
                Step::Discover(name) => enter(&mut traversal, &name),
                Step::Component(members) => components.push(members),
            }
        }
        assert_eq!(
            components,
            [vec!["b", "e"], vec!["c", "f"], vec!["g"], vec!["d"], vec!["a"]]
        );
        assert_eq!(traversal.len(), 7);
        enter(&mut traversal, "z");
        let Some(Step::Component(members)) = traversal.next() else {
            panic!("completed imports must not be rediscovered")
        };
        assert_eq!(members, ["z"]);
        assert!(traversal.next().is_none());
        assert_eq!(traversal.len(), 8);
    }

    #[test]
    fn deep_discovery_uses_an_explicit_stack() {
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                const COUNT: usize = 100_000;
                let mut traversal = Traversal::default();
                traversal.enter("m0".into(), vec!["m1".into()]);
                let mut completed = 0;
                while let Some(step) = traversal.next() {
                    match step {
                        Step::Discover(name) => {
                            let index: usize = name[1..].parse().unwrap();
                            let dependencies = if index + 1 < COUNT {
                                vec![format!("m{}", index + 1)]
                            } else {
                                vec![]
                            };
                            traversal.enter(name, dependencies);
                        }
                        Step::Component(members) => {
                            assert_eq!(members, [format!("m{}", COUNT - completed - 1)]);
                            completed += 1;
                        }
                    }
                }
                assert_eq!(completed, COUNT);
                assert_eq!(traversal.len(), COUNT);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
