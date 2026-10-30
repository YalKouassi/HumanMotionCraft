# Foot contact phases

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 115600, 116200
- **Source:** 115100

## What
Contact is decided per foot point and per frame, so one frame near the floor plants a point, and the
mean of several slips can change suddenly as points join or leave (`crates/ac-core/src/physics.rs:501-522`).
The rules also differ: 5 cm in the pass and metrics, 4 cm in `lock_feet`, and another in the quality
scan.

Make one contact function that `physics`, `metrics`, `feet.lock` and `plant_feet` all use, giving each
foot a phase per frame: swing, landing, planted, release. It needs a minimum duration, and separate
thresholds for entering and leaving contact. Correction blends in and out across landing and release.
Deterministic, with rules simple enough to check by hand, as the pass's header asks.

## Acceptance
A fixture: a foot that skims the floor for one frame isn't planted, and a real step gives the four
phases on the frames worked out by hand. The ratchet (115300) improves on slide with no new pops. The
contact thresholds exist in one place in `ac-core`. Before-and-after contact sheets of a walk, a turn
and a side step for the owner.
