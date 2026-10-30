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

## Notes
2026-10-30: the prerequisites are built (126000, 128000–133000), and 135000 and 136000 too, so
one run measures them all. Their acceptance lines are checked against this run's `evaluation` in
the card. Compare it with the shipped model's numbers in 126000; that model may have seen some
held-out subjects (per-take split), which flatters it. Training is still the owner's go-ahead. A
3-hour run makes about 135,000 steps at 12.5 a second (the shipped model had 166,755).

Also measured by this run: 141000 (the root path as an input; the card's `trajectory`), 136000
on the posed keys (`posed`, against the shipped model's 17 per 1,000), and ACCAD now in the
validation (two sessions held out, 131000). The new model's card has `path`, so the app gives it
the keys' path when `trajectory` is above 0.
