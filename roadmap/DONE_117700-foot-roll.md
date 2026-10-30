# Heel-to-toe foot roll

- **Status:** done (2026-10-30)
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

## Result
2026-10-30. A new undoable command, `feet.roll` (`crates/ac-core/src/footroll.rs`), separate from the
physics pass, so it never runs unasked. `fixtures/feet-roll.json` works a stiff stride by hand; proof in
`target/shots/2026-10-30/foot-roll/` (`before.png`, `after.png`: toes up, flat, heel up).
- **Finding:** the built-in walks already roll: heels rise 11 cm on `Walk`, 16 cm on `Run`. So the rule
  leaves them alone and only stiff, flat-footed motion changes. 51 of the 55 built-in clips don't change
  (`Walk`, `Run`, `Standing/Idle` and `Side step 9` are in a test). `Walk 13`, `Walk 15` and two of the
  circle walks change on 15 to 19 frames, with no worse worst velocity change. Pop counts on them rise
  by 2 to 4 only because pops are measured at keys and the command adds keys.
- **Stride fixture:** slide after `feet.lock` 0.68 cm, after the roll 0.09 cm. The ball stays within
  2 mm while it carries; nothing goes below the floor; no pops.
- **Not met as written:** the acceptance line asks for the pivot on "a walk". No built-in walk qualifies,
  so that was measured on the stride, not on a walk. AI output with stiff feet would be the real test,
  and none was run while the GPU is training.
- `write_solved` overwrites an existing key's rotations, as `feet.lock` does: running the command on
  hand-made keys changes them (undoable).
- The heel point (`HEEL_BACK`, 5 cm) is far enough to pivot on: a 13 degree roll on the stride.
- **Video (2026-10-30):** `target/shots/2026-10-30/videos/117700-foot-roll-feet-close-quarter-speed.mp4`,
  the stride's feet from the side at quarter speed. The toes rising at the start show clearly. The middle
  matches, as it should. The heel rise at the end is hard to see at this size. It is the stride only: it says nothing about a real walk or AI output.
- **Owner (2026-10-30):** accepted on the video, knowing the pivot was measured on the stride, not a walk or AI output.
