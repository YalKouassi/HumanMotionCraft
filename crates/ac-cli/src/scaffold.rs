//! `ac new-command <area.verb>`: the start of a feature, the way `docs/llm-guidelines.md` asks for
//! one. It writes a command stub in its own file (in `ac-io` when the area already lives there,
//! else `ac-core`), a fixture skeleton with `errors` and `views`, and registers the file (one
//! `mod` line and one entry in the crate's list of command areas). The stub refuses with "not
//! supported yet", so the fixture fails until the command is written.

use std::path::{Path, PathBuf};

use ac_core::CommandSpec;
use serde_json::{Value, json};

/// Which crate a new command goes in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Owner {
    Core,
    Io,
}

impl Owner {
    pub fn parse(s: &str) -> Result<Owner, String> {
        match s {
            "core" | "ac-core" => Ok(Owner::Core),
            "io" | "ac-io" => Ok(Owner::Io),
            other => Err(format!("--crate {other:?}: core or io")),
        }
    }

    /// The crate whose commands already use `area`, else `ac-core`.
    pub fn of(area: &str) -> Owner {
        let in_io = ac_io::ALL
            .iter()
            .flat_map(|s| s.iter())
            .any(|c| c.id.split('.').next() == Some(area));
        if in_io { Owner::Io } else { Owner::Core }
    }

    /// (the folder the stub goes in, the file that registers it, the list it joins, the `mod`
    /// line's prefix, how the stub names ac-core).
    fn layout(
        self,
    ) -> (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    ) {
        match self {
            Owner::Core => (
                "crates/ac-core/src/commands",
                "crates/ac-core/src/commands/mod.rs",
                "pub static AREAS",
                "mod",
                "crate",
            ),
            Owner::Io => (
                "crates/ac-io/src",
                "crates/ac-io/src/lib.rs",
                "pub const ALL",
                "pub mod",
                "ac_core",
            ),
        }
    }
}

