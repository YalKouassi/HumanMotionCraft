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
