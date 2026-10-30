# Losses that match how a fill looks

- **Status:** not started
- **Requires:** 126000
- **Source:** 115000 (review 1 §2B, review 2 §7)

## What
Today's loss is L1 on the features plus joint-position error (`training/ac_train/train.py:299-300`).
Add, each logged separately so none takes over unseen:
- velocity error over the gap and across both seams (small weight);
- hands and feet weighted above the spine;
- jerk (third difference), tiny weight;
- per-bone rotation (geodesic) error.

Don't smooth everything: a punch, a jump and a slow walk need different timing, so these match
the reference motion's velocity and don't penalise velocity itself.

## Acceptance
Seam velocity and jerk improve in 126000 with joint error no worse; the card logs each term.
