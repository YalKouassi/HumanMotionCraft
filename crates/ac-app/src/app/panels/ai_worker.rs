//! AI preview fills off the UI thread. The window hands the worker a copy of the document and the
//! span; the worker runs the same `Document::ai_preview` the command runs (ac-ai's pipeline) and
//! sends the preview back. The UI thread then puts it in `Document::ai_precomputed` and runs
//! `ai.preview`, which takes it only when the clip, links, settings, span and mode still match:
//! the result is the one the synchronous path gives, it just never blocks a frame.
//!
//! Only the newest request counts: a request not started yet is replaced by a newer one
//! (superseded while a slider moves), and a finished one that is no longer the newest is dropped.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use ac_core::Document;
use ac_core::ai::Preview;

/// One preview to work out.
struct Job {
    generation: u64,
    doc: Document,
    span: Option<(i64, i64)>,
    plain: bool,
}

/// A finished preview (or why it failed) and how long it took.
pub struct Done {
    pub generation: u64,
    pub result: Result<Arc<Preview>, String>,
    pub took: Duration,
}

/// The request still being worked out.
#[derive(Clone, Copy, Debug)]
pub struct Pending {
    pub since: Instant,
    pub plain: bool,
}

/// The newest job not started yet, and whether the window has gone.
#[derive(Default)]
struct Slot {
    job: Option<Job>,
    closed: bool,
}

type Shared = Arc<(Mutex<Slot>, Condvar)>;

pub struct Worker {
    shared: Shared,
    done: mpsc::Receiver<Done>,
    generation: u64,
    pub pending: Option<Pending>,
}

impl Worker {
    /// Start the worker thread; `wake` is called after each finished job (a repaint).
    pub fn start(wake: impl Fn() + Send + 'static) -> Result<Worker, String> {
        let shared: Shared = Arc::new((Mutex::new(Slot::default()), Condvar::new()));
        let (tx, rx) = mpsc::channel();
        let theirs = shared.clone();
        std::thread::Builder::new()
            .name("ai-preview".into())
            .spawn(move || serve(&theirs, &tx, &wake))
            .map_err(|e| format!("can't start the AI preview thread: {e}"))?;
        Ok(Worker {
            shared,
            done: rx,
            generation: 0,
            pending: None,
        })
    }

    /// Ask for a preview of `doc` (a copy of the session's document). A request still waiting is
    /// dropped: it was for older settings.
    pub fn submit(&mut self, doc: Document, span: Option<(i64, i64)>, plain: bool) {
        self.generation += 1;
        let job = Job {
            generation: self.generation,
            doc,
            span,
            plain,
        };
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(PoisonError::into_inner).job = Some(job);
        wake.notify_one();
        self.pending = Some(Pending {
            since: Instant::now(),
            plain,
        });
    }

    /// Forget the request in flight: its result will be dropped.
    pub fn cancel(&mut self) {
        self.generation += 1;
        let (lock, _) = &*self.shared;
        lock.lock().unwrap_or_else(PoisonError::into_inner).job = None;
        self.pending = None;
    }

    /// The newest request's result once it is ready (older results are dropped).
    pub fn poll(&mut self) -> Option<Done> {
        let mut newest = None;
        while let Ok(d) = self.done.try_recv() {
            if d.generation == self.generation {
                newest = Some(d);
            }
        }
        if newest.is_some() {
            self.pending = None;
        }
        newest
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        lock.lock().unwrap_or_else(PoisonError::into_inner).closed = true;
        wake.notify_all();
    }
}

/// The worker thread: take the newest job, work it out, send it back; stop when the window goes.
fn serve(shared: &Shared, tx: &mpsc::Sender<Done>, wake: &dyn Fn()) {
    let (lock, signal) = &**shared;
    loop {
        let job = {
            let mut slot = lock.lock().unwrap_or_else(PoisonError::into_inner);
            while slot.job.is_none() && !slot.closed {
                slot = signal.wait(slot).unwrap_or_else(PoisonError::into_inner);
            }
            if slot.closed {
                return;
            }
            slot.job.take()
        };
        let Some(job) = job else { continue };
        let started = Instant::now();
        let result = compute(job.doc, job.span, job.plain);
        let done = Done {
            generation: job.generation,
            result,
            took: started.elapsed(),
        };
        if tx.send(done).is_err() {
            return;
        }
        wake();
    }
}

