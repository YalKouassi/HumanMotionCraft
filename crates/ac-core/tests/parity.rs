//! Parity with cf-animate: the goldens in tests/fixtures/golden (cf-animate/tools/dump_goldens.py).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use ac_core::quat;
use common::*;

#[test]
fn fk_matches_cf_animate() {
    let sk = skeleton();
    let cases = golden("fk");
    assert!(cases.len() >= 20);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let tol = f(&c["tolerance"]);
        let rots = list::<4>(&c["inputs"]["rotations"]);
        let root = arr::<3>(&c["inputs"]["root"]);
        let (r, p) = sk.fk(&rots, root).unwrap();
        let want_p: Vec<f64> = list::<3>(&c["expect"]["positions"]).concat();
        assert_close(name, "positions", &p.concat(), &want_p, tol);
        let want_r: Vec<f64> = c["expect"]["world_rotations"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| list::<3>(m).concat())
            .collect();
        let got_r: Vec<f64> = r.iter().flat_map(|m| m.concat()).collect();
        assert_close(name, "world_rotations", &got_r, &want_r, tol);
    }
}

#[test]
fn quaternions_match_cf_animate() {
    for c in &golden("quat") {
        let name = c["name"].as_str().unwrap();
        let tol = f(&c["tolerance"]);
        let i = &c["inputs"];
        let (a, b, cq, d) = (
            arr::<4>(&i["a"]),
            arr::<4>(&i["b"]),
            arr::<4>(&i["c"]),
            arr::<4>(&i["d"]),
        );
        let t = f(&i["t"]);
        let e = &c["expect"];
        assert_close(
            name,
            "slerp",
            &quat::slerp(a, b, t),
            &arr::<4>(&e["slerp_ab"]),
            tol,
        );
        let s0 = quat::squad_control(a, b, cq);
        let s1 = quat::squad_control(b, cq, d);
        assert_close(name, "control", &s0, &arr::<4>(&e["control_b"]), tol);
        assert_close(
            name,
            "squad",
            &quat::squad(b, cq, s0, s1, t),
            &arr::<4>(&e["squad_bc"]),
            tol,
        );
        let euler = quat::from_euler(arr::<3>(&i["euler"]));
        assert_close(name, "from_euler", &euler, &arr::<4>(&e["from_euler"]), tol);
        assert_close(
            name,
            "to_euler",
            &quat::to_euler(a),
            &arr::<3>(&e["to_euler_a"]),
            tol,
        );
        assert_close(name, "log", &quat::log(a), &arr::<3>(&e["log_a"]), tol);
    }
}

#[test]
fn baked_rig_is_the_default_body() {
    let sk = skeleton();
    assert_eq!(sk.len(), 104);
    assert_eq!(sk.parents[0], None);
    for limb in [
        "upperarm01.L",
        "lowerarm01.L",
        "wrist.L",
        "upperleg01.R",
        "lowerleg01.R",
        "foot.R",
    ] {
        assert!(sk.bone(limb).is_some(), "{limb}");
    }
    for (profile, n) in [("full", 104), ("game", 68), ("body", 36)] {
        assert_eq!(sk.profiles.get(profile).map(Vec::len), Some(n), "{profile}");
    }
}

/// The clip cases come from `training/ac_train`'s port of cf-animate's `Clip.sample`, which since
/// has clamped smooth tangents (a hold before a long move stays put); the Rust and Python sides
/// must still agree.
#[test]
fn clip_sampling_matches_cf_animate() {
    use ac_core::clip::{Clip, Interp, Key};
    let sk = skeleton();
    for c in &golden("clip") {
        let name = c["name"].as_str().unwrap();
        let tol = f(&c["tolerance"]);
        let mut clip = Clip::new(sk.names.clone(), f(&c["inputs"]["fps"]));
        for k in c["inputs"]["keys"].as_array().unwrap() {
            let only = k["only"]
                .as_array()
                .map(|o| o.iter().map(|i| i.as_u64().unwrap() as usize).collect());
            let interp = Interp::parse(k["interp"].as_str().unwrap()).unwrap();
            let key = Key::new(
                k["frame"].as_i64().unwrap(),
                list::<4>(&k["rotations"]),
                arr::<3>(&k["root"]),
                interp,
            )
            .with_only(only);
            clip.set_key(key).unwrap();
        }
        let frames: Vec<f64> = c["inputs"]["frames"].as_array().unwrap().iter().map(f).collect();
        let (rots, roots) = clip.sample(&frames).unwrap();
        let want_r: Vec<f64> = c["expect"]["rotations"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|fr| list::<4>(fr).concat())
            .collect();
        let got_r: Vec<f64> = rots.iter().flat_map(|fr| fr.concat()).collect();
        assert_close(name, "rotations", &got_r, &want_r, tol);
        let want_o: Vec<f64> = list::<3>(&c["expect"]["roots"]).concat();
        assert_close(name, "roots", &roots.concat(), &want_o, tol);
    }
}

#[test]
fn two_bone_ik_matches_cf_animate() {
    use ac_core::ik;
    let sk = skeleton();
    let cases = golden("two_bone");
    assert!(cases.len() > 50);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let tol = f(&c["tolerance"]);
        let i = &c["inputs"];
        let rots = list::<4>(&i["rotations"]);
        let root = arr::<3>(&i["root"]);
        match c["op"].as_str().unwrap() {
            "two_bone" => {
                let pole = if i["pole"].is_null() {
                    None
                } else {
                    Some(arr::<3>(&i["pole"]))
                };
                let got = ik::two_bone(
                    &sk,
                    &rots,
                    root,
                    i["limb"].as_str().unwrap(),
                    arr::<3>(&i["target"]),
                    pole,
                    None,
                )
                .unwrap();
                assert_close(
                    name,
                    "rotations",
                    &canon(&got),
                    &canon(&list::<4>(&c["expect"]["rotations"])),
                    tol,
                );
            }
            "move_root" => {
                let pinned: Vec<&str> = i["pinned"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap())
                    .collect();
                let (got, at) = ik::move_root(&sk, &rots, root, arr::<3>(&i["position"]), &pinned).unwrap();
                assert_close(
                    name,
                    "rotations",
                    &canon(&got),
                    &canon(&list::<4>(&c["expect"]["rotations"])),
                    tol,
                );
                assert_close(name, "root", &at, &arr::<3>(&c["expect"]["root"]), tol);
            }
            other => panic!("unknown op {other}"),
        }
    }
}

/// q and -q are the same rotation: compare with w >= 0.
fn canon(q: &[[f64; 4]]) -> Vec<f64> {
    q.iter()
        .flat_map(|q| if q[3] < 0.0 { q.map(|x| -x) } else { *q })
        .collect()
}
