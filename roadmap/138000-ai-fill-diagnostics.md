# Say why a fill might look wrong

- **Status:** not started; needs an agreed acceptance line
- **Requires:** 126000
- **Source:** 115000 (review 1 §4)

## What
Around a filled span, show measured problems, not a made-up confidence: feet that slide, a root
path far from the keys' path, a velocity jump at a key, joints bent back by `keep_possible`.
`ai.fill_all` already returns filled, skipped (with reasons) and notes. Add the measurements to
the fill result and mark them on the timeline (`TimelineMarks`).

## Acceptance
To be agreed with the owner.
