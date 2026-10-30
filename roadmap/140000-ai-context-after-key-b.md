# Context after key B

- **Status:** built (2026-10-30); its acceptance is measured by the next retrain
- **Requires:** 134000
- **Source:** 115000 (review 1 §2A)

## What
The window is context frames ending on key A, the gap and key B (`training/ac_train/model.py:7`);
nothing after B, so the model knows B's pose but not how the motion leaves it. Give a few frames
after B, falling back to one side at the clip's end. Try it at the same total window (56 frames,
`model.py:36`) by trading left context for right.

This changes the input contract: `crates/ac-ai/src/lib.rs` (window and masks), `infer.py`, the card
and the goldens change together.

## Acceptance
Seam velocity into B improves in 126000, joint error is no worse, and the parity goldens pass.

## Result
2026-10-30. Built as a model-card option, so the shipped model's inputs are as they were, bit for
bit (its goldens pass unchanged); a model trained with the option uses it. No model has been trained
with it yet (the owner trains overnight): the acceptance line is measured by that retrain.

- **The card:** `context` is the frames before and on key A, and `context_after` (new, default 0)
  the frames after key B; they add up to the 10 the window always had, so the window stays 56
  frames. `ac_train.train --context-after N` writes both (`context` = 10 − N).
- **The window** (`ac_train.model.layout`, `train.prepare(after=N)`, `infer.window_sides`;
  `ac_ai::window` in Rust): N frames after B, known, with a signed time to go (negative after B,
  so the network can tell them from B; the old window never has a frame past B, so its values are
  as they were). At the clip's end the window falls back to the frames before A: `after` is
  what the clip still has (`min(N, last frame − B)`, every frame for a loop) and the frames
  before A make up the number. Training does the same for a fifth of the batches
  (`train.AFTER_END`): fewer frames after B, drawn 0 to N−1.
- **Context after B is made like the context before A** (`ac_train.context`, `b=`): sparse keys
  through the window's end with the app's interpolation (the last frame a key 0.7 of the time,
  else B's last key holds, as the app holds a clip's last key), key B held still, or clean; noise
  on the same share. The evaluation, `ac_train.flag` and the contact sheets lay windows out as the
  model's card says (`evaluate.window_of`).
- **Tests:** `training/tests/test_window.py` (default flags reproduce the window bit for bit
  against `training/tests/data/prepare-reference.npz`, written by the code before this change;
  the layout; the signed time; the fallback; context after B rebuilt and noised like the context
  before A), `crates/ac-ai/tests/multikey.rs` (the card read with its defaults: the shipped model
  10/0; a synthetic card 6/4; the window's frames for a loop, a clip that ends two frames after B,
  one that ends on B, and the old card at a clip's start). `cargo xtask check` and the Python
  checks pass; the shipped model's net and fill goldens are unchanged.
- **Smoke run:** `ac_train.train --sets builtin_loops --steps 30 --batch 16 --minutes 2
  --eval-windows 16 --eval-every 10 --tag smoke-140000 --context-after 4 --multi-key` trained,
  evaluated and exported (to `target/smoke-140000/`, deleted after, as was the model folder); the
  Rust port passed that export's goldens (`AC_AI_MODEL`/`AC_AI_GOLDEN`, `cargo nextest run -p
  ac-ai --test golden`), so a card with the options runs in the app as PyTorch runs it.
- **Decisions made without the owner:** N is a flag, not fixed, with 4 as the suggested value
  (the item's "a few frames"); the time to go stays one channel, signed, rather than a new input
  (so a model with `context_after` alone keeps the input width); the fallback keeps the window
  size rather than shortening it, so the position embedding sees the same layouts.

## Overnight command (not run)
From `training/`, the shipped recipe (134500's weights) with the option; 142000's `--multi-key`
can go in the same run (one model carries both options) or in a run of its own to tell the two
apart:

```bash
uv run python -m ac_train.train --minutes 180 --tag parts-after4 --context-after 4 --loss-weights vel=0.1,jerk=0.0005,skate=0.1
uv run python -m ac_train.evaluate --tag parts-after4 --checkpoint best        # seam B m/s and joint mm against parts-lowweights's card
uv run python -m ac_train.export parts-after4 --checkpoint best --as parts    # only if it wins; then cargo nextest run -p ac-ai
```

The acceptance is read from the evaluation: `seam_b_ms` down and `joint_m` no worse than
`training/data/models/parts-lowweights/model.json` (`evaluation_best`), gap bin by gap bin.
