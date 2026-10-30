# A regression suite for reproduced bugs

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** none
- **Source:** 115100

## What
Two things, on the fixture and visual golden tests we already have, with no new test system:
- **Bug fixtures:** a fixed bug lands as `fixtures/bug-<slug>.json`, with what went wrong in its
  description and hand-worked numbers. Only reproduced bugs go in.
- **A quality ratchet over the built-ins:** one test measures every clip in
  `assets/library/animations` (`clip.metrics`, keys and shown) against a checked-in table: worst
  slide, penetration, pops, balance frames, and the largest root speed above the keyed speed. A clip
  that gets worse fails; one that gets better lowers its row. So the bugs of 115100 are pinned today
  and can't come back once fixed, without a failing fixture sitting in the gate.

## Acceptance
`cargo nextest run -p ac-cli` runs the ratchet on all 55 clips, and it fails when `physics.rs` is
deliberately broken; `docs/testing.md` has rows for both; `cargo xtask check` passes.
