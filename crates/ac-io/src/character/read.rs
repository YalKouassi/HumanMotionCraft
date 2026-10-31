//! A character's skeleton from a glTF binary (.glb): the nodes with their rest transforms, the
//! parents from the children lists, the first skin's joints and inverse bind matrices, and
//! whether a mesh is skinned to it. Our own reader, on the same container code the export uses
//! (`crate::export::read_glb`); only what the retarget needs is read (no mesh data: the
//! renderer reads that itself), and every index, count and number is checked, so a file that
//! lies is refused with the reason, never trusted.
//!
//! Everything comes out in the rig's axes (Z up, facing -Y): glTF's (x, y, z) -> (x, -z, y).

use std::path::Path;

use ac_core::character::{Character, MAX_NODES, Node};
use ac_core::quat::{self, Mat3, Quat, Vec3};
use serde_json::Value;

use crate::export::{self, M4};

type R<T> = Result<T, String>;

/// Largest character file read.
pub const MAX_FILE_BYTES: u64 = 256 << 20;
/// How far from uniform a node's scale may be before the file is refused.
const UNIFORM: f64 = 1e-4;

/// glTF (x, y, z) -> rig (x, -z, y).
fn to_rig(v: Vec3) -> Vec3 {
    [v[0], -v[2], v[1]]
}

/// glTF -> rig for a rotation: conjugate by the axis change `G` (`C4T`, glTF -> rig).
fn to_rig_mat(m: &Mat3) -> Mat3 {
    let g = export::rot3(&export::C4T);
    let gt = export::rot3(&export::C4);
    quat::mat3_mul(&quat::mat3_mul(&g, m), &gt)
}

fn number(v: &Value, what: &str) -> R<f64> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("{what}: not a number"))
}

fn numbers(v: Option<&Value>, n: usize, what: &str) -> R<Option<Vec<f64>>> {
    let Some(v) = v else { return Ok(None) };
    let a = v
        .as_array()
        .filter(|a| a.len() == n)
        .ok_or_else(|| format!("{what}: expected {n} numbers"))?;
    a.iter()
        .map(|x| number(x, what))
        .collect::<R<Vec<f64>>>()
        .map(Some)
}

/// A node's rest transform (rig axes) from its TRS or matrix.
fn transform(node: &Value, i: usize) -> R<(Vec3, Quat, f64)> {
    let what = |k: &str| format!("node {i} {k}");
    if let Some(m) = numbers(node.get("matrix"), 16, &what("matrix"))? {
        // column-major: column c is m[4c..4c+4]
        let col = |c: usize| [m[4 * c], m[4 * c + 1], m[4 * c + 2]];
        let t = col(3);
        let lens = [quat::len3(col(0)), quat::len3(col(1)), quat::len3(col(2))];
        let s = (lens[0] + lens[1] + lens[2]) / 3.0;
        if s < 1e-9 || lens.iter().any(|l| (l - s).abs() > UNIFORM * s.max(1.0)) {
            return Err(format!(
                "node {i}: its matrix scales the axes differently ({:.4}, {:.4}, {:.4}); not supported yet",
                lens[0], lens[1], lens[2]
            ));
        }
        let mut r = quat::MAT3_IDENTITY;
        for c in 0..3 {
            let v = quat::scale3(col(c), 1.0 / lens[c]);
            for (row, x) in r.iter_mut().zip(v) {
                row[c] = x;
            }
        }
        // orthonormal and right-handed, else it shears or mirrors
        let rt = ac_core::quat::mat3_mul(&transpose(&r), &r);
        let off = (0..3)
            .flat_map(|a| (0..3).map(move |b| (a, b)))
            .map(|(a, b)| (rt[a][b] - if a == b { 1.0 } else { 0.0 }).abs())
            .fold(0.0, f64::max);
        if off > 1e-3 {
            return Err(format!("node {i}: its matrix shears; not supported yet"));
        }
        if det(&r) < 0.0 {
            return Err(format!("node {i}: its matrix mirrors; not supported yet"));
        }
        return Ok((to_rig(t), quat::from_mat3(&to_rig_mat(&r)), s));
    }
    let t = numbers(node.get("translation"), 3, &what("translation"))?
        .map(|v| [v[0], v[1], v[2]])
        .unwrap_or([0.0; 3]);
    let q = numbers(node.get("rotation"), 4, &what("rotation"))?
        .map(|v| [v[0], v[1], v[2], v[3]])
        .unwrap_or(quat::IDENTITY);
    if quat::norm(q) < 1e-9 {
        return Err(format!("node {i}: its rotation is zero"));
    }
    let s = match numbers(node.get("scale"), 3, &what("scale"))? {
        Some(v) => {
            let s = (v[0] + v[1] + v[2]) / 3.0;
            if s <= 1e-9 || v.iter().any(|x| (x - s).abs() > UNIFORM * s.max(1.0)) {
                return Err(format!(
                    "node {i}: its scale is not uniform ({:.4}, {:.4}, {:.4}); not supported yet",
                    v[0], v[1], v[2]
                ));
            }
            s
        }
        None => 1.0,
    };
    let r = to_rig_mat(&quat::to_mat3(quat::normalize(q)));
    Ok((to_rig(t), quat::from_mat3(&r), s))
}

