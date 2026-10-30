# Heel-to-toe foot roll

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 117400
- **Source:** 115100

## What
Feet don't roll. The fulcrum rule only moves the root, and only balance changes rotations
(`crates/ac-core/src/physics.rs:74`). `feet.lock` and `plant_feet` hold the ankle with the leg's IK,
which keeps each frame's own foot turn (`crates/ac-core/src/ik.rs:159`), so nothing keeps the foot
consistent over a contact.

Using 117400's phases: the heel lands, the foot lowers about the heel, stays flat, then rolls over
the ball of the foot and lifts. The pivot stays still in the world, and the leg IK keeps the knee
consistent. Only for motion that does this: a flat-footed stand or a shuffle stays as keyed. The heel
point sits 5 cm behind the ankle (`HEEL_BACK`), so check that it is far enough back to pivot on.
Where this lives (the pass, or a separate command) is part of the work, and manual keys are never
overwritten by the pass.

## Acceptance
On a walk, in each stance the pivot point moves under 1 cm while it carries, and no foot point goes
below the floor; the ratchet shows no new pops; standing clips are unchanged; contact sheets of heel
strike, flat and toe-off for the owner.