/// `area.verb`: dotted lower-case, letters, digits and `_`, each part starting with a letter.
pub fn check_id(id: &str) -> Result<(&str, &str), String> {
    let part_ok = |p: &str| {
        p.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    match id.split_once('.') {
        Some((a, v)) if part_ok(a) && part_ok(v) && id.len() <= 64 => Ok((a, v)),
        _ => Err(format!(
            "{id:?} is not an `area.verb` id (lower case, e.g. pose.mirror)"
        )),
    }
}

fn label_of(verb: &str) -> String {
    let words = verb.replace('_', " ");
    let mut c = words.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

fn stub(id: &str, label: &str, core: &str) -> String {
    format!(
        r#"//! `{id}`: say here what it does and why (one job per file, under 700 lines).

use serde_json::{{Map, Value}};

use {core}::command::{{CommandSpec, Kind, ParamSpec}};
use {core}::document::Document;

pub static SPECS: &[CommandSpec] = &[CommandSpec::new("{id}", "{label}", run)
    .undoable()
    .inputs(&[ParamSpec::new("amount", Kind::Number, "replace: what it is, and its unit").range(0.0, 1.0)])
    .outputs(&[ParamSpec::new("changed", Kind::Bool, "replace: what the result says")])];

/// Checked parameters in (`Session::run` checked them against `inputs`), a JSON result out, or a
/// refusal that says what was wrong. Never crash: no unwrap, no indexing on input.
fn run(_doc: &mut Document, _p: &Map<String, Value>) -> Result<Value, String> {{
    Err("not supported yet: {id}".into())
}}
"#
    )
}

fn fixture(id: &str, name: &str) -> Value {
    json!({
        "$schema": "./schema.json",
        "name": name,
        "feature": id,
        "description": "Replace: what a reviewer should see in the views, and how each expected number was worked out by hand.",
        "script": [
            { "command": id, "params": { "amount": 0.5 }, "expect": { "changed": true } }
        ],
        "expect": { "tolerance": 0.000001 },
        "errors": [
            { "command": id, "params": { "amount": 2 } }
        ],
        "views": ["front"]
    })
}

/// `text` with `item` added to the list that starts at `list` (before its closing `];`).
fn add_to_list(text: &str, list: &str, item: &str) -> Option<String> {
    let start = text.find(list)?;
    let end = start + text.get(start..)?.find("];")?;
    let before = text.get(..end)?.trim_end();
    let sep = if before.ends_with(',') || before.ends_with('[') {
        " "
    } else {
        ", "
    };
    Some(format!("{before}{sep}{item}{}", text.get(end..)?))
}

/// `text` with `line` after the last line starting with `prefix ` (or at the top).
fn add_line(text: &str, prefix: &str, line: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .rposition(|l| l.starts_with(&format!("{prefix} ")))
        .map_or(0, |i| i + 1);
    let mut out: Vec<&str> = lines.get(..at).map(<[&str]>::to_vec).unwrap_or_default();
    out.push(line);
    out.extend(lines.get(at..).unwrap_or_default());
    out.join("\n") + "\n"
}

/// Write the stub, the fixture skeleton and the registration under `repo`. Refuses an id that is
/// registered already or a file that exists (nothing is overwritten).
pub fn new_command(
    repo: &Path,
    id: &str,
    owner: Option<Owner>,
    label: Option<&str>,
    registered: &[&CommandSpec],
) -> Result<Value, String> {
    let (area, verb) = check_id(id)?;
    if registered.iter().any(|c| c.id == id) {
        return Err(format!("{id} is registered already (`ac commands {id}`)"));
    }
    let owner = owner.unwrap_or_else(|| Owner::of(area));
    let (dir, reg, list, mod_kw, core) = owner.layout();
    let module = format!("{area}_{verb}");
    let file = crate::repo_path(repo, dir).join(format!("{module}.rs"));
    let name = format!("{area}-{}", verb.replace('_', "-"));
    let fx = repo.join("fixtures").join(format!("{name}.json"));
    for p in [&file, &fx] {
        if p.exists() {
            return Err(format!("{} exists already", p.display()));
        }
    }
    let reg_path = crate::repo_path(repo, reg);
    let reg_text = std::fs::read_to_string(&reg_path).map_err(|e| format!("{}: {e}", reg_path.display()))?;
    let item = format!("{module}::SPECS");
    let mod_line = format!("{mod_kw} {module};");
    let registered_text = add_to_list(&reg_text, list, &item).map(|t| add_line(&t, mod_kw, &mod_line));

    let label = label.map_or_else(|| label_of(verb), str::to_string);
    let write =
        |p: &PathBuf, text: &str| std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()));
    write(&file, &stub(id, &label, core))?;
    let fx_text = serde_json::to_string_pretty(&fixture(id, &name)).map_err(|e| e.to_string())? + "\n";
    write(&fx, &fx_text)?;
    let mut next = vec![
        format!("write `run` in {}", file.display()),
        format!("work the numbers out by hand in {}", fx.display()),
        format!("ac fixtures --only {name}"),
        "ac commands --md (docs/commands.md lists every command)".into(),
        "cargo fmt --all".into(),
    ];
    match registered_text {
        Some(t) => write(&reg_path, &t)?,
        None => next.insert(
            0,
            format!(
                "register it by hand in {}: `{mod_line}` and `{item}` in `{list}`",
                reg_path.display()
            ),
        ),
    }
    Ok(json!({
        "command": id,
        "crate": if owner == Owner::Core { "ac-core" } else { "ac-io" },
        "stub": file.to_string_lossy(),
        "fixture": fx.to_string_lossy(),
        "registered": reg_path.to_string_lossy(),
        "next": next,
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn lists_and_mod_lines_take_the_new_area() {
        let core = "mod keys;\nmod pose;\n\nuse x;\n\npub static AREAS: &[&[CommandSpec]] = &[a::SPECS, b::SPECS];\n";
        let t = add_to_list(core, "pub static AREAS", "pose_mirror::SPECS").unwrap();
        assert!(t.contains("&[a::SPECS, b::SPECS, pose_mirror::SPECS];"));
        let t = add_line(&t, "mod", "mod pose_mirror;");
        assert!(t.starts_with("mod keys;\nmod pose;\nmod pose_mirror;\n\nuse x;"));
        let wrapped = "pub const ALL: &[&[X]] = &[\n    a::SPECS,\n    b::SPECS,\n];\n";
        let t = add_to_list(wrapped, "pub const ALL", "c::SPECS").unwrap();
        assert!(t.contains("    b::SPECS, c::SPECS];"));
    }

    #[test]
    fn ids_are_area_dot_verb() {
        assert_eq!(check_id("pose.mirror").unwrap(), ("pose", "mirror"));
        for bad in [
            "pose",
            "Pose.mirror",
            "pose.",
            ".x",
            "pose.mir ror",
            "a.b.c",
            "1a.b",
        ] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
        assert_eq!(label_of("set_bone"), "Set bone");
    }
}
