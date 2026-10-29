//! The command reference: `ac commands` (a summary of every command), `ac commands <id>` (one
//! command's whole definition) and `ac commands --md` (`docs/commands.md`, generated from the
//! registry; a test fails when it is stale).

use std::fmt::Write as _;

use ac_core::command::Form;
use ac_core::{CommandSpec, Kind, ParamSpec};
use serde_json::{Value, json};

/// Where `ac commands --md` writes, relative to the repo.
pub const DOC: &str = "docs/commands.md";

/// How a kind reads in the reference.
pub fn kind_name(k: Kind) -> String {
    match k {
        Kind::Number => "number".into(),
        Kind::Integer => "integer".into(),
        Kind::Bool => "bool".into(),
        Kind::Text => "text".into(),
        Kind::Bone => "bone".into(),
        Kind::Choice(words) => format!("one of {}", words.join(" | ")),
        Kind::Vec3 => "[x, y, z]".into(),
        Kind::Quat => "[x, y, z, w]".into(),
        Kind::TextList => "list of text".into(),
        Kind::IntList => "list of integers".into(),
        Kind::Points => "list of [x, y]".into(),
        Kind::Offsets => "{name: [dx, dy]}".into(),
    }
}

/// How a parameter's kind reads with its form: "integer", "list of [x, y, z]", "JSON".
pub fn shape_name(p: &ParamSpec) -> String {
    match p.form {
        Form::One => kind_name(p.kind),
        Form::List => format!("list of {}", kind_name(p.kind)),
        Form::Json => "JSON".into(),
    }
}

/// The form's name in the JSON definitions.
fn form_name(f: Form) -> &'static str {
    match f {
        Form::One => "one",
        Form::List => "list",
        Form::Json => "json",
    }
}

fn param(p: &ParamSpec, docs: bool) -> Value {
    let mut v = json!({ "name": p.name, "kind": shape_name(p), "required": p.required });
    if p.form != Form::One {
        v["form"] = json!(form_name(p.form));
    }
    if let Some((lo, hi)) = p.range {
        v["range"] = json!([lo, hi]);
    }
    if docs {
        v["doc"] = json!(p.doc);
    }
    v
}

/// One command's whole definition (`ac commands <id>`).
pub fn describe(c: &CommandSpec) -> Value {
    json!({
        "id": c.id,
        "label": c.label,
        "icon": c.icon,
        "shortcut": c.shortcut,
        "tool": c.tool,
        "undoable": c.undoable,
        "undo_label": undo_label(c),
        "writes": c.writes,
        "inputs": c.inputs.iter().map(|p| param(p, true)).collect::<Vec<_>>(),
        "outputs": c.outputs.iter().map(|p| param(p, true)).collect::<Vec<_>>(),
    })
}

/// The summary line of each command (`ac commands`): no docs, output names only.
pub fn summary(c: &CommandSpec) -> Value {
    json!({
        "id": c.id,
        "label": c.label,
        "shortcut": c.shortcut,
        "tool": c.tool,
        "undoable": c.undoable,
        "writes": c.writes,
        "inputs": c.inputs.iter().map(|p| param(p, false)).collect::<Vec<_>>(),
        "outputs": c.outputs.iter().map(|p| p.name).collect::<Vec<_>>(),
    })
}

