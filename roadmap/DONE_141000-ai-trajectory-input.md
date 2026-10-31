# Root path as an input

- **Status:** done (2026-10-31)
- **Requires:** 130000
- **Source:** 115000 (review 2 §3)

## What
`trajectory` blends the root onto the keys' path after the network runs
(`crates/ac-ai/src/lib.rs:407-422`), so the legs and lean don't know the path. Feed a few root
positions across the gap as a given channel (the root position given apart from the root
rotation), dropped in training often enough that a fill without a path still works. Keep the
post-blend: it is what guarantees the path.

## Acceptance
With `trajectory` 1, foot slide in 126000 drops against today's post-blend alone.

## Result
2026-10-30. A model whose card says `path` takes four more inputs a frame: the root position
(hip heights, key A's frame) and a flag, 1 on the gap frames that have a path
(`training/ac_train/model.py`, `PATH`). Training gives half the windows the recorded root across
the gap, box-smoothed over 1, 5, 9 or 15 frames because the app's path is the keys' spline,
rounder than a recording; the other half get none, so a fill without a path still works. The app
(`crates/ac-ai/src/lib.rs`, and `ac_train/infer.py` the same) sends the keys' path, stepped keys
smoothed, whenever `trajectory` is above 0, and still blends the root onto it afterwards by
`trajectory`. A model without `path` gets the same input as before (`crates/ac-ai/src/net.rs` reads
the flag from the card to size the input), so the shipped model is unchanged; its goldens still
pass. The goldens pin the path too: the net golden has a made-up path, and the fill golden has a
`trajectory` 1 case. `python -m ac_train.pipeline --quick` passes with it.

The path goes in at full strength whatever `trajectory` is (training only had 0 or 1); below 1,
the blend afterwards decides how far the root follows it.

The evaluation's root-path case (126000) puts `given` beside `blend`: the acceptance is `given`'s
`slide_ms` under `blend`'s in the retrain's card.

## Measured by the retrain
2026-10-30 (134000, its Result has the table). Foot slide with the path given 0.0738 m/s against 0.0772 with the blend alone: met, by 4 %.

## Confirmed
The owner confirmed it 2026-10-31: the line is met. The model that replaced 134000's (`parts-lowweights`, its `evaluation_best`) meets it too: foot slide 0.0772 m/s with the path given against 0.0812 with the blend alone, 5 % less.
