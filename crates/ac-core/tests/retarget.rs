//! The Rust retarget matches the Python one (`training/ac_train/retarget.py`) on the golden
//! `tests/golden/retarget-walk.json`, written by `python -m ac_train.fbxwrite` from the built-in
//! walk's landmarks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use ac_core::Skeleton;
use ac_core::quat::Vec3;
use ac_core::retarget::{self, Landmarks};
use serde_json::Value;

fn skeleton() -> Skeleton {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/rig/default_rig.json");
    Skeleton::from_json(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn golden() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/retarget-walk.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn vec3s(v: &Value) -> Vec<Vec3> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let a = p.as_array().unwrap();
            [
                a[0].as_f64().unwrap(),
                a[1].as_f64().unwrap(),
                a[2].as_f64().unwrap(),
            ]
        })
        .collect()
}

pub fn landmarks(g: &Value) -> Landmarks {
    let points: BTreeMap<String, Vec<Vec3>> = g["landmarks"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), vec3s(v)))
        .collect();
    Landmarks::new(points).unwrap()
}

#[test]
fn the_port_matches_the_python_golden() {
    let g = golden();
    let sk = skeleton();
    let src = landmarks(&g);
    assert_eq!(g["bones"].as_array().unwrap().len(), sk.len());
    let (scaled, s) = retarget::scale_to(&sk, &src).unwrap();
    // the golden landmarks are written to 10 decimals, so the leg lengths carry that rounding
    assert!((s - g["scale"].as_f64().unwrap()).abs() < 1e-7, "scale {s}");
    let (rots, roots) = retarget::retarget(&sk, &scaled).unwrap();
    let frames = g["frames"].as_u64().unwrap() as usize;
    assert_eq!(rots.len(), frames);
    assert_eq!(roots.len(), frames);
    let want_r = g["rotations"].as_array().unwrap();
    let want_o = vec3s(&g["roots"]);
    let mut worst_q = 0.0f64;
    let mut worst_o = 0.0f64;
    for f in 0..frames {
        let wr = want_r[f].as_array().unwrap();
        for (b, q) in rots[f].iter().enumerate() {
            let w = wr[b].as_array().unwrap();
            let w = [
                w[0].as_f64().unwrap(),
                w[1].as_f64().unwrap(),
                w[2].as_f64().unwrap(),
                w[3].as_f64().unwrap(),
            ];
            // q and -q are one rotation; the golden keeps w >= 0, so does the port, but a w near 0
            // may flip
            let d1 = (0..4).map(|i| (q[i] - w[i]).abs()).fold(0.0, f64::max);
            let d2 = (0..4).map(|i| (q[i] + w[i]).abs()).fold(0.0, f64::max);
            worst_q = worst_q.max(d1.min(d2));
        }
        for i in 0..3 {
            worst_o = worst_o.max((roots[f][i] - want_o[f][i]).abs());
        }
    }
    // the golden is written to 10 decimals; the acceptance asks for 1e-4
    assert!(
        worst_q < 1e-7,
        "rotations differ from the Python golden by up to {worst_q:e}"
    );
    assert!(
        worst_o < 1e-7,
        "root offsets differ from the Python golden by up to {worst_o:e}"
    );
    let floor = retarget::floor_offset(&sk, &rots, &roots).unwrap();
    assert!(
        (floor - g["floor"].as_f64().unwrap()).abs() < 1e-5,
        "floor {floor}"
    );
    // the hip joints' midpoint lands on the source's; each side is off by what the pelvis's
    // estimated tilt misses (the acceptance asks for 1 cm; the same proportions here give 6 mm)
    let err = retarget::hip_error(&sk, &rots, &roots, &scaled).unwrap();
    assert!(
        err[0] < 1e-6 && err[1] < 0.01 && err[2] < 0.01,
        "hips off by {err:?}"
    );
    eprintln!(
        "retarget golden: rotations within {worst_q:e}, roots within {worst_o:e}, floor {floor:.5}, hips {err:?}"
    );
}

#[test]
fn level_moves_the_floor_to_zero_and_leaves_an_odd_take() {
    let g = golden();
    let sk = skeleton();
    let (scaled, _) = retarget::scale_to(&sk, &landmarks(&g)).unwrap();
    let (rots, roots) = retarget::retarget(&sk, &scaled).unwrap();
    let (moved, off) = retarget::level(&sk, &rots, &roots).unwrap();
    assert!((off - g["floor"].as_f64().unwrap()).abs() < 1e-5);
    assert!(retarget::floor_offset(&sk, &rots, &moved).unwrap().abs() < 1e-9);
    // a take a metre in the air is left where it is, and says so with offset 0
    let high: Vec<Vec3> = roots
        .iter()
        .map(|r| [r[0], r[1], r[2] + 1.0 / sk.hip_height])
        .collect();
    let (same, off) = retarget::level(&sk, &rots, &high).unwrap();
    assert_eq!(off, 0.0);
    assert_eq!(same, high);
}

#[test]
fn landmarks_are_checked() {
    let g = golden();
    let mut points: BTreeMap<String, Vec<Vec3>> = g["landmarks"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), vec3s(v)))
        .collect();
    points.remove("l_knee");
    let e = Landmarks::new(points.clone()).unwrap_err().to_string();
    assert!(e.contains("l_knee"), "{e}");
    points.insert("l_knee".into(), vec![[0.0, 0.0, f64::NAN]; 40]);
    let e = Landmarks::new(points.clone()).unwrap_err().to_string();
    assert!(e.contains("non-finite"), "{e}");
    points.insert("l_knee".into(), vec![[0.0; 3]; 3]);
    let e = Landmarks::new(points).unwrap_err().to_string();
    assert!(e.contains("3 frames"), "{e}");
}
