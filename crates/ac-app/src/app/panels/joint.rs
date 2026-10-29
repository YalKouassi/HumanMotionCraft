//! The selected joint: its name and position, its rotation as X/Y/Z angles to drag or type
//! (`pose.set_bone`; a drag is one undo step), locking its body part (`parts.lock`), pinning it;
//! and the pose tools: mirror (flip, left onto right, right onto left), spread a hip bend into
//! the back, lock the feet.

use ac_core::quat;
use serde_json::json;

use super::{gesture, gesture_end};
use crate::app::App;
use crate::app::ui_ids::Tag;

/// The panel's window state.
#[derive(Default)]
pub struct JointState {
    /// The angles being dragged (bone, degrees), so a drag does not jump through the
    /// quaternion round trip near ±90°.
    editing: Option<(usize, [f64; 3])>,
}

impl App {
    pub(in crate::app) fn joint_panel(&mut self, ui: &mut egui::Ui) {
        match self.selected {
            Some(b) => self.joint_details(ui, b),
            None => {
                ui.label(
                    egui::RichText::new(
                        "Click a joint to select it; drag it to pose. Right-click a joint to pin it.",
                    )
                    .weak(),
                );
            }
        }
        ui.add_space(4.0);
        self.pose_tools(ui);
    }

    fn joint_details(&mut self, ui: &mut egui::Ui, b: usize) {
        let doc = &self.session.document;
        let name = doc.skeleton.names.get(b).cloned().unwrap_or_default();
        ui.label(egui::RichText::new(&name).strong());
        if let Some(p) = doc.joints().ok().and_then(|j| j.get(b).copied()) {
            ui.label(format!("at {:.3}, {:.3}, {:.3} m", p[0], p[1], p[2]));
        }
        let Some(q) = doc.pose.rotations.get(b).copied() else {
            return;
        };
        let mut angles = match self.panels.joint.editing {
            Some((e, a)) if e == b => a,
            _ => quat::to_euler(q),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("Rotation");
            for (axis, label) in ["X", "Y", "Z"].iter().enumerate() {
                let Some(v) = angles.get_mut(axis) else { continue };
                let r = ui
                    .add(
                        egui::DragValue::new(v)
                            .prefix(format!("{label} "))
                            .suffix("°")
                            .speed(0.5)
                            .range(-180.0..=180.0)
                            .max_decimals(1),
                    )
                    .on_hover_text(format!(
                        "Angle about {label} relative to the parent (degrees): drag or type"
                    ))
                    .tag(format!("panel.joint.rot.{}", label.to_lowercase()), *label);
                if r.drag_started() {
                    self.panels.joint.editing = Some((b, angles));
                }
                if gesture(self, &r, &format!("Rotate {name}")) {
                    self.panels.joint.editing = self.panels.joint.editing.map(|(e, _)| (e, angles));
                    let rotation = quat::normalize(quat::from_euler(angles));
                    self.run("pose.set_bone", json!({ "bone": name, "rotation": rotation }));
                }
                gesture_end(self, &r);
                if r.drag_stopped() {
                    self.panels.joint.editing = None;
                }
            }
            if ui
                .small_button("0")
                .on_hover_text("Back to the rest rotation")
                .tag("panel.joint.rot.reset", "0")
                .clicked()
            {
                self.run(
                    "pose.set_bone",
                    json!({ "bone": name, "rotation": quat::IDENTITY }),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            // the body part the joint belongs to, locked or not
            let sk = &self.session.document.skeleton;
            let part = ac_core::parts::ROWS.iter().copied().find(|row| {
                ac_core::parts::bones(sk, &self.session.document.custom_parts, row)
                    .is_ok_and(|bones| bones.contains(&b))
            });
            if let Some(part) = part {
                let locked: Vec<String> = self.session.document.locked.clone();
                let mut on = locked.iter().any(|l| l == part);
                if ui
                    .checkbox(&mut on, format!("Lock {part}"))
                    .on_hover_text("A drag leaves a locked part as it is")
                    .tag("panel.joint.lock", format!("Lock {part}"))
                    .changed()
                {
                    let mut parts: Vec<String> = locked.into_iter().filter(|l| l != part).collect();
                    if on {
                        parts.push(part.to_string());
                    }
                    self.run("parts.lock", json!({ "parts": parts }));
                }
            }
            let pinned = self.session.document.pins.contains_key(&name);
            if ui
                .button(if pinned { "Unpin" } else { "Pin" })
                .on_hover_text("Or right-click the joint in the viewport")
                .tag("panel.joint.pin", if pinned { "Unpin" } else { "Pin" })
                .clicked()
            {
                self.run("pin.set", json!({ "bone": name, "on": !pinned }));
            }
        });
    }

    /// Pose tools that work on the whole pose shown.
    fn pose_tools(&mut self, ui: &mut egui::Ui) {
        let locked = self.session.document.locked.clone();
        if !locked.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("Locked: {}", locked.join(", "))).small());
                if ui
                    .small_button("Unlock all")
                    .tag("panel.joint.unlock_all", "Unlock all")
                    .clicked()
                {
                    self.run("parts.lock", json!({ "parts": [] }));
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Mirror");
            for (label, mode, tip) in [
                ("Flip", "flip", "Swap the left and right sides of the pose"),
                ("L to R", "l2r", "Copy the left side onto the right"),
                ("R to L", "r2l", "Copy the right side onto the left"),
            ] {
                if ui
                    .button(label)
                    .on_hover_text(tip)
                    .tag(format!("panel.joint.mirror.{mode}"), label)
                    .clicked()
                {
                    self.run("pose.mirror", json!({ "mode": mode }));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Spread bend")
                .on_hover_text("Share a bend at the hips with the lower back, as people bend")
                .tag("panel.joint.spread_bend", "Spread bend")
                .clicked()
                && let Some(r) = self.run("pose.spread_bend", json!({}))
            {
                self.status = format!(
                    "spread {:.1}° of the hip bend into the back",
                    r["degrees"].as_f64().unwrap_or(0.0)
                );
            }
            if ui
                .button("Lock feet")
                .on_hover_text("Plant the feet where they stand still across your keys, so they don't slide")
                .tag("panel.joint.lock_feet", "Lock feet")
                .clicked()
                && let Some(r) = self.run("feet.lock", json!({}))
            {
                self.status = format!("locked feet: {} frames changed", r["frames"]);
            }
        });
    }
}