/// The preview `ai.preview` makes, worked out on a copy of the document.
pub fn compute(mut doc: Document, span: Option<(i64, i64)>, plain: bool) -> Result<Arc<Preview>, String> {
    doc.ai_precomputed = None;
    match catch_unwind(AssertUnwindSafe(|| doc.ai_preview(span, plain))) {
        Ok(r) => r,
        Err(_) => Err("the AI fill panicked (a bug): the animation is unchanged".into()),
    }
}

/// Whether a finished preview still fits the document (else `ai.preview` would work it out again
/// on the UI thread: drop it instead).
pub fn fits(doc: &Document, p: &Preview) -> bool {
    p.base == doc.clip && p.links == doc.links && p.settings == doc.ai_settings
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    fn session() -> ac_core::Session {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let rig = std::fs::read_to_string(assets.join("rig/default_rig.json")).unwrap();
        let mut doc = Document::new(ac_core::Skeleton::from_json(&rig).unwrap());
        doc.ai = ac_ai::shared(&assets).unwrap();
        assert!(doc.ai.is_some(), "the test needs the AI model in assets/models");
        let mut s = ac_core::Session::new(doc);
        assert!(ac_io::register(&mut s).is_empty());
        for (id, p) in [
            ("physics.set", json!({ "on": false })),
            ("key.set", json!({})),
            ("frame.set", json!({ "frame": 24 })),
            ("pose.set_root", json!({ "root": [0.4, -0.6, 1.1] })),
            (
                "pose.set_bone",
                json!({ "bone": "upperarm01.L", "rotation": [0.3, 0.1, 0.0, 0.948683] }),
            ),
            ("key.set", json!({})),
            ("frame.set", json!({ "frame": 9 })),
            ("ai.settings", json!({ "ai_force": 1.2, "trajectory": 0.5 })),
        ] {
            s.run(id, &p).unwrap();
        }
        s
    }

    /// A preview worked out on the worker thread is exactly the synchronous one, and the
    /// session takes it without working it out again.
    #[test]
    fn the_worker_gives_the_synchronous_result() {
        for plain in [false, true] {
            let mode = if plain { "plain" } else { "ai" };
            let mut sync = session();
            let want = sync.run("ai.preview", &json!({ "mode": mode })).unwrap();
            let want_clip = sync.document.ai_preview.clone().unwrap().clip.clone();

            let mut s = session();
            let (tx, rx) = mpsc::channel();
            let mut w = Worker::start(move || {
                let _ = tx.send(());
            })
            .unwrap();
            w.submit(s.document.clone(), None, plain);
            rx.recv_timeout(Duration::from_secs(120)).unwrap();
            let done = w.poll().expect("the newest result");
            let p = done.result.unwrap();
            assert_eq!(p.clip, want_clip, "{mode}: the worker's fill differs");
            assert!(fits(&s.document, &p));

            // applied through the command: the same result, and it is the worker's preview
            let (from, to) = (p.from, p.to);
            s.document.ai_precomputed = Some(p.clone());
            let got = s
                .run("ai.preview", &json!({ "from": from, "to": to, "mode": mode }))
                .unwrap();
            assert_eq!(got, want, "{mode}");
            assert!(Arc::ptr_eq(s.document.ai_preview.as_ref().unwrap(), &p));
            assert!(s.document.ai_precomputed.is_none());
        }
    }

    /// Only the newest request is answered; an older one is dropped.
    #[test]
    fn a_newer_request_supersedes_an_older_one() {
        let s = session();
        let (tx, rx) = mpsc::channel();
        let mut w = Worker::start(move || {
            let _ = tx.send(());
        })
        .unwrap();
        w.submit(s.document.clone(), None, false);
        w.submit(s.document.clone(), None, true);
        let mut got = None;
        while got.is_none() {
            rx.recv_timeout(Duration::from_secs(120)).unwrap();
            got = w.poll();
        }
        assert!(
            got.unwrap().result.unwrap().plain,
            "the newest (plain) request wins"
        );
        assert!(w.pending.is_none());
    }

    /// A preview made before the settings changed is not taken: it no longer fits.
    #[test]
    fn a_stale_preview_does_not_fit() {
        let mut s = session();
        let p = compute(s.document.clone(), None, false).unwrap();
        s.run("ai.settings", &json!({ "head": 0.7 })).unwrap();
        assert!(!fits(&s.document, &p));
    }
}
