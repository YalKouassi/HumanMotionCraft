//! Clips onto a character's own file: the `.glb` is written back as it was (its nodes, skin,
//! meshes, materials, textures) with one animation per clip on its own nodes: a rotation track
//! for every driven node and a translation track for the hips, sampled on every frame, in the
//! file's own axes and the node's parent space. Nodes nothing drives get no track and keep
//! their rest. The file's own animations, if any, are replaced and the summary says so.

use ac_core::character::check::{self, Report};
use ac_core::character::plan::Plan;
use ac_core::clip::Clip;
use ac_core::contact;
use ac_core::physics::Body;
use ac_core::quat::{self, Quat};
use ac_core::rig::Skeleton;
use serde_json::{Value, json};

use crate::export::{self, Item, MAX_FRAMES};

type R<T> = Result<T, String>;

/// Rig axes -> glTF (x, y, z) -> (x, z, -y): a local rotation matrix conjugated by the axis
/// change `C` (`C4`, rig -> glTF).
fn to_gltf_quat(r: &quat::Mat3) -> Quat {
    let c = export::rot3(&export::C4);
    let ct = export::rot3(&export::C4T);
    quat::from_mat3(&quat::mat3_mul(&quat::mat3_mul(&c, r), &ct))
}

fn to_gltf_vec(v: quat::Vec3) -> [f64; 3] {
    [v[0], v[2], -v[1]]
}

/// The motion of one clip as the export shows it: every frame, the physics pass where it plays.
fn motion(
    sk: &Skeleton,
    it: &Item,
    limits: Option<&ac_core::fullbody::Limits>,
) -> R<(Vec<Vec<Quat>>, Vec<quat::Vec3>, f64)> {
    let n = it.clip.length_with(it.links).max(1) as usize;
    let fr: Vec<f64> = (0..n).map(|f| f as f64).collect();
    let (mut rots, mut roots) = it.clip.sample_below(&fr, it.clip.layers.len(), it.links)?;
    if it.clip.physics {
        let r = ac_core::physics::solve_held(
            sk,
            &rots,
            &roots,
            it.clip.fps,
            &it.clip.surfaces,
            &it.clip.contacts,
            limits,
        )?;
        (rots, roots) = (r.rotations, r.roots);
    }
    let fps = if it.clip.fps > 0.0 { it.clip.fps } else { 30.0 };
    Ok((rots, roots, fps))
}

/// Drive one clip through the plan: its report (the frames' worlds and the feet's numbers).
pub fn drive_item(
    sk: &Skeleton,
    plan: &Plan,
    c: &ac_core::character::Character,
    it: &Item,
    limits: Option<&ac_core::fullbody::Limits>,
) -> R<Report> {
    let (rots, roots, fps) = motion(sk, it, limits)?;
    let body = Body::new(sk)?;
    let phases = contact::clip_phases(&body, sk, &rots, &roots, fps, &it.clip.surfaces)?;
    check::drive_clip(plan, c, sk, &rots, &roots, &phases)
}

fn clip_check(it: &Item, sk: &Skeleton) -> R<()> {
    if it.clip.bones != sk.names {
        return Err(format!("clip {:?}: its bones are not the rig's", it.name));
    }
    Ok(())
}

