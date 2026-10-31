//! What the commands of this crate return: their declared outputs, kept apart so the command
//! tables stay short. A const may serve several commands with the same result.

use ac_core::command::{Kind as K, ParamSpec};

pub const LIBRARY_LIST: &[ParamSpec] = &[
    ParamSpec::new(
        "animations",
        K::Text,
        "[{id, name, group, current, builtin, edited, favourite, frames, fps, seconds, loop, modified, ...}]",
    )
    .json()
    .required(),
    ParamSpec::new(
        "poses",
        K::Text,
        "[{id, name, group, part, builtin, edited, favourite, modified, ...}]",
    )
    .json()
    .required(),
    ParamSpec::new("groups", K::TextList, "every group").required(),
    ParamSpec::new("custom_parts", K::Text, "your part sets")
        .json()
        .required(),
];

pub const LIBRARY_FIND: &[ParamSpec] = &[
    ParamSpec::new(
        "items",
        K::Text,
        "the items found, as library.list lists them, in order",
    )
    .json()
    .required(),
    ParamSpec::new("count", K::Integer, "how many").required(),
    ParamSpec::new("groups", K::Text, "[{group, count}] of the items found, in order")
        .json()
        .required(),
];

pub const LIBRARY_FAVOURITE: &[ParamSpec] = &[
    ParamSpec::new("id", K::Text, "the item").required(),
    ParamSpec::new("favourite", K::Bool, "whether it is starred now").required(),
];

pub const LIBRARY_INSTALL: &[ParamSpec] = &[ParamSpec::new("copied", K::Integer, "files copied").required()];

pub const LIBRARY_SYNC: &[ParamSpec] = &[
    ParamSpec::new(
        "animations",
        K::Text,
        "{added, updated, retired: [id], moved: [id], kept: [{id, why}]}: new built-ins copied in, unchanged copies replaced by a fixed one, ones no longer shipped moved to <library>/.retired (or, when they moved to the community, kept in place as community installs), and ones you changed left as they are",
    )
    .json()
    .required(),
    ParamSpec::new("poses", K::Text, "the same for poses").json().required(),
    ParamSpec::new("changed", K::Integer, "items added, updated, retired or moved").required(),
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

pub const CLIP_IMPORT: &[ParamSpec] = &[
    ParamSpec::new("length", K::Integer, "frames").required(),
    ParamSpec::new("fps", K::Number, "an .fbx: the file's frame rate"),
    ParamSpec::new("frames", K::Integer, "an .fbx: frames read (setup frames dropped)"),
    ParamSpec::new("joints", K::Text, "an .fbx: {landmark: the file's joint that stood for it}").json(),
    ParamSpec::new("bone_map", K::Text, "an .fbx: what said which joint is which: unity_meta (the .meta beside it) or names (guessed)"),
    ParamSpec::new("clip", K::Text, "an .fbx with named clips: {name, first, last, loop, take} of the one opened").json(),
    ParamSpec::new("clips", K::Text, "an .fbx with named clips: [{name, first, last, loop}] of the file").json(),
    ParamSpec::new("given", K::TextList, "an .fbx: the landmarks the joints parameter set"),
    ParamSpec::new("unused", K::TextList, "an .fbx: the file's joints with no landmark (fingers, face, props, ends)"),
    ParamSpec::new("at_rest", K::TextList, "an .fbx: rig bone families left at rest (\"finger: 30 of 30 at rest\")"),
    ParamSpec::new("scale", K::Number, "an .fbx: the factor that made the source's legs the rig's"),
    ParamSpec::new("floor", K::Number, "an .fbx: how far the floor was moved to z = 0 (metres; 0: left as it was)"),
    ParamSpec::new("hips", K::Text, "an .fbx: {mid, left, right}: how far the rig's hip joints land from the source's on the worst frame (metres)").json(),
    ParamSpec::new("notes", K::TextList, "an .fbx: what was guessed, dropped or left at rest"),
    ParamSpec::new("source", K::Text, "an .fbx: {version, up_axis, front_axis, unit_scale, fps_from, take, joints, frames, dropped}").json(),
];

pub const CLIP_IMPORT_BATCH: &[ParamSpec] = &[
    ParamSpec::new("folder", K::Text, "the folder the clips went in").required(),
    ParamSpec::new("files", K::Text, "[{file, path, frames, fps, notes}] written; a file a .meta cuts into named clips gives one each, with clip and loop")
        .json()
        .required(),
    ParamSpec::new("failed", K::Text, "[{file, error}] not imported; with clip when one named clip of the file was refused")
        .json()
        .required(),
    ParamSpec::new(
        "cut",
        K::Text,
        "set when more files were found than one batch takes",
    ),
];

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

pub const LAYERS_LIST: &[ParamSpec] = &[ParamSpec::new("layers", K::Text, "[{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}]").json().required(), ParamSpec::new("edit", K::Integer, "the layer keys go on").required(), ParamSpec::new("surfaces", K::Text, "[{name, min, max, top}]").json().required(), ParamSpec::new("contacts", K::Text, "[{bone, from, to, at}]: the hands and feet held").json().required(), ParamSpec::new("physics", K::Bool, "the physics pass plays").required()];

pub const LAYER_TRAVEL: &[ParamSpec] = &[ParamSpec::new("layers", K::Text, "[{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}]").json().required(), ParamSpec::new("edit", K::Integer, "the layer keys go on").required(), ParamSpec::new("surfaces", K::Text, "[{name, min, max, top}]").json().required(), ParamSpec::new("contacts", K::Text, "[{bone, from, to, at}]: the hands and feet held").json().required(), ParamSpec::new("physics", K::Bool, "the physics pass plays").required(), ParamSpec::new("i", K::Integer, "the travel layer").required()];

pub const LAYER_BAKE: &[ParamSpec] = &[ParamSpec::new("layers", K::Text, "[{i, name, source, link, part, mode, weight, offset, speed, loop, muted, keys, edit, start, end, fade, gait, waypoints, moves, steps, missing}]").json().required(), ParamSpec::new("edit", K::Integer, "the layer keys go on").required(), ParamSpec::new("surfaces", K::Text, "[{name, min, max, top}]").json().required(), ParamSpec::new("contacts", K::Text, "[{bone, from, to, at}]: the hands and feet held").json().required(), ParamSpec::new("physics", K::Bool, "the physics pass plays").required(), ParamSpec::new("baked", K::Integer, "keys written").required()];

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
    ParamSpec::new("joints", K::Integer, "joints exported (with a target: nodes driven)").required(),
    ParamSpec::new("animations", K::Integer, "animations in the file").required(),
    ParamSpec::new("frames", K::IntList, "frames per animation").required(),
    ParamSpec::new("path", K::Text, "the file written").required(),
    ParamSpec::new(
        "target",
        K::Text,
        "with a target character: {character, driven, undriven: [node], scale, feet: [{clip, L: {planted_frames, slide_cm, travel_cm, lowest_cm, unreached_frames, holds}, R}], problems: [[{from, to, kind, label}]], notes}",
    )
    .json(),
];

