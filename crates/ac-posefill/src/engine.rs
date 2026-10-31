//! The seam between the commands and whatever makes the fill: [`Engine`]. The real one is the
//! helper process ([`crate::helper::Helper`]); tests put a fake in its place with [`use_engine`].
//!
//! One engine serves the whole process, behind a lock ([`ac_helper::slot::Slot`]): a request
//! waits for the one before it. The last [`CACHE`] fills are kept by their request, so a front end
//! can make them off its UI thread ([`make`]) and then run the command, which finds them ready:
//! the same request gives the same fill, and a chained fill's later requests are built from the
//! earlier fills, so the chain finds every one of them.

use std::sync::Arc;

use ac_core::quat::{Quat, Vec3};
use ac_helper::slot::{Cache, Slot};
use serde::Serialize;

pub use ac_helper::Status;

/// A pose: every bone's rotation and the root offset (hip heights).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pose {
    pub rotations: Vec<Quat>,
    pub root: Vec3,
}

/// What to make: `frames` frames at `fps` of the rig in `rig`, the first on key A (the last
/// context pose) and the last on key B (the first target).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Request {
    pub rig: String,
    pub frames: usize,
    pub fps: f64,
    /// The [`crate::CONTEXT`] poses before and on key A, oldest first.
    pub context: Vec<Pose>,
    /// Key B's pose, and up to [`crate::MAX_TARGETS`] - 1 after it.
    pub targets: Vec<Pose>,
}

/// The fill made: rotations `[frame][bone]`, root offsets `[frame]`, what to tell the user, and
/// how long the model took (0 when it doesn't say).
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub rotations: Vec<Vec<Quat>>,
    pub roots: Vec<Vec3>,
    pub notes: Vec<String>,
    pub seconds: f64,
}

/// What the engine knows about its plan and GPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    /// "built", or why not.
    pub plan: String,
    pub gpu: String,
}

/// Something that fills between poses. `say` reports progress while it works.
pub trait Engine: Send {
    fn status(&self) -> Status;
    fn info(&self) -> Info;
    fn load(&mut self, say: &mut dyn FnMut(&str)) -> Result<(), String>;
    fn fill(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Fill, String>;
}

/// Fills kept by their request (a chained fill makes one a segment).
pub const CACHE: usize = 64;

static ENGINE: Slot<dyn Engine> = Slot::new();
static MADE: Cache<Fill> = Cache::new(CACHE);

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

/// What the engine knows about its plan and GPU; `None` while it is busy, or when there is none.
pub fn info() -> Option<Info> {
    ENGINE.peek(|e| e.map(|e| e.info())).flatten()
}

/// The last progress line of the request being worked on ("" when none).
pub fn progress() -> String {
    ENGINE.progress()
}

fn key(req: &Request) -> Result<String, String> {
    serde_json::to_string(req).map_err(|e| e.to_string())
}

/// The fill for `req` when it was made already.
pub fn ready(req: &Request) -> Option<Arc<Fill>> {
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

/// The fill for `req`: made already, or made now (this blocks while the model works).
pub fn make(req: &Request) -> Result<Arc<Fill>, String> {
    if let Some(m) = ready(req) {
        return Ok(m);
    }
    let fill = Arc::new(with_engine(|e, say| e.fill(req, say))?);
    let bones = req.context.last().map(|p| p.rotations.len());
    ac_helper::frames::check(&fill.rotations, &fill.roots, req.frames, bones)?;
    MADE.put(key(req)?, fill.clone());
    Ok(fill)
}
