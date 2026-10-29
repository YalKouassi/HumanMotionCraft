//! Footsteps between two keys (`anim.steps`). By default the walk is the built-in Walk loop played
//! step by step between the footfalls, each foot planted where it lands ([`crate::gait`]). The other
//! fills set only the step keys an animator would, a heel-down key per step (and the first knee
//! lifted), for the in-betweener: a model given only the two standing ends can't tell which foot
//! leads, blends the steps and the feet glide.

use serde_json::{Map, Value, json};

use crate::clip::{Interp, Key};
use crate::command::{self as cmd, CommandSpec, Kind, ParamSpec};
use crate::document::Document;
use crate::gait;
use crate::quat::{self, Quat, Vec3};

/// The stride the default step count aims at (m)...
const STRIDE: f64 = 0.55;
/// ...and the frames a step (30 fps): the default is halfway between the two.
const STEP_FRAMES: f64 = 15.0;
/// A stride longer than this is a lunge, not a walk: said in the notes.
const LONG_STRIDE: f64 = 0.9;
/// Frames a step takes at the least.
const MIN_STEP_FRAMES: i64 = 4;
/// At a contact the hips sit this much lower than at the ends (m).
const CONTACT_DIP: f64 = 0.03;
/// At a contact the back foot's heel is up this much (m).
const HEEL_UP: f64 = 0.04;
/// The first step's foot is this high when it passes the other (m).
const KNEE_UP: f64 = 0.12;
/// The keys must be at least this far apart for footsteps (m).
const MIN_TRAVEL: f64 = 0.1;
/// A walk loop step quicker than this many frames (at 30 fps) is a scurry: said in the notes.
const QUICK_STEP: f64 = 10.0;
/// A cycle walk keys every this many frames (as the Walk loop does)...
const KEY_EVERY: i64 = 2;
/// ...and every frame this close to a footfall, where the foot rolls flat fastest.
const KEY_NEAR: std::ops::RangeInclusive<i64> = -1..=2;

pub static SPECS: &[CommandSpec] = &[CommandSpec::new("anim.steps", "Footsteps", anim_steps)
    .icon("foot")
    .undoable()
    .inputs(&[
        ParamSpec::new(
            "from",
            Kind::Integer,
            "your key the walk starts on (with `to`; default: the keys around the playhead)",
        )
        .range(0.0, 10_000_000.0),
        ParamSpec::new("to", Kind::Integer, "your key the walk ends on").range(0.0, 10_000_000.0),
        ParamSpec::new(
            "steps",
            Kind::Integer,
            "footfalls, the last one bringing the feet together on `to` (default: between a step every 0.55 m and every 15 frames)",
        )
        .range(2.0, 64.0),
        ParamSpec::new(
            "lead",
            Kind::Choice(&["L", "R"]),
            "the foot that steps first (default R)",
        ),
        ParamSpec::new(
            "fill",
            Kind::Choice(&["cycle", "ai", "plain", "none"]),
            "how it walks: the built-in Walk loop with planted feet (default, a key every 2 frames), or only step keys filled by the model, plain interpolation, or the keys' own curve",
        ),
        ParamSpec::new("style", Kind::Text, "the AI style for the fills (default walk)"),
    ])
    .outputs(&[
        ParamSpec::new("from", Kind::Integer, "the walk's first key").required(),
        ParamSpec::new("to", Kind::Integer, "its last key").required(),
        ParamSpec::new("keys", Kind::IntList, "the keys set, in order").required(),
        ParamSpec::new(
            "contacts",
            Kind::IntList,
            "the footfalls between the two keys (a foot lands on each)",
        )
        .required(),
        ParamSpec::new("steps", Kind::Integer, "footfalls").required(),
        ParamSpec::new("stride", Kind::Number, "metres from one footfall to the next").required(),
        ParamSpec::new("lead", Kind::Text, "the foot that steps first").required(),
        ParamSpec::new("fills", Kind::Text, "[{from, to, model}]")
            .json()
            .required(),
        ParamSpec::new("notes", Kind::TextList, "what the user should know").required(),
    ])];

fn int(p: &Map<String, Value>, name: &str) -> Option<i64> {
    p.get(name).and_then(Value::as_i64)
}

/// One foot's job on a step key: where its ankle goes.
struct Plant {
    side: &'static str,
    at: Vec3,
}

