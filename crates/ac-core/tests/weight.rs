//! The capture-point gap, worked by hand.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ac_core::weight::{Sample, gaps};

/// Two feet on the floor, each a square 20 cm across about the origin. The centre of mass is 1 m up,
/// starting at x = 0.2, moving along +x at 0.25 m/s: over a second it goes 25 cm, under the 30 cm of
/// a body going anywhere, so the whole gap counts. On frame 15 (30 fps) it is at x = 0.125 + 0.2 =
/// 0.325 and the capture point is 0.25 / sqrt(9.81 / 1) = 0.0798 further on: 0.4048. The support's
/// edge is at 0.1, so the gap is 0.3048, back along -x.
#[test]
fn a_moving_body_past_its_feet_has_the_gap_worked_by_hand() {
    let square = [[-0.1, -0.1], [0.1, -0.1], [0.1, 0.1], [-0.1, 0.1]];
    let frames: Vec<Sample> = (0..31)
        .map(|f| Sample {
            feet: (0..2)
                .flat_map(|foot| square.iter().map(move |p| (foot, [p[0], p[1], 0.0])))
                .collect(),
            com: [0.2 + 0.25 * f as f64 / 30.0, 0.0, 1.0],
            rests: false,
        })
        .collect();
    let out = gaps(&frames, 30.0, &[]);
    let g = out[15].expect("a gap on frame 15");
    assert!((g.outside - 0.3048).abs() < 1e-3, "{g:?}");
    assert!((g.by[0] + 0.3048).abs() < 1e-3 && g.by[1].abs() < 1e-9, "{g:?}");
}

/// A body swaying 10 cm either side once a second goes nowhere. On frame 15 (30 fps) its centre of
/// mass is at the origin, 1 m up, moving at 0.1 * 2 * sin(2 pi / 30) * 30 / 2 = -0.6237 m/s along x (a
/// difference over two frames), so the capture point is at x = -0.6237 / sqrt(9.81) = -0.1991. The
/// feet are squares 20 cm across, centred 0.3 behind it, so the nearest support is the corner
/// (-0.1, -0.2): 0.0991 and 0.2 away, a gap of 0.2232. If one of those feet instead slides back along
/// the floor at 0.9 m/s the whole time, the body is walking on the spot and no gap is measured.
#[test]
fn a_walk_on_the_spot_has_no_gap_and_the_same_body_on_held_feet_has_one() {
    let square = [[-0.1, -0.1], [0.1, -0.1], [0.1, 0.1], [-0.1, 0.1]];
    let body = |slide: f64| -> Vec<Sample> {
        (0..31)
            .map(|f| {
                let t = f as f64 / 30.0;
                let at = |foot: usize, p: &[f64; 2]| {
                    let back = if foot == 0 { slide * t } else { 0.0 };
                    (foot, [p[0], p[1] - 0.3 - back, 0.0])
                };
                Sample {
                    feet: (0..2)
                        .flat_map(|foot| square.iter().map(move |p| at(foot, p)))
                        .collect(),
                    com: [0.1 * (std::f64::consts::TAU * t).sin(), 0.0, 1.0],
                    rests: false,
                }
            })
            .collect()
    };
    let g = gaps(&body(0.0), 30.0, &[])[15].expect("a gap on held feet");
    assert!((g.outside - 0.2232).abs() < 1e-3, "{g:?}");
    assert_eq!(gaps(&body(0.9), 30.0, &[])[15], None);
}

/// The same body over its feet, heading nowhere near an edge: no gap.
#[test]
fn a_body_over_its_feet_has_no_gap() {
    let square = [[-0.3, -0.3], [0.3, -0.3], [0.3, 0.3], [-0.3, 0.3]];
    let frames: Vec<Sample> = (0..31)
        .map(|f| Sample {
            feet: (0..2)
                .flat_map(|foot| square.iter().map(move |p| (foot, [p[0], p[1], 0.0])))
                .collect(),
            com: [0.25 * f as f64 / 30.0 - 0.125, 0.0, 1.0],
            rests: false,
        })
        .collect();
    assert_eq!(gaps(&frames, 30.0, &[])[15], None);
}
