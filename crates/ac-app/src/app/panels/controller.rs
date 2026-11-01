//! The Controller panel: W A S D and a controller drawn in code, lit by what is really pressed and
//! pressable with the mouse, so a take can be steered with nothing plugged in. It is window state
//! feeding `live.input` through `live_frame`, the same way as the keys and the stick. A take reads
//! the left stick, W A S D and Shift; the rest only shows, and the panel says so.

use egui::{Color32, Key, Pos2, Rect, Sense, Stroke, Ui, Vec2, vec2};

use crate::app::App;
use crate::app::shell::{DockPanel, Level};
use crate::app::ui_ids::Tag;

use super::gamepad::{Btn, Inputs, Pad};
use super::live::Held;

/// A way of drawing a controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Standard,
    Xbox,
    PlayStation,
}

impl Layout {
    const ALL: [Layout; 3] = [Layout::Standard, Layout::Xbox, Layout::PlayStation];

    fn name(self) -> &'static str {
        match self {
            Layout::Standard => "Standard controller",
            Layout::Xbox => "Xbox controller",
            Layout::PlayStation => "PlayStation controller",
        }
    }

    /// The layout that suits the name a controller reports.
    fn guess(name: &str) -> Layout {
        let n = name.to_lowercase();
        let ps = ["playstation", "dualsense", "dualshock", "ps3", "ps4", "ps5"];
        if ps.iter().any(|p| n.contains(p)) {
            Layout::PlayStation
        } else {
            // most controllers follow this one's shape, and it draws every control
            Layout::Xbox
        }
    }

    fn parts(self) -> &'static [Part] {
        match self {
            Layout::Standard => STANDARD,
            Layout::Xbox => XBOX,
            Layout::PlayStation => PLAYSTATION,
        }
    }
}

/// What the dropdown has chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// W A S D only: controllers are ignored.
    None,
    /// This drawing, simulated with the mouse (a real controller's inputs show on it too).
    Simulated(Layout),
    /// A controller plugged in: its number and name.
    Plugged(usize, String),
}

impl Default for Choice {
    fn default() -> Self {
        Choice::Simulated(Layout::Standard)
    }
}

impl Choice {
    fn label(&self) -> String {
        match self {
            Choice::None => "No controller".into(),
            Choice::Simulated(l) => l.name().into(),
            Choice::Plugged(_, name) => format!("Plugged in: {name}"),
        }
    }

    fn layout(&self) -> Option<Layout> {
        match self {
            Choice::None => None,
            Choice::Simulated(l) => Some(*l),
            Choice::Plugged(_, name) => Some(Layout::guess(name)),
        }
    }
}

/// What a part of the drawing is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Control {
    Stick(usize),
    Trigger(usize),
    Button(Btn),
}

/// What is drawn on a part.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mark {
    Text(&'static str),
    Triangle,
    Circle,
    Cross,
    Square,
    Arrow(f32),
}

/// One control, on a drawing [`WIDE`] by [`TALL`]: its centre and half size.
#[derive(Clone, Copy, Debug)]
struct Part {
    control: Control,
    at: [f32; 2],
    half: [f32; 2],
    mark: Mark,
    id: &'static str,
}

const WIDE: f32 = 100.0;
const TALL: f32 = 64.0;

const fn part(control: Control, at: [f32; 2], half: [f32; 2], mark: Mark, id: &'static str) -> Part {
    Part {
        control,
        at,
        half,
        mark,
        id,
    }
}
const fn stick(i: usize, at: [f32; 2], id: &'static str) -> Part {
    part(Control::Stick(i), at, [9.0, 9.0], Mark::Text(""), id)
}
const fn btn(b: Btn, at: [f32; 2], mark: Mark, id: &'static str) -> Part {
    part(Control::Button(b), at, [4.0, 4.0], mark, id)
}
const fn small(b: Btn, at: [f32; 2], text: &'static str, id: &'static str) -> Part {
    part(Control::Button(b), at, [5.0, 2.5], Mark::Text(text), id)
}
const fn pull(i: usize, x: f32, text: &'static str, id: &'static str) -> Part {
    part(Control::Trigger(i), [x, 5.0], [7.0, 4.0], Mark::Text(text), id)
}
const fn bump(b: Btn, x: f32, text: &'static str, id: &'static str) -> Part {
    part(Control::Button(b), [x, 5.0], [7.0, 4.0], Mark::Text(text), id)
}
const fn arrow(b: Btn, at: [f32; 2], turn: f32, id: &'static str) -> Part {
    part(Control::Button(b), at, [3.0, 3.0], Mark::Arrow(turn), id)
}
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

