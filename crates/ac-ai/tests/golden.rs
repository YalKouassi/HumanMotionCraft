//! The port against the reference (`training/ac_train`, PyTorch): the network alone on fixed inputs, and
//! whole fills of a library walk. Goldens come from `training/` (`python -m ac_train.export`).
//!
//! `AC_AI_MODEL=<dir>` and `AC_AI_GOLDEN=<dir>` check another export instead of the committed one
//! (`python -m ac_train.pipeline --quick` does, on a model it just trained); the goldens are then
//! `<dir name of the model>-{net,fill}.json`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_ai::Model;
use ac_core::Skeleton;
use ac_core::ai::{FillSettings, Inbetweener};
use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn model_dir() -> PathBuf {
    std::env::var_os("AC_AI_MODEL").map_or_else(|| repo().join("assets/models/parts"), PathBuf::from)
}

/// `parts-net.json` and so on: the goldens are named after the model's folder.
fn golden(what: &str) -> Value {
    let tag = model_dir().file_name().unwrap().to_string_lossy().into_owned();
    let dir = std::env::var_os("AC_AI_GOLDEN").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden"),
        PathBuf::from,
    );
    let p = dir.join(format!("{tag}-{what}"));
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .unwrap()
}

fn floats(v: &Value) -> Vec<f32> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<f32>) {
        match v {
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            x => out.push(x.as_f64().unwrap() as f32),
        }
    }
    walk(v, &mut out);
    out
}

fn model() -> Model {
    Model::load(&model_dir()).unwrap()
}

#[test]
fn network_matches_the_reference() {
    let m = model();
    let g = golden("net.json");
    let net = m.net();
    let given: Vec<usize> = g["given"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as usize)
        .collect();
    let mut gmask = vec![0.0f32; net.given];
    given.iter().for_each(|&i| gmask[i] = 1.0);
    let style = g["style"].as_u64().map(|s| s as usize);
    // a model with the root path pins it too
    assert_eq!(net.path, g.get("path").is_some());
    let (path, pmask) = match net.path {
        true => (floats(&g["path"]), floats(&g["pmask"])),
        false => (Vec::new(), Vec::new()),
    };
    let path = net.path.then_some((path.as_slice(), pmask.as_slice()));
    let (out, contacts) = net
        .forward_contacts(
            &floats(&g["known"]),
            &floats(&g["interp"]),
            &floats(&g["mask"]),
            &floats(&g["ttg"]),
            style,
            Some(&gmask),
            path,
        )
        .unwrap();
    let worst = |got: &[f32], want: &[f32]| {
        assert_eq!(got.len(), want.len());
        got.iter()
            .zip(want)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max)
    };
    let w = worst(&out, &floats(&g["out"]));
    assert!(w < 1e-4, "largest difference {w}");
    // a model with the contact head pins its foot contacts too
    assert_eq!(contacts.is_some(), g.get("contacts").is_some());
    if let Some(c) = contacts {
        let w = worst(&c, &floats(&g["contacts"]));
        assert!(w < 1e-4, "largest contact difference {w}");
    }
}

