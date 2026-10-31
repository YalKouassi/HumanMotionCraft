# Long fills keep regular steps

- **Status:** not started; step 1 agreed by the owner (2026-10-31); the fix is on hold, no training without the owner's word
- **Requires:** none
- **Source:** the owner's review of the long-fill video (2026-10-31); measured in 167000

## What
A long walking fill goes left full, right full, left half, right full, then full steps. On
`ai-fill-long-gap` the second left lands at y -0.89 m beside the right at -0.91 m. The foot hold didn't cause
it (the same without `hold_contacts`). The anchors on 30 and 60 are about 2.7 source steps apart, so a span has
to fit a step count that doesn't divide evenly. The trials are in 167000. Anchors on every step give regular
steps, but most of the motion is then the library's, against the rule that AI Fill is AI, so the fix is in the
model.

1. **Fixture and measure, no training.** A "steps stay regular" measure in `ac_train.evaluate` and in the fill's
   diagnostics, and a fixture of this fill. Baseline the shipped model on many long walking fills: only one has
   been measured so far.
2. **Retrain with options already built.** `--context-after` (140000) and `--multi-key` (142000), never trained.
3. **Only if that fails:** give the model the anchor source's contact timing as an input, a network change and
   another run.

Proposed order: before 154000, because add-ons are tied to their base's exact weights and would have to be
redone after a base retrain. 154000's prerequisites stay as they are until the owner agrees.

## Acceptance
- **Measure:** on a fill, feet alternate and no step is under half the fill's median step length. The fixture
  pins today's half step as the known result, so the gate stays usable, and flips to the pass condition when a
  model passes.
- **Baseline:** the result lists how many held-out long walking fills fail on the shipped model.
- **Fix:** the same fills pass on the new model, with anchors no denser than today's and slide on
  `ai-fill-long-gap` still under 2 cm (167000).
- **Owner's eye:** side-by-side videos of this fill and a few others, physics off, approved by the owner.
