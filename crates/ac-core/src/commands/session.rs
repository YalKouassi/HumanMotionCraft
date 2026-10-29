//! The session's own commands: their `run` is never called, [`crate::Session::run`] handles them
//! (undo history, gestures, batches and checkpoints live in the session, not the document; the
//! code is in `crate::history` and `crate::batch`). Listed here so the UI, the CLI and MCP see them
//! like any other command.

use serde_json::{Map, Value};

use crate::command::{CommandSpec, Kind, ParamSpec};
use crate::document::Document;

fn by_session(_: &mut Document, _: &Map<String, Value>) -> Result<Value, String> {
    Err("handled by the session; run it through Session::run".into())
}

/// A gesture's or a batch's undo step reads as its `label` parameter.
fn label_param(fallback: &'static str, p: &Map<String, Value>, _: &Value) -> String {
    p.get("label")
        .and_then(Value::as_str)
        .map_or_else(|| fallback.to_string(), str::to_string)
}

const STEP_OUT: &[ParamSpec] = &[
    ParamSpec::new("changed", Kind::Bool, "whether a step was undone or redone").required(),
    ParamSpec::new("note", Kind::Text, "why nothing changed"),
    ParamSpec::new("command", Kind::Text, "the command that made the step"),
    ParamSpec::new("label", Kind::Text, "the step's name"),
];

const LABEL: ParamSpec = ParamSpec::new("label", Kind::Text, "the undo step's name");

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("edit.undo", "Undo", by_session)
        .icon("undo")
        .shortcut("Ctrl+Z")
        .outputs(STEP_OUT),
    CommandSpec::new("edit.redo", "Redo", by_session)
        .icon("redo")
        .shortcut("Ctrl+Y")
        .outputs(STEP_OUT),
    CommandSpec::new("edit.begin", "Begin gesture", by_session)
        .inputs(&[LABEL])
        .undo_label(label_param),
    CommandSpec::new("edit.end", "End gesture", by_session).outputs(&[ParamSpec::new(
        "changed",
        Kind::Bool,
        "whether the gesture made an undo step",
    )
    .required()]),
    CommandSpec::new("edit.batch", "Run commands", by_session)
        .icon("batch")
        .undoable()
        .undo_label(label_param)
        .inputs(&[
            ParamSpec::new(
                "commands",
                Kind::Text,
                "[{id, params}, ...] run in order (at most 1000; not edit.* or session.*)",
            )
            .json()
            .required(),
            ParamSpec::new(
                "atomic",
                Kind::Bool,
                "true (default): a refusal puts everything back; false: keep the steps before it",
            ),
            LABEL,
        ])
        .outputs(&[
            ParamSpec::new("results", Kind::Text, "each step's result, in order")
                .json()
                .required(),
            ParamSpec::new("done", Kind::Integer, "steps that ran").required(),
            ParamSpec::new("label", Kind::Text, "the undo step's name").required(),
            ParamSpec::new(
                "failed",
                Kind::Text,
                "atomic false only: {step, id, error} of the step that was refused",
            )
            .json(),
        ]),
    CommandSpec::new("edit.dry_run", "Try a command", by_session)
        .icon("preview")
        .inputs(&[
            ParamSpec::new("id", Kind::Text, "the command to try").required(),
            ParamSpec::new("params", Kind::Text, "its parameters (an object)").json(),
        ])
        .outputs(&[
            ParamSpec::new("id", Kind::Text, "the command tried").required(),
            ParamSpec::new("result", Kind::Text, "what it would return")
                .json()
                .required(),
            ParamSpec::new("changed", Kind::Text, "what it would change (the change summary)")
                .json()
                .required(),
            ParamSpec::new(
                "undo_label",
                Kind::Text,
                "the undo step it would make (null: none)",
            )
            .required(),
        ]),
    CommandSpec::new("edit.history", "Undo history", by_session)
        .icon("history")
        .outputs(&[
            ParamSpec::new(
                "steps",
                Kind::Text,
                "[{index, label, command, done}] oldest first; done false: undone (redo brings it back)",
            )
            .json()
            .required(),
            ParamSpec::new(
                "current",
                Kind::Integer,
                "the index of the document shown (0: before the first step)",
            )
            .required(),
            ParamSpec::new("first", Kind::Integer, "the oldest index still reachable").required(),
            ParamSpec::new("checkpoints", Kind::Text, "[{name, index}]")
                .json()
                .required(),
            ParamSpec::new("gesture", Kind::Text, "the open gesture's label"),
        ]),
    CommandSpec::new("edit.checkpoint", "Name this point", by_session)
        .icon("flag")
        .inputs(&[ParamSpec::new(
            "name",
            Kind::Text,
            "the checkpoint's name (an existing one moves here)",
        )
        .required()])
        .outputs(&[
            ParamSpec::new("name", Kind::Text, "the checkpoint").required(),
            ParamSpec::new("index", Kind::Integer, "its history index").required(),
        ]),
    CommandSpec::new("edit.revert_to", "Go back to", by_session)
        .icon("history")
        .inputs(&[
            ParamSpec::new("name", Kind::Text, "a checkpoint"),
            ParamSpec::new("index", Kind::Integer, "or a history index (edit.history)").range(0.0, 1e9),
        ])
        .outputs(&[
            ParamSpec::new("from", Kind::Integer, "the index before").required(),
            ParamSpec::new("to", Kind::Integer, "the index now").required(),
            ParamSpec::new("undone", Kind::Integer, "steps undone").required(),
            ParamSpec::new("redone", Kind::Integer, "steps redone").required(),
        ]),
    CommandSpec::new("session.reset", "Reset session", by_session)
        .icon("reset")
        .outputs(&[
            ParamSpec::new("reset", Kind::Bool, "a fresh document; undo history cleared").required(),
            ParamSpec::new("dropped", Kind::Integer, "undo and redo steps cleared").required(),
        ]),
    CommandSpec::new("session.info", "Session info", by_session)
        .icon("info")
        .outputs(&[
            ParamSpec::new("product", Kind::Text, "the product's name").required(),
            ParamSpec::new("version", Kind::Text, "its version").required(),
            ParamSpec::new(
                "seed",
                Kind::Integer,
                "the random seed (null: nothing draws random numbers)",
            )
            .required(),
            ParamSpec::new(
                "deterministic",
                Kind::Bool,
                "same commands, same results (byte for byte)",
            )
            .required(),
            ParamSpec::new(
                "rig_hash",
                Kind::Text,
                "FNV-1a of the rig (names, parents, heads)",
            )
            .required(),
            ParamSpec::new(
                "limits_hash",
                Kind::Text,
                "FNV-1a of the joint limits (null: none loaded)",
            )
            .required(),
            ParamSpec::new(
                "model",
                Kind::Text,
                "{tag, hash} of the in-betweener (null: none loaded; hash null: it gives none)",
            )
            .json()
            .required(),
            ParamSpec::new("commands", Kind::Integer, "commands registered").required(),
            ParamSpec::new("history", Kind::Integer, "the history index shown").required(),
            ParamSpec::new(
                "check_outputs",
                Kind::Bool,
                "results are checked against their declared outputs",
            )
            .required(),
        ]),
];

/// Whether `spec` is one of the session's own commands (handled by the session, not `run`). By id:
/// ids are unique in a session (a second spec with one is refused), and these register first.
pub fn is_own(spec: &CommandSpec) -> bool {
    SPECS.iter().any(|s| s.id == spec.id)
}