/// Two sticks and four buttons: no one's in particular.
const STANDARD: &[Part] = &[
    stick(0, [20.0, 32.0], "stick_left"),
    stick(1, [50.0, 32.0], "stick_right"),
    btn(Btn::North, [80.0, 22.0], Mark::Text("1"), "north"),
    btn(Btn::East, [90.0, 32.0], Mark::Text("2"), "east"),
    btn(Btn::South, [80.0, 42.0], Mark::Text("3"), "south"),
    btn(Btn::West, [70.0, 32.0], Mark::Text("4"), "west"),
];

const XBOX: &[Part] = &[
    pull(0, 14.0, "LT", "trigger_left"),
    bump(Btn::BumperL, 30.0, "LB", "bumper_left"),
    bump(Btn::BumperR, 70.0, "RB", "bumper_right"),
    pull(1, 86.0, "RT", "trigger_right"),
    stick(0, [22.0, 24.0], "stick_left"),
    small(Btn::ThumbL, [22.0, 37.0], "L3", "thumb_left"),
    arrow(Btn::Up, [38.0, 41.0], 0.0, "up"),
    arrow(Btn::Right, [45.0, 48.0], HALF_PI, "right"),
    arrow(Btn::Down, [38.0, 55.0], 2.0 * HALF_PI, "down"),
    arrow(Btn::Left, [31.0, 48.0], 3.0 * HALF_PI, "left"),
    stick(1, [62.0, 46.0], "stick_right"),
    small(Btn::ThumbR, [62.0, 59.0], "R3", "thumb_right"),
    small(Btn::Select, [42.0, 20.0], "View", "select"),
    small(Btn::Start, [58.0, 20.0], "Menu", "start"),
    btn(Btn::North, [80.0, 18.0], Mark::Text("Y"), "north"),
    btn(Btn::East, [89.0, 27.0], Mark::Text("B"), "east"),
    btn(Btn::South, [80.0, 36.0], Mark::Text("A"), "south"),
    btn(Btn::West, [71.0, 27.0], Mark::Text("X"), "west"),
];

const PLAYSTATION: &[Part] = &[
    pull(0, 14.0, "L2", "trigger_left"),
    bump(Btn::BumperL, 30.0, "L1", "bumper_left"),
    bump(Btn::BumperR, 70.0, "R1", "bumper_right"),
    pull(1, 86.0, "R2", "trigger_right"),
    arrow(Btn::Up, [20.0, 19.0], 0.0, "up"),
    arrow(Btn::Right, [27.0, 26.0], HALF_PI, "right"),
    arrow(Btn::Down, [20.0, 33.0], 2.0 * HALF_PI, "down"),
    arrow(Btn::Left, [13.0, 26.0], 3.0 * HALF_PI, "left"),
    stick(0, [36.0, 46.0], "stick_left"),
    small(Btn::ThumbL, [36.0, 59.0], "L3", "thumb_left"),
    stick(1, [64.0, 46.0], "stick_right"),
    small(Btn::ThumbR, [64.0, 59.0], "R3", "thumb_right"),
    small(Btn::Select, [40.0, 20.0], "Create", "select"),
    small(Btn::Start, [60.0, 20.0], "Options", "start"),
    btn(Btn::North, [80.0, 18.0], Mark::Triangle, "north"),
    btn(Btn::East, [89.0, 27.0], Mark::Circle, "east"),
    btn(Btn::South, [80.0, 36.0], Mark::Cross, "south"),
    btn(Btn::West, [71.0, 27.0], Mark::Square, "west"),
];

/// The panel's window state, never saved.
#[derive(Default)]
pub struct ControllerState {
    pub choice: Choice,
    /// The person picked in the dropdown, so a controller plugged in doesn't take over.
    chosen: bool,
    pad: Option<Pad>,
    /// Said once: why controllers can't be read.
    no_pads: Option<String>,
    pads: Vec<(usize, String)>,
    real: Inputs,
    mouse: Inputs,
    mouse_keys: Held,
    /// The panel drew since the last frame: closed or a hidden tab, nothing can stay held by mouse.
    drawn: bool,
    /// Asked for from outside (`--ui controller=<name>`), applied once the pad is known.
    pin_wanted: Option<Option<usize>>,
}

