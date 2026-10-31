//! A character of someone's own as the target of an export: its skeleton (the nodes of a glTF
//! file, read by `ac-io`), the mapping from the retarget's landmarks onto those nodes, and the
//! drive that moves the character as the rig moves (roadmap 146000).
//!
//! The data here is in the rig's axes (metres, Z up, facing -Y): the reader converts from glTF's
//! Y up, and the export converts back. The character's rest pose is whatever the file holds (an
//! A-pose, a T-pose): each driven node turns, in world space, by the rig bone's own world turn,
//! after an alignment that points the node's rest segment the way the rig's rest segment points
//! ([`plan`]). The hips are placed from the rig's hip joints scaled by the legs' length ratio;
//! the legs are solved with two-bone IK onto the rig's ankles on planted frames ([`drive`]), and
//! [`check`] measures how the character's feet behaved over a clip, for the fixture, the command
//! and the timeline marks.

pub mod check;
pub mod drive;
pub mod plan;

use std::collections::BTreeMap;

use crate::quat::{self, Mat3, Quat, Vec3};

/// More nodes than any character we load.
pub const MAX_NODES: usize = crate::rig::MAX_BONES;

/// One node of the character's file, at rest, in its parent's space.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub parent: Option<usize>,
    /// Rig axes, metres (the parent's scale not applied).
    pub translation: Vec3,
    pub rotation: Quat,
    /// Uniform scale (a file with a non-uniform one is refused by the reader).
    pub scale: f64,
}

/// A character's skeleton: its nodes, its skin (which nodes are joints, their inverse bind
/// matrices) and whether a skinned mesh came with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Character {
    pub nodes: Vec<Node>,
    /// Node indices in the order the skin lists them (empty: the file has no skin).
    pub skin: Vec<usize>,
    /// One per skin joint, rig axes, row-major: maps the bind-pose mesh into the joint's space.
    pub bind_inverse: Vec<[[f64; 4]; 4]>,
    /// The file has a mesh skinned to the skin.
    pub has_mesh: bool,
    /// The mesh's lowest point at rest (rig z), when the file has one: the character's floor.
    pub lowest: Option<f64>,
    /// Nodes parents first.
    order: Vec<usize>,
}

/// A node's world transform: position, rotation and uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct World {
    pub p: Vec3,
    pub r: Mat3,
    pub s: f64,
}

impl World {
    pub const IDENTITY: World = World {
        p: [0.0; 3],
        r: quat::MAT3_IDENTITY,
        s: 1.0,
    };

    /// The child's world from its local transform.
    pub fn child(&self, translation: Vec3, rotation: Quat, scale: f64) -> World {
        World {
            p: quat::add3(self.p, quat::mat3_vec(&self.r, quat::scale3(translation, self.s))),
            r: quat::mat3_mul(&self.r, &quat::to_mat3(rotation)),
            s: self.s * scale,
        }
    }

    /// The local transform (translation, rotation) of a world under this one as its parent.
    pub fn local_of(&self, w: &World) -> (Vec3, Quat) {
        let rt = transpose(&self.r);
        let s = if self.s.abs() > 1e-12 { 1.0 / self.s } else { 0.0 };
        let t = quat::scale3(quat::mat3_vec(&rt, quat::sub3(w.p, self.p)), s);
        (t, quat::from_mat3(&quat::mat3_mul(&rt, &w.r)))
    }

    /// As a row-major 4x4 matrix.
    pub fn matrix(&self) -> [[f64; 4]; 4] {
        let mut m = [[0.0; 4]; 4];
        for (i, row) in m.iter_mut().enumerate().take(3) {
            for (j, x) in row.iter_mut().enumerate().take(3) {
                *x = self.r[i][j] * self.s;
            }
            row[3] = self.p[i];
        }
        m[3][3] = 1.0;
        m
    }
}

pub(crate) fn transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

