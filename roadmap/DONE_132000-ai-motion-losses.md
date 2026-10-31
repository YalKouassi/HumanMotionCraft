# Losses that match how a fill looks

- **Status:** done (2026-10-31), by the owner's decision: the line is not met
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

## Result
2026-10-30. `training/ac_train/losses.py`: feature L1; joint positions with the wrists and feet
(and the balls of the feet) weighted twice the rest; velocity error from key A to key B, both
seams included; jerk past the recording's own (extra shake only); per-bone geodesic rotation
error. Each is logged every `--eval-every` steps and its weight is in the card (`loss_weights`;
`--loss-weights` changes them). The losses see the fill as the app keeps it: the network's output
on a known frame is replaced by the known frame. A training step with them runs at 12.5 steps a
second on the RTX 3080 Ti (20.9 with the old loss; the old run made 15.4).

## Closed
The owner's decision, 2026-10-31. The losses are in the shipped model, which the owner chose to ship (134000), with lower velocity, jerk and skate weights (134500). Measured from `parts-lowweights`'s `evaluation_best` when closing, never written here before: seams 0.198 / 0.228 m/s against the old model's 0.13 / 0.17, so no improvement, and joint error is worse (134500). The card logs each term, as asked.
