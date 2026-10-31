//! From a mapping to a plan: which rig bone drives each node of the character, the world
//! alignment between the two rest poses, the chains shared out (the rig's five spine bones and
//! three neck bones over however many the character has), the clavicles guessed from the tree,
//! the legs and arms for IK, the scale and the floor.
//!
//! Alignment: a driven node turns in world space by its rig bone's world rotation `R_b(f)` (the
//! rig's rest rotations are the identity, so that is the bone's whole turn from rest), applied to
//! the node's rest world rotation after `A`, the smallest rotation that takes the node's rest
//! segment (towards its mapped child) onto the rig bone's rest segment:
//! `W_n(f) = R_b(f) · A · W_n(rest)`. Whatever the character's rest pose (A-pose, T-pose) and
//! bone roll, the node's segment then points where the rig's does, and the roll stays the
//! character's own. A node with no segment to aim (a hand without fingers, the head) takes its
//! nearest driven ancestor's alignment, so it keeps its rest relation to it.

use std::collections::BTreeMap;

use crate::quat::{self, Mat3, Vec3};
use crate::rig::Skeleton;

use super::{Character, Mapping, World};

/// One node and the rig bone that drives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Drive {
    pub node: usize,
    pub bone: usize,
    /// World rotation applied between the bone's turn and the node's rest rotation.
    pub align: Mat3,
    /// What the node stands for (`hips`, `spine`, `l_shoulder`, `l_clavicle`, ...).
    pub role: String,
}

/// A leg (or an arm) solved by two-bone IK: the character's three nodes and the rig's bones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limb {
    pub upper: usize,
    pub lower: usize,
    pub end: usize,
    pub upper_bone: usize,
    pub lower_bone: usize,
    pub end_bone: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub drives: Vec<Drive>,
    /// Per node: its drive, if driven.
    pub driven: Vec<Option<usize>>,
    pub hips: usize,
    pub root_bone: usize,
    /// Left, then right.
    pub legs: [Limb; 2],
    pub arms: [Limb; 2],
    /// The character's legs over the rig's (thigh + shin, at rest).
    pub scale: f64,
    /// The character's floor (rig z): the mesh's lowest point, else its lowest joint.
    pub floor: f64,
    /// The rig's ankle height above its lowest joint; the character's above its floor.
    pub ankle_height: [f64; 2],
    /// The hips node's rest position less the midpoint of the hip joints.
    pub hips_offset: Vec3,
    /// The rig's hip joints (left, right), whose midpoint places the hips.
    pub hip_bones: [usize; 2],
    pub rest: Vec<World>,
    /// Nodes under the hips no bone drives (fingers, twist bones, props): they keep their rest.
    pub undriven: Vec<usize>,
    /// Nodes not under the hips (the armature's root, a mesh node): left as they are.
    pub outside: Vec<usize>,
    pub notes: Vec<String>,
    pub hands: bool,
}

/// Our rig's bones the plan drives from, by side.
fn sided(base: &str, side: &str) -> String {
    format!("{base}.{side}")
}

const SPINE: [&str; 5] = ["spine05", "spine04", "spine03", "spine02", "spine01"];
const NECK: [&str; 3] = ["neck01", "neck02", "neck03"];

/// The smallest rotation taking `from` onto `to` (both made unit); `None` when either is too
/// short to have a direction.
pub fn align(from: Vec3, to: Vec3) -> Option<Mat3> {
    let (lf, lt) = (quat::len3(from), quat::len3(to));
    if lf < 1e-9 || lt < 1e-9 {
        return None;
    }
    let v = quat::scale3(from, 1.0 / lf);
    let u = quat::scale3(to, 1.0 / lt);
    let axis = quat::cross(v, u);
    let s = quat::len3(axis);
    let c = v[0] * u[0] + v[1] * u[1] + v[2] * u[2];
    if s < 1e-12 {
        if c > 0.0 {
            return Some(quat::MAT3_IDENTITY);
        }
        // opposite: a half turn about any axis across v
        let pick = if v[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let axis = quat::cross(v, pick);
        return Some(crate::ik::axis_angle(
            quat::scale3(axis, 1.0 / quat::len3(axis).max(1e-12)),
            std::f64::consts::PI,
        ));
    }
    Some(crate::ik::axis_angle(quat::scale3(axis, 1.0 / s), s.atan2(c)))
}

/// Of `children`, the one whose rest direction from `node` continues `along` best (the middle
/// finger of a hand, the toe's end): `None` when there is none.
fn continuation(rest: &[World], node: usize, children: &[usize], along: Vec3) -> Option<usize> {
    let la = quat::len3(along);
    if la < 1e-9 {
        return children.first().copied();
    }
    let a = quat::scale3(along, 1.0 / la);
    children
        .iter()
        .copied()
        .filter_map(|c| {
            let d = quat::sub3(rest.get(c)?.p, rest.get(node)?.p);
            let l = quat::len3(d);
            (l > 1e-9).then(|| (c, (d[0] * a[0] + d[1] * a[1] + d[2] * a[2]) / l))
        })
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(c, _)| c)
}

