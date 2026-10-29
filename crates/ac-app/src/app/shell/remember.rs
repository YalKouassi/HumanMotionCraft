//! The viewport's viewing aids and the timeline's switches as preferences: read from the window
//! each frame ([`App::remembered_view`]) and put back at start-up ([`App::restore_view`]) through
//! the same `--ui` names a script uses, so a saved value that is out of range is refused the same
//! way, and said.

use super::super::App;
use super::super::prefs::{Prefs, TimelinePrefs, Viewport};
use super::super::timeline::TimeFormat;
use super::super::view_state::OnionMode;
use super::Level;

impl TimeFormat {
    /// The timeline counts time the way Settings > Units says.
    pub fn from_units(u: super::super::prefs::Units) -> TimeFormat {
        use super::super::prefs::Units;
        match u {
            Units::Frames => TimeFormat::Frames,
            Units::Seconds => TimeFormat::Seconds,
            Units::Timecode => TimeFormat::Timecode,
        }
    }

    pub fn units(self) -> super::super::prefs::Units {
        use super::super::prefs::Units;
        match self {
            TimeFormat::Frames => Units::Frames,
            TimeFormat::Seconds => Units::Seconds,
            TimeFormat::Timecode => Units::Timecode,
        }
    }
}

impl App {
    /// The viewport's aids and the timeline's switches as they are now.
    pub(super) fn remembered_view(&self) -> (Viewport, TimelinePrefs) {
        let v = &self.view;
        let viewport = Viewport {
            onion: v.onion,
            onion_count: v.onion_count,
            onion_step: v.onion_step,
            onion_mode: match v.onion_mode {
                OnionMode::Frames => "frames",
                OnionMode::Keys => "keys",
            }
            .into(),
            trails: v.trails,
            trail_span: v.trail_span,
            picker: v.picker,
            range: v.range,
            handles: if v.all_handles { "all" } else { "body" }.into(),
            gizmo: if v.local { "local" } else { "world" }.into(),
            follow: v.follow,
        };
        let tl = &self.timeline;
        let timeline = TimelinePrefs {
            speed: tl.playback.speed,
            every_frame: tl.playback.every_frame,
            contacts: tl.contacts,
            curves: tl.curves,
            retime: tl.retime,
            curve_channels: tl.curve_view.show,
        };
        (viewport, timeline)
    }

    /// Put the saved aids and switches back; a value that doesn't fit is left at its default and
    /// said.
    pub(super) fn restore_view(&mut self, p: &Prefs) {
        let v = &p.viewport;
        let t = &p.timeline;
        let on = |b: bool| if b { "on" } else { "off" }.to_string();
        let mode = if v.onion_mode == "keys" { "keys" } else { "frames" };
        let mut named: Vec<(&str, String)> = vec![
            ("onion", mode.to_string()),
            ("onion", on(v.onion)),
            ("onion_count", v.onion_count.to_string()),
            ("onion_step", v.onion_step.to_string()),
            ("trails", on(v.trails)),
            ("trail_span", v.trail_span.to_string()),
            ("picker", on(v.picker)),
            ("range", on(v.range)),
            ("handles", v.handles.clone()),
            ("gizmo", v.gizmo.clone()),
            ("follow", on(v.follow)),
            ("ortho", on(p.view.camera.ortho)),
            ("speed", t.speed.to_string()),
            ("every_frame", on(t.every_frame)),
            ("contacts", on(t.contacts)),
            ("curves", on(t.curves)),
            ("retime", on(t.retime)),
        ];
        // `onion=keys` turns it on: the on/off comes after the mode
        if v.onion_mode != "keys" && v.onion_mode != "frames" {
            named.remove(0);
        }
        for (name, value) in named {
            if let Err(e) = self.set_ui(name, &value) {
                self.shell.messages.push(
                    Level::Warning,
                    format!("settings: {name} {value:?}: {e}; the default is used"),
                );
            }
        }
        self.timeline.curve_view.show = t.curve_channels;
    }
}
