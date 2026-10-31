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
