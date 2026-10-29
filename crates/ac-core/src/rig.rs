//! The skeleton and forward kinematics.
//!
//! Rotations are per bone, relative to the parent, with every bone at rest along the world axes at
//! its head. For rest heads `h`:
//!
//! ```text
//! R_i = R_parent(i) q_i
//! P_i = P_parent(i) + R_parent(i) (h_i - h_parent(i))
//! P_root = h_root + root_offset * hip_height
//! ```
//!
//! Metres, Z up, the rig faces -Y.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::CoreError;
use crate::quat::{self, Mat3, Quat, Vec3};

pub const RIG_FORMAT: &str = concat!(crate::product_id!(), ".rig");
/// More bones than any rig we load; a file asking for more is refused, not allocated.
pub const MAX_BONES: usize = 4096;

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub names: Vec<String>,
    /// `None` for the root; a parent always comes before its children.
    pub parents: Vec<Option<usize>>,
    pub heads: Vec<Vec3>,
    /// Root height above the floor at rest; root offsets are in hip heights.
    pub hip_height: f64,
    /// Bone profiles for export: profile name -> the bones it keeps.
    pub profiles: BTreeMap<String, Vec<String>>,
    index: BTreeMap<String, usize>,
}

#[derive(Deserialize)]
struct RigFile {
    format: String,
    version: u32,
    names: Vec<String>,
    parents: Vec<i64>,
    heads: Vec<Vec3>,
    hip_height: f64,
    #[serde(default)]
    profiles: BTreeMap<String, Vec<String>>,
}

impl Skeleton {
    pub fn new(
        names: Vec<String>,
        parents: Vec<Option<usize>>,
        heads: Vec<Vec3>,
        hip_height: f64,
    ) -> Result<Skeleton, CoreError> {
        let n = names.len();
        if n == 0 || n > MAX_BONES {
            return Err(CoreError::Rig(format!(
                "a rig needs 1 to {MAX_BONES} bones, got {n}"
            )));
        }
        if parents.len() != n || heads.len() != n {
            return Err(CoreError::Rig(format!(
                "{n} names but {} parents and {} heads",
                parents.len(),
                heads.len()
            )));
        }
        if !(hip_height.is_finite() && hip_height > 0.0) {
            return Err(CoreError::Rig(format!(
                "hip height must be positive, got {hip_height}"
            )));
        }
        let mut index = BTreeMap::new();
        for (i, name) in names.iter().enumerate() {
            if index.insert(name.clone(), i).is_some() {
                return Err(CoreError::Rig(format!("bone {name:?} appears twice")));
            }
            match parents.get(i).copied().flatten() {
                Some(p) if p >= i => {
                    return Err(CoreError::Rig(format!("bone {name:?} comes before its parent")));
                }
                None if i != 0 => return Err(CoreError::Rig(format!("bone {name:?} has no parent"))),
                _ => {}
            }
            if heads.get(i).is_some_and(|h| h.iter().any(|x| !x.is_finite())) {
                return Err(CoreError::Rig(format!("bone {name:?} has a non-finite head")));
            }
        }
        Ok(Skeleton {
            names,
            parents,
            heads,
            hip_height,
            profiles: BTreeMap::new(),
            index,
        })
    }

    /// A rig from `default_rig.json` (`RIG_FORMAT`, version 1).
    pub fn from_json(text: &str) -> Result<Skeleton, CoreError> {
        let file: RigFile =
            serde_json::from_str(text).map_err(|e| CoreError::Rig(format!("rig file: {e}")))?;
        if !crate::product::is_format(&file.format, "rig") || file.version != 1 {
            return Err(CoreError::Rig(format!(
                "not a rig file: format {:?} version {} (expected {RIG_FORMAT:?} 1)",
                file.format, file.version
            )));
        }
        let parents = file
            .parents
            .iter()
            .map(|&p| {
                if p < 0 {
                    Ok(None)
                } else {
                    usize::try_from(p).map(Some)
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CoreError::Rig(format!("bad parent index: {e}")))?;
        let mut sk = Skeleton::new(file.names, parents, file.heads, file.hip_height)?;
        for (profile, bones) in &file.profiles {
            if let Some(b) = bones.iter().find(|b| !sk.index.contains_key(*b)) {
                return Err(CoreError::Rig(format!(
                    "profile {profile:?} names unknown bone {b:?}"
                )));
            }
        }
        sk.profiles = file.profiles;
        Ok(sk)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// World rotations and joint positions of one pose. `rotations` must have one entry per bone.
    pub fn fk(&self, rotations: &[Quat], root: Vec3) -> Result<(Vec<Mat3>, Vec<Vec3>), CoreError> {
        if rotations.len() != self.len() {
            return Err(CoreError::Pose(format!(
                "{} rotations for a {}-bone rig",
                rotations.len(),
                self.len()
            )));
        }
        let mut r: Vec<Mat3> = Vec::with_capacity(self.len());
        let mut p: Vec<Vec3> = Vec::with_capacity(self.len());
        for (i, q) in rotations.iter().enumerate() {
            let local = quat::to_mat3(*q);
            let head = self.heads.get(i).copied().unwrap_or_default();
            match self.parents.get(i).copied().flatten() {
                None => {
                    r.push(local);
                    p.push(quat::add3(head, quat::scale3(root, self.hip_height)));
                }
                Some(parent) => {
                    let (Some(rp), Some(pp), Some(hp)) = (
                        r.get(parent).copied(),
                        p.get(parent).copied(),
                        self.heads.get(parent).copied(),
                    ) else {
                        return Err(CoreError::Rig(format!("bone {i} has a missing parent {parent}")));
                    };
                    r.push(quat::mat3_mul(&rp, &local));
                    p.push(quat::add3(pp, quat::mat3_vec(&rp, quat::sub3(head, hp))));
                }
            }
        }
        Ok((r, p))
    }
}
