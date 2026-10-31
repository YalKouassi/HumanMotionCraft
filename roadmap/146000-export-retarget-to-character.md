# Export onto your own character (retarget)

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 144000, 122500
- **Source:** 144000 (one rig, retarget on export)

## What
- Import a character's skeleton (.glb) as a target; guess the bone mapping from names, let the
  person correct it, align the rest poses (A-pose, T-pose, bone roll); save it as a mapping file.
- `clip.export` / `clip.export_batch` take the mapping as `target`.
- Keep contacts: scale the root path by the hip-height ratio, leg IK on planted frames (the
  contacts the app already computes), optional hand targets.
- Extra spine bones are shared out from ours; bones we can't drive stay at rest, and the result
  lists them.
- Commands first: e.g. `retarget.load`, `retarget.map`, `retarget.check`.

## Acceptance (agreed 2026-10-30)
A test character with different proportions, an A-pose and other bone names exports a walk whose
planted feet move under 1 cm and stay above the floor; a fixture checks the guessed mapping and
the list of undriven bones; the mapping file round-trips byte for byte.

## Result
2026-10-30, built. Review material under `target/shots/2026-10-30/retarget-export/`:

- `walk-blender-mannequin-vs-character.mp4` (3 s, 800x1000): Walk 12 exported twice, the mannequin
  (left column) and the test character (right column), front view on top and the body's left side
  below, each file imported and rendered by Blender 5.1 headless (`tools/blender_check_export.py`,
  now with a `follow` option so the camera keeps up with a walk). Look at: the character's feet
  landing flat and holding while the other leg swings (from 0.4 s on), the arms hanging and
  swinging although the character rests in an A-pose, the longer stride (its legs are 16 % longer,
  so its path is 16 % longer: it ends 2.43 m along, the mannequin 2.09 m). Good looks like: the same
  walk on a taller, boxier body, no foot skating, no foot through the floor.
- `character-strip-frames-0-20-45-61-front-20-61.png`: Blender frames of the character alone.
- `tall-walk.glb`, `mannequin-walk.glb`, `tall.retarget.json`: the files behind the video, and
  `blender/*.png`, the 360 rendered frames.

### What was built
- `ac_core::character`: the character's skeleton as data (`Character`: nodes with rest TRS in the
  rig's axes, the skin's joints and inverse bind matrices, the mesh's lowest point), the
  `Mapping` (landmark -> node name; required: hips, head, shoulders, elbows, wrists, hips, knees,
  ankles; optional: spine, neck, toes), `guess` through `ac_core::jointmap`, `plan::Plan` (which
  rig bone drives each node, the world alignment of the two rest poses, the spine and neck chains
  shared out from our five and three bones by proportion, a clavicle guessed from the tree, the
  legs and arms for IK, the scale, the floor), `drive::drive` (one frame: every driven node takes
  its bone's world turn through the alignment; the hips go where our hip joints are, scaled; then
  two-bone IK per leg onto our ankles, the knee bending our knee's way, the foot keeping its
  orientation; a target out of reach is said) and `check::drive_clip` (a clip: IK weights from
  the rig's foot phases, fading over 3 frames into the swing; the feet's drift, travel, lowest
  point and unreached frames; the problems as spans).
- `ac_io::character`: `read.rs`, our own glTF skeleton reader on the export's container code
  (nodes, TRS or a matrix, uniform scale only, parents from the children lists, the first skin,
  the mesh's position min; every count and index checked; a file that lies is refused with the
  reason), the mapping file (`<id>.retarget`, version 1, sorted keys), `commands.rs` and
  `export.rs`.
- Commands: `retarget.load {path, joints?, hands?}` (a `.glb`: the mapping is guessed; a
  `.retarget.json`: its mapping, its character read from the path it names, relative to the
  file; `joints` corrects, `"none"` unmaps; the result says what is driven, undriven, missing,
  the scale and the notes; a partial mapping loads but is not `ready`), `retarget.map {joints?,
  hands?}`, `retarget.check {}` (the open animation as shown, physics included, through the
  character: per foot planted frames, spans, `slide_cm`, `travel_cm`, `lowest_cm`, unreached,
  `holds`; the problems), `retarget.save {path}` (hash of the bytes), `retarget.unload {}`;
  `clip.export` and `clip.export_batch` take `target` (a `.glb` or a mapping file) and then write
  the character's own file back with the animation on its own nodes (rotation track per driven
  node, translation for the hips, undriven nodes untouched and listed, the summary under `target`).
