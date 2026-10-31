# Feet stay planted on long fills

- **Status:** done (2026-10-31)
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

## The owner's finding: a half step (2026-10-31)
The owner watched the video: left full, right full, left half, right full, then full steps. Measured on
`ai-fill-long-gap` (y of each foot's contact, metres; one 90-frame fill, so one case only):

| Anchors on | Contacts in order | Reading |
|---|---|---|
| 30, 60 (today) | L -0.41, R -0.91, L -0.89, R -1.34, L -1.97 | the half step: the second left lands beside the right |
| 30, 60, no `hold_contacts` | L -0.40, R -0.88, L -0.82, R -1.25, L -1.87 | the same, so the hold didn't cause it |
| 33, 66 | L -0.22, R -0.81, L -1.19, R -1.27, then L -1.78 | a short step moves later |
| 22, 44, 66 (every 2 source steps) | L -0.54, R -0.75, L -1.12, R -1.33, L -1.79, R -1.63 | alternates; lengths uneven, last step goes back |
| about every 11 (every source step) | L -0.53, R -0.84, L -1.17, R -1.46, L -1.74, R -1.99 | regular, 0.25 to 0.33 m a step |

The anchor segment plays at 1.5x, so a step is 11 frames and 30 frames is 2.7 steps: the span has to fit
a step count that doesn't divide evenly. Anchors on every step give the pattern back, but then most of the
motion is the library clip's and little is the model's, which sits badly with the rule that AI Fill is AI
and doesn't go through the Walk loop. None of these trials is in the code.

## Closed
Accepted by the owner (2026-10-31), closed on the foot slide: 1.894 cm against the 2 cm line. The half step the owner saw moves to 153500.
