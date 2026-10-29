//! The fixture format as types (`docs/llm-guidelines.md`, "Fixture format"). The runner reads
//! fixtures as JSON, and every fixture is first checked against these types, so a misspelt key
//! (`"expcet"`) is an error instead of a check that silently never runs. `fixtures/schema.json` is
//! generated from them ([`schema`], `ac fixtures --schema`), with the registered command ids and
//! the view names filled in, so an editor can complete and check a fixture as it is written.

use std::collections::BTreeMap;

use ac_core::CommandSpec;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// Where the schema lives, relative to the repo; fixtures name it as `"$schema": "./schema.json"`.
pub const SCHEMA: &str = "fixtures/schema.json";

/// The named camera views a fixture can ask for (`ac_render::offscreen::view`).
pub const VIEWS: &[&str] = &["front", "back", "left", "right", "side", "top", "three-quarter"];

/// A feature fixture: a script of commands with hand-worked expectations, mistakes that must be
/// refused, and the views a person looks at.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Feature fixture")]
pub struct Fixture {
    /// The schema this file follows (for editors; the runner ignores it).
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    /// Unique, lower-case with dashes; `ac fixtures --only <name>` runs it.
    pub name: String,
    /// The command (or feature) it proves.
    pub feature: String,
    /// What a reviewer should see in the views, and how each expected number was worked out.
    pub description: String,
    /// Numbers copied from the program's own output: why that is acceptable here.
    #[serde(default)]
    pub pinned: Option<String>,
    /// The commands, run in order on a fresh session.
    pub script: Vec<Step>,
    /// Checks after the script.
    #[serde(default)]
    pub expect: Option<Expect>,
    /// Mistakes: each must be refused with a message and leave the document unchanged.
    #[serde(default)]
    pub errors: Vec<Mistake>,
    /// Camera views rendered for the gallery and the visual goldens.
    #[serde(default)]
    pub views: Vec<String>,
}

/// One command of the script.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// A registered command id (`ac commands`).
    pub command: String,
    /// Its parameters. `{tmp}` in a text becomes the fixture's scratch folder, `{assets}` the
    /// assets folder.
    #[serde(default)]
    pub params: Option<Map<String, Value>>,
    /// What its result must hold: numbers within the tolerance, objects key by key (keys not named
    /// are not checked), `{"$contains": [items]}` for an array holding matching items.
    #[serde(default)]
    pub expect: Option<Value>,
    /// This step's tolerance (default: the fixture's).
    #[serde(default)]
    pub tolerance: Option<f64>,
    /// Numbers copied from the program's own output: why.
    #[serde(default)]
    pub pinned: Option<String>,
}

/// Checks after the script.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// How far a number may be off (default 1e-6).
    #[serde(default)]
    pub tolerance: Option<f64>,
    /// Bone name to what `bone.world` must return for it.
    #[serde(default)]
    pub bones: Option<BTreeMap<String, Value>>,
    /// What `editor.state` must hold.
    #[serde(default)]
    pub state: Option<Value>,
}

/// A mistake that must be refused.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mistake {
    /// A registered command id.
    pub command: String,
    #[serde(default)]
    pub params: Option<Map<String, Value>>,
}

/// `fixture` follows the format (only the shape: commands and numbers are the runner's job).
pub fn check(fixture: &Value) -> Result<Fixture, String> {
    Fixture::deserialize(fixture).map_err(|e| format!("not a fixture: {e}"))
}

/// `fixtures/schema.json`: the types' schema, with the registered command ids (sorted) and the
/// views as the allowed values.
pub fn schema(commands: &[&CommandSpec]) -> Value {
    let mut s = schemars::schema_for!(Fixture).to_value();
    let mut ids: Vec<&str> = commands.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let defs = s.get_mut("$defs").and_then(Value::as_object_mut);
    for def in defs.into_iter().flat_map(|d| d.values_mut()) {
        if let Some(cmd) = def.pointer_mut("/properties/command") {
            cmd["enum"] = json!(ids);
        }
    }
    if let Some(items) = s.pointer_mut("/properties/views/items") {
        items["enum"] = json!(VIEWS);
    }
    // a script step's params follow its command's inputs (kinds, lists and JSON as declared,
    // ranges, required ones, no unknown keys); `errors` entries are mistakes on purpose, so not them
    if let Some(step) = s.pointer_mut("/$defs/Step") {
        let mut sorted: Vec<&CommandSpec> = commands.to_vec();
        sorted.sort_by_key(|c| c.id);
        step["allOf"] = sorted.iter().map(|c| step_rule(c)).collect();
    }
    s
}

/// How a command reads where a fixture names it: its label and what kind of step it makes.
fn command_note(c: &CommandSpec) -> String {
    let mut s = c.label.to_string();
    if c.undoable {
        s.push_str(if c.undo_label.is_some() {
            "; undoable (the undo step is named for each run)"
        } else {
            "; undoable"
        });
    }
    if c.writes {
        s.push_str("; writes files (undo does not take them back)");
    }
    s
}

/// The schema rule for a step running `c`: its params are `c`'s inputs.
fn step_rule(c: &CommandSpec) -> Value {
    json!({
        "if": { "properties": { "command": { "const": c.id } }, "required": ["command"] },
        "then": { "properties": {
            "command": { "description": command_note(c) },
            "params": crate::mcp::input_schema(c, None),
        } },
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn misspelt_keys_are_refused_and_schema_is_ignored() {
        let ok = json!({ "$schema": "./schema.json", "name": "a", "feature": "x.y", "description": "d",
            "script": [{ "command": "x.y", "params": {}, "expect": { "k": 1 } }] });
        assert!(check(&ok).is_ok());
        let typo = json!({ "name": "a", "feature": "x.y", "description": "d",
            "script": [{ "command": "x.y", "expcet": {} }] });
        assert!(check(&typo).unwrap_err().contains("expcet"));
    }

    #[test]
    fn every_view_renders() {
        for v in VIEWS {
            assert!(
                ac_render::offscreen::view(v, ac_render::glam::Vec3::ZERO).is_ok(),
                "{v}"
            );
        }
    }
}
