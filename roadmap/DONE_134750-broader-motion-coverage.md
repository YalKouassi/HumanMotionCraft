# Broader motion coverage, where evaluation finds gaps

- **Status:** done (2026-10-31)
- **Requires:** 126500, 134500
- **Source:** 115100

## What
Not more data for its own sake. Take the weakest slices from 126500, with how many training minutes
each has, and add data only for those: first from sets already registered, then from new ones.
Free-licence sets only (112000): a non-commercial set never goes in the `REGISTRY`, the docs or a
release.

## From 126500 (2026-10-30)
Start, stop and crouch have too few evaluation windows to name a weakest slice yet (17, 25 and 19 of
2560). That needs settling first, or this item would pick its data from noise.

## Acceptance
The result lists the two weakest slices with their minutes and what was added; a retrain improves
those slices on held-out subjects with no other slice getting clearly worse; `ATTRIBUTION.md` and
the manifest have a row for each new set.

## Analysis (2026-10-30)

Nothing trained, nothing downloaded. `python -m ac_train.coverage --card parts-lowweights:best
--card parts` (new, `training/ac_train/coverage.py`, CPU, 1.5 minutes) reads every take of the six
registered sets and labels its frames with 126500's own thresholds (`evaluate.py`: still under
0.2 m/s, moving over 0.7, crouch at 40–75 % of hip height on the floor, flight two frames with
both feet off it, a turn over 45° at over 0.5 m/s, a gap of at most 45 frames), so the minutes
below are the motion the evaluation's slices would find in the training takes, not what the take
names say. It writes `training/data/coverage.json` (gitignored). Check of the classifier: at the
evaluation's uniform draw over the held-out subjects it predicts 22 / 22 / 24 / 647 / 73 / 172
windows for start / stop / crouch / flight / turn_step / turn; the cards drew 17 / 25 / 19 / 653 /
57 / 152. The windows are the same seeded draw in both cards, so the counts hold for the shipped
model too.

**One check failed:** the shipped model (`parts-lowweights` best, per `assets/models/MANIFEST.json`)
has no `scenarios` rows: its card was written before 126500 landed and 134500 evaluated it with
`--no-write`. Its by_type, hard, gap and context rows are below; the scenario rows (start, stop,
crouch, flight, turn_step) are the previous `parts` model's from 126500, marked. The GPU was busy,
so it was not re-evaluated: `uv run python -m ac_train.evaluate --tag parts-lowweights --checkpoint
best` fills them in (about 10 minutes on the GPU).

### Minutes and windows per slice

Minutes: *train* is what training uses (cut frames left out), *held out* what validation holds
out, *out* what the scan or the review leaves out. Windows at 512 a bin (2,552 clean windows).
Ratio = model / plain interpolation on joint error (lower is better; 1.0 means the model adds
nothing).

| Slice | Train min | Held out | Out | Mostly from | Windows | Trusted | Ratio | mm (model / interp) | Card |
|---|---|---|---|---|---|---|---|---|---|
| start (still → moving) | 322.0 | 15.6 | 21.8 | 100STYLE 223, CMU 86 | 17 | noise | 0.574 | 57.6 / 100 | parts |
| stop | 322.6 | 16.5 | 19.0 | 100STYLE 231, CMU 79 | 25 | noise | 0.528 | 51.5 / 98 | parts |
| crouch | 22.3 | 1.0 | 3.1 | CMU 11, AddBiomechanics 9 | 19 | noise | 1.012 | 40.8 / 40 | parts |
| flight | 161.2 | 15.0 | 37.7 | CMU 89, 100STYLE 52 | 653 | yes | 0.627 | 42.0 / 67 | parts |
| turn_step | 589.0 | 24.7 | 9.8 | 100STYLE 507, CMU 78 | 57 | thin | 0.425 | 63.4 / 149 | parts |
| hard: turn | 861.4 | 41.8 | 20.8 | 100STYLE 740, CMU 110 | 152 | yes | 0.419 | 59.7 / 143 | lowweights |
| hard: plant change (events) | 85.9 | 4.3 | 1.7 | 100STYLE 65, CMU 18 | 959 | yes | 0.548 | 26.7 / 49 | lowweights |
| still hips (what the still context keys on) | 399.1 | 27.5 | 36.6 | CMU 229, 100STYLE 121 | — | — | — | — | — |
| type: walk | 697.2 | 38.2 | 18.4 | 100STYLE 548, CMU 117 | 1109 | yes | 0.527 | 19.6 / 37 | lowweights |
| type: run | 299.5 | 15.6 | 1.4 | 100STYLE 290 | 465 | yes | 0.486 | 34.3 / 71 | lowweights |
| type: untyped (transitions) | 362.2 | 29.4 | 41.7 | 100STYLE TR takes 212, CMU 149 | 737 | yes | 0.620 | 19.6 / 32 | lowweights |
| type: idle | 37.1 | 2.4 | 0.7 | 100STYLE 21, CMU 10 | 66 | thin | 0.971 | 8.6 / 9 | lowweights |
| type: combat | 35.5 | 2.6 | 0.6 | CMU 29 | 42 | thin | 0.836 | 40.9 / 49 | lowweights |
| type: sit | 17.7 | 1.2 | 0.7 | AddBiomechanics 10, CMU 7 | 30 | thin | 0.935 | 69.9 / 75 | lowweights |
| type: jump | 17.6 | 1.1 | 2.0 | CMU 12, AddBiomechanics 3 | 18 | noise | 0.674 | 44.1 / 65 | lowweights |
| type: exercise | 25.1 | 1.4 | 3.7 | AddBiomechanics 14, CMU 10 | 26 | noise | 0.874 | 17.2 / 20 | lowweights |
| type: sports | 21.8 | 1.0 | 1.4 | CMU 20 | 20 | noise | 0.965 | 10.8 / 11 | lowweights |
| type: ground | 8.4 | 0.8 | 1.9 | CMU 4, ACCAD 3 | 18 | noise | 0.983 | 57.0 / 58 | lowweights |
| type: carry | 12.8 | 0.7 | 0.4 | CMU 10 | 13 | noise | 0.884 | 9.7 / 11 | lowweights |
| type: dance | 30.0 | 0.6 | 2.2 | CMU 28 | 8 | noise | 0.926 | 23.3 / 25 | lowweights |
| type: gesture / fall / acrobatic | 34.0 / 6.7 / 5.2 | 0.1 / 0 / 0 | 0.5 / 0.2 / 2.6 | CMU / Kimodo / CMU | 0 | — | — | — | not drawn |
| gap 1–4 / 5–10 / 11–20 / 21–30 / 31–45 | every frame (1,610.9) | 94.9 | 78.4 | | 512 / 510 / 510 / 510 / 510 | yes | 0.664 / 0.603 / 0.552 / 0.544 / 0.565 | 2.9 / 9.4 / 20.4 / 32.0 / 51.6 (model) | lowweights |
| context clean / smooth / linear / step / still | every frame | | | | 2552 each | yes | 0.562 / 0.576 / 0.573 / 0.579 / 0.648 | 23.2 / 23.8 / 23.7 / 24.0 / 26.8 (interp 41.4) | lowweights |

