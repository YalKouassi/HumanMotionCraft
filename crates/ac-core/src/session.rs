//! The session: the document, the registered commands and the undo history.
//!
//! The session's own commands (`edit.*`, `session.*`: undo, gestures, batches, dry runs, history,
//! checkpoints, reset, info) live in [`crate::history`] and [`crate::batch`]; this file runs a
//! command, keeps the undo stacks and names the steps.

use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::{Map, Value, json};

use crate::command::{CommandSpec, check};
use crate::document::Document;
use crate::error::{CoreError, split_hint};
use crate::{diff, outputs, suggest};

/// Undo steps kept; the oldest go first.
pub const MAX_UNDO: usize = 500;

/// An undo step: the command (or gesture) that made it and how a person reads it ("AI fill
/// 1–29", "Move key 20→30", "Drag joint").
#[derive(Debug, Clone)]
pub(crate) struct Step {
    pub(crate) id: String,
    pub(crate) label: String,
}

pub struct Session {
    pub document: Document,
    commands: Vec<&'static CommandSpec>,
    pub(crate) undo: Vec<(Step, Document)>,
    pub(crate) redo: Vec<(Step, Document)>,
    /// An open gesture (a drag, a rotate, a batch): the step it makes and the document before
    /// it. Undoable commands inside it make that one undo step when it ends.
    pub(crate) gesture: Option<(Step, Document)>,
    /// Steps dropped off the bottom of the undo stack, so history indices stay put.
    pub(crate) dropped: usize,
    /// Named points of the history (`edit.checkpoint`): name and history index.
    pub(crate) checkpoints: Vec<(String, usize)>,
    /// Check every result against its command's declared outputs; a result that breaks them is
    /// refused as a bug ([`CoreError::Outputs`]). On in debug and test builds.
    pub check_outputs: bool,
}

impl Session {
    /// A session with ac-core's own commands registered.
    pub fn new(document: Document) -> Session {
        let mut s = Session {
            document,
            commands: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            dropped: 0,
            checkpoints: Vec::new(),
            check_outputs: cfg!(debug_assertions),
        };
        for area in crate::commands::AREAS {
            s.register(area);
        }
        s
    }

    /// Add a crate's commands. A later spec with the same id replaces nothing: it is refused, so
    /// two crates can never fight over an id silently.
    pub fn register(&mut self, specs: &'static [CommandSpec]) -> Vec<CoreError> {
        let mut clashes = Vec::new();
        for spec in specs {
            if self.spec(spec.id).is_some() {
                clashes.push(CoreError::Refused {
                    command: spec.id.to_string(),
                    message: "a command with this id is already registered".into(),
                    hint: None,
                });
            } else {
                self.commands.push(spec);
            }
        }
        clashes
    }

    pub fn commands(&self) -> &[&'static CommandSpec] {
        &self.commands
    }

