# AI settings presets

- **Status:** built, waiting for the owner (2026-10-30)
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
