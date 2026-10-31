# Secondary motion

- **Status:** done (2026-10-31)
- **Requires:** 117400
- **Source:** 115100

## What
Arm swing, shoulders, pelvis, spine counter-rotation, head stabilisation, hands, and follow-through
when movement stops. The network solves a group of bones, while fingers, toes, twist bones and the
head keep the keys' spline (`crates/ac-ai/src/lib.rs:7-8`), so a fill has no head stabilisation
and no hand follow-through.

## Decision
Route (a), the owner, 2026-10-31. The question was: (a) a separate, visible command that writes a layer the person can mute, or (b) more
bones in the model, which goes with a retrain and changes the network's size.

Why (a): it never overwrites manual keys, works on hand-keyed clips without AI,
and is deterministic. It is a separate feature from AI Fill and never runs inside it.

## Acceptance
Agreed by the owner, 2026-10-31: one undo step; muting the layer gives back the clip
exactly; no key of the person's changes; side-by-side videos of a hand-keyed walk for the owner.

## Result
Written 2026-10-31 in a worktree that doesn't build: its tests have not run yet and no video exists
yet. The integration build runs them and makes the videos.

`layer.secondary {amount?, parts?}` (Layer menu, and a button in the layers panel) adds one additive
layer named "Secondary motion" with a key on each frame, worked out by `ac_core::secondary` from the
motion below it; running it again replaces that layer. It is deterministic, needs no model, and AI
Fill never calls it.

- Acceptance: `crates/ac-cli/tests/secondary_motion.rs` checks one undo step, that muting gives the
  same samples bit for bit, and that the base keys, the length and the root's path stay.
  `fixtures/secondary-motion.json` checks that a still clip gets identity turns.
- A turned pelvis moves the hips, so the legs follow by IK with the ankles back in place. The command
  reports how far an ankle stays off (`foot_drift`) and says so past 1 cm.
- The sizes (arm 20 degrees, pelvis 6, spine 6 and so on, the constants in
  `crates/ac-core/src/secondary.rs`) are judged, not measured: the owner's eye decides them.
- Limits: it adds to arms already keyed swinging, so such a clip takes `amount` down or leaves `arms`
  out of `parts`. The layer doesn't follow later edits, as its note says. The head can only correct the
  trunk's sway, not a bob of the whole body.
- Videos: `tools/review/secondary-motion-video.sh <folder>` makes them (hand-keyed walk, physics off;
  side, front, three-quarter, and the stop at quarter speed).

## Review videos (2026-10-31)
The first videos used `anim.steps` with `fill: none`, whose feet dragged; the owner saw it. The layer didn't cause it: the sampled feet match with and without the layer. Remade on `anim.steps`' default walk, with planted feet and about 19 cm of lift. That walk's arms already swing, so the layer adds to them; the acceptance's hand-keyed walk is not what these videos show.

## Closed
Accepted by the owner on the remade videos (2026-10-31).
