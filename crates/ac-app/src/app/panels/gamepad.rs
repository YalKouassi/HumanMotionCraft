//! A controller for live motion: the left stick, and word when it is unplugged. Windows only for
//! now (the crate needs no system libraries there); elsewhere [`Pad::new`] says so and the keys
//! still steer.

/// What the controller is doing this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadState {
    /// Right and forward, -1 to 1.
    pub stick: [f32; 2],
    /// The name of a controller just unplugged.
    pub lost: Option<String>,
}

#[cfg(windows)]
pub struct Pad {
    gilrs: gilrs::Gilrs,
    active: Option<gilrs::GamepadId>,
}

#[cfg(windows)]
impl Pad {
    pub fn new() -> Result<Pad, String> {
        let gilrs = gilrs::Gilrs::new().map_err(|e| format!("can't read controllers: {e}"))?;
        let active = gilrs.gamepads().next().map(|(id, _)| id);
        Ok(Pad { gilrs, active })
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
                }
                gilrs::EventType::AxisChanged(..) | gilrs::EventType::ButtonPressed(..) => {
                    // the controller in use is the one last touched
                    self.active = Some(id);
                }
                _ => {}
            }
        }
        let stick = self.active.map_or([0.0; 2], |id| {
            let pad = self.gilrs.gamepad(id);
            [
                pad.value(gilrs::Axis::LeftStickX),
                pad.value(gilrs::Axis::LeftStickY),
            ]
        });
        let finite = stick.iter().all(|x| x.is_finite());
        PadState {
            stick: if finite { stick } else { [0.0; 2] },
            lost,
        }
    }
}

#[cfg(not(windows))]
pub struct Pad;

#[cfg(not(windows))]
impl Pad {
    pub fn new() -> Result<Pad, String> {
        Err("controllers work on Windows only for now".into())
    }

    pub fn poll(&mut self) -> PadState {
        PadState::default()
    }
}