    pub fn spec(&self, id: &str) -> Option<&'static CommandSpec> {
        self.commands.iter().copied().find(|s| s.id == id)
    }

    /// The spec for `id`, or an unknown-command error with the closest ids.
    pub(crate) fn find(&self, id: &str) -> Result<&'static CommandSpec, CoreError> {
        self.spec(id).ok_or_else(|| CoreError::UnknownCommand {
            command: id.to_string(),
            suggestions: suggest::closest(id, self.commands.iter().map(|c| c.id)),
        })
    }

    /// Run a command by id with JSON parameters. On any refusal or panic the document is exactly
    /// as it was. The session's own commands (`edit.*`, `session.*`) act on the history.
    ///
    /// Each run is a `command` tracing span (id, undoable, refused); the front end's subscriber
    /// times it. No clock is read here, so results stay deterministic.
    pub fn run(&mut self, id: &str, params: &Value) -> Result<Value, CoreError> {
        let undoable = self.spec(id).is_some_and(|s| s.undoable);
        let span = tracing::debug_span!("command", id, undoable, refused = tracing::field::Empty);
        let _in = span.enter();
        let out = self.run_traced(id, params);
        span.record("refused", out.is_err());
        if let Err(e) = &out {
            tracing::debug!("refused: {e}");
        }
        out
    }

    fn run_traced(&mut self, id: &str, params: &Value) -> Result<Value, CoreError> {
        let spec = self.find(id)?;
        // every command's parameters are checked, the session's own included
        let checked = check(spec, params).map_err(|message| CoreError::Params {
            command: id.to_string(),
            message,
        })?;
        if crate::commands::is_session_own(spec) {
            let result = self.run_own(spec, &checked)?;
            self.check_result(spec, &result)?;
            return Ok(result);
        }
        self.run_spec(spec, &checked)
    }

    /// A result against the spec's declared outputs, when checking is on.
    fn check_result(&self, spec: &CommandSpec, result: &Value) -> Result<(), CoreError> {
        if !self.check_outputs {
            return Ok(());
        }
        outputs::check(spec, result).map_err(|message| CoreError::Outputs {
            command: spec.id.to_string(),
            message,
        })
    }

    /// Run a registered (not session-own) command: undo step, change summary, refusals restored.
    fn run_spec(
        &mut self,
        spec: &'static CommandSpec,
        checked: &Map<String, Value>,
    ) -> Result<Value, CoreError> {
        let id = spec.id;
        let before = self.document.clone();
        let outcome = catch_unwind(AssertUnwindSafe(|| (spec.run)(&mut self.document, checked)));
        match outcome {
            Ok(Ok(mut result)) => {
                if let Err(e) = self.check_result(spec, &result) {
                    self.restore(before);
                    return Err(e);
                }
                if spec.undoable {
                    if let Some(m) = result.as_object_mut() {
                        m.insert("changed".into(), diff::summary(&before, &self.document));
                    }
                    if self.gesture.is_none() && self.document != before {
                        let label = spec.step_label(checked, &result);
                        self.push_undo(
                            Step {
                                id: id.to_string(),
                                label,
                            },
                            before,
                        );
                    }
                }
                Ok(result)
            }
            Ok(Err(message)) => {
                self.restore(before);
                let (message, hint) = split_hint(&message);
                let message = if message.trim().is_empty() {
                    "refused without a reason (a bug)".into()
                } else {
                    message
                };
                Err(CoreError::Refused {
                    command: id.to_string(),
                    message,
                    hint,
                })
            }
            Err(panic) => {
                self.restore(before);
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".into());
                Err(CoreError::Panicked {
                    command: id.to_string(),
                    message,
                })
            }
        }
    }

    /// Put `doc` in place and return the one it replaces, keeping what lives outside the undo
    /// history: what the library holds (`Document::saved`) doesn't change when an edit is undone.
    pub(crate) fn restore(&mut self, doc: Document) -> Document {
        let saved = std::mem::take(&mut self.document.saved);
        let old = std::mem::replace(&mut self.document, doc);
        self.document.saved = saved;
        old
    }

    /// The history index of the document shown: steps done since the session began.
    pub fn history_index(&self) -> usize {
        self.dropped + self.undo.len()
    }

    pub(crate) fn push_undo(&mut self, step: Step, before: Document) {
        // the steps that were undone are gone, and so are checkpoints on them
        let here = self.history_index();
        self.checkpoints.retain(|(_, i)| *i <= here);
        self.undo.push((step, before));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
            self.dropped += 1;
            let first = self.dropped;
            self.checkpoints.retain(|(_, i)| *i >= first);
        }
        self.redo.clear();
    }

    /// Close the open gesture: one undo step if the document changed. Returns whether it did.
    pub fn end_gesture(&mut self) -> bool {
        let Some((step, before)) = self.gesture.take() else {
            return false;
        };
        let changed = self.document != before;
        if changed {
            self.push_undo(step, before);
        }
        changed
    }

    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    pub(crate) fn step(&mut self, undo: bool) -> Value {
        self.end_gesture();
        let from = if undo { &mut self.undo } else { &mut self.redo };
        let Some((step, doc)) = from.pop() else {
            return json!({ "changed": false, "note": if undo { "nothing to undo" } else { "nothing to redo" } });
        };
        let current = self.restore(doc);
        let out = json!({ "changed": true, "command": step.id, "label": step.label });
        let to = if undo { &mut self.redo } else { &mut self.undo };
        to.push((step, current));
        out
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What `edit.undo` would undo, as a person reads it.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.0.label.as_str())
    }

    /// What `edit.redo` would redo.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.0.label.as_str())
    }

    /// The undo history, oldest first.
    pub fn history(&self) -> Vec<&str> {
        self.undo.iter().map(|s| s.0.label.as_str()).collect()
    }

    /// What `edit.redo` would redo, in the order it would: the next redo first.
    pub fn redo_history(&self) -> Vec<&str> {
        self.redo.iter().rev().map(|s| s.0.label.as_str()).collect()
    }
}
