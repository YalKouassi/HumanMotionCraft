//! A command's form, built from its typed inputs: numbers (with their ranges), whole numbers,
//! on/off, text, bone names (from the rig), choices, vectors, lists. Optional inputs start unset;
//! the form sends only what's set, and the session checks the rest (a refusal shows under the
//! form, under the command's title). Opened from the palette, the menus (commands with required
//! inputs) and `--ui window=form:<id>`.

use ac_core::CommandSpec;
use ac_core::command::{Kind, ParamSpec};
use egui::RichText;
use serde_json::{Map, Value, json};

use super::super::App;
use super::super::ui_ids::Tag;

/// A field's value while it's edited.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Num(f64),
    Bool(bool),
    /// Text, a bone name or a choice.
    Text(String),
    Vec(Vec<f64>),
    /// A list or JSON as typed.
    Raw(String),
}

#[derive(Clone, Debug)]
pub struct Field {
    pub spec: ParamSpec,
    /// Sent with the command (always for required inputs).
    pub on: bool,
    pub val: Val,
}

/// What a fresh form starts from.
#[derive(Clone, Debug, Default)]
pub struct Defaults {
    pub frame: f64,
    pub bone: Option<String>,
    pub fps: f64,
    pub clip_id: Option<String>,
}

pub struct Form {
    pub id: &'static str,
    pub label: &'static str,
    pub fields: Vec<Field>,
    /// The last run: its result (pretty JSON) or the refusal.
    pub result: Option<Result<String, String>>,
}

/// A field's starting value: what `given` has, else a guess from its name and the editor.
fn initial(p: &ParamSpec, given: Option<&Value>, d: &Defaults) -> Field {
    let frame_like = matches!(p.name, "frame" | "from" | "to" | "at" | "start" | "end");
    let clamp = |x: f64| match p.range {
        Some((lo, hi)) => x.clamp(lo, hi),
        None => x,
    };
    let guess = match p.kind {
        Kind::Number | Kind::Integer => {
            let x = if frame_like {
                d.frame.round()
            } else if p.name == "fps" {
                d.fps
            } else {
                match p.range {
                    Some((lo, hi)) if lo <= 0.0 && hi >= 0.0 => 0.0,
                    Some((lo, _)) => lo,
                    None => 0.0,
                }
            };
            Val::Num(clamp(x))
        }
        Kind::Bool => Val::Bool(false),
        Kind::Bone => Val::Text(d.bone.clone().unwrap_or_default()),
        Kind::Choice(words) => Val::Text(words.first().map(|w| w.to_string()).unwrap_or_default()),
        Kind::Text if p.name == "id" => Val::Text(d.clip_id.clone().unwrap_or_default()),
        Kind::Text => Val::Text(String::new()),
        Kind::Vec3 => Val::Vec(vec![0.0; 3]),
        Kind::Quat => Val::Vec(vec![0.0, 0.0, 0.0, 1.0]),
        Kind::TextList | Kind::IntList | Kind::Points | Kind::Offsets => Val::Raw(String::new()),
    };
    let from_given = given.filter(|v| !v.is_null()).and_then(|v| match (&guess, v) {
        (Val::Num(_), Value::Number(n)) => n.as_f64().map(Val::Num),
        (Val::Bool(_), Value::Bool(b)) => Some(Val::Bool(*b)),
        (Val::Text(_), Value::String(s)) => Some(Val::Text(s.clone())),
        (Val::Vec(_), Value::Array(a)) => a
            .iter()
            .map(Value::as_f64)
            .collect::<Option<Vec<f64>>>()
            .map(Val::Vec),
        (Val::Raw(_), Value::Array(a)) if matches!(p.kind, Kind::TextList | Kind::IntList) => Some(Val::Raw(
            a.iter()
                .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
                .collect::<Vec<_>>()
                .join(", "),
        )),
        (Val::Raw(_), other) => Some(Val::Raw(other.to_string())),
        _ => None,
    });
    Field {
        spec: *p,
        on: p.required || from_given.is_some(),
        val: from_given.unwrap_or(guess),
    }
}

impl Form {
    pub fn new(spec: &'static CommandSpec, given: &Value, d: &Defaults) -> Form {
        Form {
            id: spec.id,
            label: spec.label,
            fields: spec
                .inputs
                .iter()
                .map(|p| initial(p, given.get(p.name), d))
                .collect(),
            result: None,
        }
    }

