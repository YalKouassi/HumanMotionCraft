# Test add-on models: a natural base plus a short training run per style

- **Status:** not started; the next training run, agreed by the owner (2026-10-31); starts when the owner gives the word to train
- **Requires:** 153000
- **Base:** `training/data/models/parts-natural` (best checkpoint), the owner's choice (2026-10-31)
- **Source:** owner's preferred direction (2026-10-30)

## What
Instead of one model with every style inside it, a natural base model and a separate add-on model per
style or motion type, so adding one never changes the others. An add-on is a whole model made by
copying the base's weights and training briefly on its own data. Training one from scratch can't work:
zombie has about 11 minutes.

1. `ac_train.train` gets a way to start from another model's weights (it has no such option today).
   The card records the base's tag and hash.
2. Two test add-ons, because they test different things:
   - **`zombie`**: a way of moving over gaits the base knows. Easy to judge by eye. 100STYLE's 8
     Zombie takes, 20,959 frames.
   - **`combat`**: motion the base never saw, about 35 minutes. This shows whether an add-on can bring
     back what a natural-only base gives up.
3. Validation that works on so little data. `ac_train.split` holds out by name hash and none of the 8
   Zombie takes falls under it, so a real run would stop (`train.py:366`). An add-on names the takes
   it holds out, for example the Zombie forwards walk, and its card says so.

Try these on the owner's machine through the app's own models folder. Nothing ships from this item.

## Acceptance
- Each add-on is compared on its own held-out takes with today's `parts` model given the same style
  (`zombie`, `combat`): joint error, foot slide, and the owner's side-by-side videos.
- On held-out natural motion, each add-on stays no worse than plain interpolation, and the result says
  how far it moved from the base.
- The result gives each add-on's training time and whether the base idea holds: go on to 155000, or
  stay with one model.

## Decision (2026-10-31)
The next training run. Start with one or two styles and compare them by eye before training more.
