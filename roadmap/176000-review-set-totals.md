# Review app: live totals and source warning

- **Status:** built, waiting for the owner
- **Source:** owner request (2026-11-01)
- **Requires:** 174000

## What
Minutes per label, and clips and minutes that are mine or purchased. Each pack is marked mine or purchased in a file outside the repo; unmarked counts as purchased. A public set holding a purchased clip warns.

## Acceptance
Hand-worked totals in a test; the warning shows on a public set with a purchased clip and on one with an unmarked clip.

## Result
A band under the set dropdown and `review.totals` give minutes per label and clips and minutes mine or purchased, live; `review.source` and the band mark each pack in `sources.json` beside the sets, refused inside a git checkout (`crates/ac-review/src/totals.rs`). Totals count kept clips less their cuts; a clip with two labels counts under both. A public set warns of every purchased or unmarked clip it holds, kept or rejected. A clip's pack is its set name or its library group. Hand-worked tests are in `totals.rs` and `tests/sets.rs`. Written without a build: run them at integration.
