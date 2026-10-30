# Train on short gaps (1–4 frames)

- **Status:** not started
- **Requires:** 126000
- **Source:** 115000 (review 1 §2, review 2 §9)

## What
Training samples gaps of 5..45 (`training/ac_train/train.py:290`), but the app fills 1..45
(`crates/ac-ai/src/lib.rs:273`). Sample every length, stratified by the bins in 126000, then tune
the mix from the evaluation. A curriculum (short gaps first, growing to 45) can be tried on top,
but the final mix must keep 1–4.

## Acceptance
On held-out takes, the 1–4 bin is no worse than plain interpolation and better than today's model;
the other bins are no worse than today's.
