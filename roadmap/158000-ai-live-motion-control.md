# Live motion control: drive the character, then clip it

- **Status:** not started; measure targets and speed first (below)
- **Requires:** 157000
- **Source:** owner's idea (2026-10-30)

## What
Press Play and steer the character with WASD or a controller while MotionBricks generates the motion
live. Everything shown is recorded, and afterwards the person picks a stretch and keeps it as a clip.
This is a new way of making animation: perform it, then cut the good part.

The model suits this. Upstream's Scene Preview demo does the same thing: WASD or a controller stick
moves, a target pose is placed ahead of the character along the desired velocity, and the last four
poses shown become the next generation's context (`docs/scene_preview.md`, `docs/runtime_api.md` in
the upstream repo). A new generation starts when playback nears the transition point or the input
changes enough. The model has no controller input, so all of this is application code we write. The
demo doesn't export motion, so recording and clipping are ours too.



see D:/CreativeFusion/AnimationCraft/reference repos

### The trap: we can't use the demo's walk targets
The model only moves towards target poses, and those decide what the walk looks like. The demo's
come from `assets/smart_primitives/soma/locomotion_bs1_linear.smart_primitive.json` and
`locomotion_style_variations_bs1_linear`, and both point at clips in `assets/motions/` (checked
2026-10-30). Those clips are evaluation-only and come from Bones' data, so anything we keep can't be
made with them. Its six style modes (zombie, combat and so on) go with them. The upstream notice
says the sample licence doesn't cover the Smart Primitive documents themselves, but the README gives
those documents no licence of their own, so we don't copy them either.

**We make our own target sets**, as poses chosen by speed and direction, from what we may already
use: CMU and 100STYLE (which has walking styles), and our generated `kimodo` motion. They are
converted to SOMA and stored under our own format. The upstream docs say any pose with the right
layout works as a target. Whether ours look as good is the main unknown.

### Measure first
Do this before building any UI, and write the numbers here:
1. **Targets:** a script drives a scripted stick path through walk, run and turn with our own target
   set, and the owner judges the result by eye. It needs no UI. If the targets look bad, this stops
   here.
2. **Speed:** time per generation including the helper round trip, on the owner's NVIDIA card through
   TensorRT, as in 157000 (owner, 2026-10-30). Published timings are 2.5 ms for the plan alone on an
   RTX 4090, with nothing for the steps around it. The proposed limit is 100 ms, so the character
   answers the stick within about 3 frames at 30 fps. Time the conversion to our rig separately.

### Live or converted afterwards
Either is fine (owner, 2026-10-30), so the conversion to our rig isn't part of the speed limit. If it
keeps up, the person steers our rig live. If not, they steer a simple figure drawn from the model's
own SOMA joints, and the conversion runs when they clip. The recording holds the model's poses either
way, and the clip step shows the converted result before it is kept, because it can differ a little
from what was steered.

### The feature
- **Commands first:** `live.start`, `live.input` (direction, speed, style), `live.stop`,
  `live.clip` (a range of the recording, into the current animation or a new clip). A fixture steers
  the fake engine with a script, so all of it is testable without a window.
- **Window:** keyboard and gamepad feed `live.input`, using a craft-libs gamepad crate if one exists.
  A controller unplugged mid-take stops the character and says so; it never crashes.
- **Recording:** the recording holds the poses generated, in memory, and is capped. It survives
  `live.stop` so the person can pick a range, and it is not saved in the document until clipped.
- **Clipping:** the kept range is converted to our rig and becomes keys through the same path as
  `posefill.fill`, tagged as generated. Saving uses `clip.save_as`, so the clip reaches the library like any other.
- **First version:** walk, run, turn, stop, and whatever styles our targets give. Interaction, jumps,
  obstacles and scenes are left out.
- **Rule 1:** without the install, or if the helper falls behind or dies, it says so and the
  recording made so far stays.

## Acceptance
- Speed and target results are recorded here, and the owner has judged the target test by eye before
  the UI starts.
- `fixtures/live-motion.json` and the crate's tests pass on the fake engine: a scripted take, a clip
  of a range with one undo step, the cap, a refused range, and a helper that dies mid-take with the
  recording kept.
- No file from `assets/motions/` or `assets/smart_primitives/` is shipped, read or used for targets,
  and each target set's rows in `ATTRIBUTION.md` name where its poses came from.
- A real take with WASD and one with a controller, in the release app, clipped and saved to the
  library, then reloaded and played. Contact sheets of the clip.
- Measured generation time and any dropped frames during those takes are under `## Result`.
- `cargo xtask check` passes, and the owner plays it and approves.
