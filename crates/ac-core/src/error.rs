use serde_json::{Value, json};
use thiserror::Error;

/// Where a refusal message carries its fix: a command's `run` returns
/// `format!("{message}{HINT}{fix}")` (see [`hinted`]) and the session splits it into
/// [`CoreError::Refused`]'s `message` and `hint`.
pub const HINT: &str = "\nfix: ";

/// A refusal message with how to fix it ("the animation has no keys" + "pose the body and run
/// key.set first"), for a refusal whose cause is the document's state rather than the input.
pub fn hinted(message: impl std::fmt::Display, fix: impl std::fmt::Display) -> String {
    format!("{message}{HINT}{fix}")
}

/// A refusal message split into what failed and the fix it carries (if any).
pub fn split_hint(message: &str) -> (String, Option<String>) {
    match message.split_once(HINT) {
        Some((m, h)) if !h.trim().is_empty() => (m.to_string(), Some(h.trim().to_string())),
        Some((m, _)) => (m.to_string(), None),
        None => (message.to_string(), None),
    }
}

fn fix(hint: &Option<String>) -> String {
    hint.as_ref().map(|h| format!(" (fix: {h})")).unwrap_or_default()
}

fn did_you_mean(suggestions: &[String]) -> String {
    match suggestions {
        [] => String::new(),
        [one] => format!("; did you mean {one:?}?"),
        many => format!("; did you mean one of {many:?}?"),
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum CoreError {
    #[error("rig: {0}")]
    Rig(String),
    #[error("pose: {0}")]
    Pose(String),
    /// A command's parameters were refused before it ran.
    #[error("{command}: {message}")]
    Params { command: String, message: String },
    /// No command has this id; `suggestions` are the registered ids closest to it.
    #[error("unknown command {command:?}{}", did_you_mean(.suggestions))]
    UnknownCommand {
        command: String,
        suggestions: Vec<String>,
    },
    /// The command ran and refused (bad id, impossible request); the document is unchanged.
    /// `hint` says how to get past it when the cause is the document's state.
    #[error("{command}: {message}{}", fix(.hint))]
    Refused {
        command: String,
        message: String,
        hint: Option<String>,
    },
    /// The command panicked; the document was restored. Always a bug.
    #[error("{command} failed unexpectedly and was undone: {message}")]
    Panicked { command: String, message: String },
    /// One step of a batch failed (`step` counts from 0); `note` says what was kept.
    #[error("{command}: step {step} ({id}) failed: {error}; {note}")]
    Step {
        command: String,
        step: usize,
        id: String,
        error: Box<CoreError>,
        note: String,
    },
    /// The result breaks the command's declared outputs (checked in debug and test builds; a bug).
    #[error("{command}: its result breaks its declared outputs (a bug): {message}")]
    Outputs { command: String, message: String },
}

impl CoreError {
    /// A short machine name for the kind of failure.
    pub fn kind(&self) -> &'static str {
        match self {
            CoreError::Rig(_) => "rig",
            CoreError::Pose(_) => "pose",
            CoreError::Params { .. } => "params",
            CoreError::UnknownCommand { .. } => "unknown_command",
            CoreError::Refused { .. } => "refused",
            CoreError::Panicked { .. } => "panicked",
            CoreError::Step { .. } => "step",
            CoreError::Outputs { .. } => "outputs",
        }
    }

    /// The error as structured JSON (for MCP and scripts): `kind`, `message` (the whole text),
    /// and, when there are any, `command`, `hint`, `suggestions` and the failed `step`.
    pub fn to_json(&self) -> Value {
        let mut v = json!({ "kind": self.kind(), "message": self.to_string() });
        let Some(m) = v.as_object_mut() else {
            return v;
        };
        match self {
            CoreError::Rig(_) | CoreError::Pose(_) => {}
            CoreError::Params { command, .. }
            | CoreError::Panicked { command, .. }
            | CoreError::Outputs { command, .. } => {
                m.insert("command".into(), json!(command));
            }
            CoreError::UnknownCommand { command, suggestions } => {
                m.insert("command".into(), json!(command));
                m.insert("suggestions".into(), json!(suggestions));
            }
            CoreError::Refused { command, hint, .. } => {
                m.insert("command".into(), json!(command));
                if let Some(h) = hint {
                    m.insert("hint".into(), json!(h));
                }
            }
            CoreError::Step {
                command,
                step,
                id,
                error,
                note,
            } => {
                m.insert("command".into(), json!(command));
                m.insert("step".into(), json!(step));
                m.insert("id".into(), json!(id));
                m.insert("error".into(), error.to_json());
                m.insert("note".into(), json!(note));
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_split_and_show() {
        let (m, h) = split_hint(&hinted("no keys", "run key.set first"));
        assert_eq!((m.as_str(), h.as_deref()), ("no keys", Some("run key.set first")));
        assert_eq!(split_hint("plain").1, None);
        let e = CoreError::Refused {
            command: "clip.sample".into(),
            message: m,
            hint: h,
        };
        assert_eq!(e.to_string(), "clip.sample: no keys (fix: run key.set first)");
        assert_eq!(e.to_json()["hint"], "run key.set first");
        let u = CoreError::UnknownCommand {
            command: "key.sett".into(),
            suggestions: vec!["key.set".into()],
        };
        assert_eq!(
            u.to_string(),
            "unknown command \"key.sett\"; did you mean \"key.set\"?"
        );
        assert_eq!(u.to_json()["kind"], "unknown_command");
    }
}
