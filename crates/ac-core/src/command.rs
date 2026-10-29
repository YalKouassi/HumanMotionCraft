//! Commands: everything a person or an agent can do is a [`CommandSpec`].
//!
//! The UI, shortcuts, scripts, fixtures, the CLI and MCP all run the same specs through
//! [`crate::Session::run`]. Inputs are declared, so they are checked before `run` and the extremes
//! test can try every one of them.

use serde_json::{Map, Value};

use crate::document::Document;

/// Longest text input accepted (bone names, labels, paths).
pub const MAX_TEXT: usize = 4096;
/// Most items in an array input.
pub const MAX_ITEMS: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Number,
    Integer,
    Bool,
    Text,
    /// Text naming a bone of the document's rig (checked by the command).
    Bone,
    /// One of the listed words.
    Choice(&'static [&'static str]),
    /// `[x, y, z]`.
    Vec3,
    /// `[x, y, z, w]`, normalised on the way in; a zero quaternion is refused.
    Quat,
    /// A list of non-empty texts (body parts, bone names).
    TextList,
    /// A list of whole numbers (frames).
    IntList,
    /// A list of `[x, y]` points (metres on the floor).
    Points,
    /// An object of names to `[dx, dy]` (or null: none).
    Offsets,
}

/// How a value is laid out around its [`Kind`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Form {
    /// One value of the kind.
    One,
    /// A list of values of the kind.
    List,
    /// Free-form JSON (an object, a list of objects): the doc says its shape and the command checks
    /// it. The kind is not used.
    Json,
}

#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
    /// Inclusive bounds for `Number` and `Integer`.
    pub range: Option<(f64, f64)>,
    pub doc: &'static str,
    /// One value, a list of them, or free-form JSON.
    pub form: Form,
}

impl ParamSpec {
    pub const fn new(name: &'static str, kind: Kind, doc: &'static str) -> ParamSpec {
        ParamSpec {
            name,
            kind,
            required: false,
            range: None,
            doc,
            form: Form::One,
        }
    }

    /// A list of values of the kind.
    pub const fn list(mut self) -> ParamSpec {
        self.form = Form::List;
        self
    }

    /// Free-form JSON; `doc` gives its shape and the command checks it.
    pub const fn json(mut self) -> ParamSpec {
        self.form = Form::Json;
        self
    }

    pub const fn required(mut self) -> ParamSpec {
        self.required = true;
        self
    }

    pub const fn range(mut self, lo: f64, hi: f64) -> ParamSpec {
        self.range = Some((lo, hi));
        self
    }
}

/// A command's `run`: the checked parameters in, a JSON result out, or a refusal message.
/// A refusal must leave the document unchanged (the session restores it anyway).
pub type RunFn = fn(&mut Document, &Map<String, Value>) -> Result<Value, String>;

