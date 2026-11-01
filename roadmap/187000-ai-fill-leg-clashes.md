# AI Fill: leg clashes

- **Status:** not started; no method tested yet
- **Requires:** 184000
- **Source:** limb clash analysis (2026-11-01); owner asked for it to be filed

## What
186000 covers arms only. On whole hand-made poses, gap 30, 200 windows, 13% of fills make a new
leg-against-leg clash of 4 cm or more (interpolation 7.5%).

Legs are harder than arms. Moving a leg moves the feet, so a route has to keep foot plants and the
floor (`plant_spans` in `crates/ac-core/src/ai.rs`, `hold_contacts` in `crates/ac-core/src/edits.rs`)
and body balance. Test first
whether a capped correction of the model's output is enough, or whether legs need routes too.

## Acceptance
- On the same 200 windows, physics off: new leg-against-leg clashes of 4 cm or more at or below
  interpolation's 7.5%.
- Foot slide no worse than today's fill on those windows and on the 126000 held-out windows.
- The owner approves a reel by eye.
