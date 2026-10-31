//! The layers panel: every layer (top first) with mute, solo, reorder and its name to pick; the
//! picked layer's name, weight, blend, travel replan, bake and delete; and travel between keys
//! with a preview of the path before it is added, refused in plain words when it can't run.

use ac_core::clip::{Clip, KeySource, LayerMode, LayerSource};
use serde_json::json;

use super::{gesture, gesture_end};
use crate::app::App;
use crate::app::ui_ids::Tag;

/// The panel's window state.
#[derive(Default)]
pub struct LayersState {
    /// The layer picked in the list (the edited one, unless a travel or link layer was picked).
    picked: Option<usize>,
    /// The name being typed (layer, text).
    name: Option<(usize, String)>,
    /// A previewed travel: the clip it was worked out for, its span and the hips' path.
    travel: Option<(Clip, i64, i64, Vec<[f64; 3]>)>,
    /// Why travel can't run, in words, from the last try.
    travel_note: Option<String>,
}

/// One row of the list, read from the document.
struct Row {
    name: String,
    muted: bool,
    source: LayerSource,
    keys: usize,
    span: Option<(f64, f64)>,
}

impl LayersState {
    /// Pick a layer in the list.
    pub fn pick(&mut self, i: usize) {
        self.picked = Some(i);
        self.name = None;
    }

    /// Drop the travel preview.
    pub fn clear_travel(&mut self) {
        self.travel = None;
        self.travel_note = None;
    }
}

impl App {
    /// The previewed travel path, while the animation is still the one it was worked out for.
    pub(in crate::app) fn travel_preview_path(&self) -> Option<&[[f64; 3]]> {
        match &self.panels.layers.travel {
            Some((clip, _, _, path)) if *clip == self.session.document.clip => Some(path),
            _ => None,
        }
    }

    pub(in crate::app) fn layers_panel(&mut self, ui: &mut egui::Ui) {
        let doc = &self.session.document;
        let rows: Vec<Row> = doc
            .clip
            .layers
            .iter()
            .map(|l| Row {
                name: l.name.clone(),
                muted: l.muted,
                source: l.source,
                keys: l.keys.len(),
                span: l.start.zip(l.end),
            })
            .collect();
        let editing = doc.clip.edit;
        let n = rows.len();
        let picked = self.panels.layers.picked.filter(|&i| i < n).unwrap_or(editing);
        // a layer soloed: the only one above the base that plays (0: every one above is muted)
        let playing: Vec<usize> = (1..n)
            .filter(|&i| rows.get(i).is_some_and(|r| !r.muted))
            .collect();
        let solo = match playing.as_slice() {
            [] if n > 1 => Some(0),
            [one] if n > 2 => Some(*one),
            _ => None,
        };
        for i in (0..n).rev() {
            let Some(row) = rows.get(i) else { continue };
            ui.horizontal_wrapped(|ui| {
                let mut on = !row.muted;
                if i > 0 {
                    if ui
                        .checkbox(&mut on, "")
                        .on_hover_text("Plays (untick to mute)")
                        .tag(format!("panel.layers.play.{i}"), "Plays")
                        .changed()
                    {
                        self.run("layer.set", json!({ "i": i, "muted": !on }));
                    }
                } else {
                    ui.add_space(ui.spacing().interact_size.y + ui.spacing().item_spacing.x);
                }
                let kind = match row.source {
                    LayerSource::Keys => format!("{} keys", row.keys),
                    LayerSource::Link => "link".into(),
                    LayerSource::Travel => "travel".into(),
                };
                // the layer keys go on reads in bold
                let mut name = egui::RichText::new(&row.name);
                if i == editing {
                    name = name.strong().underline();
                }
                if ui
                    .selectable_label(i == picked, name)
                    .on_hover_text(format!(
                        "{kind}; click to pick (a keys layer is the one keys go on)"
                    ))
                    .tag(format!("panel.layers.pick.{i}"), row.name.clone())
                    .clicked()
                {
                    self.panels.layers.picked = Some(i);
                    self.panels.layers.name = None;
                    if row.source == LayerSource::Keys && i != editing {
                        self.run("layer.edit", json!({ "i": i }));
                    }
                }
                let soloed = solo == Some(i);
                if ui
                    .selectable_label(soloed, "S")
                    .on_hover_text(if soloed {
                        "Solo off: every layer plays"
                    } else {
                        "Solo: mute every other layer above the base"
                    })
                    .tag(format!("panel.layers.solo.{i}"), "S")
                    .clicked()
                {
                    self.run("layer.solo", json!({ "i": i, "on": !soloed }));
                }
                if i > 0 {
                    if ui
                        .add_enabled(i + 1 < n, egui::Button::new("Up").small())
                        .on_hover_text("Move up")
                        .tag(format!("panel.layers.up.{i}"), "Up")
                        .clicked()
                    {
                        self.run("layer.move", json!({ "i": i, "to": i + 1 }));
                        self.panels.layers.picked = Some(i + 1);
                    }
                    if ui
                        .add_enabled(i > 1, egui::Button::new("Down").small())
                        .on_hover_text("Move down (the base stays at the bottom)")
                        .tag(format!("panel.layers.down.{i}"), "Down")
                        .clicked()
                    {
                        self.run("layer.move", json!({ "i": i, "to": i - 1 }));
                        self.panels.layers.picked = Some(i - 1);
                    }
                }
            });
        }
        if ui
            .button("+ Layer")
            .on_hover_text("A layer of keys over the others")
            .tag("panel.layers.add", "+ Layer")
            .clicked()
        {
            self.run("layer.add", json!({}));
            self.panels.layers.picked = None;
        }
        if ui
            .button("Secondary motion")
            .on_hover_text(
                "Arm swing, shoulders, pelvis, spine, head, hands and follow-through, worked out from the \
                 motion, on a layer of its own: mute it to get your animation back",
            )
            .tag("panel.layers.secondary", "Secondary motion")
            .clicked()
            && let Some(r) = self.run("layer.secondary", json!({}))
        {
            self.status = r["notes"]
                .as_array()
                .and_then(|n| n.last())
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .to_string();
            self.panels.layers.picked = None;
        }
        if let Some(row) = rows.get(picked) {
            ui.separator();
            self.layer_details(ui, picked, row);
        }
        ui.separator();
        self.travel_row(ui);
    }

