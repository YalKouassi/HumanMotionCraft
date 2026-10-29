//! `expect`: fixture-style checks against the session as it is now, with the fixture runner's own
//! matcher (`crate::matches`: numbers within a tolerance, objects key by key, `$contains`). With
//! `save_as`, the steps this server ran so far and the checks become a new fixture file.
//!
//! A check: `{command?, params?, bone?, path?, value?, expect?, tolerance?}`. `command` defaults
//! to `editor.state`; `bone: "wrist.L"` is short for `bone.world {bone}`. `path` picks part of the
//! result (`keys.0.frame`, or a JSON pointer `/keys/0/frame`) and `value` is what it must be;
//! `expect` instead matches the whole result as a fixture step's `expect` does. Only queries run:
//! an undoable command is refused, so checking never changes the document.

use serde_json::{Map, Value, json};

use super::Server;
use super::preview::out_path;

/// Most checks in one call.
pub const MAX_CHECKS: usize = 200;

/// The last part of a query's id: these read the document and change nothing.
pub const QUERY_VERBS: &[&str] = &[
    "state", "get", "world", "info", "list", "sample", "curves", "contacts", "overlay", "solve", "metrics",
    "critique", "query", "history",
];

/// A command `expect` may run: not undoable, and named as a query.
pub fn is_query(c: &ac_core::CommandSpec) -> bool {
    !c.undoable
        && c.id
            .rsplit('.')
            .next()
            .is_some_and(|verb| QUERY_VERBS.contains(&verb))
}

/// The path's parts: `a.b.0` or `/a/b/0` (`~1` and `~0` unescape in a pointer).
fn parts(path: &str) -> Vec<String> {
    if path.is_empty() {
        return Vec::new();
    }
    match path.strip_prefix('/') {
        Some(p) => p
            .split('/')
            .map(|s| s.replace("~1", "/").replace("~0", "~"))
            .collect(),
        None => path.split('.').map(str::to_string).collect(),
    }
}

/// The value at `path`, and whether the path goes through a list on the way.
fn at<'a>(v: &'a Value, path: &[String]) -> Result<(&'a Value, bool), String> {
    let mut cur = v;
    let mut through_list = false;
    for (i, p) in path.iter().enumerate() {
        cur = match cur {
            Value::Object(m) => m.get(p),
            Value::Array(a) => {
                through_list = true;
                p.parse::<usize>().ok().and_then(|n| a.get(n))
            }
            _ => None,
        }
        .ok_or_else(|| {
            format!(
                "no {:?} in the result",
                path.get(..=i).unwrap_or_default().join(".")
            )
        })?;
    }
    Ok((cur, through_list))
}

/// What a fixture step expects for `value` at `path` of `actual`: `{a: {b: value}}` through
/// objects; a list on the way is kept whole (a step's list matches item for item) with `value` in
/// its place.
fn graft(actual: &Value, path: &[String], value: Value) -> Value {
    let Some((key, rest)) = path.split_first() else {
        return value;
    };
    match actual {
        Value::Array(a) => {
            let mut list = a.clone();
            if let Some(slot) = key.parse::<usize>().ok().and_then(|i| list.get_mut(i)) {
                *slot = graft(slot, rest, value);
            }
            Value::Array(list)
        }
        _ => {
            let mut m = Map::new();
            let inner = actual.get(key.as_str()).unwrap_or(&Value::Null);
            m.insert(key.clone(), graft(inner, rest, value));
            Value::Object(m)
        }
    }
}

impl Server {
    /// The `expect` tool.
    pub(super) fn expect(&mut self, args: &Value) -> Value {
        let tol = args["tolerance"]
            .as_f64()
            .filter(|t| t.is_finite() && *t >= 0.0)
            .unwrap_or(1e-6);
        let checks = args["checks"].as_array().map_or(&[][..], Vec::as_slice);
        if checks.is_empty() || checks.len() > MAX_CHECKS {
            return super::fail(format!(
                "checks: 1 to {MAX_CHECKS} checks, each {{command?, params?, bone?, path?, value? or expect?}}"
            ));
        }
        let mut results = Vec::new();
        let mut steps = Vec::new();
        let mut unsaveable = Vec::new();
        let mut failed = 0;
        for (i, c) in checks.iter().enumerate() {
            let (outcome, step) = self.check(c, tol);
            match step {
                Ok(s) => steps.push(s),
                Err(e) => unsaveable.push(format!("check {i}: {e}")),
            }
            if outcome["ok"] != true {
                failed += 1;
            }
            let mut o = outcome;
            o["check"] = json!(i);
            results.push(o);
        }
        let mut out = json!({ "ok": failed == 0, "passed": checks.len() - failed, "failed": failed, "results": results });
        if let Some(path) = args["save_as"].as_str() {
            out["saved"] = match self.save_fixture(path, args, tol, steps, &unsaveable) {
                Ok(mut v) if failed > 0 => {
                    if let Some(n) = v["notes"].as_array_mut() {
                        n.push(json!(format!("{failed} check(s) fail now: the fixture fails until the feature does what it expects")));
                    }
                    v
                }
                Ok(v) => v,
                Err(e) => json!({ "error": e }),
            };
        }
        super::text(&out, false)
    }