    /// The parameters to send: the fields that are on. Text that doesn't read as its kind is
    /// refused here, by name; everything else the session checks.
    pub fn params(&self) -> Result<Value, String> {
        let mut out = Map::new();
        for f in &self.fields {
            if !f.on {
                continue;
            }
            let name = f.spec.name;
            let v = match (&f.val, f.spec.kind) {
                (Val::Num(x), _) if !x.is_finite() => return Err(format!("{name}: not a number")),
                (Val::Num(x), Kind::Integer) => json!(x.round() as i64),
                (Val::Num(x), _) => json!(x),
                (Val::Bool(b), _) => json!(b),
                (Val::Text(s), _) if s.trim().is_empty() => {
                    if f.spec.required {
                        return Err(format!("{name} is needed ({})", f.spec.doc));
                    }
                    continue;
                }
                (Val::Text(s), _) => json!(s.trim()),
                (Val::Vec(v), _) => json!(v),
                (Val::Raw(s), Kind::TextList) => json!(
                    s.split(',')
                        .map(str::trim)
                        .filter(|x| !x.is_empty())
                        .collect::<Vec<_>>()
                ),
                (Val::Raw(s), Kind::IntList) => {
                    let mut list = Vec::new();
                    for x in s.split(',').map(str::trim).filter(|x| !x.is_empty()) {
                        list.push(
                            x.parse::<i64>()
                                .map_err(|_| format!("{name}: {x:?} is not a whole number"))?,
                        );
                    }
                    json!(list)
                }
                (Val::Raw(s), _) if s.trim().is_empty() => {
                    if f.spec.required {
                        return Err(format!("{name} is needed ({})", f.spec.doc));
                    }
                    continue;
                }
                (Val::Raw(s), _) => {
                    serde_json::from_str::<Value>(s).map_err(|e| format!("{name}: not JSON ({e})"))?
                }
            };
            out.insert(name.to_string(), v);
        }
        Ok(Value::Object(out))
    }
}

/// The editor for one field.
fn field_ui(ui: &mut egui::Ui, f: &mut Field, bones: &[String]) {
    let p = f.spec;
    let id = format!("window.form.field.{}", p.name);
    match &mut f.val {
        Val::Num(x) => {
            let mut d = egui::DragValue::new(x);
            if let Some((lo, hi)) = p.range {
                d = d.range(lo..=hi).speed(((hi - lo) / 300.0).clamp(0.001, 1.0));
            } else {
                d = d.speed(if p.kind == Kind::Integer { 1.0 } else { 0.01 });
            }
            if p.kind == Kind::Integer {
                d = d.fixed_decimals(0);
            }
            ui.add(d).tag(id, p.name);
        }
        Val::Bool(b) => {
            ui.checkbox(b, "").tag(id, p.name);
        }
        Val::Text(s) => match p.kind {
            Kind::Choice(words) => {
                egui::ComboBox::from_id_salt(("form-choice", p.name))
                    .selected_text(s.as_str())
                    .show_ui(ui, |ui| {
                        for w in words {
                            ui.selectable_value(s, w.to_string(), *w)
                                .tag(format!("{id}.{w}"), *w);
                        }
                    })
                    .response
                    .tag(id, p.name);
            }
            Kind::Bone => {
                ui.add(egui::TextEdit::singleline(s).desired_width(160.0))
                    .tag(id.clone(), p.name);
                egui::ComboBox::from_id_salt(("form-bone", p.name))
                    .selected_text("pick")
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        for b in bones {
                            ui.selectable_value(s, b.clone(), b)
                                .tag(format!("{id}.pick.{b}"), b);
                        }
                    })
                    .response
                    .tag(format!("{id}.pick"), "pick");
            }
            _ => {
                ui.add(egui::TextEdit::singleline(s).desired_width(240.0))
                    .tag(id, p.name);
            }
        },
        Val::Vec(v) => {
            for (k, x) in v.iter_mut().enumerate() {
                ui.add(egui::DragValue::new(x).speed(0.01))
                    .tag(format!("{id}.{k}"), format!("{} {k}", p.name));
            }
        }
        Val::Raw(s) => {
            let hint = match p.kind {
                Kind::TextList => "comma-separated",
                Kind::IntList => "whole numbers, comma-separated",
                Kind::Points => "[[x, y], ...]",
                _ => "{\"name\": [dx, dy]}",
            };
            ui.add(egui::TextEdit::singleline(s).hint_text(hint).desired_width(240.0))
                .tag(id, p.name);
        }
    }
}

impl App {
    fn form_defaults(&self) -> Defaults {
        let doc = &self.session.document;
        Defaults {
            frame: doc.frame,
            bone: self.selected.and_then(|i| doc.skeleton.names.get(i)).cloned(),
            fps: self.shell.prefs.get().general.default_fps,
            clip_id: doc.clip_id.clone(),
        }
    }

    /// Open `spec`'s form, with `given` filled in.
    pub(in crate::app) fn open_form(&mut self, spec: &'static CommandSpec, given: &Value) {
        self.shell.form = Some(Form::new(spec, given, &self.form_defaults()));
    }

