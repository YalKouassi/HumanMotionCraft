# Train a named set; the card records it

- **Status:** in progress
- **Source:** owner request (2026-11-01)
- **Requires:** 174000, 176000

## What
`ac_train.train --set <name>` reads a set file as `--takes`, reading library clips as a set, and the model card records the set's name, version and hash, giving the what's-in-this-model view. A public set with a purchased or unmarked clip is refused, and such a model can't be exported into the repo.

## Acceptance
Tests without training a real model: `--dry-run` on a set prints its takes and minutes; the card fields are written; the refusals fire. No clip is read that the set doesn't name.