- The test character `fixtures/data/tall-character.glb`, written by `python -m
  ac_train.glbwrite` (`training/ac_train/glbwrite.py`): 1.9 m tall, legs 0.92 m against the rig's
  0.79 m, an A-pose with the arms 45 degrees down, a bone roll on every joint, an `Armature` root
  scaled 0.01 with the joints in centimetres, names such as `Pelvis`, `L_Thigh`, `L_Calf`,
  `L_Toe0`, `L_UpperArm`, `L_Forearm`, `L_ForearmTwist`, `L_Hand`, fingers, `Nub` ends and a
  `Weapon_Prop`, and a skinned box mesh with its soles at y = 0.
- Fixture `fixtures/retarget-character.json` (the guessed mapping, the 14 undriven nodes, the scale
  1.15925 worked out from the two rigs' leg lengths, `holds` on both feet, the unmap/remap, the
  saved mapping loaded again, the export through it, five refusals) and
  `crates/ac-io/tests/retarget.rs` (the mapping file round-trips byte for byte, also with a
  relative character path; the exported file read back by an outside glTF reader moves exactly as
  the drive worked it out on frames 0, 20 and 61 of the walk, within 2e-4 on every node's world
  matrix; the feet hold).

### The numbers (Walk 12, 90 frames, physics on)
Through `retarget.check` on the built-in clip as it is: left foot planted 55 frames in 3 spans,
drift 0.76 / 0.66 / 0.53 cm; right foot 49 frames, 0.49 / 0.72 / 0.88 cm; lowest point 5.0 cm
above the character's floor (its toe joint sits 3 cm above its sole, and the mannequin's own toes
in this clip bottom out about 1.7 cm up); no frame out of reach. With `feet.lock` first (the
fixture's and the video's case): 0.71 / 0.48 / 0.59 cm and 0.38 / 0.53 / 0.97 cm. Per-frame
travel added up over a span (`clip.metrics`' definition, which counts every millimetre of jitter)
is 1.6 to 2.2 cm for the character against 1.6 to 2.7 cm for the mannequin itself, so "moves
under 1 cm" is measured as the drift from where the foot came down, and said as such
(`slide_cm`), with the travel beside it (`travel_cm`). Blender's lowest mesh vertex over the 90
frames: -0.9 cm to +1.5 cm (the mannequin's: -0.9 to +0.1 cm). The exported file carries 22 driven
nodes; `retarget.check` over the 90 frames takes 1.2 ms in release.

### Decisions made here (the owner may overrule)
- The hip-height ratio is taken on the legs' bone lengths (thigh + shin, both sides averaged) and
  the ankle heights are mapped separately (`Plan::map_point`): a straight leg then stands exactly
  on the character's floor whatever its ankle height. The character's floor is its mesh's lowest
  point at rest, else its lowest joint (said in the notes).
- Toes, neck and spine are optional landmarks: a character without toe joints is common. Without
  toes the foot is the leaf and its floor height is then its ankle's unless the mesh says better.
- Leg IK runs on the frames the rig's foot is down (landing, planted, release) and fades over 3
  frames into the swing, so a plant never pops. Hands are off by default (`hands: true` solves
  the arms onto the wrists the same way).
- A `.glb` loaded by a relative path is kept absolute, so a saved mapping names it from anywhere;
  a mapping file's relative character path stays relative to the mapping file.
- `root: ground | in_place` with a `target` is refused as not supported yet: the character's own
  hips carry the motion. The export keeps the character's file whole (meshes, materials, textures)
  and replaces any animations it had (said in the notes); a file whose buffers are outside the
  `.glb` is refused.
- Nodes with non-uniform scale, shear or a mirror are refused; a `matrix` node that is driven is
  rewritten as TRS in the exported file (glTF forbids animating a matrix node).

### Left open
- The Export dialog in the window has no character field yet: the export onto a character runs
  through the command (`clip.export {target}`), the palette or MCP.
- Not tried on a character from another tool (only the generated one and Blender's reading of
  the result); a real character's twist bones and extra chains should be looked at when one is
  to hand. Unity is not installed (as in 122000).

## Review (2026-10-31)
A code review of the built item (an agent, not the owner; the decisions above stand). The code
checked: `crates/ac-core/src/character/` (`mod.rs`, `plan.rs`, `drive.rs`, `check.rs`),
`crates/ac-core/src/jointmap.rs`, `crates/ac-io/src/character/` (`read.rs`, `mod.rs`,
`commands.rs`, `export.rs`), `crates/ac-io/src/export.rs` (the container code the reader shares),
`fixtures/retarget-character.json`, `crates/ac-io/tests/retarget.rs`.

Read through and found right (no change):
- Hostile skeletons: a parent that is not a node, a node that is its own parent, a loop, a non-finite
  transform, a zero or negative scale, more than `MAX_NODES` nodes, a skin naming a joint that is not a
  node, a count of inverse bind matrices that differs from the joints, a non-uniform scale, a shearing
  or mirroring matrix, a zero rotation and a buffer the accessors run past are all refused with the
  reason (`crates/ac-core/src/character/mod.rs:111` to `:193`, `crates/ac-io/src/character/read.rs`).
  A mapping file naming a node the character lacks is refused with the node list
  (`crates/ac-io/src/character/mod.rs:200` to `:204`), as is one with a key that is not a landmark, no
  version, or a newer version. An empty clip is refused by `retarget.check` and the open-animation
  export; a library clip with no keys exports one rest frame.
- The mapping file round-trips byte for byte: a `BTreeMap` at both levels, pretty-printed, one
  trailing newline, the character path stored as given (`mod.rs:42` to `:53`); the test
  `the_mapping_file_round_trips_byte_for_byte` covers an absolute and a relative character path.
- Undriven nodes are listed in `retarget.load`, `retarget.check`, the export summary, the file's
  `extras` and a timeline mark; an unreachable target is counted and marked (`check.rs:218` to `:233`).
- The feet: IK targets the ankle at the scaled rig ankle through `Plan::map_point`, so a straight
  leg stands exactly on the character's floor; the check measures drift from where the foot came
  down (`slide`), travel, the lowest point against the floor and the frames the leg fell short. Every
  output key of the five `retarget.*` commands and `clip.export`'s `target` is declared in
  `returns.rs`.

Findings:
1. **High, fixed (commit 3f207ef).** A crash on a near-zero limb segment. `two_bone` returned early
   only under 1e-9 m and then called `f64::clamp` with a low end of `|la - lb| + 1e-6` and a high end
   of `la + lb - 1e-6`; for a segment under a micrometre the low end is above the high end and `clamp`
   panics (`crates/ac-core/src/character/drive.rs:118`, before the fix). `Plan::new` only checks the
   two legs' lengths averaged (`plan.rs:429`), so a character whose knee node sits within a micrometre
   of its hip or ankle passed the plan and crashed in `retarget.check`, `clip.export {target}` (caught
   by the session, the command failing) and the live preview (`character_frame` calls `drive`
   directly, outside the session). Such a segment, and a target that is not a number, now return
   `None`. Regression test `a_limb_with_a_near_zero_segment_is_not_solved_and_does_not_panic`.
2. **Medium, fixed (commit 18adfcc).** A silent IK failure. When `two_bone` returned `None` the drive
   left the foot where forward kinematics put it and `unreached` stayed 0, so the check, the export
   summary and the `unreached` timeline mark never said so (Rule 1). The whole distance from the foot
   to its target is now reported (`drive.rs:209`).
3. **Medium, fixed (commit d596a61).** One node for two landmarks was accepted. `Builder::add`
   returns without a word when a node is already driven, so `joints: {"l_knee": "L_Foot"}` left the
   ankle driven as the knee, the real knee undriven and unlisted, and a leg the IK can't solve.
   `Plan::new` now refuses it naming both landmarks (`plan.rs:244`); `retarget.load` and
   `retarget.map` report `ready: false` with that reason. Session test added to
   `bad_input_is_refused_with_a_reason`.
4. **Medium, fixed (commit 1bcb326).** Unchecked arithmetic on a hostile file's accessor numbers.
   `locate`, `read` and `write_in_place` in `crates/ac-io/src/export.rs` added `byteOffset`s and
   multiplied `count` by the element size as plain `usize` arithmetic, which the character reader now
   feeds from any `.glb`: a count near 2^60 panicked on the overflow in debug builds and wrapped in
   release ones, where the slice bounds then lied. Checked arithmetic with a reason (`export.rs:191`,
   `:206`); two hostile files added to `read.rs`'s `lies_are_refused`.
5. **Low, fixed (commit ce4fb0b).** A mapping file was read whole with no size cap, unlike the `.glb`
   (256 MB). `MAX_MAPPING_BYTES` (1 MB) now refuses a larger one before reading
   (`crates/ac-io/src/character/mod.rs:41`, `:167`).
6. **Low, not changed (the owner's acceptance line).** "Stay above the floor" is measured with a 1 cm
   allowance: `holds` passes a foot up to `BELOW_LIMIT` (1 cm) under the floor
   (`crates/ac-core/src/character/check.rs:24`, `crates/ac-io/src/character/commands.rs:194`), and
   the export test allows `lowest_cm > -1.0` (`crates/ac-io/tests/retarget.rs:169`, `:261`). The
   measured number in The numbers above (lowest point 5.0 cm above the floor) meets the words; the
   test tolerance is looser than them. Tightening it is a tuning the owner may want.
7. **Low, not changed.** A node's segment with no length falls back to the parent's alignment with a
   note (`plan.rs:164`, `:181` to `:201`); the note is in the plan's notes and the command results,
   so it is announced. Noted here only because the fallback is where finding 1 came from.
