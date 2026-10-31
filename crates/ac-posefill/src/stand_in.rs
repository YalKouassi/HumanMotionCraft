//! The engine fixture runs use in place of the helper ([`use_stand_in`]): fixtures must give the
//! same result on every machine, with or without the install. It makes no motion of its own, so a
//! fixture's numbers can be worked out by hand: every frame holds the last context pose (key A's),
//! and the root moves in a straight line from key A's root to key B's. Each result carries a note
//! saying it came from here, not the model.
//!
//! A live take holds the start pose and moves the root by the velocity, m/s taken as root units a
//! second (1/30 a frame), 30 frames a stretch with the transition on frame 19. The style `die` stands
//! for a helper that dies mid-take.
//!
//! Two targets stand for a broken helper, so fixtures can check such output is refused: a first
//! target root of `[9, 9, 9]` gives roots that are not numbers, and `[8, 8, 8]` one frame short.

use std::sync::Once;

use crate::engine::{self, Engine, Fill, Info, LiveBegun, LiveStart, Pose, Request, Status};
use crate::live::{StepAsk, Stretch};

pub const NOTE: &str = "made by the fixtures' stand-in, not the model";

#[derive(Default)]
pub struct StandIn {
    session: i64,
    start: Option<Pose>,
    /// The last stretch's roots.
    roots: Vec<[f64; 3]>,
}

impl Engine for StandIn {
    fn status(&self) -> Status {
        Status {
            running: true,
            loaded: true,
            model: "stand-in".into(),
        }
    }

    fn info(&self) -> Info {
        Info {
            plan: "built".into(),
            gpu: "none (stand-in)".into(),
        }
    }

    fn load(&mut self, _: &mut dyn FnMut(&str)) -> Result<(), String> {
        Ok(())
    }

    fn fill(&mut self, req: &Request, _: &mut dyn FnMut(&str)) -> Result<Fill, String> {
        let a = req.context.last().ok_or("no context pose")?;
        let b = req.targets.first().ok_or("no target pose")?;
        let frames = req.frames;
        if b.root == [9.0, 9.0, 9.0] {
            return Ok(Fill {
                rotations: vec![a.rotations.clone(); frames],
                roots: vec![[f64::NAN, 0.0, 0.0]; frames],
                notes: vec![],
                seconds: 0.0,
            });
        }
        if b.root == [8.0, 8.0, 8.0] {
            return Ok(Fill {
                rotations: vec![a.rotations.clone(); frames.saturating_sub(1)],
                roots: vec![a.root; frames],
                notes: vec![],
                seconds: 0.0,
            });
        }
        let last = frames.saturating_sub(1).max(1) as f64;
        let roots = (0..frames)
            .map(|i| {
                let t = i as f64 / last;
                std::array::from_fn(|c| a.root[c] + (b.root[c] - a.root[c]) * t)
            })
            .collect();
        Ok(Fill {
            rotations: vec![a.rotations.clone(); frames],
            roots,
            notes: vec![NOTE.into()],
            seconds: 0.0,
        })
    }

    fn live_start(&mut self, req: &LiveStart, _: &mut dyn FnMut(&str)) -> Result<LiveBegun, String> {
        let list = req.targets["targets"].as_array().ok_or("targets: no targets")?;
        self.session += 1;
        self.start = Some(req.start.clone());
        self.roots.clear();
        Ok(LiveBegun {
            session: self.session,
            targets: list
                .iter()
                .filter_map(|t| t["name"].as_str().map(String::from))
                .collect(),
            max_speed: list
                .iter()
                .filter_map(|t| t["speed"].as_f64())
                .fold(0.0, f64::max),
        })
    }

    fn live_step(&mut self, ask: &StepAsk, _: &mut dyn FnMut(&str)) -> Result<Stretch, String> {
        if ask.style.as_deref() == Some("die") {
            return Err("the helper ended".into());
        }
        let start = self
            .start
            .as_ref()
            .filter(|_| ask.session == self.session)
            .ok_or("no live session")?;
        let base = self.roots.get(ask.shown).copied().unwrap_or(start.root);
        let at = |i: usize, c: usize| base[c] + ask.velocity.get(c).copied().unwrap_or(0.0) * i as f64 / 30.0;
        self.roots = (0..30).map(|i| std::array::from_fn(|c| at(i, c))).collect();
        Ok(Stretch {
            rotations: vec![start.rotations.clone(); 30],
            roots: self.roots.clone(),
            transition: 19,
            target: ask.style.clone().unwrap_or_else(|| "stand-in".into()),
            seconds: 0.0,
            notes: vec![NOTE.into()],
        })
    }
}

static ONCE: Once = Once::new();

/// Put the stand-in in place of the helper for the rest of this process (fixture runs only).
pub fn use_stand_in() {
    ONCE.call_once(|| {
        engine::use_engine(Box::<StandIn>::default());
        crate::setup::use_stand_in();
    });
}
