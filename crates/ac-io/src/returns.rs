//! What the commands of this crate return: their declared outputs, kept apart so the command
//! tables stay short. A const may serve several commands with the same result.

use ac_core::command::{Kind as K, ParamSpec};

pub const LIBRARY_LIST: &[ParamSpec] = &[
    ParamSpec::new(
        "animations",
        K::Text,
        "[{id, name, group, current, builtin, edited, ...}]",
    )
    .json()
    .required(),
    ParamSpec::new(
        "poses",
        K::Text,
        "[{id, name, group, part, builtin, edited, ...}]",
    )
    .json()
    .required(),
    ParamSpec::new("groups", K::TextList, "every group").required(),
    ParamSpec::new("custom_parts", K::Text, "your part sets")
        .json()
        .required(),
];

pub const LIBRARY_INSTALL: &[ParamSpec] = &[ParamSpec::new("copied", K::Integer, "files copied").required()];

pub const LIBRARY_SYNC: &[ParamSpec] = &[
    ParamSpec::new(
        "animations",
        K::Text,
        "{added, updated, retired: [id], kept: [{id, why}]}: new built-ins copied in, unchanged copies replaced by a fixed one, ones no longer shipped moved to <library>/.retired, and ones you changed left as they are",
    )
    .json()
    .required(),
    ParamSpec::new("poses", K::Text, "the same for poses").json().required(),
    ParamSpec::new("changed", K::Integer, "items added, updated or retired").required(),
];

pub const CLIP_NEW: &[ParamSpec] =
    &[ParamSpec::new("id", K::Text, "its library id (null: no library)").required()];

pub const CLIP_OPEN: &[ParamSpec] = &[
    ParamSpec::new("id", K::Text, "the library id").required(),
    ParamSpec::new("length", K::Integer, "frames").required(),
];

pub const CLIP_SAVE: &[ParamSpec] = &[ParamSpec::new("id", K::Text, "the library id").required()];

pub const CLIP_DUPLICATE: &[ParamSpec] = &[ParamSpec::new("id", K::Text, "the copy").required()];

pub const CLIP_DELETE: &[ParamSpec] = &[ParamSpec::new(
    "closed",
    K::Text,
    "the open animation was this one and is closed",
)];

pub const CLIP_IMPORT: &[ParamSpec] = &[ParamSpec::new("length", K::Integer, "frames").required()];

pub const CLIP_WRITE: &[ParamSpec] = &[ParamSpec::new("path", K::Text, "the file written").required()];

pub const ANIM_BLOCK: &[ParamSpec] = &[
    ParamSpec::new("keys", K::Integer, "poses keyed").required(),
    ParamSpec::new("fills", K::Text, "[{from, to, model}]")
        .json()
        .required(),
    ParamSpec::new("physics", K::Bool, "the physics pass plays").required(),
    ParamSpec::new("frames", K::Integer, "the animation length").required(),
];

pub const PARTS_SAVE: &[ParamSpec] = &[
    ParamSpec::new("name", K::Text, "the part set").required(),
    ParamSpec::new("joints", K::Integer, "joints in it").required(),
];

pub const LAYERS_LIST: &[ParamSpec] = &[ParamSpec::new("layers", K::Text, "[{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}]").json().required(), ParamSpec::new("edit", K::Integer, "the layer keys go on").required(), ParamSpec::new("surfaces", K::Text, "[{name, min, max, top}]").json().required(), ParamSpec::new("physics", K::Bool, "the physics pass plays").required()];

pub const LAYER_TRAVEL: &[ParamSpec] = &[ParamSpec::new("layers", K::Text, "[{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}]").json().required(), ParamSpec::new("edit", K::Integer, "the layer keys go on").required(), ParamSpec::new("surfaces", K::Text, "[{name, min, max, top}]").json().required(), ParamSpec::new("physics", K::Bool, "the physics pass plays").required(), ParamSpec::new("i", K::Integer, "the travel layer").required()];

pub const LAYER_BAKE: &[ParamSpec] = &[ParamSpec::new("layers", K::Text, "[{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}]").json().required(), ParamSpec::new("edit", K::Integer, "the layer keys go on").required(), ParamSpec::new("surfaces", K::Text, "[{name, min, max, top}]").json().required(), ParamSpec::new("physics", K::Bool, "the physics pass plays").required(), ParamSpec::new("baked", K::Integer, "keys written").required()];

pub const PHYSICS_SOLVE: &[ParamSpec] = &[
    ParamSpec::new("frames", K::Integer, "frames solved").required(),
    ParamSpec::new("flights", K::Text, "[[take-off, landing]]")
        .json()
        .required(),
    ParamSpec::new("lifted", K::Integer, "frames lifted out of the ground").required(),
    ParamSpec::new("slid", K::Integer, "frames the fulcrum rule moved").required(),
    ParamSpec::new("balanced", K::IntList, "frames balanced").required(),
    ParamSpec::new("lifted_frames", K::IntList, "the frames lifted out of the ground").required(),
    ParamSpec::new("slid_frames", K::IntList, "the frames where a sliding foot was held").required(),
    ParamSpec::new(
        "problems",
        K::Text,
        "runs of frames the pass fixed, to jump to: [{kind, from, to, label}] (kind: penetration, sliding or unbalanced)",
    )
    .json()
    .required(),
    ParamSpec::new("keyed", K::Vec3, "with `frame`: the keyed root offset"),
    ParamSpec::new("root", K::Vec3, "with `frame`: the solved root offset"),
    ParamSpec::new("keyed_balance", K::Text, "with `frame`: {com, inside} keyed").json(),
    ParamSpec::new("balance", K::Text, "with `frame`: {com, inside} solved").json(),
];

