# Augmentation that matches the task

- **Status:** not started
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