impl Character {
    /// Checked: every parent exists and is not the node itself, there is no cycle, and the skin
    /// names existing nodes with one inverse bind matrix each.
    pub fn new(
        nodes: Vec<Node>,
        skin: Vec<usize>,
        bind_inverse: Vec<[[f64; 4]; 4]>,
        has_mesh: bool,
    ) -> Result<Character, String> {
        let n = nodes.len();
        if n == 0 || n > MAX_NODES {
            return Err(format!("a character needs 1 to {MAX_NODES} nodes, got {n}"));
        }
        for (i, node) in nodes.iter().enumerate() {
            if let Some(p) = node.parent
                && (p >= n || p == i)
            {
                return Err(format!(
                    "node {i} ({:?}) has parent {p}, which is not a node",
                    node.name
                ));
            }
            if node.translation.iter().any(|x| !x.is_finite())
                || node.rotation.iter().any(|x| !x.is_finite())
                || !node.scale.is_finite()
                || node.scale <= 0.0
            {
                return Err(format!(
                    "node {i} ({:?}) has a transform that is not a number",
                    node.name
                ));
            }
        }
        // parents first: a node is placed once its parent is; a cycle never places
        let mut order = Vec::with_capacity(n);
        let mut placed = vec![false; n];
        let mut progress = true;
        while order.len() < n && progress {
            progress = false;
            for i in 0..n {
                if placed[i] {
                    continue;
                }
                let ready = match nodes[i].parent {
                    None => true,
                    Some(p) => placed.get(p).copied().unwrap_or(false),
                };
                if ready {
                    placed[i] = true;
                    order.push(i);
                    progress = true;
                }
            }
        }
        if order.len() < n {
            let stuck = (0..n).find(|&i| !placed[i]).unwrap_or(0);
            return Err(format!(
                "the nodes loop: node {stuck} ({:?}) is its own ancestor",
                nodes.get(stuck).map_or("", |x| x.name.as_str())
            ));
        }
        if skin.len() != bind_inverse.len() {
            return Err(format!(
                "the skin has {} joints but {} inverse bind matrices",
                skin.len(),
                bind_inverse.len()
            ));
        }
        if skin.len() > MAX_NODES {
            return Err(format!("the skin has {} joints (most {MAX_NODES})", skin.len()));
        }
        if let Some(j) = skin.iter().find(|&&j| j >= n) {
            return Err(format!("the skin names joint {j}, which is not a node"));
        }
        if bind_inverse.iter().flatten().flatten().any(|x| !x.is_finite()) {
            return Err("an inverse bind matrix is not a number".into());
        }
        Ok(Character {
            nodes,
            skin,
            bind_inverse,
            has_mesh,
            lowest: None,
            order,
        })
    }

    /// With the mesh's lowest point at rest (rig z); a non-finite value is ignored.
    pub fn with_lowest(mut self, z: Option<f64>) -> Character {
        self.lowest = z.filter(|z| z.is_finite());
        self
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Nodes parents first.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    pub fn names(&self) -> Vec<String> {
        self.nodes.iter().map(|n| n.name.clone()).collect()
    }

    pub fn parents(&self) -> Vec<Option<usize>> {
        self.nodes.iter().map(|n| n.parent).collect()
    }

    /// The node with this name; an error when there is none or there are two.
    pub fn node(&self, name: &str) -> Result<usize, String> {
        let mut found = self.nodes.iter().enumerate().filter(|(_, n)| n.name == name);
        let first = found
            .next()
            .map(|(i, _)| i)
            .ok_or_else(|| format!("no node named {name:?} in the character"))?;
        if found.next().is_some() {
            return Err(format!(
                "two nodes are named {name:?} in the character; the mapping can't tell them apart"
            ));
        }
        Ok(first)
    }

    /// Whether `node` is `ancestor` or below it.
    pub fn under(&self, node: usize, ancestor: usize) -> bool {
        crate::jointmap::under(&self.parents(), node, ancestor)
    }

    /// The nodes from `node` up to (not including) `top`, nearest first; `None` when `node` is
    /// not under `top`.
    pub fn path_up(&self, node: usize, top: usize) -> Option<Vec<usize>> {
        let mut out = Vec::new();
        let mut cur = node;
        let mut steps = 0;
        while cur != top {
            out.push(cur);
            cur = self.nodes.get(cur)?.parent?;
            steps += 1;
            if steps > self.nodes.len() {
                return None;
            }
        }
        Some(out)
    }

    pub fn children(&self, node: usize) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|&c| self.nodes[c].parent == Some(node))
            .collect()
    }

    /// Every node's world transform at rest.
    pub fn rest_worlds(&self) -> Vec<World> {
        let mut out = vec![World::IDENTITY; self.nodes.len()];
        for &i in &self.order {
            let n = &self.nodes[i];
            let parent = n
                .parent
                .and_then(|p| out.get(p).copied())
                .unwrap_or(World::IDENTITY);
            out[i] = parent.child(n.translation, n.rotation, n.scale);
        }
        out
    }

    /// Skinning matrices (rig axes, row-major), one per skin joint, from world transforms.
    pub fn skin_matrices(&self, worlds: &[World]) -> Vec<[[f64; 4]; 4]> {
        self.skin
            .iter()
            .zip(&self.bind_inverse)
            .map(|(&j, ibm)| {
                let w = worlds.get(j).copied().unwrap_or(World::IDENTITY).matrix();
                mul4(&w, ibm)
            })
            .collect()
    }
}