pub const RETARGET_LOAD: &[ParamSpec] = &[
    ParamSpec::new("character", K::Text, "the character file, as given").required(),
    ParamSpec::new("nodes", K::Integer, "nodes in the file").required(),
    ParamSpec::new("skin_joints", K::Integer, "joints of its skin (0: no skin)").required(),
    ParamSpec::new("mesh", K::Bool, "a mesh is skinned to it").required(),
    ParamSpec::new("joints", K::Text, "{landmark: the node that stands for it}")
        .json()
        .required(),
    ParamSpec::new("given", K::TextList, "the landmarks the joints parameter set").required(),
    ParamSpec::new("hands", K::Bool, "the arms are solved onto the wrists").required(),
    ParamSpec::new("missing", K::TextList, "required landmarks with no node yet").required(),
    ParamSpec::new(
        "ready",
        K::Bool,
        "the mapping is complete and the character can be checked and exported",
    )
    .required(),
    ParamSpec::new("driven", K::Integer, "nodes the rig drives").required(),
    ParamSpec::new(
        "undriven",
        K::TextList,
        "nodes under the hips nothing drives (they keep their rest)",
    )
    .required(),
    ParamSpec::new(
        "outside",
        K::TextList,
        "nodes not under the hips (left as they are)",
    )
    .required(),
    ParamSpec::new(
        "scale",
        K::Number,
        "the character's legs over the rig's (null: not ready)",
    ),
    ParamSpec::new(
        "floor",
        K::Number,
        "the character's floor height (null: not ready)",
    ),
    ParamSpec::new(
        "source",
        K::Text,
        "{nodes, skin_joints, meshes, animations} of the file",
    )
    .json(),
    ParamSpec::new("notes", K::TextList, "what was guessed, left out or is missing").required(),
];

