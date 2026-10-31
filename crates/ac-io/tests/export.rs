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
/// `root`: `hips`, or `ground` (the ground bone in front of the kept joints; every joint's world
/// pose must still be the rig's, so the hips' local under the ground absorbs the ground's path).
fn check(profile: &str, physics: bool, root: &str) {
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
    let (bytes, summary) = export(&sk, &mannequin, &items, &keep, None, root).unwrap();
    // with a ground bone the skin's first joint is the ground; the kept joints follow it
    let off = usize::from(root != "hips");
    assert_eq!(summary["joints"], keep.len() + off);

    let (doc, buffers, _) = gltf::import_slice(&bytes).unwrap();
    let skin = doc.skins().next().unwrap();
    let joints: Vec<usize> = skin.joints().map(|n| n.index()).collect();
    assert_eq!(joints.len(), keep.len() + off);
    if off == 1 {
        assert_eq!(doc.nodes().nth(joints[0]).unwrap().name(), Some("ground"));
    }
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
            let got = world(joints[n + off]) * ibm[n + off];
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
    check("full", false, "hips");
}

#[test]
fn physics_export_moves_as_the_pass() {
    check("full", true, "hips");
}

#[test]
fn game_profile_keeps_its_joints_moving_as_the_rig() {
    check("game", false, "hips");
}

/// 122500's acceptance: with `ground`, every joint's world pose equals the `hips` export's (the
/// rig's own), the ground bone's path and yaw being absorbed by the hips' local under it.
#[test]
fn ground_root_export_keeps_every_world_pose_the_rigs() {
    check("game", false, "ground");
    check("full", true, "ground");
}

#[test]
fn ground_root_bone_stays_at_height_0_and_matches_hips_path() {
    let sk =
        Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
            .unwrap();
    let mut clip = load_clip(
        &std::fs::read_to_string(repo().join("assets/library/animations/Locomotion/Walk.clip.json")).unwrap(),
    )
    .unwrap();
    clip.physics = false;
    let mannequin = std::fs::read(repo().join("assets/rig/mannequin.glb")).unwrap();
    let keep = keep_list(&sk, "game", &[], &[]).unwrap();
    let items = [Item {
        name: "walk".into(),
        clip: &clip,
        links: &ac_core::clip::NoLinks,
    }];
    let (bytes, summary) = export(&sk, &mannequin, &items, &keep, None, "ground").unwrap();
    assert_eq!(summary["joints"], keep.len() + 1);

    let (doc, buffers, _) = gltf::import_slice(&bytes).unwrap();
    let nodes: Vec<gltf::Node> = doc.nodes().collect();
    assert_eq!(nodes[0].name(), Some("ground"));
    let anim = doc.animations().next().unwrap();

    let mut t_ground: Vec<DVec3> = vec![DVec3::ZERO; clip.length() as usize];
    for ch in anim.channels() {
        if ch.target().node().index() == 0 {
            let rd = ch.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            if let Some(gltf::animation::util::ReadOutputs::Translations(v)) = rd.read_outputs() {
                t_ground = v.map(|c| DVec3::from(c.map(f64::from))).collect();
            }
        }
    }

    for (f, &tg) in t_ground.iter().enumerate() {
        // Ground bone stays at height 0 (Y in glTF is up)
        assert!(tg.y.abs() < 1e-5, "frame {f}: ground height is {}", tg.y);
        let (rots, roots) = clip.sample(&[f as f64]).unwrap();
        let (_wr, wp) = sk.fk(&rots[0], roots[0]).unwrap();
        // Path matches hips horizontal path (X and -Y in rig space map to X and Z in glTF) within 1 mm (0.001 m)
        let diff_x = (tg.x - wp[0][0]).abs();
        let diff_z = (tg.z - (-wp[0][1])).abs();
        assert!(diff_x < 0.001, "frame {f}: diff_x {diff_x}");
        assert!(diff_z < 0.001, "frame {f}: diff_z {diff_z}");
    }
}

#[test]
fn in_place_export_has_zero_horizontal_path() {
    let sk =
        Skeleton::from_json(&std::fs::read_to_string(repo().join("assets/rig/default_rig.json")).unwrap())
            .unwrap();
    let clip = load_clip(
        &std::fs::read_to_string(repo().join("assets/library/animations/Locomotion/Walk.clip.json")).unwrap(),
    )
    .unwrap();
    let mannequin = std::fs::read(repo().join("assets/rig/mannequin.glb")).unwrap();
    let keep = keep_list(&sk, "game", &[], &[]).unwrap();
    let items = [Item {
        name: "walk".into(),
        clip: &clip,
        links: &ac_core::clip::NoLinks,
    }];
    let (bytes, summary) = export(&sk, &mannequin, &items, &keep, None, "in_place").unwrap();
    assert_eq!(summary["joints"], keep.len() + 1);

    let (doc, buffers, _) = gltf::import_slice(&bytes).unwrap();
    let anim = doc.animations().next().unwrap();
    let mut t_hips: Vec<DVec3> = vec![DVec3::ZERO; clip.length() as usize];
    for ch in anim.channels() {
        // Node 1 is root (hips)
        if ch.target().node().index() == 1 {
            let rd = ch.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            if let Some(gltf::animation::util::ReadOutputs::Translations(v)) = rd.read_outputs() {
                t_hips = v.map(|c| DVec3::from(c.map(f64::from))).collect();
            }
        }
    }
    let rest_x = t_hips[0].x;
    let rest_z = t_hips[0].z;
    for (f, &th) in t_hips.iter().enumerate() {
        assert!((th.x - rest_x).abs() < 1e-4, "frame {f}: hips horizontal X moved");
        assert!((th.z - rest_z).abs() < 1e-4, "frame {f}: hips horizontal Z moved");
    }
}
