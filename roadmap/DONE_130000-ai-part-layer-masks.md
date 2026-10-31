# Train on the part masks the app sends

- **Status:** done (2026-10-31), by the owner's decision: the line is not met
- **Requires:** 126000
- **Source:** 115000 (review 2 §4, corrected)

## What
The app passes as given every model bone *outside* the edited layer's parts
(`crates/ac-core/src/ai.rs:160`). A left-arm layer therefore sends 16 given bones and 3 free.
Training gives exactly one part group as given and the rest free (`given_mask`,
`training/ac_train/train.py:166-177`), so it never shows the model "everything given except one
limb". Sample the masks the app actually makes: complements of each part group and of common
unions (arms, legs, upper, lower), plus today's single groups. Random per-bone masks (review 2) are
not the fix: the app never makes them, and pins and `parts.lock` don't use the mask.

## Acceptance
In 126000's part-layer evaluation (each part group's complement as given), the free bones' error
drops against today's model, and full-body fills are no worse.

## Result
2026-10-30. `train.given_mask`: half the windows get a mask; of those, 60 % give every bone outside
a part group (what a part layer sends) and 40 % the group itself, over every group in
`PART_GROUPS` including the unions (`tests/test_train.py`). The shipped model on the part layers
(126000, free bones): legs 18.5 mm against 37.8 for a plain fill, arms 7.0 against 8.5.

## Closed
The owner's decision, 2026-10-31. The training change is in the shipped model, which the owner chose to ship (134000). Measured from `parts-lowweights`'s `evaluation_best` when closing, never written here before: free bones legs 21.6 mm and arms 8.2 mm, against 18.5 and 7.0 for the old model (which trained on other takes of most of these subjects, so it is flattered) and 36.5 and 8.3 for interpolation. So no drop against the old model. Full-body fills are worse too; 134500 carries that.
