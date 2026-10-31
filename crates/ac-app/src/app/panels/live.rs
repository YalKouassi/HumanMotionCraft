//! Live motion control: steer the character with W A S D (Shift to run) or a controller while the
//! model makes the motion, then keep a range of what was recorded. The commands do the work
//! (`live.*`, `ac_posefill::live_commands`): this feeds them the keys, the stick and the time.
//!
//! While a take runs the keys it steers with go to it and not to the shortcuts, and the camera
//! follows the body. Stick directions are relative to the camera.

use egui::{Event, Key};
use serde_json::{Value, json};

use crate::app::App;
use crate::app::shell::Level;
use crate::app::ui_ids::Tag;

use super::gamepad::Pad;

/// Stick speeds in m/s: walking on the keys, and running with Shift (held to the set's top speed).
const WALK: f64 = 1.3;
const RUN: f64 = 3.6;
/// Frames of motion at 30 fps.
const FPS: f64 = 30.0;
/// The most frames one window frame moves on: after a long stall the take doesn't race to catch up.
const MOST: f64 = 10.0;
const SLOW_STICK: f32 = 0.15;

/// Keys held.
#[derive(Default)]
struct Held {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    shift: bool,
}

/// The panel's window state.
#[derive(Default)]
pub struct LiveState {
    running: bool,
    keys: Held,
    /// Time not yet turned into frames.
    owed: f64,
    /// The top speed the set gave.
    top: f64,
    /// Whether the camera followed the body before the take.
    followed: bool,
    pad: Option<Pad>,
    /// What `live.stop` said; the range chosen; the frame looked at.
    stopped: Option<Value>,
    from: i64,
    to: i64,
    look: i64,
    target: String,
    frames: i64,
    /// Asked to start and waiting for the model to load.
    starting: bool,
}

/// The stick (right, forward; each -1 to 1) as a heading: `yaw` is the camera's, whose right on the
/// ground is (cos, sin). The rig's forward (-Y) is heading 0, its left (+X) 90.
pub(super) fn heading(right: f32, forward: f32, yaw: f32) -> f64 {
    let (s, c) = yaw.sin_cos();
    let x = right * c - forward * s;
    let y = right * s + forward * c;
    f64::from(x.atan2(-y).to_degrees())
}

