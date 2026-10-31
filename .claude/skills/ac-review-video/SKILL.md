---
name: ac-review-video
description: Make MP4 videos for the owner to review motion work in HumanMotionCraft. Use whenever work is ready for the owner to look at, an item goes to "built, waiting for the owner", or the owner asks to see or review an animation, physics, AI fill or foot change. MP4s replace contact sheets and stills for anything that moves.
---

# Videos for the owner to review

The owner reviews motion by watching it. For anything that moves, hand over MP4s in
`target/shots/<date>/<topic>/`, not contact sheets. Stills stay fine for UI that doesn't move (a
panel, a dialog), and for your own checks.

## Where the file goes
Always under `target/shots/` in the **main checkout**, by absolute path, even when you work in a
worktree. Removing a worktree deletes its `target/`, and the proof with it.

## How to make one
No new code is needed. Pick by what you have:

| You have | Use |
|---|---|
| A script of commands, headless | `ac video` (below) |
| An MCP session | `animation_preview { "mp4": "<absolute path>", "gif": false }`, or `command_run clip.video { "path": ... }` |
| The running app | the same tools through `ac mcp --live <port>`: the window's own animation |
| The whole window, panels and all | `humanmotioncraft --open <id> --record <file.mp4>` (`--size`, `--from`, `--to`, `--fps`; `--ui library=off` and the like hide parts of it). It steps through the frames, then closes |

```
ac --assets assets video --script <script.json> --view three-quarter --size 1280x720 --out <file.mp4>
```

- `--script`: a JSON list of `{"command", "params"}`. Open a built-in with
  `[{"command": "clip.open", "params": {"id": "Locomotion/Walk 12"}}]`. To use a fixture, copy its
  `script` and leave out the `expect`s and the queries.
- `--view`: `front`, `back`, `left`, `right`, `side`, `top` or `three-quarter`. Choose the view that
  shows the change: `side` for a foot roll or a landing, `front` for weight going side to side.
- `--fixed`: one camera for the whole shot, so travel and height show. Without it the camera follows
  the body, which hides a root jump. Use it for anything about the root, hips or a jump.
- `--from`, `--to`, `--fps`: part of the animation, and the frame rate.
- Physics plays as the clip has it. Say which it is, and set it with `physics.set` in the script.

ffmpeg comes from `AC_FFMPEG`, the PATH or `C:/tools/ffmpeg/bin/ffmpeg.exe`. If it is missing the
command says so; don't fall back to a GIF without saying.

## Before and after
Make one video of each, then put them side by side, labelled, in one file, so the owner compares
them without switching windows:

```
ffmpeg -y -i before.mp4 -i after.mp4 -filter_complex "[0]drawtext=fontfile='C\:/Windows/Fonts/arial.ttf':text='BEFORE':x=20:y=20:fontsize=36:fontcolor=white[a];[1]drawtext=fontfile='C\:/Windows/Fonts/arial.ttf':text='AFTER':x=20:y=20:fontsize=36:fontcolor=white[b];[a][b]hstack" -c:v libx264 -pix_fmt yuv420p side-by-side.mp4
```

Both must use the same `--view`, `--size`, `--fixed` and frames. `--fixed` frames each video on its
own body, so if the change moves the body the two cameras can differ a little: judge positions against
the floor lines, not the edge of the picture. Delete the two separate files unless they are useful on
their own.

**Labels say what was really rendered.** `BEFORE` and `AFTER` mean the same build with the command
not run and run. Physics off against physics on is `KEYS` and `PHYSICS ON`. Old code against new means
actually building the old code.

For a change that lasts a few frames (a landing, a foot rolling), add a slowed copy: put
`setpts=4*PTS` in the filter chain, and say in the file name that it is slowed (`landing-quarter-speed.mp4`).
For something small, like feet, render at `--size 2560x1440` and add `crop=1280:720:<x>:<y>` before
the label.

## Before you hand it over
1. **Watch it yourself.** Grab frames at the moments that matter and read them:
   `ffmpeg -y -ss <seconds> -i <file.mp4> -frames:v 1 <frame.png>`. Take them where the fixture says
   the change is, not at even intervals: the difference can be only at the ends of the shot.
2. **Check it shows the change.** If before and after look the same at this size, try another view,
   `--size` or a slower copy. If they still look the same, say so: don't offer a video as proof of
   something it can't show. Look at everything in the frame, not only the part you changed; if
   something else moved, measure it with `pose.query` and say so.
3. **Tell the owner** for each video: its path, what to look at, at what time in the video, and what
   good looks like. Say anything it doesn't prove.
4. **Record it** in the item's file: the paths, under its `## Result` or `## Built`.
