# Experiment: train on natural motion only

- **Status:** not started; starts only when the owner asks, because it trains; videos and filter ready (2026-10-30)
- **Requires:** 134500
- **Source:** owner request (2026-10-30)

## What

Test whether a model trained only on natural idle, walk, run and jump fills those better than the model
trained on everything. Acrobatic waits: it isn't normal motion and has about 5 minutes of data.

Today's model (`assets/models/parts/weights.json`, held-out windows, joint error, model against plain
interpolation): walk 21 / 37 mm, run 37 / 71, jump 46 / 65, idle 9 / 9, sit 74 / 75, ground 57 / 58.  
  
For the candidates before training, produce a Walk mp4 video of them all, within one long video, with markers for each animation. so i can feedback what to include/exclude.  then do the same for idle animations, then the same for the next eg run, jimp and any others.

Then we train on those.

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

## Prepared (2026-10-30)

Steps 1 and 2 are built, nothing trained. The filter is `ac_train/selection.py`, with the same
arguments on `ac_train.train`, `ac_train.evaluate` and `ac_train.labels` (`--types`, `--styles`,
`--natural`, `--takes <json>`); `train --dry-run` prints the table below and stops. Tests:
`training/tests/test_selection.py`, `test_compare.py`.

**The natural rule.** A take is natural when its type is `idle`, `walk`, `run` or `jump` (the
review's type, else the take name's) and its style words are empty or `natural`. A take's style
words are 100STYLE's style (`Neutral` is `natural`; the 32 other ways of moving are their model
labels, `zombie`, `drunk`, `limp-left`; the 67 pose, prop and path styles count by their own name,
`teapot`, `aeroplane`, `spin-clock`, because a walk with a teapot arm is not a normal walk either)
and the review's tags, except a tag that only names a type (`running`) and the data tags
`range-of-motion` and `duplicate`, which say what was cut, not how the body moves. So 100STYLE
gives only its 8 `Neutral` takes; CMU, ACCAD, AddBiomechanics, Kimodo and the built-in loops give
their idle, walk, run and jump takes whatever their description says. The 100STYLE transitions
(`any_100style_TR1_*`) and CMU's untyped takes (`any_cmu_*`) are out: no type. The owner decides
the rest from the videos.

**Minutes that survive** (`uv run python -m ac_train.train --dry-run --natural`, cut frames left
out; the review's 67 minutes of rejected takes are out before the filter):

```
type          takes  train min        100style           accad  addbiomechanic   builtin_loops             cmu          kimodo  held out
idle             78       16.1             0.3             2.5             2.6             0.1            10.0             0.6       1.0
jump            250       17.5             0.0             0.9             3.4             0.0            12.3             0.9       1.1
run              185      11.9             2.5             3.0             0.2             0.0             5.0             1.2       0.8
walk           1687      154.7             5.7             8.3            21.3             0.0           117.1             2.3       6.7
all            2200      200.2             8.5            14.7            27.5             0.1           144.4             5.0       9.6
left out (minutes): style 899.1, type 597.1, left out 67.1
```

200 minutes train, 9.6 held out: an eighth of the 1,611 minutes the shipped model saw. Run is thin
(12 minutes) and jump and idle not much better; the 180-minute schedule will pass over the walks
many times. The owner wanted the table before deciding; it is not too little to train on.

**Candidate videos** (`uv run python -m ac_train.candidates --natural ...`) under
`target/shots/2026-10-30/natural-candidates/`: one MP4 a type, every kept take once (its first
10 s when longer, said on the frame), fixed three-quarter camera, physics off, a one-second title
card and a burnt-in line a take (index, `set/take`, length, style word, held out, frames cut),
chapter markers muxed in (`<type>.chapters.txt`) and `<type>.index.csv` (index, take id, start
second, length). Walk has 1,687 takes, so it plays at 2x speed (said on every frame).

| Video | Takes | Length | Speed | Size |
|---|---|---|---|---|
| `walk.mp4` | 1,687 (109 held out) | 76.8 min | 2x | 106 MB |
| `idle.mp4` | 78 (6 held out) | 10.4 min | 1x | 10 MB |
| `run.mp4` | 185 (2 held out) | 12.2 min | 1x | 12 MB |
| `jump.mp4` | 250 (19 held out) | 21.8 min | 1x | 25 MB |

No other natural type exists, so there is no `others-*` video. Note for the review: the Kimodo
`acrobat_*` dive roll, front flip and handspring takes are in `run.mp4` and `jump.mp4` because the
review retyped them as run and jump (the rule takes the review's type), and Kimodo's `walk_proud`
and `walk_tired` walks pass because a Kimodo take's type is its prompt column alone and its prompt
words are no style label; exclude them in the take list if they are not natural.

**The take list.** `training/review/natural-takes.json` lists the 2,200 kept takes under `include`
(written by `uv run python -m ac_train.selection --natural --write-takes review/natural-takes.json`)
with an empty `exclude`. The ids are the ones on the videos. A non-empty `include` is exactly the
takes training reads (the type and style filters then don't apply), `exclude` takes out whatever
else says; an id no set has is an error. The scan's and the review's rejections still hold.

**Review workflow.** Watch `walk.mp4`, then `idle.mp4`, `run.mp4`, `jump.mp4` (the chapter list
jumps to a take; the index CSV gives its second). Edit `training/review/natural-takes.json`: delete
a take from `include` or add its id to `exclude`; add any other take's id to `include` to bring it
in (a styled walk, a transition). Then, from the repo root:

```
pwsh -NoProfile -File training/overnight-2026-10-30.ps1
```

which runs, in order, with one log (`training/data/models/overnight-2026-10-30.log`):

1. `evaluate --tag parts-lowweights --checkpoint best` (the shipped model's missing scenario rows, 134750);
2. `train --minutes 180 --tag parts-natural --natural --takes review/natural-takes.json --loss-weights vel=0.1,jerk=0.0005,skate=0.1`
   (134500's recipe, nothing else changed; it prints the minutes table first);
3. `evaluate --tag parts-natural --checkpoint best` (every held-out window: how sit, ground, combat changed);
4. `evaluate --tag parts-natural --checkpoint best --natural --takes review/natural-takes.json --subset natural` and the same
   on `parts-lowweights`, written under `evaluation_best_natural` in each card so the full numbers stay;
5. `compare --card parts-natural:best:natural --card parts-lowweights:best:natural` and the same on the full
   evaluations: both models by gap bin and type, model beside interpolation, the table the acceptance asks for.

About 3.5 hours. Nothing is exported; the 140000 and 142000 runs the script held before go in a
later night (134500). Side-by-side videos for the owner come after the run.
