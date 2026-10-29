//! The exported .glb, read back by an independent glTF reader, moves exactly as the rig does.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ac_core::Skeleton;
use ac_core::clipfile::load_clip;
use ac_io::export::{Item, export, keep_list};
use glam::{DMat3, DMat4, DQuat, DVec3};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

/// `physics`: export through the physics pass (the file must then move as the pass does).
fn check(profile: &str, physics: bool) {
    let sk =
        Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
            .unwrap();
    let mut clip = load_clip(
        &std::fs::read_to_string(repo().join("assets/library/animations/Locomotion/Walk.clip.json")).unwrap(),
    )
    .unwrap();
    clip.physics = physics;
    let mannequin = std::fs::read(repo().join("assets/rig/mannequin.glb")).unwrap();
    let keep = keep_list(&sk, profile, &[], &[]).unwrap();
    let items = [Item {
        name: "walk".into(),
        clip: &clip,
        links: &ac_core::clip::NoLinks,
    }];
    let (bytes, summary) = export(&sk, &mannequin, &items, &keep, None).unwrap();
    assert_eq!(summary["joints"], keep.len());

    let (doc, buffers, _) = gltf::import_slice(&bytes).unwrap();
    let skin = doc.skins().next().unwrap();
    let joints: Vec<usize> = skin.joints().map(|n| n.index()).collect();
    assert_eq!(joints.len(), keep.len());
    let ibm: Vec<DMat4> = skin
        .reader(|b| buffers.get(b.index()).map(|d| &d.0[..]))
        .read_inverse_bind_matrices()
        .unwrap()
        .map(|m| DMat4::from_cols_array_2d(&m.map(|c| c.map(f64::from))))
        .collect();
    let anim = doc.animations().next().unwrap();
    let nodes: Vec<gltf::Node> = doc.nodes().collect();
    let parent: Vec<Option<usize>> = (0..nodes.len())
        .map(|i| nodes.iter().position(|n| n.children().any(|c| c.index() == i)))
        .collect();

    for f in [0usize, 7, 20] {
        // the node locals at keyframe f: rest TRS, overridden by the animation's samples
        let mut t: Vec<DVec3> = nodes
            .iter()
            .map(|n| DVec3::from(n.transform().decomposed().0.map(f64::from)))
            .collect();
        let mut r: Vec<DQuat> = nodes
            .iter()
            .map(|n| DQuat::from_array(n.transform().decomposed().1.map(f64::from)))
            .collect();
        for ch in anim.channels() {
            let node = ch.target().node().index();
            let rd = ch.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            match rd.read_outputs().unwrap() {
                gltf::animation::util::ReadOutputs::Rotations(q) => {
                    r[node] = DQuat::from_array(q.into_f32().nth(f).unwrap().map(f64::from));
                }
                gltf::animation::util::ReadOutputs::Translations(v) => {
                    t[node] = DVec3::from(v.into_iter().nth(f).unwrap().map(f64::from))
                }
                _ => {}
            }
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
        let (mut rots, mut roots) = clip.sample(&[f as f64]).unwrap();
        if physics {
            let n = clip.length().max(1) as usize;
            let all: Vec<f64> = (0..n).map(|k| k as f64).collect();
            let (r, o) = clip.sample(&all).unwrap();
            let p = ac_core::physics::solve(&sk, &r, &o, clip.fps, &clip.surfaces, None).unwrap();
            (rots, roots) = (vec![p.rotations[f].clone()], vec![p.roots[f]]);
        }
        let (wr, wp) = sk.fk(&rots[0], roots[0]).unwrap();
        for (n, &j) in keep.iter().enumerate() {
            let got = world(joints[n]) * ibm[n];
            let rm = DMat3::from_cols_array_2d(&wr[j]).transpose();
            let rig = DMat4::from_translation(DVec3::from(wp[j]))
                * DMat4::from_mat3(rm)
                * DMat4::from_translation(-DVec3::from(sk.heads[j]));
            let want = c4() * rig * c4().transpose();
            let d = (got - want)
                .to_cols_array()
                .iter()
                .map(|x| x.abs())
                .fold(0.0, f64::max);
            assert!(d < 2e-4, "{profile} frame {f} joint {}: off by {d}", sk.names[j]);
        }
    }
}

#[test]
fn full_profile_moves_as_the_rig() {
    check("full", false);
}

#[test]
fn physics_export_moves_as_the_pass() {
    check("full", true);
}

#[test]
fn game_profile_keeps_its_joints_moving_as_the_rig() {
    check("game", false);
}
