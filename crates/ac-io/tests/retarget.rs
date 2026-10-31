//! The export onto a character of the person's own (roadmap 146000), on the test character
//! `fixtures/data/tall-character.glb` (written by `python -m ac_train.glbwrite`: 1.9 m tall,
//! an A-pose, a bone roll on every joint, an armature scaled 0.01 with centimetre joints,
//! other bone names, a forearm twist, fingers, a prop).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use ac_core::character::plan::Plan;
use ac_io::character::{self, load, save_text};
use glam::{DMat4, DQuat, DVec3};
use serde_json::json;

fn repo() -> PathBuf {
    common::repo()
}

fn character_path() -> PathBuf {
    repo().join("fixtures/data/tall-character.glb")
}

fn skeleton() -> ac_core::Skeleton {
    ac_core::Skeleton::from_json(
        &std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn the_mapping_is_guessed_from_the_names_and_the_rest_listed() {
    let t = load(&character_path(), &BTreeMap::new(), None).unwrap();
    let m = &t.loaded.mapping;
    let at = |lm: &str| m.joints.get(lm).map(String::as_str).unwrap_or("-");
    assert_eq!(at("hips"), "Pelvis");
    assert_eq!(at("spine"), "Spine");
    assert_eq!(at("neck"), "Neck");
    assert_eq!(at("head"), "Head");
    assert_eq!(at("l_shoulder"), "L_UpperArm");
    assert_eq!(at("l_elbow"), "L_Forearm");
    assert_eq!(at("l_wrist"), "L_Hand");
    assert_eq!(at("l_hip"), "L_Thigh");
    assert_eq!(at("l_knee"), "L_Calf");
    assert_eq!(at("l_ankle"), "L_Foot");
    assert_eq!(at("l_toe"), "L_Toe0");
    assert_eq!(at("r_toe"), "R_Toe0");
    assert!(m.missing().is_empty(), "{:?}", m.missing());
    assert!(t.loaded.character.has_mesh);
    assert_eq!(t.loaded.character.skin.len(), 36);
    assert!(
        (t.loaded.character.lowest.unwrap() - 0.0).abs() < 1e-6,
        "the soles are on the floor"
    );
    let sk = skeleton();
    let plan = Plan::new(&sk, &t.loaded.character, m).unwrap();
    let names = |ix: &[usize]| -> Vec<&str> {
        ix.iter()
            .map(|&i| t.loaded.character.nodes[i].name.as_str())
            .collect()
    };
    let undriven = names(&plan.undriven);
    for n in [
        "L_ForearmTwist",
        "L_Finger0",
        "L_Finger01",
        "L_Finger1",
        "L_Finger11",
        "R_ForearmTwist",
        "Weapon_Prop",
        "HeadNub",
        "L_Toe0Nub",
    ] {
        assert!(undriven.contains(&n), "{n} is undriven: {undriven:?}");
    }
    assert_eq!(undriven.len(), 14, "{undriven:?}");
    assert_eq!(names(&plan.outside), ["Armature", "Body"]);
    assert_eq!(
        plan.drives.len(),
        22,
        "{:?}",
        plan.drives.iter().map(|d| &d.role).collect::<Vec<_>>()
    );
    // the legs: (0.420119 + 0.500400) / 0.794062
    assert!((plan.scale - 1.15925).abs() < 1e-4, "{}", plan.scale);
    assert!(
        plan.drives.iter().any(|d| d.role == "l_clavicle"),
        "the clavicle is driven from the tree"
    );
}

#[test]
fn the_mapping_file_round_trips_byte_for_byte() {
    let dir = std::env::temp_dir().join(format!(
        "{}-retarget-{}",
        ac_core::product::ID,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut corrections = BTreeMap::new();
    corrections.insert("l_toe".to_string(), "none".to_string());
    let t = load(&character_path(), &corrections, Some(true)).unwrap();
    assert!(!t.loaded.mapping.joints.contains_key("l_toe"));
    let text = save_text(&t.loaded).unwrap();
    let file = dir.join("tall.retarget.json");
    std::fs::write(&file, &text).unwrap();
    let again = load(&file, &BTreeMap::new(), None).unwrap();
    assert_eq!(again.loaded.mapping, t.loaded.mapping);
    assert_eq!(again.loaded.path, t.loaded.path);
    assert_eq!(
        save_text(&again.loaded).unwrap(),
        text,
        "the same bytes after a load"
    );
    // a relative character path resolves from the mapping file's folder
    std::fs::copy(character_path(), dir.join("hero.glb")).unwrap();
    let relative = text.replace(
        &json!(t.loaded.path.to_string_lossy()).to_string(),
        "\"hero.glb\"",
    );
    assert_ne!(relative, text);
    std::fs::write(&file, &relative).unwrap();
    let rel = load(&file, &BTreeMap::new(), None).unwrap();
    assert_eq!(rel.loaded.path, PathBuf::from("hero.glb"));
    assert_eq!(save_text(&rel.loaded).unwrap(), relative);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Rig (Z up) -> glTF (Y up).
fn c4() -> DMat4 {
    DMat4::from_cols_array_2d(&[
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// The exported file, read back by an outside glTF reader, moves as the drive worked it out:
/// every node's world on frames 0, 20 and 61 of the walk, through the file's own tree and
/// its animation channels, matches the drive's worlds, and the planted feet hold.
#[test]
fn the_exported_walk_moves_as_driven_and_its_planted_feet_hold() {
    let (mut session, lib) = common::session("retarget");
    let out = std::env::temp_dir().join(format!(
        "{}-retarget-walk-{}.glb",
        ac_core::product::ID,
        std::process::id()
    ));
    session
        .run("clip.open", &json!({ "id": "Locomotion/Walk 12" }))
        .unwrap();
    session.run("feet.lock", &json!({})).unwrap();
    let r = session
        .run(
            "clip.export",
            &json!({ "path": out.to_string_lossy(), "target": character_path().to_string_lossy() }),
        )
        .unwrap();
    assert_eq!(r["joints"], 22);
    assert_eq!(r["animations"], 1);
    let feet = &r["target"]["feet"][0];
    for side in ["L", "R"] {
        let f = &feet[side];
        assert!(f["planted_frames"].as_u64().unwrap() > 10, "{f}");
        assert!(f["slide_cm"].as_f64().unwrap() < 1.0, "{side}: {f}");
        assert!(f["lowest_cm"].as_f64().unwrap() > -1.0, "{side}: {f}");
        assert_eq!(f["unreached_frames"], 0, "{side}: {f}");
    }

    // what the drive worked out, for comparison
    let sk = skeleton();
    let t = load(&character_path(), &BTreeMap::new(), None).unwrap();
    let clip = session.document.clip.clone();
    let (plan, report) = character::export::worlds(&sk, &t, &clip, None).unwrap();
    assert_eq!(report.frames.len(), 90);

    let bytes = std::fs::read(&out).unwrap();
    let (doc, buffers, _) = gltf::import_slice(&bytes).unwrap();
    let nodes: Vec<gltf::Node> = doc.nodes().collect();
    let parent: Vec<Option<usize>> = (0..nodes.len())
        .map(|i| nodes.iter().position(|n| n.children().any(|c| c.index() == i)))
        .collect();
    let anim = doc.animations().next().unwrap();
    assert_eq!(anim.name(), Some("Walk 12"));
    let frames = [0usize, 20, 61];
    for &f in &frames {
        // local TRS per node: the file's rest, with the channels' value on frame f
        let mut local: Vec<(DVec3, DQuat, DVec3)> = nodes
            .iter()
            .map(|n| {
                let (t, r, s) = n.transform().decomposed();
                (
                    DVec3::from(t.map(f64::from)),
                    DQuat::from_array(r.map(f64::from)),
                    DVec3::from(s.map(f64::from)),
                )
            })
            .collect();
        for ch in anim.channels() {
            let reader = ch.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let n = ch.target().node().index();
            match reader.read_outputs().unwrap() {
                gltf::animation::util::ReadOutputs::Rotations(r) => {
                    let q = r.into_f32().nth(f).unwrap();
                    local[n].1 = DQuat::from_array(q.map(f64::from));
                }
                gltf::animation::util::ReadOutputs::Translations(mut t) => {
                    let v = t.nth(f).unwrap();
                    local[n].0 = DVec3::from(v.map(f64::from));
                }
                _ => panic!("unexpected channel"),
            }
        }
        let mut world: Vec<Option<DMat4>> = vec![None; nodes.len()];
        fn world_of(
            i: usize,
            parent: &[Option<usize>],
            local: &[(DVec3, DQuat, DVec3)],
            world: &mut Vec<Option<DMat4>>,
        ) -> DMat4 {
            if let Some(w) = world[i] {
                return w;
            }
            let (t, r, s) = local[i];
            let m = DMat4::from_scale_rotation_translation(s, r, t);
            let w = match parent[i] {
                Some(p) => world_of(p, parent, local, world) * m,
                None => m,
            };
            world[i] = Some(w);
            w
        }
        for (i, node) in nodes.iter().enumerate() {
            let w = world_of(i, &parent, &local, &mut world);
            let want = report.frames[f].worlds[i];
            let want_gltf = c4() * DMat4::from_cols_array_2d(&transpose4(&want.matrix())) * c4().transpose();
            let got = w.to_cols_array();
            let exp = want_gltf.to_cols_array();
            let err = got
                .iter()
                .zip(&exp)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f64::max);
            // f32 in the file, centimetre translations on a 0.01 armature: 1e-4 m
            assert!(
                err < 2e-4,
                "frame {f}, node {} ({:?}): off by {err}",
                i,
                node.name()
            );
        }
    }
    // the feet stay above the character's floor (its soles at y = 0) and get near it: the toe
    // node sits 3 cm above the sole at rest and the walk's lowest toe comes within 6 cm
    let l_foot = nodes.iter().position(|n| n.name() == Some("L_Foot")).unwrap();
    for f in &report.feet {
        assert!(
            f.lowest > -0.01 && f.lowest < 0.06,
            "the feet stay above the floor and come near it: {}",
            f.lowest
        );
    }
    assert!(
        plan.drives
            .iter()
            .all(|d| d.node != l_foot || d.role == "l_ankle")
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_dir_all(&lib);
}

fn transpose4(m: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut t = [[0.0; 4]; 4];
    for (i, row) in m.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            t[j][i] = *x;
        }
    }
    t
}

#[test]
fn bad_input_is_refused_with_a_reason() {
    let (mut session, lib) = common::session("retarget-refused");
    let e = session
        .run("retarget.load", &json!({ "path": "nowhere/hero.glb" }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no character file"), "{e}");
    let e = session.run("retarget.check", &json!({})).unwrap_err().to_string();
    assert!(e.contains("no character is loaded"), "{e}");
    session
        .run(
            "retarget.load",
            &json!({ "path": character_path().to_string_lossy() }),
        )
        .unwrap();
    let e = session
        .run("retarget.map", &json!({ "joints": { "l_knee": "NoSuchNode" } }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no node named"), "{e}");
    let e = session
        .run("retarget.map", &json!({ "joints": { "l_kneecap": "L_Calf" } }))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a landmark"), "{e}");
    // one node for two landmarks: the mapping is kept but not ready, and the reason is said
    let r = session
        .run("retarget.map", &json!({ "joints": { "l_knee": "L_Foot" } }))
        .unwrap();
    assert_eq!(r["ready"], false, "{r}");
    assert!(
        r["notes"].as_array().unwrap().iter().any(|n| n
            .as_str()
            .unwrap_or_default()
            .contains("both name the node \"L_Foot\"")),
        "{r}"
    );
    let r = session
        .run("retarget.map", &json!({ "joints": { "l_knee": "L_Calf" } }))
        .unwrap();
    assert_eq!(r["ready"], true, "{r}");
    // unmapping a required landmark leaves the character loaded but not ready
    let r = session
        .run("retarget.map", &json!({ "joints": { "l_hip": "none" } }))
        .unwrap();
    assert_eq!(r["ready"], false);
    assert_eq!(r["missing"], json!(["l_hip"]));
    session
        .run("clip.open", &json!({ "id": "Locomotion/Walk 12" }))
        .unwrap();
    let e = session.run("retarget.check", &json!({})).unwrap_err().to_string();
    assert!(e.contains("l_hip"), "{e}");
    let e = session
        .run(
            "retarget.save",
            &json!({ "path": std::env::temp_dir().join("x.glb").to_string_lossy() }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains(".json"), "{e}");
    let e = session
        .run(
            "clip.export",
            &json!({ "path": std::env::temp_dir().join("x.glb").to_string_lossy(), "target": character_path().to_string_lossy(), "root": "ground" }),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("not supported yet"), "{e}");
    let _ = std::fs::remove_dir_all(&lib);
}