/// Our bone at the proportional place along a chain of `ours` for item `k` (0-based) of `n`.
fn share<'a>(ours: &[&'a str], k: usize, n: usize) -> &'a str {
    // item k of n takes ceil((k + 1) * len / n): one item takes the chain's top
    let i = ((k + 1) * ours.len()).div_ceil(n.max(1)).clamp(1, ours.len()) - 1;
    ours.get(i).copied().unwrap_or("")
}

struct Builder<'a> {
    sk: &'a Skeleton,
    c: &'a Character,
    rest: Vec<World>,
    drives: Vec<Drive>,
    driven: Vec<Option<usize>>,
    notes: Vec<String>,
}

impl Builder<'_> {
    fn bone(&self, name: &str) -> Result<usize, String> {
        self.sk
            .bone(name)
            .ok_or_else(|| format!("the rig has no bone {name:?}, which the character export drives from"))
    }

    fn head(&self, bone: usize) -> Vec3 {
        self.sk.heads.get(bone).copied().unwrap_or_default()
    }

    fn pos(&self, node: usize) -> Vec3 {
        self.rest.get(node).map_or([0.0; 3], |w| w.p)
    }

    /// The alignment of the nearest driven ancestor (the identity above the hips).
    fn inherited(&self, node: usize) -> Mat3 {
        let mut cur = self.c.nodes.get(node).and_then(|n| n.parent);
        let mut steps = 0;
        while let Some(p) = cur {
            if let Some(Some(d)) = self.driven.get(p)
                && let Some(drive) = self.drives.get(*d)
            {
                return drive.align;
            }
            cur = self.c.nodes.get(p).and_then(|n| n.parent);
            steps += 1;
            if steps > self.c.len() {
                break;
            }
        }
        quat::MAT3_IDENTITY
    }

    /// Drive `node` from `bone`; the segments (our end bone, the node's end) give the alignment.
    fn add(&mut self, role: &str, node: usize, bone: usize, our_end: Option<usize>, node_end: Option<usize>) {
        if self.driven.get(node).is_some_and(Option::is_some) {
            return;
        }
        let align = match (our_end, node_end) {
            (Some(e), Some(n)) => {
                let u = quat::sub3(self.head(e), self.head(bone));
                let v = quat::sub3(self.pos(n), self.pos(node));
                match align(v, u) {
                    Some(a) => a,
                    None => {
                        self.notes.push(format!(
                            "{role}: {:?} has no length to aim, it keeps its rest relation to its parent",
                            self.c.nodes[node].name
                        ));
                        self.inherited(node)
                    }
                }
            }
            _ => self.inherited(node),
        };
        self.drives.push(Drive {
            node,
            bone,
            align,
            role: role.to_string(),
        });
        if let Some(slot) = self.driven.get_mut(node) {
            *slot = Some(self.drives.len() - 1);
        }
    }
}

