# Evaluate hand-posed keys as whole poses

- **Status:** not started
- **Requires:** none
- **Source:** limb clash analysis (2026-11-01); owner asked for it to be filed

## What
Bug. `library_poses` (`training/ac_train/evaluate.py:348`) keeps only the model's 19 bones and leaves
every other bone at rest, so `ground`, `posed_windows` and the "posed keys" evaluation judge poses
nobody made.

Measured on the 50 built-in poses:

- The reader moves a wrist by up to 24.1 cm (median 1.6), an elbow by up to 7.2 cm and a fingertip by
  up to 37.3 cm (median 7.1).
- 49 of 50 poses turn an unsolved body bone by over 5 degrees. Largest turns: wrist 133, upperarm02
  65.5, lowerarm02 62.8, neck02 49.5, upperleg02 49, pelvis 46.
- At ease overlaps itself by 3.6 cm whole and 8.5 cm reduced. Hands on hips by 3.3 cm whole and not at
  all reduced.

Every posed-key figure so far has this error, including the impossible-frame counts in 136000 and the
line in 134500's acceptance.

The model also receives only 19 bones, so it may be partly blind to hand-made keys. That is an
inference, not measured. Measure it here: the same fill judged against the whole key and the reduced
key.

## Acceptance
- Posed-key evaluation runs on all 104 bones. A test loads a pose with a turned wrist and upperarm02
  and checks that the joints match the app's own pose to under 1 mm.
- The posed-key figures for the shipped model and interpolation are measured again and written into
  136000 and 134500, with the old figures marked as distorted.
- The result says whether the 19-bone input changes what the model does on hand-made keys, with a
  number.
