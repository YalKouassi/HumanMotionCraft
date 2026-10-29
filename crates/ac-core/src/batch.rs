//! Several commands as one edit (`edit.batch`), and trying a command without keeping it
//! (`edit.dry_run`).
//!
//! **A batch** runs its steps in order through the session (each checked and refused as on its
//! own) inside a gesture, so the whole batch is one undo step named by its `label`. Atomic (the
//! default): the first refusal puts the document back as it was before the batch and the error
//! says which step failed and why; nothing enters the history. Not atomic: the steps before the
//! refusal are kept (one undo step) and the result says which step failed. Files a step wrote
//! (`writes` commands: saves, exports) are outside the document: a rollback cannot take them back,
//! and the error says so.
//!
//! **A dry run** runs one command on a copy of the document: its result and change summary come
//! back, the document and the history are untouched. Commands that write files and the session's
//! own commands are refused (a copy cannot keep them from acting).

use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::{Map, Value, json};

use crate::command::{CommandSpec, check, text};
use crate::error::{CoreError, split_hint};
use crate::session::{Session, Step};
use crate::{commands, diff};

/// Most steps in one batch.
pub const MAX_BATCH: usize = 1000;

type P = Map<String, Value>;

fn params(spec: &CommandSpec, message: impl Into<String>) -> CoreError {
    CoreError::Params {
        command: spec.id.to_string(),
        message: message.into(),
    }
}

/// The batch's steps as (id, params): each an object with `id` (or `command`, as fixtures write
/// it) and an optional `params` object.
fn steps(spec: &CommandSpec, p: &P) -> Result<Vec<(String, Value)>, CoreError> {
    let list = p
        .get("commands")
        .and_then(Value::as_array)
        .ok_or_else(|| params(spec, "`commands` must be a list of {id, params}"))?;
    if list.len() > MAX_BATCH {
        return Err(params(spec, format!("more than {MAX_BATCH} commands")));
    }
    list.iter()
        .enumerate()
        .map(|(i, s)| {
            let o = s
                .as_object()
                .ok_or_else(|| params(spec, format!("command {i} is not an object {{id, params}}")))?;
            if let Some(k) = o
                .keys()
                .find(|k| !["id", "command", "params"].contains(&k.as_str()))
            {
                return Err(params(
                    spec,
                    format!("command {i}: unknown key {k:?} (id, params)"),
                ));
            }
            let id = o
                .get("id")
                .or_else(|| o.get("command"))
                .and_then(Value::as_str)
                .ok_or_else(|| params(spec, format!("command {i} has no `id`")))?;
            let args = o.get("params").cloned().unwrap_or(json!({}));
            if !(args.is_object() || args.is_null()) {
                return Err(params(
                    spec,
                    format!("command {i} ({id}): `params` must be an object"),
                ));
            }
            Ok((id.to_string(), args))
        })
        .collect()
}

impl Session {
    /// Refuse a step a batch or a dry run cannot hold: an unknown id, or the session's own.
    fn nestable(&self, spec: &CommandSpec, i: usize, id: &str) -> Result<&'static CommandSpec, CoreError> {
        let inner = self.find(id).map_err(|e| CoreError::Step {
            command: spec.id.to_string(),
            step: i,
            id: id.to_string(),
            error: Box::new(e),
            note: "nothing was run".into(),
        })?;
        if commands::is_session_own(inner) {
            return Err(params(
                spec,
                format!("command {i} ({id}) acts on the session itself; run it on its own"),
            ));
        }
        Ok(inner)
    }

    pub(crate) fn batch(&mut self, spec: &CommandSpec, p: &P) -> Result<Value, CoreError> {
        let list = steps(spec, p)?;
        let found = list
            .iter()
            .enumerate()
            .map(|(i, (id, _))| self.nestable(spec, i, id))
            .collect::<Result<Vec<_>, _>>()?;
        let atomic = p.get("atomic").and_then(Value::as_bool).unwrap_or(true);
        let label = text(p, "label").map_or_else(
            || match found.as_slice() {
                [one] => one.label.to_string(),
                _ => format!("{} commands", list.len()),
            },
            str::to_string,
        );
        self.end_gesture();
        let before = self.document.clone();
        let step = Step {
            id: spec.id.to_string(),
            label: label.clone(),
        };
        self.gesture = Some((step, before.clone()));
        let mut results = Vec::with_capacity(list.len());
        for (i, (id, args)) in list.iter().enumerate() {
            match self.run(id, args) {
                Ok(v) => results.push(v),
                Err(e) if atomic => {
                    self.gesture = None;
                    self.restore(before);
                    let wrote: Vec<&str> = found.iter().take(i).filter(|s| s.writes).map(|s| s.id).collect();
                    let note = if wrote.is_empty() {
                        "the batch was rolled back: the document is as it was".to_string()
                    } else {
                        format!(
                            "the batch was rolled back: the document is as it was, but the files {wrote:?} wrote stay written"
                        )
                    };
                    return Err(CoreError::Step {
                        command: spec.id.to_string(),
                        step: i,
                        id: id.clone(),
                        error: Box::new(e),
                        note,
                    });
                }
                Err(e) => {
                    self.end_gesture();
                    return Ok(json!({
                        "results": results,
                        "done": i,
                        "label": label,
                        "failed": { "step": i, "id": id, "error": e.to_json() },
                        "changed": diff::summary(&before, &self.document),
                    }));
                }
            }
        }
        self.end_gesture();
        Ok(json!({
            "results": results,
            "done": list.len(),
            "label": label,
            "changed": diff::summary(&before, &self.document),
        }))
    }

    pub(crate) fn dry_run(&mut self, spec: &CommandSpec, p: &P) -> Result<Value, CoreError> {
        let id = text(p, "id").ok_or_else(|| params(spec, "id missing"))?;
        let inner = self.nestable(spec, 0, id)?;
        if inner.writes {
            return Err(CoreError::Refused {
                command: spec.id.to_string(),
                message: format!("{id} writes files, which a dry run cannot hold back"),
                hint: Some(format!("run {id} itself")),
            });
        }
        let args = p.get("params").cloned().unwrap_or(json!({}));
        let checked = check(inner, &args).map_err(|message| CoreError::Params {
            command: id.to_string(),
            message,
        })?;
        let mut doc = self.document.clone();
        let outcome = catch_unwind(AssertUnwindSafe(|| (inner.run)(&mut doc, &checked)));
        let result = match outcome {
            Ok(Ok(v)) => v,
            Ok(Err(message)) => {
                let (message, hint) = split_hint(&message);
                return Err(CoreError::Refused {
                    command: id.to_string(),
                    message,
                    hint,
                });
            }
            Err(_) => {
                return Err(CoreError::Panicked {
                    command: id.to_string(),
                    message: "it panicked on a copy; the document was not touched".into(),
                });
            }
        };
        let changed = diff::summary(&self.document, &doc);
        let undo_label =
            (inner.undoable && doc != self.document).then(|| inner.step_label(&checked, &result));
        Ok(json!({ "id": id, "result": result, "changed": changed, "undo_label": undo_label }))
    }
}
