# Say why a fill might look wrong

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** 126000
- **Source:** 115000 (review 1 §4)

## What
Around a filled span, show measured problems, not a made-up confidence: feet that slide, a root
path far from the keys' path, a velocity jump at a key, joints bent back by `keep_possible`.
`ai.fill_all` already returns filled, skipped (with reasons) and notes. Add the measurements to
the fill result and mark them on the timeline (`TimelineMarks`).

## Acceptance (proposed, 2026-10-30)
`ai.fill`, `ai.fill_all` (one entry per gap filled) and `ai.toggle` (to the model) return
`diagnostics`: per-span measurements on the keys the fill wrote, not a confidence score:

- **foot slide** per contact span, in cm, from the app's own contacts (`crates/ac-core/src/contact.rs`
  through `clip.metrics`'s `sliding`);
- **root path**: the root's largest distance in metres from the keys' own path (the clip's
  interpolation with the span's AI keys taken out), and on which frame;
- **seams**: at each of the span's two keys, the largest change of any joint's speed (m/s,
  `clip.metrics`'s `keys`) and of any bone's spin (rad/s, the new shared `metrics::spin_change`)
  across the key;
- **bent**: the joints `keep_possible` bent back, with how far past the limit (degrees), the worst
  frame and the frames.

`flags` lists what went past a line, each with its frames and the reason in words; the thresholds
are named constants in one place (`crates/ac-core/src/filldiag.rs`: `SLIDE_CM` 2 cm and
`POP_SPEED` 1 m/s reuse `clip.metrics`'s lines, `PATH_M` 0.10 m, `SPIN_RAD_S` 3 rad/s, `BENT_DEG`
0, every bent joint). Rule 1: `notes` says what could not be measured (a seam on the animation's
first or last frame, no planted foot in the span, the range tables not loaded, a measurement that
failed), never silently empty. A fixture checks the numbers on a known fill, hand-worked
(`fixtures/ai-fill-diagnostics.json`, `ai_force 0` so the fill is the plain interpolation); the
joints bent back are checked as data in `crates/ac-core/tests/possible.rs`, since a model-made
impossible pose can't be hand-worked. The app marks every flag on the timeline
(`self.timeline.marks.set("ai-fill", marks)`, the reason as the mark's text) and the AI panel lists
the numbers, the flags (a click goes to the frame) and the notes; a new fill replaces them and any
other change to the animation drops them, as the Review panel's marks are.

## Result
Built 2026-10-30, branch `session/2026-10-30-roadmap`.

- Extension point first (own commit): `Document::keep_possible_report` returns `Kept { notes,
  checked, bent: Vec<Bent> }` (`keep_possible` is its notes, unchanged); `ai::Filled` carries it.
- `crates/ac-core/src/filldiag.rs` (new): `diagnose(doc, &Filled) -> Diagnostics`, `flags()`,
  `json()`, the threshold constants and their table. `metrics::spin_change` is the one new
  measure (rad/s); `metrics::pops` got a stated tie rule (`POP_TIE` 1e-6 m/s: when the whole body
  jumps every joint ties but for IK rounding, and the first in rig order, `root`, is named).
- Outputs declared in `commands/returns.rs` (`diagnostics` required on `ai.fill` and
  `ai.fill_all`, optional on `ai.toggle`); `docs/commands.md` regenerated.
- App: `App::fill_marks(id, result)` (panels/ai.rs) from `App::run`, the control port and the
  `--script` loop; `fill_marks_drop_stale` each frame; the AI panel's report lines.
- Cost: a 30-frame fill 13.8 → 13.9 ms in a debug build (`ac fixtures --only ai-fill-speed --timings`).

Result shape (`ai.fill`; `ai.fill_all` returns a list of these):
```
"diagnostics": {
  "from": 20, "to": 30,
  "sliding": [{"foot":"L","from":23,"to":30,"slide_cm":8.4}, {"foot":"R", ...}],
  "sliding_max_cm": 8.4,
  "path": {"max_m": 0.0, "frame": null},
  "seams": [{"frame":20,"dv":0.84,"joint":"root","dw":1.5708,"bone":"eye.L"},
            {"frame":30,"dv":null,"joint":null,"dw":null,"bone":null}],
  "bent": [],
  "flags": [{"kind":"sliding","from":23,"to":30,"text":"The left foot slides 8.4 cm while it is planted on frames 23–30 (over 2 cm)."}, ...],
  "notes": ["key 30 is the animation's last frame: the seam there has no frame after it to measure"]
}
```

Fixture numbers (`fixtures/ai-fill-diagnostics.json`, all hand-worked, passing): standing keys 0
and 10 filled: both feet planted 0–10, 0 cm, path 0 m, both seams unmeasurable with the notes
saying why, no flags. Keys 20 (0.40 m along X) and 30 (0.52 m, the left eye turned 30°): filling
0–10 again gives a 1.2 m/s pop at key 10 (flagged); filling 20–30 gives 8.4 cm of slide on each
foot over frames 23–30 (two flags), 0.84 m/s and 1.5708 rad/s at key 20 (under the lines), key 30
unmeasurable (note), path 0 m, nothing bent.

Screenshot (static marks, so a still): `target/shots/2026-10-30/ai-fill-diagnostics/marks.png`
(script beside it): a grey triangle at key 20 (the 1.44 m/s pop) and the orange span 23–30 (the
slide) on the timeline's mark lane; the AI panel reads "Fill 21–29: 3 things to look at. Slide
8.4 cm, root 0.00 m off the keys' path, key 20: 1.44 m/s, 0.00 rad/s, key 30: not measurable, 0
joints bent back." with the three flag lines under it.

Decisions taken without asking (2026-10-30): `ai.accept` (keeping a preview) and `anim.steps` /
`pose.sequence` fills don't return diagnostics (the acceptance names `ai.fill` and
`ai.fill_all`; `ai.toggle` is the same fill so it does). A new fill replaces the last report rather
than merging, because easing through a shared key re-curves up to 4 frames of the neighbouring
span, which would leave a kept neighbour's seam number stale. Marks for a fill run inside
`edit.batch` are not set (the batch's result doesn't carry the step's).

## Review (2026-10-31)
Code review of commits `720333a` and `3b3769c` and the code as it is now.

| # | Finding | Evidence | Severity | Done |
|---|---|---|---|---|
| 1 | `ai.fill_all` measured each gap right after filling it. The next gap's fill eases through the key they share (`ease.rs`, 4 frames into the earlier span) and plants that span's feet again, so the earlier entry's seam was stale: in the diagnostics fixture 0–10 reported 1.2 m/s and a pop flag at key 10 where the kept motion has 0.525 m/s and no flag. | `crates/ac-core/src/commands/ai.rs` (`diagnose` inside the fill loop, before this review) | medium | fixed: every gap is measured once all are filled; `fixtures/ai-fill-diagnostics.json` gets a `fill_all {refill: true}` step with the eased numbers worked by hand from the Hermite (0.525 m/s at 10; 0.3675 m/s and 0.6872 rad/s at 20; the root 0.01125 m off the path on 9, 11 and 0.007875 m on 21), then an undo so the end state (and the visual golden) is as it was |
| 2 | With a multi-key model (`ai.info` `multi_key`), each gap's entry repeats the whole pass's `bent` list, frames outside the gap included. | `crates/ac-core/src/commands/ai.rs` (`kept: f.kept.clone()` per gap of a pass) | low | not fixed: the shipped model fills one gap at a time, so no fixture can show it; filtering `Bent.frames` to the gap and recomputing `worst_frame` is the fix when a multi-key model ships |
| 3 | Thresholds and units: `SLIDE_CM` 2 cm against `Slide.slide_cm` (cm); `POP_SPEED` 1 m/s against `pops` (positions in m × fps); `SPIN_RAD_S` 3 rad/s against `spin_change` (rotation vector × 2 × fps, rad/s); `PATH_M` 0.10 m against root offsets in hip heights × `hip_height` (m); `BENT_DEG` 0 against `Bent.past_deg` (degrees). `metrics::pops` lists every keyed frame in the range (anchors included), so a seam at an anchor is measured. | `crates/ac-core/src/filldiag.rs:27–37,165–173,181–199`; `crates/ac-core/src/metrics.rs:485–527,528–545` | — | checked |
| 4 | Rule 1: a seam on frame 0 or the last frame, a span with no planted foot, a failed measure and missing range tables each add a note; flags are sorted by frame, then kind (deterministic). | `crates/ac-core/src/filldiag.rs:92–153,294` | — | checked |
| 5 | Outputs: `diagnostics` required on `ai.fill` and `ai.fill_all`, optional on `ai.toggle` (the plain side has none); every key the JSON carries is in the declared description. The app marks every flag (`fill_marks`) from `App::run`, the control port and the `--script` loop, and drops them when the clip or links change (`fill_marks_drop_stale`); a fill inside `edit.batch` sets no marks, as the Result says. | `crates/ac-core/src/commands/returns.rs:104–111`, `crates/ac-app/src/app/panels/ai.rs:48–104`, `crates/ac-app/src/app.rs:336,449`, `crates/ac-app/src/app/agent.rs:111` | — | checked |
| 6 | Panels: every JSON read in `fill_marks` and `fill_report` goes through `as_*` with a default; no indexing on hostile input. | `crates/ac-app/src/app/panels/ai.rs:52–160` | — | checked |

What the coordinator should verify at build time: `ac fixtures --only ai-fill-diagnostics`. If the new step's numbers are off by more than the 1e-3 tolerance, the suspect is foot planting from the model's contacts on 10–20 (the review assumed none, as 20–30 showed none); the fix itself does not depend on the step.
