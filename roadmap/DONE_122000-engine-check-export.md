# Engine check of the export

- **Status:** done (2026-10-31), by the owner's decision: Blender only; Unity moved to 162000
- **Requires:** 106000
- **Source:** old `ROADMAP.md` row 3

## What
Load exported .glb files in Blender and Unity.

## Acceptance
Three clips in each of the three bone profiles play in both, with the same poses as the app;
screenshots in `target/shots/<date>/<topic>/`.

## Result (2026-10-30)

Blender half done; Unity half not done (Unity is not installed on this machine and the disk is
nearly full, so installing it is the owner's call: see Left). As extra evidence every file was run
through the Khronos glTF validator.

Clips: `Locomotion/Walk` (33 frames), `Action/Jump` (71), `Gestures/Wave` (95), each exported with
`clip.export` in the profiles `full`, `game` and `body` (`crates/ac-io/src/export.rs`; the
rig's lists in `assets/rig/default_rig.json`). Blender 5.1.2, headless, with its bundled glTF
importer, Workbench renders, scene at 30 fps; the app's own strips from `ac render --frames 0,8,15
--strip` (front and left views). Frames compared: 0, 8, 15 of each clip.

| File | Bones | Frames in action | fps | Validator errors / warnings | Blender import warnings | Pose match |
|---|---|---|---|---|---|---|
| walk-full.glb | 104 | 33 (0–32) | 30 | 0 / 0 | none | yes |
| walk-game.glb | 68 | 33 (0–32) | 30 | 0 / 0 | none | yes |
| walk-body.glb | 36 | 33 (0–32) | 30 | 0 / 0 | none | yes |
| jump-full.glb | 104 | 71 (0–70) | 30 | 0 / 0 | none | yes |
| jump-game.glb | 68 | 71 (0–70) | 30 | 0 / 0 | none | yes |
| jump-body.glb | 36 | 71 (0–70) | 30 | 0 / 0 | none | yes |
| wave-full.glb | 104 | 95 (0–94) | 30 | 0 / 0 | none | yes |
| wave-game.glb | 68 | 95 (0–94) | 30 | 0 / 0 | none | yes |
| wave-body.glb | 36 | 95 (0–94) | 30 | 0 / 0 | none | yes |

Bone counts equal the profiles' sizes (full 104, game 68, body 36) and the bone names are the
rig's names in rig order. The validator reports three infos per file, all `UNUSED_OBJECT`
(`TEXCOORD_0`, accessors 3 and 7: the mannequin's texture coordinates, which no material uses);
no errors, no warnings. Blender printed nothing on import; its importer adds a bone-shape mesh of
its own ("Icosphere"), which is not in the file.

What was checked on the comparison images (`compare-<clip>-<profile>.png`: the app's front strip,
Blender front, the app's left strip, Blender left, frames 0, 8, 15 in columns):
- **Same poses** in all 27 frame pairs: the walk's leg lead and arm swing, the jump's crouch
  deepening at 8 and 15, the wave's right arm up at 15 and bent at 8, the head and hands.
- **Up axis**: Z up in Blender in every file (the importer's Y-up to Z-up conversion of the
  exporter's rig to glTF axes); the body stands on the floor plane, faces -Y as in the app.
- **No flipped sides**: the raised arm in the wave is the right arm in both (screen left in the
  front view); the left-view silhouettes face the same way (camera on +X in both; the app's
  `left` view is yaw pi/2, `crates/ac-render/src/offscreen.rs`).
- **Root motion present**: the root's animated translation in Blender equals the app's root
  offset (`pose.get` after `frame.set`, in hip heights x 0.8698 m) plus the root's rest
  translation (z + 0.0504 m), to 0.1 mm: jump frame 0 -> 15 drops 0.1101 m in both, wave frame 15 x
  = -0.0088 m in both, walk frame 8 x = +0.0132 m in both. The three profiles of a clip give
  identical root positions. The built-in Walk is in place (root x within 2 cm), so travel was
  checked through the jump's crouch and the wave's sway, not a walk's advance.
- **Feet on the floor**: the app's `clip.metrics` reports 0 frames of ground penetration in all
  three clips; Blender's lowest skinned vertex sits 0.7–2.7 cm below z = 0 at the sampled frames,
  which is the toe mesh under the joint-measured floor, the same as the app's own render (soles on
  the grid line), not an export offset.

No mismatch found in any profile or frame.

Screenshots and data: `target/shots/2026-10-30/engine-check/` (gitignored): `glb/*.glb` and
`glb/*.validation.json`, `blender/<clip>-<profile>-<view>-<frame>.png` and
`blender/reports.jsonl` (one JSON line per file: bones, action range, fps, root and lowest-vertex
samples, import warnings), `app-<clip>-<view>.png`, `compare-<clip>-<profile>.png`.

To repeat (from the repo root; `ac-copy.exe` is a copy of `target/release/ac.exe` so the check
does not collide with a rebuild; `export.json` lists the nine `clip.export` steps,
`open-<clip>.json` one `clip.open`):

```
ac --assets assets script target/shots/2026-10-30/engine-check/export.json
ac --assets assets render --script open-walk.json --frames 0,8,15 --strip --view front --size 400x500 --out app-walk-front.png   # and --view left; jump, wave
cd target/shots/2026-10-30/engine-check/validator && npm install gltf-validator@2.0.0-dev.3.10 && node validate.js ../glb/*.glb
"D:\Installations\Blender 5.1\blender.exe" --background --python tools/blender_check_export.py -- <glb> target/shots/2026-10-30/engine-check/blender/<clip>-<profile> 0,8,15
cd training && uv run --with pillow python ../target/shots/2026-10-30/engine-check/compose.py <clip> <profile>
```

`tools/blender_check_export.py` is our own code against Blender's Python API: it imports the
file, reports the armature's bones, the action's frame range and the fps, places a front and a
left camera, renders the given clip frames with Workbench (`eevee` as an optional fourth
argument) and prints one `BLENDER_CHECK {json}` line with the importer's warnings. The npm
`gltf-validator` package is a library without an executable, hence the four-line `validate.js`.

## Left

- **Unity** (the second half of the acceptance): not installed on this machine, and installing it
  is the owner's decision (disk space is nearly out; a Unity account and licence are needed). When
  it is: make an empty 3D project, drop the nine `glb/*.glb` files in (Unity 6 imports glTF
  through the glTFast package: Window > Package Manager > Unity Registry > "glTFast"), put each
  prefab in a scene with an Animator playing its one clip, step to frames 0, 8, 15 (time 0, 8/30,
  15/30 s) in front and side views, screenshot into
  `target/shots/<date>/engine-check/unity/`, and check the same four things as above (poses against
  `app-<clip>-<view>.png`, Y-up import standing on the ground, no mirrored sides, root motion on
  the root bone with Animator "Apply Root Motion" on). Record the result in this file and set the
  status to "built, waiting for the owner".

## Closed
The owner's decision, 2026-10-31: Unity is left for now and moves to its own item, 162000, with the steps under `## Left`. This item closes on the Blender half.
