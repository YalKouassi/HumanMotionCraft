# AI fill over long gaps

- **Status:** built, waiting for the owner (2026-10-30)
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

## Result
Built 2026-10-30 (commits `121000:` on `session/2026-10-30-roadmap`).

**Where.** `crates/ac-core/src/anchors.rs` (the rule), `crates/ac-core/src/posesource.rs` +
`crates/ac-io/src/anchorlib.rs` (the extension point: a `PoseSource` the document asks for the
library's clips; `ac-io` serves the session library, cached by file time and size),
`AiFiller::fill_long` in `crates/ac-core/src/ai.rs` (the chain), `ease.rs` (an anchor is a key the
ease goes through), `gaps.rs`, `filldiag.rs` (seams at the anchors too).

**Exposed through `ai.fill` itself** (no new command): a gap over the model's 45 frames is
filled with anchors unless `anchors: "none"` is given (then refused with the reason). `ai.fill_all`,
`ai.toggle`, `ai.preview` and a key move next to a filled long gap take the same path, so
`ai.gaps` no longer refuses long gaps when a library is there (the AI panel says "split by N
anchor poses from the library"). Without a library (headless without `--library`) the fill is
refused with the reason (Rule 1), it never guesses. The result lists `anchors: [{frame, id,
source_frame}]` and `anchor_segment: {id, from, to, speed, match_deg}`.

**Anchor rule (deterministic).** Frames: the fewest evenly spaced frames keeping every span at or
under 30 frames key to key (0–90: anchors on 30 and 60). Poses: of every library animation made
for this rig, every start frame and every speed in {0.5, 0.75, 1, 1.5, 2} library frames per
animation frame, the segment whose start pose is nearest key A and whose pose `speed × (B − A)`
frames later is nearest key B (mean angle over the model's bones, the segment turned so its
start faces key A's way; a loop wraps; ties to the first library id, then the lower frame, then
the slower speed). Each anchor is that segment's pose at `start + speed × (frame − A)`, root on
the keys' own interpolation plus the segment's sway from its chord. First draft used "the library
pose nearest the plain blend at each anchor frame": degenerate (it picks copies of the two keys,
60–90 became a hold and the feet shuffled 13.5 cm), so it was replaced the same day.

Anchors land as AI keys tagged `anchor <id> @<frame>` (bands on the timeline), cleared and
refilled with the fill around them; the ease runs through them; the model's foot contacts of the
sub-spans are merged so a step across an anchor is one contact planted on one spot (a first
per-sub-span planting made a 2.7 m/s pop at the anchor).

**Fixture `fixtures/ai-fill-long-gap.json`** (passes): stand on 0, the built-in walk's frame 8 on 90
with its root 2 m ahead; `ai.gaps` → 89 frames, 2 anchors; `ai.fill` → anchors 30 and 60 from
`Locomotion/Walk` frames 50 and 95 (segment 5→140 at 1.5×, match 7.2°); bands 1–29, 30 (anchor),
31–59, 60 (anchor), 61–89; one undo step "AI fill 1–89"; the same anchors and the same foot.L
position on frame 45 after undo and a second fill; seams at 30 and 60: 0.57 and 0.32 m/s, 0.66
and 0.77 rad/s (no pop or spin flag); no joint bent back. Feet: the planted feet slide up to
11.5 cm per contact with physics off (8.6 cm with physics on) — above the 2 cm line, as a normal
30-frame fill of the same stand-to-walk is (2.6 cm) and the mocap walks' own keys are (4–8 cm,
`tests/quality/builtins.json`). This is the model's fill between real walk anchors, not faked;
recorded rather than hidden. `fixtures/ai-fill-all.json` and `ai-toggle.json` changed with the
behaviour: their 59-frame gaps are now filled (one anchor) instead of refused.

**MP4s** (`ac video`, physics off, 91 frames at 30 fps):
- `target/shots/2026-10-30/ai-fill-long-gaps/stand-to-walk-90-frames-side-fixed.mp4` — fixed
  camera, side: the body starts standing, strides at about 1.0 s and 2.5 s, covers 2 m. Look at
  the feet through the anchors at 1.0 s (frame 30) and 2.0 s (frame 60): no jump; and at whether
  the stance foot creeps (the slide the numbers report).
- `.../stand-to-walk-90-frames-three-quarter.mp4` — following camera, the whole body.
- `.../strip-side-big.png` — frames 0, 20, 30, 45, 60, 75, 90.

**Left open.** `README.md` still lists long-horizon fill under "What HumanMotionCraft Will Be":
move it up once the owner confirms. The slide numbers say the chained fill is weaker than a
short one; a better match might weigh the keys' travel against the segment's (only clips with
travel), left for the owner to judge from the video.
