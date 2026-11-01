# Review app: start training, see every model, choose the main one

- **Status:** proposed; waiting for the owner to agree the acceptance
- **Requires:** 174000, 177000, 195000
- **Source:** owner request (2026-11-01)

## What
Training is a terminal command today, and the only way to see old models is their folders. The review app stays a separate exe, so the shipped app still has no training buttons.

A Models page in the review app:

1. **Train:** pick a set, a name and how long, and see the dry run's totals (clips, minutes, what the trainer leaves out) before anything starts. Start runs `ac_train.train`, with its log and time left in the window, and Stop ends it. It reads what the trainer reads, and adds no training logic of its own.
2. **Every model:** every folder in `training/data/models` with its date, set, minutes, styles, score and whether it finished. Nothing is deleted from here. A run never writes over a model (built: `train.run_tag`).
3. **Main model on this machine:** "Set as main" exports that model's best checkpoint to the app's models folder, under its own tag, and records the choice. The app opens with it, and the AI models window still lists the others. The app has to learn this: its choice lasts one session today, and it opens with `parts`, else the first by name.
4. **Compare:** two models side by side by the card's numbers (`ac_train.compare`), with a line to make the review videos.

**The model everyone gets stays a separate step:** replacing `assets/models/parts` is a release, the owner's call, with its goldens, MANIFEST and ATTRIBUTION rows. The page shows which model that is, and doesn't change it.

## Acceptance
- From the review app the owner starts a short run of a set, watches its progress and stops it. The folder is kept and marked unfinished, and no earlier model has changed.
- A run named after an existing model is refused in the window, before it starts.
- The page lists every model in `training/data/models`, including the ones trained before this page existed.
- The owner sets a model as main, restarts the app, and fills with it. Setting another brings no loss: both stay listed.
- `assets/models` is unchanged by all of the above.
