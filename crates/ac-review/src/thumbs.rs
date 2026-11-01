//! Thumbnails for thousands of clips: a line figure of the clip's middle frame, drawn with the UI's
//! own lines, so no picture is rendered or kept per clip. A figure is made on a worker only when its
//! clip is on screen; what scrolled away before its turn is dropped unread.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use ac_core::quat::{Quat, Vec3};
use ac_core::rig::Skeleton;

pub type Key = (usize, usize);
/// A figure: lines in metres, x across from the hips, y up from the floor.
pub type Figure = Vec<[[f32; 2]; 2]>;
pub type Job = Box<dyn FnOnce() -> Result<Figure, String> + Send>;

const WORKERS: usize = 2;

/// The bones of a pose seen from the front (the rig faces -Y, so across is X and up is Z).
pub fn figure(skeleton: &Skeleton, rotations: &[Quat], root: Vec3) -> Result<Figure, String> {
    let (_, pos) = skeleton.fk(rotations, root).map_err(|e| e.to_string())?;
    let hips = pos.first().map_or(0.0, |p| p[0]);
    let at = |p: &Vec3| [(p[0] - hips) as f32, p[2] as f32];
    Ok(skeleton
        .parents
        .iter()
        .enumerate()
        .filter_map(|(b, parent)| Some([at(pos.get((*parent)?)?), at(pos.get(b)?)]))
        .collect())
}

