# Review app: training sets

- **Status:** in progress
- **Source:** owner request (2026-11-01)
- **Requires:** none

## What
A training set is a named, versioned list of clips and labels. A dropdown at the top of `ac-review` picks it, with New, Duplicate and Copy labels from X. The app also reads a `.clip.json` library (`--library`). A set holding private clips is saved outside the repo; the app refuses to write one inside a git checkout.

## Acceptance
Create, duplicate and copy labels through the window and through `review.*` commands, with tests. A set with a library clip refuses a path inside a repo.
