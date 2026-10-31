//! The two card options the window contract gained (roadmap 140000 `context_after`, 142000
//! `multi_key`), proved on a synthetic model: a tiny network written here with its output layer
//! at zero, so its fill is exactly the plain interpolation between neighbouring known frames.
//! No model trained with the options exists yet, so this checks the plumbing (the card is read,
//! the window is laid out as the card says, several keys fill in one pass through `ai.fill_all`
//! with no ease at the key they share), not that such a model fills well: that is the retrain's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use ac_ai::Model;
use ac_core::ai::{FillSettings, Inbetweener};
use ac_core::clip::{Clip, Interp, Key, KeySource, NoLinks};
use ac_core::quat::{self, Quat};
use ac_core::{Document, Session, Skeleton};
use serde_json::{Value, json};

const BONES: [&str; 19] = [
    "root",
    "spine05",
    "spine04",
    "spine03",
    "spine02",
    "spine01",
    "neck01",
    "clavicle.L",
    "upperarm01.L",
    "lowerarm01.L",
    "clavicle.R",
    "upperarm01.R",
    "lowerarm01.R",
    "upperleg01.L",
    "lowerleg01.L",
    "foot.L",
    "upperleg01.R",
    "lowerleg01.R",
    "foot.R",
];
const D: usize = 16;
const FF: usize = 32;
const WINDOW: usize = 56;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skeleton() -> Skeleton {
    Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
        .unwrap()
}

fn walk() -> Clip {
    ac_core::clipfile::load_clip(
        &std::fs::read_to_string(repo().join("assets/library/animations/Locomotion/Walk.clip.json")).unwrap(),
    )
    .unwrap()
}

/// A deterministic spread of small weights.
fn spread(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            ((s >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.2
        })
        .collect()
}

