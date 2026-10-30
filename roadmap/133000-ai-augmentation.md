# Augmentation that matches the task

- **Status:** built (2026-10-30); its acceptance is measured by the 134000 retrain
- **Requires:** 126000
- **Source:** 115000 (review 2 §8)

## What
`augment()` (`training/ac_train/train.py:133-154`) mirrors half the windows and tumbles 15 %.
- Tumble only types where a spinning root happens (acrobatic, fall, ground); a tumbled walk is
  motion no body makes.
- Small noise on the context frames only, so imperfect keys don't throw the fill.
- Mild speed warp (about 0.8–1.25×); keep jumps out of it or near 1×, because gravity doesn't
  scale.

Not needed: shifting the window by ±1 frame. Windows already start on any valid frame.

## Acceptance
126000's numbers are no worse on locomotion and better or equal on acrobatic and fall takes.

## Result
2026-10-30. Tumbling only for acrobatic, fall and ground takes (0.25 of their windows); noise on
half the spline contexts' frames before key A (about 2 degrees, 1 cm), key A exact; a speed change
of 0.8–1.25 times except for jump, acrobatic and fall takes, kept inside the window's clean frames
(`Sets.batch(warp=...)`, `--no-warp`). The card's `augment` lists them.
