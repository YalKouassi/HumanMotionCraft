//! The engine fixture runs use in place of the helper ([`use_stand_in`]): fixtures must give the
//! same result on every machine, with or without the install. It makes no motion of its own, so a
//! fixture's numbers can be worked out by hand: every frame holds the start pose (the rig's rest
//! without one), and the root moves in a straight line from the start's root to the end's. Each
//! result carries a note saying it came from here, not the model.

use std::sync::Once;

use crate::engine::{self, Engine, Motion, Request, Status};

pub const NOTE: &str = "made by the fixtures' stand-in, not the model";

pub struct StandIn;

impl Engine for StandIn {
    fn status(&self) -> Status {
        Status {
            running: true,
            loaded: true,
            model: "stand-in".into(),
        }
    }

    fn load(&mut self, _: &mut dyn FnMut(&str)) -> Result<(), String> {
        Ok(())
    }

    fn generate(&mut self, req: &Request, _: &mut dyn FnMut(&str)) -> Result<Motion, String> {
        let (rot, from) = match &req.start {
            Some(h) => (h.rotations.clone(), h.root),
            None => {
                let text = std::fs::read_to_string(&req.rig).map_err(|e| format!("{}: {e}", req.rig))?;
                let bones = ac_core::Skeleton::from_json(&text)
                    .map_err(|e| e.to_string())?
                    .names
                    .len();
                (vec![[0.0, 0.0, 0.0, 1.0]; bones], [0.0; 3])
            }
        };
        let to = req.end.as_ref().map_or(from, |h| h.root);
        let last = req.frames.saturating_sub(1).max(1) as f64;
        let roots = (0..req.frames)
            .map(|i| {
                let t = i as f64 / last;
                std::array::from_fn(|c| from[c] + (to[c] - from[c]) * t)
            })
            .collect();
        Ok(Motion {
            rotations: vec![rot; req.frames],
            roots,
            notes: vec![NOTE.into()],
        })
    }
}

static ONCE: Once = Once::new();

/// Put the stand-in in place of the helper for the rest of this process (fixture runs only).
pub fn use_stand_in() {
    ONCE.call_once(|| {
        engine::use_engine(Box::new(StandIn));
        crate::setup::use_stand_in();
    });
}
