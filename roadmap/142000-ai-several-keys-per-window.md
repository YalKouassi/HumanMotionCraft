# Several keys in one window

- **Status:** built (2026-10-30); its acceptance is measured by the next retrain
- **Requires:** 129000
- **Source:** 115000 (review 2 §5, corrected)

## What
Train with a random set of known frames inside the window, not only "context + key B", so short
neighbouring gaps can fill in one pass and agree at their shared keys. One pass per clip is not
possible: the window is 56 frames (`training/ac_train/model.py:36`). Time-to-go becomes time to the
next and from the last known frame.

## Acceptance
Two neighbouring gaps that fit one window fill in one pass with no kink at the shared key without
`ease.rs`.

## Result
2026-10-30. Built as a model-card option (`multi_key`, default false); the shipped model's inputs
and goldens are unchanged, and `ai.fill_all` behaves as before with its card. No model has been
trained with the option yet (the owner trains overnight), so the acceptance line's "no kink" is
measured by that retrain; the plumbing is proved on a synthetic model.

- **Training** (`ac_train.train --multi-key`): half the windows (`train.MULTI`) also know 1 to 3
  frames inside the gap (never all of it), drawn at random: the keys between two or three
  neighbouring gaps. The plain interpolation then runs between neighbouring known frames
  (`model.interpolate_known`) and the time channel is two wide for every window of such a model,
  time to the next known frame and from the last (`model.known_times`; 0, 0 on a known frame).
  The loss masks each window's own known frames (`losses.terms(mask=)`). The card says
  `multi_key: true`; the network's input is one wider.
- **The reference** (`ac_train.infer.fill(keys=[...])`): keys inside the stretch come back as
  the clip has them; the export adds an eleventh fill golden (keys 10–30 with 17 and 24 inside)
  and a net golden with a known frame inside the gap and the two-wide time channel, for a card
  with the option.
- **The app** (`crates/ac-ai`, `crates/ac-core`): `Net` reads `multi_key` from the card and takes
  the two-wide time channel; `Model` builds one window for any set of keys (`ac_ai::window`:
  context, the stretch, every key known, context after), the plain interpolation between
  neighbouring known frames; a fill between two keys is the same window as before.
  `Inbetweener::fill_keys` (a default that refuses on a model without the option),
  `Filler::fill_keys`, `edits::refill_keys` (the keys inside stay, every other frame becomes an
  AI key) and `Document::ai_fill_keys` (feet planted over the whole stretch as across a long
  fill's anchors; impossible poses bent back per span; the ease runs through the outer two keys
  only, never the shared ones). `ai.fill_all` groups neighbouring gaps into passes while the
  stretch fits the model's limit (`gaps::passes`) when the card says `multi_key`, and reports
  them under `passes` (new output, empty otherwise). `ai.info` shows `context_after` and
  `multi_key`. A key edit still refills the spans on both sides of it one at a time
  (`resync_ai`): the kink the ease hides comes back there until that path uses a pass too.
- **Tests:** `crates/ac-ai/tests/multikey.rs` writes a tiny synthetic model (one encoder layer,
  output layer at zero, so its fill is exactly the plain interpolation between neighbouring known
  frames) with `context_after 4, multi_key true`: `fill_keys` over keys 10, 20, 30 of the walk
  gives 19 frames, key 20 as the clip has it, each half the fill of that gap alone; `ai.fill_all`
  on a document with keys 0, 10, 20 fills both gaps in one pass (`passes: [[0, 10, 20]]`), key 10
  stays yours, and every AI key is the plain interpolation, which an ease through 10 would have
  re-curved; the same document with a one-gap synthetic card gets two passes and the ease. The
  shipped model refuses `fill_keys` and says so. `gaps::passes` has its own test. Honestly: with
  the output layer at zero the two halves meet at the key with the interpolation's own kink;
  whether a trained model removes it is what the retrain measures, with `ease.rs` off.
- **Smoke run:** see 140000 (the same run carried both options; its export passed the Rust port's
  goldens, including the keys case).
- **Decisions made without the owner:** the known frames inside a gap are drawn uniformly, 1 to
  3 of them, on half the windows (the other half are the single-gap windows the app sends most);
  `ai.fill_all` joins passes greedily in frame order; `ai.fill` and `ai.toggle` keep filling one
  gap (a pass needs neighbouring empty gaps, which only fill-all has in hand).

## Overnight command (not run)
From `training/`, with 140000's option in the same run (or without `--context-after 4` to tell
them apart):

```bash
uv run python -m ac_train.train --minutes 180 --tag parts-multikey --context-after 4 --multi-key --loss-weights vel=0.1,jerk=0.0005,skate=0.1
uv run python -m ac_train.evaluate --tag parts-multikey --checkpoint best
uv run python -m ac_train.export parts-multikey --checkpoint best --as parts    # only if it wins; then cargo nextest run -p ac-ai
```

To read the acceptance: with the exported model, key a walk on frames 0, 12, 24 (two gaps that fit
one window), `ai.fill_all` (one pass), and compare `clip.metrics`' speed change at frame 12 with
the same fill made one gap at a time with the ease off.
