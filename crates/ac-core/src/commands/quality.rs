//! Looking at a pose and judging a motion: `pose.query`, `clip.metrics`, `clip.critique`,
//! `clip.range`, `clip.possible` (the work is in `crate::posequery`, `crate::metrics`,
//! `crate::critique`, `crate::ranges` and `crate::possible`).

use serde_json::{Map, Value};

use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::{critique, metrics, posequery, possible, ranges};

type P = Map<String, Value>;
type Out = Result<Value, String>;

const FROM: ParamSpec = ParamSpec::new("from", Kind::Integer, "first frame (default 0)").range(0.0, 1e7);
const TO: ParamSpec = ParamSpec::new("to", Kind::Integer, "last frame (default: the last)").range(0.0, 1e7);
const MOTION: ParamSpec = ParamSpec::new(
    "motion",
    Kind::Choice(&["shown", "keys"]),
    "what plays (the physics pass when it is on; default) or the keys alone",
);

const METRICS_OUT: &[ParamSpec] = &[
    ParamSpec::new("from", Kind::Integer, "first frame measured").required(),
    ParamSpec::new("to", Kind::Integer, "last frame measured").required(),
    ParamSpec::new("frames", Kind::Integer, "frames measured").required(),
    ParamSpec::new("motion", Kind::Choice(&["shown", "keys"]), "the motion measured").required(),
    ParamSpec::new(
        "sliding",
        Kind::Text,
        "[{foot: L|R, from, to, slide_cm}] per contact span: how far a planted foot slid",
    )
    .json()
    .required(),
    ParamSpec::new("sliding_max_cm", Kind::Number, "the worst slide").required(),
    ParamSpec::new(
        "penetration",
        Kind::Text,
        "{frames, spans: [[a, b]], max_cm, worst_frame, joint}: below the ground",
    )
    .json()
    .required(),
    ParamSpec::new(
        "balance",
        Kind::Text,
        "{frames, spans, max_cm, worst_frame, moving_frames}: still frames with the centre of mass outside the feet",
    )
    .json()
    .required(),
    ParamSpec::new(
        "capture",
        Kind::Text,
        "{frames, spans, max_cm, worst_frame}: a moving body's capture point outside the feet it has or is about to have",
    )
    .json()
    .required(),
    ParamSpec::new(
        "limits",
        Kind::Text,
        "[{bone, frames, spans, max_deg, worst_frame}] past their joint limits, worst first (null: no limits loaded)",
    )
    .json()
    .required(),
    ParamSpec::new(
        "keys",
        Kind::Text,
        "[{frame, dv, joint}]: at each key, the largest velocity change (m/s) and its joint",
    )
    .json()
    .required(),
    ParamSpec::new("pops", Kind::Integer, "keys whose velocity change is over 1 m/s").required(),
    ParamSpec::new("notes", Kind::TextList, "what was not checked, and why").required(),
];

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("pose.query", "Measure pose", pose_query)
        .icon("ruler")
        .inputs(&[
            ParamSpec::new(
                "frame",
                Kind::Number,
                "a frame of the motion shown (default: the pose shown now)",
            )
            .range(0.0, 1e7),
            ParamSpec::new("bones", Kind::TextList, "bones to report"),
            ParamSpec::new(
                "pairs",
                Kind::Text,
                "[[bone, bone] or [bone, \"floor\"], ...]: distances",
            )
            .json(),
        ])
        .outputs(&[
            ParamSpec::new("frame", Kind::Number, "the frame asked for (null: the pose shown)").required(),
            ParamSpec::new(
                "bones",
                Kind::Text,
                "[{bone, position, rotation, local, height, past_limits_deg}] (metres; world and local [x, y, z, w])",
            )
            .json()
            .required(),
            ParamSpec::new("pairs", Kind::Text, "[{a, b, distance, delta}] (metres)")
                .json()
                .required(),
            ParamSpec::new(
                "limbs",
                Kind::Text,
                "[{limb, distance, length, reach, straight, bend_deg}] per arm and leg",
            )
            .json()
            .required(),
            ParamSpec::new(
                "outside_limits",
                Kind::Text,
                "[{bone, past_deg}] worst first (null: no limits loaded)",
            )
            .json()
            .required(),
            ParamSpec::new("notes", Kind::TextList, "what was not checked").required(),
        ]),
    CommandSpec::new("clip.metrics", "Motion metrics", clip_metrics)
        .icon("graph")
        .inputs(&[FROM, TO, MOTION])
        .outputs(METRICS_OUT),
    CommandSpec::new("feet.phases", "Foot phases", feet_phases)
        .icon("graph")
        .inputs(&[MOTION])
        .outputs(&[
            ParamSpec::new("L", Kind::TextList, "the left foot on each frame: swing, landing, planted or release")
                .required(),
            ParamSpec::new("R", Kind::TextList, "the right foot, the same").required(),
        ]),
    CommandSpec::new("clip.critique", "Review animation", clip_critique)
        .icon("review")
        .inputs(&[
            FROM,
            TO,
            MOTION,
            ParamSpec::new("limit", Kind::Integer, "most findings (default 20)").range(1.0, 200.0),
        ])
        .outputs(&[
            ParamSpec::new(
                "findings",
                Kind::Text,
                "[{kind, severity: high|medium|low, score, frames: [a, b], message, command, params, batch: [{id, params}]}] worst first",
            )
            .json()
            .required(),
            ParamSpec::new("count", Kind::Integer, "findings returned").required(),
            ParamSpec::new("metrics", Kind::Text, "the measurements (clip.metrics)")
                .json()
                .required(),
        ]),
    CommandSpec::new("clip.range", "Joint range of motion", clip_range)
        .icon("graph")
        .inputs(&[
            ParamSpec::new(
                "pair",
                Kind::Choice(ranges::PAIR_IDS),
                "coupled joint pair (default: hip_knee)",
            ),
            ParamSpec::new(
                "side",
                Kind::Choice(&["L", "R"]),
                "left or right side on sided pairs (default: L)",
            ),
            FROM,
            TO,
            MOTION,
        ])
        .outputs(&[
            ParamSpec::new("pair", Kind::Text, "the pair id").required(),
            ParamSpec::new("label", Kind::Text, "the pair's readable name").required(),
            ParamSpec::new("sided", Kind::Bool, "whether the pair has left and right sides").required(),
            ParamSpec::new("side", Kind::Text, "\"L\" or \"R\" (null on trunk_bend and neck_bend)").required(),
            ParamSpec::new("note", Kind::Text, "anatomical coupling note").required(),
            ParamSpec::new(
                "limits_note",
                Kind::Text,
                "which source each clinical limit comes from, and what joining between them is ours",
            )
            .required(),
            ParamSpec::new("from", Kind::Integer, "first frame measured").required(),
            ParamSpec::new("to", Kind::Integer, "last frame measured").required(),
            ParamSpec::new("frames", Kind::Integer, "frames measured").required(),
            ParamSpec::new("motion", Kind::Choice(&["shown", "keys"]), "the motion measured").required(),
            ParamSpec::new(
                "x",
                Kind::Text,
                "{joint, component, label, hard: [lo, hi] or null} of the horizontal angle (degrees)",
            )
                .json()
                .required(),
            ParamSpec::new("y", Kind::Text, "{joint, component, label, hard: null} of the vertical angle")
                .json()
                .required(),
            ParamSpec::new("hard", Kind::Text, "[{x, lo, hi}] clinical hard limit band (degrees)")
                .json()
                .required(),
            ParamSpec::new(
                "typical",
                Kind::Text,
                "[{x, lo, hi, count}] 1st-99th percentile mocap envelope (degrees)",
            )
            .json()
            .required(),
            ParamSpec::new(
                "playhead",
                Kind::Text,
                "{frame, x, y, hard_lo, hard_hi, typical_lo, typical_hi, closeness, past_hard_deg, past_typical_deg} of the pose shown",
            )
            .json()
            .required(),
            ParamSpec::new(
                "path",
                Kind::Text,
                "per frame: [{frame, x, y, hard_lo, hard_hi, typical_lo, typical_hi, closeness, typical_closeness, past_hard_deg, past_typical_deg}]",
            )
            .json()
            .required(),
            ParamSpec::new(
                "max_closeness",
                Kind::Number,
                "highest closeness to the hard limit across the measured frames (1.0 = on the edge, > 1.0 = past it)",
            )
            .required(),
            ParamSpec::new(
                "outside_hard",
                Kind::Text,
                "{frames, spans: [[a, b]], max_deg, worst_frame} past the clinical hard limit",
            )
            .json()
            .required(),
            ParamSpec::new(
                "outside_typical",
                Kind::Text,
                "{frames, spans: [[a, b]], max_deg, worst_frame} outside the mocap typical band",
            )
            .json()
            .required(),
            ParamSpec::new(
                "summary",
                Kind::Text,
                "every pair and side: [{pair, label, side, max_closeness, peak_frame, outside_hard_frames, outside_hard_spans, max_past_hard_deg, worst_frame}]",
            )
            .json()
            .required(),
            ParamSpec::new(
                "sources",
                Kind::Text,
                "[{id, title, url, note}] clinical and mocap reference sources",
            )
            .json()
            .required(),
            ParamSpec::new("notes", Kind::TextList, "what was not checked, and why").required(),
        ]),
    CommandSpec::new("clip.possible", "Impossible poses", clip_possible)
        .icon("graph")
        .inputs(&[FROM, TO, MOTION])
        .outputs(&[
            ParamSpec::new("from", Kind::Integer, "first frame measured").required(),
            ParamSpec::new("to", Kind::Integer, "last frame measured").required(),
            ParamSpec::new("frames", Kind::Integer, "frames measured").required(),
            ParamSpec::new("motion", Kind::Choice(&["shown", "keys"]), "the motion measured").required(),
            ParamSpec::new(
                "level",
                Kind::Choice(possible::LEVELS),
                "the worst over the frames: typical (inside the clinical limits), unusual (past them but realistic: athletic) or impossible",
            )
            .required(),
            ParamSpec::new("message", Kind::Text, "what the level means").required(),
            ParamSpec::new(
                "impossible",
                Kind::Text,
                "{frames, spans: [[a, b]], max_deg, worst_frame}: frames with a joint past its possible limit",
            )
            .json()
            .required(),
            ParamSpec::new(
                "breaches",
                Kind::Text,
                "per joint angle past its possible limit, worst first: [{joint, component, value_deg, lo, hi, past_deg, frames, spans, worst_frame}] (value and past at the worst frame)",
            )
            .json()
            .required(),
            ParamSpec::new(
                "unusual",
                Kind::Text,
                "{frames, spans, max_deg, worst_frame}: frames past a clinical limit (any range pair and side)",
            )
            .json()
            .required(),
            ParamSpec::new("possible_note", Kind::Text, "where the possible limits come from").required(),
            ParamSpec::new("notes", Kind::TextList, "what was not checked, and why").required(),
        ]),
];