#[test]
fn fills_match_the_reference() {
    let m = model();
    let sk =
        Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
            .unwrap();
    let g = golden("fill.json");
    let path = repo()
        .join("assets/library/animations")
        .join(format!("{}.clip.json", g["clip"].as_str().unwrap()));
    let clip = ac_core::clipfile::load_clip(&std::fs::read_to_string(path).unwrap()).unwrap();
    for case in g["cases"].as_array().unwrap() {
        let (a, b) = (case["a"].as_i64().unwrap(), case["b"].as_i64().unwrap());
        let s = FillSettings {
            style: case["style"].as_str().unwrap().into(),
            style_force: case["style_force"].as_f64().unwrap(),
            ai_force: case["ai_force"].as_f64().unwrap(),
            trajectory: case["trajectory"].as_f64().unwrap_or(0.0),
            head: case["head"].as_f64().unwrap_or(0.0),
        };
        // a part layer: the model's bones another layer drives, as `ac_core::ai::AiFiller` lists them
        let given: Vec<String> = case["given"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        let given = (!given.is_empty()).then_some(given.as_slice());
        let (rots, roots) = m
            .fill(&sk, &clip, &ac_core::clip::NoLinks, a, b, &s, given)
            .unwrap();
        let want_r = case["rotations"].as_array().unwrap();
        let want_o = case["roots"].as_array().unwrap();
        assert_eq!(rots.len(), want_r.len());
        let mut worst_angle = 0.0f64;
        let mut worst_root = 0.0f64;
        for (k, (fr, wr)) in rots.iter().zip(want_r).enumerate() {
            for (j, (q, w)) in fr.iter().zip(wr.as_array().unwrap()).enumerate() {
                let w: Vec<f64> = w
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap())
                    .collect();
                let dot: f64 = (0..4).map(|i| q[i] * w[i]).sum::<f64>().abs().min(1.0);
                let angle = 2.0 * dot.acos();
                assert!(
                    angle < 2e-3,
                    "case {a}-{b} frame {k} bone {} off by {angle} rad",
                    sk.names[j]
                );
                worst_angle = worst_angle.max(angle);
            }
            let w: Vec<f64> = want_o[k]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect();
            let d = (0..3).map(|i| (roots[k][i] - w[i]).abs()).fold(0.0, f64::max);
            worst_root = worst_root.max(d);
        }
        assert!(
            worst_root < 1e-4,
            "case {a}-{b}: root off by {worst_root} hip heights"
        );
        eprintln!("case {a}-{b}: worst bone {worst_angle:.2e} rad, worst root {worst_root:.2e}");
        if s.ai_force == 0.0 && s.trajectory == 0.0 && s.head == 0.0 && given.is_none() {
            plain_interpolation_comes_back(&m, &sk, &clip, a, b, &rots, &roots);
        }
    }
}

/// With `ai_force` 0 the network's change is dropped: the solved bones are the slerp between the
/// keys and the root their lerp, the other bones the keys' own spline. Checked against the
/// interpolation worked out here, not the reference, so the only slack is the features' float32
/// round trip (the reference's slerp lerps under 1.8 degrees: under 1e-6 rad from the exact one).
fn plain_interpolation_comes_back(
    m: &Model,
    sk: &Skeleton,
    clip: &ac_core::clip::Clip,
    a: i64,
    b: i64,
    rots: &[Vec<ac_core::quat::Quat>],
    roots: &[ac_core::quat::Vec3],
) {
    use ac_core::quat;
    let frames: Vec<f64> = (a..=b).map(|f| f as f64).collect();
    let (want_r, want_o) = clip.sample(&frames).unwrap();
    let (sa, sb) = (&want_r[0], &want_r[want_r.len() - 1]);
    let (oa, ob) = (want_o[0], want_o[want_o.len() - 1]);
    let solved: Vec<usize> = m.info().bones.iter().map(|n| sk.bone(n).unwrap()).collect();
    let g = (b - a) as f64;
    let mut worst = 0.0f64;
    for (k, (fr, o)) in rots.iter().zip(roots).enumerate() {
        let t = (k + 1) as f64 / g;
        for (j, q) in fr.iter().enumerate() {
            let want = if solved.contains(&j) {
                quat::slerp(sa[j], sb[j], t)
            } else {
                want_r[k + 1][j]
            };
            let dot: f64 = (0..4).map(|i| q[i] * want[i]).sum::<f64>().abs().min(1.0);
            let angle = 2.0 * dot.acos();
            assert!(
                angle < 1e-5,
                "ai_force 0, case {a}-{b} frame {k} bone {}: {angle} rad from the plain interpolation",
                sk.names[j]
            );
            worst = worst.max(angle);
        }
        let want = [0, 1, 2].map(|i| oa[i] + (ob[i] - oa[i]) * t);
        let d = (0..3).map(|i| (o[i] - want[i]).abs()).fold(0.0, f64::max);
        assert!(
            d < 1e-6,
            "ai_force 0, case {a}-{b} frame {k}: root {d} hip heights from the plain lerp"
        );
    }
    eprintln!("case {a}-{b}: ai_force 0 is the plain interpolation within {worst:.2e} rad");
}
