# AI settings presets

- **Status:** done (2026-10-31)
- **Requires:** 126000
- **Source:** 115000 (review 1 §4)

## What
A few named presets ("Preserve my motion", "Natural movement", "Follow style") that set the
existing AI sliders (`ai_force`, `style_force`, `trajectory`, `head`) together, with the sliders
still there. Presets only set the model's settings. They never route AI Fill through a walk loop or
another hand-made gait: those belong in their own visible template feature.

## Acceptance
`ai.settings` accepts `preset: "preserve" | "natural" | "follow_style"`, setting the 4 sliders;
individual slider arguments override preset values; unknown preset is rejected; UI panel offers
preset buttons. Tested in `fixtures/ai-settings-presets.json`.

## Result
2026-10-30. `ai.settings` supports `preset`:
- "preserve": `ai_force` 0.5, `style_force` 0.0, `trajectory` 0.8, `head` 0.8.
- "natural": `ai_force` 1.0, `style_force` 1.0, `trajectory` 0.0, `head` 0.0.
- "follow_style": `ai_force` 1.0, `style_force` 1.5, `trajectory` 0.2, `head` 0.0.
The AI panel provides preset buttons ("Preserve motion", "Natural", "Follow style") directly above the sliders.
Verified with fixture `fixtures/ai-settings-presets.json`.

## Review (2026-10-31)
Code review of commit `119f430` (the part for this item) and the code as it is now.

| # | Finding | Evidence | Severity | Done |
|---|---|---|---|---|
| 1 | A preset sets `ai_force`, `style_force`, `trajectory` and `head` and nothing else: `style` is untouched, the preset is applied before the individual sliders (so a slider given with it overrides), and an unknown preset is refused. Nothing routes AI Fill through a walk loop: the Footsteps row's "Walk loop" is `anim.steps` with `fill: cycle`, a separate feature, unchanged by this item. | `crates/ac-core/src/commands/keys.rs:647–702`; `crates/ac-app/src/app/panels/ai.rs` (`steps_row`) | — | checked; the fixture now also checks `style` stays `any` |
| 2 | The panel returned before drawing the sliders on the frame a preset button was clicked, so the controls jumped for a frame and showed the old values until the next. | `crates/ac-app/src/app/panels/ai.rs` (`return` after the preset ran, before this review) | low | fixed: the settings are read again after the preset runs |
| 3 | `preset` accepts aliases the acceptance does not name (`preservemotion`, `preservemymotion`, `naturalmotion`, `naturalmovement`, `style`, and any case or spacing). Harmless, but the parameter's help and `docs/commands.md` name only the three. | `crates/ac-core/src/commands/keys.rs:650–662` | low | not fixed: left as the owner's choice (keep the aliases and document them, or drop them) |
| 4 | `ai.settings` is not undoable (as before this item): a preset click cannot be undone with Ctrl+Z, only by another preset or the sliders. Pre-existing. | `crates/ac-core/src/commands/keys.rs:231` | low | not fixed: a product decision |
| 5 | Outputs match `AI_SETTINGS` (the five settings, required); `docs/commands.md:184` and `fixtures/schema.json` carry `preset`. | `crates/ac-core/src/commands/returns.rs:112–123` | — | checked |

Accepted by the owner (2026-10-31).
