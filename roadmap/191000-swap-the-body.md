# Swap the body everywhere

- **Status:** not started; later (owner, 2026-11-01)
- **Requires:** 190000
- **Source:** owner's request (2026-11-01): lay the foundation for swapping in other meshes on the same rig

## What
The mannequin's path is hard-coded where each front end loads it: `crates/ac-app/src/app.rs:143` and `:471`,
`crates/ac-cli/src/render.rs:22`, `crates/ac-media/src/shots.rs:27`, `crates/ac-review/src/viewer.rs:30` and
`crates/ac-io/src/export.rs:666`.
Replace these with one loader that returns the chosen body, with the mannequin as the default.

- Commands: `body.list`, `body.use`, `body.current`. The choice is a setting and isn't saved in a clip, so
  the same clip plays on any body.
- The viewport, `ac video`, renders, the review app and exports all use the chosen body.
- If the chosen body is missing or fails the check, the app falls back to the mannequin and says so
  (Rule 1).
- The clash check (184000) builds its body parts and capsules from the chosen body.

This step takes bodies with the rig's own lengths. Bodies with other lengths are refused with a clear reason
until 192000.

## Acceptance
- With no body chosen, renders and exports are byte-identical to today's, and the existing goldens pass
  unchanged.
- A fixture body made by a test (the mannequin with its surface pushed out along the normals, same joints)
  appears in the app, `ac video`, the review app and an exported `.glb`.
- A clip saved with one body chosen opens with another chosen, and its file is byte-identical.
- A missing body and a broken body each fall back with a message, without a crash.
- Before and after MP4 of a walk on the fixture body, under `target/shots/<date>/swap-body/`.
