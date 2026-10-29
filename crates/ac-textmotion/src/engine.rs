//! The seam between the commands and whatever makes the motion: [`Engine`]. The real one is the
//! helper process ([`crate::helper::Helper`]); tests put a fake in its place with [`use_engine`].
//!
//! One engine serves the whole process, behind a lock: a request waits for the one before it. The
//! last [`CACHE`] results are kept by their request, so a front end can make the motion off its UI
//! thread ([`make`]) and then run the command, which finds it ready: the same request gives the
//! same motion (the seed is part of it).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

use ac_core::quat::{Quat, Vec3};
use serde::Serialize;

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

/// What the engine says about itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// The process is up.
    pub running: bool,
    /// The model is in memory (the next request starts at once).
    pub loaded: bool,
    /// The model's name.
    pub model: String,
}

/// Something that makes motion from text. `say` reports progress while it works.
pub trait Engine: Send {
    fn status(&self) -> Status;
    fn load(&mut self, say: &mut dyn FnMut(&str)) -> Result<(), String>;
    fn generate(&mut self, req: &Request, say: &mut dyn FnMut(&str)) -> Result<Motion, String>;
}

/// Results kept by their request.
pub const CACHE: usize = 16;

type Slot = Option<Box<dyn Engine>>;

static ENGINE: Mutex<Slot> = Mutex::new(None);
static MADE: Mutex<VecDeque<(String, Arc<Motion>)>> = Mutex::new(VecDeque::new());
static PROGRESS: Mutex<String> = Mutex::new(String::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // a panic while holding these leaves nothing half-written that matters: carry on
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Use `engine` from now on (tests: a fake). The cache is emptied.
pub fn use_engine(engine: Box<dyn Engine>) {
    *lock(&ENGINE) = Some(engine);
    lock(&MADE).clear();
}

/// Stop the engine (the helper process ends); the next request starts it again.
pub fn stop() -> bool {
    lock(&ENGINE).take().is_some()
}

/// The engine's status; `None` while it is busy with a request.
pub fn status() -> Option<Status> {
    match ENGINE.try_lock() {
        Ok(g) => Some(g.as_ref().map(|e| e.status()).unwrap_or_default()),
        Err(TryLockError::Poisoned(p)) => {
            Some(p.into_inner().as_ref().map(|e| e.status()).unwrap_or_default())
        }
        Err(TryLockError::WouldBlock) => None,
    }
}

/// The last progress line of the request being worked on ("" when none).
pub fn progress() -> String {
    lock(&PROGRESS).clone()
}

fn key(req: &Request) -> Result<String, String> {
    serde_json::to_string(req).map_err(|e| e.to_string())
}

/// The motion for `req` when it was made already.
pub fn ready(req: &Request) -> Option<Arc<Motion>> {
    let k = key(req).ok()?;
    lock(&MADE).iter().find(|(m, _)| *m == k).map(|(_, v)| v.clone())
}

/// Run `f` on the engine, starting the helper first when there is none.
fn with_engine<T>(
    f: impl FnOnce(&mut dyn Engine, &mut dyn FnMut(&str)) -> Result<T, String>,
) -> Result<T, String> {
    let mut slot = lock(&ENGINE);
    if slot.is_none() {
        let install = crate::install::find()?;
        *slot = Some(Box::new(crate::helper::Helper::start(&install)?));
    }
    let engine = slot.as_mut().ok_or("no engine")?;
    let mut say = |s: &str| *lock(&PROGRESS) = s.to_string();
    let out = f(engine.as_mut(), &mut say);
    lock(&PROGRESS).clear();
    if out.is_err() && !engine.status().running {
        *slot = None; // the helper ended: the next request starts a new one
    }
    out
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
    let mut made = lock(&MADE);
    made.push_back((key(req)?, motion.clone()));
    while made.len() > CACHE {
        made.pop_front();
    }
    Ok(motion)
}

/// The motion has the frames and bones asked for, and every number is finite.
fn check(req: &Request, m: &Motion) -> Result<(), String> {
    let bones = req
        .start
        .as_ref()
        .map(|h| h.rotations.len())
        .or_else(|| m.rotations.first().map(Vec::len))
        .unwrap_or(0);
    if m.rotations.len() != req.frames || m.roots.len() != req.frames {
        return Err(format!(
            "the helper made {} frames of rotations and {} roots; {} were asked for",
            m.rotations.len(),
            m.roots.len(),
            req.frames
        ));
    }
    if m.rotations.iter().any(|f| f.len() != bones) {
        return Err(format!("the helper's frames don't all have {bones} bones"));
    }
    let finite = m
        .rotations
        .iter()
        .flatten()
        .flatten()
        .chain(m.roots.iter().flatten())
        .all(|x| x.is_finite());
    if !finite {
        return Err("the helper's motion has numbers that are not finite".into());
    }
    Ok(())
}
