# AI evaluation suite

- **Status:** not started
- **Requires:** 103000
- **Source:** 115000 (review 1 §3, review 2 §10–11)

Do this first: it shows which of 128000–136000 matters most before any retraining.

## What
Extend `evaluate()` (`training/ac_train/train.py:204`, today mean joint error at gaps 5/15/30) and
the model card with:
- **Gap bins** 1–4, 5–10, 11–20, 21–30, 31–45 (45 itself included).
- **Pose:** joint-position error, root error at mid-gap, per-bone rotation error.
- **Feet:** foot slide on planted frames (the `skate` test in `training/ac_train/quality.py`),
  floor penetration.
- **Continuity:** velocity jump at both ends of the gap (seam velocity), jerk.
- **Possible:** frames with a joint past the `possible` limits (the same maths as
  `crates/ac-core/src/possible.rs`, through `training/ac_train/ranges.py`).
- **Breakdown** by motion type and style; a fixed "hard" slice (foot-plant changes, direction
  changes).
- **Realistic inputs**, beside today's clean ones: context rebuilt from sparse keys with
  `training/ac_train/clip.py` (smooth, linear and step), a still context (the clip start:
  `crates/ac-ai/src/lib.rs:296-297`), and the masks the app sends for part layers (130000).
- A fixed set of hard fills rendered the same way for each model version (contact sheets with
  physics off), for side-by-side review.

## Acceptance
`python -m ac_train.evaluate --tag parts` (or the same through `train`) writes all of the above
into `model.json` for the shipped model, plain interpolation beside it; a pytest covers the new
metrics on a tiny synthetic window with hand-worked numbers.
