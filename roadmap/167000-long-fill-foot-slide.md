# Feet stay planted on long fills

- **Status:** not started
- **Requires:** none
- **Source:** the 2026-10-31 review of 121000 (finding 4); the owner's decision

## What
`fixtures/ai-fill-long-gap.json` records `sliding_max_cm` 11.5295 per contact, over the 2 cm line, though
121000's acceptance says the feet stay planted.

## Acceptance
`sliding_max_cm` in `ai-fill-long-gap` is under 2, with the fill still meeting its keys and anchors, and the
owner approves a video of a long fill.
