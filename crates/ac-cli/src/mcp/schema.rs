//! The registry as agents read it: each command's inputs and outputs as JSON Schema, a ranked
//! search over ids, labels and docs, and a full description with an example call (taken from a
//! fixture that uses the command when there is one).

use ac_core::command::{MAX_ITEMS, MAX_TEXT};
use ac_core::{CommandSpec, Form, Kind, ParamSpec};
use serde_json::{Map, Value, json};

fn numbers(n: usize, what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": n, "maxItems": n, "description": what })
}

/// One kind as JSON Schema. `bones`: the rig's bone names, listed as the allowed values of a bone
/// input (none: just text).
pub fn kind_schema(k: Kind, bones: Option<&[String]>) -> Value {
    match k {
        Kind::Number => json!({ "type": "number" }),
        Kind::Integer => json!({ "type": "integer" }),
        Kind::Bool => json!({ "type": "boolean" }),
        Kind::Text => json!({ "type": "string", "minLength": 1, "maxLength": MAX_TEXT }),
        Kind::Bone => match bones {
            Some(b) => json!({ "type": "string", "enum": b }),
            None => {
                json!({ "type": "string", "minLength": 1, "description": "a bone name (resource ac://skeleton lists them)" })
            }
        },
        Kind::Choice(w) => json!({ "type": "string", "enum": w }),
        Kind::Vec3 => numbers(3, "[x, y, z]"),
        Kind::Quat => numbers(
            4,
            "[x, y, z, w]; normalised on the way in, a zero quaternion is refused",
        ),
        Kind::TextList => {
            json!({ "type": "array", "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT }, "maxItems": MAX_ITEMS })
        }
        Kind::IntList => json!({ "type": "array", "items": { "type": "integer" }, "maxItems": MAX_ITEMS }),
        Kind::Points => {
            json!({ "type": "array", "items": numbers(2, "[x, y] metres on the floor"), "maxItems": MAX_ITEMS })
        }
        Kind::Offsets => json!({
            "type": "object",
            "additionalProperties": { "oneOf": [numbers(2, "[dx, dy] metres"), { "type": "null" }] },
        }),
    }
}

/// The default an input's doc names ("key it (default true)", "(default: 6)"), when it is a value
/// of the input's kind; prose defaults ("default: the current one") stay in the description.
pub fn default_of(p: &ParamSpec) -> Option<Value> {
    let doc = p.doc;
    let at = doc.find("default")?;
    let rest = doc.get(at + "default".len()..)?.trim_start_matches([':', ' ']);
    let word: String = rest
        .chars()
        .take_while(|c| !matches!(c, ')' | ',' | ';' | ' '))
        .collect();
    let word = word.trim_matches(['"', '\'']);
    match p.kind {
        Kind::Bool => match word {
            "true" | "on" => Some(json!(true)),
            "false" | "off" => Some(json!(false)),
            _ => None,
        },
        Kind::Number => word
            .parse::<f64>()
            .ok()
            .filter(|x| x.is_finite())
            .map(|x| json!(x)),
        Kind::Integer => word.parse::<i64>().ok().map(|x| json!(x)),
        Kind::Choice(w) => w.contains(&word).then(|| json!(word)),
        _ => None,
    }
}

/// A free-form JSON value (`ParamSpec::json`) as JSON Schema, from the shape its doc gives: a doc
/// that opens with `[` is an array, with `{` (or that says "an object") an object; any other is
/// any value (the doc says what).
fn json_schema(doc: &str) -> Value {
    let d = doc.trim_start();
    if d.starts_with('[') {
        json!({ "type": "array" })
    } else if d.starts_with('{') || d.contains("(an object)") {
        json!({ "type": "object" })
    } else {
        json!({})
    }
}

/// One input (or output) as a JSON Schema property: kind and form (one value, a list of them, or
/// free-form JSON), range, description, default.
pub fn param_schema(p: &ParamSpec, bones: Option<&[String]>) -> Value {
    let mut item = kind_schema(p.kind, bones);
    if let (Some(m), Some((lo, hi)), Form::One | Form::List) = (item.as_object_mut(), p.range, p.form) {
        let bound = |x: f64| {
            if p.kind == Kind::Integer && x.fract() == 0.0 && x.abs() < 1e15 {
                json!(x as i64)
            } else {
                json!(x)
            }
        };
        m.insert("minimum".into(), bound(lo));
        m.insert("maximum".into(), bound(hi));
    }
    let mut s = match p.form {
        Form::One => item,
        Form::List => {
            let extra = item.get("description").cloned();
            if let Some(m) = item.as_object_mut() {
                m.remove("description");
            }
            let mut list = json!({ "type": "array", "items": item, "maxItems": MAX_ITEMS });
            if let Some(e) = extra {
                list["description"] = e;
            }
            list
        }
        Form::Json => json_schema(p.doc),
    };
    if let Some(m) = s.as_object_mut() {
        let mut doc = p.doc.to_string();
        if let Some(extra) = m.get("description").and_then(Value::as_str) {
            doc = format!("{doc} ({extra})");
        }
        m.insert("description".into(), json!(doc));
        if let (Form::One, Some(d)) = (p.form, default_of(p)) {
            m.insert("default".into(), d);
        }
    }
    s
}

/// A command's inputs as a JSON Schema object (unknown keys are refused, as `Session::run` does).
pub fn input_schema(c: &CommandSpec, bones: Option<&[String]>) -> Value {
    let props: Map<String, Value> = c
        .inputs
        .iter()
        .map(|p| (p.name.to_string(), param_schema(p, bones)))
        .collect();
    let required: Vec<&str> = c.inputs.iter().filter(|p| p.required).map(|p| p.name).collect();
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

/// A command's documented result as a JSON Schema object (`null` when it documents none). The
/// session checks every result against it in debug builds and fixtures; any output may be `null`
/// ("none"), and an undoable command's result also carries `changed`, the change summary.
pub fn output_schema(c: &CommandSpec) -> Value {
    if c.outputs.is_empty() {
        return Value::Null;
    }
    let props: Map<String, Value> = c
        .outputs
        .iter()
        .map(|p| (p.name.to_string(), param_schema(p, None)))
        .collect();
    let required: Vec<&str> = c.outputs.iter().filter(|p| p.required).map(|p| p.name).collect();
    json!({ "type": "object", "properties": props, "required": required })
}

/// One input or output in `command_list`'s compact entry: the kind reads with its form ("list of
/// [x, y, z]", "JSON").
fn brief(p: &ParamSpec, input: bool) -> Value {
    let mut v = json!({ "name": p.name, "kind": crate::cmdref::shape_name(p), "doc": p.doc });
    if input {
        v["required"] = json!(p.required);
        v["range"] = json!(p.range.map(|r| [r.0, r.1]));
    }
    v
}

/// `command_list`'s compact entry; `schema` adds the JSON Schemas.
pub fn entry(c: &CommandSpec, schema: bool) -> Value {
    let mut e = json!({
        "id": c.id, "label": c.label, "undoable": c.undoable, "shortcut": c.shortcut,
        "inputs": c.inputs.iter().map(|p| brief(p, true)).collect::<Vec<_>>(),
        "outputs": c.outputs.iter().map(|p| brief(p, false)).collect::<Vec<_>>(),
    });
    if c.writes {
        e["writes"] = json!(true);
    }
    if schema {
        e["input_schema"] = input_schema(c, None);
        e["output_schema"] = output_schema(c);
    }
    e
}

/// Lower-case words of a query (ids split at dots and underscores too).
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// A word's stem for loose matching: "sliding" and "slides" both find "slide".
fn stem(w: &str) -> &str {
    for end in ["ing", "es", "ed", "s"] {
        if let Some(s) = w.strip_suffix(end)
            && s.len() >= 3
        {
            return s;
        }
    }
    w
}

/// How well `c` matches `query`, and where. Zero: not at all.
pub fn score(c: &CommandSpec, query: &str) -> (u32, Vec<String>) {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0, Vec::new());
    }
    let mut total = 0;
    let mut hits = Vec::new();
    let mut hit = |n: u32, what: String| {
        total += n;
        if !hits.contains(&what) {
            hits.push(what);
        }
    };
    if c.id == q {
        hit(100, "id".into());
    } else if c.id.contains(&q) {
        hit(40, "id".into());
    }
    let id_words = words(c.id);
    // the verb itself ("travel" is `layer.travel`, not `layer.travel_preview`)
    let verb = c.id.rsplit('.').next().unwrap_or(c.id);
    let label = c.label.to_lowercase();
    for w in words(&q) {
        let s = stem(&w);
        if verb == w {
            hit(40, "id".into());
        } else if id_words.contains(&w) {
            hit(30, "id".into());
        } else if c.id.contains(s) {
            hit(15, "id".into());
        }
        if words(&label).contains(&w) {
            hit(15, "label".into());
        } else if label.contains(s) {
            hit(8, "label".into());
        }
        for p in c.inputs {
            if p.name == w {
                hit(8, format!("input {}", p.name));
            }
            if p.doc.to_lowercase().contains(s) {
                hit(4, format!("input {}", p.name));
            }
            if let Kind::Choice(opts) = p.kind
                && opts.iter().any(|o| o.contains(s))
            {
                hit(4, format!("input {}", p.name));
            }
        }
        for p in c.outputs {
            if p.name == w || p.doc.to_lowercase().contains(s) {
                hit(2, format!("output {}", p.name));
            }
        }
    }
    (total, hits)
}

/// Commands matching `query`, best first (registry order among equals), each with its score and
/// where it matched.
pub fn search(specs: &[&'static CommandSpec], query: &str, schema: bool) -> Vec<Value> {
    let mut found: Vec<(u32, usize, Vec<String>)> = specs
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let (s, hits) = score(c, query);
            (s > 0).then_some((s, i, hits))
        })
        .collect();
    found.sort_by_key(|(s, i, _)| (std::cmp::Reverse(*s), *i));
    found
        .into_iter()
        .filter_map(|(s, i, hits)| {
            let mut e = entry(specs.get(i)?, schema);
            e["score"] = json!(s);
            e["matched"] = json!(hits);
            Some(e)
        })
        .collect()
}

