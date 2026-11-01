# Character creator app

- **Status:** not started; later (owner, 2026-11-01)
- **Requires:** 189000, 190000, 191000, 192000, 193000
- **Source:** owner's request (2026-11-01): a character creator as a separate app, humans only, using this rig

## What
A separate app that makes human bodies on this rig and saves them as body files (190000). The animation app
stays what `README.md` says: not a character creator. It only uses the bodies.

First version:

- sliders for height, build, muscle and proportions, with a few starting presets and a seeded random button;
- the body turning on the spot, and playing any library clip, so you see it move before saving;
- the clash report (184000) for the chosen clip, and any `body.check` problem, shown as you go;
- save a body file, and open it in the animation app with `body.use`.

Everything is a command, as in the main app, and the code uses `ac_core::product` rather than spelling a name.
The app's own name is the owner's choice.

Humans only: no tails, wings, extra limbs or animal bodies, since the rig has none and this is not planned.

Not in the first version, and each needs its own item: face sculpting, hair, clothes, textures, sharing.

## Acceptance
- From a clean start, a person can make a body, watch it walk and sit, save it, and see it in the animation
  app, in one MP4 under `target/shots/<date>/character-creator/`.
- The same slider values give a byte-identical body file.
- No slider position can save a body that fails `body.check`. A test sweeps every slider, plus 100 seeded mixes.
- Three bodies (tall, short, heavy) export a walk that passes 192000's lines.
- It joins the launcher as its own app and runs from a release build without the animation app open.
- The owner approves the look and the motion by eye.