impl App {
    /// Take the keys a running take steers with, before the shortcuts see them; Escape stops it.
    pub(in crate::app) fn live_keys(&mut self, raw: &mut egui::RawInput) {
        let st = &mut self.panels.live;
        if !st.running {
            return;
        }
        raw.events.retain(|e| match e {
            Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } => {
                st.keys.shift = modifiers.shift;
                match key {
                    Key::W => st.keys.w = *pressed,
                    Key::A => st.keys.a = *pressed,
                    Key::S => st.keys.s = *pressed,
                    Key::D => st.keys.d = *pressed,
                    _ => return true,
                }
                false
            }
            Event::ModifiersChanged(m) => {
                st.keys.shift = m.shift;
                true
            }
            // the focus went away with keys down: their releases won't come
            Event::WindowFocused(false) => {
                st.keys = Held::default();
                true
            }
            _ => true,
        });
    }

    fn live_end(&mut self) {
        let st = &mut self.panels.live;
        st.running = false;
        st.keys = Held::default();
        self.view.follow = st.followed;
    }

    /// Bring the body back into view: a take can end tens of metres from where the pose on show is.
    fn live_centre(&mut self) {
        let followed = self.view.follow;
        self.view.follow = true;
        self.follow_body();
        self.view.follow = followed;
    }

    fn live_begin(&mut self, v: &Value) {
        self.playing = false;
        let followed = self.view.follow;
        self.view.follow = true;
        let st = &mut self.panels.live;
        st.running = true;
        st.starting = false;
        st.followed = followed;
        st.top = v["max_speed"].as_f64().unwrap_or(RUN);
        st.owed = 0.0;
        st.stopped = None;
        st.frames = 1;
        st.target.clear();
        if st.pad.is_none() {
            match Pad::new() {
                Ok(p) => st.pad = Some(p),
                Err(e) => self
                    .shell
                    .messages
                    .push(Level::Info, format!("Controller: {e}. W A S D still steer.")),
            }
        }
    }

    fn live_stop(&mut self) {
        self.live_end();
        let out = self.run("live.stop", json!({}));
        if let Some(v) = out {
            let last = v["frames"].as_i64().unwrap_or(1) - 1;
            let st = &mut self.panels.live;
            st.from = 0;
            st.to = last.max(0);
            st.look = 0;
            st.stopped = Some(v);
        }
        self.live_centre();
    }

    /// Every window frame: a running take moves on by the time passed, with the stick as it is now.
    pub(in crate::app) fn live_frame(&mut self, ctx: &egui::Context) {
        if self.panels.live.starting && self.panels.posefill_idle() {
            // once: a load that failed has said why, and isn't tried again unasked
            self.panels.live.starting = false;
            if let Some(v) = self.run("live.start", json!({})) {
                self.live_begin(&v);
            }
        }
        if !self.panels.live.running {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.live_stop();
            return;
        }
        let yaw = self.camera.yaw;
        // the time really passed: `owed` caps a stall, and the take must keep to the clock at any refresh rate
        let dt = f64::from(ctx.input(|i| i.unstable_dt));
        let st = &mut self.panels.live;
        let pad = st.pad.as_mut().map(Pad::poll).unwrap_or_default();
        let [mut right, mut forward] = pad.stick;
        let lost = pad.lost;
        let k = &st.keys;
        let on_keys = k.w || k.a || k.s || k.d;
        let speed = if on_keys {
            right = f32::from(u8::from(k.d)) - f32::from(u8::from(k.a));
            forward = f32::from(u8::from(k.w)) - f32::from(u8::from(k.s));
            if right == 0.0 && forward == 0.0 {
                0.0
            } else if k.shift {
                RUN
            } else {
                WALK
            }
        } else {
            let tilt = right.hypot(forward).min(1.0);
            if tilt < SLOW_STICK {
                0.0
            } else {
                f64::from(tilt) * st.top
            }
        };
        st.owed = (st.owed + dt * FPS).min(MOST);
        let frames = st.owed.floor();
        st.owed -= frames;
        let params = json!({
            "heading": heading(right, forward, yaw), "speed": speed, "frames": frames as i64, "wait": false,
        });
        if let Some(name) = lost {
            let text = format!(
                "Controller {name} was unplugged: the character stops. Plug it back in, or use W A S D."
            );
            self.shell.messages.push(Level::Error, text.clone());
            self.status = text;
        }
        // not through `run`: thirty a second would fill the history of messages and timings
        match self.session.run("live.input", &params) {
            Ok(v) => {
                let st = &mut self.panels.live;
                st.frames = v["frames"].as_i64().unwrap_or(st.frames);
                if let Some(t) = v["target"].as_str() {
                    st.target = t.to_string();
                }
                for n in v["notes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    self.shell.messages.push(Level::Info, format!("Live motion: {n}"));
                    self.status = format!("Live motion: {n}");
                }
            }
            Err(e) => {
                self.refused("live.input", &e);
                self.live_stop();
            }
        }
        ctx.request_repaint();
    }

    fn live_keep(&mut self, new: bool) {
        let st = &self.panels.live;
        let (from, to) = (st.from, st.to);
        let kept = if new {
            self.run(
                "edit.batch",
                json!({ "label": "Keep a live take", "commands": [
                    { "command": "clip.new", "params": { "name": "Live take", "group": "Live motion" } },
                    { "command": "live.clip", "params": { "from": from, "to": to, "at": 0 } },
                ] }),
            )
        } else {
            self.run("live.clip", json!({ "from": from, "to": to }))
        };
        if kept.is_some() {
            self.live_centre();
            self.status = format!(
                "Kept {:.1} s of the take{}",
                (to - from) as f64 / FPS,
                if new { " as a new animation" } else { "" }
            );
        }
    }

    /// The section under the pose fill.
    pub(in crate::app) fn live_panel(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(egui::RichText::new("Live motion").strong());
        ui.label(
            egui::RichText::new(
                "Steer the character and the model makes the motion: W A S D (Shift to run) or a controller, relative to the camera. Everything shown is recorded; afterwards keep the stretch you like.",
            )
            .small()
            .weak(),
        );
        if self.panels.live.running {
            let (seconds, target) = (
                self.panels.live.frames as f64 / FPS,
                self.panels.live.target.clone(),
            );
            ui.label(format!("Recording: {seconds:.1} s. Aiming at: {target}"));
            if ui
                .button("Stop take")
                .on_hover_text("Esc")
                .tag("panel.live.stop", "Stop take")
                .clicked()
            {
                self.live_stop();
            }
            return;
        }
        let starting = self.panels.live.starting;
        if ui
            .add_enabled(
                !starting,
                egui::Button::new(if starting { "Starting…" } else { "Start take" }),
            )
            .on_hover_text("From the pose on show. A new take replaces the last recording.")
            .tag("panel.live.start", "Start take")
            .clicked()
        {
            // the model may load first, off the UI thread (`posefill_deferred`)
            match self.run("live.start", json!({})) {
                Some(v) => self.live_begin(&v),
                None if !self.panels.posefill_idle() => self.panels.live.starting = true,
                None => {}
            }
        }
        let Some(stopped) = self.panels.live.stopped.clone() else {
            return;
        };
        let last = stopped["frames"].as_i64().unwrap_or(1) - 1;
        if last < 1 {
            ui.label("The take recorded nothing to keep.");
            return;
        }
        ui.label(format!(
            "Recorded {:.1} s; a stretch took {:.0} ms on average, {:.0} ms at most; {} frames held, {} late.",
            last as f64 / FPS,
            stopped["round_trip_ms"].as_f64().unwrap_or(0.0),
            stopped["round_trip_max_ms"].as_f64().unwrap_or(0.0),
            stopped["held"],
            stopped["late"],
        ));
        let st = &mut self.panels.live;
        let look_before = st.look;
        ui.add(egui::Slider::new(&mut st.look, 0..=last).text("Look at frame"))
            .tag("panel.live.look", "Look at frame");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut st.from).range(0..=last).prefix("From "))
                .tag("panel.live.from", "From");
            ui.add(egui::DragValue::new(&mut st.to).range(0..=last).prefix("To "))
                .tag("panel.live.to", "To");
            if ui
                .small_button("From here")
                .tag("panel.live.set_from", "From here")
                .clicked()
            {
                st.from = st.look;
            }
            if ui
                .small_button("To here")
                .tag("panel.live.set_to", "To here")
                .clicked()
            {
                st.to = st.look;
            }
        });
        let (look, can) = (st.look, st.from < st.to);
        if look != look_before {
            self.run("live.show", json!({ "frame": look }));
            self.live_centre();
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(can, egui::Button::new("Keep in this animation"))
                .on_hover_text("From the playhead on, one undo step")
                .tag("panel.live.keep", "Keep in this animation")
                .clicked()
            {
                self.live_keep(false);
            }
            if ui
                .add_enabled(can, egui::Button::new("Keep as a new animation"))
                .on_hover_text("One undo step; save it to the library as any other")
                .tag("panel.live.keep_new", "Keep as a new animation")
                .clicked()
            {
                self.live_keep(true);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-3, "{a} is not {b}");
    }

    /// A camera with yaw 0 has +X on its right and looks along +Y, so pushing forward goes along
    /// +Y: the rig's back, heading 180 either way round; right is +X, the rig's left, 90.
    #[test]
    fn the_stick_goes_where_the_camera_looks() {
        near(heading(0.0, 1.0, 0.0).abs(), 180.0);
        near(heading(1.0, 0.0, 0.0), 90.0);
        near(heading(-1.0, 0.0, 0.0), -90.0);
        near(heading(0.0, -1.0, 0.0), 0.0);
    }

    /// Turned half round, the camera looks along -Y, the rig's forward.
    #[test]
    fn turning_the_camera_turns_the_stick() {
        let half = std::f32::consts::PI;
        near(heading(0.0, 1.0, half), 0.0);
        near(heading(1.0, 0.0, half), -90.0);
    }
}