fn int(p: &P, k: &str) -> Option<i64> {
    p.get(k).and_then(Value::as_f64).map(|x| x as i64)
}

fn pose_query(doc: &mut Document, p: &P) -> Out {
    let pairs = posequery::pairs(doc, p.get("pairs"))?;
    posequery::query(
        doc,
        p.get("frame").and_then(Value::as_f64),
        &cmd::texts(p, "bones"),
        &pairs,
    )
}

fn measured(doc: &Document, p: &P) -> Result<metrics::Metrics, String> {
    let shown = cmd::text(p, "motion") != Some("keys");
    metrics::measure(doc, int(p, "from"), int(p, "to"), shown)
}

fn clip_metrics(doc: &mut Document, p: &P) -> Out {
    Ok(measured(doc, p)?.json())
}

fn feet_phases(doc: &mut Document, p: &P) -> Out {
    let shown = cmd::text(p, "motion") != Some("keys");
    let [left, right] = metrics::foot_phases(doc, shown)?;
    let names = |v: Vec<crate::contact::Phase>| v.into_iter().map(|x| x.name()).collect::<Vec<_>>();
    Ok(serde_json::json!({ "L": names(left), "R": names(right) }))
}

fn clip_critique(doc: &mut Document, p: &P) -> Out {
    let m = measured(doc, p)?;
    let limit = int(p, "limit").unwrap_or(20).clamp(1, 200) as usize;
    let findings = critique::findings(doc, &m, limit);
    Ok(serde_json::json!({ "count": findings.len(), "findings": findings, "metrics": m.json() }))
}

fn clip_possible(doc: &mut Document, p: &P) -> Out {
    let shown = cmd::text(p, "motion") != Some("keys");
    possible::measure(doc, int(p, "from"), int(p, "to"), shown)
}

fn clip_range(doc: &mut Document, p: &P) -> Out {
    let shown = cmd::text(p, "motion") != Some("keys");
    ranges::measure(
        doc,
        cmd::text(p, "pair"),
        cmd::text(p, "side"),
        int(p, "from"),
        int(p, "to"),
        shown,
    )
}