/// Ids close to an unknown one, for "did you mean": near spellings first, then search matches.
pub fn suggest(specs: &[&'static CommandSpec], id: &str) -> Vec<String> {
    let mut out = ac_core::suggest::closest(id, specs.iter().map(|c| c.id));
    let id = id.to_lowercase();
    let mut by_score: Vec<(u32, &'static str)> = specs
        .iter()
        .map(|c| (score(c, &id.replace(['.', '_'], " ")).0, c.id))
        .filter(|(s, _)| *s > 0)
        .collect();
    by_score.sort_by_key(|(s, id)| (std::cmp::Reverse(*s), *id));
    for (_, c) in by_score {
        if !out.iter().any(|o| o == c) {
            out.push(c.to_string());
        }
    }
    out.truncate(5);
    out
}

/// A plausible value for an input, for a made-up example.
fn sample(p: &ParamSpec, bones: &[String]) -> Value {
    let num = |int: bool| {
        let x = p.range.map_or(0.0, |(lo, hi)| 0.0_f64.clamp(lo, hi));
        if int { json!(x.round() as i64) } else { json!(x) }
    };
    match p.kind {
        Kind::Number => num(false),
        Kind::Integer => num(true),
        Kind::Bool => json!(true),
        Kind::Text => json!("text"),
        Kind::Bone => {
            let b = bones
                .iter()
                .find(|b| b.as_str() == "wrist.L")
                .or_else(|| bones.first());
            json!(b.map_or("root", |s| s.as_str()))
        }
        Kind::Choice(w) => json!(w.first().copied().unwrap_or_default()),
        Kind::Vec3 => json!([0.0, 0.0, 0.0]),
        Kind::Quat => json!([0.0, 0.0, 0.0, 1.0]),
        Kind::TextList => json!(["Left arm"]),
        Kind::IntList => json!([0, 20]),
        Kind::Points => json!([[0.0, 0.0], [1.0, 0.0]]),
        Kind::Offsets => json!({}),
    }
}

/// An example call of `c`: the first fixture step that runs it and checks something (else any
/// that runs it), with `{tmp}`/`{assets}` filled in; else one made from the schema.
pub fn example(
    c: &CommandSpec,
    fixtures: &[(String, Value)],
    bones: &[String],
    fill: &dyn Fn(&Value) -> Value,
) -> Value {
    let mut first: Option<Value> = None;
    for (name, f) in fixtures {
        for step in f["script"].as_array().into_iter().flatten() {
            if step["command"] != c.id {
                continue;
            }
            let found = json!({
                "fixture": name,
                "arguments": { "id": c.id, "params": fill(step.get("params").unwrap_or(&json!({}))) },
                "expect": step.get("expect"),
            });
            if step.get("expect").is_some() {
                return found;
            }
            first.get_or_insert(found);
        }
    }
    first.unwrap_or_else(|| {
        let params: Map<String, Value> = c
            .inputs
            .iter()
            .filter(|p| p.required)
            .map(|p| (p.name.to_string(), sample(p, bones)))
            .collect();
        json!({ "fixture": null, "made_up": true, "arguments": { "id": c.id, "params": params } })
    })
}

/// `describe_command`: everything about one command.
pub fn describe(c: &CommandSpec, bones: &[String], example: Value) -> Value {
    json!({
        "id": c.id,
        "label": c.label,
        "undoable": c.undoable,
        "tool": c.tool,
        "shortcut": c.shortcut,
        "undo_label": crate::cmdref::undo_label(c),
        "writes": c.writes,
        "input_schema": input_schema(c, Some(bones)),
        "output_schema": output_schema(c),
        "example": example,
        "run_with": "command_run {id, params} (dry_run: true tries it without keeping it)",
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn defaults_are_read_from_the_doc_only_when_they_are_values() {
        let b = ParamSpec::new("key", Kind::Bool, "key it (default true)");
        assert_eq!(default_of(&b), Some(json!(true)));
        let n = ParamSpec::new("count", Kind::Integer, "how many (default: 6)");
        assert_eq!(default_of(&n), Some(json!(6)));
        let prose = ParamSpec::new("frame", Kind::Number, "the frame (default: the current one)");
        assert_eq!(default_of(&prose), None);
        let c = ParamSpec::new(
            "mode",
            Kind::Choice(&["ai", "plain"]),
            "\"ai\" (the model, default)",
        );
        assert_eq!(default_of(&c), None);
    }

    #[test]
    fn ranges_become_minimum_and_maximum() {
        let p = ParamSpec::new("fade", Kind::Integer, "frames").range(0.0, 600.0);
        let s = param_schema(&p, None);
        assert_eq!(s["type"], "integer");
        assert_eq!(s["minimum"], 0);
        assert_eq!(s["maximum"], 600);
        assert_eq!(s["description"], "frames");
    }

    #[test]
    fn stems_match_loosely() {
        assert_eq!(stem("sliding"), "slid");
        assert_eq!(stem("keys"), "key");
        assert_eq!(stem("is"), "is");
    }
}
