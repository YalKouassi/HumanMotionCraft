# Train on the context the app gives

- **Status:** built (2026-10-30); its acceptance is measured by the 134000 retrain
- **Requires:** 126000
- **Source:** 115000 (review 1 §1, review 2 §8 context noise)

## What
In training the context before key A is clean mocap (`training/ac_train/train.py:180-193`). In the
app it is whatever the clip samples (`crates/ac-ai/src/lib.rs:301`): a spline between your keys,
an earlier fill, or frame 0 repeated before the first key (`lib.rs:296-297`). Teach the model
those inputs:
- Rebuild the context from sparse keys of the take (varied spacing, irregular placement) with the
  app's interpolation, already ported in `training/ac_train/clip.py` (smooth, linear, step). It
  needs a batched torch version for speed.
- Some windows get a still context (the clip start).
- Keep some clean-context windows. The target is always the original dense motion.

This is also the cause behind `crates/ac-core/src/ease.rs`: the model "heads for the next key
whatever speed the motion came in with", so neighbouring fills kink at a shared key.

## Acceptance
In the 126000 realistic-input evaluation, error and seam velocity improve on sparse-key and still
contexts, with clean-context numbers no worse; a fill_all over a walk shows a smaller kink at
shared keys with the ease turned off.

## Result
2026-10-30. `training/ac_train/context.py` ports the app's interpolation (smooth, linear, step) to
batched torch; `tests/test_context.py` checks it matches `Clip.sample` key for key. Each window's
context is clean (0.4), a spline through sparse keys (key A, key B, often the window's first frame,
0 to 3 more at random: smooth 0.25, linear 0.1, step 0.1) or held still (0.15), `--context` to
change; key A, the gap and key B stay recorded. The shipped model on these contexts (126000): 13.6
mm clean, 17.6 smooth, 56.8 step, 45.6 still. Not modelled: a context that is an earlier fill.
