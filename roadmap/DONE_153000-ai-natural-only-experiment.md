# Experiment: train on natural motion only

- **Status:** done (2026-10-31)
- **Requires:** none (it was 134500; the owner chose to train this first, and it has trained)
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

## Walk review cut down (2026-10-31)

The owner found `walk.mp4` too long at 77 minutes, with limps and zombies in it. `ac_train/walkgroups.py`
(tests in `training/tests/test_walkgroups.py`) sorts the 1,687 walks using `ac_train/gaitcheck.py`:

| Walks | Takes | Minutes | What happens |
|---|---|---|---|
| Too short to train (under `WINDOW`, 56 frames) | 951 | 18 | out: training never read them, though the table above counts them |
| Flagged by description or motion | 329 | 62 | out without a review |
| Steady walk | 209 | 17 | reviewed in 67 groups |
| Starts or stops in the take | 140 | 43 | reviewed in 92 groups, shown around the first change |
| Backwards or sideways | 58 | 22 | reviewed in 31 groups |

Starts and stops stay because they are the fills between standing and walking. The ACCAD performers
stay apart: Female1, Male1 and Male2 recorded the same script, and only ACCAD says who is who.

The gait check's motion measures catch 20 of 28 clearly odd 100STYLE walks (Stiff 0 of 3, DragLeftLeg
and DragRightLeg 1 of 3 each) and flag 0 of 3 Neutral, so the review is still needed. Each group shows
its most unusual take, 3 s for a steady walk and 4 s otherwise; the rest of a long take isn't seen.

Review video: `target/shots/2026-10-31/walk-review/walk-review.mp4` (10.5 min, 1x, a chapter a group),
with `walk-review.index.csv`. Physics off, fixed three-quarter camera.

The owner reviewed it and rejected nothing (2026-10-31), so `review/natural-takes.json`, which the
overnight script reads, now has these walks: walk 79.3 train minutes, all types 124.8. Rejects would go
in with `uv run python -m ac_train.walkgroups apply --reject G003,G017`.

A one-off step check of the 407 kept walks (not in the repo; each foot's swings while the hips move,
first 60 s of a take): the 209 steady takes have a median 1.76 steps/s, 1.11 m/s, and left and right
alternate. 354 pass; 53 in 38 groups don't, and all are still in. 30 have under 1.5 s of movement,
many of them single steps or sidesteps (ACCAD G069-G076, CMU subject 83). 19 don't alternate, which
sidesteps can't be judged on: 100STYLE's Neutral sideways walk fails too. Three step fast for a walk,
G035, G062 and G184. The test can't tell a limp or a sneak from a normal walk.

## Idle, run and jump review (2026-10-31)

The same tool with `--type idle`, `run` or `jump`, without the gait check, whose measures read a walk.
These types are short of minutes, so only descriptions naming a character or a stunt
(`walkgroups.STYLED`: `Drunk Wait`, dive rolls, flips) are out without a review. A group shows its longest take.

| Video (`target/shots/2026-10-31/`) | Length | Clips | Reviewed takes | Reviewed min | Too short | Styled |
|---|---|---|---|---|---|---|
| `idle-review/idle-review.mp4` | 2.1 min | 33 | 59 | 10.7 | 8 | 11 |
| `run-review/run-review.mp4` | 4.2 min | 81 | 95 | 10.1 | 76 | 14 |
| `jump-review/jump-review.mp4` | 3.5 min | 75 | 178 | 16.1 | 65 | 7 |

Each has a chapter a group and an `index.csv`. Physics off, fixed three-quarter camera. Jump clips are
3 s around the highest point, so a take with several jumps shows one.

The owner accepted all three with no rejects (2026-10-31), so `natural-takes.json` is final: 739 takes,
1,461 excluded. `uv run python -m ac_train.train --dry-run --natural --takes review/natural-takes.json`
prints the same table as `ac_train.selection`:

