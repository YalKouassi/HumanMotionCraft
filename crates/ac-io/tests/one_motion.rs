//! One busy animation (keys, a part layer, an additive layer, a link, a box, a marker, lock keys and
//! physics on) is the same motion through every route: what the viewport shows, the exported file,
//! save and reopen, and crash recovery. Pictures and video are drawn from `Document::shown_poses`,
//! so they share the first route.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::Session;
use glam::{DMat4, DQuat, DVec3};
use serde_json::json;

/// Rig (Z up) -> glTF (Y up).
fn to_gltf(p: [f64; 3]) -> DVec3 {
    DVec3::new(p[0], p[2], -p[1])
}

fn busy(name: &str) -> (Session, std::path::PathBuf) {
    let (mut s, lib) = common::session(name);
    s.run("clip.open", &json!({ "id": "Locomotion/Walk 12" }))
        .unwrap();
    s.run("clip.save_as", &json!({ "name": "Busy", "group": "Mine" }))
        .unwrap();
    s.run("feet.lock", &json!({})).unwrap();
    s.run("marker.add", &json!({ "frame": 12, "name": "step" }))
        .unwrap();
    s.run(
        "surface.add",
        &json!({ "name": "Step", "width": 1.0, "depth": 1.0, "top": 0.1 }),
    )
    .unwrap();
    s.run("layer.add", &json!({ "name": "Arm", "parts": ["Left arm"] }))
        .unwrap();
    s.run("frame.set", &json!({ "frame": 10 })).unwrap();
    let q = [0.0, 0.0, 0.2, 0.9797958971132712];
    s.run("pose.set_bone", &json!({ "bone": "upperarm01.L", "rotation": q }))
        .unwrap();
    s.run("key.set", &json!({})).unwrap();
    s.run(
        "layer.add",
        &json!({ "name": "Lean", "mode": "additive", "parts": ["Head"] }),
    )
    .unwrap();
    s.run(
        "pose.set_bone",
        &json!({ "bone": "head", "rotation": [0.05, 0.0, 0.0, 0.99874921777] }),
    )
    .unwrap();
    s.run("key.set", &json!({})).unwrap();
    s.run(
        "layer.add",
        &json!({ "source": "link", "link": "Gestures/Nod", "name": "Link", "parts": ["Right arm"] }),
    )
    .unwrap();
    (s, lib)
}

