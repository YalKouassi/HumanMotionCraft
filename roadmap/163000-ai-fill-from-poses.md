# AI Fill can fill from poses, with a licence notice

- **Status:** built, waiting for the owner (2026-10-31)
- **Requires:** none (157000's engine is built and approved; 157000 itself stays open for its training clips)
- **Source:** owner, 2026-10-31: "I'd like to use motionbricks where possible (maybe just notify 'Check NVIDIA X License before usage' and give responsibility to user)"

## What
This reverses 157000's "`ai.fill` keeps using our own model" for the panel, not the command. The AI
in-between panel gets a choice of which model fills: **Our model** (the default) or **Pose fill**,
shown once the optional install is there. `ai.fill` and its goldens don't change, and fixtures and
agents stay on our model unless they ask for `posefill.*`.

Wherever the model is offered (the AI panel when it's chosen, the Pose fill panel before and after
the install, Live motion, the install's own output), the person is told:

> Check the NVIDIA Open Model Agreement and the TensorRT licence before use. You are responsible for
> how you use this model and what you make with it.

The model is still downloaded at install and never shipped (owner, restated 2026-10-31).

A notice covers what people do on their own machines. It doesn't change anything we do ourselves:
what the release ships, and a model we train on these outputs (157000's training clips) are ours
to answer for.

## Acceptance
Chosen by the owner 2026-10-31 ("Choice beside Fill"):

- Preview, Fill now and Fill all gaps work with either model, once the install is there. Without it
  the choice isn't shown and the panel is as before.
- A gap the chosen model can't take says why, and nothing silently goes to the other model.
- `posefill.preview` and `posefill.fill_all` are commands with a check and a refusal in
  `fixtures/pose-fill.json`, on the stand-in engine: a preview kept with `ai.accept` in one undo step,
  and `fill_all` filling what it can and naming the gaps it left.
- `ai.fill` and its goldens are unchanged. `cargo xtask check` passes.
- The notice shows in each place above, and `posefill.install` returns it.
- The owner approves the panel from a screenshot and tries a fill.

## Result
Built 2026-10-31. `cargo xtask check` passes with 83 fixtures, and `ai.fill` and its goldens are unchanged.

- **The choice.** "Fill with: Our model / Pose fill" in the AI in-between section, only once the install
  is there. It always starts on our model. "Pose fill" is the name the app already uses for this
  engine (157000 keeps other products' names out of the UI); its hover says it's NVIDIA's model.
- **New commands.** `posefill.preview` and `posefill.fill_all`; `fixtures/fill-from-poses.json`. Keep and
  discard are the existing `ai.accept` and `ai.discard`.
- **Run on the real model** (RTX 3080 Ti, Walk 12 with frames 12–38 deleted): Preview showed the ghost
  over 10–40 and changed no keys, keeping it was one undo step, and Fill now wrote the same 29 keys.
  One run of that check stalled past 280 s and was cut off, with nothing to show why; the three runs
  after it, logged, took about 20 s each. Watch for it when you try this.
- **Fill all gaps** counts only gaps this model can take, and the status line names what it left.

Review: `target/shots/2026-10-31/fill-from-poses/panel-pose-fill.png` (the choice and the notice) and
`panel-pose-preview.png` (the ghost).