/// The tilt of a stick whose centre and reach are given, for a pointer there: right and forward,
/// held to the circle. Forward is up the screen.
pub(super) fn stick_value(centre: Pos2, reach: f32, pointer: Pos2) -> [f32; 2] {
    if !(reach.is_finite() && reach > 0.0 && pointer.x.is_finite() && pointer.y.is_finite()) {
        return [0.0; 2];
    }
    let (x, y) = ((pointer.x - centre.x) / reach, (centre.y - pointer.y) / reach);
    if !(x.is_finite() && y.is_finite()) {
        return [0.0; 2];
    }
    let long = x.hypot(y);
    if long > 1.0 { [x / long, y / long] } else { [x, y] }
}

/// A trigger's pull for a pointer down it: top none, bottom all.
fn trigger_value(top: f32, height: f32, y: f32) -> f32 {
    let v = (y - top) / height;
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
}

/// The real stick and the mouse's together: the larger tilt wins.
pub(super) fn combine(real: [f32; 2], mouse: [f32; 2]) -> [f32; 2] {
    if mouse[0].hypot(mouse[1]) > real[0].hypot(real[1]) {
        mouse
    } else {
        real
    }
}

impl ControllerState {
    /// The left stick a take steers with.
    pub(super) fn steer(&self) -> [f32; 2] {
        if self.choice == Choice::None {
            return [0.0; 2];
        }
        combine(self.real.sticks[0], self.mouse.sticks[0])
    }

    /// W A S D and Shift held with the mouse.
    pub(super) fn mouse_keys(&self) -> Held {
        self.mouse_keys
    }

    /// `--ui controller=none|standard|xbox|playstation`.
    pub(super) fn set_named(&mut self, value: &str) -> Result<(), String> {
        self.choice = match value {
            "none" => Choice::None,
            "standard" => Choice::Simulated(Layout::Standard),
            "xbox" => Choice::Simulated(Layout::Xbox),
            "playstation" => Choice::Simulated(Layout::PlayStation),
            _ => return Err("none, standard, xbox or playstation".into()),
        };
        self.chosen = true;
        self.pin_wanted = Some(None);
        Ok(())
    }

    fn pick(&mut self, choice: Choice) {
        self.pin_wanted = Some(match &choice {
            Choice::Plugged(n, _) => Some(*n),
            _ => None,
        });
        self.choice = choice;
        self.chosen = true;
    }
}

impl App {
    /// Open or close the panel with `--ui controller=off` and the rest.
    pub(super) fn set_controller_ui(&mut self, value: &str) -> Result<(), String> {
        if value == "off" {
            self.set_dock_panel(DockPanel::Controller, false);
            return Ok(());
        }
        self.panels.controller.set_named(value)?;
        self.set_dock_panel(DockPanel::Controller, true);
        Ok(())
    }

    /// Every window frame, before the take reads it: the controller, while the panel shows or a
    /// take runs.
    pub(in crate::app) fn controller_frame(&mut self) {
        let running = self.panels.live.running;
        let st = &mut self.panels.controller;
        if !st.drawn {
            st.mouse = Inputs::default();
            st.mouse_keys = Held::default();
        }
        let wanted = st.drawn || running;
        st.drawn = false;
        if !wanted {
            st.real = Inputs::default();
            return;
        }
        if st.pad.is_none() && st.no_pads.is_none() {
            match Pad::new() {
                Ok(p) => st.pad = Some(p),
                Err(e) => {
                    self.shell
                        .messages
                        .push(Level::Info, format!("Controller: {e}. W A S D still steer."));
                    st.no_pads = Some(e);
                }
            }
        }
        let Some(pad) = st.pad.as_mut() else {
            return;
        };
        if let Some(which) = st.pin_wanted.take() {
            pad.pin(which);
        }
        let state = pad.poll();
        st.real = state.inputs;
        st.pads = pad.list();
        if !st.chosen
            && let Some((n, name)) = st.pads.first()
        {
            pad.pin(Some(*n));
            st.choice = Choice::Plugged(*n, name.clone());
        }
        if let Some(name) = state.lost {
            if let Choice::Plugged(_, was) = &st.choice {
                // back to the drawing it had, simulated, until one is plugged in again
                st.choice = Choice::Simulated(Layout::guess(was));
                st.chosen = false;
            }
            let text = format!(
                "Controller {name} was unplugged: the character stops. Plug it back in, use the Controller panel with the mouse, or W A S D."
            );
            self.shell.messages.push(Level::Error, text.clone());
            self.status = text;
        }
    }

