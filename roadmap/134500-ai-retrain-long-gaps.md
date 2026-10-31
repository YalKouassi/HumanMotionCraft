# Win back the long gaps

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 134000
- **Source:** 134000's result (the owner: ship it, then keep improving)

## What
The shipped model fills gaps over 4 frames about twice as far from the recordings as the one before
it, and leans towards plain interpolation. First candidate: lower the velocity, jerk and skate
weights (`--loss-weights vel=0.1,jerk=0.0005,skate=0.1`) and run the full 180-minute schedule, so at
any step it compares directly with `training/data/models/parts/model.json`'s log (at 55 minutes:
score 0.66, 21-30 at 38.5 mm, 31-45 at 56.5 mm). Stop at one hour if it isn't clearly ahead.

## Measured: the first candidate (2026-10-30)
`training/data/models/parts-lowweights`, not exported. Its best checkpoint (step 65,000, on a shared
machine, against the shipped 122,000) with `ac_train.evaluate --checkpoint best --no-write`, shipped
model's numbers in brackets:

- Joint error: 9.4 (10.3) / 20.4 (22.2) / 32.0 (34.0) / 51.6 (54.2) mm for gaps 5-10 / 11-20 / 21-30
  / 31-45, so 5-9% better and still far from the previous model.
- Step context 24.0 (25.4) mm; still context 26.8 (28.0) mm; floor 51 (58.3) mm.
- Seams 0.198 / 0.228 (0.205 / 0.239) m/s.
- Foot slide 0.0528 (0.0455) m/s, which fails the acceptance line; on turns 0.233 (0.185) m/s.
- Score 0.556 (0.574).

Lower loss weights alone don't win the long gaps back, and they cost foot slide. The previous model's
numbers are also flattered (it trained on other takes of most of these subjects, 134000), so part of
this line's target may not be reachable by any honest model.

The owner watched side-by-side videos of turns, foot plants, a sit and a jump
(`target/shots/2026-10-30/ai-judge-lowweights/`) and judged this model the best, so it replaces the
shipped one (owner's decision, 2026-10-30), its extra foot slide included. This item stays open: the
long gaps are still far from the target.

## Acceptance
On the 126000 held-out windows: gaps 5-10 / 11-20 / 21-30 / 31-45 closer to the previous model's 6.2 /
11.5 / 17.5 / 23.4 mm than today's 10.3 / 22.2 / 34.0 / 54.2 mm; step and still contexts stay under
interpolation's 41.4 mm; foot slide no worse than 0.045 m/s; the owner judges side-by-side videos.

## Overnight plan (prepared 2026-10-30, not run; changed the same day)
`training/overnight-2026-10-30.ps1` now runs 153000's night (owner's decision, 2026-10-30: the
natural-only base model trains next): the shipped model's missing scenario evaluation (134750 found
its card has no scenario rows; this step stays), then `parts-natural` for 180 minutes on the shipped
recipe (`vel=0.1,jerk=0.0005,skate=0.1`) with the natural take list, then both models evaluated on
the natural held-out windows and the comparison tables. About 3.5 hours. The 140000
(`--context-after 4`) and 142000 (`--context-after 4 --multi-key`) runs this script held before were
moved out and go in a later night, each 180 minutes on the same recipe and evaluated with
`--checkpoint best`. Nothing is exported: the owner compares the cards against this file's acceptance
line and judges side-by-side videos before any export (`ac_train.export <tag> --checkpoint best --as parts`).
