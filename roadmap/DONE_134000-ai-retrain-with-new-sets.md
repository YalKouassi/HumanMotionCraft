# Retrain `parts` with ACCAD and AddBiomechanics

- **Status:** done (2026-10-30), by the owner's decision: the no-worse line is not met (Result)
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

## Result
2026-10-30, the owner's decision. The run took 180 minutes (131,733 steps); `best.pt` (step 122,000)
ships. **The "no worse in any bin" line is not met**: the owner compared the two side by side (10
sparse-key scenes, old, new and plain interpolation), found both "mostly fine", and chose to ship the
new model and drop the old one, then keep improving (134500). The old weights are in git at `5c81eaa`.

Held-out windows (the old model at 64 windows a bin, the new at 512; the same set, ACCAD included;
the old model trained on other takes of most of these subjects, which flatters it):

| | old | new | plain interpolation |
|---|---|---|---|
| joint error, gaps 1-4 | 4.9 mm | **3.1 mm** | 4.4 mm |
| 5-10 | **6.2 mm** | 10.3 mm | 15.6 mm |
| 11-20 | **11.5 mm** | 22.2 mm | 37.0 mm |
| 21-30 | **17.5 mm** | 34.0 mm | 58.8 mm |
| 31-45 | **23.4 mm** | 54.2 mm | 91.3 mm |
| context from step keys | 53.1 mm | **25.4 mm** | 41.4 mm |
| context held still | 42.6 mm | **28.0 mm** | 41.4 mm |
| seam velocity A / B | **0.13 / 0.17 m/s** | 0.20 / 0.24 m/s | 0.35 / 0.35 m/s |
| foot slide | 0.045 m/s | 0.045 m/s | 0.17 m/s |
| hard turns | **23 mm** | 63 mm | 143 mm |
| impossible per 1,000, posed keys | **17** | 19.8 | 20 |
| score | **0.50** | 0.57 | 1.0 |

A likely cause, not proven: the velocity, jerk and skate losses add up to about twice the accuracy
terms. Four 20-minute runs (`--loss-weights vel=0.1,jerk=0.0005,skate=0.1`, more clean context, fewer
part masks) did not settle it: at about 12,000 steps all sit near 0.93-0.95.
