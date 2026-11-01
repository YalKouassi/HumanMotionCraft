# Decision: other bodies and the character creator

- **Status:** done (decided by the owner, 2026-11-01)
- **Requires:** none
- **Source:** owner's request (2026-11-01): a later character creator as a separate app, humans only, using
  this rig, with the foundation for swapping in other meshes that follow the same rig

## Decision
**1. The same rig means:** the same 104 joints, parents and rest directions, with each body having its own
bone lengths. Every bone rests with no rotation of its own, and a pose is only rotations applied to rest
positions (`crates/ac-core/src/rig.rs:3-10`), so rotations carry over to any lengths, and the root offset is
already in hip heights. The app still animates only the built-in rig, so AI Fill, physics, ranges and the
library stay as they are (144000). Other lengths move hands and feet, so contacts need 192000.

**2. Humans:** start with humans at adult size. Small or child bodies may be needed in future, so nothing in
the foundation may assume adult size: the allowed range is data, not code. Small bodies need their own item
first. Bodies have no genitals or similar detail.

**3. The creator is its own app, outside this repo.** So this repo owns the rig and the body file, and has to
publish them in a form another repo can use (190000).

**4. Bodies are local only for now.**

## For a later sharing item
The owner's view: bodies have no genitals, so community sharing checks should be safe. That holds for what the
creator makes. A `.glb` can hold any mesh, so a shared file could contain anything whatever the creator does.
A sharing item needs to settle how a body proves where it came from or how it gets checked, including small
bodies. Nothing is shared until then.

## Result
190000 to 194000 changed to match, 2026-11-01.
