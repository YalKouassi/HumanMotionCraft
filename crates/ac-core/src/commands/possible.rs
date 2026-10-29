//! Bending impossible poses back: `clip.fix_impossible` (the work is in `crate::possible`;
//! `clip.possible` finds them).

use serde_json::{Map, Value};

use crate::command::{CommandSpec, Kind, ParamSpec};
use crate::document::Document;

type P = Map<String, Value>;
type Out = Result<Value, String>;

pub static SPECS: &[CommandSpec] = &[CommandSpec::new(
    "clip.fix_impossible",
    "Fix impossible poses",
    clip_fix_impossible,
)
.icon("graph")
.undoable()
.inputs(&[
    ParamSpec::new("from", Kind::Integer, "first frame (default 0)").range(0.0, 1e7),
    ParamSpec::new("to", Kind::Integer, "last frame (default: the last)").range(0.0, 1e7),
])
.outputs(&[
    ParamSpec::new(
        "keys",
        Kind::Integer,
        "keys of the edited layer between from and to that were checked",
    )
    .required(),
    ParamSpec::new(
        "fixed",
        Kind::Text,
        "[{frame, breaches: [{joint, component, value_deg, lo, hi, past_deg}]}]: keys bent back to the possible limit (the values before)",
    )
    .json()
    .required(),
    ParamSpec::new(
        "added",
        Kind::Text,
        "[frame]: frames between keys where the curve swung past a limit, given a key with the pose bent back",
    )
    .json()
    .required(),
    ParamSpec::new(
        "left",
        Kind::Integer,
        "frames still impossible after (a joint another layer sets); notes say which",
    )
    .required(),
    ParamSpec::new("notes", Kind::TextList, "what was left, and why").required(),
])];

fn clip_fix_impossible(doc: &mut Document, p: &P) -> Out {
    let int = |k: &str| p.get(k).and_then(Value::as_f64).map(|x| x as i64);
    doc.fix_impossible(int("from"), int("to"))
}
