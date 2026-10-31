# Feet stay planted on long fills

- **Status:** built, waiting for the owner
- **Requires:** none
- **Source:** the 2026-10-31 review of 121000 (finding 4); the owner's decision

## What
`fixtures/ai-fill-long-gap.json` records `sliding_max_cm` 11.5295 per contact, over the 2 cm line, though
121000's acceptance says the feet stay planted.

## Acceptance
`sliding_max_cm` in `ai-fill-long-gap` is under 2, with the fill still meeting its keys and anchors, and the
owner approves a video of a long fill.

## Result
Cause, from reading the code (2026-10-31; not yet confirmed by a run): `plant_steps` and `plant_feet` hold the
ankle, with `two_bone` keeping the foot's own turn, while `clip.metrics` measures the foot's toe and heel points.
A foot rolling over a held ankle moves those points across the floor, and frames the model did not call down are
not held at all. `plant_feet` also lets an ankle drift between two places up to 25 cm apart.

Fix: `Document::hold_contacts` (`crates/ac-core/src/edits.rs`) runs in `finish_fill` for fills with anchors. It finds
contacts with the test the measure uses (`crate::contact`), and holds the foot's lowest point across the floor, the
heel as it lands and the ball as it rolls off, through leg IK. Your keys and the anchors are never written by it;
a contact on one of them holds where that frame has it. The measure, the Walk loop and fills without anchors are
untouched.

Not measured yet: this was written without a build. `sliding_max_cm` in `ai-fill-long-gap` still holds the old
11.5295 until the integration build pins it, and `crates/ac-cli/tests/long_fill_feet.rs` checks the 2 cm line. The
owner's video is still to come.

## Build check (2026-10-31)
Measured in integration with the shipped `parts-natural`: `sliding_max_cm` 1.894, under the line. The same code on the previous model measured 2.006, so the line is met narrowly and depends on the model. Video for the owner, physics off: `target/shots/2026-10-31/long-fill/long-fill-feet.mp4` in the main checkout.
