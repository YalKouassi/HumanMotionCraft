# Train on the part masks the app sends

- **Status:** not started
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