impl Plan {
    /// Work the plan out; refused when the mapping lacks a required landmark, names a node the
    /// character lacks, or maps a body part that is not under the hips.
    pub fn new(sk: &Skeleton, c: &Character, m: &Mapping) -> Result<Plan, String> {
        let missing = m.missing();
        if !missing.is_empty() {
            let hint: Vec<String> = missing.iter().map(|l| format!("\"{l}\": \"<node>\"")).collect();
            return Err(format!(
                "the mapping has no node for {}; name them with joints: {{{}}}",
                missing.join(", "),
                hint.join(", ")
            ));
        }
        for lm in m.joints.keys() {
            if !Mapping::is_landmark(lm) {
                return Err(format!(
                    "joints: {lm:?} is not a landmark; the landmarks are {}, and optionally {}",
                    super::REQUIRED.join(", "),
                    super::OPTIONAL.join(", ")
                ));
            }
        }
        // one node stands for one landmark: two landmarks on a node would leave the second
        // undriven without a word, and a limb with two of its joints on one node can't be solved
        let mut by_node: BTreeMap<&str, &str> = BTreeMap::new();
        for (lm, name) in &m.joints {
            if let Some(other) = by_node.insert(name.as_str(), lm.as_str()) {
                return Err(format!(
                    "joints: {other} and {lm} both name the node {name:?}; a node stands for one landmark (unmap one with \"none\")"
                ));
            }
        }
        let at = |lm: &str| -> Result<usize, String> {
            m.node(c, lm)?
                .ok_or_else(|| format!("the mapping has no node for {lm}"))
        };
        let opt = |lm: &str| -> Result<Option<usize>, String> { m.node(c, lm) };
        let hips = at("hips")?;
        for lm in super::REQUIRED.iter().chain(super::OPTIONAL.iter()) {
            if let Some(n) = opt(lm)?
                && n != hips
                && !c.under(n, hips)
            {
                return Err(format!(
                    "{lm} ({:?}) is not under the hips ({:?}) in the character's tree",
                    c.nodes[n].name, c.nodes[hips].name
                ));
            }
        }
        let mut b = Builder {
            sk,
            c,
            rest: c.rest_worlds(),
            drives: Vec::new(),
            driven: vec![None; c.len()],
            notes: Vec::new(),
        };
        let root_bone = b.bone("root")?;
        let head_bone = b.bone("head")?;
        let head = at("head")?;
        let neck = opt("neck")?;
        let spine = opt("spine")?;

        // the hips: the whole body's turn
        b.add("hips", hips, root_bone, None, None);

        // the spine chain: every node between the hips and the neck (or the head), shared out
        let top = neck.unwrap_or(head);
        let path = c
            .path_up(top, hips)
            .ok_or_else(|| format!("{:?} is not under the hips", c.nodes[top].name))?;
        // path: top, its parent, ..., the hips' child; the chain is the parents, bottom-up
        let mut spine_chain: Vec<usize> = path.iter().skip(1).copied().collect();
        spine_chain.reverse();
        if let Some(s) = spine
            && !spine_chain.contains(&s)
        {
            b.notes.push(format!(
                "spine ({:?}) is not between the hips and the neck; the spine chain is taken from the tree instead",
                c.nodes[s].name
            ));
        }
        if spine_chain.is_empty() {
            b.notes.push("the character has no spine node between the hips and the neck: the rig's spine bend goes to the neck".into());
        }
        let n = spine_chain.len();
        let neck01 = b.bone("neck01")?;
        for k in 0..n {
            let bone = b.bone(share(&SPINE, k, n))?;
            let next_bone = if k + 1 < n {
                b.bone(share(&SPINE, k + 1, n))?
            } else {
                neck01
            };
            let next_node = spine_chain.get(k + 1).copied().unwrap_or(top);
            b.add("spine", spine_chain[k], bone, Some(next_bone), Some(next_node));
        }
        // the neck chain: the nodes from the neck up to below the head
        let neck_chain: Vec<usize> = match neck {
            Some(nk) => {
                let mut p = c.path_up(head, nk).ok_or_else(|| {
                    format!(
                        "the head ({:?}) is not under the neck ({:?})",
                        c.nodes[head].name, c.nodes[nk].name
                    )
                })?;
                p.retain(|&x| x != head);
                p.push(nk);
                p.reverse();
                p
            }
            None => Vec::new(),
        };
        let nn = neck_chain.len();
        for k in 0..nn {
            let bone = b.bone(share(&NECK, k, nn))?;
            let next_bone = if k + 1 < nn {
                b.bone(share(&NECK, k + 1, nn))?
            } else {
                head_bone
            };
            let next_node = neck_chain.get(k + 1).copied().unwrap_or(head);
            b.add("neck", neck_chain[k], bone, Some(next_bone), Some(next_node));
        }
        b.add("head", head, head_bone, None, None);

        // arms and legs
        let mut arms = Vec::new();
        let mut legs = Vec::new();
        for side in ["L", "R"] {
            let lm = |k: &str| format!("{}_{k}", side.to_lowercase());
            let (shoulder, elbow, wrist) = (at(&lm("shoulder"))?, at(&lm("elbow"))?, at(&lm("wrist"))?);
            let (hip, knee, ankle) = (at(&lm("hip"))?, at(&lm("knee"))?, at(&lm("ankle"))?);
            let toe = opt(&lm("toe"))?;
            let upperarm = b.bone(&sided("upperarm01", side))?;
            let lowerarm = b.bone(&sided("lowerarm01", side))?;
            let wrist_bone = b.bone(&sided("wrist", side))?;
            let clavicle = b.bone(&sided("clavicle", side))?;
            let upperleg = b.bone(&sided("upperleg01", side))?;
            let lowerleg = b.bone(&sided("lowerleg01", side))?;
            let foot = b.bone(&sided("foot", side))?;
            let toe_bone = b.bone(&sided("toe3-1", side))?;
            let toe_end = sk.bone(&sided("toe3-3", side));
            let finger = sk.bone(&sided("finger3-1", side));
            // a clavicle: the upper arm's parent when it is a node of its own under the hips
            if let Some(p) = c.nodes[shoulder].parent
                && p != hips
                && c.under(p, hips)
                && !spine_chain.contains(&p)
                && !neck_chain.contains(&p)
                && b.driven.get(p).is_some_and(Option::is_none)
            {
                b.add(&lm("clavicle"), p, clavicle, Some(upperarm), Some(shoulder));
                b.notes.push(format!(
                    "{:?} is driven as the {} clavicle (the upper arm hangs from it)",
                    c.nodes[p].name,
                    if side == "L" { "left" } else { "right" }
                ));
            }
            b.add(&lm("shoulder"), shoulder, upperarm, Some(lowerarm), Some(elbow));
            b.add(&lm("elbow"), elbow, lowerarm, Some(wrist_bone), Some(wrist));
            let along = quat::sub3(b.pos(wrist), b.pos(elbow));
            let hand_end = continuation(&b.rest, wrist, &c.children(wrist), along);
            b.add(&lm("wrist"), wrist, wrist_bone, finger, hand_end);
            b.add(&lm("hip"), hip, upperleg, Some(lowerleg), Some(knee));
            b.add(&lm("knee"), knee, lowerleg, Some(foot), Some(ankle));
            let along = quat::sub3(b.pos(ankle), b.pos(knee));
            let foot_end = toe.or_else(|| continuation(&b.rest, ankle, &c.children(ankle), along));
            b.add(&lm("ankle"), ankle, foot, Some(toe_bone), foot_end);
            if let Some(t) = toe {
                let along = quat::sub3(b.pos(t), b.pos(ankle));
                let end = continuation(&b.rest, t, &c.children(t), along);
                b.add(&lm("toe"), t, toe_bone, toe_end, end);
            } else {
                b.notes.push(format!(
                    "no {} toe is mapped: the foot is the leaf",
                    if side == "L" { "left" } else { "right" }
                ));
            }
            arms.push(Limb {
                upper: shoulder,
                lower: elbow,
                end: wrist,
                upper_bone: upperarm,
                lower_bone: lowerarm,
                end_bone: wrist_bone,
            });
            legs.push(Limb {
                upper: hip,
                lower: knee,
                end: ankle,
                upper_bone: upperleg,
                lower_bone: lowerleg,
                end_bone: foot,
            });
        }
        let (Some(&la), Some(&ra), Some(&ll), Some(&rl)) =
            (arms.first(), arms.get(1), legs.first(), legs.get(1))
        else {
            return Err("both sides are needed".into());
        };

        // the scale: the character's legs over the rig's
        let leg_len = |a: Vec3, k: Vec3, e: Vec3| quat::len3(quat::sub3(k, a)) + quat::len3(quat::sub3(e, k));
        let ours = (leg_len(b.head(ll.upper_bone), b.head(ll.lower_bone), b.head(ll.end_bone))
            + leg_len(b.head(rl.upper_bone), b.head(rl.lower_bone), b.head(rl.end_bone)))
            / 2.0;
        let theirs = (leg_len(b.pos(ll.upper), b.pos(ll.lower), b.pos(ll.end))
            + leg_len(b.pos(rl.upper), b.pos(rl.lower), b.pos(rl.end)))
            / 2.0;
        if !ours.is_finite() || ours <= 1e-6 {
            return Err("the rig's legs have no length".into());
        }
        if !theirs.is_finite() || theirs <= 1e-6 {
            return Err(
                "the character's legs have no length: its hip, knee and ankle nodes sit on one point".into(),
            );
        }
        let scale = theirs / ours;

        // the floors and ankle heights
        let lowest_head = sk.heads.iter().map(|h| h[2]).fold(f64::INFINITY, f64::min);
        let ankle_ours = (b.head(ll.end_bone)[2] + b.head(rl.end_bone)[2]) / 2.0 - lowest_head;
        let under: Vec<usize> = (0..c.len()).filter(|&i| c.under(i, hips)).collect();
        let lowest_joint = under.iter().map(|&i| b.pos(i)[2]).fold(f64::INFINITY, f64::min);
        let floor = match c.lowest {
            Some(z) if z.is_finite() => z,
            _ => {
                if c.has_mesh {
                    b.notes.push(
                        "the mesh gives no lowest point: the floor is taken at the lowest joint".into(),
                    );
                } else {
                    b.notes
                        .push("no mesh: the character's floor is taken at its lowest joint".into());
                }
                lowest_joint
            }
        };
        let ankle_theirs = (b.pos(ll.end)[2] + b.pos(rl.end)[2]) / 2.0 - floor;
        if ankle_theirs < 1e-6 {
            b.notes.push(
                "the character's ankles are at its floor: map its toes (or a mesh) so the feet stand on it"
                    .into(),
            );
        }
        let mid = quat::scale3(quat::add3(b.pos(ll.upper), b.pos(rl.upper)), 0.5);
        let hips_offset = quat::sub3(b.pos(hips), mid);

        let undriven: Vec<usize> = under
            .iter()
            .copied()
            .filter(|&i| b.driven.get(i).is_some_and(Option::is_none))
            .collect();
        let outside: Vec<usize> = (0..c.len()).filter(|&i| !c.under(i, hips)).collect();
        if !undriven.is_empty() {
            b.notes.push(format!(
                "{} nodes under the hips are not driven and keep their rest pose",
                undriven.len()
            ));
        }
        Ok(Plan {
            drives: b.drives,
            driven: b.driven,
            hips,
            root_bone,
            legs: [ll, rl],
            arms: [la, ra],
            scale,
            floor,
            ankle_height: [ankle_ours, ankle_theirs],
            hips_offset,
            hip_bones: [ll.upper_bone, rl.upper_bone],
            rest: b.rest,
            undriven,
            outside,
            notes: b.notes,
            hands: m.hands,
        })
    }

