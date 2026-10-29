//! A command's result against its declared `outputs`: the contract an agent reads from
//! `command_list`. The session checks every result in debug and test builds
//! ([`crate::Session::check_outputs`]), so a result that drifts from its declaration is a failing
//! test, not a surprise for whoever reads it.
//!
//! The rules: the result is a JSON object; every key it has is declared (or is one the session
//! adds itself, [`SESSION_KEYS`]); every declared `required` output is present; every value is of
//! its declared kind and form. `null` stands for "none" and is accepted for any output.

use serde_json::Value;

use crate::command::{CommandSpec, Form, Kind};

/// Keys the session adds to results itself (the change summary of an undoable command).
pub const SESSION_KEYS: &[&str] = &["changed"];

/// Whether `result` keeps `spec`'s declared outputs; the message says where it does not.
pub fn check(spec: &CommandSpec, result: &Value) -> Result<(), String> {
    let obj = result
        .as_object()
        .ok_or_else(|| format!("the result is {}, not an object", kind_of(result)))?;
    for (k, v) in obj {
        if SESSION_KEYS.contains(&k.as_str()) && spec.undoable {
            continue;
        }
        let Some(p) = spec.outputs.iter().find(|p| p.name == k.as_str()) else {
            let declared: Vec<&str> = spec.outputs.iter().map(|p| p.name).collect();
            return Err(format!("undeclared output {k:?} (declared: {declared:?})"));
        };
        if v.is_null() {
            continue;
        }
        let ok = match p.form {
            Form::Json => true,
            Form::One => value_ok(p.kind, v),
            Form::List => v
                .as_array()
                .is_some_and(|a| a.iter().all(|x| x.is_null() || value_ok(p.kind, x))),
        };
        if !ok {
            return Err(format!(
                "output {k:?} is {}, declared {}",
                short(v),
                describe(p.kind, p.form)
            ));
        }
    }
    if let Some(p) = spec
        .outputs
        .iter()
        .find(|p| p.required && !obj.contains_key(p.name))
    {
        return Err(format!("required output {:?} is missing", p.name));
    }
    Ok(())
}

/// Whether one value is of `kind` (results are lenient where inputs are strict: an empty text is
/// a text).
fn value_ok(kind: Kind, v: &Value) -> bool {
    let nums = |n: usize| {
        v.as_array()
            .is_some_and(|a| a.len() == n && a.iter().all(Value::is_number))
    };
    match kind {
        Kind::Number => v.is_number(),
        Kind::Integer => is_whole(v),
        Kind::Bool => v.is_boolean(),
        Kind::Text | Kind::Bone => v.is_string(),
        Kind::Choice(words) => v.as_str().is_some_and(|s| words.contains(&s)),
        Kind::Vec3 => nums(3),
        Kind::Quat => nums(4),
        Kind::TextList => v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
        Kind::IntList => v.as_array().is_some_and(|a| a.iter().all(is_whole)),
        Kind::Points => v.as_array().is_some_and(|a| {
            a.iter().all(|p| {
                p.as_array()
                    .is_some_and(|p| p.len() == 2 && p.iter().all(Value::is_number))
            })
        }),
        Kind::Offsets => v.is_object(),
    }
}

fn is_whole(v: &Value) -> bool {
    v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|x| x.fract() == 0.0)
}

/// How a declared output reads in a message ("a list of [x, y, z]").
pub fn describe(kind: Kind, form: Form) -> String {
    let one = match kind {
        Kind::Number => "a number".to_string(),
        Kind::Integer => "a whole number".into(),
        Kind::Bool => "true or false".into(),
        Kind::Text => "text".into(),
        Kind::Bone => "a bone name".into(),
        Kind::Choice(w) => format!("one of {w:?}"),
        Kind::Vec3 => "[x, y, z]".into(),
        Kind::Quat => "[x, y, z, w]".into(),
        Kind::TextList => "a list of text".into(),
        Kind::IntList => "a list of whole numbers".into(),
        Kind::Points => "a list of [x, y]".into(),
        Kind::Offsets => "an object".into(),
    };
    match form {
        Form::One => one,
        Form::List => format!("a list of {one}"),
        Form::Json => "JSON".into(),
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// A value, cut short for a message.
fn short(v: &Value) -> String {
    let s = v.to_string();
    match s.char_indices().nth(80) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::ParamSpec;
    use crate::document::Document;
    use serde_json::{Map, json};

    fn run(_: &mut Document, _: &Map<String, Value>) -> Result<Value, String> {
        Ok(json!({}))
    }

    static OUT: &[ParamSpec] = &[
        ParamSpec::new("n", Kind::Integer, "").required(),
        ParamSpec::new("at", Kind::Vec3, "").list(),
        ParamSpec::new("any", Kind::Text, "").json(),
    ];

    #[test]
    fn declared_keys_kinds_and_forms() {
        let spec = CommandSpec::new("t.t", "T", run).outputs(OUT);
        assert!(check(&spec, &json!({ "n": 3, "at": [[0, 0, 1]], "any": [{}] })).is_ok());
        assert!(check(&spec, &json!({ "n": null })).is_ok());
        assert!(check(&spec, &json!({ "at": [] })).is_err(), "n is required");
        assert!(check(&spec, &json!({ "n": 1.5 })).is_err());
        assert!(check(&spec, &json!({ "n": 1, "at": [[0, 0]] })).is_err());
        assert!(check(&spec, &json!({ "n": 1, "other": 0 })).is_err());
        assert!(check(&spec, &json!([1])).is_err());
        // the session's own summary rides on undoable results only
        assert!(check(&spec, &json!({ "n": 1, "changed": {} })).is_err());
        let undoable = CommandSpec::new("t.t", "T", run).outputs(OUT).undoable();
        assert!(check(&undoable, &json!({ "n": 1, "changed": {} })).is_ok());
    }
}
