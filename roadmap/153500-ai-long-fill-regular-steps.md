# Long fills keep regular steps

- **Status:** in progress: step 1 done, the measure corrected and baselined; steps 2 and 3 wait for the owner's word to train
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

## Result
Step 1, written without a build (2026-10-31); nothing here has been run yet.

- **Measure:** `crates/ac-core/src/steps.rs`, returned as `diagnostics.steps` by every fill, with `step_report` in
  `ac_train.evaluate` as the same rule, both tested on the 167000 numbers by hand. A step is signed along the root's
  travel: the feet stand about 0.2 m apart sideways, so plain distance would hide a foot landing level with the other, and
  a step back would pass. A foot already down on the span's first frame is not a contact. Not a timeline flag: only a
  walk should step evenly, and the baseline comes first.
- **Fixture:** `fixtures/ai-fill-regular-steps.json` pins the half step (`regular` false, `short` [1]).
- **Baseline, not run:** `python -m ac_train.evaluate --long-walks 200` fills held-out walks of 90 frames with the
  app's `ai.fill` on the shipped model and writes `target/long-walks/baseline.json`, the recordings' own steps beside
  them. It needs a built `ac` and trains nothing.

## Baseline (2026-10-31)
`python -m ac_train.evaluate --long-walks 200`, shipped `parts-natural`, no training. Of 176 fills with enough steps, 157 fail (89 %); 24 more have too few. **The measure isn't trustworthy yet:** the recordings themselves fail 42 of the 55 with enough steps (76 %), and 145 have too few, so it flags real walks too. On `ai-fill-long-gap` it finds the half step (step 3) and also counts a replant before the first step. Recordings and fills use different contact tests (Python's scan, `crate::contact`). The rule needs correcting against recordings before these rates mean anything.

## The measure corrected (2026-10-31)
Found by reading the code, written without a build; no number here has been run on recordings. Why real walks failed:

- **Two contact tests.** The scan's test wants ankle and ball both within 2 cm of flat and slow, with no hysteresis, so a rolling foot is flat only briefly. Contacts went missing, which reads as too few steps or as the same foot twice. `evaluate.py` now ports `contact::clip_phases` on the app's foot points (every toe's head and the heel), its constants copied and named, and `steps.rs` calls `clip_phases`, so a clip above the floor lands too.
- **A replant counted as a step.** The same foot coming down within 0.10 m of its last place, the other foot not landing between, is one contact at the first one's place. A foot down before the span stays out through a replant.
- **Standing steps were judged.** With the root travelling under 0.10 m between two contacts, its way is noise and a closing step is natural. Such a step is reported but neither judged nor in the median.
- **Windows that weren't walking.** `--long-walks` now needs every second to average at least 0.4 m/s, on top of 0.7 m/s across the window.
- **Turning: unchanged.** The way of travel is the chord between two neighbouring contacts, which follows a gentle curve. The baseline now writes `recording_lengths_m` and `recording_alternate`, to show whether turns still fail.

By hand on `ai-fill-long-gap`: R 0.015 (the replant joined), L -0.42, R -0.92, L -0.90, R -1.35, L -1.99; steps 0.43, 0.50, -0.01, 0.45, 0.64; median 0.45; step 2 alone is short, so it still fails. 0.10, 0.10 and 0.4 are judgement until the baseline runs: recordings should mostly pass.

## Baseline after correcting the measure (2026-10-31)
One contact rule on both sides, a replant in place is one contact, steps with under 0.10 m of root travel aren't judged, walking at 0.4 m/s or more. 200 long walks: fills fail 139 of 177 (79 %), 23 too few steps; recordings fail 26 of 199 (13 %), 1 too few. The rule now separates them, so the half step is common in long fills, about 6 times the rate of real walking. It replaces the earlier baseline above.
