//! The mannequin mesh from a glTF binary: positions and normals in rig space (Z up, the rig faces
//! -Y), four joints and weights per vertex, triangle indices.

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
    /// The first skinned primitive of a `.glb`.
    pub fn from_glb(bytes: &[u8]) -> Result<Mesh, String> {
        let (doc, buffers, _) =
            gltf::import_slice(bytes).map_err(|e| format!("not a readable glTF file: {e}"))?;
        let skin = doc.skins().next().ok_or("the file has no skin (joints)")?;
        let joints: Vec<String> = skin
            .joints()
            .map(|n| n.name().unwrap_or_default().to_string())
            .collect();
        let mesh = doc.meshes().next().ok_or("the file has no mesh")?;
        let prim = mesh.primitives().next().ok_or("the mesh has no primitives")?;
        let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
        let positions: Vec<[f32; 3]> = reader.read_positions().ok_or("no positions")?.collect();
        if positions.len() > MAX_VERTICES {
            return Err(format!("{} vertices (most {MAX_VERTICES})", positions.len()));
        }
        let normals: Vec<[f32; 3]> = match reader.read_normals() {
            Some(n) => n.collect(),
            None => vec![[0.0, 1.0, 0.0]; positions.len()],
        };
        let js: Vec<[u16; 4]> = reader
            .read_joints(0)
            .ok_or("no skin joints")?
            .into_u16()
            .collect();
        let ws: Vec<[f32; 4]> = reader
            .read_weights(0)
            .ok_or("no skin weights")?
            .into_f32()
            .collect();
        if normals.len() != positions.len() || js.len() != positions.len() || ws.len() != positions.len() {
            return Err("vertex attributes have different lengths".into());
        }
        let n_joints = joints.len() as u32;
        let mut vertices = Vec::with_capacity(positions.len());
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
        let indices: Vec<u32> = match reader.read_indices() {
            Some(i) => i.into_u32().collect(),
            None => (0..vertices.len() as u32).collect(),
        };
        if indices.len() > MAX_VERTICES * 3 || indices.iter().any(|&i| i as usize >= vertices.len()) {
            return Err("bad triangle indices".into());
        }
        Ok(Mesh {
            vertices,
            indices,
            joints,
        })
    }
}
