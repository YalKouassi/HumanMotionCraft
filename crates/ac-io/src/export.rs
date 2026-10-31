//! Clips to a rigged glTF binary (.glb) for engines, with a choice of bones (after cf-animate's
//! `export.py`).
//!
//! The mannequin (`assets/rig/mannequin.glb`) is rewritten:
//! - **bone profile**: keep every joint (`full`) or the rig's `game` / `body` lists, or any list. A
//!   dropped joint hands its skin weights to its nearest kept ancestor and its children hang from
//!   that ancestor, so the mesh deforms as before wherever the kept joints move.
//! - **animations**: one per clip, sampled on every frame (LINEAR), in the kept joints' own parent
//!   space.
//!
//! Joints come from the inverse bind matrices (not the file's node transforms), so the export
//! moves exactly as the viewport draws: world_j = C · S_j · Cᵀ · bind_j, with S_j the rig-space
//! skinning matrix T(P) R T(-H) and C the rig -> glTF axes (x, y, z) -> (x, z, -y).

use std::collections::BTreeMap;
use std::path::Path;

use ac_core::clip::{Clip, Links};
use ac_core::quat::{self, Mat3};
use ac_core::rig::Skeleton;
use serde_json::{Value, json};

type R<T> = Result<T, String>;
pub(crate) type M4 = [[f64; 4]; 4];

const GLTF: u32 = 0x4654_6C67;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;
/// Most frames one export samples (all clips together).
pub const MAX_FRAMES: usize = 200_000;

pub(crate) const C4: M4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];
pub(crate) const C4T: M4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, -1.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];
pub(crate) const I4: M4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

pub(crate) fn mul(a: &M4, b: &M4) -> M4 {
    let mut m = [[0.0; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

/// Inverse of an affine matrix (last row 0 0 0 1).
#[allow(clippy::needless_range_loop)]
pub(crate) fn inverse(m: &M4) -> R<M4> {
    let a = |i: usize, j: usize| m[i][j];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1))
        - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    if det.abs() < 1e-12 || !det.is_finite() {
        return Err("a joint matrix can't be inverted".into());
    }
    let mut r = [[0.0; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            let (i1, i2, j1, j2) = ((j + 1) % 3, (j + 2) % 3, (i + 1) % 3, (i + 2) % 3);
            r[i][j] = (a(i1, j1) * a(i2, j2) - a(i1, j2) * a(i2, j1)) / det;
        }
    }
    for i in 0..3 {
        r[i][3] = -(0..3).map(|k| r[i][k] * m[k][3]).sum::<f64>();
    }
    r[3][3] = 1.0;
    Ok(r)
}

pub(crate) fn rot3(m: &M4) -> Mat3 {
    [
        [m[0][0], m[0][1], m[0][2]],
        [m[1][0], m[1][1], m[1][2]],
        [m[2][0], m[2][1], m[2][2]],
    ]
}

/// The rig-space skinning matrix T(P) R T(-H).
#[allow(clippy::needless_range_loop)]
fn skin(r: &Mat3, p: [f64; 3], h: [f64; 3]) -> M4 {
    let mut m = I4;
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = r[i][j];
        }
        m[i][3] = p[i] - (0..3).map(|k| r[i][k] * h[k]).sum::<f64>();
    }
    m
}

// ---- glb container ---------------------------------------------------------------------------

fn u32_at(d: &[u8], o: usize) -> R<u32> {
    d.get(o..o + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| "the glb is cut short".to_string())
}

pub fn read_glb(data: &[u8]) -> R<(Value, Vec<u8>)> {
    if u32_at(data, 0)? != GLTF || u32_at(data, 4)? != 2 {
        return Err("not a glTF 2 binary".into());
    }
    let (mut off, mut gltf, mut bin) = (12usize, None, Vec::new());
    while off + 8 <= data.len() {
        let (len, kind) = (u32_at(data, off)? as usize, u32_at(data, off + 4)?);
        let chunk = data
            .get(off + 8..off + 8 + len)
            .ok_or("a glb chunk runs past the end")?;
        if kind == JSON_CHUNK {
            gltf = Some(serde_json::from_slice(chunk).map_err(|e| format!("glb json: {e}"))?);
        } else if kind == BIN_CHUNK {
            bin = chunk.to_vec();
        }
        off += 8 + len;
    }
    Ok((gltf.ok_or("the glb has no JSON chunk")?, bin))
}

