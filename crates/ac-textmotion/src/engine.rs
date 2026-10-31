//! The seam between the commands and whatever makes the motion: [`Engine`]. The real one is the
//! helper process ([`crate::helper::Helper`]); tests put a fake in its place with [`use_engine`].
//!
//! One engine serves the whole process, behind a lock ([`ac_helper::slot::Slot`]): a request
//! waits for the one before it. The last [`CACHE`] results are kept by their request, so a front
//! end can make the motion off its UI thread ([`make`]) and then run the command, which finds it
//! ready: the same request gives the same motion (the seed is part of it).

use std::sync::Arc;

use ac_core::quat::{Quat, Vec3};
use ac_helper::slot::{Cache, Slot};
use serde::Serialize;

pub use ac_helper::Status;

/// A pose the motion is held to: every bone's rotation and the root offset (hip heights).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Held {
    pub rotations: Vec<Quat>,
    pub root: Vec3,
}

/// What to make: `frames` frames at `fps` (both ends included) of the rig in `rig`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Request {
    pub prompt: String,
    pub frames: usize,
    pub fps: f64,
    pub rig: String,
    pub seed: i64,
    /// The pose of the first frame (`None`: wherever the model starts, at the origin).
    pub start: Option<Held>,
    /// The pose of the last frame.
    pub end: Option<Held>,
}

/// The motion made: rotations `[frame][bone]`, root offsets `[frame]`, and what to tell the user.
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    pub rotations: Vec<Vec<Quat>>,
    pub roots: Vec<Vec3>,
    pub notes: Vec<String>,
}

/// Something that makes motion from text. `say` reports progress while it works.
pub trait Engine: Send {
    fn status(&self) -> Status;
    fn load(&mut self, say: &mut dyn FnMut(&str)) -> Result<(), String>;
    fn generate(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Motion, String>;
}

/// Results kept by their request.
pub const CACHE: usize = 16;

static ENGINE: Slot<dyn Engine> = Slot::new();
static MADE: Cache<Motion> = Cache::new(CACHE);

/// Use `engine` from now on (tests: a fake). The cache is emptied.
pub fn use_engine(engine: Box<dyn Engine>) {
    ENGINE.put(engine);
    MADE.clear();
}

/// Stop the engine (the helper process ends); the next request starts it again.
pub fn stop() -> bool {
    ENGINE.take()
}

/// The engine's status; `None` while it is busy with a request.
pub fn status() -> Option<Status> {
    ENGINE.peek(|e| e.map(|e| e.status()).unwrap_or_default())
}

/// The last progress line of the request being worked on ("" when none).
pub fn progress() -> String {
    ENGINE.progress()
}

fn key(req: &Request) -> Result<String, String> {
    serde_json::to_string(req).map_err(|e| e.to_string())
}

/// The motion for `req` when it was made already.
pub fn ready(req: &Request) -> Option<Arc<Motion>> {
    MADE.get(&key(req).ok()?)
}

/// Run `f` on the engine, starting the helper first when there is none.
fn with_engine<T>(
    f: impl FnOnce(&mut dyn Engine, &mut dyn FnMut(&str)) -> Result<T, String>,
) -> Result<T, String> {
    ENGINE.with(
        || {
            let install = crate::install::find()?;
            Ok(Box::new(crate::helper::Helper::start(&install)?) as Box<dyn Engine>)
        },
        |e| e.status().running,
        |e, say| f(e, say),
    )
}

/// Start the engine and load the model now (else the first request does).
pub fn load() -> Result<Status, String> {
    with_engine(|e, say| {
        e.load(say)?;
        Ok(e.status())
    })
}

/// The motion for `req`: made already, or made now (this blocks while the model works).
pub fn make(req: &Request) -> Result<Arc<Motion>, String> {
    if let Some(m) = ready(req) {
        return Ok(m);
    }
    let motion = Arc::new(with_engine(|e, say| e.generate(req, say))?);
    check(req, &motion)?;
    MADE.put(key(req)?, motion.clone());
    Ok(motion)
}

/// The motion has the frames and bones asked for, and every number is finite.
fn check(req: &Request, m: &Motion) -> Result<(), String> {
    let bones = req.start.as_ref().map(|h| h.rotations.len());
    ac_helper::frames::check(&m.rotations, &m.roots, req.frames, bones)
}
