//! The body-part picker: a small figure of the rig seen from the front, over the viewport's right
//! side. Click a joint to select it, Shift+click to add or remove it; the part buttons select a
//! whole part (Shift adds). "Mirror" selects the other side, "Key" keys the parts the selection
//! touches (`key.set`), and "Fingers & face" shows those handles here and in the viewport.

use std::collections::BTreeSet;

use ac_core::parts;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{Map, Value, json};

use super::App;
use crate::app::ui_ids::{self, Tag};

const PICKER_W: f32 = 190.0;
const FIGURE_H: f32 = 230.0;
const HAND_H: f32 = 84.0;
/// How near (points) a click must be to a joint.
const PICK_PX: f32 = 9.0;

const BONE_LINE: Color32 = Color32::from_gray(120);

/// The rig's rest joints seen from the front: (bone, [across, down]).
fn front(heads: &[[f64; 3]], bones: &[usize]) -> Vec<(usize, [f32; 2])> {
    bones
        .iter()
        .filter_map(|&b| heads.get(b).map(|h| (b, [h[0] as f32, -(h[2] as f32)])))
        .collect()
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: V3) -> Option<V3> {
    let l = dot(a, a).sqrt();
    (l > 1e-9).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// A hand's axes at rest: its wrist, down the middle finger, and across the knuckles.
fn hand_axes(sk: &ac_core::Skeleton, side: &str) -> Option<(V3, V3, V3)> {
    let at = |n: &str| {
        sk.bone(&format!("{n}.{side}"))
            .and_then(|b| sk.heads.get(b).copied())
    };
    let wrist = at("wrist")?;
    let down = unit(sub(at("finger3-3")?, wrist))?;
    let k = sub(at("finger2-1")?, at("finger5-1")?);
    let along = dot(k, down);
    let across = unit(sub(k, [down[0] * along, down[1] * along, down[2] * along]))?;
    Some((wrist, down, across))
}

/// A hand's rest joints seen square onto its palm (thumb side left for the right hand, as the
/// front view shows it); the front view when the rig's hand lacks the joints to tell.
fn palm(sk: &ac_core::Skeleton, side: &str, bones: &[usize]) -> Vec<(usize, [f32; 2])> {
    let Some((wrist, down, across)) = hand_axes(sk, side) else {
        return front(&sk.heads, bones);
    };
    let flip = if side == "R" { -1.0 } else { 1.0 };
    bones
        .iter()
        .filter_map(|&b| {
            let v = sub(*sk.heads.get(b)?, wrist);
            Some((b, [(flip * dot(v, across)) as f32, dot(v, down) as f32]))
        })
        .collect()
}

/// Points fitted into `rect`, keeping their proportions.
fn layout(pts: Vec<(usize, [f32; 2])>, rect: Rect) -> Vec<(usize, Pos2)> {
    let pts: Vec<(usize, [f32; 2])> = pts
        .into_iter()
        .filter(|(_, p)| p[0].is_finite() && p[1].is_finite())
        .collect();
    let Some(first) = pts.first() else {
        return Vec::new();
    };
    let (mut lo, mut hi) = (first.1, first.1);
    for (_, p) in &pts {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let inner = rect.shrink(10.0);
    let size = [(hi[0] - lo[0]).max(1e-3), (hi[1] - lo[1]).max(1e-3)];
    let scale = (inner.width() / size[0]).min(inner.height() / size[1]);
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    pts.into_iter()
        .map(|(b, p)| {
            (
                b,
                inner.center() + Vec2::new((p[0] - mid[0]) * scale, (p[1] - mid[1]) * scale),
            )
        })
        .collect()
}

impl App {
    /// The picker, over the viewport's right side.
    pub(super) fn picker(&mut self, ctx: &egui::Context, rect: Rect) {
        self.layout["picker"] = Value::Null;
        if !self.view.picker {
            return;
        }
        let at = Pos2::new(rect.right() - PICKER_W - 12.0, rect.top() + 40.0);
        egui::Area::new(egui::Id::unique("viewport-picker"))
            .fixed_pos(at)
            .constrain_to(rect)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(PICKER_W);
                    self.picker_body(ui);
                });
            });
    }

    fn picker_body(&mut self, ui: &mut egui::Ui) {
        let add = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl || i.modifiers.command);
        ui.horizontal(|ui| {
            ui.strong("Picker");
            ui.toggle_value(&mut self.view.all_handles, "Fingers & face")
                .on_hover_text("Handles on the fingers, eyes and toes too, here and in the viewport")
                .tag("picker.all_handles", "Fingers & face");
        });
        let sk = self.session.document.skeleton.clone();
        let handles = self.handles();
        // the hands get their own close-ups when finger handles show
        let hands: BTreeSet<usize> = if self.view.all_handles {
            ["wrist.L", "wrist.R"]
                .iter()
                .flat_map(|w| {
                    let top = sk.bone(w);
                    parts::subtree(&sk, w)
                        .into_iter()
                        .filter(move |b| Some(*b) != top)
                })
                .collect()
        } else {
            BTreeSet::new()
        };
        let body: Vec<usize> = (0..sk.len())
            .filter(|b| handles.get(*b).copied().unwrap_or(false) && !hands.contains(b))
            .collect();
        let (resp, painter) = ui.allocate_painter(Vec2::new(PICKER_W, FIGURE_H), Sense::click());
        let resp = resp.tag("picker.figure", "Figure");
        let spots = layout(front(&sk.heads, &body), resp.rect);
        self.draw_figure(&painter, &spots);
        let mut boxes = vec![(resp, spots)];
        if !hands.is_empty() {
            ui.horizontal(|ui| {
                // the figure faces the viewer: its left hand is on the right
                for (wrist, side) in [("wrist.R", "R"), ("wrist.L", "L")] {
                    let mut set: Vec<usize> = parts::subtree(&sk, wrist)
                        .into_iter()
                        .filter(|b| handles.get(*b).copied().unwrap_or(false))
                        .collect();
                    set.sort_unstable();
                    let (r, p) = ui.allocate_painter(Vec2::new(PICKER_W / 2.0 - 4.0, HAND_H), Sense::click());
                    let r = r.tag(format!("picker.hand.{side}"), format!("Hand {side}"));
                    p.rect_stroke(
                        r.rect,
                        3.0,
                        Stroke::new(1.0, Color32::from_gray(60)),
                        egui::StrokeKind::Inside,
                    );
                    let s = layout(palm(&sk, side, &set), r.rect);
                    self.draw_figure(&p, &s);
                    boxes.push((r, s));
                }
            });
        }
        // where things are, for the control port's layout
        let mut spots_at = Map::new();
        let mut buttons = Map::new();
        for (b, p) in boxes.iter().flat_map(|(_, s)| s.iter()) {
            if let Some(n) = sk.names.get(*b) {
                spots_at.insert(n.clone(), json!([p.x, p.y]));
                // a joint is a dot in the figure's click area: report the dot as its own widget
                let dot = Rect::from_center_size(*p, Vec2::splat(PICK_PX));
                ui_ids::report(ui.ctx(), format!("picker.joint.{n}"), n.clone(), Some(dot), true);
            }
        }
        for (r, s) in &boxes {
            if r.clicked()
                && let Some(at) = r.interact_pointer_pos()
            {
                let hit = s
                    .iter()
                    .map(|(b, p)| (*b, p.distance(at)))
                    .filter(|(_, d)| *d < PICK_PX)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(b, _)| b);
                if hit.is_some() || !add {
                    self.select_joint(hit, add);
                }
            }
        }
        // the parts
        ui.horizontal_wrapped(|ui| {
            for part in parts::ROWS.iter().chain(["Hands"].iter()) {
                let bones = parts::bones(&sk, &self.session.document.custom_parts, part).unwrap_or_default();
                let all_in = !bones.is_empty() && bones.iter().all(|b| self.view.picked.contains(b));
                let r = ui
                    .selectable_label(all_in, *part)
                    .on_hover_text("Select this part (Shift: add it)")
                    .tag(
                        format!("picker.part.{}", part.to_lowercase().replace(' ', "_")),
                        *part,
                    );
                buttons.insert(part.to_string(), json!([r.rect.center().x, r.rect.center().y]));
                if r.clicked() {
                    if !add {
                        self.view.picked.clear();
                    }
                    self.view.picked.extend(bones.iter().copied());
                    self.selected = bones
                        .iter()
                        .copied()
                        .find(|b| handles.get(*b).copied().unwrap_or(false))
                        .or(bones.first().copied());
                }
            }
        });
        ui.horizontal(|ui| {
            let r = ui
                .button("Mirror")
                .on_hover_text("Select the same joints on the other side")
                .tag("picker.mirror", "Mirror");
            buttons.insert("Mirror".into(), json!([r.rect.center().x, r.rect.center().y]));
            if r.clicked() {
                self.select_mirror();
            }
            let r = ui.button("Clear").tag("picker.clear", "Clear");
            buttons.insert("Clear".into(), json!([r.rect.center().x, r.rect.center().y]));
            if r.clicked() {
                self.select_joint(None, false);
            }
            let sel = self.selection();
            if ui
                .add_enabled(!sel.is_empty(), egui::Button::new("Key"))
                .on_hover_text("Key the body parts the selection touches")
                .tag("picker.key", "Key")
                .clicked()
            {
                let rows = parts::rows(&sk, Some(&sel));
                if let Some(r) = self.run("key.set", json!({ "parts": rows })) {
                    self.status = format!("keyed {} on frame {}", rows.join(", "), r["frame"]);
                }
            }
        });
        let n = self.selection().len();
        ui.label(match (n, self.selected.and_then(|b| sk.names.get(b))) {
            (0, _) => "Nothing selected".to_string(),
            (1, Some(name)) => name.clone(),
            (n, Some(name)) => format!("{n} joints (active: {name})"),
            (n, None) => format!("{n} joints"),
        });
        self.layout["picker"] = json!({ "joints": spots_at, "buttons": buttons });
    }

    fn draw_figure(&self, painter: &egui::Painter, spots: &[(usize, Pos2)]) {
        let sk = &self.session.document.skeleton;
        let at = |b: usize| spots.iter().find(|s| s.0 == b).map(|s| s.1);
        // a line from each shown joint to its nearest shown ancestor
        for (b, p) in spots {
            let mut up = sk.parents.get(*b).copied().flatten();
            while let Some(u) = up {
                if let Some(q) = at(u) {
                    painter.line_segment([q, *p], Stroke::new(2.0, BONE_LINE));
                    break;
                }
                up = sk.parents.get(u).copied().flatten();
            }
        }
        for (b, p) in spots {
            let (fill, r) = if self.selected == Some(*b) {
                (self.palette().joint_active, 4.5)
            } else if self.view.picked.contains(b) {
                (self.palette().joint_picked, 4.0)
            } else {
                (self.palette().joint, 3.0)
            };
            painter.circle(*p, r, fill, Stroke::new(1.0, Color32::BLACK));
        }
    }

    /// Select the same joints on the other side of the body (centre joints stay).
    pub(super) fn select_mirror(&mut self) {
        let sk = self.session.document.skeleton.clone();
        let mirror = |b: usize| {
            sk.names
                .get(b)
                .and_then(|n| sk.bone(&parts::mirror_name(n)))
                .unwrap_or(b)
        };
        self.view.picked = self.view.picked.iter().map(|b| mirror(*b)).collect();
        self.selected = self.selected.map(mirror);
        self.status = format!("selected the mirror side ({} joints)", self.selection().len());
    }
}
