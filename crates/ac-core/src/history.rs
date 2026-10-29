//! The session's own commands: undo and redo, gestures, the history an agent reads
//! (`edit.history`), named checkpoints, going back to one, a fresh session and what it runs with.
//!
//! **History indices.** Index 0 is the document before the first step the session made; index
//! `n` is the document after step `n`. `edit.history` lists the steps done (`done: true`) and
//! the steps undone that redo would bring back (`done: false`). Indices stay put when the oldest
//! steps fall off the undo stack ([`crate::session::MAX_UNDO`]); `first` says how far back one can
//! still go.
//!
//! **Going back (`edit.revert_to`) walks the undo stack**: it is the same as pressing undo (or
//! redo) step by step, so it adds no step of its own, and redo, or `edit.revert_to` the later
//! index, brings the work back. A new edit after going back drops the undone steps, as any edit
//! after an undo does, and with them the checkpoints on them.

use serde_json::{Map, Value, json};

use crate::command::{CommandSpec, text};
use crate::document::Document;
use crate::error::CoreError;
use crate::session::{Session, Step};

/// Most checkpoints a session keeps (the oldest goes first).
pub const MAX_CHECKPOINTS: usize = 256;

type P = Map<String, Value>;

fn params(spec: &CommandSpec, message: impl Into<String>) -> CoreError {
    CoreError::Params {
        command: spec.id.to_string(),
        message: message.into(),
    }
}

impl Session {
    /// Run one of the session's own commands (its parameters already checked).
    pub(crate) fn run_own(&mut self, spec: &'static CommandSpec, p: &P) -> Result<Value, CoreError> {
        match spec.id {
            "edit.undo" => Ok(self.step(true)),
            "edit.redo" => Ok(self.step(false)),
            "edit.begin" => {
                self.end_gesture();
                let step = Step {
                    id: spec.id.to_string(),
                    label: spec.step_label(p, &Value::Null),
                };
                self.gesture = Some((step, self.document.clone()));
                Ok(json!({}))
            }
            "edit.end" => Ok(json!({ "changed": self.end_gesture() })),
            "edit.batch" => self.batch(spec, p),
            "edit.dry_run" => self.dry_run(spec, p),
            "edit.history" => Ok(self.history_json()),
            "edit.checkpoint" => self.checkpoint(spec, p),
            "edit.revert_to" => self.revert_to(spec, p),
            "session.reset" => Ok(self.reset()),
            "session.info" => Ok(self.info()),
            other => Err(CoreError::Refused {
                command: other.to_string(),
                message: "the session has no handler for this command (a bug)".into(),
                hint: None,
            }),
        }
    }

    /// The history as `edit.history` returns it.
    pub fn history_json(&self) -> Value {
        let first = self.dropped;
        let mut steps: Vec<Value> = self
            .undo
            .iter()
            .enumerate()
            .map(|(i, (s, _))| json!({ "index": first + i + 1, "label": s.label, "command": s.id, "done": true }))
            .collect();
        let here = self.history_index();
        // the redo stack's top is the next step
        steps.extend(self.redo.iter().rev().enumerate().map(
            |(i, (s, _))| json!({ "index": here + i + 1, "label": s.label, "command": s.id, "done": false }),
        ));
        json!({
            "steps": steps,
            "current": here,
            "first": first,
            "checkpoints": self.checkpoints.iter().map(|(n, i)| json!({ "name": n, "index": i })).collect::<Vec<_>>(),
            "gesture": self.gesture.as_ref().map(|(s, _)| s.label.clone()),
        })
    }

    /// Name the document shown (an open gesture is closed first, so the name covers it).
    fn checkpoint(&mut self, spec: &CommandSpec, p: &P) -> Result<Value, CoreError> {
        let name = text(p, "name")
            .ok_or_else(|| params(spec, "name missing"))?
            .to_string();
        self.end_gesture();
        let index = self.history_index();
        self.checkpoints.retain(|(n, _)| *n != name);
        self.checkpoints.push((name.clone(), index));
        if self.checkpoints.len() > MAX_CHECKPOINTS {
            self.checkpoints.remove(0);
        }
        Ok(json!({ "name": name, "index": index }))
    }

    /// Undo (or redo) step by step to a checkpoint or a history index.
    fn revert_to(&mut self, spec: &CommandSpec, p: &P) -> Result<Value, CoreError> {
        let refuse = |message: String| CoreError::Refused {
            command: spec.id.to_string(),
            message,
            hint: Some("edit.history lists the indices and checkpoints".into()),
        };
        self.end_gesture();
        let target = match (text(p, "name"), p.get("index").and_then(Value::as_f64)) {
            (Some(n), None) => self
                .checkpoints
                .iter()
                .find(|(c, _)| c == n)
                .map(|(_, i)| *i)
                .ok_or_else(|| {
                    let names: Vec<&str> = self.checkpoints.iter().map(|(c, _)| c.as_str()).collect();
                    let near = crate::suggest::closest(n, names.iter().copied());
                    let mut m = format!("no checkpoint named {n:?} (there are {names:?})");
                    if !near.is_empty() {
                        m.push_str("; ");
                        m.push_str(&crate::suggest::did_you_mean(&near));
                    }
                    refuse(m)
                })?,
            (None, Some(i)) => i as usize,
            _ => return Err(params(spec, "give exactly one of `name` and `index`")),
        };
        let from = self.history_index();
        let (first, last) = (self.dropped, from + self.redo.len());
        if target < first || target > last {
            return Err(refuse(format!(
                "index {target} is not reachable: the history goes from {first} to {last}"
            )));
        }
        let (mut undone, mut redone) = (0, 0);
        while self.history_index() > target && self.can_undo() {
            self.step(true);
            undone += 1;
        }
        while self.history_index() < target && self.can_redo() {
            self.step(false);
            redone += 1;
        }
        Ok(json!({ "from": from, "to": self.history_index(), "undone": undone, "redone": redone }))
    }

    /// A fresh document on the same rig, limits, model, library and assets; the undo history,
    /// checkpoints and any gesture are cleared. What the library holds stays (it is on disk).
    pub fn reset(&mut self) -> Value {
        let old = &self.document;
        let mut fresh = Document::new((*old.skeleton).clone());
        fresh.skeleton = old.skeleton.clone();
        fresh.library.clone_from(&old.library);
        fresh.assets.clone_from(&old.assets);
        fresh.limits.clone_from(&old.limits);
        fresh.ai.clone_from(&old.ai);
        fresh.custom_parts.clone_from(&old.custom_parts);
        fresh.saved.clone_from(&old.saved);
        let dropped = self.undo.len() + self.redo.len();
        self.document = fresh;
        self.undo.clear();
        self.redo.clear();
        self.gesture = None;
        self.checkpoints.clear();
        self.dropped = 0;
        json!({ "reset": true, "dropped": dropped })
    }

    /// What this session runs with (`session.info`).
    pub fn info(&self) -> Value {
        let d = &self.document;
        json!({
            "product": crate::product::NAME,
            "version": env!("CARGO_PKG_VERSION"),
            "seed": Value::Null,
            "deterministic": true,
            "rig_hash": crate::hash::skeleton(&d.skeleton),
            "limits_hash": d.limits.as_deref().map(crate::hash::limits),
            "model": d.ai.as_ref().map(|m| json!({ "tag": m.info().tag, "hash": m.hash() })),
            "commands": self.commands().len(),
            "history": self.history_index(),
            "check_outputs": self.check_outputs,
        })
    }
}