/// How the undo step of an undoable command is named: its label, or a name made for each run
/// (`CommandSpec::undo_label`, e.g. "Move key 20→30"). `None` for commands that make no step.
pub fn undo_label(c: &CommandSpec) -> Option<&'static str> {
    match (c.undoable || c.undo_label.is_some(), c.undo_label.is_some()) {
        (false, _) => None,
        (true, false) => Some("label"),
        (true, true) => Some("per run"),
    }
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn number(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

fn table(out: &mut String, title: &str, params: &[ParamSpec]) -> std::fmt::Result {
    if params.is_empty() {
        return Ok(());
    }
    writeln!(
        out,
        "\n{title}:\n\n| Name | Kind | Required | Range | What |\n|---|---|---|---|---|"
    )?;
    for p in params {
        let range = p
            .range
            .map(|(lo, hi)| format!("{} to {}", number(lo), number(hi)))
            .unwrap_or_default();
        writeln!(
            out,
            "| `{}` | {} | {} | {} | {} |",
            p.name,
            cell(&shape_name(p)),
            if p.required { "yes" } else { "" },
            range,
            cell(p.doc)
        )?;
    }
    Ok(())
}

/// `docs/commands.md`: every command, sorted by id, grouped by area. Only the registry goes in, so
/// the same commands always give the same bytes.
pub fn markdown(commands: &[&CommandSpec]) -> Result<String, String> {
    let mut out = String::new();
    write_markdown(&mut out, commands).map_err(|e| format!("writing the command reference: {e}"))?;
    Ok(out)
}

fn write_markdown(out: &mut String, commands: &[&CommandSpec]) -> std::fmt::Result {
    let mut list: Vec<&CommandSpec> = commands.to_vec();
    list.sort_by_key(|c| c.id);
    out.push_str(
        "# Commands\n\n\
         Generated from the command registry by `ac commands --md`: do not edit by hand. A test\n\
         (`crates/ac-cli/tests/commands_doc.rs`) fails when it is stale. `ac commands <id>` prints\n\
         one command as JSON.\n\n\
         Every command runs through the session, from the UI, a script, a fixture, the CLI or MCP.\n\
         An undoable command makes one undo step, named by its label unless the command names it\n\
         for each run. A command that writes files writes outside the document: undo does not\n\
         take the files back. Kind \"JSON\" is free-form: its description gives the shape.\n",
    );
    let mut area = "";
    for c in list {
        let this = c.id.split('.').next().unwrap_or(c.id);
        if this != area {
            area = this;
            writeln!(out, "\n## {area}")?;
        }
        writeln!(out, "\n### `{}`: {}\n", c.id, c.label)?;
        let mut facts = vec![if c.undoable { "undoable" } else { "not undoable" }.to_string()];
        if undo_label(c) == Some("per run") {
            facts.push("the undo step is named for each run".into());
        }
        if c.writes {
            facts.push("writes files (undo and a batch rollback leave them; a dry run refuses it)".into());
        }
        if let Some(k) = c.shortcut {
            facts.push(format!("shortcut `{k}`"));
        }
        if c.tool {
            facts.push("interactive tool".into());
        }
        if !c.icon.is_empty() {
            facts.push(format!("icon `{}`", c.icon));
        }
        writeln!(out, "{}.", facts.join(", "))?;
        if c.inputs.is_empty() {
            out.push_str("\nNo inputs.\n");
        }
        table(out, "Inputs", c.inputs)?;
        table(out, "Outputs", c.outputs)?;
    }
    Ok(())
}

/// `ac commands [<id> | --md [<file>]]` over `commands`. Returns what to print (or, for `--md`,
/// the file written).
pub fn cli(
    commands: &[&'static CommandSpec],
    args: &[&str],
    repo: &std::path::Path,
) -> Result<Value, String> {
    match args {
        [] => Ok(Value::Array(commands.iter().map(|c| summary(c)).collect())),
        ["--md", rest @ ..] => {
            let path = match rest {
                [] => crate::repo_path(repo, DOC),
                [p] => std::path::PathBuf::from(p),
                _ => return Err("commands --md [<file>]".into()),
            };
            std::fs::write(&path, markdown(commands)?).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(json!({ "wrote": path.to_string_lossy(), "commands": commands.len() }))
        }
        [id] => commands
            .iter()
            .find(|c| c.id == *id)
            .map(|c| describe(c))
            .ok_or_else(|| format!("no command {id:?} (`ac commands` lists them)")),
        _ => Err("commands [<id> | --md [<file>]]".into()),
    }
}
