//! A live take: the stick smoothed into a desired velocity, when to ask the model for the next
//! stretch, what plays each frame, and what was recorded. No threads, clocks or engine here: the
//! commands ([`crate::live_commands`]) carry each [`StepAsk`] to the engine and bring back its
//! [`Stretch`], at once in a script and off the UI thread in the window. One frame is 1/30 s.

use ac_core::quat::{Quat, Vec3};
use serde::Serialize;

use crate::engine::Pose;

const DT: f64 = 1.0 / crate::MODEL_FPS;
/// Half-lives of the stick's smoothing, in seconds: speeding up, slowing down, turning.
const HALF_UP: f64 = 0.6;
const HALF_DOWN: f64 = 0.4;
const HALF_TURN: f64 = 0.2;
/// A speed change asks again past this share of what is left of the range, or of the top speed.
const SPEED_SHARE: f64 = 0.3;
const MIN_SPEED_SHARE: f64 = 0.1;
/// A turn asks again when the direction's dot with the last one asked for drops below this.
const TURN_DOT: f64 = 0.92;
/// The first stretch after standing aims at this share of the stick's speed, not at the smoothed
/// speed, which has barely left zero when the stick moves: such a stretch would all but stand still.
const START_SHARE: f64 = 0.6;
/// Smoothed speeds below this are standing.
const STILL: f64 = 0.05;
/// While moving or stopping, the model may take this many tokens (26 to 34 frames) to its target,
/// so the character keeps up with the stick. Standing, the helper's default applies.
pub const MOVING_TOKENS: &[i64] = &[8, 9, 10];
/// The most frames a recording holds: ten minutes.
pub const MAX_FRAMES: usize = 18_000;

/// What the person is asking for: `heading` in degrees (0 the rig's forward, -Y; positive to its
/// left) and `speed` in m/s. A style names a target of the set; none blends by speed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stick {
    pub heading: f64,
    pub speed: f64,
    pub style: Option<String>,
}

/// One request for the next stretch.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StepAsk {
    pub session: i64,
    /// Frames 1 to this of the last stretch will have been shown when the new one starts.
    pub shown: usize,
    pub velocity: [f64; 2],
    pub facing: [f64; 2],
    pub style: Option<String>,
    pub tokens: Option<Vec<i64>>,
}

/// The next stretch, on the rig; frame 0 is the last pose shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stretch {
    pub rotations: Vec<Vec<Quat>>,
    pub roots: Vec<Vec3>,
    /// The frame at which to ask again.
    pub transition: usize,
    /// What it aimed at.
    pub target: String,
    /// The model's own time.
    pub seconds: f64,
    pub notes: Vec<String>,
}

/// How the take is going.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Live,
    /// The recording reached its cap; the character stands where it was.
    Full,
    /// Over: stopped, or the helper failed (the reason).
    Ended(Option<String>),
}

pub struct Take {
    pub session: i64,
    pub state: State,
    /// Everything shown, from the start pose on.
    pub frames: Vec<Pose>,
    cap: usize,
    max_speed: f64,
    speed: f64,
    angle: f64,
    facing: f64,
    stretch: Option<Stretch>,
    /// The frame of the stretch to show next.
    played: usize,
    /// A reply and the frame of the current stretch it takes over from.
    next: Option<(Stretch, usize)>,
    pending_from: usize,
    waiting: bool,
    /// What the last request was for.
    asked_speed: f64,
    asked_dir: [f64; 2],
    asked_style: Option<String>,
    /// Requests made and why the last one was.
    pub calls: usize,
    pub why: &'static str,
    /// Frames the stretch ran out and the last pose held, while a reply was awaited.
    pub held: usize,
    /// Frames shown past the point the reply was meant to start from.
    pub late: usize,
    /// The last stretch's target, and the notes not yet told.
    pub target: String,
    pub notes: Vec<String>,
}

fn forward() -> f64 {
    (-1.0f64).atan2(0.0)
}

fn decay(x: f64, goal: f64, half: f64) -> f64 {
    goal + (x - goal) * 0.5f64.powf(DT / half)
}

