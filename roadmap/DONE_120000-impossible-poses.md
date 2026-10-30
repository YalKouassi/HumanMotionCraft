# Impossible poses

- **Status:** done (2026-10-30)
- **Requires:** 101000, 103000
- **Source:** old `ROADMAP.md` row 9

## What
Three levels per joint: typical (inside the clinical limits), unusual or athletic (past them but
within what reviewed mocap reaches; a warning, fine to keep) and impossible (past the `possible`
limits in `assets/rig/ranges.json`: a foot turned round, a knee bent the wrong way).
`clip.possible` measures a clip, `clip.fix_impossible` bends keys back (adding keys where the curve
swings past), the Range panel shows the level with a Fix button, and every AI fill bends its own
impossible frames back and says so in `notes`. Built-ins that were impossible are fixed
(Acrobatics 3, Jump 5, Fall and get up 1, Sit 7, Fight 1) or deleted (Acrobatics 1, 2, 4, Dance 1
to 3, On the ground 1 to 4, Sit 5, 6); the review rejects 353 CMU duplicate takes (subjects
recorded twice).

## Acceptance
Every built-in reads typical or unusual in `clip.possible`; `fixtures/clip-possible.json` and
`crates/ac-core/tests/possible.rs` pass; screenshots of the impossible and unusual banners in
`target/shots/2026-10-29/possible/`.

## Result
2026-10-30. Confirmed by the owner. `crates/ac-core/tests/possible.rs` passes (3 tests). Banners:
`target/shots/2026-10-29/possible/` (`impossible.png`, `unusual.png`).
