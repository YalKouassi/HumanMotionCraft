# A key move or copy finishes its refill like a fill

- **Status:** not started
- **Requires:** none
- **Source:** the 2026-10-31 review of 121000 (finding 5); the owner's decision

## What
`move_key` and `refill_touching` (`crates/ac-core/src/edits.rs`) refill a span with no ease through the keys
and anchors, no planting and no `keep_possible`. `resync_ai` (`crates/ac-core/src/ai.rs`), used by `key.set`,
does all three.

## Acceptance
A bug fixture moves and copies a key beside a short and a long fill; each span ends as a fresh `ai.fill` with
the same keys would. One undo step each. The existing `ai.fill` goldens don't change.
