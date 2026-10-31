//! What an FBX scene says about its skeleton and its animation: the models and their hierarchy,
//! each model's transform properties, the animation curves connected to them, the axis system,
//! the unit and the frame rate. [`motion`] samples every frame and gives world joint positions in
//! the rig's axes (Z up, facing -Y) and metres.
//!
//! A model's local transform is, as the format defines it,
//! `T · Roff · Rp · Rpre · R · Rpost⁻¹ · Rp⁻¹ · Soff · Sp · S · Sp⁻¹` (Euler angles in degrees,
//! the rotation order naming which axis turns first), and its world transform is its parent's
//! times that, as `InheritType` says ([`World::child`]). `UpAxis`, `FrontAxis`, `CoordAxis` (with their signs) and `UnitScaleFactor` are
//! read from `GlobalSettings`, never assumed; a file without them is refused.

use std::collections::BTreeMap;

use ac_core::quat::{self, MAT3_IDENTITY, Mat3, Vec3};

use super::binary::{Document, Node, Prop};

/// FBX time: ticks per second.
pub const TICKS_PER_SECOND: f64 = 46_186_158_000.0;
/// Most frames one import samples (the retarget keeps every frame of every bone in memory).
pub const MAX_FRAMES: usize = ac_core::retarget::MAX_FRAMES;
/// Most models (joints and other nodes) one file may hold.
pub const MAX_MODELS: usize = 100_000;
/// Most keys one curve may hold.
pub const MAX_KEYS: usize = 10_000_000;
/// Most joint positions one import samples (frames times models): the sampled scene is kept in
/// memory, so a long take of a scene with many nodes is refused rather than swapped to death.
pub const MAX_SAMPLES: usize = 20_000_000;
/// Frame rates by the `TimeMode` enum; a rate of 0 means the mode does not fix one.
/// `KeyAttrFlags`: the bit saying a key's interpolation is cubic.
const CUBIC_FLAG: i64 = 0x8;
const TIME_MODES: [f64; 19] = [
    0.0, 120.0, 100.0, 60.0, 50.0, 48.0, 30.0, 30.0, 29.97, 29.97, 25.0, 24.0, 1000.0, 23.976, 0.0, 96.0,
    72.0, 59.94, 119.88,
];

type R<T> = Result<T, String>;

/// A translation and a 3x3 (rotation and scale) part.
#[derive(Debug, Clone, Copy)]
struct Affine {
    m: Mat3,
    t: Vec3,
}

impl Affine {
    const IDENTITY: Affine = Affine {
        m: MAT3_IDENTITY,
        t: [0.0; 3],
    };

    fn translation(t: Vec3) -> Affine {
        Affine { m: MAT3_IDENTITY, t }
    }

    fn linear(m: Mat3) -> Affine {
        Affine { m, t: [0.0; 3] }
    }

    fn then(&self, other: &Affine) -> Affine {
        // self · other (other applied first)
        Affine {
            m: quat::mat3_mul(&self.m, &other.m),
            t: quat::add3(self.t, quat::mat3_vec(&self.m, other.t)),
        }
    }
}

