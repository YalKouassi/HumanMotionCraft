# AI fill over long gaps

- **Status:** not started
- **Requires:** 103000
- **Source:** old `ROADMAP.md` row 1

## What
Today `ai.fill` refuses gaps over 45 frames. Place anchor poses from the mocap library inside the
gap, then fill the shorter spans between them.

## Acceptance
A 90-frame gap between a stand and a walk fills in one undo step; the anchors are the same on
every run; a fixture checks the anchor frames and that the feet stay planted.

## Notes
The spans between anchors meet at the anchors, so they need the same easing as any two
neighbouring fills (`crates/ac-core/src/ease.rs`).