    /// A rig-space point as the character's: horizontally by the scale, vertically by the scale
    /// above the ankle heights, so a straight leg stands on the character's floor.
    pub fn map_point(&self, p: Vec3) -> Vec3 {
        [
            p[0] * self.scale,
            p[1] * self.scale,
            self.floor + self.ankle_height[1] + self.scale * (p[2] - self.ankle_height[0]),
        ]
    }

    pub fn drive_of(&self, node: usize) -> Option<&Drive> {
        self.driven
            .get(node)
            .copied()
            .flatten()
            .and_then(|d| self.drives.get(d))
    }
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

    #[test]
    fn align_turns_one_direction_onto_another() {
        let a = align([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]).unwrap();
        let v = quat::mat3_vec(&a, [1.0, 0.0, 0.0]);
        assert!((v[1] - 1.0).abs() < 1e-12, "{v:?}");
        let flip = align([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]).unwrap();
        let v = quat::mat3_vec(&flip, [0.0, 0.0, 1.0]);
        assert!((v[2] + 1.0).abs() < 1e-12, "{v:?}");
        assert_eq!(
            align([0.0, 0.0, 1.0], [0.0, 0.0, 2.0]).unwrap(),
            quat::MAT3_IDENTITY
        );
        assert!(align([0.0; 3], [1.0, 0.0, 0.0]).is_none());
    }

    #[test]
    fn chains_are_shared_out_from_the_top() {
        assert_eq!(share(&SPINE, 0, 1), "spine01");
        assert_eq!([share(&SPINE, 0, 2), share(&SPINE, 1, 2)], ["spine03", "spine01"]);
        assert_eq!(
            [share(&SPINE, 0, 3), share(&SPINE, 1, 3), share(&SPINE, 2, 3)],
            ["spine04", "spine02", "spine01"]
        );
        assert_eq!(share(&SPINE, 4, 5), "spine01");
        assert_eq!(share(&SPINE, 0, 5), "spine05");
        assert_eq!([share(&NECK, 0, 2), share(&NECK, 1, 2)], ["neck02", "neck03"]);
    }
}