pub(crate) fn mul4(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut m = [[0.0; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

/// The landmarks a mapping must name (the optional ones: [`OPTIONAL`]).
pub const REQUIRED: [&str; 14] = [
    "hips",
    "head",
    "l_shoulder",
    "l_elbow",
    "l_wrist",
    "l_hip",
    "l_knee",
    "l_ankle",
    "r_shoulder",
    "r_elbow",
    "r_wrist",
    "r_hip",
    "r_knee",
    "r_ankle",
];
/// Landmarks a character may lack: the toes (the foot is then the leaf), the neck (the head hangs
/// from the spine) and the spine (the chain between the hips and the neck is found anyway).
pub const OPTIONAL: [&str; 4] = ["spine", "neck", "l_toe", "r_toe"];

/// Which node of the character stands for each landmark, by node name, and the choices made.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mapping {
    pub joints: BTreeMap<String, String>,
    /// Solve the arms so the hands land where the rig's do (scaled), not only the legs.
    pub hands: bool,
}

impl Mapping {
    /// Whether `name` is a landmark the mapping takes.
    pub fn is_landmark(name: &str) -> bool {
        REQUIRED.contains(&name) || OPTIONAL.contains(&name)
    }

    /// The required landmarks it lacks.
    pub fn missing(&self) -> Vec<&'static str> {
        REQUIRED
            .iter()
            .copied()
            .filter(|l| !self.joints.contains_key(*l))
            .collect()
    }

    /// Node index of a landmark, if mapped and the name resolves.
    pub fn node(&self, c: &Character, landmark: &str) -> Result<Option<usize>, String> {
        match self.joints.get(landmark) {
            Some(name) => c.node(name).map(Some),
            None => Ok(None),
        }
    }
}

/// A character loaded into the document with its mapping: what `retarget.load` keeps and the
/// export and the viewport read. Not saved in a project file: a project is self-contained and
/// the character is a file of its own; the mapping file (`retarget.save`) names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The character file (.glb) as given (a mapping file's relative path stays relative).
    pub path: std::path::PathBuf,
    /// The character file, resolved: what a front end reads the mesh from.
    pub file: std::path::PathBuf,
    pub character: Character,
    pub mapping: Mapping,
    /// What the load guessed or left out (Rule 1).
    pub notes: Vec<String>,
}

/// Guess a mapping from the nodes' names and tree (`jointmap`), as the FBX import does; the
/// notes say what was doubtful. Node names that repeat in the file are not used (a note says so).
pub fn guess(c: &Character) -> (Mapping, Vec<String>) {
    let names = c.names();
    let g = crate::jointmap::guess(&names, &c.parents());
    let mut notes = g.notes.clone();
    let mut joints = BTreeMap::new();
    for (lm, &i) in &g.map {
        if !Mapping::is_landmark(lm) {
            continue;
        }
        let Some(name) = names.get(i) else { continue };
        if names.iter().filter(|n| *n == name).count() > 1 {
            notes.push(format!(
                "{lm}: the node {name:?} appears more than once in the file; not mapped, name another with joints"
            ));
            continue;
        }
        joints.insert((*lm).to_string(), name.clone());
    }
    (Mapping { joints, hands: false }, notes)
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

    fn node(name: &str, parent: Option<usize>, t: Vec3) -> Node {
        Node {
            name: name.into(),
            parent,
            translation: t,
            rotation: quat::IDENTITY,
            scale: 1.0,
        }
    }

    #[test]
    fn worlds_chain_translation_rotation_and_scale() {
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let q = [0.0, 0.0, h, h];
        let nodes = vec![
            Node {
                name: "a".into(),
                parent: None,
                translation: [1.0, 0.0, 0.0],
                rotation: q,
                scale: 2.0,
            },
            node("b", Some(0), [1.0, 0.0, 0.0]),
        ];
        let c = Character::new(
            nodes,
            vec![1],
            vec![[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]],
            false,
        )
        .unwrap();
        let w = c.rest_worlds();
        // b: a's position + a's rotation (90° about Z) of 2 * (1, 0, 0) = (1, 2, 0)
        assert!((w[1].p[0] - 1.0).abs() < 1e-12 && (w[1].p[1] - 2.0).abs() < 1e-12);
        assert!((w[1].s - 2.0).abs() < 1e-12);
        let (t, r) = w[0].local_of(&w[1]);
        assert!((t[0] - 1.0).abs() < 1e-12 && t[1].abs() < 1e-12, "{t:?}");
        assert!((r[3].abs() - 1.0).abs() < 1e-12, "{r:?}");
        let m = c.skin_matrices(&w);
        assert_eq!(m.len(), 1);
        assert!((m[0][0][3] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_loop_and_a_bad_parent_are_refused() {
        let nodes = vec![node("a", Some(1), [0.0; 3]), node("b", Some(0), [0.0; 3])];
        assert!(
            Character::new(nodes, vec![], vec![], false)
                .unwrap_err()
                .contains("loop")
        );
        let nodes = vec![node("a", Some(7), [0.0; 3])];
        assert!(Character::new(nodes, vec![], vec![], false).is_err());
        let nodes = vec![node("a", None, [0.0; 3])];
        assert!(Character::new(nodes, vec![3], vec![[[0.0; 4]; 4]], false).is_err());
    }

    #[test]
    fn a_name_that_repeats_is_told_apart_from_a_missing_one() {
        let nodes = vec![
            node("a", None, [0.0; 3]),
            node("x", Some(0), [0.0; 3]),
            node("x", Some(0), [0.0; 3]),
        ];
        let c = Character::new(nodes, vec![], vec![], false).unwrap();
        assert!(c.node("x").unwrap_err().contains("two nodes"));
        assert!(c.node("y").unwrap_err().contains("no node"));
        assert_eq!(c.node("a").unwrap(), 0);
        assert_eq!(c.path_up(2, 0).unwrap(), vec![2]);
        assert!(c.path_up(0, 2).is_none());
    }
}
