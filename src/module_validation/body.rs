//! Structural facts retained from the first typed body decode.
//!
//! Canonical digest verification still reads checked private bytes before these
//! facts are used. References and legacy interfaces wait for the complete module.

use crate::model::{Instruction, LegacyTarget, MAX_INSTRUCTIONS, Op, Signature};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, LazyLock};

type CaptureNames = Option<Arc<BTreeSet<String>>>;
pub(super) type ClosureCaptures = BTreeMap<Arc<str>, CaptureNames>;

#[derive(Default)]
pub(crate) struct Facts {
    events: Vec<Event>,
}
enum Event {
    Invalid(&'static str),
    Reference {
        function: Arc<str>,
        captures: Result<CaptureNames, &'static str>,
        named: bool,
    },
    Legacy(LegacyTarget),
}

impl Facts {
    pub(crate) fn collect(code: &[Instruction]) -> Arc<Self> {
        let mut facts = Self::default();
        for instruction in code {
            if instruction.span.file.is_empty() || instruction.span.line == 0 {
                facts
                    .events
                    .push(Event::Invalid("invalid instruction source location in module"));
                break;
            }
            if instruction
                .op
                .instruction_target()
                .is_some_and(|target| target >= code.len())
            {
                facts
                    .events
                    .push(Event::Invalid("invalid instruction target in module"));
                break;
            }
            match &instruction.op {
                Op::Closure { function, captures, .. } | Op::FunctionValue { function, captures } => {
                    let captures = capture_names(captures);
                    let invalid = captures.is_err();
                    facts.events.push(Event::Reference {
                        function: function.as_str().into(),
                        captures,
                        named: matches!(instruction.op, Op::Closure { .. }),
                    });
                    if invalid {
                        break;
                    }
                }
                Op::LegacyCall { target, .. } => facts.events.push(Event::Legacy(target.clone())),
                _ => {}
            }
        }
        // Most bodies need no delayed checks. Do not allocate a fact record for
        // every pure routine in a wide independently compiled interface.
        if facts.events.is_empty() {
            static EMPTY: LazyLock<Arc<Facts>> = LazyLock::new(|| Arc::new(Facts::default()));
            Arc::clone(&EMPTY)
        } else {
            Arc::new(facts)
        }
    }

    pub(super) fn verify(
        &self,
        signature: &Signature,
        contains_function: impl Fn(&str) -> bool,
        closures: &mut ClosureCaptures,
        enum_interface: &impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        for event in &self.events {
            match event {
                Event::Invalid(error) => return Err((*error).into()),
                Event::Reference {
                    function,
                    captures,
                    named,
                } => {
                    if !contains_function(function) {
                        return Err("invalid local closure target".into());
                    }
                    let captures = captures.as_ref().map_err(|error| (*error).to_owned())?;
                    if *named
                        && let Some(previous) = closures.insert(Arc::clone(function), captures.clone())
                        && previous != *captures
                    {
                        return Err("inconsistent named closure captures".into());
                    }
                }
                Event::Legacy(target) => {
                    crate::legacy::validate_signature_with(signature, target, enum_interface)?;
                }
            }
        }
        Ok(())
    }
}

fn capture_names(names: &Option<Vec<String>>) -> Result<CaptureNames, &'static str> {
    let Some(names) = names else { return Ok(None) };
    if names.len() > MAX_INSTRUCTIONS || names.iter().any(String::is_empty) {
        return Err("invalid closure capture names");
    }
    let unique: BTreeSet<_> = names.iter().cloned().collect();
    if unique.len() != names.len() {
        return Err("invalid closure capture names");
    }
    Ok(Some(Arc::new(unique)))
}