/// How an undo step made by a command reads, from the command's label, its checked parameters and
/// its result ("AI fill 1–29", "Move key 20→30").
pub type LabelFn = fn(&'static str, &Map<String, Value>, &Value) -> String;

pub struct CommandSpec {
    /// Our own dotted lower-case id, `area.verb`.
    pub id: &'static str,
    pub label: &'static str,
    /// Icon name, drawn in code by the UI.
    pub icon: &'static str,
    pub shortcut: Option<&'static str>,
    /// A toolbar click starts an interactive tool (which then runs this command).
    pub tool: bool,
    /// Changes the document and goes on the undo stack.
    pub undoable: bool,
    pub inputs: &'static [ParamSpec],
    /// What it returns (documented; part of its contract).
    pub outputs: &'static [ParamSpec],
    pub run: RunFn,
    /// How its undo step reads (`None`: its label).
    pub undo_label: Option<LabelFn>,
    /// Writes files outside the document (the library, an export): neither undo nor a batch's
    /// rollback takes that back, and a dry run refuses it.
    pub writes: bool,
}

impl CommandSpec {
    /// A query (not undoable, no inputs); chain the builders below for the rest.
    pub const fn new(id: &'static str, label: &'static str, run: RunFn) -> CommandSpec {
        CommandSpec {
            id,
            label,
            icon: "",
            shortcut: None,
            tool: false,
            undoable: false,
            inputs: &[],
            outputs: &[],
            run,
            undo_label: None,
            writes: false,
        }
    }

    /// Name its undo steps with `f` instead of the plain label.
    pub const fn undo_label(mut self, f: LabelFn) -> CommandSpec {
        self.undo_label = Some(f);
        self
    }

    /// It writes files outside the document.
    pub const fn writes(mut self) -> CommandSpec {
        self.writes = true;
        self
    }

    /// The undo step's name for one run.
    pub fn step_label(&self, params: &Map<String, Value>, result: &Value) -> String {
        match self.undo_label {
            Some(f) => f(self.label, params, result),
            None => self.label.to_string(),
        }
    }

    pub const fn undoable(mut self) -> CommandSpec {
        self.undoable = true;
        self
    }

    pub const fn icon(mut self, icon: &'static str) -> CommandSpec {
        self.icon = icon;
        self
    }

    pub const fn shortcut(mut self, shortcut: &'static str) -> CommandSpec {
        self.shortcut = Some(shortcut);
        self
    }

    pub const fn tool(mut self) -> CommandSpec {
        self.tool = true;
        self
    }

    pub const fn inputs(mut self, inputs: &'static [ParamSpec]) -> CommandSpec {
        self.inputs = inputs;
        self
    }

    pub const fn outputs(mut self, outputs: &'static [ParamSpec]) -> CommandSpec {
        self.outputs = outputs;
        self
    }
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandSpec")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// The parameters checked against `spec.inputs`: an object, no unknown keys, every required input
/// present, every value of its kind and in range. Returns them with quaternions normalised.
pub fn check(spec: &CommandSpec, params: &Value) -> Result<Map<String, Value>, String> {
    let empty = Map::new();
    let obj = match params {
        Value::Object(m) => m,
        Value::Null => &empty,
        other => {
            return Err(format!(
                "parameters must be a JSON object, got {}",
                kind_of(other)
            ));
        }
    };
    if let Some(k) = obj
        .keys()
        .find(|k| !spec.inputs.iter().any(|p| p.name == k.as_str()))
    {
        let known: Vec<&str> = spec.inputs.iter().map(|p| p.name).collect();
        let near = crate::suggest::closest(k, known.iter().copied());
        let hint = if near.is_empty() {
            String::new()
        } else {
            format!("; {}", crate::suggest::did_you_mean(&near))
        };
        return Err(format!(
            "unknown parameter {k:?}{hint}; expected one of {known:?}"
        ));
    }
    let mut out = Map::new();
    for p in spec.inputs {
        match obj.get(p.name) {
            None | Some(Value::Null) if p.required => {
                return Err(format!("missing required parameter {:?} ({})", p.name, p.doc));
            }
            None | Some(Value::Null) => {}
            Some(v) => {
                let v = check_value(p, v).map_err(|e| format!("parameter {:?}: {e}", p.name))?;
                out.insert(p.name.to_string(), v);
            }
        }
    }
    Ok(out)
}

fn check_value(p: &ParamSpec, v: &Value) -> Result<Value, String> {
    match p.form {
        Form::One => check_kind(p, v),
        Form::List => {
            let a = v
                .as_array()
                .ok_or_else(|| format!("expected a list, got {}", kind_of(v)))?;
            if a.len() > MAX_ITEMS {
                return Err(format!("more than {MAX_ITEMS} items"));
            }
            a.iter()
                .enumerate()
                .map(|(i, x)| check_kind(p, x).map_err(|e| format!("item {i}: {e}")))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        Form::Json => {
            let n = json_size(v);
            if n > MAX_ITEMS {
                return Err(format!("more than {MAX_ITEMS} values"));
            }
            Ok(v.clone())
        }
    }
}

/// How many values `v` holds, counting nested ones (stops counting past [`MAX_ITEMS`]).
fn json_size(v: &Value) -> usize {
    let mut n = 0;
    let mut stack = vec![v];
    while let Some(x) = stack.pop() {
        n += 1;
        if n > MAX_ITEMS {
            break;
        }
        match x {
            Value::Array(a) => stack.extend(a.iter()),
            Value::Object(m) => stack.extend(m.values()),
            Value::String(s) if s.len() > MAX_TEXT * 16 => n += MAX_ITEMS,
            _ => {}
        }
    }
    n
}

fn check_kind(p: &ParamSpec, v: &Value) -> Result<Value, String> {
    match p.kind {
        Kind::Number | Kind::Integer => {
            let x = v
                .as_f64()
                .ok_or_else(|| format!("expected a number, got {}", kind_of(v)))?;
            if !x.is_finite() {
                return Err(format!("{x} is not a finite number"));
            }
            if p.kind == Kind::Integer && x.fract() != 0.0 {
                return Err(format!("expected a whole number, got {x}"));
            }
            if let Some((lo, hi)) = p.range
                && !(lo..=hi).contains(&x)
            {
                return Err(format!("{x} is outside {lo}..={hi}"));
            }
            Ok(v.clone())
        }
        Kind::Bool => v
            .as_bool()
            .map(Value::Bool)
            .ok_or_else(|| format!("expected true or false, got {}", kind_of(v))),
        Kind::Text | Kind::Bone => {
            let s = v
                .as_str()
                .ok_or_else(|| format!("expected text, got {}", kind_of(v)))?;
            if s.is_empty() {
                return Err("must not be empty".into());
            }
            if s.len() > MAX_TEXT {
                return Err(format!("longer than {MAX_TEXT} bytes"));
            }
            Ok(v.clone())
        }
        Kind::Choice(words) => {
            let s = v
                .as_str()
                .ok_or_else(|| format!("expected one of {words:?}, got {}", kind_of(v)))?;
            if words.contains(&s) {
                Ok(v.clone())
            } else {
                Err(format!("{s:?} is not one of {words:?}"))
            }
        }
        Kind::Vec3 => Ok(numbers(v, 3)?.into()),
        Kind::TextList => {
            let a = v
                .as_array()
                .ok_or_else(|| format!("expected a list of text, got {}", kind_of(v)))?;
            if a.len() > MAX_ITEMS {
                return Err(format!("more than {MAX_ITEMS} items"));
            }
            for x in a {
                match x.as_str() {
                    Some(s) if !s.is_empty() && s.len() <= MAX_TEXT => {}
                    _ => return Err(format!("expected non-empty text items, got {x}")),
                }
            }
            Ok(v.clone())
        }
        Kind::IntList => {
            let a = v
                .as_array()
                .ok_or_else(|| format!("expected a list of whole numbers, got {}", kind_of(v)))?;
            if a.len() > MAX_ITEMS {
                return Err(format!("more than {MAX_ITEMS} items"));
            }
            for x in a {
                if x.as_i64().is_none() {
                    return Err(format!("expected whole numbers, got {x}"));
                }
            }
            Ok(v.clone())
        }
        Kind::Points => {
            let a = v
                .as_array()
                .ok_or_else(|| format!("expected a list of [x, y], got {}", kind_of(v)))?;
            if a.len() > MAX_ITEMS {
                return Err(format!("more than {MAX_ITEMS} items"));
            }
            let pts = a.iter().map(|x| numbers(x, 2)).collect::<Result<Vec<_>, _>>()?;
            if pts.iter().flatten().any(|x| x.abs() > 1e4) {
                return Err("a point is more than 10 km away".into());
            }
            Ok(pts.into())
        }
        Kind::Offsets => {
            let m = v
                .as_object()
                .ok_or_else(|| format!("expected an object of name: [dx, dy], got {}", kind_of(v)))?;
            if m.len() > MAX_ITEMS {
                return Err(format!("more than {MAX_ITEMS} items"));
            }
            let mut out = Map::new();
            for (k, x) in m {
                if x.is_null() {
                    continue;
                }
                let d = numbers(x, 2).map_err(|e| format!("{k:?}: {e}"))?;
                if d.iter().any(|x| x.abs() > 100.0) {
                    return Err(format!("{k:?}: more than 100 m"));
                }
                out.insert(k.clone(), d.into());
            }
            Ok(Value::Object(out))
        }
        Kind::Quat => {
            let q = numbers(v, 4)?;
            let n = q.iter().map(|x| x * x).sum::<f64>().sqrt();
            if !(n.is_finite() && n > 1e-12) {
                return Err("a rotation quaternion must not be zero".into());
            }
            Ok(q.iter().map(|x| x / n).collect::<Vec<f64>>().into())
        }
    }
}

fn numbers(v: &Value, len: usize) -> Result<Vec<f64>, String> {
    let a = v
        .as_array()
        .ok_or_else(|| format!("expected {len} numbers, got {}", kind_of(v)))?;
    if a.len() != len {
        return Err(format!("expected {len} numbers, got {}", a.len().min(MAX_ITEMS)));
    }
    a.iter()
        .map(|x| match x.as_f64() {
            Some(f) if f.is_finite() => Ok(f),
            _ => Err(format!("expected finite numbers, got {x}")),
        })
        .collect()
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

/// Read a checked `Vec3` parameter.
pub fn vec3(p: &Map<String, Value>, name: &str) -> Option<[f64; 3]> {
    let a = p.get(name)?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

/// Read a checked `Quat` parameter.
pub fn quat(p: &Map<String, Value>, name: &str) -> Option<[f64; 4]> {
    let a = p.get(name)?.as_array()?;
    Some([
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2)?.as_f64()?,
        a.get(3)?.as_f64()?,
    ])
}

/// Read a checked `TextList` parameter (empty when absent).
pub fn texts(p: &Map<String, Value>, name: &str) -> Vec<String> {
    p.get(name)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Read a checked text parameter.
pub fn text<'a>(p: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    p.get(name).and_then(Value::as_str)
}
