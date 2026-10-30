# Landing compression

- **Status:** built, waiting for the owner (2026-10-30)
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

## Result
2026-10-30. In the physics pass (`crates/ac-core/src/physics.rs`), so it needs no command and no keys
change. Proof: `target/shots/2026-10-30/landing/after.png` (frames 28 to 36 of the stiff jump). There is no
"before" sheet: only the numbers show that side, since the old hips simply stop on the landing frame.
- **Hand-worked fixture:** `fixtures/physics-landing.json`: the change of the hips' downward speed at
  landing falls from 3.11 to 1.35 m/s, the hips dip 6.4 cm on the frame after next and are back by frame 45,
  and the ankle's height doesn't move.
- **Keys that compress aren't doubled:** the same jump with a keyed dip of 3.9 cm gets nothing added.
- **Built-ins:** none changes: the ratchet's numbers for every clip are as they were, jumps included.
  The first version compressed `Run 10`'s strides and added a pop, so landings under 1.5 m/s don't compress.
- **Size:** fixed by the arriving speed and a 0.06 s time constant. Nobody has judged how it looks in
  motion yet, and that constant is the thing to change if it reads too quick or too soft.
