# Choose which AI model fills

- **Status:** not started
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

## Acceptance
- A fixture lists two models, fills with each and gets the expected different results, and one refused
  model reports why with the base used instead.
- `session.info` gives the chosen model's tag and hash, and the same inputs still give byte-identical
  output.
- A screenshot of the panel with two models, approved by the owner.
