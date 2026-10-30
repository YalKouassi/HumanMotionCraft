# AI evaluation: more scenarios

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 126000
- **Source:** 115100

## What
126000 already scores by motion type (walk, run, jump, fall, carry and others) and on the
plant-change and turn slices. Add what is still missing, as fixed slices with their own measures:
- **Starting and stopping:** the speed of the hips rising from or falling to still.
- **Crouching:** its own slice; `crouch` falls under `ground` now (`training/ac_train/labels.py`).
- **Take-off and landing:** windows with a flight, scored on the flight's shape and the landing.
- **Turning while stepping:** foot placement on the existing turn slice.

Running's stride and walking's weight transfer can read the existing types. Carrying is scored only
as `carry` takes, because the rig has no load model. Each slice reports its window count, and a slice
too small to trust says so.

## Acceptance
`python -m ac_train.evaluate --tag parts` writes the slices into `model.json`, plain interpolation
beside them; a pytest works each new measure by hand on a tiny synthetic window.

## Built (2026-10-30)
`ac_train/evaluate.py` writes `scenarios` into the card, plain interpolation beside each, and the
report prints them. Slices come from the recording's motion, not its take name, so crouch no longer
depends on `ground`. `tests/test_evaluate.py` works each measure by hand. The overall score is
unchanged at 0.57405, as it should be.

The shipped parts model on held-out subjects, model / interpolation, at the default 512 windows a bin
(2560 in all):

| Slice | Windows | Joint mm | Hip speed m/s | Flight mm | Landing mm | Foot place mm |
|---|---|---|---|---|---|---|
| start (too few) | 17 | 57.6 / 100 | 0.108 / 0.229 | 62.5 / 106 | 50.4 / 94.0 | 33.2 / 54.5 |
| stop (too few) | 25 | 51.5 / 97.6 | 0.114 / 0.250 | 49.9 / 95.6 | 44.0 / 113 | 48.8 / 115 |
| crouch (too few) | 19 | 40.8 / 40.3 | 0.061 / 0.072 | none | none | 6.6 / 32.5 |
| flight | 653 | 42.0 / 67.0 | 0.093 / 0.131 | 35.6 / 54.1 | 41.7 / 72.6 | 50.6 / 108 |
| turn_step | 57 | 63.4 / 149 | 0.132 / 0.198 | 80.1 / 203 | 70.4 / 193 | 62.6 / 167 |

- **Don't read much into three of these:** start, stop and crouch are under the 30 windows a slice
  needs and say so. Those windows are drawn at random, so more of them would need a bigger `--windows`
  or sampling aimed at each slice. Neither was tried.
- **Crouch is the one to watch:** on joint error the model is no better than interpolation, 40.8
  against 40.3 mm, but only on 19 windows.
- **Flight looks too common:** 653 of 2560 windows, about a quarter, have both feet off the floor for
  two frames in a row. Running would account for some. Feet recorded above the floor could account for
  others, so its numbers may not all be jumps. Not checked by eye.
- **Carry** is the `by_type` row, with 13 windows.
