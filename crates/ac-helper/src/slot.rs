//! One engine serves the whole process, behind a lock ([`Slot`]): a request waits for the one
//! before it. The last results are kept by their request ([`Cache`]), so a front end can make a
//! result off its UI thread and then run the command, which finds it ready: the same request
//! gives the same result.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // a panic while holding these leaves nothing half-written that matters: carry on
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The one engine (`E`: a crate's engine trait object), and the progress line of the request
/// being worked on.
pub struct Slot<E: ?Sized + Send> {
    engine: Mutex<Option<Box<E>>>,
    progress: Mutex<String>,
}

impl<E: ?Sized + Send> Default for Slot<E> {
    fn default() -> Self {
        Slot::new()
    }
}

impl<E: ?Sized + Send> Slot<E> {
    pub const fn new() -> Slot<E> {
        Slot {
            engine: Mutex::new(None),
            progress: Mutex::new(String::new()),
        }
    }

    /// Use `engine` from now on (tests: a fake).
    pub fn put(&self, engine: Box<E>) {
        *lock(&self.engine) = Some(engine);
    }

    /// Drop the engine (a helper process ends); returns whether there was one.
    pub fn take(&self) -> bool {
        lock(&self.engine).take().is_some()
    }

    /// `f` on the engine (or none), now; `None` while it is busy with a request.
    pub fn peek<T>(&self, f: impl FnOnce(Option<&E>) -> T) -> Option<T> {
        match self.engine.try_lock() {
            Ok(g) => Some(f(g.as_deref())),
            Err(TryLockError::Poisoned(p)) => Some(f(p.into_inner().as_deref())),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// The last progress line of the request being worked on ("" when none).
    pub fn progress(&self) -> String {
        lock(&self.progress).clone()
    }

    /// Run `f` on the engine, `start`ing one first when there is none; its progress lines go to
    /// [`Slot::progress`] meanwhile. An engine that `f` left not `up` is dropped, so the next
    /// request starts a new one.
    pub fn with<T>(
        &self,
        start: impl FnOnce() -> Result<Box<E>, String>,
        up: impl FnOnce(&E) -> bool,
        f: impl FnOnce(&mut E, &mut dyn FnMut(&str)) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut slot = lock(&self.engine);
        if slot.is_none() {
            *slot = Some(start()?);
        }
        let engine = slot.as_mut().ok_or("no engine")?;
        let mut say = |s: &str| *lock(&self.progress) = s.to_string();
        let out = f(engine.as_mut(), &mut say);
        lock(&self.progress).clear();
        if out.is_err() && !up(engine.as_ref()) {
            *slot = None;
        }
        out
    }
}

/// The last `most` results, by the request they answer.
pub struct Cache<T> {
    kept: Mutex<VecDeque<(String, Arc<T>)>>,
    most: usize,
}

impl<T> Cache<T> {
    pub const fn new(most: usize) -> Cache<T> {
        Cache {
            kept: Mutex::new(VecDeque::new()),
            most,
        }
    }

    pub fn clear(&self) {
        lock(&self.kept).clear();
    }

    /// The result for `key` when it was made already.
    pub fn get(&self, key: &str) -> Option<Arc<T>> {
        lock(&self.kept)
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    pub fn put(&self, key: String, value: Arc<T>) {
        let mut kept = lock(&self.kept);
        kept.push_back((key, value));
        while kept.len() > self.most {
            kept.pop_front();
        }
    }
}
