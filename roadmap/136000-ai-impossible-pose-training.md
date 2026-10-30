# Train the model away from impossible poses

- **Status:** built (2026-10-30); its acceptance is measured by the 134000 retrain
- **Requires:** 120000, 126000
- **Source:** commit `bd70d5b`, 115000

## What
Every fill now runs `keep_possible` (`crates/ac-core/src/possible.rs:222`) because the model can
make impossible joints. That fixes the output after the fact. Count those frames in 126000, then
add a hinge loss past the `possible` limits (`training/ac_train/ranges.py`, the same maths as
`crates/ac-core/src/anatomy.rs`) so the fill rarely needs bending back.

## Acceptance
Impossible frames per 1,000 filled frames drop against today's model in 126000, with joint error
no worse.

## Result
2026-10-30. `Body.impossible` (`training/ac_train/kinematics.py`) measures the possible limits in
torch (the same angles as `anatomy.py`, vectorised, checked in `tests/test_kinematics.py`); the
loss adds the degrees past them on the gap frames (weight 0.002 a degree); the evaluation counts
them per 1,000 gap frames.

Finding: the shipped model makes **0** impossible frames per 1,000 on the held-out recordings, on
every context (126000), so this acceptance can't show a drop there. The impossible fills behind
`keep_possible` come from keys the evaluation doesn't sample (posed by hand).

The evaluation now has that case (126000, "posed keys": two built-in poses, the second stepped and
turned). The shipped model makes **17** impossible frames per 1,000 there (interpolation 20), so
this item's acceptance is measured on the posed keys: the retrain's count there against 17, with
joint error on the recordings no worse.