#[test]
fn the_export_moves_as_the_viewport_shows() {
    let (mut s, lib) = busy("one-motion-export");
    assert!(s.document.physics_shown(), "the pass should play");
    let file = lib.join("busy.glb");
    s.run("clip.export", &json!({ "path": file.to_string_lossy() }))
        .unwrap();
    let doc = &s.document;
    let sk = &doc.skeleton;
    let n = doc.clip.length_with(&doc.links).max(1) as usize;
    let frames: Vec<f64> = (0..n).map(|f| f as f64).collect();
    let shown = doc.shown_poses(&frames).unwrap();

    let (gltf_doc, buffers, _) = gltf::import(&file).unwrap();
    let skin = gltf_doc.skins().next().unwrap();
    let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
    assert_eq!(joints.len(), sk.len());
    let nodes: Vec<gltf::Node> = gltf_doc.nodes().collect();
    let parent: Vec<Option<usize>> = (0..nodes.len())
        .map(|i| nodes.iter().position(|n| n.children().any(|c| c.index() == i)))
        .collect();
    let anim = gltf_doc.animations().next().unwrap();
    // every channel's samples, once
    let mut rotations: Vec<(usize, Vec<[f32; 4]>)> = Vec::new();
    let mut translations: Vec<(usize, Vec<[f32; 3]>)> = Vec::new();
    for ch in anim.channels() {
        let node = ch.target().node().index();
        let rd = ch.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
        match rd.read_outputs().unwrap() {
            gltf::animation::util::ReadOutputs::Rotations(q) => {
                rotations.push((node, q.into_f32().collect()));
            }
            gltf::animation::util::ReadOutputs::Translations(v) => translations.push((node, v.collect())),
            _ => {}
        }
    }
    assert!(
        rotations.iter().all(|(_, v)| v.len() == n),
        "the file has other than {n} frames"
    );
    let ibm: Vec<DMat4> = skin
        .reader(|b| buffers.get(b.index()).map(|d| &d.0[..]))
        .read_inverse_bind_matrices()
        .unwrap()
        .map(|m| DMat4::from_cols_array_2d(&m.map(|c| c.map(f64::from))))
        .collect();

    for (f, pose) in shown.iter().enumerate() {
        let mut t: Vec<DVec3> = nodes
            .iter()
            .map(|n| DVec3::from(n.transform().decomposed().0.map(f64::from)))
            .collect();
        let mut r: Vec<DQuat> = nodes
            .iter()
            .map(|n| DQuat::from_array(n.transform().decomposed().1.map(f64::from)))
            .collect();
        for (node, v) in &rotations {
            r[*node] = DQuat::from_array(v[f].map(f64::from));
        }
        for (node, v) in &translations {
            t[*node] = DVec3::from(v[f].map(f64::from));
        }
        let local: Vec<DMat4> = (0..nodes.len())
            .map(|i| DMat4::from_rotation_translation(r[i], t[i]))
            .collect();
        let world = |mut i: usize| {
            let mut m = local[i];
            while let Some(p) = parent[i] {
                m = local[p] * m;
                i = p;
            }
            m
        };
        let (_, want) = sk.fk(&pose.rotations, pose.root).unwrap();
        for (k, &j) in joints.iter().enumerate() {
            let got = (world(j) * ibm[k]).transform_point3(to_gltf(sk.heads[k]));
            let d = got.distance(to_gltf(want[k]));
            assert!(d < 1e-4, "frame {f} joint {}: the file is {d} m off", sk.names[k]);
        }
    }
    std::fs::remove_dir_all(lib).ok();
}

/// The clip with what naming and saving change put back.
fn same_motion(s: &Session, was: &ac_core::clip::Clip) {
    let mut now = s.document.clip.clone();
    now.name = was.name.clone();
    now.group = was.group.clone();
    now.origin = was.origin.clone();
    // which layer is being edited isn't saved: a clip opens on its base layer
    now.edit = was.edit;
    if &now != was {
        let (a, b) = (format!("{now:?}"), format!("{was:?}"));
        let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(0);
        panic!(
            "the clip changed at byte {at} of {}:\n{}\n---\n{}",
            a.len(),
            &a[at.saturating_sub(200)..(at + 120).min(a.len())],
            &b[at.saturating_sub(200)..(at + 120).min(b.len())]
        );
    }
}

#[test]
fn saving_and_recovery_keep_the_animation() {
    let (mut s, lib) = busy("one-motion-save");
    s.run("clip.save", &json!({})).unwrap();
    let was = s.document.clip.clone();
    let id = s.document.clip_id.clone().unwrap();
    let n = was.length_with(&s.document.links).max(1) as usize;
    let frames: Vec<f64> = (0..n).map(|f| f as f64).collect();
    let shown = s.document.shown_poses(&frames).unwrap();

    s.run("clip.new", &json!({ "name": "Other", "group": "Mine" }))
        .unwrap();
    s.run("clip.open", &json!({ "id": id })).unwrap();
    same_motion(&s, &was);
    assert_eq!(s.document.shown_poses(&frames).unwrap(), shown, "reopened");

    // unsaved work, then a crash: recovery brings it back
    s.run("frame.set", &json!({ "frame": 20 })).unwrap();
    s.run("key.set", &json!({})).unwrap();
    let unsaved = s.document.clip.clone();
    let written = s.run("recovery.write", &json!({})).unwrap();
    s.run("clip.new", &json!({ "name": "Other 2", "group": "Mine" }))
        .unwrap();
    s.run("recovery.restore", &json!({ "file": written["file"] }))
        .unwrap();
    same_motion(&s, &unsaved);
    std::fs::remove_dir_all(lib).ok();
}
