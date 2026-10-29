//! Pose commands: read and set the current pose, bone by bone.

use serde_json::{Map, Value, json};

use crate::command::{self, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::quat;

/// Root offsets beyond this many hip heights are refused (a figure 100 hip heights away is a typo).
pub const MAX_ROOT: f64 = 100.0;

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("rig.info", "Rig info", rig_info)
        .icon("rig")
        .outputs(&[
            ParamSpec::new("bones", Kind::Integer, "number of bones").required(),
            ParamSpec::new("names", Kind::TextList, "bone names, parents first").required(),
            ParamSpec::new("hip_height", Kind::Number, "metres").required(),
        ]),
    CommandSpec::new("pose.get", "Get pose", pose_get)
        .icon("pose")
        .inputs(&[ParamSpec::new(
            "bone",
            Kind::Bone,
            "just this bone: its local rotation",
        )])
        .outputs(&[
            ParamSpec::new("rotations", Kind::Quat, "one [x, y, z, w] per bone").list(),
            ParamSpec::new("root", Kind::Vec3, "root offset in hip heights").required(),
            ParamSpec::new("bone", Kind::Bone, "with `bone`: the bone asked for"),
            ParamSpec::new("rotation", Kind::Quat, "with `bone`: its local rotation"),
        ]),
    CommandSpec::new("pose.reset", "Reset pose", pose_reset)
        .icon("reset")
        .undoable(),
    CommandSpec::new("pose.set_bone", "Set bone rotation", pose_set_bone)
        .icon("rotate")
        .undoable()
        .inputs(&[
            ParamSpec::new("bone", Kind::Bone, "the bone to turn").required(),
            ParamSpec::new("rotation", Kind::Quat, "[x, y, z, w] relative to the parent").required(),
        ]),
    CommandSpec::new("pose.set_root", "Set root offset", pose_set_root)
        .icon("move")
        .undoable()
        .inputs(&[ParamSpec::new("root", Kind::Vec3, "offset in hip heights").required()]),
    CommandSpec::new("bone.world", "Bone world transform", bone_world)
        .icon("bone")
        .inputs(&[ParamSpec::new("bone", Kind::Bone, "the bone").required()])
        .outputs(&[
            ParamSpec::new("position", Kind::Vec3, "head position in metres").required(),
            ParamSpec::new("rotation", Kind::Quat, "world rotation").required(),
        ]),
];

fn rig_info(doc: &mut Document, _: &Map<String, Value>) -> Result<Value, String> {
    let sk = &doc.skeleton;
    Ok(json!({ "bones": sk.len(), "names": sk.names, "hip_height": sk.hip_height }))
}

fn pose_get(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    if let Some(name) = p.get("bone").and_then(Value::as_str) {
        let i = doc.bone(name)?;
        return Ok(json!({ "bone": name, "rotation": doc.pose.rotations.get(i), "root": doc.pose.root }));
    }
    Ok(json!({ "rotations": doc.pose.rotations, "root": doc.pose.root }))
}

fn pose_reset(doc: &mut Document, _: &Map<String, Value>) -> Result<Value, String> {
    doc.pose = crate::document::standing(&doc.skeleton);
    Ok(json!({}))
}

fn pose_set_bone(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    let name = p.get("bone").and_then(Value::as_str).unwrap_or_default();
    let i = doc.bone(name)?;
    let q = command::quat(p, "rotation").ok_or("rotation missing")?;
    let slot = doc
        .pose
        .rotations
        .get_mut(i)
        .ok_or_else(|| format!("pose has no slot for bone {i}"))?;
    *slot = q;
    Ok(json!({}))
}

fn pose_set_root(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    let root = command::vec3(p, "root").ok_or("root missing")?;
    if root.iter().any(|x| x.abs() > MAX_ROOT) {
        return Err(format!(
            "root offset {root:?} is more than {MAX_ROOT} hip heights away"
        ));
    }
    doc.pose.root = root;
    Ok(json!({}))
}

fn bone_world(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    let name = p.get("bone").and_then(Value::as_str).unwrap_or_default();
    let i = doc.bone(name)?;
    let (r, pos) = doc
        .skeleton
        .fk(&doc.pose.rotations, doc.pose.root)
        .map_err(|e| e.to_string())?;
    let (Some(m), Some(at)) = (r.get(i), pos.get(i)) else {
        return Err(format!("no world transform for bone {i}"));
    };
    Ok(json!({ "position": at, "rotation": quat::from_mat3(m) }))
}
