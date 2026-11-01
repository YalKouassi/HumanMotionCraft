# Character creator app (in its own repo)

- **Status:** not started; later (owner, 2026-11-01)
- **Requires:** 189000, 190000, 191000, 192000, 193000
- **Source:** owner's request (2026-11-01): a character creator as a separate app, humans only, using this rig

## What
A separate app, in its own repo (189000), that makes human bodies on this rig and saves them as body files
(190000). This file only tracks it from here. Once the repo exists, the work moves to that repo's own roadmap
and this file points to it. The animation app stays what `README.md` says: not a character creator. It only
uses the bodies.

What it takes from this repo is the contract: `ac-body` by git revision and `docs/body-file.md`. Linking
`ac-body` means the app's licence must be compatible with GPL-3.0-or-later. Its name, repo and licence are the
owner's choice when the work starts. It has its own product name, credits and rules.

First version:

- sliders for height, build, muscle and proportions at adult sizes, with a few starting presets and a seeded
  random button;
- no genitals or similar detail;
- the body turning on the spot, and playing a few clips, so you see it move before saving. The clips it ships
  carry their own licences and credits in its repo;
- any `body.check` problem shown as you go;
- save a local body file, which the animation app opens with `body.use` (191000).

Humans only: no tails, wings, extra limbs or animal bodies, since the rig has none and this is not planned.

Not in the first version, and each needs its own item: small or child bodies, sharing, a clash report inside the
creator, face sculpting, hair, clothes, textures. The clash report for a body is in the animation app (192000).

## Acceptance
- From a clean start, a person can make a body, watch it walk and sit, save it, and see it in the animation
  app, in one MP4.
- The same slider values give a byte-identical body file.
- No slider position can save a body that fails `body.check`. A test sweeps every slider, plus 100 seeded mixes.
- Three bodies (tall, short, heavy) export a walk from the animation app that passes 192000's lines.
- It joins the launcher as its own app and runs from a release build without the animation app open.
- The owner approves the look and the motion by eye.