/// The scale and middle that fit a figure inside a box, with a small margin: a raised arm or a
/// body lying down stays whole. A figure never grows past a standing body's height (2 m to the
/// box), so a crouch still looks smaller than a stand.
pub fn fit(fig: &Figure, size: [f32; 2]) -> (f32, [f32; 2]) {
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in fig.iter().flatten() {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    if fig.is_empty() {
        return (1.0, [0.0, 0.0]);
    }
    let room = [size[0] * 0.92, size[1] * 0.92];
    let scale = (room[1] / 2.0)
        .min(room[0] / (hi[0] - lo[0]).max(1e-3))
        .min(room[1] / (hi[1] - lo[1]).max(1e-3));
    (scale, [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0])
}

#[derive(Default)]
struct Queue {
    jobs: Vec<(Key, Job)>,
    stop: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

pub struct Thumbs {
    shared: Shared,
    results: Receiver<(Key, Result<Figure, String>)>,
    done: HashMap<Key, Result<Figure, String>>,
    /// Waiting or being made.
    wanted: HashSet<Key>,
    /// How many figures were asked of the workers, for the test that only those on screen are.
    pub started: usize,
}

fn work(shared: &Shared, out: &Sender<(Key, Result<Figure, String>)>) {
    loop {
        let (key, job) = {
            let (lock, wake) = &**shared;
            let Ok(mut q) = lock.lock() else { return };
            loop {
                if q.stop {
                    return;
                }
                if !q.jobs.is_empty() {
                    break q.jobs.remove(0);
                }
                q = match wake.wait(q) {
                    Ok(q) => q,
                    Err(_) => return,
                };
            }
        };
        if out.send((key, job())).is_err() {
            return;
        }
    }
}

impl Default for Thumbs {
    fn default() -> Self {
        Thumbs::new()
    }
}

impl Thumbs {
    pub fn new() -> Thumbs {
        let shared: Shared = Arc::default();
        let (tx, results) = channel();
        for _ in 0..WORKERS {
            let (shared, tx) = (shared.clone(), tx.clone());
            std::thread::spawn(move || work(&shared, &tx));
        }
        Thumbs {
            shared,
            results,
            done: HashMap::new(),
            wanted: HashSet::new(),
            started: 0,
        }
    }

    /// The clips on screen now: finished figures are taken in, what waits for a clip no longer on
    /// screen is dropped, and each one here with no figure yet is queued (`make`: its job, or none
    /// when it can't be read yet).
    pub fn show(&mut self, on_screen: &[Key], mut make: impl FnMut(Key) -> Option<Job>) {
        while let Ok((k, r)) = self.results.try_recv() {
            self.wanted.remove(&k);
            self.done.insert(k, r);
        }
        let (lock, wake) = &*self.shared;
        let Ok(mut q) = lock.lock() else { return };
        let wanted = &mut self.wanted;
        q.jobs.retain(|(k, _)| {
            let keep = on_screen.contains(k);
            if !keep {
                wanted.remove(k);
            }
            keep
        });
        for &k in on_screen {
            if self.done.contains_key(&k) || self.wanted.contains(&k) {
                continue;
            }
            if let Some(job) = make(k) {
                self.wanted.insert(k);
                self.started += 1;
                q.jobs.push((k, job));
            }
        }
        wake.notify_all();
    }

    pub fn get(&self, k: Key) -> Option<&Result<Figure, String>> {
        self.done.get(&k)
    }

    /// Whether figures are still coming (the window keeps drawing until they have).
    pub fn busy(&self) -> bool {
        !self.wanted.is_empty()
    }
}

impl Drop for Thumbs {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        if let Ok(mut q) = lock.lock() {
            q.stop = true;
        }
        wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(t: &mut Thumbs, keys: &[Key]) {
        let start = Instant::now();
        while t.busy() || keys.iter().any(|k| t.get(*k).is_none()) {
            assert!(start.elapsed() < Duration::from_secs(30), "figures never came");
            t.show(keys, |_| None);
            std::thread::yield_now();
        }
    }

    fn line() -> Job {
        Box::new(|| Ok(vec![[[0.0, 0.0], [0.0, 1.0]]]))
    }

    #[test]
    fn only_what_is_on_screen_is_made_and_only_once() {
        let mut t = Thumbs::new();
        // 7,500 clips, a screenful of 24 at the top
        let all: Vec<Key> = (0..7500).map(|i| (0, i)).collect();
        let screen = &all[..24];
        let mut asked = Vec::new();
        t.show(screen, |k| {
            asked.push(k);
            Some(line())
        });
        wait(&mut t, screen);
        assert_eq!(asked, screen);
        assert_eq!(t.started, 24);
        // drawn again: nothing new
        t.show(screen, |_| panic!("already there"));
        assert_eq!(t.started, 24);
        assert!(t.get((0, 24)).is_none());
    }

    #[test]
    fn what_scrolled_away_before_its_turn_is_dropped() {
        let mut t = Thumbs::new();
        // the workers are held up, so the jobs behind them stay queued
        let held = Arc::new(std::sync::Barrier::new(WORKERS + 1));
        let first: Vec<Key> = (0..WORKERS).map(|i| (1, i)).collect();
        let h = held.clone();
        t.show(&first, move |_| {
            let h = h.clone();
            let job: Job = Box::new(move || {
                h.wait();
                Ok(Vec::new())
            });
            Some(job)
        });
        // queued behind them, then off screen before a worker comes free
        let queued: Vec<Key> = (0..50).map(|i| (2, i)).collect();
        let mut on = first.clone();
        on.extend(&queued);
        t.show(&on, |_| Some(line()));
        t.show(&first, |_| panic!("the first are wanted already"));
        held.wait();
        wait(&mut t, &first);
        assert!(!t.busy());
        assert!(
            queued.iter().all(|k| t.get(*k).is_none()),
            "made after leaving the screen"
        );
        // back on screen, it is asked for again
        t.show(&queued[..1], |_| Some(line()));
        wait(&mut t, &queued[..1]);
    }

    #[test]
    fn a_wide_figure_fits_its_box() {
        let fig: Figure = vec![[[-2.0, 0.0], [2.0, 0.5]]];
        let (scale, centre) = fit(&fig, [100.0, 100.0]);
        assert_eq!(centre, [0.0, 0.25]);
        assert!(4.0 * scale <= 100.0);
        // a short figure isn't blown up to a standing body's size
        let (small, _) = fit(&vec![[[0.0, 0.0], [0.0, 1.0]]], [100.0, 100.0]);
        assert_eq!(small, 46.0);
    }

    #[test]
    fn a_figure_has_a_line_a_bone_with_a_parent_across_from_the_hips() {
        // hips at rest 1 m up, a child 1 m above them; root x = 5 hip heights of 1 m: the hips at
        // (5, 0, 1), the child at (5, 0, 2), so the line runs (0, 1) to (0, 2)
        let sk = Skeleton::new(
            vec!["hips".into(), "up".into()],
            vec![None, Some(0)],
            vec![[0.0, 0.0, 1.0], [0.0, 0.0, 2.0]],
            1.0,
        )
        .unwrap();
        let id = [0.0, 0.0, 0.0, 1.0];
        let f = figure(&sk, &[id, id], [5.0, 0.0, 0.0]).unwrap();
        assert_eq!(f, vec![[[0.0, 1.0], [0.0, 2.0]]]);
    }
}