    /// The panel.
    pub(in crate::app) fn controller_panel(&mut self, ui: &mut Ui) {
        let running = self.panels.live.running;
        let held = self.panels.live.keys;
        let st = &mut self.panels.controller;
        st.drawn = true;
        // nothing else wakes the window for a controller
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));

        let current = st.choice.clone();
        let mut picked = None;
        let combo = egui::ComboBox::from_id_salt("controller-choice")
            .selected_text(current.label())
            .width(ui.available_width().min(260.0))
            .show_ui(ui, |ui| {
                let mut row = |ui: &mut Ui, c: Choice| {
                    if ui.selectable_label(c == current, c.label()).clicked() {
                        picked = Some(c);
                    }
                };
                row(ui, Choice::None);
                for l in Layout::ALL {
                    row(ui, Choice::Simulated(l));
                }
                for (n, name) in &st.pads {
                    row(ui, Choice::Plugged(*n, name.clone()));
                }
            });
        combo.response.tag("panel.controller.choice", "Controller");
        if let Some(c) = picked {
            st.pick(c);
        }

        ui.add_space(4.0);
        let mut keys = Held::default();
        draw_keys(ui, &mut keys, running.then_some(held));
        st.mouse_keys = keys;

        if let Some(layout) = st.choice.layout() {
            ui.add_space(6.0);
            let real = st.real;
            st.mouse = draw_pad(ui, layout, &real);
        } else {
            st.mouse = Inputs::default();
        }

        ui.add_space(4.0);
        let source = match (&st.no_pads, &st.choice) {
            (Some(e), Choice::None) | (Some(e), Choice::Simulated(_)) => format!("Simulated: {e}."),
            (_, Choice::None) => "W A S D only; controllers are ignored.".to_string(),
            (_, Choice::Simulated(_)) if st.pads.is_empty() => {
                "Simulated: press and drag with the mouse. No controller is plugged in.".to_string()
            }
            (_, Choice::Simulated(_)) => {
                "Simulated with the mouse; a controller plugged in shows here too.".to_string()
            }
            (_, Choice::Plugged(..)) => "Live from the controller, and the mouse works too.".to_string(),
        };
        ui.label(egui::RichText::new(source).small());
        let what = if running {
            "A take is running: it reads W A S D, Shift and the left stick. The other controls only show for now."
        } else {
            "Start a take under Pose fill to steer with these: W A S D, Shift and the left stick. The other controls only show for now, and outside a take the keys keep their shortcuts."
        };
        ui.label(egui::RichText::new(what).small().weak());
    }
}

/// A lit fill and the text on it, from the theme.
fn colours(ui: &Ui, lit: bool) -> (Color32, Color32) {
    let v = ui.visuals();
    if lit {
        (v.selection.bg_fill, v.selection.stroke.color)
    } else {
        (v.widgets.inactive.bg_fill, v.widgets.inactive.fg_stroke.color)
    }
}

fn draw_keys(ui: &mut Ui, mouse: &mut Held, taken: Option<Held>) {
    let size = 30.0_f32.min((ui.available_width() - 12.0) / 6.0).max(16.0);
    let gap = 3.0;
    let (rect, _) = ui.allocate_exact_size(vec2(size * 6.0, size * 2.0 + gap), Sense::hover());
    let down = |ui: &Ui, key: Key| ui.input(|i| i.key_down(key));
    let shift = ui.input(|i| i.modifiers.shift);
    let t = taken.unwrap_or_default();
    let col = |n: f32, row: f32| rect.min + vec2(n * size, row * (size + gap));
    let cell = vec2(size - gap, size);
    let rows = [
        ("W", "w", col(2.0, 0.0), cell, down(ui, Key::W) || t.w),
        ("A", "a", col(1.0, 1.0), cell, down(ui, Key::A) || t.a),
        ("S", "s", col(2.0, 1.0), cell, down(ui, Key::S) || t.s),
        ("D", "d", col(3.0, 1.0), cell, down(ui, Key::D) || t.d),
        (
            "Shift",
            "shift",
            col(4.0, 1.0),
            vec2(size * 2.0 - gap, size),
            shift || t.shift,
        ),
    ];
    for (text, id, at, cell, real) in rows {
        let r = ui
            .interact(
                Rect::from_min_size(at, cell),
                ui.scope_id().with(id),
                Sense::drag(),
            )
            .tag(format!("panel.controller.key.{id}"), text);
        let by_mouse = r.is_pointer_button_down_on();
        match id {
            "w" => mouse.w = by_mouse,
            "a" => mouse.a = by_mouse,
            "s" => mouse.s = by_mouse,
            "d" => mouse.d = by_mouse,
            _ => mouse.shift = by_mouse,
        }
        let (fill, ink) = colours(ui, real || by_mouse);
        ui.painter().rect_filled(r.rect, 4.0, fill);
        ui.painter().text(
            r.rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(12.0),
            ink,
        );
    }
}