pub const PHYSICS_OVERLAY: &[ParamSpec] = &[
    ParamSpec::new("frame", K::Integer, "the whole frame shown").required(),
    ParamSpec::new("physics", K::Bool, "the pass plays").required(),
    ParamSpec::new("com", K::Vec3, "centre of mass (metres)").required(),
    ParamSpec::new(
        "support",
        K::Vec3,
        "the support outline on the ground (null: off the ground)",
    )
    .list()
    .required(),
    ParamSpec::new(
        "inside",
        K::Bool,
        "the centre of mass is over the support (null: off the ground)",
    )
    .required(),
    ParamSpec::new("contacts", K::Text, "[{point, pinned, foot}]")
        .json()
        .required(),
    ParamSpec::new("pinned", K::Integer, "contacts pinned").required(),
    ParamSpec::new("flight", K::Text, "{from, to, arc} (null: on the ground)")
        .json()
        .required(),
    ParamSpec::new("keyed_moved", K::Number, "metres the pass moved the root head").required(),
];

pub const LIBRARY_IMPORT: &[ParamSpec] = &[
    ParamSpec::new("group", K::Text, "the group they went into").required(),
    ParamSpec::new("animations", K::Text, "{copied, kept}")
        .json()
        .required(),
    ParamSpec::new("poses", K::Text, "{copied, kept}")
        .json()
        .required(),
    ParamSpec::new("skipped", K::Text, "[{file, error}]")
        .json()
        .required(),
    ParamSpec::new("cut", K::Integer, "files left out past the limit"),
];

pub const LIBRARY_SAMPLES: &[ParamSpec] = &[
    ParamSpec::new("copied", K::TextList, "samples copied").required(),
    ParamSpec::new("kept", K::TextList, "samples already there, kept").required(),
    ParamSpec::new("group", K::Text, "their group").required(),
];

pub const CLIP_EXPORT: &[ParamSpec] = &[
    ParamSpec::new("bytes", K::Integer, "file size").required(),
    ParamSpec::new("joints", K::Integer, "joints exported").required(),
    ParamSpec::new("animations", K::Integer, "animations in the file").required(),
    ParamSpec::new("frames", K::IntList, "frames per animation").required(),
    ParamSpec::new("path", K::Text, "the file written").required(),
];

pub const LAYER_TRAVEL_PREVIEW: &[ParamSpec] = &[
    ParamSpec::new("from", K::Integer, "the travel's first frame").required(),
    ParamSpec::new("to", K::Integer, "its last frame").required(),
    ParamSpec::new("gait", K::Text, "the gait it walks with").required(),
    ParamSpec::new(
        "path",
        K::Vec3,
        "where the hips go, one [x, y, z] a frame (metres)",
    )
    .list()
    .required(),
    ParamSpec::new("steps", K::Text, "its footsteps, as the travel layer keeps them")
        .json()
        .required(),
];

pub const LIBRARY_ADD_FILE: &[ParamSpec] = &[
    ParamSpec::new("kind", K::Choice(&["animation", "pose"]), "what the file was").required(),
    ParamSpec::new("id", K::Text, "its library id").required(),
    ParamSpec::new(
        "already",
        K::Bool,
        "the same item was in the library already (nothing written)",
    )
    .required(),
];

pub const CLIP_EXPORT_BATCH: &[ParamSpec] = &[
    ParamSpec::new("folder", K::Text, "the folder the files went in").required(),
    ParamSpec::new("files", K::Text, "[{id, file, path, joints, frames}] written")
        .json()
        .required(),
    ParamSpec::new("failed", K::Text, "[{id, error}] not written")
        .json()
        .required(),
];

pub const RECOVERY_WRITE: &[ParamSpec] = &[
    ParamSpec::new("file", K::Text, "the recovery file's name").required(),
    ParamSpec::new("id", K::Text, "the animation's library id (null: never saved)").required(),
];

pub const RECOVERY_LIST: &[ParamSpec] = &[
    ParamSpec::new(
        "items",
        K::Text,
        "[{file, id, name, keys, library, offer, error?}] (library: missing, same, older or newer)",
    )
    .json()
    .required(),
    ParamSpec::new("offered", K::Integer, "files worth restoring").required(),
];

pub const RECOVERY_RESTORE: &[ParamSpec] = &[
    ParamSpec::new(
        "id",
        K::Text,
        "the library id it keeps (null: opened as a new animation)",
    )
    .required(),
    ParamSpec::new("unsaved", K::Bool, "changes the library lacks").required(),
    ParamSpec::new("length", K::Integer, "frames").required(),
];

pub const RECOVERY_CLEAR: &[ParamSpec] =
    &[ParamSpec::new("removed", K::TextList, "the files removed").required()];

pub const ASSETS_RELOAD: &[ParamSpec] = &[
    ParamSpec::new("reloaded", K::TextList, "what was reloaded").required(),
    ParamSpec::new("limits", K::Text, "{bones}: joints with limits").json(),
    ParamSpec::new("library", K::Text, "{animations, poses, part_sets}").json(),
    ParamSpec::new("samples", K::Text, "{samples, recipes}").json(),
];
