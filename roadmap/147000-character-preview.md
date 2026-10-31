# Preview your character while animating

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 146000
- **Source:** 144000 (one rig, retarget on export)

## What
Show the target character in the viewport, driven live through its mapping, next to or over the
mannequin, with frames where the retarget struggles marked on the timeline (`TimelineMarks`):
sliding feet, missed contacts, bones it can't drive.

## Acceptance (agreed 2026-10-30)
With a mapping loaded, the viewport shows both figures in step at 60 fps on the walk clip;
screenshots of side-by-side and overlaid views in `target/shots/<date>/character-preview/`.

## Result
2026-10-30, built. Screenshots under `target/shots/2026-10-30/character-preview/` (the release
app, `--script load.json --screenshot`, Walk 12 with `fixtures/data/tall-character.glb` loaded by
`retarget.load`):

- `beside-three-quarter-frame20.png`: the character 1.2 m to the rig's left of the mannequin,
  both on frame 20, the same leg forward.
- `over-front-frame20.png`, `over-side-frame20.png`: the character over the mannequin; the side
  view shows the two strides in step (the character's legs are 16 % longer, so its hips sit
  higher and its path is 16 % longer: over a travelling walk the two drift apart along the
  path, which is the scaled path doing its job, not a lag).
- `over-three-quarter-frame61.png`: the same later in the walk.
- `skeleton-only-beside-three-quarter.png`: a character file with no mesh
  (`tall-skeleton.glb`, the test character with its mesh and skin removed) is drawn as joints and
  bones.
- `playing-settled-perf-beside.png`: playing the walk looped with the character beside the
  mannequin, after 8 s, the performance HUD on: **144 fps, 6.9 ms a frame, app work 2.5 ms a
  frame** (RTX 3080 Ti, Vulkan). The HUD's rate is the app's own frame timing (`shell/perf.rs`,
  the rate of frames drawn); `playing-perf-beside.png` is the same run's first frames, where the
  HUD still averages the start-up (3 fps): not the steady state.

### What was built
- `crates/ac-app/src/app/character.rs`: the window state (`CharacterState`: the view, the plan
  made for the loaded character, whether its mesh is in the renderer, the IK weights per frame)
  and three hooks: `character_refresh` (every frame, cheap when nothing changed: makes the plan,
  loads the mesh into the renderer's slot, and when the animation changed drives the whole clip
  through `ac_io::character::check_document` for the IK weights and the timeline marks),
  `character_frame` (the pose shown, a Move drag included, driven through the plan: skinning
  matrices for a skinned character, else its bones as lines) and `draw_character` (the lines).
- Rendering: a character whose file has a mesh skinned to its skin is drawn skinned, through the
  renderer's new mesh slot (`ac_render::Renderer::set_mesh` / `clear_mesh`, `Body::mesh`; slot 0
  stays the mannequin; one body a frame per slot; an empty or doubly used slot is an error the
  status line shows). `Mesh::from_glb` now takes every primitive skinned to the first skin, not
  only the first. A character with a skeleton only (no skin, or its mesh fails to load: the reason
  goes to the status line and the character is drawn as bones) is drawn as lines and dots in the
  character's blue, over the picture.
- Timeline marks, source `retarget`: `foot-slide` (a planted foot drifting over 1 cm, in the
  theme's sliding colour), `below-floor` (a foot more than 1 cm under the character's floor, the
  penetration colour), `unreached` (a leg short of its target by over 5 mm) and `undriven` (one
  grey span over the clip naming the nodes nothing drives). Cleared when the character is unloaded
  or the animation has no keys.
- `--ui character=off|beside|over` (default beside), `--ui playing=on|off`, and the View menu's
  "Character preview" (`app.character`), which cycles off, beside, over and says which in the
  status line.
- The document holds only the character and its mapping (`Document::character`, an
  `Arc<ac_core::character::Loaded>`; undoable with `retarget.load` / `map` / `unload`). **Not
  saved in the project file**: a project is self-contained by design (`ac_io::project`: one file
  that can be sent to someone), and the character is a file of its own outside it; the mapping
  file (`retarget.save`) is how a mapping is kept. A later item could reference a mapping file from
  the project if wanted.

