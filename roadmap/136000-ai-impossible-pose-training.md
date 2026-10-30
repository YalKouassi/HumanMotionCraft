# Train the model away from impossible poses

- **Status:** not started
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
