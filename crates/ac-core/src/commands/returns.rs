//! What the commands of `keys.rs` and `clipquery.rs` return: their declared outputs, kept apart so
//! the command tables stay short. A const may serve several commands with the same result.

use crate::command::{Kind, ParamSpec};

pub const FRAME_SET: &[ParamSpec] = &[ParamSpec::new("frame", Kind::Number, "the playhead").required()];

pub const KEY_SET: &[ParamSpec] = &[
    ParamSpec::new("frame", Kind::Integer, "the frame keyed").required(),
    ParamSpec::new(
        "notes",
        Kind::TextList,
        "spans next to the key that were left as they were",
    ),
];

pub const KEY_MOVE: &[ParamSpec] = &[ParamSpec::new(
    "notes",
    Kind::TextList,
    "what else changed (AI spans refilled, keys merged)",
)
.required()];

pub const KEY_COPY: &[ParamSpec] =
    &[ParamSpec::new("notes", Kind::TextList, "what else changed (AI spans refilled)").required()];

pub const POSE_DRAG: &[ParamSpec] =
    &[ParamSpec::new("position", Kind::Vec3, "the bone's head after the drag (metres)").required()];

pub const POSE_REACH: &[ParamSpec] = &[
    ParamSpec::new("position", Kind::Vec3, "the bone's head after the solve (metres)").required(),
    ParamSpec::new("error", Kind::Number, "metres left to the target").required(),
    ParamSpec::new("reached", Kind::Bool, "within 0.1 mm").required(),
];

pub const POSE_SPREAD_BEND: &[ParamSpec] =
    &[ParamSpec::new("degrees", Kind::Number, "the lean moved into the back").required()];

pub const POSE_COPY: &[ParamSpec] = &[ParamSpec::new("copied", Kind::Text, "the parts copied").required()];

pub const PIN_SET: &[ParamSpec] = &[ParamSpec::new("pins", Kind::TextList, "the pinned bones").required()];

pub const PARTS_LOCK: &[ParamSpec] =
    &[ParamSpec::new("locked", Kind::TextList, "the locked parts").required()];

pub const FEET_LOCK: &[ParamSpec] = &[ParamSpec::new("frames", Kind::Integer, "frames changed").required()];

pub const AI_KEEP: &[ParamSpec] = &[ParamSpec::new("kept", Kind::Integer, "AI keys made yours").required()];

pub const AI_CLEAR: &[ParamSpec] = &[ParamSpec::new("cleared", Kind::Integer, "AI keys removed").required()];

pub const AI_FILL: &[ParamSpec] = &[
    ParamSpec::new("from", Kind::Integer, "your key before the gap").required(),
    ParamSpec::new("to", Kind::Integer, "your key after the gap").required(),
    ParamSpec::new("frames", Kind::Integer, "frames filled").required(),
    ParamSpec::new("model", Kind::Text, "the model label").required(),
    ParamSpec::new(
        "notes",
        Kind::TextList,
        "impossible poses the fill made and bent back to the possible limit, or left (empty: none)",
    )
    .required(),
];

pub const AI_SETTINGS: &[ParamSpec] = &[
    ParamSpec::new("style", Kind::Text, "the style").required(),
    ParamSpec::new("style_force", Kind::Number, "style guidance").required(),
    ParamSpec::new(
        "ai_force",
        Kind::Number,
        "how much of the network's change is kept",
    )
    .required(),
    ParamSpec::new("trajectory", Kind::Number, "pull onto the keys' path").required(),
    ParamSpec::new("head", Kind::Number, "neck and head on the keys' spline").required(),
];

pub const AI_INFO: &[ParamSpec] = &[
    ParamSpec::new("model", Kind::Text, "the model card (null: none loaded)")
        .json()
        .required(),
    ParamSpec::new("settings", Kind::Text, "the fill settings")
        .json()
        .required(),
];

pub const AI_PREVIEW: &[ParamSpec] = &[
    ParamSpec::new("from", Kind::Integer, "your key before the gap").required(),
    ParamSpec::new("to", Kind::Integer, "your key after the gap").required(),
    ParamSpec::new("frames", Kind::Integer, "frames filled").required(),
    ParamSpec::new("model", Kind::Text, "the model label, or plain interpolation").required(),
    ParamSpec::new(
        "mode",
        Kind::Choice(&["ai", "plain"]),
        "the model or plain interpolation",
    )
    .required(),
    ParamSpec::new(
        "root_heads",
        Kind::Vec3,
        "the root head on every frame of the span (metres)",
    )
    .list()
    .required(),
    ParamSpec::new(
        "notes",
        Kind::TextList,
        "impossible poses the fill made and bent back to the possible limit, or left (empty: none)",
    )
    .required(),
];

pub const AI_DISCARD: &[ParamSpec] =
    &[ParamSpec::new("discarded", Kind::Bool, "there was a preview").required()];