The gap and context slices are conditions every frame serves, so they have no minutes of their
own; the still context is the one that reads on a motion: key A held still is a start, and the
start slice is where its data would come from.

### The rule for "enough windows"

**A slice is trusted at 100 windows or more** (`coverage.TRUSTED_WINDOWS`); 30–99 is *thin*
(read the direction, not the number); under 30 is *noise* (`evaluate.MIN_WINDOWS`, which already
says so in the card). Why 100: per-window joint error varies about as much as its mean (the by_type
rows run from 9 to 74 mm, and a window's error grows with its gap), so a 100-window mean is known
to about 10 % of itself, well inside the 20–60 % by which the slices differ; 30 windows gives about
18 %, and 17–25 windows 20–25 %, which is larger than the differences being ranked (start 0.574
against stop 0.528, say).

Applied: trusted are flight (653), turn (152), plant change (959), walk, run, the transitions
row (737), every gap bin and every context. Thin: turn_step (57), idle (66), combat (42), sit (30).
Noise: start (17), stop (25), crouch (19), jump (18), exercise, sports, ground, carry, dance.

What it takes to trust the noise rows: at the evaluation's uniform draw the script predicts a
start or a stop in 0.9 % of windows and a crouch in 0.9 %, so `--windows 2400` (about 12,000
windows, five times the run) would get start and stop to 100, `--windows 2200` crouch, `--windows
800` turn_step. Cheaper and not built: a draw aimed at each scenario (the predicates in
`coverage.window_tests` run over every held-out window start in seconds, so the evaluation could
draw 100 windows a scenario from the ones that qualify and keep the uniform draw for the rest).
That is a 126500 follow-up, not this item's.

### The two weakest trustworthy slices

Ranked by ratio among the trusted motion slices (the condition slices aside: gap 1–4 at 0.664 is
128000's known problem, and the still context at 0.648 is the start slice in another guise):

1. **Flight (take-off and landing): 653 windows, ratio 0.627, 161 train minutes by the
   classifier, but only 17.6 minutes of jump-typed takes.** The 161 minutes are mostly not jumps:
   the takes with the most "flight" are CMU subject 86's fights and sports and subject 120's walks
   (55–72 seconds airborne each), feet recorded above the floor, as 126500 suspected ("flight looks
   too common"). Another 31 minutes of it sit in CMU takes the scan cuts or rejects (float cuts, the
   same thing). The honest number is the jump row: 17.6 minutes (CMU 12.2 over 162 takes,
   AddBiomechanics 3.4, ACCAD 0.9, Kimodo 0.9), 18 windows, ratio 0.674, the same direction.
   Kimodo's generated jumps barely leave the floor by the 2 cm contact test (0–1 s of flight a take;
   `jump_forward` dips its hips to 31 % of standing, a glitch) so they teach little flight.
2. **Transitions (the untyped row): 737 windows, ratio 0.620, 362 train minutes:** 100STYLE's
   TR1–TR3 takes (212 minutes: idle, walk and run changing into each other, one actor) and 496 CMU
   takes whose index line matches no type word (149 minutes; subjects 29–32 and 55 lead). This is
   the trusted stand-in for start and stop, which point the same way (0.574 and 0.528 on 17 and
   25 windows) and whose 322 minutes each are 70 % that one 100STYLE actor; 100STYLE is held out
   per take, so the same actor is on both sides of its evaluation (`split.py`), which flatters it.

Crouch stays unranked (19 windows), and its data is thinner than the table says: of the 22.3
minutes, the big takes are CMU 55/86/114/135 and Uhlrich2023 sit-to-stand, i.e. hips at chair
height, not squats; ACCAD's real crouches (`female1_a07_crouch`, `male1_a7_crouch`,
`walktocrouch`) sink the hips to 28–35 % of standing, under the band's 40 % floor ("lower is lying
down or sitting"), so they count for 0–1 s each. The band mixes sitting in and leaves deep squats
out; a 126500 follow-up should split sit (hips 40–75 %, thighs level) from crouch (hips under 60 %
with the feet flat) before any crouch data is judged.

