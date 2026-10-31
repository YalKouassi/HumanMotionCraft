//! The frame-rate fit between the animation and a model that works at its own rate, and a reply's
//! frames read as rotations and roots.

use ac_core::quat::{Quat, Vec3};
use serde_json::Value;

/// How many frames at `model_fps` `frames` frames at `fps` are (both ends included): the frame
/// the model makes for the animation's last frame.
pub fn model_frames(frames: i64, fps: f64, model_fps: f64) -> i64 {
    ((frames - 1) as f64 * model_fps / fps).round() as i64 + 1
}

fn floats(v: &Value) -> Option<Vec<f64>> {
    v.as_array()?.iter().map(Value::as_f64).collect()
}

fn quat(v: &Value) -> Option<Quat> {
    floats(v)?.try_into().ok()
}

fn vec3(v: &Value) -> Option<Vec3> {
    floats(v)?.try_into().ok()
}

/// A reply's frames: rotations `[frame][bone]`, root offsets `[frame]`, and the notes it carried.
pub type Frames = (Vec<Vec<Quat>>, Vec<Vec3>, Vec<String>);

/// A reply's `rotations` `[frame][bone][x, y, z, w]`, `roots` `[frame][x, y, z]` and `notes`.
pub fn read(v: &Value) -> Result<Frames, String> {
    let bad = || {
        "the helper's result is not rotations [frame][bone][x, y, z, w] and roots [frame][x, y, z]"
            .to_string()
    };
    let rotations = v["rotations"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|f| {
            f.as_array()
                .and_then(|b| b.iter().map(quat).collect::<Option<Vec<_>>>())
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(bad)?;
    let roots = v["roots"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(vec3)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(bad)?;
    let notes = v["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n.as_str().map(String::from))
        .collect();
    Ok((rotations, roots, notes))
}

/// The frames have the count and bones asked for (`bones` `None`: as many as the first frame),
/// and every number is finite.
pub fn check(
    rotations: &[Vec<Quat>],
    roots: &[Vec3],
    frames: usize,
    bones: Option<usize>,
) -> Result<(), String> {
    let bones = bones.or_else(|| rotations.first().map(Vec::len)).unwrap_or(0);
    if rotations.len() != frames || roots.len() != frames {
        return Err(format!(
            "the helper made {} frames of rotations and {} roots; {frames} were asked for",
            rotations.len(),
            roots.len(),
        ));
    }
    if rotations.iter().any(|f| f.len() != bones) {
        return Err(format!("the helper's frames don't all have {bones} bones"));
    }
    let finite = rotations
        .iter()
        .flatten()
        .flatten()
        .chain(roots.iter().flatten())
        .all(|x| x.is_finite());
    if !finite {
        return Err("the helper's motion has numbers that are not finite".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(r: Result<(), String>) -> String {
        r.err().unwrap_or_default()
    }

    #[test]
    fn frames_convert_to_the_models_rate() {
        assert_eq!(model_frames(31, 30.0, 30.0), 31);
        assert_eq!(model_frames(61, 60.0, 30.0), 31);
        assert_eq!(model_frames(25, 24.0, 30.0), 31);
        assert_eq!(model_frames(2, 30.0, 30.0), 2);
    }

    #[test]
    fn a_short_or_non_finite_reply_is_refused() {
        let rots = vec![vec![[0.0, 0.0, 0.0, 1.0]; 2]; 3];
        let roots = vec![[0.0; 3]; 3];
        assert!(check(&rots, &roots, 3, Some(2)).is_ok());
        assert!(err(check(&rots, &roots, 4, Some(2))).contains("4 were asked"));
        assert!(err(check(&rots, &roots, 3, Some(3))).contains("3 bones"));
        let nan = vec![[f64::NAN; 3]; 3];
        assert!(err(check(&rots, &nan, 3, None)).contains("not finite"));
    }
}
