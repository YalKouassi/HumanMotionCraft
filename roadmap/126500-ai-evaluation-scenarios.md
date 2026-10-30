# AI evaluation: more scenarios

- **Status:** not started; the acceptance line needs the owner's agreement
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
