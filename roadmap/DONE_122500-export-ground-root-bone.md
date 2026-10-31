# Export: a root bone on the floor

- **Status:** done (2026-10-31)
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

## Review (2026-10-31)
A code review of the built item (an agent, not the owner; the owner's decisions above stand). The
code checked: `crates/ac-io/src/export.rs` (the ground bone), `crates/ac-app/src/app/files/export.rs`
(the dialog), `fixtures/export-root-bone.json`, `crates/ac-io/tests/export.rs`.

Invariants read through and found right (no change):
- The ground bone's height is written as exactly 0 every frame (`crates/ac-io/src/export.rs:570`),
  its path is the root joint's own x and -y (`:555` to `:571`), so the 1 mm acceptance holds to
  f32 rounding, and its yaw is `atan2(m[1][0], m[0][0])` of the root's world rotation, the same
  formula as `heading` in `crates/ac-ai/src/lib.rs:215`, as the What asked.
- World poses under `ground` equal the `hips` export's: the hips' local is `inv(ground) · world(hips)`
  (`:616`), every other joint's local is unchanged, and the skin's inverse bind matrices gain only an
  identity row for the ground. `in_place` zeroes only the root offset's x and y and removes only the
  yaw from the root's rotation (`:538` to `:547`), height and every other bone untouched.
- `clip.export_batch` takes the same `root` and `target` as `clip.export` and passes them through
  (`crates/ac-io/src/export_batch.rs:107`); `root` and `target` are in `docs/commands.md` and
  `fixtures/schema.json`; the summary's keys are all declared in `returns.rs` (`CLIP_EXPORT`).
- Hostile input: an unknown `root` is refused with the three choices; an open animation with no keys is
  refused; `MAX_FRAMES` caps the frames; a clip whose bones are not the rig's is refused.

Findings:
1. **Medium, fixed (commit d99920d).** The acceptance's "world poses equal the `hips` export's" had no
   test: `ground_root_bone_stays_at_height_0_and_matches_hips_path` checked height and path only.
   `crates/ac-io/tests/export.rs:146` now reads every kept joint's world pose through the ground node
   with an outside glTF reader, for the game profile and for the full profile through the physics
   pass, and asserts it is the rig's within 2e-4.
2. **Low, fixed (commit 2eb07cb).** The dialog's `Custom` preset was `(full, hips)`, the same choices
   as `Blender`, so both lit at once and `Custom` meant nothing of its own
   (`crates/ac-app/src/app/files/export.rs:24`). `Custom` now sets nothing and lights when no named
   preset matches; each preset's hover says exactly the profile and root it writes. The presets write
   only those two options, which is all they claim.
3. **Low, the owner's call, not changed.** The presets name four other products in the window
   (`crates/ac-app/src/app/files/export.rs:24` to `:29`). `cargo xtask names` does not list them (the
   roadmap itself names Unity and Blender in 122000 and 162000), but AGENTS.md's "other products'
   names don't appear in code" is the rule as written. Renaming them would make the presets
   meaningless, so the wording is left for the owner to decide.
4. **Low, not changed.** The dialog remembers the joint profile between sessions
   (`crates/ac-app/src/app/files/export.rs:390`) but not the root mode: it opens on `hips` every time
   (`:78`). A saved preference for `root` would be a new `#[serde(default)]` field on the files state;
   left as a follow-up rather than widen this review.
5. **Low, by design, not changed.** The ground bone's yaw follows the pelvis frame by frame, so a walk's
   pelvis sway (a few degrees each step) is in the root motion's yaw. That is what the What asks for
   ("the yaw from the hip line"); an engine that wants a smooth heading would need a smoothed or
   per-stride yaw, which is a change of design for the owner.
6. **Low, wording.** The Result's "interactive visual diagram" is a static drawing that changes with the
   selected root mode (`draw_root_visual`), not something the person can act on. The Result's other
   claims (the option on both commands, the fixture, the unit tests) match the code.

Accepted by the owner (2026-10-31).

## Decisions on the review (2026-10-31)
The owner: the presets keep the product names, with an exception in the rule, and the dialog remembers the root mode, both in 170000. The ground bone's yaw stays until an engine user complains.
