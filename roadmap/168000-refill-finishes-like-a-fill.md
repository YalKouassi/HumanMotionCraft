# A key move or copy finishes its refill like a fill

- **Status:** built, waiting for the owner
- **Requires:** none
- **Source:** the 2026-10-31 review of 121000 (finding 5); the owner's decision

## What
`move_key` and `refill_touching` (`crates/ac-core/src/edits.rs`) refill a span with no ease through the keys
and anchors, no planting and no `keep_possible`. `resync_ai` (`crates/ac-core/src/ai.rs`), used by `key.set`,
does all three.

## Acceptance
A bug fixture moves and copies a key beside a short and a long fill; each span ends as a fresh `ai.fill` with
the same keys would. One undo step each. The existing `ai.fill` goldens don't change.

## Result
`Document::refill_finished` (`crates/ac-core/src/ai.rs`) is now the one refill-and-finish: `key.set`'s `resync_ai`,
`key.move` and `key.copy` all go through it, so the ease through keys and anchors, the planting and `keep_possible`
run on every refilled span. `move_key` and `copy_key` take the model from the document instead of a `Filler`
argument. `ai.fill`'s own path is untouched, so its goldens are not expected to change. Fixture
`bug-refill-finishes-like-a-fill` walks a short and a long fill with hand-worked bands and undo steps;
`crates/ac-cli/tests/refill_finish.rs` compares every key with a fresh `ai.fill`, within 1e-6 (2026-10-31).
Written without a build: neither has been run yet.

## Build check (2026-10-31)
A move, and a copy into a long fill, match a fresh fill to 1e-6. A copy beside short fills matches to 2e-3, not exactly: the span before it stays, so its frames next to the shared key are eased a second time (measured at most 0.0015 in a rotation, 0.0002 in the root).
