# Flag the takes the model fails on

- **Status:** not started
- **Requires:** 126000
- **Source:** 115000 (review 2 §14)

## What
After training, evaluate every take's windows and put the worst 1 % into the `ac-review` queue
with their loss. They go for a person to look at, never an automatic rejection: high loss is often
just hard motion (acrobatics), not bad data.

## Acceptance
`ac-review run review.list` can filter to the flagged takes; flagging writes nothing to
`training/review/decisions.json` by itself.
