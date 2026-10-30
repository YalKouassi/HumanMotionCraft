# Experiment: train on natural motion only

- **Status:** not started; starts only when the owner asks, because it trains
- **Requires:** 134500
- **Source:** owner request (2026-10-30)

## What
Test whether a model trained only on natural idle, walk, run and jump fills those better than the model
trained on everything. Acrobatic waits: it isn't normal motion and has about 5 minutes of data.

Today's model (`assets/models/parts/weights.json`, held-out windows, joint error, model against plain
interpolation): walk 21 / 37 mm, run 37 / 71, jump 46 / 65, idle 9 / 9, sit 74 / 75, ground 57 / 58.

Walk and run are already about 16 of the 28 training hours, but most of 100STYLE (about 19 hours)
is styled walking typed as `walk`: a type filter alone keeps the zombies and limps. `natural`
is only about 10 minutes. So:

1. Add a filter to `ac_train.train`, `ac_train.evaluate` and `ac_train.labels`: by type, and by style
   word so that styled takes can be left out (it filters sets only today, `train.py:340`).
2. Print how many minutes survive, by type and set, before any training. If it is too little to train on,
   stop and report it to the owner.
3. Train the full 180 minutes with 134500's outcome as the baseline, changing nothing else.
4. Evaluate both models on the same held-out natural windows.

The model stays out of `assets/models` until the owner decides: it would shrink the style menu and
may fill other motion worse.

## Acceptance
The result gives the minutes kept and a table of both models by type and gap bin. It passes when the new
model has lower joint error and no worse foot slide on held-out idle, walk, run and jump, and
the owner approves side-by-side videos. The result also says how sit, ground and combat fills changed,
so the cost is known.