pub fn write_glb(gltf: &mut Value, mut bin: Vec<u8>) -> R<Vec<u8>> {
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    gltf["buffers"] = json!([{ "byteLength": bin.len() }]);
    let mut text = serde_json::to_vec(gltf).map_err(|e| e.to_string())?;
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    for x in [GLTF, 2, total as u32, text.len() as u32, JSON_CHUNK] {
        out.extend(x.to_le_bytes());
    }
    out.extend(&text);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(BIN_CHUNK.to_le_bytes());
    out.extend(&bin);
    Ok(out)
}

fn width(kind: &str) -> R<usize> {
    Ok(match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        k => return Err(format!("accessor type {k} is not supported")),
    })
}

/// (byte start, count, width, component type) of an accessor; refuses interleaved views.
pub(crate) fn locate(gltf: &Value, i: usize) -> R<(usize, usize, usize, u64)> {
    let acc = &gltf["accessors"][i];
    let view = &gltf["bufferViews"][acc["bufferView"].as_u64().ok_or("accessor without a view")? as usize];
    let ct = acc["componentType"]
        .as_u64()
        .ok_or("accessor without a component type")?;
    let w = width(acc["type"].as_str().unwrap_or_default())?;
    let size = match ct {
        5121 => 1,
        5123 => 2,
        5125 | 5126 => 4,
        c => return Err(format!("component type {c} is not supported")),
    };
    if let Some(s) = view["byteStride"].as_u64()
        && s as usize != size * w
    {
        return Err("interleaved accessors are not supported".into());
    }
    // every number here comes from the file: added and multiplied without overflow
    let too_large = || "an accessor's offset or count is too large".to_string();
    let start = view["byteOffset"]
        .as_u64()
        .unwrap_or(0)
        .checked_add(acc["byteOffset"].as_u64().unwrap_or(0))
        .and_then(|s| usize::try_from(s).ok())
        .ok_or_else(too_large)?;
    let count = usize::try_from(acc["count"].as_u64().unwrap_or(0)).map_err(|_| too_large())?;
    Ok((start, count, w, ct))
}

pub(crate) fn read(gltf: &Value, bin: &[u8], i: usize) -> R<Vec<f64>> {
    let (start, count, w, ct) = locate(gltf, i)?;
    let size = match ct {
        5121 => 1,
        5123 => 2,
        _ => 4,
    };
    let len = count
        .checked_mul(w)
        .and_then(|n| n.checked_mul(size))
        .ok_or("an accessor's count is too large")?;
    let raw = start
        .checked_add(len)
        .and_then(|end| bin.get(start..end))
        .ok_or("an accessor runs past the buffer")?;
    Ok(raw
        .chunks_exact(size)
        .map(|b| match ct {
            5121 => b[0] as f64,
            5123 => u16::from_le_bytes([b[0], b[1]]) as f64,
            5125 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
        })
        .collect())
}

fn write_in_place(gltf: &Value, bin: &mut [u8], i: usize, values: &[f64]) -> R<()> {
    let (start, count, w, ct) = locate(gltf, i)?;
    if count.checked_mul(w) != Some(values.len()) {
        return Err("accessor size changed".into());
    }
    let mut raw = Vec::new();
    for &v in values {
        match ct {
            5121 => raw.push(v as u8),
            5123 => raw.extend((v as u16).to_le_bytes()),
            5125 => raw.extend((v as u32).to_le_bytes()),
            _ => raw.extend((v as f32).to_le_bytes()),
        }
    }
    start
        .checked_add(raw.len())
        .and_then(|end| bin.get_mut(start..end))
        .ok_or("an accessor runs past the buffer")?
        .copy_from_slice(&raw);
    Ok(())
}

