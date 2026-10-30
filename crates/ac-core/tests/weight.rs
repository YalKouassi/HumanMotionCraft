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
