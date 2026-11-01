# The floor hides the body: the mesh goes, the joints stay

- **Status:** not started
- **Requires:** none
- **Source:** owner's bug report (2026-11-01); reproduced the same day

## What
The body vanishes while its joints and bones still show. The floor is the cause: it is a solid,
two-sided quad at z = 0 that writes depth (`crates/ac-render/src/renderer.rs:278`, `fs_floor` in
`shader.wgsl` is opaque), so anything on the other side of it from the camera fails the depth test.
The joints are painted over the finished picture by egui, so nothing hides them.

Two ways to reach it, both reproduced:

- **The camera goes under the floor.** Orbit allows pitch down to -1.4
  (`crates/ac-app/src/app/viewport.rs:140`), but with the default target and distance the eye passes
  z = 0 at pitch -0.25. From the default view that is a right-drag of about 54 px upward. Driving
  the app, a 60 px drag gave pitch -0.30 and no mesh, with every joint still drawn. The Bottom view
  (Ctrl+7, pitch -1.45) always does it: `ac render --view bottom` draws only the floor.
- **The body goes under the floor.** With `pose.set_root` at `[0, 0, -1.3]`, `ac render --view front`
  draws only the floor. A sunk root from a clip or the Move tool does the same.

The fix: the floor stops writing depth. It is drawn first, so it needs none to sit behind the
bodies. The camera may still go under it, and the body is then drawn over it. A sunk body shows
through instead of disappearing, which also fits Rule 1. `clip.picture`, `clip.video` and the other
offscreen pictures share this renderer, so they change too.

## Acceptance
- A new fixture renders the Bottom view and a body with its root at z = -1.3: the body is in both
  pictures (the visual golden is looked at, then blessed).
- In the app, a right-drag from the default view to pitch -0.3 keeps the mesh on screen
  (`tools/drive_ui.py` gets this check, with a screenshot).
- Feet near the floor look right from a normal view: the changed visual goldens are looked at and
  the blessing is named in the commit.
- `cargo xtask check` passes.
