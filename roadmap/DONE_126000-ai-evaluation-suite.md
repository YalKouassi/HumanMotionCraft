# AI evaluation suite

- **Status:** done (2026-10-30)
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

## Result
2026-10-30. `training/ac_train/evaluate.py` (with `kinematics.py` for the body measurements in torch,
checked against the numpy rig and `anatomy.py` in `tests/test_kinematics.py`). `python -m
ac_train.evaluate --tag parts [--checkpoint best] [--sheets]` writes `evaluation` into the card;
training writes it for the last and the best checkpoint. Every metric in What is there, per gap bin,
per context (clean, smooth, linear, step, still), by motion type and style, on the hard slice
(plant change, turn) and per part layer (each part group's complement given, scored on the free
bones, against a plain fill that keeps the given bones as the app does). `--sheets` draws six fixed
hard fills (recording, model, interpolation) with physics off into
`training/data/models/<tag>/sheets-<checkpoint>/`. `tests/test_evaluate.py` works the numbers by
hand on a 5-frame window (a fill stepping off planted feet, a fill sinking, a turn).

The shipped `parts` model, on the held-out subjects (512 windows a bin; it was trained with the
old per-take split, so it may have seen some of these subjects' takes, and the card says so):

| | model | plain interpolation |
|---|---|---|
| joint error, gaps 1–4 | 5.5 mm | **4.1 mm** (the model is worse: 128000) |
| joint error, 5–10 / 11–20 / 21–30 / 31–45 | 6.2 / 11.8 / 17.9 / 26.8 mm | 16.8 / 37.3 / 61.8 / 96.1 mm |
| joint error, context clean / smooth keys / step keys / still | 13.6 / 17.6 / **56.8** / **45.6** mm | 43.2 mm (129000) |
| seam velocity A / B, clean context | 0.14 / 0.18 m/s | 0.35 / 0.35 m/s |
| seam velocity A / B, step keys | 0.52 / 0.44 m/s | |
| foot slide on planted frames | 0.051 m/s | 0.18 m/s |
| jerk, gaps 1–4 | 224 m/s³ (the recording: 80) | |
| impossible frames per 1,000 | 0 on every context | 0 |
| part layers, legs / arms (free bones) | 18.5 / 7.0 mm | 37.8 / 8.5 mm |

Score (the model's share of the interpolation's error over the composite, lower is better): 0.494.

**Posed keys** (added 2026-10-30, for 136000): keys A and B are two of the 50 built-in hand-made
poses, each stood on the floor by its lowest foot point, A held still before the gap and B a step
away, turned up to 30 degrees (64 windows a bin). Nothing was recorded between them, so only
impossible frames, floor sinking and jerk are measured. The shipped model, all bins: impossible
**17** per 1,000 (interpolation 20), floor 60.1 mm (66.1), jerk 752 m/s³ (680). This is where the
app's impossible fills come from: the recordings give 0.

**Root path** (added 2026-10-30, for 141000): on the clean windows, the root put on the recorded
path smoothed over 9 frames, as `trajectory` 1 does. `blend` is the fill moved onto it afterwards;
`given` is a model trained with the path also given it, beside `blend`.