    /// The picked layer's settings and actions.
    fn layer_details(&mut self, ui: &mut egui::Ui, i: usize, row: &Row) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Name");
            let text = match &mut self.panels.layers.name {
                Some((j, t)) if *j == i => t,
                slot => &mut slot.insert((i, row.name.clone())).1,
            };
            let r = ui.text_edit_singleline(text).tag("panel.layers.name", "Name");
            let typed = text.trim().to_string();
            if r.lost_focus() && !typed.is_empty() && typed != row.name {
                self.run("layer.set", json!({ "i": i, "name": typed }));
                self.panels.layers.name = None;
            }
        });
        let Some(lay) = self.session.document.clip.layers.get(i) else {
            return;
        };
        let (mut weight, mode, gait) = (lay.weight, lay.mode, lay.gait.clone());
        if i > 0 {
            let r = ui
                .add(egui::Slider::new(&mut weight, 0.0..=1.0).text("weight"))
                .tag("panel.layers.weight", "weight");
            if gesture(self, &r, "Layer weight") {
                self.run("layer.set", json!({ "i": i, "weight": weight }));
            }
            gesture_end(self, &r);
            ui.horizontal_wrapped(|ui| {
                for (m, label, tip) in [
                    (LayerMode::Override, "Override", "Replace what is below"),
                    (LayerMode::Additive, "Additive", "Add to what is below"),
                ] {
                    if ui
                        .selectable_label(mode == m, label)
                        .on_hover_text(tip)
                        .tag(format!("panel.layers.mode.{}", label.to_lowercase()), label)
                        .clicked()
                        && mode != m
                    {
                        let name = if m == LayerMode::Additive {
                            "additive"
                        } else {
                            "override"
                        };
                        self.run("layer.set", json!({ "i": i, "mode": name }));
                    }
                }
            });
        }
        if row.source == LayerSource::Travel {
            let (a, b) = row.span.unwrap_or((0.0, 0.0));
            ui.label(format!("Travel {a}–{b}, gait {gait}"));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Plan travel again")
                    .on_hover_text("Match the walk again to the keys around it (after you changed them)")
                    .tag("panel.layers.plan", "Plan travel again")
                    .clicked()
                {
                    self.run("layer.plan", json!({ "i": i }));
                }
                egui::ComboBox::from_id_salt("travel-gait")
                    .selected_text(gait.clone())
                    .show_ui(ui, |ui| {
                        for g in ["auto", "walk", "run", "crouch"] {
                            if ui
                                .selectable_label(gait == g, g)
                                .tag(format!("panel.layers.gait.{g}"), g)
                                .clicked()
                                && gait != g
                            {
                                self.run("layer.set", json!({ "i": i, "gait": g }));
                            }
                        }
                    })
                    .response
                    .tag("panel.layers.gait", gait.clone());
            });
        }
        if i > 0 {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Bake down")
                    .on_hover_text("Flatten this layer and the ones under it into the base keys")
                    .tag("panel.layers.bake", "Bake down")
                    .clicked()
                    && let Some(r) = self.run("layer.bake", json!({ "i": i }))
                {
                    self.status = format!("baked into {} base keys", r["baked"]);
                    self.panels.layers.picked = Some(0);
                }
                if ui
                    .button("Delete layer")
                    .tag("panel.layers.delete", "Delete layer")
                    .clicked()
                    && self.run("layer.delete", json!({ "i": i })).is_some()
                {
                    self.panels.layers.picked = None;
                }
            });
        }
    }

    /// Travel between the keys around the playhead: preview the path, then add it.
    fn travel_row(&mut self, ui: &mut egui::Ui) {
        let doc = &self.session.document;
        let f = doc.key_frame();
        let mine: Vec<i64> = doc
            .clip
            .base()
            .keys
            .iter()
            .filter(|k| k.source == KeySource::Manual)
            .map(|k| k.frame)
            .collect();
        let before = mine.iter().any(|&k| k <= f);
        let after = mine.iter().any(|&k| k > f);
        let refusal = match (before, after) {
            (true, true) => None,
            (false, false) => Some("needs a key on each side of the playhead (there are no base keys)"),
            (false, true) => Some("needs a key on each side of the playhead (none before it)"),
            (true, false) => Some("needs a key on each side of the playhead (none after it)"),
        };
        let previewed = self.travel_preview_path().is_some();
        ui.horizontal_wrapped(|ui| {
            let why = refusal.unwrap_or_default();
            if ui
                .add_enabled(refusal.is_none(), egui::Button::new("Preview travel"))
                .on_hover_text("Show where a walk between the keys around the playhead would go (orange), without adding it")
                .on_disabled_hover_text(why)
                .tag("panel.layers.travel_preview", "Preview travel")
                .clicked()
            {
                self.preview_travel();
            }
            if ui
                .add_enabled(refusal.is_none(), egui::Button::new("Travel between keys"))
                .on_hover_text("Walk between the keys around the playhead (a travel layer, one undo)")
                .on_disabled_hover_text(why)
                .tag("panel.layers.travel", "Travel between keys")
                .clicked()
            {
                match self.session.run("layer.travel", &json!({})) {
                    Ok(_) => {
                        self.panels.layers.travel = None;
                        self.panels.layers.travel_note = None;
                        self.status = "added a travel layer".into();
                    }
                    Err(e) => self.travel_refused(&e.to_string()),
                }
            }
            if previewed
                && ui
                    .small_button("Clear")
                    .tag("panel.layers.travel_clear", "Clear")
                    .clicked()
            {
                self.panels.layers.travel = None;
            }
        });
        if let Some(why) = refusal {
            ui.label(egui::RichText::new(format!("Travel {why}.")).small().weak());
        }
        if let Some((_, a, b, path)) = &self.panels.layers.travel
            && previewed
        {
            let metres = path
                .windows(2)
                .filter_map(|w| match w {
                    [p, q] => Some((q[0] - p[0]).hypot(q[1] - p[1])),
                    _ => None,
                })
                .sum::<f64>();
            ui.label(
                egui::RichText::new(format!(
                    "Preview: frames {a}–{b}, the hips cover {metres:.2} m (orange path)"
                ))
                .small()
                .color(self.palette().travel),
            );
        }
        if let Some(note) = &self.panels.layers.travel_note {
            ui.label(egui::RichText::new(note).small().color(self.palette().error));
        }
    }

    pub(in crate::app) fn preview_travel(&mut self) {
        match self.session.run("layer.travel_preview", &json!({})) {
            Ok(r) => {
                let path: Vec<[f64; 3]> = r["path"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?, p.get(2)?.as_f64()?]))
                    .collect();
                let (a, b) = (r["from"].as_i64().unwrap_or(0), r["to"].as_i64().unwrap_or(0));
                self.panels.layers.travel = Some((self.session.document.clip.clone(), a, b, path));
                self.panels.layers.travel_note = None;
            }
            Err(e) => self.travel_refused(&e.to_string()),
        }
    }

    /// Say why travel was refused, in words a person reads (the engine's reason after it).
    fn travel_refused(&mut self, e: &str) {
        let plain = if e.contains("playhead between two keys") {
            "Travel needs a key on each side of the playhead.".to_string()
        } else if e.contains("Locomotion") || e.contains("no animation") || e.contains("clips") {
            format!("Travel needs the Locomotion clips in your library (Import library): {e}")
        } else {
            format!("Travel couldn't be planned: {e}")
        };
        self.status = plain.clone();
        self.panels.layers.travel_note = Some(plain);
        self.panels.layers.travel = None;
    }
}