    pub(in crate::app) fn form_window(&mut self, ctx: &egui::Context) {
        let Some(form) = self.shell.form.as_mut() else {
            return;
        };
        let bones = &self.session.document.skeleton.names;
        let mut open = true;
        let mut run = false;
        egui::Window::new(form.label)
            .id(egui::Id::unique("command-form"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(form.id).weak().monospace());
                egui::Grid::new("form-grid")
                    .num_columns(3)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        for f in form.fields.iter_mut() {
                            if f.spec.required {
                                ui.label(RichText::new(format!("{} *", f.spec.name)).strong())
                                    .on_hover_text("required");
                            } else {
                                ui.checkbox(&mut f.on, f.spec.name)
                                    .on_hover_text("sent with the command when ticked")
                                    .tag(format!("window.form.on.{}", f.spec.name), f.spec.name);
                            }
                            ui.add_enabled_ui(f.on, |ui| {
                                ui.horizontal(|ui| field_ui(ui, f, bones));
                            });
                            let mut doc = f.spec.doc.to_string();
                            if let Some((lo, hi)) = f.spec.range {
                                doc.push_str(&format!(" ({lo} to {hi})"));
                            }
                            ui.label(RichText::new(doc).weak().small());
                            ui.end_row();
                        }
                    });
                if form.fields.is_empty() {
                    ui.label("No inputs.");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    run = ui.button("Run").tag("window.form.run", "Run").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                    ui.label(RichText::new("Ctrl+Enter").weak().small());
                });
                match &form.result {
                    Some(Ok(text)) => {
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            ui.label(RichText::new(text).monospace());
                        });
                    }
                    Some(Err(e)) => {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                    None => {}
                }
            });
        if run {
            self.run_form();
        }
        if !open {
            self.shell.form = None;
        }
    }

    /// Run the form's command through the session; the result or refusal shows under it.
    fn run_form(&mut self) {
        let Some(form) = self.shell.form.as_ref() else {
            return;
        };
        let (id, label) = (form.id, form.label);
        let result = match form.params() {
            Err(e) => Err(format!("{label}: {e}")),
            Ok(params) => {
                let ran = self.run_from_ui(id, &params);
                match ran {
                    Some(v) => {
                        let text = serde_json::to_string_pretty(&v).unwrap_or_default();
                        let short: String = text.chars().take(8000).collect();
                        Ok(if short.is_empty() { "done".into() } else { short })
                    }
                    None => Err(self.status.clone()),
                }
            }
        };
        if let Some(f) = self.shell.form.as_mut() {
            f.result = Some(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INPUTS: &[ParamSpec] = &[
        ParamSpec::new("frame", Kind::Integer, "a frame").required(),
        ParamSpec::new("weight", Kind::Number, "how much").range(0.5, 2.0),
        ParamSpec::new("bone", Kind::Bone, "a bone"),
        ParamSpec::new("interp", Kind::Choice(&["linear", "step"]), "how"),
        ParamSpec::new("parts", Kind::TextList, "parts"),
        ParamSpec::new("frames", Kind::IntList, "frames"),
        ParamSpec::new("to", Kind::Vec3, "where"),
    ];

    fn none(_: &mut ac_core::Document, _: &Map<String, Value>) -> Result<Value, String> {
        Ok(json!({}))
    }

    static SPEC: CommandSpec = CommandSpec::new("test.form", "Test form", none).inputs(INPUTS);

    #[test]
    fn a_fresh_form_sends_only_the_required_inputs() {
        let d = Defaults {
            frame: 12.4,
            bone: Some("hand.L".into()),
            fps: 30.0,
            clip_id: None,
        };
        let f = Form::new(&SPEC, &json!({}), &d);
        assert_eq!(f.params(), Ok(json!({ "frame": 12 })));
        // the optional ones start from sensible values
        assert_eq!(
            f.fields[1].val,
            Val::Num(0.5),
            "a range without 0 starts at its low end"
        );
        assert_eq!(f.fields[2].val, Val::Text("hand.L".into()), "the selected joint");
        assert_eq!(f.fields[3].val, Val::Text("linear".into()));
    }

    #[test]
    fn lists_and_given_values_convert() {
        let d = Defaults::default();
        let mut f = Form::new(
            &SPEC,
            &json!({ "parts": ["arm.L", "leg.R"], "to": [1, 2, 3] }),
            &d,
        );
        assert!(f.fields[4].on && f.fields[6].on, "given values are sent");
        assert_eq!(f.fields[4].val, Val::Raw("arm.L, leg.R".into()));
        f.fields[5].on = true;
        f.fields[5].val = Val::Raw("3, 7,, 9".into());
        assert_eq!(
            f.params(),
            Ok(
                json!({ "frame": 0, "parts": ["arm.L", "leg.R"], "frames": [3, 7, 9], "to": [1.0, 2.0, 3.0] })
            )
        );
        f.fields[5].val = Val::Raw("3, x".into());
        assert!(f.params().is_err_and(|e| e.contains("frames")));
        f.fields[0].val = Val::Num(f64::NAN);
        assert!(f.params().is_err());
    }

    #[test]
    fn a_required_text_left_empty_is_named() {
        static NEEDS: &[ParamSpec] = &[ParamSpec::new("path", Kind::Text, "where to write").required()];
        static S: CommandSpec = CommandSpec::new("test.path", "Write", none).inputs(NEEDS);
        let f = Form::new(&S, &json!({}), &Defaults::default());
        assert!(f.params().is_err_and(|e| e.contains("path is needed")));
    }
}
