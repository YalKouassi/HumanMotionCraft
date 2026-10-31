# Train on short gaps (1–4 frames)

- **Status:** done (2026-10-31), by the owner's decision: the line is not fully met
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

## Result
2026-10-30. Training draws a gap bin by `--gap-mix` (default 0.15 / 0.2 / 0.2 / 0.2 / 0.25 for 1–4,
5–10, 11–20, 21–30, 31–45), then a length in it (`train.sample_gap`, `tests/test_train.py`). The
shipped model's 1–4 bin is 5.5 mm against the interpolation's 4.1 mm (126000): the number to beat.

## Measured by the retrain
2026-10-30 (134000, its Result has the table). Gaps 1-4: 3.1 mm against the old model's 4.9 and interpolation's 4.4, as asked. The other bins are worse than the old model's, so this line is not fully met. 134500 is on them.

## Closed
The owner's decision, 2026-10-31. The training change is in the shipped model, which the owner chose to ship (134000, then 134500's lower-weights candidate) with the other bins worse than the old model's. Those bins are 134500's work.