### Left open
- The marks share the lane with the physics marks; on Walk 12 the physics pass also marks "Foot
  sliding 1–89", so the `undriven` span lies under it. Hovering tells them apart.
- The character is drawn in one flat colour (its materials and textures are not read; the
  renderer has one shader).
- With `over`, a travelling clip drifts the two figures apart along the path (the scaled path);
  `beside` is the view for walks, `over` for poses and in-place motion.

## Review (2026-10-31)
A code review of the built item (an agent, not the owner). The code checked:
`crates/ac-app/src/app/character.rs`, the viewport's use of it (`crates/ac-app/src/app/viewport.rs:357`
to `:399`), `crates/ac-render/src/renderer.rs` (`set_mesh`, `clear_mesh`, `has_mesh`, the slot
checks in `render`), `crates/ac-render/src/mesh.rs`, `crates/ac-app/src/app/timeline/marks.rs`.

Read through and found right (no change):
- Timeline marks go through the extension point, source `retarget`
  (`crates/ac-app/src/app/character.rs:217`), with the four kinds the Result names; they are
  cleared when the character is unloaded, when the plan can't be made, when the animation has no
  keys and when the check fails (`:131`, `:149`, `:197`, `:222`), and replaced whenever the clip or
  its links change (`:186` to `:194`), so no stale mark survives an edit. `TimelineMarks::set` drops
  marks whose frame is not finite and caps them at `MAX_PER_SOURCE`.
- The mesh slot: slot 1, set once per loaded character and cleared on unload; a mesh whose skin
  joint count differs from the skeleton's is refused and the character drawn as bones with the
  reason in the status line (`:167` to `:181`); an empty or doubly used slot is an error from
  `Renderer::render` shown in the status line (`renderer.rs:469` to `:471`).
- The 60 fps claim: the Result's 144 fps is the app's own frame timing with the character beside the
  mannequin on Walk 12; the drive is one `drive()` call a frame (`:239`) and the whole-clip check runs
  only when the animation changed, not every frame, so the claim is consistent with the code. The
  screenshots the acceptance asks for are named under `target/shots/2026-10-30/character-preview/`,
  which is gitignored: not checkable from this review's worktree.
- `drive()` runs outside the session here, so a panic in it would be a crash of the window: finding
  1 of 146000's review (fixed in commit 3f207ef) applied to this preview too.

Findings:
1. **Low, fixed (commit 4240fc6).** `Mesh::from_glb` took "every primitive skinned to the first skin"
   but removed only neighbouring repeats of a mesh index (`Vec::dedup` on an unsorted list,
   `crates/ac-render/src/mesh.rs:51` before the fix), so a character whose one mesh sits on two
   non-adjacent skinned nodes was drawn twice over itself. The indices are sorted first.
2. **Low, not changed.** The mesh is re-read from disk and re-uploaded after every `retarget.map`,
   because the slot is keyed on the `Loaded` pointer and each correction makes a new one
   (`character.rs:158`). Correct, and cheap for the test character; a large character would pause
   the window on each correction. Keying the mesh on the character file instead is a small follow-up.
3. **Low, not changed.** With no weights for the frame (the check failed, or the clip has no keys)
   the preview drives the feet with full IK (`character.rs:238`, `Weights::FEET`) rather than none;
   the status line says the check failed, so it is not silent, but the fallback choice is noted.
4. **Low, not changed.** The whole-clip check runs on the window thread on every edit of the
   animation (`character.rs:200`); 1.2 ms for 90 frames, so about a second at 100 000 frames
   (`check::MAX_FRAMES`). Fine for clips; a very long one would make each edit pause.
5. The Result's claims match the code: the three views and their switches, the View menu action, the
   skinned and the bones-only drawing, the marks, and the document holding the character outside the
   project file.
