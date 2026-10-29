# HumanMotionCraft



HumanMotionCraft is a free keyframe animator for human characters. You pose a ready-made
mannequin, set keys on a timeline, and export the motion as a rigged `.glb` file to use on your own
characters in a game or film.

It does one thing: humans. Because every animation uses the same calibrated 104-bone rig, the
joint limits, balance, foot contacts, motion library and AI work together with no rigging or setup.

## What you can do with it

- **Pose by dragging.** Pull a hand or a foot and the arm or leg follows (IK), or drag the whole
  body while pinned hands and planted feet stay where they are. Joints can't bend past what a
  real body allows.
- **Animate on a timeline.** Keys with six kinds of interpolation, a curve editor and a dope
  sheet, and layers that override or add to the motion below them.
- **Let physics keep it believable.** Planted feet don't slide, the body balances over its
  feet, and jumps follow a real arc. An overlay shows the support area and the centre of mass.
- **Fill the gaps with AI.** Set a few key poses and AI Fill makes the motion between them, in
  one of 33 styles. It runs inside the app, with no internet connection and no extra install.
- **Make motion from a sentence.** With the optional Motion from text install, type "A person
  waves hello with their right hand." and get an animation, or fill the gap between two of your
  keys.
- **Walk along a path.** Travel builds a walk or run along a path you draw, with the feet held
  to the ground.
- **Start from the library.** 229 motion-capture and generated clips and a set of poses to watch,
  learn from and build on.
- **Export.** Rigged `.glb` with all 104 bones, a 68-bone game set or a 36-bone body set, plus
  MP4 and GIF video and contact sheets for review.

## What it is not

- Not a tool for animals, creatures or custom rigs.
- Not a character creator: there are no body sliders, hair, cloth or weight painting. You animate
  the mannequin and put the motion on your own character.
- Not a 3D scene or rendering tool: no materials, lights or camera sequences, just what you need
  to judge weight, arcs and timing.

## Free, and your work is yours

HumanMotionCraft is free and open source (GPL-3.0-or-later). The animations you make are yours,
including the frames the AI fills and anything built from the library clips: you may sell them or
use them in a commercial game, with no credit needed. The one exception is the mannequin mesh
itself: if you ship it, include its notice. [What you may do with your animations]() explains it
in full.

## For agents and scripts

Everything the app does is a command, the same one whether you click a button, press a shortcut,
run the `ac` command line or ask an AI agent through MCP. [Commands](commands.md) lists them all.

## Where to start

HumanMotionCraft is tested on Windows and is built from source for now; a ready-to-download release is
on the way. [Installing]() gets it running, and [Your first animation]() walks you through a short
clip from first pose to export.