fn transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

fn det(m: &Mat3) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// What the file held besides the skeleton, for the result.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub nodes: usize,
    pub skin_joints: usize,
    pub meshes: usize,
    pub animations: usize,
    pub notes: Vec<String>,
}

/// The character in `bytes` (a .glb).
pub fn read_bytes(bytes: &[u8]) -> R<(Character, Info)> {
    let (gltf, bin) = export::read_glb(bytes)?;
    let nodes_v = gltf
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or("the file has no nodes")?;
    if nodes_v.is_empty() || nodes_v.len() > MAX_NODES {
        return Err(format!("the file has {} nodes (1 to {MAX_NODES})", nodes_v.len()));
    }
    let n = nodes_v.len();
    let mut parents: Vec<Option<usize>> = vec![None; n];
    for (i, node) in nodes_v.iter().enumerate() {
        for c in node
            .get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let c = c
                .as_u64()
                .and_then(|c| usize::try_from(c).ok())
                .filter(|&c| c < n)
                .ok_or_else(|| format!("node {i}: a child index is not a node"))?;
            if c == i {
                return Err(format!("node {i} is its own child"));
            }
            if parents[c].is_some() {
                return Err(format!("node {c} has two parents"));
            }
            parents[c] = Some(i);
        }
    }
    let mut nodes = Vec::with_capacity(n);
    for (i, node) in nodes_v.iter().enumerate() {
        let (translation, rotation, scale) = transform(node, i)?;
        nodes.push(Node {
            name: node
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            parent: parents[i],
            translation,
            rotation,
            scale,
        });
    }
    let mut info = Info {
        nodes: n,
        ..Info::default()
    };
    let skins = gltf.get("skins").and_then(Value::as_array);
    let mut skin = Vec::new();
    let mut ibm_rig: Vec<M4> = Vec::new();
    if let Some(s) = skins.and_then(|s| s.first()) {
        if skins.is_some_and(|s| s.len() > 1) {
            info.notes.push(format!(
                "the file has {} skins: the first is used",
                skins.map_or(0, Vec::len)
            ));
        }
        let joints = s
            .get("joints")
            .and_then(Value::as_array)
            .ok_or("the skin has no joints")?;
        if joints.len() > MAX_NODES {
            return Err(format!("the skin has {} joints (most {MAX_NODES})", joints.len()));
        }
        for j in joints {
            let j = j
                .as_u64()
                .and_then(|j| usize::try_from(j).ok())
                .filter(|&j| j < n)
                .ok_or("the skin names a joint that is not a node")?;
            skin.push(j);
        }
        match s.get("inverseBindMatrices").and_then(Value::as_u64) {
            Some(acc) => {
                let acc = usize::try_from(acc).map_err(|e| e.to_string())?;
                if gltf
                    .get("accessors")
                    .and_then(Value::as_array)
                    .is_none_or(|a| acc >= a.len())
                {
                    return Err("the skin's inverse bind matrices accessor is not there".into());
                }
                let (_, _, w, _) = export::locate(&gltf, acc)?;
                if w != 16 {
                    return Err("the skin's inverse bind matrices are not MAT4".into());
                }
                let m = export::read(&gltf, &bin, acc)?;
                if m.len() != skin.len() * 16 {
                    return Err(format!(
                        "the skin has {} joints but {} inverse bind matrices",
                        skin.len(),
                        m.len() / 16
                    ));
                }
                for j in 0..skin.len() {
                    let mut mm = [[0.0; 4]; 4];
                    for c in 0..4 {
                        for r in 0..4 {
                            mm[r][c] = m[j * 16 + c * 4 + r];
                        }
                    }
                    ibm_rig.push(export::mul(&export::mul(&export::C4T, &mm), &export::C4));
                }
            }
            None => {
                // no bind matrices: the bind pose is the rest pose
                info.notes.push(
                    "the skin has no inverse bind matrices: the rest pose is taken as the bind pose".into(),
                );
                ibm_rig = vec![[[0.0; 4]; 4]; skin.len()];
                let tmp = Character::new(nodes.clone(), Vec::new(), Vec::new(), false)?;
                let rest = tmp.rest_worlds();
                for (k, &j) in skin.iter().enumerate() {
                    let w = rest.get(j).map_or(ac_core::character::World::IDENTITY, |w| *w);
                    ibm_rig[k] = export::inverse(&w.matrix())?;
                }
            }
        }
        info.skin_joints = skin.len();
    }
    // a mesh skinned to it, and its lowest point
    let mut has_mesh = false;
    let mut lowest: Option<f64> = None;
    let meshes = gltf.get("meshes").and_then(Value::as_array);
    for node in nodes_v {
        let (Some(mesh), Some(_)) = (node.get("mesh").and_then(Value::as_u64), node.get("skin")) else {
            continue;
        };
        let Some(mesh) = meshes.and_then(|m| m.get(usize::try_from(mesh).unwrap_or(usize::MAX))) else {
            continue;
        };
        for prim in mesh
            .get("primitives")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let attrs = prim.get("attributes");
            if attrs.and_then(|a| a.get("JOINTS_0")).is_none() {
                continue;
            }
            has_mesh = true;
            if let Some(pos) = attrs.and_then(|a| a.get("POSITION")).and_then(Value::as_u64)
                && let Some(acc) = gltf
                    .get("accessors")
                    .and_then(Value::as_array)
                    .and_then(|a| a.get(usize::try_from(pos).unwrap_or(usize::MAX)))
                && let Some(min) = acc.get("min").and_then(Value::as_array)
                && let Some(y) = min.get(1).and_then(Value::as_f64)
                && y.is_finite()
            {
                lowest = Some(lowest.map_or(y, |l: f64| l.min(y)));
            }
        }
    }
    info.meshes = meshes.map_or(0, Vec::len);
    info.animations = gltf
        .get("animations")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if has_mesh && lowest.is_none() {
        info.notes
            .push("the mesh's positions have no min: the floor is taken at the lowest joint".into());
    }
    if skin.is_empty() {
        info.notes.push("the file has no skin: the character is drawn as joints and bones, and the export carries its nodes only".into());
    }
    let c = Character::new(nodes, skin, ibm_rig, has_mesh)?.with_lowest(lowest);
    Ok((c, info))
}