    /// One check: its outcome, and the fixture step that makes it (or why there is none).
    fn check(&mut self, c: &Value, tol: f64) -> (Value, Result<Value, String>) {
        let fail = |e: String| (json!({ "ok": false, "error": e.clone() }), Err(e));
        let (id, params) = match c["bone"].as_str() {
            Some(b) => ("bone.world".to_string(), json!({ "bone": b })),
            None => (
                c["command"].as_str().unwrap_or("editor.state").to_string(),
                c.get("params").cloned().unwrap_or(json!({})),
            ),
        };
        match self.specs.iter().find(|s| s.id == id) {
            None => return fail(format!("unknown command {id:?}")),
            Some(s) if !is_query(s) => {
                return fail(format!(
                    "{id} may change the document; expect only runs queries (ids ending in {}), so set things up with command_run first",
                    QUERY_VERBS.join(", ")
                ));
            }
            Some(_) => {}
        }
        let tol = c["tolerance"]
            .as_f64()
            .filter(|t| t.is_finite() && *t >= 0.0)
            .unwrap_or(tol);
        let result = match self.backend.run(&id, &params) {
            Ok(v) => v,
            Err(e) => return fail(format!("{id} was refused: {e}")),
        };
        let path = parts(c["path"].as_str().unwrap_or(""));
        let label = format!("{id} {}", path.join("."));
        let (expected, actual, through_list) = match (c.get("expect"), c.get("value")) {
            (Some(e), None) => (e.clone(), result.clone(), false),
            (None, Some(v)) => match at(&result, &path) {
                Ok((a, l)) => (v.clone(), a.clone(), l),
                Err(e) => return fail(e),
            },
            _ => return fail("give `value` (with `path`) or `expect`, not both or neither".into()),
        };
        let verdict = crate::matches(&actual, &expected, tol, label.trim());
        let outcome = match &verdict {
            Ok(()) => json!({ "ok": true, "command": id, "path": c["path"] }),
            Err(m) => json!({ "ok": false, "command": id, "path": c["path"], "error": m, "actual": actual }),
        };
        let mut step = json!({ "command": id, "params": params });
        step["expect"] = if c.get("expect").is_some() {
            expected
        } else {
            graft(&result, &path, expected)
        };
        if through_list {
            // a step's list matches item for item, so the items expect did not name come along
            step["pinned"] = json!(format!(
                "expect checked only {}; the other items of the lists on that path were copied from the session",
                path.join(".")
            ));
        }
        if c.get("tolerance").is_some() {
            step["tolerance"] = json!(tol);
        }
        (outcome, Ok(step))
    }

    fn save_fixture(
        &mut self,
        path: &str,
        args: &Value,
        tol: f64,
        checks: Vec<Value>,
        unsaveable: &[String],
    ) -> Result<Value, String> {
        if !unsaveable.is_empty() {
            return Err(format!("not saved: {}", unsaveable.join("; ")));
        }
        let file = out_path(path, "json")?;
        if file.exists() {
            return Err(format!(
                "{} exists; save_as writes a new fixture only",
                file.display()
            ));
        }
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "recorded".into());
        let name = args["name"].as_str().map_or(stem, str::to_string);
        let feature = match args["feature"].as_str() {
            Some(f) => f.to_string(),
            // the last edit, looking inside batches (command_batch runs as one edit.batch step)
            None => self
                .steps
                .iter()
                .rev()
                .flat_map(|s| match s["command"].as_str() {
                    Some("edit.batch") => s["params"]["commands"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .rev()
                        .filter_map(|c| c["id"].as_str().or_else(|| c["command"].as_str()))
                        .collect(),
                    other => other.into_iter().collect::<Vec<&str>>(),
                })
                .find(|id| *id != "edit.batch" && self.specs.iter().any(|c| c.id == *id && c.undoable))
                .unwrap_or_default()
                .to_string(),
        };
        let description = args["description"].as_str().map_or_else(
            || "Recorded with the MCP expect tool. The expected values are the agent's; work each out by hand and \
                say how here before relying on this fixture."
                .to_string(),
            str::to_string,
        );
        let mut script = self.steps.clone();
        let n_steps = script.len();
        script.extend(checks);
        let fixture = json!({
            "name": name,
            "feature": feature,
            "description": description,
            "script": script,
            "expect": { "tolerance": tol },
            "errors": [],
            "views": [],
        });
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text = serde_json::to_string_pretty(&fixture).map_err(|e| e.to_string())? + "\n";
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut notes = vec![
            "the fixture replays in a fresh session on a scratch copy of the built-in library: steps that use your own library items will not replay".to_string(),
            "add `errors` (refused inputs) before it counts as a feature's fixture".to_string(),
        ];
        if self.live {
            notes.push(
                "live: only the commands this server ran are in the script, not edits made in the window"
                    .into(),
            );
        }
        Ok(
            json!({ "path": file.to_string_lossy(), "steps": n_steps, "checks": script.len() - n_steps, "notes": notes }),
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn paths_pick_and_nest() {
        let v = json!({ "keys": [{ "frame": 3 }], "a": { "b": 2 } });
        assert_eq!(at(&v, &parts("a.b")).unwrap(), (&json!(2), false));
        assert_eq!(at(&v, &parts("/keys/0/frame")).unwrap(), (&json!(3), true));
        assert!(at(&v, &parts("keys.9")).is_err());
        assert!(at(&v, &parts("a.b.c")).is_err());
        assert_eq!(graft(&v, &parts("a.b"), json!(1)), json!({ "a": { "b": 1 } }));
        assert_eq!(graft(&v, &parts(""), json!(1)), json!(1));
        let p = json!({ "position": [1.0, 2.0, 3.0] });
        assert_eq!(
            graft(&p, &parts("position.2"), json!(9)),
            json!({ "position": [1.0, 2.0, 9] })
        );
    }
}