fn axis_rotation(axis: usize, degrees: f64) -> Mat3 {
    let (s, c) = degrees.to_radians().sin_cos();
    match axis {
        0 => [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
        1 => [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
        _ => [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
    }
}

/// The rotation of Euler angles `(x, y, z)` in degrees; `order` (the `RotationOrder` enum) names
/// which axis turns first.
pub fn euler(deg: Vec3, order: i64) -> Mat3 {
    let seq: [usize; 3] = match order {
        0 => [0, 1, 2],
        1 => [0, 2, 1],
        2 => [1, 2, 0],
        3 => [1, 0, 2],
        4 => [2, 0, 1],
        _ => [2, 1, 0],
    };
    let first = axis_rotation(seq[0], deg[seq[0]]);
    let second = axis_rotation(seq[1], deg[seq[1]]);
    let third = axis_rotation(seq[2], deg[seq[2]]);
    quat::mat3_mul(&third, &quat::mat3_mul(&second, &first))
}

fn transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

fn diag(v: Vec3) -> Mat3 {
    [[v[0], 0.0, 0.0], [0.0, v[1], 0.0], [0.0, 0.0, v[2]]]
}

/// The transform properties of a model (`Properties70`), with the format's defaults.
#[derive(Debug, Clone)]
struct Transform {
    translation: Vec3,
    rotation: Vec3,
    scaling: Vec3,
    pre_rotation: Vec3,
    post_rotation: Vec3,
    rotation_offset: Vec3,
    rotation_pivot: Vec3,
    scaling_offset: Vec3,
    scaling_pivot: Vec3,
    order: i64,
    rotation_active: bool,
    inherit: i64,
}

impl Default for Transform {
    fn default() -> Transform {
        Transform {
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scaling: [1.0; 3],
            pre_rotation: [0.0; 3],
            post_rotation: [0.0; 3],
            rotation_offset: [0.0; 3],
            rotation_pivot: [0.0; 3],
            scaling_offset: [0.0; 3],
            scaling_pivot: [0.0; 3],
            order: 0,
            rotation_active: false,
            inherit: 0,
        }
    }
}

impl Transform {
    /// The local matrix with `t`, `r`, `s` as sampled this frame, and its rotation part alone.
    fn local(&self, t: Vec3, r: Vec3, s: Vec3) -> (Affine, Mat3) {
        let (pre, post) = if self.rotation_active {
            (euler(self.pre_rotation, 0), euler(self.post_rotation, 0))
        } else {
            (MAT3_IDENTITY, MAT3_IDENTITY)
        };
        let rot = quat::mat3_mul(&pre, &quat::mat3_mul(&euler(r, self.order), &transpose(&post)));
        let neg = |v: Vec3| quat::scale3(v, -1.0);
        let local = Affine::translation(t)
            .then(&Affine::translation(self.rotation_offset))
            .then(&Affine::translation(self.rotation_pivot))
            .then(&Affine::linear(rot))
            .then(&Affine::translation(neg(self.rotation_pivot)))
            .then(&Affine::translation(self.scaling_offset))
            .then(&Affine::translation(self.scaling_pivot))
            .then(&Affine::linear(diag(s)))
            .then(&Affine::translation(neg(self.scaling_pivot)));
        (local, rot)
    }
}

/// `InheritType`: how a parent's scale reaches its child. The child's position is the parent's
/// whole matrix times the local position in all three; the matrix handed on differs:
/// - 0 (the format's default): `Rp · r · Sp · s`, the parent's scale after the child's rotation.
/// - 1: `Rp · Sp · r · s`, a plain matrix product.
/// - 2: `Rp · r · (Sp / sp) · s`, leaving out the parent's own local scale `sp`.
///
/// They agree when no scale is present, and 0 and 1 agree when the parent's scale is uniform. So
/// a world transform keeps its rotation and scale apart. A type 1 child of a non-uniform scale
/// shears, and a shear has no one split: type 1 children below it are still exact, and a type 0
/// or 2 child is refused.
#[derive(Debug, Clone, Copy)]
struct World {
    a: Affine,
    rot: Mat3,
    /// The world scale; `None` once sheared.
    scale: Option<Vec3>,
    local_scale: Vec3,
}

impl World {
    fn root(local: Affine, rot: Mat3, s: Vec3) -> World {
        World {
            a: local,
            rot,
            scale: Some(s),
            local_scale: s,
        }
    }

    fn child(&self, local: Affine, rot: Mat3, s: Vec3, inherit: i64, name: &str) -> R<World> {
        let mul = |a: Vec3, b: Vec3| [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
        let new_rot = quat::mat3_mul(&self.rot, &rot);
        let t = quat::add3(self.a.t, quat::mat3_vec(&self.a.m, local.t));
        // no scale above: all three types are this product, kept free of rounding from the split
        if self.scale == Some([1.0; 3]) && (inherit != 2 || self.local_scale == [1.0; 3]) {
            return Ok(World {
                a: self.a.then(&local),
                rot: new_rot,
                scale: Some(s),
                local_scale: s,
            });
        }
        let (m, scale) = if inherit == 1 {
            let scale = self.scale.filter(|p| is_uniform(*p)).map(|p| mul(p, s));
            (quat::mat3_mul(&self.a.m, &local.m), scale)
        } else {
            let Some(above) = self.scale else {
                return Err(format!(
                    "{name}: its scale inheritance (type {inherit}) sits below a sheared node, a type 1 child of a non-uniform scale; this reader has no one rotation and scale to split it into: bake the scale and export again"
                ));
            };
            let above = if inherit == 2 {
                let sp = self.local_scale;
                if sp.contains(&0.0) {
                    return Err(format!(
                        "{name}: scale inheritance type 2 under a parent scaled by zero"
                    ));
                }
                [above[0] / sp[0], above[1] / sp[1], above[2] / sp[2]]
            } else {
                above
            };
            let scale = mul(above, s);
            (quat::mat3_mul(&new_rot, &diag(scale)), Some(scale))
        };
        Ok(World {
            a: Affine { m, t },
            rot: new_rot,
            scale,
            local_scale: s,
        })
    }
}

fn is_uniform(s: Vec3) -> bool {
    let tolerance = 1e-9 * s[0].abs().max(s[1].abs()).max(s[2].abs());
    (s[0] - s[1]).abs() <= tolerance && (s[1] - s[2]).abs() <= tolerance
}

/// Values of a `Properties70` block by property name.
fn properties(node: &Node) -> BTreeMap<String, Vec<Prop>> {
    let mut out = BTreeMap::new();
    if let Some(p70) = node.child("Properties70") {
        for p in p70.children_named("P") {
            if let Some(name) = p.str_at(0) {
                out.insert(
                    name.to_string(),
                    p.props.get(4..).map(<[Prop]>::to_vec).unwrap_or_default(),
                );
            }
        }
    }
    out
}

fn vec3_of(props: &BTreeMap<String, Vec<Prop>>, name: &str, default: Vec3) -> R<Vec3> {
    let Some(v) = props.get(name) else {
        return Ok(default);
    };
    let x = v.first().and_then(Prop::as_f64);
    let y = v.get(1).and_then(Prop::as_f64);
    let z = v.get(2).and_then(Prop::as_f64);
    match (x, y, z) {
        (Some(x), Some(y), Some(z)) if x.is_finite() && y.is_finite() && z.is_finite() => Ok([x, y, z]),
        _ => Err(format!("{name}: expected three finite numbers")),
    }
}

fn int_of(props: &BTreeMap<String, Vec<Prop>>, name: &str, default: i64) -> i64 {
    props
        .get(name)
        .and_then(|v| v.first())
        .and_then(Prop::as_i64)
        .unwrap_or(default)
}

fn float_of(props: &BTreeMap<String, Vec<Prop>>, name: &str) -> Option<f64> {
    props
        .get(name)
        .and_then(|v| v.first())
        .and_then(Prop::as_f64)
        .filter(|x| x.is_finite())
}

/// `Name\x00\x01Class` -> `Name`.
fn object_name(raw: &str) -> String {
    raw.split("\u{0}\u{1}").next().unwrap_or(raw).to_string()
}

/// One animation curve: key times (ticks) and values.
#[derive(Debug, Clone, Default)]
struct Curve {
    times: Vec<i64>,
    values: Vec<f64>,
    /// A key says cubic: this reader draws straight lines all the same, and says so.
    cubic: bool,
}

impl Curve {
    /// The value at `t`: straight lines between keys, held flat outside them.
    fn at(&self, t: f64) -> Option<f64> {
        let n = self.times.len().min(self.values.len());
        if n == 0 {
            return None;
        }
        let (Some(&t0), Some(&v0), Some(&tn), Some(&vn)) = (
            self.times.first(),
            self.values.first(),
            self.times.get(n - 1),
            self.values.get(n - 1),
        ) else {
            return None;
        };
        if t <= t0 as f64 {
            return Some(v0);
        }
        if t >= tn as f64 {
            return Some(vn);
        }
        let i = self
            .times
            .get(..n)
            .map_or(0, |s| s.partition_point(|&k| (k as f64) <= t))
            .saturating_sub(1);
        let (Some(&ta), Some(&tb), Some(&va), Some(&vb)) = (
            self.times.get(i),
            self.times.get(i + 1),
            self.values.get(i),
            self.values.get(i + 1),
        ) else {
            return Some(vn);
        };
        // key times are the file's: the difference is taken in f64 so no pair can overflow
        let span = tb as f64 - ta as f64;
        if span <= 0.0 {
            return Some(va);
        }
        Some(va + (vb - va) * (t - ta as f64) / span)
    }
}

/// A model's animated channels: curves on X, Y, Z of a property, with the curve node's defaults.
#[derive(Debug, Clone, Default)]
struct Channel {
    default: [Option<f64>; 3],
    curves: [Option<Curve>; 3],
}

impl Channel {
    fn at(&self, t: f64, fallback: Vec3) -> Vec3 {
        let mut out = fallback;
        for (k, o) in out.iter_mut().enumerate() {
            if let Some(v) = self.curves.get(k).and_then(|c| c.as_ref()).and_then(|c| c.at(t)) {
                *o = v;
            } else if let Some(d) = self.default.get(k).copied().flatten() {
                *o = d;
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
struct Model {
    name: String,
    class: String,
    parent: Option<i64>,
    transform: Transform,
    translation: Channel,
    rotation: Channel,
    scaling: Channel,
}

/// The joints' world positions on every frame, in the rig's axes and metres.
#[derive(Debug, Clone)]
pub struct Motion {
    pub names: Vec<String>,
    /// The model class (`LimbNode`, `Null`, `Mesh`, ...).
    pub classes: Vec<String>,
    pub parents: Vec<Option<usize>>,
    pub fps: f64,
    pub frames: usize,
    /// Where sample 0 is in the file's own timeline, in frames (the take's start).
    pub start_frame: f64,
    /// `[frame][joint]`.
    pub positions: Vec<Vec<Vec3>>,
    pub version: u32,
    pub up_axis: String,
    pub front_axis: String,
    pub unit_scale: f64,
    /// Where the frame rate came from (`TimeMode`, `CustomFrameRate`, `keys`).
    pub fps_from: String,
    pub take: String,
    pub notes: Vec<String>,
}

/// The axis system as `GlobalSettings` states it: the matrix taking the file's axes to the rig's.
fn axis_matrix(settings: &BTreeMap<String, Vec<Prop>>) -> R<(Mat3, String, String)> {
    let up = int_of(settings, "UpAxis", -1);
    let front = int_of(settings, "FrontAxis", -1);
    let coord = int_of(settings, "CoordAxis", -1);
    if !(0..3).contains(&up) {
        return Err("GlobalSettings has no UpAxis: the file does not say which way is up".into());
    }
    if !(0..3).contains(&front) {
        return Err("GlobalSettings has no FrontAxis: the file does not say which way is forward".into());
    }
    let sign = |name: &str| if int_of(settings, name, 1) < 0 { -1.0 } else { 1.0 };
    let unit = |axis: i64, s: f64| -> Vec3 {
        let mut v = [0.0; 3];
        if let Some(x) = v.get_mut(axis as usize) {
            *x = s;
        }
        v
    };
    if up == front {
        return Err(format!("GlobalSettings: UpAxis and FrontAxis are both axis {up}"));
    }
    let u = unit(up, sign("UpAxisSign"));
    let f = unit(front, sign("FrontAxisSign"));
    let left = quat::cross(u, f);
    if (0..3).contains(&coord) {
        let c = unit(coord, sign("CoordAxisSign"));
        let dot = c[0] * left[0] + c[1] * left[1] + c[2] * left[2];
        if dot < 0.0 {
            return Err(
                "a left-handed axis system (CoordAxis against up x front) is not supported yet".into(),
            );
        }
    }
    // rows: the file's left -> +X, front -> -Y, up -> +Z
    let m = [left, quat::scale3(f, -1.0), u];
    let name = |v: Vec3| -> String {
        let (i, s) = v
            .iter()
            .enumerate()
            .find(|(_, x)| **x != 0.0)
            .map_or((0, 1.0), |(i, x)| (i, *x));
        format!(
            "{}{}",
            if s < 0.0 { "-" } else { "+" },
            ["X", "Y", "Z"].get(i).copied().unwrap_or("?")
        )
    };
    Ok((m, name(u), name(f)))
}

/// Sample the file's animation: every frame of the first take.
pub fn motion(doc: &Document) -> R<Motion> {
    let mut notes = Vec::new();
    let settings = doc
        .top("GlobalSettings")
        .map(properties)
        .ok_or("no GlobalSettings: the file does not say its axes or units")?;
    let (axes, up_name, front_name) = axis_matrix(&settings)?;
    let unit_scale = float_of(&settings, "UnitScaleFactor")
        .filter(|s| *s > 0.0)
        .ok_or("GlobalSettings has no UnitScaleFactor: the file does not say its unit")?;
    let objects = doc.top("Objects").ok_or("no Objects: the file has no scene")?;
    let connections = doc
        .top("Connections")
        .ok_or("no Connections: the file has no hierarchy")?;

    // models
    let mut models: Vec<Model> = Vec::new();
    let mut by_id: BTreeMap<i64, usize> = BTreeMap::new();
    for m in objects.children_named("Model") {
        let id = m.i64_at(0).ok_or("a Model has no id")?;
        let name = object_name(m.str_at(1).unwrap_or(""));
        let class = m.str_at(2).unwrap_or("").to_string();
        let p = properties(m);
        let transform = Transform {
            translation: vec3_of(&p, "Lcl Translation", [0.0; 3])
                .map_err(|e| format!("model {name}: {e}"))?,
            rotation: vec3_of(&p, "Lcl Rotation", [0.0; 3]).map_err(|e| format!("model {name}: {e}"))?,
            scaling: vec3_of(&p, "Lcl Scaling", [1.0; 3]).map_err(|e| format!("model {name}: {e}"))?,
            pre_rotation: vec3_of(&p, "PreRotation", [0.0; 3]).map_err(|e| format!("model {name}: {e}"))?,
            post_rotation: vec3_of(&p, "PostRotation", [0.0; 3]).map_err(|e| format!("model {name}: {e}"))?,
            rotation_offset: vec3_of(&p, "RotationOffset", [0.0; 3])
                .map_err(|e| format!("model {name}: {e}"))?,
            rotation_pivot: vec3_of(&p, "RotationPivot", [0.0; 3])
                .map_err(|e| format!("model {name}: {e}"))?,
            scaling_offset: vec3_of(&p, "ScalingOffset", [0.0; 3])
                .map_err(|e| format!("model {name}: {e}"))?,
            scaling_pivot: vec3_of(&p, "ScalingPivot", [0.0; 3]).map_err(|e| format!("model {name}: {e}"))?,
            order: int_of(&p, "RotationOrder", 0),
            // the format applies pre/post rotations only with RotationActive; most writers set it,
            // and some leave it out while writing a PreRotation they mean: a present PreRotation
            // counts as active
            rotation_active: int_of(&p, "RotationActive", 0) != 0 || p.contains_key("PreRotation"),
            inherit: int_of(&p, "InheritType", 0),
        };
        if !(0..=2).contains(&transform.inherit) {
            return Err(format!(
                "model {name}: InheritType {} is not one of the format's three",
                transform.inherit
            ));
        }
        if !(0..=5).contains(&transform.order) {
            notes.push(format!(
                "{name}: rotation order {} is not one of the six Euler orders; read as ZYX",
                transform.order
            ));
        }
        if models.len() >= MAX_MODELS {
            return Err(format!("more than {MAX_MODELS} models"));
        }
        if let Some(&other) = by_id.get(&id) {
            let first = models.get(other).map_or("", |m| m.name.as_str());
            return Err(format!(
                "two models share the id {id} ({first:?} and {name:?}): the file is corrupt"
            ));
        }
        by_id.insert(id, models.len());
        models.push(Model {
            name,
            class,
            parent: None,
            transform,
            translation: Channel::default(),
            rotation: Channel::default(),
            scaling: Channel::default(),
        });
    }
    if models.is_empty() {
        return Err("the file has no models (no skeleton)".into());
    }

    // animation objects
    let mut stacks: Vec<(i64, String, BTreeMap<String, Vec<Prop>>)> = Vec::new();
    let mut layers: Vec<i64> = Vec::new();
    let mut layer_nodes: BTreeMap<i64, (String, BTreeMap<String, Vec<Prop>>)> = BTreeMap::new();
    let mut curve_nodes: BTreeMap<i64, [Option<f64>; 3]> = BTreeMap::new();
    let mut curves: BTreeMap<i64, Curve> = BTreeMap::new();
    for n in &objects.children {
        match n.name.as_str() {
            "AnimationStack" => {
                if let Some(id) = n.i64_at(0) {
                    stacks.push((id, object_name(n.str_at(1).unwrap_or("")), properties(n)));
                }
            }
            "AnimationLayer" => {
                if let Some(id) = n.i64_at(0) {
                    layers.push(id);
                    layer_nodes.insert(id, (object_name(n.str_at(1).unwrap_or("")), properties(n)));
                }
            }
            "AnimationCurveNode" => {
                if let Some(id) = n.i64_at(0) {
                    let p = properties(n);
                    curve_nodes.insert(
                        id,
                        [float_of(&p, "d|X"), float_of(&p, "d|Y"), float_of(&p, "d|Z")],
                    );
                }
            }
            "AnimationCurve" => {
                if let Some(id) = n.i64_at(0) {
                    let times = match n.child("KeyTime").and_then(|k| k.prop(0)) {
                        None => Vec::new(),
                        Some(k) => k.as_i64s().ok_or_else(|| {
                            format!("animation curve {id}: its key times are not whole numbers of ticks")
                        })?,
                    };
                    let cubic = n
                        .child("KeyAttrFlags")
                        .and_then(|k| k.prop(0))
                        .and_then(Prop::as_i64s)
                        .is_some_and(|flags| flags.iter().any(|f| f & CUBIC_FLAG != 0));
                    let values = n
                        .child("KeyValueFloat")
                        .and_then(|k| k.prop(0))
                        .and_then(Prop::as_f64s)
                        .unwrap_or_default();
                    if times.len() != values.len() {
                        return Err(format!(
                            "animation curve {id}: {} key times but {} values",
                            times.len(),
                            values.len()
                        ));
                    }
                    if times.len() > MAX_KEYS {
                        return Err(format!(
                            "animation curve {id}: {} keys is more than {MAX_KEYS}",
                            times.len()
                        ));
                    }
                    if times.windows(2).any(|w| w[1] < w[0]) {
                        return Err(format!("animation curve {id}: its key times are not in order"));
                    }
                    if values.iter().any(|v| !v.is_finite()) {
                        return Err(format!("animation curve {id}: a key value is not finite"));
                    }
                    curves.insert(id, Curve { times, values, cubic });
                }
            }
            _ => {}
        }
    }

    // connections
    let mut layer_of_node: BTreeMap<i64, i64> = BTreeMap::new();
    // in the file's connection order: that, not where the objects sit, is the layers' order
    let mut stack_of_layer: Vec<(i64, i64)> = Vec::new();
    let mut weight_animated: Vec<i64> = Vec::new();
    let mut node_targets: Vec<(i64, i64, String)> = Vec::new(); // curve node -> model, property
    let mut curve_targets: Vec<(i64, i64, String)> = Vec::new(); // curve -> curve node, d|X
    for c in connections.children_named("C") {
        let (Some(kind), Some(child), Some(parent)) = (c.str_at(0), c.i64_at(1), c.i64_at(2)) else {
            continue;
        };
        match kind {
            "OO" => {
                if let Some(&ci) = by_id.get(&child) {
                    if by_id.contains_key(&parent)
                        && let Some(m) = models.get_mut(ci)
                    {
                        m.parent = Some(parent);
                    }
                } else if layers.contains(&child) && stacks.iter().any(|s| s.0 == parent) {
                    stack_of_layer.push((child, parent));
                } else if curve_nodes.contains_key(&child) && layers.contains(&parent) {
                    layer_of_node.insert(child, parent);
                }
            }
            "OP" => {
                let prop = c.str_at(3).unwrap_or("").to_string();
                if curve_nodes.contains_key(&child) && layers.contains(&parent) && prop == "Weight" {
                    weight_animated.push(parent);
                } else if curve_nodes.contains_key(&child) && by_id.contains_key(&parent) {
                    node_targets.push((child, parent, prop));
                } else if curves.contains_key(&child) && curve_nodes.contains_key(&parent) {
                    curve_targets.push((child, parent, prop));
                }
            }
            _ => {}
        }
    }

    // the take: the first stack and its first layer
    let (take, stack_props, wanted_layer) = match stacks.first() {
        Some((sid, name, p)) => {
            if stacks.len() > 1 {
                notes.push(format!(
                    "the file has {} takes (animation stacks): the first, {name:?}, was read",
                    stacks.len()
                ));
            }
            let of_stack: Vec<i64> = stack_of_layer
                .iter()
                .filter(|(_, s)| s == sid)
                .map(|(l, _)| *l)
                .collect();
            let plan = LayerPlan {
                take: name,
                nodes: &layer_nodes,
                weight_animated: &weight_animated,
                layer_of_node: &layer_of_node,
                node_targets: &node_targets,
                curve_targets: &curve_targets,
                curves: &curves,
                curve_nodes: &curve_nodes,
            };
            if let Some(note) = plan.check(&of_stack)? {
                notes.push(note);
            }
            (name.clone(), Some(p), of_stack.first().copied())
        }
        None => (String::new(), None, None),
    };
    let mut used_nodes = 0;
    let mut elsewhere = 0; // curve nodes with curves that belong to another take's layer
    for (cn, model_id, prop) in &node_targets {
        let has_curves = curve_targets
            .iter()
            .any(|(c, n, _)| n == cn && curves.contains_key(c));
        if let Some(l) = wanted_layer
            && layer_of_node.get(cn) != Some(&l)
        {
            if has_curves {
                elsewhere += 1;
            }
            continue;
        }
        if !has_curves {
            continue;
        }
        let Some(&mi) = by_id.get(model_id) else { continue };
        let defaults = curve_nodes.get(cn).copied().unwrap_or([None; 3]);
        let mut channel = Channel {
            default: defaults,
            curves: [None, None, None],
        };
        for (curve_id, node_id, axis) in &curve_targets {
            if node_id != cn {
                continue;
            }
            let k = match axis.as_str() {
                "d|X" => 0,
                "d|Y" => 1,
                "d|Z" => 2,
                _ => continue,
            };
            if let (Some(c), Some(slot)) = (curves.get(curve_id), channel.curves.get_mut(k)) {
                *slot = Some(c.clone());
            }
        }
        used_nodes += 1;
        if let Some(m) = models.get_mut(mi) {
            match prop.as_str() {
                "Lcl Translation" => m.translation = channel,
                "Lcl Rotation" => m.rotation = channel,
                "Lcl Scaling" => m.scaling = channel,
                _ => used_nodes -= 1,
            }
        }
    }
    if used_nodes == 0 {
        if elsewhere > 0 {
            return Err(format!(
                "the first take, {take:?}, drives no model: the file's curves belong to another take's layer"
            ));
        }
        if curves.is_empty() {
            return Err("the file has no animation: no animation curves (a character in its rest pose, or an empty take)".into());
        }
        return Err(
            "the file has no animation: no curve drives a model's translation, rotation or scale".into(),
        );
    }

    // the frame rate and the range
    let mode = int_of(&settings, "TimeMode", -1);
    let (fps, fps_from) = match TIME_MODES.get(usize::try_from(mode).unwrap_or(usize::MAX)) {
        Some(&rate) if rate > 0.0 => (rate, "TimeMode".to_string()),
        Some(_) if mode == 14 => match float_of(&settings, "CustomFrameRate").filter(|r| *r > 0.0) {
            Some(r) => (r, "CustomFrameRate".to_string()),
            None => (key_rate(&models)?, "keys".to_string()),
        },
        _ => (key_rate(&models)?, "keys".to_string()),
    };
    if !(1.0..=1000.0).contains(&fps) {
        return Err(format!("a frame rate of {fps} is outside 1..=1000"));
    }
    let ticks = TICKS_PER_SECOND / fps;
    let range = |name_a: &str, name_b: &str| -> Option<(i64, i64)> {
        let p = stack_props?;
        let a = p.get(name_a)?.first()?.as_i64()?;
        let b = p.get(name_b)?.first()?.as_i64()?;
        (b > a).then_some((a, b))
    };
    let (start, stop) =
        match range("LocalStart", "LocalStop").or_else(|| range("ReferenceStart", "ReferenceStop")) {
            Some(r) => r,
            None => {
                let mut lo = i64::MAX;
                let mut hi = i64::MIN;
                for m in &models {
                    for ch in [&m.translation, &m.rotation, &m.scaling] {
                        for c in ch.curves.iter().flatten() {
                            if let (Some(&a), Some(&b)) = (c.times.first(), c.times.last()) {
                                lo = lo.min(a);
                                hi = hi.max(b);
                            }
                        }
                    }
                }
                if lo > hi {
                    return Err("the animation has no keys".into());
                }
                notes.push("the take states no time range: the keys' span was read".into());
                (lo, hi)
            }
        };
    // the range is the file's: taken in f64 so a start and stop at the ends of i64 can't overflow,
    // and checked against the cap before it becomes a count
    let span = ((stop as f64 - start as f64) / ticks).round();
    if !span.is_finite() || span < 0.0 {
        return Err("the take's time range is not usable".into());
    }
    if span >= MAX_FRAMES as f64 {
        return Err(format!(
            "{:.0} frames at {fps} fps is more than {MAX_FRAMES}: cut the take into shorter ones",
            span + 1.0
        ));
    }
    let frames = span as usize + 1;
    let samples = frames.saturating_mul(models.len());
    if samples > MAX_SAMPLES {
        return Err(format!(
            "{frames} frames of {} models is {samples} joint positions, more than {MAX_SAMPLES}: cut the take into shorter ones, or strip the scene's extra models",
            models.len()
        ));
    }
    let off_grid = models
        .iter()
        .flat_map(|m| [&m.translation, &m.rotation, &m.scaling])
        .flat_map(|ch| ch.curves.iter().flatten())
        .filter(|c| {
            c.times.iter().any(|&t| {
                let at = (t as f64 - start as f64) / ticks;
                (at - at.round()).abs() > 1e-3
            })
        })
        .count();
    if off_grid > 0 {
        notes.push(format!(
            "{off_grid} curves have keys between frames: they are read with straight lines between keys"
        ));
    }
    let sparse_cubic = models
        .iter()
        .flat_map(|m| [&m.translation, &m.rotation, &m.scaling])
        .flat_map(|ch| ch.curves.iter().flatten())
        .filter(|c| {
            c.cubic
                && c.times
                    .windows(2)
                    .any(|w| (w[1] as f64 - w[0] as f64) > ticks * 1.001)
        })
        .count();
    if sparse_cubic > 0 {
        notes.push(format!(
            "{sparse_cubic} curves have cubic keys more than a frame apart: they are read with straight lines between keys, which can differ from the curve; bake every frame to be exact"
        ));
    }
    if mode == 0 {
        notes.push(
            "the file leaves TimeMode at its default: the frame rate was read from the keys' spacing".into(),
        );
    }

    // the hierarchy, parents first
    let parent_index: Vec<Option<usize>> = models
        .iter()
        .map(|m| m.parent.and_then(|p| by_id.get(&p).copied()))
        .collect();
    let mut order: Vec<usize> = Vec::with_capacity(models.len());
    let mut placed = vec![false; models.len()];
    let mut pending: Vec<usize> = (0..models.len()).collect();
    while !pending.is_empty() {
        let before = order.len();
        pending.retain(|&i| {
            let ready = match parent_index.get(i).copied().flatten() {
                None => true,
                Some(p) => placed.get(p).copied().unwrap_or(true),
            };
            if ready {
                order.push(i);
                if let Some(x) = placed.get_mut(i) {
                    *x = true;
                }
            }
            !ready
        });
        if order.len() == before {
            let names: Vec<&str> = pending
                .iter()
                .filter_map(|&i| models.get(i))
                .map(|m| m.name.as_str())
                .take(5)
                .collect();
            return Err(format!(
                "the model hierarchy loops (a node is its own ancestor): {names:?}"
            ));
        }
    }

    // sample
    let to_metres = unit_scale / 100.0;
    let mut positions: Vec<Vec<Vec3>> = Vec::with_capacity(frames);
    let mut world = vec![World::root(Affine::IDENTITY, MAT3_IDENTITY, [1.0; 3]); models.len()];
    for f in 0..frames {
        let t = start as f64 + f as f64 * ticks;
        for &i in &order {
            let Some(m) = models.get(i) else { continue };
            let tr = m.translation.at(t, m.transform.translation);
            let ro = m.rotation.at(t, m.transform.rotation);
            let sc = m.scaling.at(t, m.transform.scaling);
            let (local, rot) = m.transform.local(tr, ro, sc);
            let w = match parent_index.get(i).copied().flatten().and_then(|p| world.get(p)) {
                Some(pw) => pw.child(local, rot, sc, m.transform.inherit, &m.name)?,
                None => World::root(local, rot, sc),
            };
            if let Some(slot) = world.get_mut(i) {
                *slot = w;
            }
        }
        let row: Vec<Vec3> = world
            .iter()
            .map(|w| quat::scale3(quat::mat3_vec(&axes, w.a.t), to_metres))
            .collect();
        if row.iter().any(|p| p.iter().any(|x| !x.is_finite())) {
            return Err(format!("frame {f}: a joint position is not finite"));
        }
        positions.push(row);
    }
    Ok(Motion {
        names: models.iter().map(|m| m.name.clone()).collect(),
        classes: models.iter().map(|m| m.class.clone()).collect(),
        parents: parent_index,
        fps,
        frames,
        start_frame: start as f64 / ticks,
        positions,
        version: doc.version,
        up_axis: up_name,
        front_axis: front_name,
        unit_scale,
        fps_from,
        take,
        notes,
    })
}

/// What says whether a take's extra animation layers change its pose. Layers blend, so a later one
/// that does anything would move the joints from what the first alone gives, and this reader doesn't
/// blend them: such a take is refused. A layer that is muted, at weight 0, or additive with nothing
/// but zero offsets (a scale of 1) changes nothing, and is passed over and said.
struct LayerPlan<'a> {
    take: &'a str,
    nodes: &'a BTreeMap<i64, (String, BTreeMap<String, Vec<Prop>>)>,
    weight_animated: &'a [i64],
    layer_of_node: &'a BTreeMap<i64, i64>,
    node_targets: &'a [(i64, i64, String)],
    curve_targets: &'a [(i64, i64, String)],
    curves: &'a BTreeMap<i64, Curve>,
    curve_nodes: &'a BTreeMap<i64, [Option<f64>; 3]>,
}

impl LayerPlan<'_> {
    /// Does `layer`, on top of what is below it, move anything?
    fn moves_anything(&self, layer: i64) -> bool {
        let additive = self
            .nodes
            .get(&layer)
            .is_none_or(|(_, p)| int_of(p, "BlendMode", 0) == 0);
        self.node_targets.iter().any(|(cn, _, prop)| {
            if self.layer_of_node.get(cn) != Some(&layer)
                || !matches!(prop.as_str(), "Lcl Translation" | "Lcl Rotation" | "Lcl Scaling")
            {
                return false;
            }
            if !additive {
                return true;
            }
            let neutral = if prop == "Lcl Scaling" { 1.0 } else { 0.0 };
            let of_node: Vec<&(i64, i64, String)> =
                self.curve_targets.iter().filter(|(_, n, _)| n == cn).collect();
            let curved = of_node
                .iter()
                .filter_map(|(c, _, _)| self.curves.get(c))
                .any(|c| c.values.iter().any(|v| *v != neutral));
            let default = ["d|X", "d|Y", "d|Z"].iter().enumerate().any(|(k, axis)| {
                !of_node.iter().any(|(_, _, a)| a == axis)
                    && self
                        .curve_nodes
                        .get(cn)
                        .and_then(|d| d.get(k).copied().flatten())
                        .is_some_and(|v| v != neutral)
            });
            curved || default
        })
    }

    /// Refuse what can't be read from the first layer alone; a note if layers were passed over.
    fn check(&self, of_stack: &[i64]) -> R<Option<String>> {
        let take = self.take;
        let (Some(&base), Some(rest)) = (of_stack.first(), of_stack.get(1..)) else {
            return Ok(None);
        };
        let about = |l: i64| -> (&str, f64, i64, i64, i64) {
            self.nodes.get(&l).map_or(("", 100.0, 0, 0, 0), |(name, p)| {
                (
                    name.as_str(),
                    float_of(p, "Weight").unwrap_or(100.0),
                    int_of(p, "Mute", 0),
                    int_of(p, "Solo", 0),
                    int_of(p, "BlendMode", 0),
                )
            })
        };
        let bake = "bake the layers into one and export again";
        let (base_name, base_weight, base_mute, base_solo, _) = about(base);
        if base_mute != 0 || base_weight != 100.0 || self.weight_animated.contains(&base) {
            return Err(format!(
                "take {take:?}: its first animation layer, {base_name:?}, is muted or not at full weight, and layers are not blended: {bake}"
            ));
        }
        let mut passed = 0;
        for &l in rest {
            let (name, weight, mute, solo, blend) = about(l);
            if solo != 0 && self.moves_anything(l) {
                return Err(format!(
                    "take {take:?}: animation layer {name:?} is soloed, and layers are not blended: {bake}"
                ));
            }
            let silent = (base_solo != 0 && solo == 0)
                || mute != 0
                || (weight == 0.0 && !self.weight_animated.contains(&l))
                || !self.moves_anything(l);
            if !silent {
                let kind = match blend {
                    0 => "additive",
                    1 => "override",
                    2 => "override passthrough",
                    _ => "an unknown blend mode",
                };
                return Err(format!(
                    "take {take:?}: animation layer {name:?} ({kind}, weight {weight}) changes the pose, and layers are not blended: {bake}"
                ));
            }
            passed += 1;
        }
        Ok((passed > 0).then(|| {
            format!(
                "take {take:?}: {passed} more animation layers change nothing (muted, weight 0 or no offset) and were passed over"
            )
        }))
    }
}

/// The frame rate the keys suggest: the median spacing of the longest curve's keys.
fn key_rate(models: &[Model]) -> R<f64> {
    let longest = models
        .iter()
        .flat_map(|m| [&m.translation, &m.rotation, &m.scaling])
        .flat_map(|ch| ch.curves.iter().flatten())
        .max_by_key(|c| c.times.len())
        .ok_or("the file has no animation curves")?;
    let gaps: Vec<f64> = longest
        .times
        .windows(2)
        .map(|w| w[1] as f64 - w[0] as f64)
        .filter(|g| *g > 0.0)
        .collect();
    let gap = ac_core::retarget::median(&gaps);
    if gap <= 0.0 {
        return Err("the file states no frame rate (TimeMode) and its keys do not show one".into());
    }
    Ok(TICKS_PER_SECOND / gap)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn euler_orders_name_the_axis_that_turns_first() {
        // ZXY: Z first, then X, then Y; the angles stay (x, y, z)
        let m = euler([30.0, 40.0, 50.0], 4);
        let want = quat::mat3_mul(
            &axis_rotation(1, 40.0),
            &quat::mat3_mul(&axis_rotation(0, 30.0), &axis_rotation(2, 50.0)),
        );
        for r in 0..3 {
            for c in 0..3 {
                assert!((m[r][c] - want[r][c]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn a_curve_holds_flat_outside_its_keys_and_is_straight_between() {
        let c = Curve {
            times: vec![0, 10, 20],
            values: vec![1.0, 3.0, 2.0],
            cubic: false,
        };
        assert_eq!(c.at(-5.0), Some(1.0));
        assert_eq!(c.at(5.0), Some(2.0));
        assert_eq!(c.at(15.0), Some(2.5));
        assert_eq!(c.at(50.0), Some(2.0));
        assert_eq!(Curve::default().at(0.0), None);
    }

    #[test]
    fn the_axis_system_is_read_not_assumed() {
        let mut s: BTreeMap<String, Vec<Prop>> = BTreeMap::new();
        assert!(axis_matrix(&s).unwrap_err().contains("UpAxis"));
        s.insert("UpAxis".into(), vec![Prop::I32(1)]);
        s.insert("FrontAxis".into(), vec![Prop::I32(2)]);
        s.insert("CoordAxis".into(), vec![Prop::I32(0)]);
        let (m, up, front) = axis_matrix(&s).unwrap();
        assert_eq!((up.as_str(), front.as_str()), ("+Y", "+Z"));
        // a Y-up point (x, y, z) lands at (x, -z, y)
        assert_eq!(quat::mat3_vec(&m, [1.0, 2.0, 3.0]), [1.0, -3.0, 2.0]);
        s.insert("CoordAxisSign".into(), vec![Prop::I32(-1)]);
        assert!(axis_matrix(&s).unwrap_err().contains("left-handed"));
        s.insert("CoordAxisSign".into(), vec![Prop::I32(1)]);
        s.insert("UpAxis".into(), vec![Prop::I32(2)]);
        s.insert("FrontAxis".into(), vec![Prop::I32(1)]);
        s.insert("FrontAxisSign".into(), vec![Prop::I32(-1)]);
        let (m, up, front) = axis_matrix(&s).unwrap();
        assert_eq!((up.as_str(), front.as_str()), ("+Z", "-Y"));
        assert_eq!(quat::mat3_vec(&m, [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    fn node(name: &str, props: Vec<Prop>, children: Vec<Node>) -> Node {
        Node {
            name: name.into(),
            props,
            children,
        }
    }

    /// A `Properties70` entry: name, type, label, flags, then the value.
    fn p(name: &str, value: Prop) -> Node {
        node(
            "P",
            vec![
                Prop::Str(name.into()),
                Prop::Str(String::new()),
                Prop::Str(String::new()),
                Prop::Str(String::new()),
                value,
            ],
            vec![],
        )
    }

    fn p70(entries: Vec<Node>) -> Node {
        node("Properties70", vec![], entries)
    }

    fn c(kind: &str, child: i64, parent: i64, prop: Option<&str>) -> Node {
        let mut props = vec![Prop::Str(kind.into()), Prop::I64(child), Prop::I64(parent)];
        if let Some(pr) = prop {
            props.push(Prop::Str(pr.into()));
        }
        node("C", props, vec![])
    }

    fn object(name: &str, id: i64, label: &str, class: &str, children: Vec<Node>) -> Node {
        node(
            name,
            vec![
                Prop::I64(id),
                Prop::Str(format!("{label}\u{0}\u{1}{name}")),
                Prop::Str(class.into()),
            ],
            children,
        )
    }

    /// A Y-up scene at 30 fps of `models` joints (ids 1000..), the first driven by one translation
    /// curve of two keys, and a take whose range is `start..stop` ticks.
    fn scene(models: usize, start: i64, stop: i64) -> Document {
        let settings = node(
            "GlobalSettings",
            vec![],
            vec![p70(vec![
                p("UpAxis", Prop::I32(1)),
                p("FrontAxis", Prop::I32(2)),
                p("CoordAxis", Prop::I32(0)),
                p("UnitScaleFactor", Prop::F64(1.0)),
                p("TimeMode", Prop::I32(6)),
            ])],
        );
        let mut objects = Vec::new();
        for i in 0..models {
            objects.push(object(
                "Model",
                1000 + i as i64,
                &format!("J{i}"),
                "LimbNode",
                vec![],
            ));
        }
        objects.push(object(
            "AnimationStack",
            1,
            "Take",
            "",
            vec![p70(vec![
                p("LocalStart", Prop::I64(start)),
                p("LocalStop", Prop::I64(stop)),
            ])],
        ));
        objects.push(object("AnimationLayer", 2, "Base", "", vec![]));
        objects.push(object("AnimationCurveNode", 3, "T", "", vec![]));
        objects.push(object(
            "AnimationCurve",
            4,
            "",
            "",
            vec![
                node(
                    "KeyTime",
                    vec![Prop::I64s(vec![0, TICKS_PER_SECOND as i64])],
                    vec![],
                ),
                node("KeyValueFloat", vec![Prop::F32s(vec![0.0, 1.0])], vec![]),
            ],
        ));
        let connections = node(
            "Connections",
            vec![],
            vec![
                c("OO", 2, 1, None),
                c("OO", 3, 2, None),
                c("OP", 3, 1000, Some("Lcl Translation")),
                c("OP", 4, 3, Some("d|X")),
            ],
        );
        Document {
            version: 7400,
            nodes: vec![settings, node("Objects", vec![], objects), connections],
        }
    }

    #[test]
    fn a_small_scene_samples_its_range() {
        let ticks = (TICKS_PER_SECOND / 30.0) as i64;
        let m = motion(&scene(2, 0, 10 * ticks)).unwrap();
        assert_eq!((m.frames, m.fps, m.fps_from.as_str()), (11, 30.0, "TimeMode"));
        assert_eq!(m.names, ["J0", "J1"]);
        // the curve moves J0 one unit along the file's X over the first second (cm -> m)
        assert!(
            (m.positions[10][0][0] - 0.01 / 3.0).abs() < 1e-9,
            "{:?}",
            m.positions[10][0]
        );
        assert_eq!(m.start_frame, 0.0);
        // a take that starts later says where: 5 frames in
        let late = motion(&scene(2, 5 * ticks, 10 * ticks)).unwrap();
        assert_eq!(late.start_frame.round(), 5.0);
    }

    #[test]
    fn a_take_range_at_the_ends_of_i64_is_refused_not_overflowed() {
        // stop - start would overflow i64; the range is refused as too long, with no panic
        let e = motion(&scene(1, -1, i64::MAX)).unwrap_err();
        assert!(e.contains("more than"), "{e}");
        let e = motion(&scene(1, i64::MIN, i64::MAX)).unwrap_err();
        assert!(e.contains("more than"), "{e}");
        // keys at the ends of i64 on a curve: the spacing is read in f64
        let mut d = scene(1, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        let curve = d.nodes[1]
            .children
            .iter_mut()
            .find(|n| n.name == "AnimationCurve")
            .unwrap();
        curve.children[0].props[0] = Prop::I64s(vec![i64::MIN, i64::MAX]);
        let m = motion(&d).unwrap();
        assert_eq!(m.frames, 11);
    }

    #[test]
    fn too_many_joint_positions_are_refused() {
        let ticks = (TICKS_PER_SECOND / 30.0) as i64;
        // 1001 models over 20,000 frames is more than MAX_SAMPLES; the frame count alone is fine
        let e = motion(&scene(1001, 0, (MAX_FRAMES as i64 - 1) * ticks)).unwrap_err();
        assert!(e.contains("joint positions") && e.contains("more than"), "{e}");
        assert_eq!(
            motion(&scene(1, 0, (MAX_FRAMES as i64 - 1) * ticks))
                .unwrap()
                .frames,
            MAX_FRAMES
        );
    }

    #[test]
    fn two_models_with_one_id_are_refused() {
        let mut d = scene(2, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        d.nodes[1].children[1].props[0] = Prop::I64(1000);
        let e = motion(&d).unwrap_err();
        assert!(
            e.contains("share the id 1000") && e.contains("J0") && e.contains("J1"),
            "{e}"
        );
    }

    fn model_mut(d: &mut Document, i: usize) -> &mut Node {
        d.nodes[1]
            .children
            .iter_mut()
            .filter(|n| n.name == "Model")
            .nth(i)
            .unwrap()
    }

    fn set_props(d: &mut Document, i: usize, entries: Vec<Node>) {
        model_mut(d, i).children.push(p70(entries));
    }

    fn vec3_p(name: &str, v: Vec3) -> Node {
        let mut n = p(name, Prop::F64(v[0]));
        n.props.push(Prop::F64(v[1]));
        n.props.push(Prop::F64(v[2]));
        n
    }

    /// J0 scaled by `scale`; J1 its child, turned 90° about Z, of `inherit`; J2 J1's child, 1 unit
    /// along J1's X, of type 1. Where J2 lands, in centimetres, on frame 0.
    fn inherited(scale: Vec3, inherit: i32) -> R<Vec3> {
        let mut d = scene(3, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        set_props(&mut d, 0, vec![vec3_p("Lcl Scaling", scale)]);
        set_props(
            &mut d,
            1,
            vec![
                vec3_p("Lcl Rotation", [0.0, 0.0, 90.0]),
                p("InheritType", Prop::I32(inherit)),
            ],
        );
        set_props(
            &mut d,
            2,
            vec![
                vec3_p("Lcl Translation", [1.0, 0.0, 0.0]),
                p("InheritType", Prop::I32(1)),
            ],
        );
        let conns = &mut d.nodes[2].children;
        conns.push(c("OO", 1001, 1000, None));
        conns.push(c("OO", 1002, 1001, None));
        let m = motion(&d)?;
        // the file's Y up: the rig's (x, y, z) is the file's (x, -z, y); back to the file's cm
        let p = m.positions[0][2];
        Ok([p[0] * 100.0, p[2] * 100.0, -p[1] * 100.0])
    }

    fn near(a: Vec3, b: Vec3) {
        for k in 0..3 {
            assert!((a[k] - b[k]).abs() < 1e-9, "{a:?} against {b:?}");
        }
    }

    #[test]
    fn scale_inheritance_follows_the_format() {
        // Worked by hand. The turn takes X to Y. With a uniform scale of 2 on J0: types 0 and 1
        // give J1 the matrix 2·Rz, so J2 is at (0, 2, 0); type 2 leaves J0's own scale out: (0, 1, 0).
        near(inherited([2.0; 3], 0).unwrap(), [0.0, 2.0, 0.0]);
        near(inherited([2.0; 3], 1).unwrap(), [0.0, 2.0, 0.0]);
        near(inherited([2.0; 3], 2).unwrap(), [0.0, 1.0, 0.0]);
        // A scale of 2 along X alone. Type 0: Rz · diag(2,1,1) takes X to 2X to 2Y: (0, 2, 0).
        // Type 1: diag(2,1,1) · Rz takes X to Y, which the scale leaves: (0, 1, 0). Type 2: (0, 1, 0).
        near(inherited([2.0, 1.0, 1.0], 0).unwrap(), [0.0, 2.0, 0.0]);
        near(inherited([2.0, 1.0, 1.0], 1).unwrap(), [0.0, 1.0, 0.0]);
        near(inherited([2.0, 1.0, 1.0], 2).unwrap(), [0.0, 1.0, 0.0]);
        assert!(inherited([1.0; 3], 3).unwrap_err().contains("InheritType 3"));
    }

    #[test]
    fn a_type_0_child_below_a_shear_is_refused() {
        let mut d = scene(3, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        set_props(&mut d, 0, vec![vec3_p("Lcl Scaling", [2.0, 1.0, 1.0])]);
        set_props(
            &mut d,
            1,
            vec![
                vec3_p("Lcl Rotation", [0.0, 0.0, 90.0]),
                p("InheritType", Prop::I32(1)),
            ],
        );
        let conns = &mut d.nodes[2].children;
        conns.push(c("OO", 1001, 1000, None));
        conns.push(c("OO", 1002, 1001, None));
        let e = motion(&d).unwrap_err();
        assert!(e.contains("J2") && e.contains("sheared"), "{e}");
    }

    #[test]
    fn an_animated_scale_is_inherited() {
        // the curve scales J0's X from 0 by 1 a second; J1 sits 1 cm along it, of the default type:
        // on frame 10, a third of a second, at 1/3 cm
        let ticks = (TICKS_PER_SECOND / 30.0) as i64;
        let mut d = scene(2, 0, 10 * ticks);
        set_props(&mut d, 1, vec![vec3_p("Lcl Translation", [1.0, 0.0, 0.0])]);
        let conns = &mut d.nodes[2].children;
        conns[2] = c("OP", 3, 1000, Some("Lcl Scaling"));
        conns.push(c("OO", 1001, 1000, None));
        let m = motion(&d).unwrap();
        assert!(
            (m.positions[10][1][0] - 0.01 / 3.0).abs() < 1e-9,
            "{:?}",
            m.positions[10][1]
        );
    }

    /// `scene` with a second layer, id 5, of the first take, holding a curve node, id 6, on J0's
    /// rotation with a curve, id 7, of `values`; `layer` is that layer's properties.
    fn two_layers(values: Vec<f32>, layer: Vec<Node>) -> Document {
        let mut d = scene(1, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        let objects = &mut d.nodes[1].children;
        objects.push(object("AnimationLayer", 5, "Upper", "", vec![p70(layer)]));
        objects.push(object("AnimationCurveNode", 6, "R", "", vec![]));
        let times = vec![0, TICKS_PER_SECOND as i64];
        objects.push(object(
            "AnimationCurve",
            7,
            "",
            "",
            vec![
                node("KeyTime", vec![Prop::I64s(times)], vec![]),
                node("KeyValueFloat", vec![Prop::F32s(values)], vec![]),
            ],
        ));
        let conns = &mut d.nodes[2].children;
        conns.push(c("OO", 5, 1, None));
        conns.push(c("OO", 6, 5, None));
        conns.push(c("OP", 6, 1000, Some("Lcl Rotation")));
        conns.push(c("OP", 7, 6, Some("d|X")));
        d
    }

    #[test]
    fn a_later_layer_that_moves_anything_is_refused() {
        let e = motion(&two_layers(vec![0.0, 10.0], vec![])).unwrap_err();
        assert!(
            e.contains("\"Upper\"") && e.contains("additive") && e.contains("changes the pose"),
            "{e}"
        );
        // an override layer changes the pose even at zero
        let e = motion(&two_layers(vec![0.0, 0.0], vec![p("BlendMode", Prop::I32(1))])).unwrap_err();
        assert!(e.contains("override"), "{e}");
    }

    #[test]
    fn a_later_layer_that_moves_nothing_is_passed_over_and_said() {
        for layer in [vec![p("Mute", Prop::I32(1))], vec![p("Weight", Prop::F64(0.0))]] {
            let m = motion(&two_layers(vec![0.0, 10.0], layer)).unwrap();
            assert!(
                m.notes.iter().any(|n| n.contains("1 more animation layers")),
                "{:?}",
                m.notes
            );
        }
        // additive offsets of zero
        let m = motion(&two_layers(vec![0.0, 0.0], vec![])).unwrap();
        assert!(m.notes.iter().any(|n| n.contains("passed over")), "{:?}", m.notes);
    }

    #[test]
    fn the_first_layer_is_the_first_connected_and_must_be_whole() {
        // the layer connected first is the base, wherever the objects sit: connect "Upper" first,
        // and the old base, which moves J0, is the later layer that gets refused
        let mut d = two_layers(vec![0.0, 10.0], vec![]);
        let conns = &mut d.nodes[2].children;
        let upper = conns
            .iter()
            .position(|n| n.props.get(1) == Some(&Prop::I64(5)))
            .unwrap();
        conns.swap(0, upper);
        let e = motion(&d).unwrap_err();
        assert!(e.contains("\"Base\"") && e.contains("changes the pose"), "{e}");
        // a muted first layer
        let mut d = scene(1, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        d.nodes[1]
            .children
            .iter_mut()
            .find(|n| n.name == "AnimationLayer")
            .unwrap()
            .children
            .push(p70(vec![p("Mute", Prop::I32(1))]));
        assert!(motion(&d).unwrap_err().contains("first animation layer"));
    }

    #[test]
    fn an_animated_layer_weight_counts_as_moving() {
        let mut d = two_layers(vec![0.0, 10.0], vec![p("Weight", Prop::F64(0.0))]);
        d.nodes[1]
            .children
            .push(object("AnimationCurveNode", 8, "Weight", "", vec![]));
        d.nodes[2].children.push(c("OP", 8, 5, Some("Weight")));
        assert!(motion(&d).unwrap_err().contains("changes the pose"));
    }

    #[test]
    fn sparse_cubic_keys_and_a_default_time_mode_are_said() {
        // the scene's curve has keys a second apart at 30 fps
        let mut d = scene(1, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        let curve = d.nodes[1]
            .children
            .iter_mut()
            .find(|n| n.name == "AnimationCurve")
            .unwrap();
        curve
            .children
            .push(node("KeyAttrFlags", vec![Prop::I32s(vec![8])], vec![]));
        let m = motion(&d).unwrap();
        assert!(
            m.notes.iter().any(|n| n.contains("1 curves have cubic keys")),
            "{:?}",
            m.notes
        );
        for s in d.nodes[0].children[0].children.iter_mut() {
            if s.str_at(0) == Some("TimeMode") {
                s.props[4] = Prop::I32(0);
            }
        }
        let m = motion(&d).unwrap();
        assert_eq!(m.fps_from, "keys");
        assert!(
            m.notes.iter().any(|n| n.contains("TimeMode at its default")),
            "{:?}",
            m.notes
        );
    }

    #[test]
    fn key_times_that_are_not_whole_ticks_are_refused_in_those_words() {
        let mut d = scene(1, 0, 10 * (TICKS_PER_SECOND / 30.0) as i64);
        let curve = d.nodes[1]
            .children
            .iter_mut()
            .find(|n| n.name == "AnimationCurve")
            .unwrap();
        curve.children[0].props[0] = Prop::F64s(vec![0.5, 1.5]);
        assert!(motion(&d).unwrap_err().contains("not whole numbers"));
    }
}
