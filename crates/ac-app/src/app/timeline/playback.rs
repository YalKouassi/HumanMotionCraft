//! Playback: speed (0.25x to 2x), real time (frames are skipped to keep time) or every frame
//! (each frame shown, slower if drawing can't keep up), and a play range (in and out points,
//! dragged on the ruler). Looping is the animation's own switch (`clip.loop`): looped, playback
//! wraps from the out point to the in point; not looped, it stops on the out point.

use serde_json::json;

use super::super::App;

pub const SPEEDS: &[f64] = &[0.25, 0.5, 1.0, 1.5, 2.0];

/// Window state, not saved with the animation (worth keeping in the app's preferences).
#[derive(Debug, Clone)]
pub struct Playback {
    pub speed: f64,
    /// Show every frame instead of keeping to real time.
    pub every_frame: bool,
    /// In and out points (whole frames, in <= out); `None`: the whole animation.
    pub range: Option<(i64, i64)>,
    /// Frames owed to every-frame playback (at most one is carried, so none is skipped).
    owed: f64,
}

impl Default for Playback {
    fn default() -> Self {
        Playback {
            speed: 1.0,
            every_frame: false,
            range: None,
            owed: 0.0,
        }
    }
}

impl Playback {
    /// The frames playback runs over: the range, inside the animation `0..len`.
    pub fn span(&self, len: i64) -> (f64, f64) {
        let last = (len - 1).max(0);
        match self.range {
            Some((a, b)) => {
                let a = a.clamp(0, last);
                (a as f64, b.clamp(a, last) as f64)
            }
            None => (0.0, last as f64),
        }
    }

    /// Set an in or out point; the other end moves with it when they would cross.
    pub fn set_point(&mut self, out: bool, frame: i64, len: i64) {
        let f = frame.max(0);
        let last = (len - 1).max(0);
        let (a, b) = self.range.unwrap_or((0, last));
        self.range = Some(if out { (a.min(f), f) } else { (f, b.max(f)) });
    }

    /// The next frame from `now`, `dt` seconds later at `fps`; `None`: stop (end reached
    /// without looping).
    pub fn advance(&mut self, now: f64, dt: f64, fps: f64, len: i64, looping: bool) -> Option<f64> {
        let (a, b) = self.span(len);
        if now < a || now > b {
            self.owed = 0.0;
            return Some(a);
        }
        let step = dt.max(0.0) * fps * self.speed;
        let mut f = if self.every_frame {
            self.owed = (self.owed + step).min(1.0);
            if self.owed < 1.0 {
                return Some(now);
            }
            self.owed -= 1.0;
            now.floor() + 1.0
        } else {
            now + step
        };
        if f > b {
            if !looping {
                return None;
            }
            let period = b - a + 1.0;
            f = a + (f - a).rem_euclid(period.max(1.0));
            if f > b {
                f = a;
            }
        }
        Some(f)
    }
}

impl App {
    /// Called once a frame: move the playhead while playing.
    pub(in crate::app) fn advance_playback(&mut self, ctx: &egui::Context) {
        if !self.playing {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt) as f64;
        let doc = &self.session.document;
        let len = doc.clip.length_with(&doc.links).max(1);
        let (now, fps, looping) = (doc.frame, doc.clip.fps, doc.clip.loop_);
        match self.timeline.playback.advance(now, dt, fps, len, looping) {
            Some(f) => {
                if f != now {
                    self.run("frame.set", json!({ "frame": f }));
                }
            }
            None => {
                let (_, b) = self.timeline.playback.span(len);
                self.run("frame.set", json!({ "frame": b }));
                self.playing = false;
            }
        }
        ctx.request_repaint();
    }

    /// Play or pause; playing from the end of an unlooped animation starts again from the in point.
    pub(in crate::app) fn toggle_play(&mut self) {
        self.playing = !self.playing;
        let doc = &self.session.document;
        let len = doc.clip.length_with(&doc.links).max(1);
        let (a, b) = self.timeline.playback.span(len);
        if self.playing && !doc.clip.loop_ && doc.frame >= b {
            self.run("frame.set", json!({ "frame": a }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_time_wraps_or_stops_at_the_out_point() {
        let mut p = Playback {
            range: Some((10, 20)),
            ..Playback::default()
        };
        assert_eq!(p.advance(5.0, 0.1, 30.0, 100, true), Some(10.0));
        assert_eq!(p.advance(10.0, 0.1, 30.0, 100, true), Some(13.0));
        assert_eq!(p.advance(19.0, 0.1, 30.0, 100, true), Some(11.0));
        assert_eq!(p.advance(19.0, 0.1, 30.0, 100, false), None);
        p.speed = 0.5;
        assert_eq!(p.advance(10.0, 0.1, 30.0, 100, true), Some(11.5));
    }

    #[test]
    fn every_frame_never_skips() {
        let mut p = Playback {
            every_frame: true,
            ..Playback::default()
        };
        // a slow frame (0.5 s) still moves one frame only
        assert_eq!(p.advance(3.0, 0.5, 30.0, 100, true), Some(4.0));
        // a quick one owes a third of a frame: stays
        assert_eq!(p.advance(4.0, 0.011, 30.0, 100, true), Some(4.0));
    }

    #[test]
    fn points_keep_in_before_out() {
        let mut p = Playback::default();
        p.set_point(true, 30, 100);
        assert_eq!(p.range, Some((0, 30)));
        p.set_point(false, 40, 100);
        assert_eq!(p.range, Some((40, 40)));
        assert_eq!(p.span(20), (19.0, 19.0));
    }
}
