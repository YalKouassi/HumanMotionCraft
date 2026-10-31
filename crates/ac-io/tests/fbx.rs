//! The FBX import: the generated test files (`fixtures/data/*.fbx`, written by
//! `python -m ac_train.fbxwrite` and checked against an outside importer) come onto the rig as
//! the Python retarget's golden says, in the same place from a Y-up and a Z-up file, facing and
//! travelling the right way; broken and non-humanoid files are refused with a reason.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use ac_core::Skeleton;
use ac_core::quat::Vec3;
use ac_core::retarget;
use ac_io::fbx::{binary, import_bytes, scene};
use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skeleton() -> Skeleton {
    Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
        .unwrap()
}

fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(repo().join("fixtures/data").join(name)).unwrap()
}

fn golden() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(repo().join("crates/ac-core/tests/golden/retarget-walk.json")).unwrap(),
    )
    .unwrap()
}

fn vec3s(v: &Value) -> Vec<Vec3> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            [
                p[0].as_f64().unwrap(),
                p[1].as_f64().unwrap(),
                p[2].as_f64().unwrap(),
            ]
        })
        .collect()
}

fn dist(a: Vec3, b: Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The world positions the reader makes of a file's joints, by joint name.
fn positions(name: &str) -> (scene::Motion, BTreeMap<String, Vec<Vec3>>) {
    let doc = binary::parse(&bytes(name)).unwrap();
    let m = scene::motion(&doc).unwrap();
    let mut out = BTreeMap::new();
    for (j, n) in m.names.iter().enumerate() {
        out.insert(n.clone(), m.positions.iter().map(|row| row[j]).collect());
    }
    (m, out)
}

const LANDMARK_JOINTS: [(&str, &str); 18] = [
    ("hips", "Hips"),
    ("spine", "Spine"),
    ("neck", "Neck"),
    ("head", "Head"),
    ("l_shoulder", "LeftArm"),
    ("l_elbow", "LeftForeArm"),
    ("l_wrist", "LeftHand"),
    ("l_hip", "LeftUpLeg"),
    ("l_knee", "LeftLeg"),
    ("l_ankle", "LeftFoot"),
    ("l_toe", "LeftToeBase"),
    ("r_shoulder", "RightArm"),
    ("r_elbow", "RightForeArm"),
    ("r_wrist", "RightHand"),
    ("r_hip", "RightUpLeg"),
    ("r_knee", "RightLeg"),
    ("r_ankle", "RightFoot"),
    ("r_toe", "RightToeBase"),
];

#[test]
fn the_reader_gives_the_joint_positions_the_files_were_written_with() {
    let g = golden();
    for (file, up, front, unit, version) in [
        ("walk-yup.fbx", "+Y", "+Z", 1.0, 7400),
        ("walk-zup.fbx", "+Z", "-Y", 100.0, 7400),
        ("walk-yup-75.fbx", "+Y", "+Z", 1.0, 7500),
    ] {
        let (m, pos) = positions(file);
        assert_eq!(
            (m.up_axis.as_str(), m.front_axis.as_str(), m.unit_scale, m.version),
            (up, front, unit, version),
            "{file}"
        );
        assert_eq!(
            (m.frames, m.fps, m.fps_from.as_str()),
            (40, 30.0, "TimeMode"),
            "{file}"
        );
        assert_eq!(m.take, "Take 001");
        let mut worst = 0.0f64;
        for (lm, joint) in LANDMARK_JOINTS {
            let want = vec3s(&g["landmarks"][lm]);
            let have = &pos[joint];
            for (a, b) in want.iter().zip(have) {
                worst = worst.max(dist(*a, *b));
            }
        }
        // the files hold f32 values in centimetres: a few hundredths of a millimetre
        assert!(worst < 1e-4, "{file}: joints off by up to {worst} m");
        // the end sites, the finger and the prop are there as joints, with no landmark
        for extra in [
            "LeftToe_End",
            "HeadTop_End",
            "LeftHandThumb1",
            "Prop_Sword",
            "LeftArmRoll",
        ] {
            assert!(pos.contains_key(extra), "{file}: {extra}");
        }
    }
}

#[test]
fn the_import_matches_the_golden_and_lands_the_hips_and_feet() {
    let sk = skeleton();
    let g = golden();
    let none = BTreeMap::new();
    let got = import_bytes(&bytes("walk-yup.fbx"), &sk, "walk", &none).unwrap();
    assert_eq!(got.clip.length(), 40);
    assert_eq!(got.clip.fps, 30.0);
    let r = &got.report;
    assert_eq!(r["frames"], 40);
    for (lm, joint) in LANDMARK_JOINTS {
        assert_eq!(r["joints"][lm], joint, "{lm}");
    }
    let unused: Vec<&str> = r["unused"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for extra in [
        "LeftToe_End",
        "HeadTop_End",
        "LeftHandThumb1",
        "Prop_Sword",
        "LeftArmRoll",
        "Reference",
    ] {
        assert!(
            unused.contains(&extra),
            "{extra} should have no landmark: {unused:?}"
        );
    }
    let rest: Vec<&str> = r["at_rest"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(rest.contains(&"finger: 30 of 30 at rest"), "{rest:?}");
    assert!((r["floor"].as_f64().unwrap() - g["floor"].as_f64().unwrap()).abs() < 1e-4);
    assert!(r["hips"]["mid"].as_f64().unwrap() < 1e-4, "{}", r["hips"]);
    assert!(
        r["hips"]["left"].as_f64().unwrap() < 0.01 && r["hips"]["right"].as_f64().unwrap() < 0.01,
        "{}",
        r["hips"]
    );
    // the keys against the golden (the retarget of the same landmarks, before levelling)
    let want_r = g["rotations"].as_array().unwrap();
    let want_o = vec3s(&g["roots"]);
    let floor = g["floor"].as_f64().unwrap();
    let mut worst_q = 0.0f64;
    let mut worst_o = 0.0f64;
    for (f, key) in got.clip.keys().iter().enumerate() {
        let wr = want_r[f].as_array().unwrap();
        for (b, q) in key.rotations.iter().enumerate() {
            let w: Vec<f64> = wr[b]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect();
            let d1 = (0..4).map(|i| (q[i] - w[i]).abs()).fold(0.0, f64::max);
            let d2 = (0..4).map(|i| (q[i] + w[i]).abs()).fold(0.0, f64::max);
            worst_q = worst_q.max(d1.min(d2));
        }
        let o = [key.root[0], key.root[1], key.root[2] + floor / sk.hip_height];
        worst_o = worst_o.max(dist(o, want_o[f]));
    }
    assert!(worst_q < 1e-4, "rotations differ from the golden by {worst_q:e}");
    assert!(worst_o < 1e-4, "roots differ from the golden by {worst_o:e}");
    // the feet rest on z = 0 where the source's rest on its floor
    let rots: Vec<Vec<_>> = got.clip.keys().iter().map(|k| k.rotations.clone()).collect();
    let roots: Vec<Vec3> = got.clip.keys().iter().map(|k| k.root).collect();
    let left = retarget::floor_offset(&sk, &rots, &roots).unwrap();
    assert!(left.abs() < 1e-6, "floor left at {left}");
    eprintln!(
        "import: rotations within {worst_q:e}, roots within {worst_o:e}, hips {}",
        r["hips"]
    );
}

#[test]
fn y_up_and_z_up_files_of_the_same_motion_give_the_same_clip() {
    let sk = skeleton();
    let none = BTreeMap::new();
    let a = import_bytes(&bytes("walk-yup.fbx"), &sk, "a", &none).unwrap();
    let b = import_bytes(&bytes("walk-zup.fbx"), &sk, "b", &none).unwrap();
    let c = import_bytes(&bytes("walk-yup-75.fbx"), &sk, "c", &none).unwrap();
    assert_eq!(b.report["source"]["up_axis"], "+Z");
    assert_eq!(b.report["source"]["unit_scale"], 100.0);
    assert_eq!(c.report["source"]["version"], 7500);
    for other in [&b, &c] {
        let mut worst = 0.0f64;
        for (ka, kb) in a.clip.keys().iter().zip(other.clip.keys()) {
            worst = worst.max(dist(ka.root, kb.root) * sk.hip_height);
            for (qa, qb) in ka.rotations.iter().zip(&kb.rotations) {
                worst = worst.max((0..4).map(|i| (qa[i] - qb[i]).abs()).fold(0.0, f64::max));
            }
        }
        assert!(worst < 1e-4, "the files differ by {worst}");
    }
}

#[test]
fn forward_is_minus_y_left_is_plus_x_and_the_feet_keep_their_sides() {
    let sk = skeleton();
    let none = BTreeMap::new();
    let world = |clip: &ac_core::clip::Clip, f: usize, bone: &str| -> Vec3 {
        let k = &clip.keys()[f];
        let (_, p) = sk.fk(&k.rotations, k.root).unwrap();
        p[sk.bone(bone).unwrap()]
    };
    let walk = import_bytes(&bytes("walk-yup.fbx"), &sk, "walk", &none)
        .unwrap()
        .clip;
    let (r0, r39) = (world(&walk, 0, "root"), world(&walk, 39, "root"));
    assert!(r39[1] - r0[1] < -1.0, "the walk travels -Y: {r0:?} -> {r39:?}");
    assert!((r39[0] - r0[0]).abs() < 0.05);
    let (fl, fr) = (world(&walk, 0, "foot.L"), world(&walk, 0, "foot.R"));
    assert!(
        fl[0] > 0.05 && fr[0] < -0.05,
        "left foot at +X, right at -X: {fl:?} {fr:?}"
    );
    // the ankles land where the source's are (two-bone solve), within a millimetre
    let g = golden();
    let floor = g["floor"].as_f64().unwrap();
    for (bone, lm) in [
        ("foot.L", "l_ankle"),
        ("foot.R", "r_ankle"),
        ("wrist.L", "l_wrist"),
        ("wrist.R", "r_wrist"),
    ] {
        let want = vec3s(&g["landmarks"][lm]);
        for f in [0, 20, 39] {
            let mut w = want[f];
            w[2] -= floor;
            assert!(dist(world(&walk, f, bone), w) < 2e-3, "{bone} frame {f}");
        }
    }
    let side = import_bytes(&bytes("sidestep-left.fbx"), &sk, "side", &none)
        .unwrap()
        .clip;
    let (s0, s29) = (world(&side, 0, "root"), world(&side, 29, "root"));
    assert!(
        s29[0] - s0[0] > 0.25,
        "the side step goes to the rig's left (+X): {s0:?} -> {s29:?}"
    );
}

#[test]
fn corrupt_truncated_and_non_humanoid_files_are_refused_with_a_reason() {
    let sk = skeleton();
    let none = BTreeMap::new();
    let good = bytes("walk-yup.fbx");
    // truncated: half the file
    let e = import_bytes(&good[..good.len() / 2], &sk, "t", &none).unwrap_err();
    assert!(e.contains("truncated"), "{e}");
    // truncated inside the header
    let e = import_bytes(&good[..30], &sk, "t", &none).unwrap_err();
    assert!(e.contains("truncated"), "{e}");
    // corrupt: a record's end offset pointing past the file
    let mut bad = good.clone();
    bad[27..31].copy_from_slice(&u32::MAX.to_le_bytes());
    let e = import_bytes(&bad, &sk, "c", &none).unwrap_err();
    assert!(e.contains("ends at byte"), "{e}");
    // corrupt: a compressed array that does not inflate
    let mut bad = good.clone();
    let at = bad.windows(8).position(|w| w == b"KeyTime\x6c").unwrap() + 8 + 12;
    for b in &mut bad[at..at + 40] {
        *b = 0xFF;
    }
    let e = import_bytes(&bad, &sk, "c", &none).unwrap_err();
    assert!(
        e.contains("inflate") || e.contains("truncated") || e.contains("ends at"),
        "{e}"
    );
    // not FBX at all
    let e = import_bytes(b"{\"format\": \"clip\"}", &sk, "n", &none).unwrap_err();
    assert!(e.contains("not an FBX"), "{e}");
    let e = import_bytes(&[0u8; 100], &sk, "n", &none).unwrap_err();
    assert!(e.contains("not an FBX"), "{e}");
    // a skeleton that is not a humanoid: the missing landmarks are named, with the fix
    let e = import_bytes(&bytes("not-humanoid.fbx"), &sk, "n", &none).unwrap_err();
    assert!(
        e.contains("not a humanoid") && e.contains("l_knee") && e.contains("joints:") && e.contains("Bone1"),
        "{e}"
    );
    // a correction naming a joint that is not there, and a landmark that is not one
    let mut j = BTreeMap::new();
    j.insert("l_knee".to_string(), "NoSuchJoint".to_string());
    let e = import_bytes(&good, &sk, "j", &j).unwrap_err();
    assert!(e.contains("NoSuchJoint"), "{e}");
    let mut j = BTreeMap::new();
    j.insert("elbow".to_string(), "LeftForeArm".to_string());
    let e = import_bytes(&good, &sk, "j", &j).unwrap_err();
    assert!(e.contains("not a landmark"), "{e}");
    // a correction that is used
    let mut j = BTreeMap::new();
    j.insert("l_toe".to_string(), "LeftToe_End".to_string());
    let got = import_bytes(&good, &sk, "j", &j).unwrap();
    assert_eq!(got.report["joints"]["l_toe"], "LeftToe_End");
    assert_eq!(got.report["given"], serde_json::json!(["l_toe"]));
}

#[test]
fn swapped_sides_are_refused() {
    let sk = skeleton();
    let mut j = BTreeMap::new();
    for (lm, joint) in LANDMARK_JOINTS {
        let swapped = if let Some(rest) = joint.strip_prefix("Left") {
            format!("Right{rest}")
        } else if let Some(rest) = joint.strip_prefix("Right") {
            format!("Left{rest}")
        } else {
            joint.to_string()
        };
        j.insert(lm.to_string(), swapped);
    }
    let e = import_bytes(&bytes("walk-yup.fbx"), &sk, "s", &j).unwrap_err();
    assert!(e.contains("swapped"), "{e}");
}

#[test]
fn a_batch_never_writes_two_files_of_one_name_onto_each_other() {
    let (mut s, lib) = common::session("fbx-batch-names");
    let from = lib.join("from");
    let to = lib.join("to");
    for sub in ["a", "b"] {
        std::fs::create_dir_all(from.join(sub)).unwrap();
        std::fs::write(from.join(sub).join("walk.fbx"), bytes("walk-yup.fbx")).unwrap();
    }
    std::fs::write(from.join("b").join("Side.fbx"), bytes("sidestep-left.fbx")).unwrap();
    let r = s
        .run(
            "clip.import_batch",
            &serde_json::json!({ "from": from.to_string_lossy(), "to": to.to_string_lossy() }),
        )
        .unwrap();
    let files = r["files"].as_array().unwrap();
    let failed = r["failed"].as_array().unwrap();
    assert_eq!(
        files
            .iter()
            .map(|f| f["file"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["a/walk.fbx", "b/Side.fbx"],
        "{r}"
    );
    assert_eq!(failed.len(), 1, "{r}");
    assert_eq!(failed[0]["file"], "b/walk.fbx");
    let why = failed[0]["error"].as_str().unwrap();
    assert!(why.contains("same name") && why.contains("a/walk.fbx"), "{why}");
    // the clip written is the first file's, untouched by the second
    let written: Vec<_> = std::fs::read_dir(&to)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(written.len(), 2, "{written:?}");
    std::fs::remove_dir_all(lib).unwrap();
}
