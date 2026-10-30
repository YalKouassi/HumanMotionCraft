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

## Acceptance
On the 126000 held-out windows: gaps 5-10 / 11-20 / 21-30 / 31-45 closer to the previous model's 6.2 /
11.5 / 17.5 / 23.4 mm than today's 10.3 / 22.2 / 34.0 / 54.2 mm; step and still contexts stay under
interpolation's 41.4 mm; foot slide no worse than 0.045 m/s; the owner judges side-by-side videos.
