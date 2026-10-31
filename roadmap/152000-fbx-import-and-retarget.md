# FBX import and retarget onto the rig

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** none
- **Source:** owner request (2026-10-30)

## What
Bring a humanoid FBX animation onto the rig through a command, so it works from the CLI and MCP as
well as the app. Today `clip.import` reads `.clip.json` only (`crates/ac-io/src/commands.rs:509`),
and the retarget exists only in Python (`training/ac_train/retarget.py`), which can't run in the app.

- **Reader:** our own Rust reader for binary FBX (7.x, compressed arrays included). Input is hostile:
  caps, finite checks, and a refusal that says what is wrong, never a panic. The file's
  `UpAxis`, `FrontAxis` and `UnitScaleFactor` are read, not assumed.
- **Retarget:** port `retarget.py` to Rust, pinned to the Python results by goldens. The source's world
  positions give the landmarks, so it doesn't matter which bone carries the motion.
- **Joint map:** guessed from bone names, correctable by parameter. A missing landmark is a
  refusal that names it.
- **Direction:** the rig is Z up and faces -Y.
- **Root:** the rig's root is the hips, stored as an offset in hip heights. As in `retarget.py:222`, the
  hip joints land on the source's, because the rig's root sits about 11.5 cm above them. The floor
  moves to z = 0.
- **Commands:** extend `clip.import`, plus a batch command that writes a folder of `.clip.json`.
- **Training:** a set reader for those `.clip.json` files, so there is one importer and no second
  FBX reader in Python. Training ignores in-place clips: they teach the model that feet slide.

Licences: test files are FBX we generate ourselves or animations the owner owns. A purchased pack
may forbid training, whatever the tool can read; store packs stay out of `training/` unless their
publisher has consented in writing.

Fingers, face and props have no landmark: they stay at rest, and the result says so (Rule 1).
Related: 146000 is the other direction (export onto a character) and can reuse the name guessing.

## Acceptance
- A fixture imports a generated FBX through `clip.import` and through `ac`, and an MCP test does the
  same.
- Direction: a clip walking forward travels -Y; a clip stepping left moves to the rig's left; the left
  and right feet are not swapped; a Y-up and a Z-up file of the same motion give the same result.
- Root: the rig's hip joints stay within 1 cm of the source's (scaled) on every frame, and the feet
  rest on z = 0 where the source's rest on its floor.
- The Rust retarget matches the Python goldens within 1e-4 on a fixed input.
- Corrupt, truncated and non-humanoid files are refused with a reason, with a regression test for each.
- `training/` can build a set from a folder of imported clips, and the scan (`ac_train.quality`) runs on it.
- The owner approves a contact sheet of one of their own FBX files beside the source.

## Result (2026-10-30, built; the owner's look is still to come)

**What was built**
- `ac_io::fbx` (`crates/ac-io/src/fbx/`): `binary.rs` reads the binary container (32-bit records
  and the 64-bit ones of 7.5+, the typed properties, raw and zlib arrays through `miniz_oxide`,
  which was already in the lock); every count, offset and length is checked and a file that lies
  is refused with the byte it lies at. `scene.rs` takes the models, their `Properties70`
  transforms (translation, rotation with its order, pre/post rotation, pivots, offsets, scale),
  the first take's curves, and samples every frame into world joint positions in the rig's axes
  and metres; `UpAxis`, `FrontAxis`, `CoordAxis` and `UnitScaleFactor` are read (a file without
  them, or left-handed, is refused), the frame rate comes from `TimeMode`/`CustomFrameRate` or
  the keys' spacing, and the result says which. `import.rs` guesses the landmarks
  (`ac_core::jointmap`), drops setup frames, checks the sides (toes and bent knees against the
  hip line), scales, retargets, levels the floor and reports.
- `ac_core::retarget`: the port of `retarget.py` (and `scale_to`, `level`), pinned to the Python
  by `crates/ac-core/tests/golden/retarget-walk.json`: rotations within 2.6e-8 and root offsets
  within 9e-11 of the Python on the same landmarks (the acceptance asked for 1e-4).
  `ac_core::jointmap` is the name guess on its own (146000 reuses it): words from the names,
  sides, the tree deciding `Leg`, the hips as where the thighs meet, the spine as the hips'
  child towards the neck.
- Commands: `clip.import` reads `.fbx` (new `joints` correction, and the result gains `fps`,
  `frames`, `joints`, `given`, `unused`, `at_rest`, `scale`, `floor`, `hips`, `notes`,
  `source`); `clip.import_batch {from, to, joints}` writes a `.clip.json` per file.
- Training: `ac_train.datasets` reads a folder of those clips as a set (`Dataset(folder=...)`,
  `clips_set`, `python -m ac_train.datasets --clips <folder> --name myclips [--commercial]`),
  skips in-place takes (`in_place`: the feet step but the hips stay) and the scan runs on the
  set (`training/tests/test_clipset.py`). No FBX reader in Python.