/// Draw the controller lit by `real`; returns what the mouse holds this frame.
fn draw_pad(ui: &mut Ui, layout: Layout, real: &Inputs) -> Inputs {
    let width = ui.available_width().clamp(200.0, 460.0);
    let scale = width / WIDE;
    let (rect, _) = ui.allocate_exact_size(vec2(width, TALL * scale), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        10.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let mut mouse = Inputs::default();
    for p in layout.parts() {
        let centre = rect.min + vec2(p.at[0], p.at[1]) * scale;
        let area = Rect::from_center_size(centre, vec2(p.half[0], p.half[1]) * 2.0 * scale);
        let r = ui
            .interact(area, ui.scope_id().with(("pad", p.id)), Sense::drag())
            .tag(format!("panel.controller.{}", p.id), p.id);
        let down = r.is_pointer_button_down_on();
        let at = r.interact_pointer_pos().filter(|_| down);
        match p.control {
            Control::Stick(i) => {
                let reach = p.half[0] * scale;
                let by_mouse = at.map_or([0.0; 2], |at| stick_value(centre, reach, at));
                if let Some(s) = mouse.sticks.get_mut(i) {
                    *s = by_mouse;
                }
                let tilt = combine(real.sticks.get(i).copied().unwrap_or_default(), by_mouse);
                paint_stick(ui, centre, reach, tilt, down);
                // not while held: it would cover the buttons below
                if !down {
                    r.on_hover_text("Hold and drag; it springs back when you let go");
                }
            }
            Control::Trigger(i) => {
                let by_mouse = at.map_or(0.0, |at| trigger_value(area.top(), area.height(), at.y));
                if let Some(t) = mouse.triggers.get_mut(i) {
                    *t = by_mouse;
                }
                let pull = by_mouse.max(real.triggers.get(i).copied().unwrap_or(0.0));
                let (fill, ink) = colours(ui, false);
                ui.painter().rect_filled(area, 3.0, fill);
                let mut bar = area;
                bar.set_height(area.height() * pull);
                ui.painter().rect_filled(bar, 3.0, ui.visuals().selection.bg_fill);
                paint_mark(ui, area, p.mark, ink);
                if !down {
                    r.on_hover_text("Drag down it for a partial pull");
                }
            }
            Control::Button(b) => {
                if down {
                    mouse.press(b);
                }
                let (fill, ink) = colours(ui, down || real.pressed(b));
                ui.painter()
                    .rect_filled(area, area.height().min(area.width()) / 2.5, fill);
                paint_mark(ui, area, p.mark, ink);
            }
        }
    }
    mouse
}

fn paint_stick(ui: &Ui, centre: Pos2, reach: f32, tilt: [f32; 2], down: bool) {
    let moved = tilt[0].hypot(tilt[1]) > 0.02;
    let (well, _) = colours(ui, false);
    let (thumb, _) = colours(ui, down || moved);
    ui.painter().circle_filled(centre, reach, well);
    let at = centre + vec2(tilt[0], -tilt[1]) * reach * 0.55;
    ui.painter().circle(
        at,
        reach * 0.45,
        thumb,
        Stroke::new(1.0, ui.visuals().widgets.active.fg_stroke.color),
    );
}

fn paint_mark(ui: &Ui, area: Rect, mark: Mark, ink: Color32) {
    let p = ui.painter();
    let c = area.center();
    let s = area.height().min(area.width()) * 0.3;
    let line = Stroke::new(1.5, ink);
    let poly = |points: &[Vec2]| {
        let mut v: Vec<Pos2> = points.iter().map(|o| c + *o * s).collect();
        if let Some(first) = v.first().copied() {
            v.push(first);
        }
        p.add(egui::Shape::line(v, line));
    };
    match mark {
        Mark::Text(t) => {
            let size = (area.height() * 0.6).clamp(8.0, 14.0);
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                t,
                egui::FontId::proportional(size),
                ink,
            );
        }
        Mark::Triangle => poly(&[vec2(0.0, -1.0), vec2(1.0, 0.8), vec2(-1.0, 0.8)]),
        Mark::Square => poly(&[vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0)]),
        Mark::Circle => {
            p.circle_stroke(c, s, line);
        }
        Mark::Cross => {
            p.line_segment([c + vec2(-s, -s), c + vec2(s, s)], line);
            p.line_segment([c + vec2(s, -s), c + vec2(-s, s)], line);
        }
        Mark::Arrow(turn) => {
            let (sin, cos) = turn.sin_cos();
            let spin = |x: f32, y: f32| vec2(x * cos - y * sin, x * sin + y * cos);
            poly(&[spin(0.0, -1.0), spin(1.0, 0.7), spin(-1.0, 0.7)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn rect(p: &Part) -> Rect {
        Rect::from_center_size(pos2(p.at[0], p.at[1]), vec2(p.half[0], p.half[1]) * 2.0)
    }

    #[test]
    fn a_stick_follows_the_pointer_and_stays_in_its_circle() {
        let c = pos2(100.0, 100.0);
        assert_eq!(stick_value(c, 10.0, pos2(105.0, 100.0)), [0.5, 0.0]);
        // up the screen is forward
        assert_eq!(stick_value(c, 10.0, pos2(100.0, 90.0)), [0.0, 1.0]);
        let far = stick_value(c, 10.0, pos2(200.0, 200.0));
        assert!((far[0].hypot(far[1]) - 1.0).abs() < 1e-5, "{far:?}");
        assert!(far[0] > 0.0 && far[1] < 0.0);
    }

    #[test]
    fn a_stick_refuses_hostile_numbers() {
        let c = pos2(0.0, 0.0);
        assert_eq!(stick_value(c, 10.0, pos2(f32::NAN, 0.0)), [0.0; 2]);
        assert_eq!(stick_value(c, 10.0, pos2(f32::INFINITY, 0.0)), [0.0; 2]);
        assert_eq!(stick_value(c, 0.0, pos2(1.0, 0.0)), [0.0; 2]);
    }

    #[test]
    fn the_larger_tilt_wins() {
        assert_eq!(combine([0.2, 0.0], [0.0, 0.9]), [0.0, 0.9]);
        assert_eq!(combine([0.9, 0.0], [0.0, 0.2]), [0.9, 0.0]);
        // let go: what the real stick says, so nothing stays held
        assert_eq!(combine([0.3, 0.0], [0.0, 0.0]), [0.3, 0.0]);
    }

    #[test]
    fn no_controller_ignores_the_sticks() {
        let mut st = ControllerState::default();
        st.real.sticks[0] = [1.0, 0.0];
        assert_eq!(st.steer(), [1.0, 0.0]);
        st.choice = Choice::None;
        assert_eq!(st.steer(), [0.0, 0.0]);
    }

    #[test]
    fn a_trigger_pulls_down_its_length() {
        assert_eq!(trigger_value(10.0, 20.0, 20.0), 0.5);
        assert_eq!(trigger_value(10.0, 20.0, 0.0), 0.0);
        assert_eq!(trigger_value(10.0, 20.0, 99.0), 1.0);
        assert_eq!(trigger_value(10.0, 0.0, f32::NAN), 0.0);
    }

    #[test]
    fn every_layout_fits_with_no_control_on_another() {
        let frame = Rect::from_min_size(pos2(0.0, 0.0), vec2(WIDE, TALL));
        for l in Layout::ALL {
            let parts = l.parts();
            assert!(parts.iter().any(|p| p.control == Control::Stick(0)), "{l:?}");
            for (i, a) in parts.iter().enumerate() {
                assert!(frame.contains_rect(rect(a)), "{l:?} {} is outside", a.id);
                for b in parts.iter().skip(i + 1) {
                    assert!(!rect(a).intersects(rect(b)), "{l:?}: {} on {}", a.id, b.id);
                    assert_ne!(a.id, b.id);
                }
            }
        }
    }

    #[test]
    fn a_controller_gets_the_drawing_of_its_name() {
        assert_eq!(Layout::guess("DualSense Wireless"), Layout::PlayStation);
        assert_eq!(Layout::guess("Xbox Series Controller"), Layout::Xbox);
        assert_eq!(Layout::guess("Some Pad"), Layout::Xbox);
    }
}