pub(crate) fn append(
    gltf: &mut Value,
    bin: &mut Vec<u8>,
    values: &[f64],
    kind: &str,
    minmax: bool,
) -> R<usize> {
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let w = width(kind)?;
    let start = bin.len();
    for &v in values {
        bin.extend((v as f32).to_le_bytes());
    }
    let views = gltf["bufferViews"].as_array_mut().ok_or("no bufferViews")?;
    views.push(json!({ "buffer": 0, "byteOffset": start, "byteLength": values.len() * 4 }));
    let view = views.len() - 1;
    let mut acc =
        json!({ "bufferView": view, "componentType": 5126, "count": values.len() / w, "type": kind });
    if minmax {
        let col = |j: usize| values.iter().skip(j).step_by(w).map(|&v| v as f32 as f64);
        acc["min"] = (0..w)
            .map(|j| col(j).fold(f64::INFINITY, f64::min))
            .collect::<Vec<_>>()
            .into();
        acc["max"] = (0..w)
            .map(|j| col(j).fold(f64::NEG_INFINITY, f64::max))
            .collect::<Vec<_>>()
            .into();
    }
    let accs = gltf["accessors"].as_array_mut().ok_or("no accessors")?;
    accs.push(acc);
    Ok(accs.len() - 1)
}

// ---- export ----------------------------------------------------------------------------------

