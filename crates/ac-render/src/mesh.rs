//! A skinned mesh from a glTF binary (the mannequin, or a character of the person's own):
//! positions and normals in rig space (Z up, the rig faces -Y), four joints and weights per
//! vertex, triangle indices. Every primitive skinned to the file's first skin is taken, one
//! after the other (a character is often several: body, hair, clothes).

use bytemuck::{Pod, Zeroable};

/// Most vertices and indices a mesh may have (a hostile file is refused, not allocated).
pub const MAX_VERTICES: usize = 4_000_000;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub joints: [u32; 4],
    pub weights: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// The skin's joint names, in the order `Vertex::joints` indexes them.
    pub joints: Vec<String>,
}

/// glTF (Y up, facing +Z) -> rig (Z up, facing -Y): (x, y, z) -> (x, -z, y).
fn to_rig(v: [f32; 3]) -> [f32; 3] {
    [v[0], -v[2], v[1]]
}

impl Mesh {
    /// Every primitive of a `.glb` skinned to its first skin, in file order.
    pub fn from_glb(bytes: &[u8]) -> Result<Mesh, String> {
        let (doc, buffers, _) =
            gltf::import_slice(bytes).map_err(|e| format!("not a readable glTF file: {e}"))?;
        let skin = doc.skins().next().ok_or("the file has no skin (joints)")?;
        let joints: Vec<String> = skin
            .joints()
            .map(|n| n.name().unwrap_or_default().to_string())
            .collect();
        let n_joints = joints.len() as u32;
        // the meshes on nodes skinned by the first skin; a file with no such node: the first mesh
        let mut mesh_indices: Vec<usize> = doc
            .nodes()
            .filter(|n| n.skin().is_some_and(|s| s.index() == skin.index()))
            .filter_map(|n| n.mesh().map(|m| m.index()))
            .collect();
        mesh_indices.dedup();
        if mesh_indices.is_empty() {
            mesh_indices.push(doc.meshes().next().ok_or("the file has no mesh")?.index());
        }
        let mut vertices = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for mi in mesh_indices {
            let Some(mesh) = doc.meshes().nth(mi) else {
                continue;
            };
            for prim in mesh.primitives() {
                let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
                let Some(positions) = reader.read_positions() else {
                    continue;
                };
                let positions: Vec<[f32; 3]> = positions.collect();
                let (Some(js), Some(ws)) = (reader.read_joints(0), reader.read_weights(0)) else {
                    continue;
                };
                let js: Vec<[u16; 4]> = js.into_u16().collect();
                let ws: Vec<[f32; 4]> = ws.into_f32().collect();
                if vertices.len() + positions.len() > MAX_VERTICES {
                    return Err(format!("more than {MAX_VERTICES} vertices"));
                }
                let normals: Vec<[f32; 3]> = match reader.read_normals() {
                    Some(n) => n.collect(),
                    None => vec![[0.0, 1.0, 0.0]; positions.len()],
                };
                if normals.len() != positions.len()
                    || js.len() != positions.len()
                    || ws.len() != positions.len()
                {
                    return Err("vertex attributes have different lengths".into());
                }
                let base = vertices.len() as u32;
                for (((p, n), j), w) in positions.iter().zip(&normals).zip(&js).zip(&ws) {
                    let j = j.map(u32::from);
                    if j.iter().any(|&x| x >= n_joints) {
                        return Err(format!("a vertex names joint {j:?} of {n_joints}"));
                    }
                    vertices.push(Vertex {
                        position: to_rig(*p),
                        normal: to_rig(*n),
                        joints: j,
                        weights: *w,
                    });
                }
                let prim_indices: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().collect(),
                    None => (0..positions.len() as u32).collect(),
                };
                if prim_indices.len() > MAX_VERTICES * 3
                    || prim_indices.iter().any(|&i| i as usize >= positions.len())
                {
                    return Err("bad triangle indices".into());
                }
                indices.extend(prim_indices.iter().map(|i| i + base));
            }
        }
        if vertices.is_empty() {
            return Err("the file has no primitive skinned to its skin".into());
        }
        Ok(Mesh {
            vertices,
            indices,
            joints,
        })
    }
}
