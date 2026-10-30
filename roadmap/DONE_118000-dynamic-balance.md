# Dynamic balance and weight transfer

- **Status:** done (2026-10-30)
- **Requires:** 115600, 117400
- **Source:** 115100

## What
Balance checks only whether a slow centre of mass is over the support (`crates/ac-core/src/physics.rs:12-16`).
A moving body isn't checked at all, and `clip.metrics` only counts those frames as `moving_frames`.

Use the centre of mass with its velocity: where it is heading, measured against the support the
feet are about to give (the capture point). Measure it first, in `clip.metrics` and `clip.critique`.
Correct only with the hips and spine, blended, and with planted feet held. The upper body's
counterbalance follows the hips.

Not here: anticipation and recovery (115100). Static balance stays for a body that really stands.

## Acceptance
A new metric on all built-ins, by hand on a small case in a fixture; the recorded walks (`Walk 12` to
`17`) stay within a stated tolerance of their keys, since they already move correctly; a hand-keyed
walk with its weight on the wrong side measurably improves; the ratchet doesn't get worse; contact
sheets for the owner.

## Result
2026-10-30. New `capture` block in `clip.metrics`, a `capture` finding in `clip.critique`, and a new
undoable command, `hips.transfer` (`crates/ac-core/src/weight.rs`). The ratchet records it as `capture_cm`.
Proof in `target/shots/2026-10-30/weight/` (`before.png`, `after.png`).
- **The measure changed from the plan.** The gap counted in any direction flagged good motion: recorded
  walks reached 82 cm, because a body is always falling ahead of its feet until the next step catches it.
  Only the part **across the way the body is going** is counted, with a foot landing within 0.5 s as
  support. Recorded walks `Walk 12` to `17` then reach at most 6.8 cm and sprints 7.9, so 8 cm is tolerated.
- **Tolerance on the recorded walks:** `Walk 12` to `17` don't change at all (in a test).
- **Hand-keyed wrong-side weight:** `fixtures/hips-transfer.json`: worst gap 26.3 to 15.8 cm, frames out
  28 to 18, feet still, no pops. It improves it but doesn't close it. `left_cm` says what is left.
- **Hand-worked formula:** `crates/ac-core/tests/weight.rs`.
- **Over 8 cm on built-ins:** `Walk` (on the spot) 20.6, `Sit down and stand up 5` 25.3, `Jump` 23.1,
  `Jump 13` 8.8 and the circle walks 10.5 to 13.7 (they lean into the turn). So `clip.critique` now
  reports these. `hips.transfer` keeps `Walk` and `Sit down and stand up 5` unchanged, because no round
  helped; it moves `Jump` to 16.5 and the circle walks to about 8. On those, pop counts rise because
  pops are measured at keys and the command adds keys.
- **Not built:** the upper body's counterbalance. Hips moving carry it along; nothing turns the spine.
- **Not in the physics pass:** it is a command, so nothing changes unless it's run.
- **Video (2026-10-30):** `target/shots/2026-10-30/videos/118000-weight-quarter-speed.mp4`, the fixture from
  the front at quarter speed; the widest point is at 4 s. The cameras differ slightly, because `--fixed`
  frames each body.
- **Finding from the video (2026-10-30):** only the planted foot is held. By `pose.query` at frame 30, the
  planted `foot.L` doesn't move, but the lifted `foot.R` goes from (-0.177, -0.004, 0.223) to
  (-0.301, -0.067, 0.167): about 15 cm, carried with the hips. The fixture's "feet stay where they are"
  is only true of feet on the floor.
- **Owner (2026-10-30):** accepted on the video, including the lifted foot moving with the hips and no upper body counterbalance.
