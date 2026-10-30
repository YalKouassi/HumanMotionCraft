# Train on the context the app gives

- **Status:** not started
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
