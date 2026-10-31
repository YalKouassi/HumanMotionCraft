//! An AI key carries the hash of the weights that made it (roadmap 155000): a filler's hash lands
//! on every key it writes, survives the clip file, and a file without one reads as empty.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::clip::{Clip, Interp, Key, KeySource, NoLinks};
use ac_core::edits::{self, Filler};
use ac_core::quat::{Quat, Vec3};

/// Holds every bone at rest and lifts the root a little a frame, under the tag "test".
struct Lift;

impl Filler for Lift {
    fn fill(&self, clip: &Clip, a: i64, b: i64) -> Result<(Vec<Vec<Quat>>, Vec<Vec3>, String), String> {
        let n = clip.bones.len();
        let frames = (b - a - 1) as usize;
        let rots = vec![vec![[0.0, 0.0, 0.0, 1.0]; n]; frames];
        let roots = (0..frames).map(|i| [0.0, 0.0, 0.01 * i as f64]).collect();
        Ok((rots, roots, "test".into()))
    }

    fn hash(&self) -> String {
        "0123456789abcdef".into()
    }
}

fn two_keys() -> Clip {
    let sk = common::skeleton();
    let mut clip = Clip::new(sk.names.clone(), 30.0);
    let rest = vec![[0.0, 0.0, 0.0, 1.0]; sk.names.len()];
    for f in [0, 6] {
        clip.set_key(Key::new(f, rest.clone(), [0.0, 0.0, 0.0], Interp::Linear))
            .unwrap();
    }
    clip
}

#[test]
fn a_fill_writes_the_hash_on_every_key_and_the_file_keeps_it() {
    let mut clip = two_keys();
    assert_eq!(edits::refill(&mut clip, 0, 6, &Lift, &NoLinks).unwrap(), "");
    let ai: Vec<&Key> = clip.keys().iter().filter(|k| k.source == KeySource::Ai).collect();
    assert_eq!(ai.len(), 5);
    assert!(
        ai.iter()
            .all(|k| k.model == "test" && k.model_hash == "0123456789abcdef")
    );
    let text = ac_core::clipfile::save_clip(&clip);
    assert!(
        text.contains("\"model_hash\": \"0123456789abcdef\"")
            || text.contains("\"model_hash\":\"0123456789abcdef\""),
        "{text}"
    );
    let back = ac_core::clipfile::load_clip(&text).unwrap();
    assert_eq!(back.keys(), clip.keys());
    // keeping the keys as yours drops the label and the hash
    assert_eq!(edits::keep_ai(&mut clip, 1, 5).unwrap(), 5);
    assert!(clip.keys().iter().all(|k| k.model_hash.is_empty()));
}

#[test]
fn a_key_without_a_hash_reads_as_empty_and_saves_without_one() {
    let mut clip = two_keys();
    let mut k = Key::new(
        3,
        vec![[0.0, 0.0, 0.0, 1.0]; clip.bones.len()],
        [0.0; 3],
        Interp::Smooth,
    );
    k.source = KeySource::Ai;
    k.model = "old".into();
    clip.set_key(k).unwrap();
    let text = ac_core::clipfile::save_clip(&clip);
    assert!(!text.contains("model_hash"), "{text}");
    let back = ac_core::clipfile::load_clip(&text).unwrap();
    let k = back.keys().iter().find(|k| k.frame == 3).unwrap();
    assert_eq!((k.model.as_str(), k.model_hash.as_str()), ("old", ""));
}