pub const RETARGET_CHECK: &[ParamSpec] = &[
    ParamSpec::new("frames", K::Integer, "frames driven").required(),
    ParamSpec::new("scale", K::Number, "the character's legs over the rig's").required(),
    ParamSpec::new("driven", K::Integer, "nodes the rig drives").required(),
    ParamSpec::new("undriven", K::TextList, "nodes under the hips nothing drives").required(),
    ParamSpec::new(
        "feet",
        K::Text,
        "{L, R: {planted_frames, spans: [{from, to, slide_cm}], slide_cm, travel_cm, lowest_cm, unreached_frames, unreached_cm, holds}}: how far a planted foot drifts from where it came down (slide), its per-frame travel added up (travel), the lowest it got above the character's floor, frames its leg could not reach, and whether it holds (drift under 1 cm, never under the floor, always reached)",
    )
    .json()
    .required(),
    ParamSpec::new("problems", K::Text, "[{from, to, kind: foot-slide | below-floor | unreached | undriven, label}]")
        .json()
        .required(),
    ParamSpec::new("notes", K::TextList, "the plan's notes").required(),
];

pub const RETARGET_SAVE: &[ParamSpec] = &[
    ParamSpec::new("path", K::Text, "the file written").required(),
    ParamSpec::new("bytes", K::Integer, "its size").required(),
    ParamSpec::new(
        "hash",
        K::Text,
        "a hash of its bytes (the same mapping gives the same file)",
    )
    .required(),
    ParamSpec::new("character", K::Text, "the character file it names").required(),
];

pub const RETARGET_UNLOAD: &[ParamSpec] = &[
    ParamSpec::new("unloaded", K::Bool, "the character is out of the document").required(),
    ParamSpec::new("character", K::Text, "the character file that was loaded").required(),
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

pub const PROJECT_SAVE: &[ParamSpec] = &[
    ParamSpec::new("path", K::Text, "the file written").required(),
    ParamSpec::new("bytes", K::Integer, "its size").required(),
    ParamSpec::new(
        "hash",
        K::Text,
        "FNV-1a of its bytes (the same document gives the same hash)",
    )
    .required(),
    ParamSpec::new("links", K::TextList, "the linked clips embedded, by library id").required(),
    ParamSpec::new(
        "clips",
        K::Integer,
        "clips in the file (the animation and its links)",
    )
    .required(),
];

pub const PROJECT_OPEN: &[ParamSpec] = &[
    ParamSpec::new("path", K::Text, "the file opened").required(),
    ParamSpec::new("name", K::Text, "the animation's name").required(),
    ParamSpec::new("length", K::Integer, "frames").required(),
    ParamSpec::new("frame", K::Number, "the playhead, as saved").required(),
    ParamSpec::new("links", K::TextList, "the linked clips it carries, by library id").required(),
    ParamSpec::new("layers", K::Integer, "layers, the base included").required(),
    ParamSpec::new("boxes", K::Integer, "scene boxes").required(),
    ParamSpec::new("version", K::Integer, "the version the file was written with").required(),
    ParamSpec::new(
        "migrated",
        K::Bool,
        "an older version, brought up to date on opening",
    )
    .required(),
    ParamSpec::new(
        "notes",
        K::TextList,
        "what the open session can't honour (an unknown AI style)",
    )
    .required(),
];

pub const PROJECT_INFO: &[ParamSpec] = &[
    ParamSpec::new("path", K::Text, "the file read").required(),
    ParamSpec::new("version", K::Integer, "the version it was written with").required(),
    ParamSpec::new(
        "current",
        K::Bool,
        "whether that is the version this build writes",
    )
    .required(),
    ParamSpec::new("name", K::Text, "the animation's name").required(),
    ParamSpec::new("group", K::Text, "its group").required(),
    ParamSpec::new("fps", K::Number, "frames per second").required(),
    ParamSpec::new("frames", K::Integer, "its length").required(),
    ParamSpec::new("layers", K::Integer, "layers, the base included").required(),
    ParamSpec::new("boxes", K::Integer, "scene boxes").required(),
    ParamSpec::new("links", K::TextList, "the linked clips it carries, by library id").required(),
    ParamSpec::new("clips", K::Integer, "clips in the file").required(),
    ParamSpec::new("bytes", K::Integer, "its size").required(),
    ParamSpec::new("hash", K::Text, "FNV-1a of its bytes").required(),
];