pub const EDITOR_STATE: &[ParamSpec] = &[
    ParamSpec::new("frame", Kind::Number, "the playhead").required(),
    ParamSpec::new("length", Kind::Integer, "frames").required(),
    ParamSpec::new("fps", Kind::Number, "frames per second").required(),
    ParamSpec::new("loop", Kind::Bool, "the animation loops").required(),
    ParamSpec::new("name", Kind::Text, "its name").required(),
    ParamSpec::new("group", Kind::Text, "its group").required(),
    ParamSpec::new("id", Kind::Text, "its library id (null: not saved)").required(),
    ParamSpec::new("unsaved", Kind::Bool, "changes the library lacks").required(),
    ParamSpec::new(
        "keys",
        Kind::Text,
        "[{frame, interp, source, model, part?, rows?}] of the edited layer",
    )
    .json()
    .required(),
    ParamSpec::new("layers", Kind::Text, "[{name, keys, muted, weight, link}]")
        .json()
        .required(),
    ParamSpec::new("edit_layer", Kind::Integer, "the layer keys go on").required(),
    ParamSpec::new("locked", Kind::TextList, "locked parts").required(),
    ParamSpec::new("pins", Kind::TextList, "pinned bones").required(),
    ParamSpec::new("bands", Kind::Text, "AI bands [{from, to, model}]")
        .json()
        .required(),
    ParamSpec::new("clipboard", Kind::Text, "the parts copied (null: nothing)").required(),
    ParamSpec::new("part_presets", Kind::Text, "the body part presets")
        .json()
        .required(),
    ParamSpec::new("part_rows", Kind::Text, "the dope sheet rows")
        .json()
        .required(),
    ParamSpec::new("physics", Kind::Bool, "the physics pass plays").required(),
    ParamSpec::new(
        "preview",
        Kind::Text,
        "{from, to, model, plain, live} of the AI preview (null: none)",
    )
    .json()
    .required(),
    ParamSpec::new("markers", Kind::Text, "timeline markers [{frame, name}] by frame")
        .json()
        .required(),
];

pub const CLIP_CURVES: &[ParamSpec] = &[
    ParamSpec::new("frames", Kind::Text, "per frame, the curve values")
        .json()
        .required(),
    ParamSpec::new("keys", Kind::Text, "[{frame, values}]")
        .json()
        .required(),
];

pub const CLIP_CONTACTS: &[ParamSpec] = &[
    ParamSpec::new("L", Kind::Bool, "per frame: the left foot is planted")
        .list()
        .required(),
    ParamSpec::new("R", Kind::Bool, "per frame: the right foot is planted")
        .list()
        .required(),
];

pub const CLIP_SAMPLE: &[ParamSpec] = &[
    ParamSpec::new("frames", Kind::Number, "the frames sampled")
        .list()
        .required(),
    ParamSpec::new("rotations", Kind::Text, "per frame, one [x, y, z, w] per bone")
        .json()
        .required(),
    ParamSpec::new("roots", Kind::Vec3, "per frame, the root offset (hip heights)")
        .list()
        .required(),
];

pub const AI_GAPS: &[ParamSpec] = &[
    ParamSpec::new(
        "gaps",
        Kind::Text,
        "every gap between your keys: [{from, to, frames, filled, refused}] (refused: why the model can't fill it, null: it can)",
    )
    .json()
    .required(),
    ParamSpec::new("at", Kind::Text, "the gap the playhead is in (null: none)")
        .json()
        .required(),
    ParamSpec::new(
        "max_gap",
        Kind::Integer,
        "most frames the model fills in one gap (null: no model loaded)",
    )
    .required(),
];

pub const AI_FILL_ALL: &[ParamSpec] = &[
    ParamSpec::new("filled", Kind::Text, "the gaps filled: [[from, to]]")
        .json()
        .required(),
    ParamSpec::new("skipped", Kind::Text, "the gaps left: [{from, to, reason}]")
        .json()
        .required(),
    ParamSpec::new("model", Kind::Text, "the model label").required(),
    ParamSpec::new(
        "notes",
        Kind::TextList,
        "impossible poses the fills made and bent back to the possible limit, or left (each starts with its gap, from–to; empty: none)",
    )
    .required(),
];

pub const AI_TOGGLE: &[ParamSpec] = &[
    ParamSpec::new("from", Kind::Integer, "your key before the gap").required(),
    ParamSpec::new("to", Kind::Integer, "your key after the gap").required(),
    ParamSpec::new("mode", Kind::Choice(&["ai", "plain"]), "what the span is now").required(),
    ParamSpec::new("cleared", Kind::Integer, "plain: AI keys removed"),
    ParamSpec::new("model", Kind::Text, "ai: the model label"),
    ParamSpec::new(
        "notes",
        Kind::TextList,
        "ai: impossible poses the fill made and bent back, or left",
    ),
];
