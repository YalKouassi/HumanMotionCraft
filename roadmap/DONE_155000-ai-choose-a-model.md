# Choose which AI model fills

- **Status:** done (2026-10-31)
- **Requires:** 154000
- **Source:** owner's preferred direction (2026-10-30)

## What
The app always loads one model: `parts`, else the first by name (`crates/ac-ai/src/lib.rs:128`). With
add-ons, the person picks one.

- Commands first: `ai.models` lists the bundled and installed models with their cards, and `ai.settings`
  (or a new `ai.model`) picks one by tag. The fill records which model made it, so a saved animation
  still says where its motion came from.
- The AI panel's style menu becomes a model choice, and the base model's styles, if any are left, stay
  under it.
- An add-on is checked against what the app expects (bones, context, gap, its base's hash), and a mismatch
  is announced with a fallback to the base (Rule 1).
- Fixtures, scripts and agent sessions keep using the bundled default unless a command picks another.

Mixing two add-ons in one fill is not part of this item.

## Note (2026-10-31): built before 154000
154000 (add-on models) is not done and the owner has not asked for training. The code side of this item
needs no trained add-on, only a second model file to choose between, so it was built now; the Requires
line stays, and the item is not done until an add-on from 154000 has been chosen through it. Decisions
made on the way, all routine:

- **`ai.model {tag}`**, a command of its own, not a field on `ai.settings`: a model is a session's
  choice like the settings, but choosing one can change the settings (a style the new model doesn't
  know goes back to `any`, said in `notes`), so it is not a setting. Not undoable, as `ai.settings` is
  not.
- **Where models come from** is an extension point in `ac-core`, `ac_core::ai::ModelSource` (cards and
  load by tag), set by the front end through `SessionBuilder::models`, as the pose source is. `ac-ai`
  implements it (`ac_ai::models::Models`): the app's own models folder first, then the bundled
  `assets/models`, then any folder `ai.models {folder}` is given (kept on the document for the session,
  so the panel lists it too). The first model with a tag wins; a later one with the same tag is listed
  as refused. The CLI, fixtures and MCP look at the bundled folder only, so a run is the same on every
  machine; the app looks at the installed folder as it did before, and not in automated runs.
- **The checks.** Every model: its weights load (every number and shape in a weights file is hostile:
  offsets, products and the head count are checked, and `crates/ac-ai/tests/choose.rs` throws
  thirteen broken files at the loader), and its bones are bones of the rig. An add-on (a card with
  `base` and `base_hash`): its base is here, the base's weights hash is the one it names, and its
  bones, context, gap and context-after are its base's. A refused model is listed with the reason
  (`loads: false`, `refused`), never loaded; `ai.model` on it announces the reason in the result and
  uses its base (else the default) instead; the panel greys it with the reason on hover and under the
  menu.
- **What a fill records.** `Key.model_hash` (saved only when set, read with a default: the format
  version does not move) and `model_hash` on `ai.fill`, `ai.fill_all` and `ai.toggle`, beside the
  label already there. `session.info` already gave the model's tag and hash.
- **Test models.** A copy of the shipped weights is 19.5 MB, too big for the repo, so
  `python -m ac_train.tinymodel` writes two tiny deterministic models under `fixtures/data/models/`
  (the smallest shape the loader takes, every weight zero but the output bias, so a fill is the plain
  interpolation with the root raised by a fixed number of hip heights: a hand-worked number).
  `tiny` (6.8 KB) is a base model of its own; `stale` (18 KB) has the shipped model's bones and names
  `parts` as its base with a hash it never had, so it is refused for that one reason.
  `training/tests/test_tinymodel.py` checks the committed files are byte for byte what the generator
  writes. Nothing is trained.
- **`ai.fill`'s goldens and fixtures are unchanged.**

## Acceptance
- A fixture lists two models, fills with each and gets the expected different results, and one refused
  model reports why with the base used instead.
- `session.info` gives the chosen model's tag and hash, and the same inputs still give byte-identical
  output.
- A screenshot of the panel with two models, approved by the owner.

## Result (2026-10-31, built; not yet confirmed)
- `fixtures/ai-choose-model.json`: `ai.models` lists `parts` (bundled, default), `tiny` (loads, hash
  `ae671a139173c4d0`) and `stale` (refused: "an add-on made for parts with hash 0000000000000000, not
  the parts here"). With `parts` at AI force 0 the fill between the ai-fill keys puts frame 10's root
  head at 1.3073286 m; `ai.model tiny` makes `session.info` say `tiny` and its hash, and its fill
  lifts the root by 0.25 hip heights, 1.3073286 + 0.25 x 0.8698308 = 1.5247863 m, twice the same;
  `ai.model stale` is refused with the reason and `parts` is used instead (1.3073286 m again); a
  style the new model doesn't know goes back to `any` and the result says so. Refused: an unknown
  tag, no tag, a folder that isn't there, an empty folder name.
- `crates/ac-ai/tests/choose.rs`: the cards, the refusal, the installed folder first, thirteen hostile
  weights files refused without a crash. `crates/ac-core/tests/model_hash.rs`: the hash on every key a
  fill writes, through the clip file and back, and a file without one.
- `training/tests/test_tinymodel.py`: the test models match their generator.
- The panel: a Model menu above the Style menu, shown once there are two models or more (the
  bundled one alone has nothing to choose). The screenshot for the owner is still to take: run the app
  with `--script` holding `ai.models {folder: "<repo>/fixtures/data/models"}` and `--screenshot`.

Accepted by the owner (2026-10-31).
Accepted before its prerequisite 154000 was built and without the two-model panel screenshot; the two tiny test models stand in for a second model until then.
