//! Looking at a pose and judging a motion: `pose.query`, `clip.metrics`, `clip.critique`
//! (the work is in `crate::posequery`, `crate::metrics` and `crate::critique`).

use serde_json::{Map, Value};

use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::{critique, metrics, posequery};

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

fn clip_critique(doc: &mut Document, p: &P) -> Out {
    let m = measured(doc, p)?;
    let limit = int(p, "limit").unwrap_or(20).clamp(1, 200) as usize;
    let findings = critique::findings(doc, &m, limit);
    Ok(serde_json::json!({ "count": findings.len(), "findings": findings, "metrics": m.json() }))
}
