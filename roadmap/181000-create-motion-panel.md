# Create motion: one plain panel for making motion

- **Status:** built, waiting for the owner
- **Requires:** none
- **Source:** owner request (2026-11-01): too many toggles and fill types; better UX and plain words, losing no function

## What
Organise the inspector by what the person wants to do, not by engine. One **Create motion** section
asks what to do: Fill between my poses, Describe it, or Steer it live. Each shows only its own
controls. Our model is named **HumanMotion AI** (owner, 2026-11-01), the pose-to-pose one NVIDIA AI.
Setup moves to an **AI models** window (Settings, Fine-tune, or a Set up button): installs, licences,
Load and Stop, sample clips, the choice among our own models. Physics reads Realistic balance, its
counts under Details and its button only when there is something to check. Command ids, labels and
widget ids stay, except tags that were labelled with the old words.

## Acceptance
- Every control the three old sections had is still reachable (the table below).
- Without a gap, the panel says how to get one; a gap the AI can't take says why in full.
- Screenshots of each job and the window; a real fill, and Fill every gap, clicked through the app.
- `cargo xtask check` passes, and the owner approves.

| Old | Now |
|---|---|
| Fill now, Preview fill | Apply, Preview |
| Fill all gaps | Fill every gap, then Apply |
| Fill report | Report |
| Footsteps and its menus | Movement: Walking steps |
| Forces and Shift+A hint | Stay closer to; the rest under Fine-tune |
| Model menu | AI models |
| Generate, Fill gap, Take | Create, Fill this gap, Version |
| Pose fill's Fill gap | Apply with NVIDIA AI chosen |
| Load, Stop helper, Sample clips, installs, licences | AI models |
| Start take, Stop take | Start, Stop |

## Result
Built 2026-11-01. Screenshots: `target/shots/2026-11-01/create-motion/`.

**Changed from the suggestion:** "Try another version" wasn't added, since a button that makes new motion
at once would stack results; Version takes a number. "Keep head steady" is "Keep head as posed", since the
setting holds the head to the keys rather than steadying it.

**The licence notice** now shows at install, in the NVIDIA AI hover and in AI models, no longer on every
use (163000 showed it wherever the model was offered; the owner asked for this change).

**Not changed:** menus and the palette keep the commands' own labels (Secondary motion, Travel between
keys), as those are the command registry's.

**Clicked through the release app (2026-11-01, HumanMotion AI, three keys):** Preview showed the ghost
and Discard dropped it, Apply filled the gap at the playhead, and Fill every gap with Apply filled both.
**Not clicked:** anything with NVIDIA AI chosen, Describe it's Create, a take, or an install from the new window.