/// The joints kept (always the root), in rig order: a profile of the rig, or a list of bones.
pub fn keep_list(sk: &Skeleton, profile: &str, bones: &[String], drop: &[String]) -> R<Vec<usize>> {
    let mut keep: Vec<bool> = if !bones.is_empty() {
        let mut k = vec![false; sk.len()];
        for b in bones {
            k[sk.names
                .iter()
                .position(|n| n == b)
                .ok_or_else(|| format!("no bone named {b:?}"))?] = true;
        }
        k
    } else if profile == "full" {
        vec![true; sk.len()]
    } else {
        let list = sk.profiles.get(profile).ok_or_else(|| {
            format!(
                "unknown profile {profile:?}; one of full, {}",
                sk.profiles.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;
        sk.names.iter().map(|n| list.contains(n)).collect()
    };
    for d in drop {
        let i = sk
            .names
            .iter()
            .position(|n| n == d)
            .ok_or_else(|| format!("no bone named {d:?}"))?;
        keep[i] = false;
    }
    keep[0] = true;
    Ok((0..sk.len()).filter(|&i| keep[i]).collect())
}

/// One clip to export: its name, the clip and what its link layers play.
pub struct Item<'a> {
    pub name: String,
    pub clip: &'a Clip,
    pub links: &'a dyn Links,
}

/// The `.glb` bytes: the mannequin, the kept joints and one animation per clip, plus a summary.
#[allow(clippy::needless_range_loop)]
pub fn export(
    sk: &Skeleton,
    mannequin: &[u8],
    items: &[Item],
    keep: &[usize],
    limits: Option<&ac_core::fullbody::Limits>,
    root: &str,
) -> R<(Vec<u8>, Value)> {
    let has_ground = match root {
        "hips" => false,
        "ground" | "in_place" => true,
        _ => {
            return Err(format!(
                "unknown root mode {root:?}; one of hips, ground, in_place"
            ));
        }
    };
    let (mut gltf, mut bin) = read_glb(mannequin)?;
    let nj = sk.len();
    let names: Vec<String> = gltf["nodes"]
        .as_array()
        .ok_or("the mannequin has no nodes")?
        .iter()
        .take(nj)
        .map(|n| n["name"].as_str().unwrap_or_default().to_string())
        .collect();
    if names != sk.names {
        return Err("the mannequin's joints are not the rig's".into());
    }
    let mut frames_total = 0usize;
    for it in items {
        if it.clip.bones != sk.names {
            return Err(format!("clip {:?}: its bones are not the rig's", it.name));
        }
        frames_total += it.clip.length_with(it.links).max(1) as usize;
    }
    if frames_total > MAX_FRAMES {
        return Err(format!("{frames_total} frames to export (most {MAX_FRAMES})"));
    }
    let new_of: BTreeMap<usize, usize> = keep.iter().enumerate().map(|(n, &j)| (j, n)).collect();
    let kept_ancestor = |mut j: usize| -> usize {
        while !new_of.contains_key(&j) {
            j = sk.parents[j].unwrap_or(0);
        }
        j
    };
    let new_parent: Vec<Option<usize>> = keep
        .iter()
        .map(|&j| sk.parents[j].map(|p| new_of[&kept_ancestor(p)]))
        .collect();

    // rest: the bind worlds from the inverse bind matrices (glTF stores them column-major)
    let skin_v = gltf["skins"][0].clone();
    let ibm_i = skin_v["inverseBindMatrices"]
        .as_u64()
        .ok_or("the skin has no inverse bind matrices")? as usize;
    let ibm = read(&gltf, &bin, ibm_i)?;
    if ibm.len() != nj * 16 {
        return Err("the skin's joints are not the rig's".into());
    }
    let ibm_m: Vec<M4> = (0..nj)
        .map(|j| {
            let mut m = [[0.0; 4]; 4];
            for c in 0..4 {
                for r in 0..4 {
                    m[r][c] = ibm[j * 16 + c * 4 + r];
                }
            }
            m
        })
        .collect();
    let bind: Vec<M4> = ibm_m.iter().map(inverse).collect::<R<_>>()?;
    let local_of = |world: &[M4], n: usize| -> R<M4> {
        match new_parent[n] {
            Some(p) => Ok(mul(&inverse(&world[keep[p]])?, &world[keep[n]])),
            None => Ok(world[keep[n]]),
        }
    };
    let mesh_node = gltf["nodes"][nj].clone();
    let mut nodes = Vec::new();
    let mut rest_t = Vec::new();
    if has_ground {
        nodes.push(json!({
            "name": "ground",
            "translation": [0.0, 0.0, 0.0],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "children": [1],
        }));
        rest_t.push([0.0, 0.0, 0.0]);
    }
    let node_offset = usize::from(has_ground);
    for n in 0..keep.len() {
        let l = local_of(&bind, n)?;
        let q = quat::from_mat3(&rot3(&l));
        let mut node =
            json!({ "name": sk.names[keep[n]], "translation": [l[0][3], l[1][3], l[2][3]], "rotation": q });
        let children: Vec<usize> = (0..keep.len())
            .filter(|&c| new_parent[c] == Some(n))
            .map(|c| c + node_offset)
            .collect();
        if !children.is_empty() {
            node["children"] = json!(children);
        }
        rest_t.push([l[0][3], l[1][3], l[2][3]]);
        nodes.push(node);
    }
    nodes.push(mesh_node);
    let mesh_at = nodes.len() - 1;
    gltf["nodes"] = Value::Array(nodes);
    gltf["scenes"][0]["nodes"] = json!([0, mesh_at]);
    gltf["nodes"][mesh_at]["skin"] = json!(0);

    // skin: the kept rows of the inverse bind matrices; dropped joints' weights to their owners
    let kept_ibm: Vec<f64> = if has_ground {
        let mut all_ibm = Vec::with_capacity(16 + keep.len() * 16);
        all_ibm.extend([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        all_ibm.extend(keep.iter().flat_map(|&j| ibm[j * 16..j * 16 + 16].to_vec()));
        all_ibm
    } else {
        keep.iter()
            .flat_map(|&j| ibm[j * 16..j * 16 + 16].to_vec())
            .collect()
    };
    let total_joints = if has_ground { keep.len() + 1 } else { keep.len() };
    let ibm_new = append(&mut gltf, &mut bin, &kept_ibm, "MAT4", false)?;
    gltf["skins"][0]["joints"] = json!((0..total_joints).collect::<Vec<_>>());
    gltf["skins"][0]["skeleton"] = json!(0);
    gltf["skins"][0]["inverseBindMatrices"] = json!(ibm_new);
    let remap_offset = usize::from(has_ground);
    let remap: Vec<f64> = (0..nj)
        .map(|j| (new_of[&kept_ancestor(j)] + remap_offset) as f64)
        .collect();
    let prims = gltf["meshes"][0]["primitives"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for prim in &prims {
        let attrs = &prim["attributes"];
        let mut sets: Vec<String> = attrs
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, _)| k.clone())
            .filter(|k| k.starts_with("JOINTS_"))
            .collect();
        sets.sort();
        for set in sets {
            let ja = attrs[&set].as_u64().ok_or("bad joints accessor")? as usize;
            let wa = attrs[set.replace("JOINTS", "WEIGHTS")]
                .as_u64()
                .ok_or("joints without weights")? as usize;
            let (j, w) = (read(&gltf, &bin, ja)?, read(&gltf, &bin, wa)?);
            if locate(&gltf, wa)?.3 != 5126 {
                return Err("only float skin weights are supported".into());
            }
            let mut jo = Vec::with_capacity(j.len());
            let mut wo = Vec::with_capacity(w.len());
            for (jv, wv) in j.as_chunks::<4>().0.iter().zip(w.as_chunks::<4>().0.iter()) {
                let mut js: Vec<f64> = jv
                    .iter()
                    .map(|&x| remap.get(x as usize).copied().unwrap_or(0.0))
                    .collect();
                let mut ws = wv.to_vec();
                // merge repeated joints of a vertex into the first slot
                for a in 0..4 {
                    for b in a + 1..4 {
                        if js[b] == js[a] && ws[b] > 0.0 && ws[a] > 0.0 {
                            ws[a] += ws[b];
                            ws[b] = 0.0;
                        }
                    }
                }
                for k in 0..4 {
                    if ws[k] == 0.0 {
                        js[k] = 0.0;
                    }
                }
                let mut order = [0usize, 1, 2, 3];
                order.sort_by(|&a, &b| ws[b].total_cmp(&ws[a]));
                jo.extend(order.iter().map(|&k| js[k]));
                wo.extend(order.iter().map(|&k| ws[k]));
            }
            write_in_place(&gltf, &mut bin, ja, &jo)?;
            write_in_place(&gltf, &mut bin, wa, &wo)?;
        }
    }

    // animations
    let mut animations = Vec::new();
    let mut frames_out = Vec::new();
    for it in items {
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
        if root == "in_place" {
            for (r, o) in rots.iter_mut().zip(&mut roots) {
                // remove horizontal displacement
                o[0] = 0.0;
                o[1] = 0.0;
                // remove yaw turn from root rotation
                let rot_m = quat::to_mat3(r[0]);
                let heading_rad = rot_m[1][0].atan2(rot_m[0][0]);
                let (s, c) = (-heading_rad).sin_cos();
                let inv_yaw: Mat3 = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
                let r_unturned = quat::mat3_mul(&inv_yaw, &rot_m);
                r[0] = quat::from_mat3(&r_unturned);
            }
        }
        let mut world: Vec<Vec<M4>> = Vec::with_capacity(n);
        let mut ground_m: Vec<M4> = Vec::with_capacity(n);
        for (r, o) in rots.iter().zip(&roots) {
            let (wr, wp) = sk.fk(r, *o).map_err(|e| e.to_string())?;
            if has_ground {
                let (gx, gy, yaw_val) = if root == "ground" {
                    let h = wr[0][1][0].atan2(wr[0][0][0]);
                    (wp[0][0], wp[0][1], h)
                } else {
                    (0.0, 0.0, 0.0)
                };
                let (s, c) = yaw_val.sin_cos();
                let r_ground_rig: Mat3 = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
                let mut gm = I4;
                let rg = quat::mat3_mul(&quat::mat3_mul(&rot3(&C4), &r_ground_rig), &rot3(&C4T));
                for r_idx in 0..3 {
                    for c_idx in 0..3 {
                        gm[r_idx][c_idx] = rg[r_idx][c_idx];
                    }
                }
                gm[0][3] = gx;
                gm[1][3] = 0.0; // Y is height in glTF: always 0.0
                gm[2][3] = -gy;
                ground_m.push(gm);
            }
            world.push(
                (0..nj)
                    .map(|j| mul(&mul(&mul(&C4, &skin(&wr[j], wp[j], sk.heads[j])), &C4T), &bind[j]))
                    .collect(),
            );
        }
        let fps = if it.clip.fps > 0.0 { it.clip.fps } else { 30.0 };
        let times: Vec<f64> = (0..n).map(|f| f as f64 / fps).collect();
        let t_acc = append(&mut gltf, &mut bin, &times, "SCALAR", true)?;
        let (mut samplers, mut channels) = (Vec::new(), Vec::new());
        if has_ground {
            let mut qs: Vec<[f64; 4]> = ground_m.iter().map(|l| quat::from_mat3(&rot3(l))).collect();
            for f in 1..qs.len() {
                if (0..4).map(|i| qs[f][i] * qs[f - 1][i]).sum::<f64>() < 0.0 {
                    qs[f] = qs[f].map(|x| -x);
                }
            }
            let mut tracks = vec![("rotation", qs.concat(), "VEC4")];
            let ts: Vec<[f64; 3]> = ground_m.iter().map(|l| [l[0][3], l[1][3], l[2][3]]).collect();
            if ts
                .iter()
                .any(|t| (0..3).any(|i| (t[i] - rest_t[0][i]).abs() > 1e-6))
            {
                tracks.push(("translation", ts.concat(), "VEC3"));
            }
            for (path, values, kind) in tracks {
                let out = append(&mut gltf, &mut bin, &values, kind, false)?;
                samplers.push(json!({ "input": t_acc, "output": out, "interpolation": "LINEAR" }));
                channels
                    .push(json!({ "sampler": samplers.len() - 1, "target": { "node": 0, "path": path } }));
            }
        }

        let inv_ground: Vec<M4> = if has_ground {
            ground_m.iter().map(inverse).collect::<R<_>>()?
        } else {
            Vec::new()
        };

        for k in 0..keep.len() {
            let target_node = k + node_offset;
            let locals: Vec<M4> = if has_ground && k == 0 {
                (0..n).map(|f| mul(&inv_ground[f], &world[f][keep[0]])).collect()
            } else {
                world.iter().map(|w| local_of(w, k)).collect::<R<Vec<M4>>>()?
            };
            let mut qs: Vec<[f64; 4]> = locals.iter().map(|l| quat::from_mat3(&rot3(l))).collect();
            for f in 1..qs.len() {
                if (0..4).map(|i| qs[f][i] * qs[f - 1][i]).sum::<f64>() < 0.0 {
                    qs[f] = qs[f].map(|x| -x);
                }
            }
            let mut tracks = vec![("rotation", qs.concat(), "VEC4")];
            let ts: Vec<[f64; 3]> = locals.iter().map(|l| [l[0][3], l[1][3], l[2][3]]).collect();
            if ts
                .iter()
                .any(|t| (0..3).any(|i| (t[i] - rest_t[target_node][i]).abs() > 1e-6))
            {
                tracks.push(("translation", ts.concat(), "VEC3"));
            }
            for (path, values, kind) in tracks {
                let out = append(&mut gltf, &mut bin, &values, kind, false)?;
                samplers.push(json!({ "input": t_acc, "output": out, "interpolation": "LINEAR" }));
                channels.push(
                    json!({ "sampler": samplers.len() - 1, "target": { "node": target_node, "path": path } }),
                );
            }
        }
        animations.push(json!({ "name": it.name, "samplers": samplers, "channels": channels }));
        frames_out.push(n);
    }
    if !animations.is_empty() {
        gltf["animations"] = Value::Array(animations);
    }
    let mut kept_names: Vec<String> = if has_ground {
        vec!["ground".to_string()]
    } else {
        Vec::new()
    };
    kept_names.extend(keep.iter().map(|&j| sk.names[j].clone()));
    gltf["scenes"][0]["extras"] = json!({ ac_core::product::ID: {
        "joints": kept_names,
        "dropped": sk.names.iter().enumerate().filter(|(j, _)| !new_of.contains_key(j)).map(|x| x.1).collect::<Vec<_>>(),
        "clips": items.iter().map(|i| &i.name).collect::<Vec<_>>(),
    }});
    let bytes = write_glb(&mut gltf, bin)?;
    let summary = json!({ "bytes": bytes.len(), "joints": total_joints, "animations": items.len(), "frames": frames_out });
    Ok((bytes, summary))
}

/// Read the mannequin next to the rig.
pub fn mannequin(assets: &Path) -> R<Vec<u8>> {
    let p = assets.join("rig/mannequin.glb");
    std::fs::read(&p).map_err(|e| format!("can't read {}: {e}", p.display()))
}

// ---- the command -------------------------------------------------------------------------------

use ac_core::Document;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use serde_json::Map;

pub static SPECS: &[CommandSpec] = &[CommandSpec::new("clip.export", "Export .glb", clip_export)
    .icon("export")
    .shortcut("Ctrl+E")
    .inputs(&[
        ParamSpec::new("path", K::Text, "the .glb to write").required(),
        ParamSpec::new(
            "ids",
            K::TextList,
            "library animations to export (default: the open one)",
        ),
        ParamSpec::new(
            "profile",
            K::Choice(&["full", "game", "body"]),
            "which joints to keep",
        ),
        ParamSpec::new("bones", K::TextList, "exactly these joints instead of a profile"),
        ParamSpec::new("drop", K::TextList, "joints to leave out as well"),
        ParamSpec::new(
            "root",
            K::Choice(&["hips", "ground", "in_place"]),
            "root motion bone: hips (pelvis only), ground (bone on the floor), in_place (ground bone with movement removed)",
        ),
        TARGET,
    ])
    .writes()
    .outputs(crate::returns::CLIP_EXPORT)];

/// The `target` of an export onto a character of the person's own.
pub const TARGET: ParamSpec = ParamSpec::new(
    "target",
    K::Text,
    "export onto your own character: its .glb (the mapping is guessed) or a mapping file saved by retarget.save; the file is written with the animation on the character's own nodes",
);

pub(crate) fn clip_export(doc: &mut Document, p: &Map<String, Value>) -> R<Value> {
    let path = std::path::PathBuf::from(cmd::text(p, "path").unwrap_or_default());
    if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("glb")) {
        return Err("the file must end in .glb".into());
    }
    let assets = doc
        .assets
        .clone()
        .ok_or("no assets folder is set (start the app with one)")?;
    let sk = doc.skeleton.clone();
    let keep = keep_list(
        &sk,
        cmd::text(p, "profile").unwrap_or("full"),
        &cmd::texts(p, "bones"),
        &cmd::texts(p, "drop"),
    )?;
    let ids = cmd::texts(p, "ids");
    let mut loaded: Vec<(String, Clip, BTreeMap<String, Clip>)> = Vec::new();
    for id in &ids {
        let lib = crate::commands::anims(doc)?;
        let clip = ac_core::clipfile::load_clip(&lib.read(id)?).map_err(|e| format!("{id}: {e}"))?;
        let links = crate::commands::load_links(&lib, &clip)?;
        if ac_core::evaluation::marked(&clip, &links) {
            return Err(ac_core::evaluation::refusal(&format!("exporting {id}")));
        }
        loaded.push((id.clone(), clip, links));
    }
    let mut items: Vec<Item> = loaded
        .iter()
        .map(|(id, c, l)| Item {
            name: id.rsplit('/').next().unwrap_or(id).to_string(),
            clip: c,
            links: l,
        })
        .collect();
    if ids.is_empty() {
        ac_core::evaluation::check(doc, "exporting it")?;
        if doc.clip.base().keys.is_empty() {
            return Err("the animation has no keys to export".into());
        }
        items.push(Item {
            name: doc.clip.name.clone(),
            clip: &doc.clip,
            links: &doc.links,
        });
    }
    let root = cmd::text(p, "root").unwrap_or("hips");
    let target = crate::character::commands::target_param(p)?;
    let (bytes, mut summary) = match &target {
        Some(t) => crate::character::export::export(&sk, t, &items, doc.limits.as_deref(), root)?,
        None => export(
            &sk,
            &mannequin(&assets)?,
            &items,
            &keep,
            doc.limits.as_deref(),
            root,
        )?,
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    craft_atomic::atomic_write(&path, &bytes).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    summary["path"] = json!(path.to_string_lossy());
    Ok(summary)
}
