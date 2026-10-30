# Dynamic balance and weight transfer

- **Status:** not started; the acceptance line needs the owner's agreement
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