/// Write a tiny model (one encoder layer, 16 wide, the output layer at zero) with the card
/// options given, under a fresh folder; returns the folder.
fn synthetic(name: &str, context_after: usize, multi_key: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ac-ai-multikey-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let feat = BONES.len() * 6 + 3;
    let times = if multi_key { 2 } else { 1 };
    let inp = feat * 2 + 1 + times + BONES.len() + 4;
    let mut tensors: Vec<(&str, Vec<usize>, Vec<f32>)> = vec![
        ("pos", vec![WINDOW, D], spread(WINDOW * D, 1)),
        ("inp.weight", vec![D, inp], spread(D * inp, 2)),
        ("inp.bias", vec![D], spread(D, 3)),
        (
            "body.layers.0.self_attn.in_proj_weight",
            vec![3 * D, D],
            spread(3 * D * D, 4),
        ),
        (
            "body.layers.0.self_attn.in_proj_bias",
            vec![3 * D],
            spread(3 * D, 5),
        ),
        (
            "body.layers.0.self_attn.out_proj.weight",
            vec![D, D],
            spread(D * D, 6),
        ),
        ("body.layers.0.self_attn.out_proj.bias", vec![D], spread(D, 7)),
        ("body.layers.0.linear1.weight", vec![FF, D], spread(FF * D, 8)),
        ("body.layers.0.linear1.bias", vec![FF], spread(FF, 9)),
        ("body.layers.0.linear2.weight", vec![D, FF], spread(D * FF, 10)),
        ("body.layers.0.linear2.bias", vec![D], spread(D, 11)),
        ("body.layers.0.norm1.weight", vec![D], vec![1.0; D]),
        ("body.layers.0.norm1.bias", vec![D], vec![0.0; D]),
        ("body.layers.0.norm2.weight", vec![D], vec![1.0; D]),
        ("body.layers.0.norm2.bias", vec![D], vec![0.0; D]),
        ("out.0.weight", vec![D], vec![1.0; D]),
        ("out.0.bias", vec![D], vec![0.0; D]),
        ("out.1.weight", vec![feat, D], vec![0.0; feat * D]),
        ("out.1.bias", vec![feat], vec![0.0; feat]),
    ];
    let mut index = serde_json::Map::new();
    let mut blob: Vec<u8> = Vec::new();
    let mut offset = 0usize;
    for (name, shape, data) in tensors.drain(..) {
        index.insert(name.into(), json!({ "offset": offset, "shape": shape }));
        offset += data.len();
        blob.extend(data.iter().flat_map(|v| v.to_le_bytes()));
    }
    let card = json!({
        "tag": name, "bones": BONES, "styles": [], "context": 10 - context_after,
        "context_after": context_after, "max_gap": 45, "multi_key": multi_key, "given": true,
        "path": true, "contacts": false, "heads": 4, "commercial": true, "warning": "",
    });
    std::fs::write(
        dir.join("weights.json"),
        serde_json::to_string(
            &json!({ "format": "ac-inbetweener", "version": 1, "tensors": index, "card": card }),
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("weights.bin"), blob).unwrap();
    dir
}

fn angle(a: Quat, b: Quat) -> f64 {
    let dot: f64 = (0..4).map(|i| a[i] * b[i]).sum::<f64>().abs().min(1.0);
    2.0 * dot.acos()
}

#[test]
fn the_card_options_are_read_and_default_to_the_window_as_it_was() {
    let shipped = Model::load(&repo().join("assets/models/parts")).unwrap();
    let i = shipped.info();
    assert_eq!((i.context, i.context_after, i.multi_key), (10, 0, false));
    assert_eq!(shipped.net().times(), 1);
    let dir = synthetic("options", 4, true);
    let m = Model::load(&dir).unwrap();
    let i = m.info();
    assert_eq!((i.context, i.context_after, i.multi_key), (6, 4, true));
    assert_eq!(m.net().times(), 2);
    // the window of a fill between keys 10 and 20 of a loop (every frame exists): 6 frames
    // before and on 10, the stretch, 4 after 20
    let w = ac_ai::window(i, None, &[10, 20]).unwrap();
    let frames: Vec<i64> = w.frames.iter().map(|f| *f as i64).collect();
    assert_eq!(frames, (5..=24).collect::<Vec<_>>());
    assert_eq!((w.before, w.after), (6, 4));
    let known: Vec<usize> = w
        .known
        .iter()
        .enumerate()
        .filter(|(_, k)| **k)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(known, vec![0, 1, 2, 3, 4, 5, 15, 16, 17, 18, 19]);
    // a key inside the stretch is known too
    let w = ac_ai::window(i, None, &[10, 14, 20]).unwrap();
    assert!(w.known[6 + 3] && !w.known[6 + 2] && !w.known[6 + 4]);
    // the clip ends on 22: two frames after 20, so eight before and on 10
    let w = ac_ai::window(i, Some(22), &[10, 20]).unwrap();
    let frames: Vec<i64> = w.frames.iter().map(|f| *f as i64).collect();
    assert_eq!(frames, (3..=22).collect::<Vec<_>>());
    assert_eq!((w.before, w.after), (8, 2));
    // the clip ends on 20: the fallback is the window as it always was, ten before and on 10
    let w = ac_ai::window(i, Some(20), &[10, 20]).unwrap();
    let frames: Vec<i64> = w.frames.iter().map(|f| *f as i64).collect();
    assert_eq!(frames, (1..=20).collect::<Vec<_>>());
    assert_eq!((w.before, w.after), (10, 0));
    // the shipped card: ten before (frame 0 repeated before the clip's start), none after
    let w = ac_ai::window(shipped.info(), Some(40), &[2, 8]).unwrap();
    let frames: Vec<i64> = w.frames.iter().map(|f| *f as i64).collect();
    assert_eq!(frames, vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(ac_ai::window(i, None, &[10]).is_err());
    assert!(ac_ai::window(i, None, &[20, 10]).is_err());
    assert!(ac_ai::window(i, None, &[10, 60]).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_model_trained_on_one_gap_at_a_time_refuses_several_keys() {
    let m = Model::load(&repo().join("assets/models/parts")).unwrap();
    let e = m
        .fill_keys(
            &skeleton(),
            &walk(),
            &NoLinks,
            &[10, 15, 20],
            &FillSettings::default(),
            None,
        )
        .unwrap_err();
    assert!(e.contains("one gap at a time"), "{e}");
}

/// With the output layer at zero the network's change is nothing, so a pass over several keys
/// is the plain interpolation between neighbouring keys: the key inside comes back as the clip
/// has it, the frames either side of it run into it and out of it (continuous at the key), and
/// each half is what a fill of that gap alone gives.
#[test]
fn several_keys_fill_in_one_pass_as_the_interpolation_between_neighbours() {
    let dir = synthetic("pass", 4, true);
    let m = Model::load(&dir).unwrap();
    let sk = skeleton();
    let clip = walk();
    let s = FillSettings::default();
    let (r, o, c) = m
        .fill_keys(&sk, &clip, &NoLinks, &[10, 20, 30], &s, None)
        .unwrap();
    assert_eq!((r.len(), o.len()), (19, 19));
    assert!(c.is_none(), "the synthetic model has no contact head");
    let (kr, ko) = clip.sample(&[10.0, 20.0, 30.0]).unwrap();
    // frame 20 (index 9) is the clip's own key
    assert_eq!(r[9], kr[1]);
    assert_eq!(o[9], ko[1]);
    let solved: Vec<usize> = BONES.iter().map(|n| sk.bone(n).unwrap()).collect();
    for (k, (fr, root)) in r.iter().zip(&o).enumerate() {
        let f = 11 + k as i64;
        if f == 20 {
            continue;
        }
        let (ka, kb, t) = if f < 20 {
            (0, 1, (f - 10) as f64 / 10.0)
        } else {
            (1, 2, (f - 20) as f64 / 10.0)
        };
        for &j in &solved {
            let want = quat::slerp(kr[ka][j], kr[kb][j], t);
            assert!(
                angle(fr[j], want) < 1e-5,
                "frame {f} bone {}: {} rad from the plain slerp",
                sk.names[j],
                angle(fr[j], want)
            );
        }
        let want = [0, 1, 2].map(|i| ko[ka][i] + (ko[kb][i] - ko[ka][i]) * t);
        let d = (0..3).map(|i| (root[i] - want[i]).abs()).fold(0.0, f64::max);
        assert!(d < 1e-6, "frame {f}: root {d} from the plain lerp");
    }
    // each half is the fill of that gap alone, and a fill with two keys is a pass of two keys
    let (r1, o1) = m.fill(&sk, &clip, &NoLinks, 10, 20, &s, None).unwrap();
    let (r2, o2) = m.fill(&sk, &clip, &NoLinks, 20, 30, &s, None).unwrap();
    let mut worst = 0.0f64;
    for (k, (fr, root)) in r1.iter().zip(&o1).enumerate() {
        for (q, w) in fr.iter().zip(&r[k]) {
            worst = worst.max(angle(*q, *w));
        }
        assert!((0..3).all(|i| (root[i] - o[k][i]).abs() < 1e-9));
    }
    for (k, (fr, root)) in r2.iter().zip(&o2).enumerate() {
        for (q, w) in fr.iter().zip(&r[10 + k]) {
            worst = worst.max(angle(*q, *w));
        }
        assert!((0..3).all(|i| (root[i] - o[10 + k][i]).abs() < 1e-9));
    }
    // the same numbers (acos cannot tell equal quaternions apart below about 1e-7)
    assert!(worst < 1e-6, "the pass against the two fills: worst {worst} rad");
    let (r3, o3, _) = m.fill_keys(&sk, &clip, &NoLinks, &[10, 20], &s, None).unwrap();
    assert_eq!((r3, o3), (r1, o1));
    std::fs::remove_dir_all(&dir).ok();
}

/// A document with keys of yours on 0, 10 and 20 (the walk's poses) and the model given.
fn document(model: Model, sk: &Skeleton) -> Document {
    let mut doc = Document::new(sk.clone());
    doc.ai = Some(Arc::new(model));
    let clip = walk();
    let (r, o) = clip.sample(&[0.0, 10.0, 20.0]).unwrap();
    for (i, f) in [0i64, 10, 20].into_iter().enumerate() {
        // a hip height apart in x: no foot stands still between two keys, so the fill plants
        // none (`plant_feet`) and the AI keys are the model's alone
        let root = [o[i][0] + i as f64, o[i][1], o[i][2]];
        doc.clip
            .set_key(Key::new(f, r[i].clone(), root, Interp::Smooth))
            .unwrap();
    }
    doc
}

/// The AI keys of a document's clip by frame, and the plain slerp between its keys 0, 10, 20.
fn ai_keys(doc: &Document) -> Vec<(i64, Vec<Quat>)> {
    doc.clip
        .keys()
        .iter()
        .filter(|k| k.source == KeySource::Ai)
        .map(|k| (k.frame, k.rotations.clone()))
        .collect()
}

/// `ai.fill_all` with a model trained with several keys in a window takes the two gaps 0–10 and
/// 10–20 in one pass: both are filled, `passes` says so, key 10 is yours still, and no ease
/// ran through it (with the output layer at zero the AI keys are the plain interpolation
/// between the keys, which an ease would have re-curved). With a model trained on one gap at a
/// time the same document gets two passes and the ease, as before.
#[test]
fn fill_all_takes_two_neighbouring_gaps_in_one_pass_without_the_ease() {
    let sk = skeleton();
    let solved: Vec<usize> = BONES.iter().map(|n| sk.bone(n).unwrap()).collect();
    let multi = synthetic("fill-all-multi", 4, true);
    let single = synthetic("fill-all-single", 0, false);
    let run = |dir: &PathBuf| -> (Value, Document) {
        let doc = document(Model::load(dir).unwrap(), &sk);
        let mut s = Session::new(doc);
        let out = s.run("ai.fill_all", &json!({})).unwrap();
        (out, s.document)
    };
    let (out, doc) = run(&multi);
    assert_eq!(out["filled"], json!([[0, 10], [10, 20]]));
    assert_eq!(out["passes"], json!([[0, 10, 20]]));
    assert_eq!(out["skipped"], json!([]));
    assert_eq!(out["diagnostics"].as_array().unwrap().len(), 2);
    let ten = doc.clip.keys().iter().find(|k| k.frame == 10).unwrap();
    assert_eq!(ten.source, KeySource::Manual);
    let keys = ai_keys(&doc);
    assert_eq!(
        keys.iter().map(|k| k.0).collect::<Vec<_>>(),
        (1..=19).filter(|f| *f != 10).collect::<Vec<_>>()
    );
    let (kr, _) = doc.clip.sample(&[0.0, 10.0, 20.0]).unwrap();
    let plain = |f: i64, j: usize| -> Quat {
        let (ka, kb, t) = if f < 10 {
            (0, 1, f as f64 / 10.0)
        } else {
            (1, 2, (f - 10) as f64 / 10.0)
        };
        quat::slerp(kr[ka][j], kr[kb][j], t)
    };
    let mut worst = 0.0f64;
    for (f, rots) in &keys {
        for &j in &solved {
            worst = worst.max(angle(rots[j], plain(*f, j)));
        }
    }
    assert!(
        worst < 1e-5,
        "the one pass is the plain interpolation on every AI key (no ease ran through key 10): worst {worst} rad"
    );
    // the same document with a one-gap model: two passes, and the ease re-curves the frames
    // around key 10
    let (out, doc) = run(&single);
    assert_eq!(out["filled"], json!([[0, 10], [10, 20]]));
    assert_eq!(out["passes"], json!([]));
    let mut worst = 0.0f64;
    for (f, rots) in &ai_keys(&doc) {
        for &j in &solved {
            worst = worst.max(angle(rots[j], plain(*f, j)));
        }
    }
    assert!(
        worst > 1e-4,
        "two passes eased through key 10 should move the frames beside it: worst {worst} rad"
    );
    std::fs::remove_dir_all(&multi).ok();
    std::fs::remove_dir_all(&single).ok();
}
