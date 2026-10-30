# Landing compression

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 117400
- **Source:** 115100

## What
The flight rule keeps the take-off frame, the landing frame and every pose in the air
(`crates/ac-core/src/physics.rs:17-21`), so the hips follow the parabola into the landing and stop on
the keyed pose. Nothing absorbs the landing.

After a landing, carry the hips' arriving speed down into a short compression and recover, with feet
held by the leg IK, sized from the landing speed. Skip it when the keys already compress, so a
keyed landing isn't doubled.

## Acceptance
On a hand-keyed stiff jump: the hips' downward speed changes less sharply at landing than now, by a
number worked out by hand in a fixture, and the feet don't move; the built-in jumps that already
compress are unchanged within a stated tolerance; contact sheets for the owner.