fn anim_steps(doc: &mut Document, p: &Map<String, Value>) -> Result<Value, String> {
    if doc.clip.edit != 0 {
        return Err(crate::error::hinted(
            "footsteps go on the base layer",
            "layer.edit {\"i\": 0} first, then anim.steps",
        ));
    }
    let (a, b) = match (int(p, "from"), int(p, "to")) {
        (Some(a), Some(b)) => (a, b),
        (None, None) => crate::edits::span_around(&doc.clip, doc.key_frame()).ok_or(
            "footsteps go between two of your keys: put the playhead between them, or give from and to",
        )?,
        _ => return Err("give both `from` and `to`, or neither".into()),
    };
    let mine = crate::edits::manual_frames(&doc.clip);
    if !mine.contains(&a) || !mine.contains(&b) || a >= b {
        return Err(format!(
            "{a} and {b} must be two of your keys, the first one earlier"
        ));
    }
    if let Some(f) = mine.iter().find(|&&f| a < f && f < b) {
        return Err(format!(
            "your key on frame {f} is between {a} and {b}: footsteps fill a gap between neighbouring keys"
        ));
    }
    let (lead, other) = if cmd::text(p, "lead") == Some("L") {
        ("L", "R")
    } else {
        ("R", "L")
    };
    let fill = cmd::text(p, "fill").unwrap_or("cycle");
    let style = cmd::text(p, "style").unwrap_or("walk");
    if fill == "ai" {
        let known = doc
            .ai
            .as_ref()
            .map(|m| m.info().styles.clone())
            .unwrap_or_default();
        if style != "any" && !known.iter().any(|k| k == style) {
            return Err(format!("style {style:?}: the model knows {known:?}"));
        }
    }

    // the two ends, as the keys have them
    let sk = doc.skeleton.clone();
    let (rots, roots) = doc.clip.sample_with(&[a as f64, b as f64], &doc.links)?;
    let (Some(ra), Some(rb), Some(&oa), Some(&ob)) = (rots.first(), rots.get(1), roots.first(), roots.get(1))
    else {
        return Err("the keys could not be sampled".into());
    };
    let (_, ja) = sk.fk(ra, oa).map_err(|e| e.to_string())?;
    let (_, jb) = sk.fk(rb, ob).map_err(|e| e.to_string())?;
    let foot = |side: &str| {
        sk.bone(&format!("foot.{side}"))
            .ok_or_else(|| format!("the rig has no foot.{side}"))
    };
    let (il, io) = (foot(lead)?, foot(other)?);
    let get = |j: &[Vec3], i: usize| j.get(i).copied().ok_or("a joint is missing");
    let (lead_a, lead_b, other_a, other_b) = (get(&ja, il)?, get(&jb, il)?, get(&ja, io)?, get(&jb, io)?);
    let (root_a, root_b) = (get(&ja, 0)?, get(&jb, 0)?);
    let travel = ((root_b[0] - root_a[0]).powi(2) + (root_b[1] - root_a[1]).powi(2)).sqrt();
    if travel < MIN_TRAVEL {
        return Err(format!(
            "the body moves {:.0} cm from {a} to {b}: footsteps need it to travel (move the end key first)",
            travel * 100.0
        ));
    }
    let span = b - a;
    let n = match int(p, "steps") {
        Some(n) => n,
        None => {
            let by_way = travel / STRIDE + 1.0;
            let by_time = span as f64 * 30.0 / doc.clip.fps.max(1.0) / STEP_FRAMES;
            (((by_way + by_time) / 2.0).round() as i64).clamp(2, (span / MIN_STEP_FRAMES).max(2))
        }
    };
    if span < n * MIN_STEP_FRAMES {
        return Err(format!(
            "{n} steps in {span} frames is under {MIN_STEP_FRAMES} frames a step: give fewer steps or more frames"
        ));
    }
    let stride = travel / (n - 1) as f64;
    let mut notes = Vec::new();
    if stride > LONG_STRIDE {
        notes.push(format!(
            "{:.2} m a step is a long stride: more steps make it a walk",
            stride
        ));
    }

    // where each foot lands: footfall k (1..n-1) of the stepping foot, a share of the way from where
    // it stood on `a` to where it stands on `b`
    let landing = |from: Vec3, to: Vec3, k: i64| {
        let u = k as f64 / (n - 1) as f64;
        quat::add3(from, quat::scale3(quat::sub3(to, from), u))
    };
    // the leading foot lands on odd footfalls, the other on even ones; footfall 0 is the start
    let foot_at = |is_lead: bool, k: i64| -> Vec3 {
        // the last landing of this foot at or before footfall k
        let mine = (0..=k)
            .rev()
            .find(|&j| j == 0 || (j % 2 == 1) == is_lead)
            .unwrap_or(0);
        if is_lead {
            if mine == 0 {
                lead_a
            } else {
                landing(lead_a, lead_b, mine)
            }
        } else if mine == 0 {
            other_a
        } else {
            landing(other_a, other_b, mine)
        }
    };
    let hip = sk.hip_height;
    let frame_of = |k: i64| a + ((k * span) as f64 / n as f64).round() as i64;
    let blend = |u: f64| -> Vec<Quat> { ra.iter().zip(rb).map(|(x, y)| quat::slerp(*x, *y, u)).collect() };
    // the hips' offset when they are `along` of the way to `b` (0..1), lowered by `dip` metres
    let hips = |along: f64, dip: f64| -> Vec3 {
        let mut o = quat::add3(oa, quat::scale3(quat::sub3(ob, oa), along));
        o[2] -= dip / hip;
        o
    };

    // how far a leg reaches, hip to ankle (a hair short, so a reached foot sits flat)
    let joint = |name: String| sk.bone(&name).ok_or(format!("the rig has no {name}"));
    let mut reach = f64::INFINITY;
    for side in ["L", "R"] {
        let (h, k, f) = (
            joint(format!("upperleg01.{side}"))?,
            joint(format!("lowerleg01.{side}"))?,
            joint(format!("foot.{side}"))?,
        );
        let (h, k, f) = (get(&ja, h)?, get(&ja, k)?, get(&ja, f)?);
        reach = reach.min(0.995 * (quat::len3(quat::sub3(k, h)) + quat::len3(quat::sub3(f, k))));
    }
    let contacts: Vec<i64> = (1..n).map(frame_of).collect();

    if fill == "cycle" {
        let cycle = gait::Cycle::builtin(&sk)?;
        // where each foot is after footfall k; on `b` both stand as the key has them
        let pos = |is_lead: bool, k: i64| match (k >= n, is_lead) {
            (true, true) => lead_b,
            (true, false) => other_b,
            _ => foot_at(is_lead, k),
        };
        let side = |is_lead: bool| usize::from((if is_lead { lead } else { other }) == "R");
        let walk = gait::Walk {
            sk: &sk,
            a,
            b,
            ra,
            rb,
            oa,
            ob,
            contacts: (0..=n).map(frame_of).collect(),
            swings: (1..=n)
                .map(|k| {
                    let is_lead = k % 2 == 1;
                    (side(is_lead), pos(is_lead, k - 1), pos(is_lead, k))
                })
                .collect(),
            stands: (1..=n).map(|k| pos(k % 2 == 0, k - 1)).collect(),
            reach,
        };
        let walked = gait::walk(&cycle, &walk)?;
        let per_step = span as f64 / n as f64;
        if per_step < QUICK_STEP {
            notes.push(format!(
                "{per_step:.0} frames a step is quick for a walk (the Walk loop takes {:.0}): give it more frames or fewer steps",
                cycle.step_frames
            ));
        }
        if walked.drop > 0.01 {
            notes.push(format!(
                "the hips come down {:.0} cm so the legs reach {stride:.2} m steps",
                walked.drop * 100.0
            ));
        }
        crate::edits::clear_ai(&mut doc.clip, a, b)?;
        let mut set = Vec::new();
        for (f, r, o) in walked.poses {
            if (f - a) % KEY_EVERY == 0 || contacts.iter().any(|c| KEY_NEAR.contains(&(f - c))) {
                doc.clip.set_key(Key::new(f, r, o, Interp::Smooth))?;
                set.push(f);
            }
        }
        doc.go_to(doc.frame)?;
        return Ok(json!({
            "from": a,
            "to": b,
            "keys": set,
            "contacts": contacts,
            "steps": n,
            "stride": stride,
            "lead": lead,
            "fills": [],
            "notes": notes,
        }));
    }

    let mut keys: Vec<(i64, Vec<Quat>, Vec3, Vec<Plant>)> = Vec::new();
    let first = frame_of(1);
    if first - a >= MIN_STEP_FRAMES {
        // the first step from a stand: the leading knee up as it passes the other foot
        let f = a + (first - a) / 2;
        let land = landing(lead_a, lead_b, 1);
        let mut at = quat::add3(lead_a, quat::scale3(quat::sub3(land, lead_a), 0.4));
        at[2] += KNEE_UP;
        let along = 0.2 / (n - 1) as f64;
        keys.push((
            f,
            blend((f - a) as f64 / span as f64),
            hips(along, 0.0),
            vec![
                Plant { side: lead, at },
                Plant {
                    side: other,
                    at: other_a,
                },
            ],
        ));
    }
    for k in 1..n {
        let f = frame_of(k);
        let stepping_lead = k % 2 == 1;
        let (step_side, back_side) = if stepping_lead {
            (lead, other)
        } else {
            (other, lead)
        };
        let step_at = foot_at(stepping_lead, k);
        let mut back_at = foot_at(!stepping_lead, k);
        back_at[2] += HEEL_UP;
        // the hips halfway between the feet along the walk
        let along = (k as f64 - 0.5) / (n - 1) as f64;
        keys.push((
            f,
            blend((f - a) as f64 / span as f64),
            hips(along.clamp(0.0, 1.0), CONTACT_DIP),
            vec![
                Plant {
                    side: step_side,
                    at: step_at,
                },
                Plant {
                    side: back_side,
                    at: back_at,
                },
            ],
        ));
    }

    // replace the gap's in-betweens with the step keys
    crate::edits::clear_ai(&mut doc.clip, a, b)?;
    let mut set = Vec::new();
    for (f, mut r, mut o, plants) in keys {
        if f <= a || f >= b || set.contains(&f) {
            continue;
        }
        // a stride longer than the legs reach from these hips: lower the hips until both feet land
        let (_, j) = sk.fk(&r, o).map_err(|e| e.to_string())?;
        let mut drop: f64 = 0.0;
        for pl in &plants {
            let hip_at = get(&j, joint(format!("upperleg01.{}", pl.side))?)?;
            let flat = ((hip_at[0] - pl.at[0]).powi(2) + (hip_at[1] - pl.at[1]).powi(2)).sqrt();
            let high = hip_at[2] - pl.at[2];
            if flat < reach && (flat * flat + high * high).sqrt() > reach {
                drop = drop.max(high - (reach * reach - flat * flat).sqrt());
            }
        }
        o[2] -= drop / hip;
        for pl in &plants {
            r = crate::ik::two_bone(&sk, &r, o, &format!("leg.{}", pl.side), pl.at, None, None)
                .map_err(|e| e.to_string())?;
        }
        doc.clip.set_key(Key::new(f, r, o, Interp::Smooth))?;
        set.push(f);
    }

    let mut fills = Vec::new();
    if fill != "none" {
        let saved = doc.ai_settings.clone();
        doc.ai_settings.style = style.to_string();
        // the walk keeps the model's own hips (a pull onto the keys' curve flattens the bounce)
        doc.ai_settings.trajectory = 0.0;
        if fill == "plain" {
            doc.ai_settings.ai_force = 0.0;
        }
        let mut out = Ok(());
        let ends: Vec<i64> = std::iter::once(a)
            .chain(set.iter().copied())
            .chain(std::iter::once(b))
            .collect();
        for w in ends.windows(2).filter(|w| w[1] - w[0] > 1) {
            match doc.ai_fill(Some((w[0], w[1]))) {
                Ok(f) => fills.push(json!({
                    "from": f.from,
                    "to": f.to,
                    "model": if fill == "plain" { "plain interpolation".to_string() } else { f.model },
                    "notes": f.notes,
                })),
                Err(e) => {
                    out = Err(e);
                    break;
                }
            }
        }
        doc.ai_settings = saved;
        out?;
    }
    doc.go_to(doc.frame)?;
    Ok(json!({
        "from": a,
        "to": b,
        "keys": set,
        "contacts": contacts,
        "steps": n,
        "stride": stride,
        "lead": lead,
        "fills": fills,
        "notes": notes,
    }))
}
