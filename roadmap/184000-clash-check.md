# One clash check for the app, the evaluation and the data

- **Status:** owner's decision first: how far the mesh goes (below)
- **Requires:** none
- **Source:** limb clash analysis (2026-11-01); owner asked for it to be filed

## What
Nothing in the repo checks whether the body goes through itself. Every "penetration" in
`crates/ac-core/src/metrics.rs` and `critique.rs` means the floor. All animation is on one rig and one
mesh (`assets/rig/mannequin.glb`: 14,898 vertices, 27,420 triangles, 104-joint skin), so one check can
serve everything.

Prototype, not in the repo (gitignored `target/analysis/2026-11-01/limb-clash/`, `cargo clean` loses
it): capsules built from the mesh's skin weights, 132 non-adjacent pairs, grouped as arm-arm,
arm-head, arm-leg, arm-torso, leg-head, leg-leg and leg-torso. Pairs already touching at rest get an
allowance. Lines: under 2 cm touching, 4 cm clash, 8 cm deep.

Checked against the skinned mesh with a winding-number test:

- 4 cm or more on the capsules is reliable: the hand is inside the mesh in 100% of cases (median 4.5
  cm), the forearm in 100% (median 3.9 cm).
- Shins read high (median 1.7-3.8 cm inside), so their radius needs trimming.
- Hand overlap of -1 to 1 cm is right only 50% of the time, because capsules miss the fingers.

A nearest-normal inside test gave false positives near the torso. Don't use it.

Uses: metrics, critique, timeline marks, evaluation (126000), a scan of the training sets, and the
checks in 186000 and 187000. Export onto other characters is not measured.

## Finding (2026-11-01)
Other bodies are planned (189000 to 194000). Generate body parts, capsules and allowances from a body's mesh
and skin weights by a tool, not by hand for the mannequin alone, so that the same check runs on any body.

## Question
Owner's question (2026-11-01): since there is one mesh, should there be a clash detection mesh system
for easier diagnosing? Capsules are enough for the 4 cm line, but they can't judge light contact or
fingers, and can't show where the clash is on the body. Decide how far the mesh goes before building.

## Acceptance
- Rust and `ac_train` give the same overlap to within 1 mm on a shared fixture of poses.
- A command reports clashes for a clip as JSON (pair, frames, deepest overlap), and the timeline marks
  them.
- Each report separates clashes the keys already have from clashes the motion between them makes.
- The 4 cm line is checked against the mesh again on the final shapes, at 95% or better for hand,
  forearm and shin.
- The check runs on the training sets and writes a list of the worst takes.
