# HumanMotionCraft roadmap

What the app does today and what comes next. Rules for agents live in `AGENTS.md`. Licences and
sources live in `ATTRIBUTION.md` and `NOTICE`.

A row under **Next** is `[ ]` until it is built. Mark it `[~]` when it is built and its images are
ready, and `[x]` only after the owner confirms. Every row has an acceptance line. If a row has none,
propose one and wait for the owner to agree before building it.

## Built (2026-10-29)

| Area | What works |
|---|---|
| Rig and posing | 104-bone default rig with per-bone limits and the CC0 mannequin; FK; analytic two-bone IK; full-body damped least squares IK (`pose.reach`, `pose.drag` with pins); mirror, copy/paste, pins, locks, part sets; the rest pose stands on the floor |
| Clips and keys | keys with 6 interpolations, override/additive layers, links between clips; `key.*`, `keys.retime` (insert, delete, scale), `keys.copy/paste` (mirror), `key.move_parts` (dope rows), markers, `feet.plant`, `clip.move` (slide or turn the whole animation) |
| Physics | support, contact, fulcrum (planted feet don't slide), static balance, ballistic flights; applied in playback and export when the switch is on; `physics.solve` reports problems; overlay with support area, centre of mass and jump arcs |
| AI in-between | `ac-ai`: our own `parts` network run in plain Rust (f32 transformer, no ML runtime), trained by `training/` on the free-licence sets only: CMU, 100STYLE, our Kimodo-generated motion and the CC0 loops (2.2 cm mean joint error over 30-frame gaps on their held-out takes). 33 AI styles and 14 motion types. Canonical frame, 6D rotations, style guidance, `ai_force`, trajectory, head, given bones for part layers. `ai.fill`, `ai.fill_all`, `ai.preview/accept/discard`, `ai.gaps`, `ai.toggle`; edits refill the AI spans they touch; feet planted after every fill. Matches PyTorch: network within 1e-4, fills within 0.04 degrees |
| Travel | `layer.travel` / `layer.plan`: motion matching over the Locomotion mocap, feet held with leg IK; scene boxes (`surface.*`) |
| Library | your animations, built-ins to watch and learn from, poses; 40 CMU, 104 100STYLE and 85 generated clips (free-licence only; a walk per style in `Walk styles`); samples; import a folder; search; contact-sheet thumbnails; `library.install` for clips baked locally |
| Export and media | rigged .glb (profiles full/game/body or a bone list) with a round-trip test; `clip.export_batch`; MP4 and GIF (`clip.video`), contact sheets (`clip.sheet`) |
| App | wgpu GPU-skinned viewport with Drag, IK, Move and rotate tools, ghosts, onion skin, trails, ortho presets; timeline with zoom, snap, box-select, dope sheet, AI bands, curve editor; inspector, layers, physics and AI panels; menus and keymap from the registry, command palette, undo history, settings, themes, autosave and recovery, welcome and tips, block-out wizard |
| Agents and tools | everything is a command; `ac` CLI (`run`, `script`, `fixtures`, `render`, `video`, `samples`, `new-command`); MCP over stdio and live against the running app; `edit.batch`, `edit.dry_run`, checkpoints, `clip.metrics`, `clip.critique` |
| Tests and gates | a fixture for every command, visual goldens, parity goldens, `cargo xtask check` (fmt, clippy, nextest, fixtures, name/asset/doc guards), the same gate in CI; training pytest |
| Training | `training/`: dataset registry and fetch, retargeting, quality scan, the `ac-review` app, training, weight export, library clip cutting, build stamps |

## Next

1. [ ] **AI fill over long gaps.** Today `ai.fill` refuses gaps over 45 frames. Place anchor poses
   from the mocap library inside the gap, then fill the shorter spans between them.
   *Acceptance:* a 90-frame gap between a stand and a walk fills in one undo step; the anchors are
   the same on every run; a fixture checks the anchor frames and that the feet stay planted.
2. [x] **Train on 100STYLE.** Cut library clips from it and retrain `parts` with it (CC BY 4.0, so
   its clips can ship).
   *Acceptance:* the new model is no worse on held-out takes at 15, 30 and 45 frames; the card in
   `weights.json` lists the set; the `ac-ai` goldens are regenerated.
   *Result (2026-10-29), accepted by the owner after testing:* retrained on the free-licence sets
   only (open decision 2). Mean joint error at 15 / 30 / 45 frames on the free sets' held-out
   takes: 1.17 / 2.18 / 3.33 cm (the previous model 1.57 / 3.06 / 4.90; plain interpolation
   3.73 / 7.79 / 11.51). The card lists 100STYLE; the goldens are regenerated.
3. [ ] **Engine check of the export.** Load exported .glb files in Blender and Unity.
   *Acceptance:* three clips in each of the three bone profiles play in both, with the same
   poses as the app; screenshots in `target/shots/<date>/<topic>/`.
4. [ ] **Project files.** Save and open several clips, the scene boxes and the layers as one
   versioned project file.
   *Acceptance:* save, close and open gives a byte-identical document; an older version loads
   through a migration with a fixture.
5. [ ] **Packaging.** A release build: one `.exe` with its assets, ready to download.
   *Acceptance:* the zip runs on a clean Windows machine with no Rust, Python or ffmpeg installed
   (video export then explains what is missing).
6. [~] **Install motion from text from the app.** The Motion from text panel lists what the
   install needs (`text.install` checks the source tree, `uv`, Git, the C++ build tools and the
   GPU), names the licences installing accepts, and Install runs `setup_textmotion.py` in the
   background (through `uv run`), with its numbered step, Stop and the log (owner's decision,
   2026-10-29). The setup stops when the app closes its stdin, and starting again resumes. It
   needs the source tree (the setup copies training's rig maths), so packaging (row 5) must ship
   `tools/textmotion` and those modules with the app.
   *Acceptance:* on a machine without the install, the panel lists what's needed and Install runs
   the setup with step progress and Stop; after Stop, Install resumes; when it finishes the panel
   offers Generate without restarting the app; `fixtures/text-install.json` passes; screenshots
   of each state.
7. [ ] **Fill speed.** *Acceptance:* one 30-frame AI fill takes under 50 ms in a release build, as
   measured by `ac fixtures --timings`.
8. [x] **Motion from text.** An optional helper (`tools/textmotion/setup_textmotion.py`, Python,
   installed outside the repo; owner's decision 2026-10-28) makes motion from a prompt:
   `text.generate` writes a new stretch from the playhead, `text.fill` fills between two of your
   keys held to both, `text.helper` loads or stops it. The keys are AI keys tagged with the prompt,
   which edits next to them leave as they are and say so. The window makes the motion on a thread,
   and the app runs as before without the install.
   *Acceptance:* with the install, "A person waves hello with their right hand." fills a 2-second
   gap between two standing keys and generates 3 seconds from the playhead, and the contact sheets
   show the wave meeting both keys; without it, the panel and every `text.*` command say how to
   install it; `fixtures/text-motion.json` and `crates/ac-textmotion/tests` pass.

## Open decisions (the owner's)

1. Custom rigs: retargeting onto your own humanoid skeleton, or only the built-in rig.
2. Decided (2026-10-28), done (2026-10-29): `parts` itself is retrained on the free-licence sets
   only (CMU, 100STYLE, the Kimodo-generated set, the CC0 loops). The repo supplies only these;
   a non-commercial set is one a user adds and trains on alone (`--noncommercial`, the app's own
   models folder).
3. The minimum GPU feature level for wgpu (storage buffers are needed, so there's no WebGL2 target).
