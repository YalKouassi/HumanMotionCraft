//! A controller for live motion and the Controller panel: every stick, trigger and button, and word
//! when it is unplugged. Windows only for now (the crate needs no system libraries there); elsewhere
//! [`Pad::new`] says so and the keys still steer.

/// The buttons a controller can have, whichever layout draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    South,
    East,
    West,
    North,
    BumperL,
    BumperR,
    Up,
    Down,
    Left,
    Right,
    Start,
    Select,
    ThumbL,
    ThumbR,
}

/// How many [`Btn`]s there are.
pub const BUTTONS: usize = 14;

/// A whole controller's state, real or pressed with the mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inputs {
    /// Left then right; each right and forward, -1 to 1.
    pub sticks: [[f32; 2]; 2],
    /// Left then right, 0 to 1.
    pub triggers: [f32; 2],
    pub buttons: [bool; BUTTONS],
}

impl Inputs {
    pub fn pressed(&self, b: Btn) -> bool {
        self.buttons.get(b as usize).copied().unwrap_or(false)
    }

    pub fn press(&mut self, b: Btn) {
        if let Some(x) = self.buttons.get_mut(b as usize) {
            *x = true;
        }
    }
}

/// What the controller is doing this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadState {
    pub inputs: Inputs,
    /// The name of a controller just unplugged.
    pub lost: Option<String>,
}

/// A hostile number from a driver: not finite reads as at rest.
fn clean(x: f32) -> f32 {
    if x.is_finite() { x.clamp(-1.0, 1.0) } else { 0.0 }
}

#[cfg(windows)]
pub struct Pad {
    gilrs: gilrs::Gilrs,
    active: Option<gilrs::GamepadId>,
    /// The controller chosen in the panel; without one, the one last touched.
    pinned: bool,
}

#[cfg(windows)]
impl Pad {
    pub fn new() -> Result<Pad, String> {
        let gilrs = gilrs::Gilrs::new().map_err(|e| format!("can't read controllers: {e}"))?;
        let active = gilrs.gamepads().next().map(|(id, _)| id);
        Ok(Pad {
            gilrs,
            active,
            pinned: false,
        })
    }

    /// The controllers plugged in: a number for [`Pad::pin`], and the name each reports.
    pub fn list(&self) -> Vec<(usize, String)> {
        self.gilrs
            .gamepads()
            .map(|(id, p)| (usize::from(id), p.name().to_string()))
            .collect()
    }

    /// Read this controller and no other; `None` goes back to the one last touched.
    pub fn pin(&mut self, which: Option<usize>) {
        let found = which.and_then(|n| {
            self.gilrs
                .gamepads()
                .find(|(id, _)| usize::from(*id) == n)
                .map(|(id, _)| id)
        });
        self.pinned = found.is_some();
        if found.is_some() {
            self.active = found;
        }
    }

    pub fn poll(&mut self) -> PadState {
        let mut lost = None;
        while let Some(gilrs::Event { id, event, .. }) = self.gilrs.next_event() {
            match event {
                gilrs::EventType::Connected => {
                    self.active.get_or_insert(id);
                }
                gilrs::EventType::Disconnected if self.active == Some(id) => {
                    lost = Some(self.gilrs.gamepad(id).name().to_string());
                    self.active = None;
                    self.pinned = false;
                }
                gilrs::EventType::AxisChanged(..) | gilrs::EventType::ButtonPressed(..) if !self.pinned => {
                    // the controller in use is the one last touched
                    self.active = Some(id);
                }
                _ => {}
            }
        }
        let inputs = self
            .active
            .and_then(|id| self.gilrs.connected_gamepad(id))
            .map_or_else(Inputs::default, |pad| read(&pad));
        PadState { inputs, lost }
    }
}

#[cfg(windows)]
fn read(pad: &gilrs::Gamepad<'_>) -> Inputs {
    use gilrs::{Axis, Button};
    let axis = |a| clean(pad.value(a));
    let pull = |b| pad.button_data(b).map_or(0.0, |d| clean(d.value()).max(0.0));
    let mut out = Inputs {
        sticks: [
            [axis(Axis::LeftStickX), axis(Axis::LeftStickY)],
            [axis(Axis::RightStickX), axis(Axis::RightStickY)],
        ],
        triggers: [pull(Button::LeftTrigger2), pull(Button::RightTrigger2)],
        ..Inputs::default()
    };
    for (ours, theirs) in [
        (Btn::South, Button::South),
        (Btn::East, Button::East),
        (Btn::West, Button::West),
        (Btn::North, Button::North),
        (Btn::BumperL, Button::LeftTrigger),
        (Btn::BumperR, Button::RightTrigger),
        (Btn::Up, Button::DPadUp),
        (Btn::Down, Button::DPadDown),
        (Btn::Left, Button::DPadLeft),
        (Btn::Right, Button::DPadRight),
        (Btn::Start, Button::Start),
        (Btn::Select, Button::Select),
        (Btn::ThumbL, Button::LeftThumb),
        (Btn::ThumbR, Button::RightThumb),
    ] {
        if pad.is_pressed(theirs) {
            out.press(ours);
        }
    }
    out
}

#[cfg(not(windows))]
pub struct Pad;

#[cfg(not(windows))]
impl Pad {
    pub fn new() -> Result<Pad, String> {
        Err("controllers work on Windows only for now".into())
    }

    pub fn list(&self) -> Vec<(usize, String)> {
        Vec::new()
    }

    pub fn pin(&mut self, _which: Option<usize>) {}

    pub fn poll(&mut self) -> PadState {
        PadState::default()
    }
}
