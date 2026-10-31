# Live motion control: drive the character, then clip it

- **Status:** built, waiting for the owner: the owner plays it with W A S D and a controller. No controller take has been made (2026-10-31)
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

## Result
**Built (2026-10-31):** `live.start`, `live.input`, `live.stop`, `live.show`, `live.clip` in
`crates/ac-posefill` (`live.rs` the take as a pure state machine, `live_commands.rs` the commands, the
model asked off the UI thread), the Live motion section under Pose fill, W A S D with Shift to run,
a controller through gilrs on Windows, keeping a range in this animation or as a new one in one undo
step. `fixtures/live-motion.json` and `crates/ac-posefill/tests/live.rs` pass on the stand-in and a fake.

**Not tested:** no controller take: this needs a person and a controller, so `panels/gamepad.rs` has
never read a real stick. No person has steered it: the takes below are keys held by a script through
the control port, so they say nothing about how it feels. Reload was checked by key count, and the
reloaded clip wasn't played back in the app.

**A real take in the release app** (RTX 3080 Ti, physics off, W held, Shift, A with it, Shift off, all
off; clipped as a new animation, saved, reopened with its 477 keys):
`target/shots/2026-10-31/live-motion/app-take-wasd.mp4` (left following, right from above),
`app-take/stopped-panel.png`.
- Three takes: a stretch took 50 to 62 ms on average, 80 to 153 ms at most. The last take went over the
  100 ms limit once, with the machine shared with another session's work. 4 to 6 frames held, 2 to 11
  late. All 4 to 6 held frames are at the very start, before the first reply.
- Speed: about 1.2 m/s for 1.3 walking, about 3.5 for 3.6 running. It takes about 2 to 3 s to reach a
  run after Shift, and about 1.5 s to stop from a walk after the keys come up, and longer from a run.
- The largest joint jump is 27 degrees in one frame at about 1.3 s, on one upper arm at a seam while
  standing. It is twist: the wrist moves 1.6 cm that frame. First reply: the wrists move 4.6 cm.

**Found by the real take, fixed:** the take ran 2.4 times too fast on a high refresh monitor (`stable_dt`
is a prediction unless the last frame asked for a repaint; a take now uses the time really passed);
after a take the camera stayed where the run ended, with the body out of view, up to 33 m away; in an
animation with no keys, stopping left the pose where the take ended. The control port gained
`modifiers` input, which holds Shift across requests as a person does.

So far before this: "Measure first".

**Owner's judgement (2026-10-31):** happy with everything in `target/shots/2026-10-31/`, which includes
the video below, so the targets pass and the UI may start. The current set stays. 157000 isn't `DONE_`
yet (training clips and two install checks), but the engine this item uses is approved and built, and
this item's own gate was this judgement.

**To judge:** `target/shots/2026-10-31/live-motion/live-target-test-old-vs-new.mp4` (three-quarter,
following the body, physics off, a caption with what the stick is doing). Right is the current set,
left the first one. Also `live-target-test-side.mp4` (current set, side). The stick: stand, walk from
1 s, run from 4 s, 90 degrees left at 7 s, straight again at 9 s, walk at 11 s, stop at 13 s. Remake:
`python tools/posefill/live_drive.py --out <folder>` with the training environment's Python (it
starts the installed helper). The take is not played live: the script steps as fast as the helper
answers, so this judges the motion and the timings, not how steering feels.

**Speed** (RTX 3080 Ti, release helper, 30 calls in a 16 s take, 481 frames):
| | mean | max |
|---|---|---|
| round trip through the helper (the 100 ms limit) | 34 ms | 96 ms (the first call; every later call about 30 to 46 ms) |
| GPU | 6.9 ms | 56 ms (the first call), about 5 ms after |
| conversion to our rig, for a whole stretch | 11.7 ms (0.36 ms a frame) | 19.5 ms |
| conversion from our rig, the 4 target poses | about 6 ms | |

No call is over the limit, with our rig driven live, so the simple figure isn't needed. The whole take
took 1.04 s of helper time against 16 s of motion. Helper load 1.4 s. The first run of the test,
before the fixes, had a first call of 241 ms: treat a first-call spike as possible and make that
call before the take starts.

**Targets** (`assets/posefill-targets/locomotion.json`, `python -m ac_train.posetargets`): stand and
run from our Kimodo clips, walk and jog from CMU. Choosing by speed alone took a CMU run leaning 27
degrees with hips at 0.765 m, and the character copied it (40 degrees after the turn). Posture now
counts (the gait's median lean and hip height among the candidates at that speed), and the left foot
leads in every moving gait.

Measured on the take (`analyse.py` beside the video), first set then current:
- Trunk tilt, worst second: 30.7 mean / 40.8 peak, then 20.8 mean / 31.0 peak.
- Seams: no jump. The largest joint move a frame relative to the root averages 7.5 cm at the seams
  and 7.4 cm elsewhere (peaks 16.1 and 21.4 cm).
- Lowest foot: about 0 to 2.4 cm above the floor; lowest -0.8 cm.
- Foot slide while planted: 0.3 to 2.7 cm a frame, worst in second 12 (slowing to a walk).
- Speed reached: about 3.1 m/s for 3.6 asked. The target sits one second ahead and the model may take
  longer than that to reach it. Limiting a moving stretch to 8 to 10 tokens (26 to 34 frames) is the
  default in `live_drive.py`: between 2 and 3 s it reached 0.78 m/s against 0.15 with the helper's 8
  to 14 tokens, and it ran a little faster. `VELOCITY_SCALE` and `POSITION_SCALE` are not tuned.

**Response to the stick is slower than the generation time suggests.** The 100 ms limit is met, but
the body answers later (`latency.py` beside the video):
| Stick | Asked at | Body answers at | Delay |
|---|---|---|---|
| start walking (hips over 0.2 m/s) | 1 s | 1.97 s | about 1 s |
| turn left (heading past 45 degrees) | 7 s | 7.43 s | about 0.4 s |
| turn back (heading under 45 degrees) | 9 s | 9.53 s | about 0.5 s |
| stop (hips under 0.2 m/s) | 13 s | 14.20 s | about 1.2 s |

These include the stick smoothing's half-lives (0.2 to 0.6 s), which are not tuned. The slow start
has a further cause to fix in the Rust loop: the first generation after standing fires as soon as the
smoothed speed leaves zero, so it aims at an almost standing target (0.05 m/s), and the next waits
for that stretch's transition point. This needs a real take to judge, because the test isn't steered
by a person.

**Seen in the frames, for the owner to judge:** elbows held out and hands splayed in the walk and run;
a hunched, low stretch for about a second coming out of the turn (9.5 to 10.5 s). Whether those come
from the targets, the model or our conversion is not known yet. There is no style target yet.

Files: `training/ac_train/posetargets.py`, `tools/posefill/live.py`, `live_drive.py`, the
`live_start` and `live_step` ops in `helper.py`, tests in `training/tests/test_posetargets.py` and
`tools/posefill/tests/test_helper.py`. The ops are additions, so the helper's protocol stays 1 and
the Rust side will look for them in `ops`.
