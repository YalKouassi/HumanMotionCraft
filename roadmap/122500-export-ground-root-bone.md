# Export: a root bone on the floor

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 106000
- **Source:** 144000 (one rig, retarget on export); owner request 2026-10-30

## What
Our top bone, `root`, is the pelvis: it sits at the hips (`assets/rig/default_rig.json`, every
rest head is relative to it) and the character's position is its offset in hip heights. That is
the mocap convention (BVH files and every set we train on put the root at the hips) and what the
AI, physics and IK expect, so it stays. Many engines instead want a bone on the floor at the top
of the hierarchy that carries the root motion (the pelvis then moves relative to it). Some engines
derive root motion themselves and don't need one.

Add an export option `root`:
- `hips` (today): no extra bone.
- `ground`: a new top bone on the floor under the hips; it carries the horizontal path and the
  facing turn (the yaw from the hip line, as `heading` in `crates/ac-ai/src/lib.rs` computes it);
  the pelvis keeps the rest, relative to it.
- `in_place`: as `ground`, with the path and turn removed (for engines that drive movement
  themselves).

## Acceptance
A fixture exports a walk with each option; with `ground` the new bone stays at height 0 and its
path matches the hips' horizontal path within 1 mm, and world poses equal the `hips` export's;
with `in_place` the hips' horizontal path is 0.

## Result
2026-10-30. Added `root` option (`hips` | `ground` | `in_place`) to `clip.export` and `clip.export_batch`.
`ground` mode generates a top bone `ground` at Y=0 carrying horizontal path and heading yaw, with all joint
world poses identical to the `hips` export. `in_place` mode locks ground at origin and removes horizontal
displacement and yaw from the hips.
The Export dialog now includes game engine presets (Unreal Engine, Unity, Godot, Blender, Custom) and
an interactive visual diagram illustrating root bone placement relative to the floor.
Verified with fixture `fixtures/export-root-bone.json` and unit tests in `crates/ac-io/tests/export.rs`.
