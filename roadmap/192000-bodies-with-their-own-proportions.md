# Bodies with their own proportions

- **Status:** not started; later (owner, 2026-11-01)
- **Requires:** 184000, 189000, 191000
- **Source:** owner's request (2026-11-01): a character creator, and other meshes on the same rig

## What
Only if 189000 allows each body its own bone lengths.

Editing, AI Fill, physics and ranges stay on the built-in rig (144000). A body with other lengths is shown
and exported by applying the same rotations to its own rest positions (`crates/ac-core/src/rig.rs:3-10`).
Bone names match, so this needs no mapping and no rest-pose alignment, unlike a person's own character in
146000.

Rotations carry over, but positions don't:

- feet slide or sink, so reuse 146000's scaled root path and leg IK on planted frames;
- hands miss what they touched, such as the hips, or the other hand;
- longer or thicker limbs clash where the rig doesn't, so run the clash check (184000) on the body itself;
- the problem frames go on the timeline, as in 147000.

Export onto such a body uses the same corrected motion the viewport shows.

## Acceptance
- A tall and a short fixture body each play a walk with planted feet moving under 1 cm and staying above the
  floor (146000's line).
- Hand-on-hip and arms-crossed poses report any contact lost or clash gained, per body.
- The viewport shows the body and marks at 60 fps on the walk clip.
- An exported `.glb` moves as the viewport does.
- The owner approves side-by-side MP4s, physics off, under `target/shots/<date>/body-proportions/`.