```
type          takes  train min        100style           accad  addbiomechanic   builtin_loops             cmu          kimodo  held out
idle             59        9.8             0.3             2.5             2.6             0.1             3.7             0.6       1.0
jump            178       15.9             0.0             0.9             2.6             0.0            11.6             0.8       0.2
run              95        9.3             2.5             2.9             0.0             0.0             3.0             0.9       0.8
walk            407       79.3             5.7             5.3             2.8             0.0            64.7             0.8       3.1
all             739      114.3             8.5            11.6             8.0             0.1            83.0             3.1       5.1
```

Jump's held-out set fell from 1.1 to 0.2 minutes, so the natural-subset jump row of the acceptance table
rests on about 14 s of motion; the full evaluation (step 3 of the overnight script) reads every
held-out window and doesn't change.

## Result (2026-10-31)

`pwsh -NoProfile -File training/overnight-2026-10-30.ps1` ran 02:48 to 05:51 with no errors, on the
739 takes. The model is `training/data/models/parts-natural/`; nothing is exported. Log:
`training/data/models/overnight-2026-10-30.log`.

**The acceptance line is not met.** Natural held-out windows, best checkpoints, model / interpolation:

| | Natural: joint mm | Shipped: joint mm | Natural: slide m/s | Shipped: slide m/s | Windows |
|---|---|---|---|---|---|
| idle | 7.85 / 8.52 | 8.17 / 8.52 | 0.0033 / 0.00317 | 0.00322 / 0.00317 | 458 |
| walk | 14.9 / 34.2 | 16.8 / 34.2 | 0.073 / 0.23 | 0.0911 / 0.23 | 1,642 |
| run | **51.2** / 70.4 | 28.9 / 70.4 | 0.101 / 0.383 | 0.128 / 0.383 | 417 |
| jump | 48 / 83.1 | 66.8 / 83.1 | 0.169 / 0.281 | 0.219 / 0.281 | 35 |
| all | 20 / 36.2 | 17.9 / 36.2 | 0.0296 / 0.076 | 0.0368 / 0.076 | 2,552 |

The natural model wins walk and jump and loses run on joint error; idle's foot slide is a little worse.
Foot slide is better in every gap bin from 5 frames up. The run row is mostly a sideways run: 45.5 of the
50.4 held-out seconds are `100style/run_100style_SR_052`, and the only other take is `cmu/run_cmu_113_07`.
The natural model trained on no sideways run; the shipped one saw 100STYLE's styled ones. So the row may
overstate the loss on forward running. The jump row is about 14 s of 5 takes; on every held-out window jump is
42 / 65.5 against 44.1 / 65.5 joint with slide 0.169 against 0.152, so the jump gain isn't firm.

**The cost, every held-out window** (natural / shipped joint mm; interpolation in brackets): sit 98.9 / 69.9
(74.7), ground 87.6 / 57 (58), exercise 35.6 / 17.2 (19.6), combat 46.3 / 40.9 (49), run 47.5 / 34.3 (70.6),
walk 23.1 / 19.6 (37.2), all 28.6 / 23.2 (41.4). Sit, ground and exercise are worse than plain
interpolation; ground foot slide is 0.123 against 0.00245 m/s.

The model learned `range-of-motion` as a style, from 11,393 frames of takes with that data tag: the
filter treats it as no style word, but `labels.of` still makes it a label.

**Review video:** `target/shots/2026-10-31/natural-judge/ALL.mp4` (2.1 min, a chapter a scene; each scene
is also its own MP4), from `ac_train.judge`: REAL MOTION, SHIPPED and NATURAL side by side, fixed
three-quarter camera, physics off, once at full speed and once at half. SHIPPED is `parts-lowweights`
at its best checkpoint. All 14 takes are held out from both models. Waiting for the owner.

**Owner's judgement (2026-10-31):** the videos look the same to the owner and the natural model is fine,
because the plan is a natural base with the styles as add-on models (154000), not this model alone. So
the style menu and the sit, ground and combat losses above are what the add-ons must win back, and they
don't count against it as a base. It stays out of `assets/models` until 154000 and 155000 exist: exporting
it now would take 41 of the 48 styles out of the app with nothing to replace them.

## Closed (2026-10-31)
Closed by the owner. parts-natural is the base for 154000; it is not exported.

## Decision (2026-10-31)
The owner: `parts-natural` is not exported until add-ons work (154000), because the app would lose 41 of its 48 styles.