fn turn_to(a: f64, goal: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let d = (goal - a + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI;
    a + d * (1.0 - 0.5f64.powf(DT / HALF_TURN))
}

fn dir(angle: f64) -> [f64; 2] {
    [angle.cos(), angle.sin()]
}

impl Take {
    pub fn new(session: i64, start: Pose, max_speed: f64, cap: usize) -> Take {
        Take {
            session,
            state: State::Live,
            frames: vec![start],
            cap: cap.clamp(2, MAX_FRAMES),
            max_speed,
            speed: 0.0,
            angle: forward(),
            facing: forward(),
            stretch: None,
            played: 1,
            next: None,
            pending_from: 1,
            waiting: false,
            asked_speed: 0.0,
            asked_dir: dir(forward()),
            asked_style: None,
            calls: 0,
            why: "",
            held: 0,
            late: 0,
            target: String::new(),
            notes: Vec::new(),
        }
    }

    /// The speed the stick has been smoothed to, m/s.
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// The pose on show.
    pub fn pose(&self) -> Option<&Pose> {
        self.frames.last()
    }

    /// Why to ask for the next stretch now, if at all, and the speed to ask for.
    fn wanted(&self, stick: &Stick, direction: [f64; 2]) -> Option<(&'static str, f64)> {
        let Some(stretch) = &self.stretch else {
            return Some(("start", self.speed));
        };
        if self.played > stretch.transition {
            return Some(("transition", self.speed));
        }
        if stick.style != self.asked_style {
            return Some(("style", self.speed));
        }
        let asked = self.asked_speed;
        if stick.speed <= 0.0 {
            return (asked > 1e-6).then_some(("stop", 0.0));
        }
        if asked <= 1e-6 {
            let aim = self.speed.max(stick.speed.min(self.max_speed) * START_SHARE);
            return Some(("started", aim));
        }
        let delta = self.speed - asked;
        let left = if delta > 0.0 {
            self.max_speed - asked
        } else {
            asked
        };
        if delta.abs() > (left * SPEED_SHARE).max(self.max_speed * MIN_SPEED_SHARE) {
            return Some(("speed", self.speed));
        }
        let dot = direction[0] * self.asked_dir[0] + direction[1] * self.asked_dir[1];
        (asked > 1e-6 && dot < TURN_DOT).then_some(("turn", self.speed))
    }

    /// Smooth the stick by one frame and say what to ask the model for, if anything. `lookahead`
    /// frames of the current stretch play on while the reply is awaited, and the new stretch takes
    /// over from them (0 when the reply comes back before the frame is shown).
    pub fn tick(&mut self, stick: &Stick, lookahead: usize) -> Option<StepAsk> {
        if self.state != State::Live {
            return None;
        }
        let goal = forward() + stick.heading.to_radians();
        self.angle = turn_to(self.angle, goal);
        let wanted = stick.speed.clamp(0.0, self.max_speed);
        let half = if wanted > self.speed { HALF_UP } else { HALF_DOWN };
        self.speed = decay(self.speed, wanted, half);
        if wanted <= 0.0 && self.speed < STILL {
            self.speed = 0.0;
        }
        if self.speed > 0.0 {
            self.facing = self.angle;
        }
        // a reply on its way, or come back and about to take over: nothing to ask yet
        if self.waiting || self.next.is_some() {
            return None;
        }
        let direction = dir(self.angle);
        let (why, aim) = self.wanted(stick, direction)?;
        if matches!(why, "started" | "stop") {
            // the smoothing carries on from what was asked, so the next frame doesn't undo it
            self.speed = aim;
        }
        let shown = match &self.stretch {
            Some(s) => (self.played - 1 + lookahead).min(s.roots.len().saturating_sub(1)),
            None => 0,
        };
        self.waiting = true;
        self.calls += 1;
        self.why = why;
        self.asked_speed = aim;
        self.asked_dir = direction;
        self.asked_style.clone_from(&stick.style);
        let short = aim > 0.0 || why == "stop";
        self.pending_from = shown + 1;
        Some(StepAsk {
            session: self.session,
            shown,
            velocity: [direction[0] * aim, direction[1] * aim],
            facing: dir(self.facing),
            style: stick.style.clone(),
            tokens: short.then(|| MOVING_TOKENS.to_vec()),
        })
    }

    /// The reply to the last [`StepAsk`].
    pub fn deliver(&mut self, stretch: Stretch) {
        self.waiting = false;
        self.target.clone_from(&stretch.target);
        for n in &stretch.notes {
            if !self.notes.contains(n) {
                self.notes.push(n.clone());
            }
        }
        self.next = Some((stretch, self.pending_from));
    }

    /// The reply failed: the take is over and what was recorded stays.
    pub fn fail(&mut self, why: String) {
        self.waiting = false;
        self.next = None;
        self.state = State::Ended(Some(why));
    }

    pub fn stop(&mut self) {
        if !matches!(self.state, State::Ended(_)) {
            self.state = State::Ended(None);
        }
        self.waiting = false;
        self.next = None;
    }

    /// Show one frame and record it. A stretch that has run out holds its last pose.
    pub fn show(&mut self) {
        if self.state != State::Live {
            return;
        }
        if self.frames.len() >= self.cap {
            self.state = State::Full;
            self.notes.push(format!(
                "the recording is full ({} frames): the character stopped; stop the take and clip what you have",
                self.frames.len()
            ));
            return;
        }
        if self
            .next
            .as_ref()
            .is_some_and(|(_, from)| self.stretch.is_none() || self.played >= *from)
            && let Some((stretch, from)) = self.next.take()
        {
            let late = if self.stretch.is_some() {
                self.played - from
            } else {
                0
            };
            self.late += late;
            self.played = 1 + late;
            self.stretch = Some(stretch);
        }
        let Some(stretch) = &self.stretch else {
            // the first stretch is still being made: the start pose holds
            if let Some(p) = self.frames.last().cloned() {
                self.frames.push(p);
            }
            self.held += 1;
            return;
        };
        let last = stretch.roots.len().saturating_sub(1);
        if self.played > last {
            self.held += 1;
        }
        let at = self.played.min(last);
        if let (Some(rotations), Some(root)) = (stretch.rotations.get(at), stretch.roots.get(at)) {
            self.frames.push(Pose {
                rotations: rotations.clone(),
                root: *root,
            });
        }
        self.played += 1;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn pose(z: f64) -> Pose {
        Pose {
            rotations: vec![[0.0, 0.0, 0.0, 1.0]],
            root: [0.0, 0.0, z],
        }
    }

    /// `n` frames whose roots count up from `base`, one a frame; the transition on `t`.
    fn stretch(base: f64, n: usize, t: usize) -> Stretch {
        Stretch {
            rotations: vec![vec![[0.0, 0.0, 0.0, 1.0]]; n],
            roots: (0..n).map(|i| [0.0, 0.0, base + i as f64]).collect(),
            transition: t,
            target: "fake".into(),
            ..Stretch::default()
        }
    }

    fn walk() -> Stick {
        Stick {
            heading: 0.0,
            speed: 1.3,
            style: None,
        }
    }

    #[test]
    fn the_first_frame_asks_and_a_reply_plays_from_its_frame_one() {
        let mut t = Take::new(1, pose(0.0), 3.6, 100);
        let ask = t.tick(&Stick::default(), 0).unwrap();
        assert_eq!((ask.session, ask.shown), (1, 0));
        assert_eq!(ask.tokens, None, "standing: the helper's default");
        assert_eq!(t.why, "start");
        t.deliver(stretch(0.0, 30, 19));
        t.show();
        assert_eq!(t.frames.last().unwrap().root[2], 1.0);
        assert_eq!(t.frames.len(), 2);
    }

    #[test]
    fn the_transition_frame_asks_again_with_the_frames_shown() {
        let mut t = Take::new(1, pose(0.0), 3.6, 100);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 30, 19));
        t.show();
        for _ in 0..18 {
            assert_eq!(t.tick(&Stick::default(), 0), None);
            t.show();
        }
        // frames 1 to 19 shown; 20 is past the transition
        let ask = t.tick(&Stick::default(), 0).unwrap();
        assert_eq!((ask.shown, t.why), (19, "transition"));
        t.deliver(stretch(19.0, 30, 19));
        t.show();
        assert_eq!(t.frames.last().unwrap().root[2], 20.0, "continues without a jump");
        assert_eq!(t.late, 0);
    }

    #[test]
    fn moving_the_stick_from_standing_asks_at_once_and_aims_at_a_real_speed() {
        let mut t = Take::new(1, pose(0.0), 3.6, 100);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 30, 19));
        t.show();
        let ask = t.tick(&walk(), 0).unwrap();
        assert_eq!(t.why, "started");
        // 0.6 of the stick's 1.3 m/s, along the rig's forward
        assert!(
            ask.velocity[0].abs() < 1e-9 && (ask.velocity[1] + 0.78).abs() < 1e-9,
            "{:?}",
            ask.velocity
        );
        assert_eq!(ask.tokens.as_deref(), Some(MOVING_TOKENS));
        t.deliver(stretch(1.0, 30, 19));
        t.show();
        assert_eq!(t.tick(&walk(), 0), None, "the next frame doesn't undo it");
        assert!(
            (t.speed() - (1.3 - 0.52 * 0.5f64.powf(1.0 / 18.0))).abs() < 1e-9,
            "smooths on from 0.78"
        );
    }

    #[test]
    fn letting_go_asks_once_for_a_stop_at_speed_zero() {
        let mut t = Take::new(1, pose(0.0), 3.6, 500);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 30, 19));
        t.show();
        t.tick(&walk(), 0).unwrap();
        t.deliver(stretch(1.0, 30, 19));
        t.show();
        let ask = t.tick(&Stick::default(), 0).unwrap();
        assert_eq!(t.why, "stop");
        assert_eq!(ask.velocity, [0.0, 0.0]);
        assert!(ask.facing[1] < -0.9);
        assert_eq!(t.speed(), 0.0);
        t.deliver(stretch(2.0, 30, 19));
        t.show();
        assert_eq!(t.tick(&Stick::default(), 0), None, "not again every frame");
    }

    #[test]
    fn a_turn_asks_again_and_the_heading_is_smoothed_to_the_left_of_forward() {
        let mut t = Take::new(1, pose(0.0), 3.6, 500);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 30, 19));
        t.show();
        t.tick(&walk(), 0).unwrap();
        t.deliver(stretch(1.0, 30, 19));
        t.show();
        let left = Stick {
            heading: 90.0,
            ..walk()
        };
        let mut asks = None;
        for _ in 0..15 {
            if let Some(a) = t.tick(&left, 0) {
                asks = Some(a);
                break;
            }
            t.show();
        }
        let a = asks.expect("a turn asks");
        assert_eq!(t.why, "turn");
        // the rig's left is +X: the direction has swung from -Y towards it
        assert!(a.velocity[0] > 0.0 && a.velocity[1] < 0.0, "{:?}", a.velocity);
    }

    #[test]
    fn a_reply_taken_ahead_starts_where_it_was_asked_from_with_nothing_lost() {
        let mut t = Take::new(1, pose(0.0), 3.6, 100);
        t.tick(&Stick::default(), 2).unwrap();
        t.show(); // the first reply isn't back: the start pose holds
        assert_eq!(t.held, 1);
        t.deliver(stretch(0.0, 30, 3));
        assert_eq!(
            t.tick(&Stick::default(), 2),
            None,
            "the reply is about to take over"
        );
        for _ in 0..3 {
            t.show();
            assert_eq!(t.tick(&Stick::default(), 2).is_some(), t.frames.len() == 5);
        }
        // frames 1 to 3 shown, past the transition: it asked from frame 3 + 2
        assert_eq!(t.why, "transition");
        t.deliver(stretch(5.0, 30, 19));
        t.show();
        t.show();
        assert_eq!(
            t.frames.last().unwrap().root[2],
            5.0,
            "the old stretch plays to the join"
        );
        t.show();
        assert_eq!(t.frames.last().unwrap().root[2], 6.0, "the new one takes over");
        assert_eq!(t.late, 0);
    }

    #[test]
    fn a_late_reply_says_so_and_skips_on_to_where_the_body_should_be() {
        let mut t = Take::new(1, pose(0.0), 3.6, 100);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 30, 0));
        t.show();
        let ask = t.tick(&Stick::default(), 0).unwrap();
        assert_eq!(ask.shown, 1);
        t.show();
        t.show();
        t.deliver(stretch(1.0, 30, 19));
        t.show();
        assert_eq!(t.late, 2);
        assert_eq!(t.frames.last().unwrap().root[2], 4.0);
    }

    #[test]
    fn a_stretch_that_runs_out_holds_its_last_pose_and_counts_it() {
        let mut t = Take::new(1, pose(0.0), 3.6, 100);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 3, 1));
        for _ in 0..4 {
            t.show();
        }
        assert_eq!(t.held, 2);
        assert_eq!(t.frames.last().unwrap().root[2], 2.0);
    }

    #[test]
    fn the_recording_stops_at_its_cap_and_says_so() {
        let mut t = Take::new(1, pose(0.0), 3.6, 3);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 30, 19));
        for _ in 0..5 {
            t.show();
        }
        assert_eq!(t.frames.len(), 3);
        assert_eq!(t.state, State::Full);
        assert_eq!(t.notes.iter().filter(|n| n.contains("full")).count(), 1);
        assert_eq!(t.tick(&walk(), 0), None);
    }

    #[test]
    fn a_failed_reply_ends_the_take_and_keeps_the_recording() {
        let mut t = Take::new(1, pose(0.0), 3.6, 100);
        t.tick(&Stick::default(), 0).unwrap();
        t.deliver(stretch(0.0, 30, 19));
        t.show();
        t.fail("the helper ended".into());
        assert_eq!(t.state, State::Ended(Some("the helper ended".into())));
        assert_eq!(t.frames.len(), 2);
        t.show();
        assert_eq!(t.frames.len(), 2);
    }

    #[test]
    fn the_stick_speed_is_held_to_the_targets_top_speed() {
        let mut t = Take::new(1, pose(0.0), 2.0, 2000);
        let fast = Stick {
            speed: 99.0,
            ..Stick::default()
        };
        for _ in 0..600 {
            t.tick(&fast, 0);
            t.waiting = false;
        }
        assert!(t.speed() <= 2.0 && t.speed() > 1.99);
    }
}