- Test files: `training/ac_train/fbxwrite.py` writes `fixtures/data/walk-yup.fbx` (Y up, cm,
  7.4, a plain Reference node, pre-rotations and three rotation orders on some joints, twist and
  roll joints, end sites, a thumb and a prop), `walk-zup.fbx` (Z up, metres, uncompressed),
  `walk-yup-75.fbx` (7.5, 64-bit records), `sidestep-left.fbx` and `not-humanoid.fbx`, all from
  the built-in Walk 12 / Side step 9 (100STYLE, CC BY 4.0) through the rig's own FK. Checked
  against an outside importer (`tools/blender_fbx_joints.py`, Blender 5.1 headless): its joint
  positions match the landmarks the files were written from to 1.05e-5 m (f32 in cm). That
  check found and fixed two things in the writer: a `bool` property in `Properties70` is an
  `I`, and `RotationOrder` names which axis turns first while the angles stay (x, y, z).

**Numbers (fixture `fbx-import`, ac-io test `fbx.rs`, physics off)**
- Walk: root frame 0 (0, -0.0098, 0.8664) m, frame 39 (-0.0039, -1.0802, 0.8698): travels -Y
  1.07 m. foot.L frame 0 at x +0.130, foot.R at x -0.114: left is the rig's left, feet not
  swapped. Y-up, Z-up and 7.5 files: the same keys within 1e-4 (rotations and roots).
- Side step: root x 0.000 -> +0.301 over 30 frames (the rig's left).
- Hips: the midpoint of the hip joints lands on the source's within 2.4e-10 m on every frame;
  each hip joint within 5.9 mm (the pelvis's tilt is estimated from the spine landmark, so the
  hip line can turn a little; the acceptance's 1 cm holds). Floor moved 2.3 mm to z = 0; the
  feet then rest on z = 0 (`floor_offset` 0 after levelling). Ankles and wrists within 2 mm of
  the source's (the two-bone solve reaches them).
- Refusals, each with a test (`crates/ac-io/tests/fbx.rs`, `binary.rs` unit tests): truncated
  (half a file, a cut header), corrupt (an end offset past the file, a property list that lies,
  an unknown property type, an array that does not inflate), not FBX (text FBX is named as such),
  no animation, non-humanoid (names the missing landmarks and the `joints` fix), a correction
  naming a joint that is not there or a landmark that is not one, swapped sides.
- MCP: `crates/ac-cli/tests/mcp.rs::an_fbx_imports_through_mcp` imports through `command_run`.

**The owner's own files.** Under `D:/CreativeFusion` the only FBX that are the owner's own
work (`com.cf.humanoid`'s DefaultHumanoid: `Humanoid.fbx`, `Base-Humanoid.fbx`, `Armature.fbx`;
`AnimationSandbox/Masculine_TPose.fbx`) hold no animation: the import says so
("no animation curves (a character in its rest pose, or an empty take)", "only 1 frame(s) of
motion"). The animated FBX there are purchased packs (Dynamic Archer Set, UAL, AL) and were not
used. So the review material is from the generated files, beside the source as an outside
importer reads it:
- `target/shots/2026-10-30/fbx-import/walk-yup-source-vs-imported-side.mp4`: left, the source
  file's joints as Blender's importer places them (stick figure, fixed camera); right, the
  imported clip (`ac video --view side --fixed`, keys, physics off). Look at the leg phase at
  0.3 s and 1.0 s and the travel to the right (-Y): both match.
- `sidestep-left-source-vs-imported-front.mp4`: the same for the side step; the body goes to
  its left (+X, the viewer's right from the front).
- `walk-yup-contact-sheet-source-vs-imported.png`: frames 0, 10, 20, 30, 39 (the source fixed
  camera, the app's strip one camera per frame).
- `walk-yup-imported-front-keys.mp4`, `walk-yup-imported-side-keys.mp4`,
  `sidestep-left-imported-front-keys.mp4`: the imported clips alone.
To review a file of your own: `ac --assets assets run clip.import '{"path": "<file>.fbx"}'`
(or `clip.import_batch`), then `ac video`; `tools/blender_fbx_joints.py` draws the source
side (`target/draw_source.py` was a throwaway for the stick figures).

**Decisions made here (no owner question asked)**
- Keys land on every frame with linear interpolation, the file's own frame rate kept.
- Physics stays on for an imported clip as for any clip; the checks turn it off.
- Only the first take and its first animation layer are read; the result says when a file has
  more. Curves are read with straight lines between keys (a note says when keys sit between
  frames). A scale-inheritance type other than the default is read as the default and said.
- The retarget keeps every frame of every bone in memory, so one import takes at most 20,000
  frames (11 minutes at 30 fps); a longer take is refused and told to be cut.
- `PreRotation` counts as active when present, even without `RotationActive` (some writers
  leave the flag out).
- The clips set built with `--clips` is non-commercial unless `--commercial` is given (your own
  work may train the shipped model; a pack's may not).

**Left open**
- The owner's look at the videos, and an FBX of their own with an animation in it when they
  have one (the review material above is from the generated files).
- Scale inheritance types other than the default and animation layers beyond the first are not
  blended (said in the result).