/// The character in a `.glb` file.
pub fn read_file(path: &Path) -> R<(Character, Info)> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("can't read {}: {e}", path.display()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "{} is {size} bytes, larger than {MAX_FILE_BYTES}",
            path.display()
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    read_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )]
    use super::*;
    use serde_json::json;

    fn glb(gltf: &Value, bin: Vec<u8>) -> Vec<u8> {
        export::write_glb(&mut gltf.clone(), bin).unwrap()
    }

    #[test]
    fn a_y_up_tree_comes_out_z_up_with_its_parents() {
        let g = json!({
            "asset": {"version": "2.0"},
            "nodes": [
                {"name": "Root", "children": [1], "scale": [0.01, 0.01, 0.01]},
                {"name": "Hips", "translation": [0.0, 90.0, 0.0], "children": [2]},
                {"name": "Spine", "translation": [0.0, 10.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0]}
            ],
            "bufferViews": [], "accessors": []
        });
        let (c, info) = read_bytes(&glb(&g, Vec::new())).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c.nodes[1].parent, Some(0));
        assert_eq!(c.nodes[2].parent, Some(1));
        // Y up 90 -> rig z 90 (cm, the root's scale applies in the world)
        assert_eq!(c.nodes[1].translation, [0.0, 0.0, 90.0]);
        let w = c.rest_worlds();
        assert!(
            (w[1].p[2] - 0.9).abs() < 1e-12 && (w[2].p[2] - 1.0).abs() < 1e-12,
            "{w:?}"
        );
        assert!(info.notes.iter().any(|n| n.contains("no skin")));
    }

    #[test]
    fn lies_are_refused() {
        let two_parents =
            json!({"nodes": [{"children": [2]}, {"children": [2]}, {}], "bufferViews": [], "accessors": []});
        assert!(
            read_bytes(&glb(&two_parents, Vec::new()))
                .unwrap_err()
                .contains("two parents")
        );
        let bad_child = json!({"nodes": [{"children": [9]}], "bufferViews": [], "accessors": []});
        assert!(
            read_bytes(&glb(&bad_child, Vec::new()))
                .unwrap_err()
                .contains("not a node")
        );
        let loop_ =
            json!({"nodes": [{"children": [1]}, {"children": [0]}], "bufferViews": [], "accessors": []});
        assert!(
            read_bytes(&glb(&loop_, Vec::new()))
                .unwrap_err()
                .contains("two parents")
                || read_bytes(&glb(&loop_, Vec::new())).unwrap_err().contains("loop")
        );
        let stretched = json!({"nodes": [{"scale": [1.0, 2.0, 1.0]}], "bufferViews": [], "accessors": []});
        assert!(
            read_bytes(&glb(&stretched, Vec::new()))
                .unwrap_err()
                .contains("not uniform")
        );
        let nan = json!({"nodes": [{"translation": [1.0, "x", 0.0]}], "bufferViews": [], "accessors": []});
        assert!(
            read_bytes(&glb(&nan, Vec::new()))
                .unwrap_err()
                .contains("not a number")
        );
        let skin = json!({"nodes": [{}], "skins": [{"joints": [5]}], "bufferViews": [], "accessors": []});
        assert!(
            read_bytes(&glb(&skin, Vec::new()))
                .unwrap_err()
                .contains("not a node")
        );
        let short = json!({"nodes": [{}], "skins": [{"joints": [0], "inverseBindMatrices": 0}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 64}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 1, "type": "MAT4"}]});
        assert!(
            read_bytes(&glb(&short, vec![0; 16]))
                .unwrap_err()
                .contains("past the buffer")
        );
        assert!(read_bytes(b"not a glb at all").is_err());
        // a count or offset that would overflow the arithmetic is refused, not computed
        let huge = json!({"nodes": [{}], "skins": [{"joints": [0], "inverseBindMatrices": 0}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 64}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 1152921504606846976u64, "type": "MAT4"}]});
        assert!(
            read_bytes(&glb(&huge, vec![0; 64]))
                .unwrap_err()
                .contains("too large")
        );
        let far = json!({"nodes": [{}], "skins": [{"joints": [0], "inverseBindMatrices": 0}],
            "bufferViews": [{"buffer": 0, "byteOffset": 18446744073709551615u64, "byteLength": 64}],
            "accessors": [{"bufferView": 0, "byteOffset": 16, "componentType": 5126, "count": 1, "type": "MAT4"}]});
        assert!(
            read_bytes(&glb(&far, vec![0; 64]))
                .unwrap_err()
                .contains("too large")
        );
    }
}