### Where more of that motion could come from

**Flight / jumps.** Registered sets: every CMU, ACCAD and Kimodo jump take is already used (none
held out but 1.1 minutes, none cut but 2.0). AddBiomechanics is the one registered set with
untouched material: the registry fetches 5 of the archive's 15 studies (`b3d.OVERGROUND_STUDIES`);
the jumps are in Han2023 (GroundLink: drop jumps, hops, squats, tai chi, 7 subjects, 1.7 h in the
archive, already fetched) and Uhlrich2023 (OpenCap: drop jumps, squats, sit-to-stand, already
fetched), and the paper names no other study with jumps (Carter2023 is 21.6 h of treadmill
running, Camargo2021 stairs and ramps, Santos2017 4.9 h of standing balance, the rest walking), so
widening the study filter adds standing and gait, not flight. Kimodo can generate more jump
prompts for free, but its jumps show no flight (above), so check them by eye first. Public
free-licence candidates: **Cologne Motion Capture Database** (TH Köln; CC BY 4.0; BVH and C3D; 48
actors with roles Athlete, Jumping Jack, Basketball, Footballer; the motion list is behind its
search page and was not fetched); **Eyes, JAPAN / mocapdata.com** (4,671 free files: jump, walk,
turn, sitdown_standup, sports, fighting; its terms page says Creative Commons Attribution 2.1
Japan in one place and Attribution-ShareAlike 2.1 Japan in another, both allowing commercial use;
share-alike would be the owner's call before it is registered); **Quaternius Universal Animation
Library** (CC0; hand-keyed jumps, crouch walks and idles, not mocap: the same kind as
`builtin_loops`). GroundLink's own site (250 fps FBX and C3D) states no licence: use the CC BY
4.0 copy inside AddBiomechanics, which is what the registry already does.

**Transitions / starts / stops.** Registered sets: 100STYLE's TR takes are all in; CMU has 18
minutes of start spans and 15 of stop in takes the scan cuts or rejects (a review pass could win
some back; `review.list --show flagged`), and 4 minutes each in its held-out subjects (stay out).
AddBiomechanics' unfetched studies are treadmill or stairs, except Santos2017 (standing balance:
still hips, no start) and Camargo2021's level-ground sections (its transitions are stairs and ramps,
which the rig has no scenery for, so it was skipped by design). Kimodo: "stand, then walk off",
"walk, then stop" prompts cost nothing but a GPU hour and would be the first second actor for
starts; its 3 minutes of start today came from prompts not aimed at it. Public: the same two,
Eyes, JAPAN (walk, turn, sitdown_standup) and the Cologne database (48 actors: the actor diversity
100STYLE lacks).

**Ruled out (non-commercial, never registered):** SFU Motion Capture Database ("cannot be used
for commercial products or resale"; it has the jumps, vaults and parkour rolls), Bandai Namco
Research motion datasets (CC BY-NC-ND 4.0), LaFAN1 (removed 2026-10-29), KIT Whole-Body Human
Motion Database (non-commercial research per the licence summaries found; its own terms page was
not reachable), AMASS as a package (research only; its CMU, ACCAD and Eyes Japan sources are free
at source, which is how CMU and ACCAD are already here), MoVi/BMLmovi (licence not verified).

### Proposed next step, for the owner

Before any data is added: fill the shipped model's scenario rows (the evaluate command above) and
settle the small slices with an aimed draw or `--windows 2400`, then add data for flight and for
transitions only: first the Cologne database's jump and start/stop takes if its list has them
(CC BY 4.0, a row in `ATTRIBUTION.md` and the manifest), then Kimodo prompts for starts and stops.
A retrain is the owner's call (134500 is still open on the long gaps).

## Closed (2026-10-31)
Closed by the owner. The analysis stands; the retrain, new data and the held-out comparison in the acceptance were not done.