/// The character's `.glb` bytes with the clips as animations, and a summary.
pub fn export(
    sk: &Skeleton,
    target: &super::Target,
    items: &[Item],
    limits: Option<&ac_core::fullbody::Limits>,
    root: &str,
) -> R<(Vec<u8>, Value)> {
    if root != "hips" {
        return Err(format!(
            "root: {root:?} is not supported yet with a target character: the character's own hips carry the motion (root: hips)"
        ));
    }
    let l = &target.loaded;
    let c = &l.character;
    let plan = Plan::new(sk, c, &l.mapping)?;
    let (mut gltf, mut bin) = export::read_glb(&target.glb)?;
    let buffers = gltf
        .get("buffers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if buffers.len() > 1 || buffers.iter().any(|b| b.get("uri").is_some()) {
        return Err(
            "the character's data must be inside its .glb (one buffer, no uri); not supported yet".into(),
        );
    }
    if gltf.get("bufferViews").and_then(Value::as_array).is_none() {
        gltf["bufferViews"] = json!([]);
    }
    if gltf.get("accessors").and_then(Value::as_array).is_none() {
        gltf["accessors"] = json!([]);
    }
    let mut notes: Vec<String> = Vec::new();
    if let Some(a) = gltf.get("animations").and_then(Value::as_array)
        && !a.is_empty()
    {
        notes.push(format!(
            "the character's own {} animation(s) are replaced",
            a.len()
        ));
    }
    let mut frames_total = 0usize;
    for it in items {
        clip_check(it, sk)?;
        frames_total += it.clip.length_with(it.links).max(1) as usize;
    }
    if frames_total > MAX_FRAMES {
        return Err(format!("{frames_total} frames to export (most {MAX_FRAMES})"));
    }
    // driven nodes animated by rotation must carry TRS, not a matrix
    for d in &plan.drives {
        if let Some(node) = gltf["nodes"].get_mut(d.node)
            && node.get("matrix").is_some()
        {
            let n = &c.nodes[d.node];
            let obj = node.as_object_mut().ok_or("a node is not an object")?;
            obj.remove("matrix");
            obj.insert("translation".into(), json!(to_gltf_vec(n.translation)));
            obj.insert("rotation".into(), json!(to_gltf_quat(&quat::to_mat3(n.rotation))));
            obj.insert("scale".into(), json!([n.scale, n.scale, n.scale]));
        }
    }
    let mut animations = Vec::new();
    let mut frames_out = Vec::new();
    let mut feet_out = Vec::new();
    let mut problems_out = Vec::new();
    for it in items {
        let report = drive_item(sk, &plan, c, it, limits)?;
        let n = report.frames.len();
        let fps = if it.clip.fps > 0.0 { it.clip.fps } else { 30.0 };
        let times: Vec<f64> = (0..n).map(|f| f as f64 / fps).collect();
        let t_acc = export::append(&mut gltf, &mut bin, &times, "SCALAR", true)?;
        let (mut samplers, mut channels) = (Vec::new(), Vec::new());
        let mut push = |gltf: &mut Value,
                        bin: &mut Vec<u8>,
                        node: usize,
                        path: &str,
                        values: &[f64],
                        kind: &str|
         -> R<()> {
            let out = export::append(gltf, bin, values, kind, false)?;
            samplers.push(json!({ "input": t_acc, "output": out, "interpolation": "LINEAR" }));
            channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": node, "path": path } }));
            Ok(())
        };
        for d in &plan.drives {
            let parent = c.nodes[d.node].parent;
            let mut qs: Vec<Quat> = Vec::with_capacity(n);
            let mut ts: Vec<[f64; 3]> = Vec::with_capacity(n);
            for p in &report.frames {
                let w = p
                    .worlds
                    .get(d.node)
                    .copied()
                    .unwrap_or(ac_core::character::World::IDENTITY);
                let pw = parent
                    .and_then(|i| p.worlds.get(i).copied())
                    .unwrap_or(ac_core::character::World::IDENTITY);
                let (t, q) = pw.local_of(&w);
                qs.push(to_gltf_quat(&quat::to_mat3(q)));
                ts.push(to_gltf_vec(t));
            }
            for f in 1..qs.len() {
                if (0..4).map(|i| qs[f][i] * qs[f - 1][i]).sum::<f64>() < 0.0 {
                    qs[f] = qs[f].map(|x| -x);
                }
            }
            push(&mut gltf, &mut bin, d.node, "rotation", &qs.concat(), "VEC4")?;
            if d.node == plan.hips {
                push(&mut gltf, &mut bin, d.node, "translation", &ts.concat(), "VEC3")?;
            }
        }
        animations.push(json!({ "name": it.name, "samplers": samplers, "channels": channels }));
        frames_out.push(n);
        let foot = |f: &check::FootReport| json!({ "planted_frames": f.planted_frames, "slide_cm": r4(f.slide * 100.0), "travel_cm": r4(f.travel * 100.0), "lowest_cm": r4(f.lowest * 100.0), "unreached_frames": f.unreached_frames, "holds": super::commands::holds(f) });
        feet_out.push(json!({ "clip": it.name, "L": foot(&report.feet[0]), "R": foot(&report.feet[1]) }));
        problems_out.push(json!(
            report
                .problems
                .iter()
                .map(|p| json!({ "from": p.from, "to": p.to, "kind": p.kind, "label": p.text }))
                .collect::<Vec<_>>()
        ));
    }
    gltf["animations"] = Value::Array(animations);
    let names = |ix: &[usize]| -> Vec<String> {
        ix.iter()
            .filter_map(|&i| c.nodes.get(i).map(|n| n.name.clone()))
            .collect()
    };
    let driven: Vec<String> = plan.drives.iter().map(|d| c.nodes[d.node].name.clone()).collect();
    let undriven = names(&plan.undriven);
    let scene = gltf["scenes"].get_mut(0).and_then(Value::as_object_mut);
    if let Some(s) = scene {
        s.insert(
            "extras".into(),
            json!({ ac_core::product::ID: {
                "character": l.path.to_string_lossy(),
                "driven": driven,
                "undriven": undriven,
                "clips": items.iter().map(|i| &i.name).collect::<Vec<_>>(),
            }}),
        );
    }
    notes.extend(plan.notes.iter().cloned());
    let bytes = export::write_glb(&mut gltf, bin)?;
    let summary = json!({
        "bytes": bytes.len(),
        "joints": plan.drives.len(),
        "animations": items.len(),
        "frames": frames_out,
        "target": {
            "character": l.path.to_string_lossy(),
            "driven": plan.drives.len(),
            "undriven": undriven,
            "scale": r4(plan.scale),
            "feet": feet_out,
            "problems": problems_out,
            "notes": notes,
        },
    });
    Ok((bytes, summary))
}

fn r4(x: f64) -> f64 {
    if x.is_finite() {
        (x * 1e4).round() / 1e4
    } else {
        0.0
    }
}

/// The world transforms of every node on each frame of a clip, for tests.
pub fn worlds(
    sk: &Skeleton,
    target: &super::Target,
    clip: &Clip,
    limits: Option<&ac_core::fullbody::Limits>,
) -> R<(Plan, Report)> {
    let plan = Plan::new(sk, &target.loaded.character, &target.loaded.mapping)?;
    let it = Item {
        name: clip.name.clone(),
        clip,
        links: &ac_core::clip::NoLinks,
    };
    let r = drive_item(sk, &plan, &target.loaded.character, &it, limits)?;
    Ok((plan, r))
}
