# Retrain `parts` with ACCAD and AddBiomechanics

- **Status:** not started; needs the owner's go-ahead to train
- **Requires:** 126000, 128000, 129000, 130000, 131000, 132000
- **Source:** commit `ee47a2c` ("parts is not retrained"), 115000

## What
ACCAD (CC BY 3.0) and AddBiomechanics (CC BY 4.0) are registered, retargeted and scanned, and the
review now drops duplicate and impossible takes. The next `python -m ac_train.train` reads them by
default. Retrain once the training changes above are in, so a single run measures them all, and
regenerate the `ac-ai` goldens on export.

## Acceptance
The new model is no worse than today's in any 126000 bin on the held-out subjects; the card and
`assets/models/MANIFEST.json` list both sets; `ATTRIBUTION.md` and `NOTICE` credit them; the
goldens are regenerated; the owner tests and accepts it.
