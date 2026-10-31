# Optional sample clips, marked evaluation-only

- **Status:** built, waiting for the owner (2026-10-31)
- **Requires:** none (157000's engine is built and approved)
- **Source:** owner, 2026-10-31: choosing "Optional user download" when asked how to use MotionBricks' sample clips

## What
The engine's 38 human sample clips (`assets/motions/soma/`, NVIDIA Sample Data Evaluation License) as
an off-by-default download in the Pose fill panel, from NVIDIA to the person's machine, behind a
notice and their own acceptance. Anything touched by the clips is marked evaluation-only and blocked
from export and community upload.

This changes 157000's "never read" for the person's own download only. For us nothing changes: the
clips are never in the repo or release, never trained on, and never used for anything we keep or ship.

The licence (read 2026-10-31, `assets/motions/LICENSE.txt`, v. Jan. 16, 2026): §1 internal use, solely
to evaluate and test NVIDIA's technologies; §2(c) no making them or derivative works available to
others; §2(b) keep the notices; §6 on termination destroy all copies and derivatives; only an adult
of legal age can accept. The clips derive from Bones' RP1 data. A notice can't widen any of this: the
mark and the blocks are how the app keeps to it, and the licence stays the person's to keep.

## Acceptance
Chosen by the owner 2026-10-31 ("Optional user download"):

- `posefill.samples` (check, start, stop, add, remove). `start` refuses without `accept`, whose
  description tells agents it is the person's own answer. The panel's Download is off until they tick
  what accepting says.
- The download is only the 38 SOMA BVH files, the licence and the README, at the pinned commit,
  checked against pinned hashes, into `samples/` beside the install. The main install's check that it
  holds no clips stays as it was.
- A clip made from them has `evaluation_only` in its file. The mark follows the motion through a saved
  pose, a copied pose, copied keys, a new animation started from it and a link layer, and saving under
  a new name keeps it.
- With the mark, these refuse and say why: `clip.export`, `clip.export_batch`, `clip.write`,
  `community.share`, `clip.video`, `clip.picture`, `clip.sheet`, `ac video` and `--record`.
- The anchor library, travel's walk clips and training skip marked clips.
- `remove` deletes the download and the sample clips, and lists what was made from them (`derived: true`
  deletes that too).
- No sample clip is in the repo. Tests and fixtures use made-up data.
- The owner approves the panel from a screenshot, and tries the download.

## Result
Built 2026-10-31. Number 165000, because 164000 is the owner's FBX library.

- **Tests.** `crates/ac-posefill/tests/samples.rs` (10), `tools/posefill/tests/test_samples.py` (6),
  `fixtures/sample-clips.json`.
- **Offline run of the setup.** On the 38 clips from the local reference copy, in
  `target/test-scratch` with a test acceptance file: every hash matched, all 38 converted, 64 MB on the
  rig at 30 fps. In the app all 38 went in the library, the status bar and library row said so, and
  export, video, picture, file write and community upload each refused and wrote nothing. Remove
  deleted the 38 and the folder and listed the pose saved from one. That folder is deleted. I looked at
  one clip (a skip) in the window: it stands on the floor and moves as it should; the other 37 weren't
  looked at. `target/shots/2026-10-31/sample-clips/clip-open.png` and `panel-installed.png` show a
  frame of it, so they are evaluation output: delete them after looking, and don't post them.
- **Not run: the real download.** It needs the person's acceptance, so it is left to the owner. Both
  URL forms answer (the licence, and one clip by its large-file URL, with the pinned size), but the
  download path itself has only been run with every file already present.
- **Found by the owner's first real try (2026-10-31):** the download stopped at `LICENSE.txt` with a hash
  mismatch. Its pin and README's came from the Windows checkout, which has CRLF line endings; GitHub
  serves LF. The text is the same. Both are now pinned from the upstream commit itself, and all 38 clip
  pins match that commit's large-file pointers.
- **Limits.** The mark is honest marking, not protection: window screenshots, inline previews to an
  agent, `clip.sample`-style queries, `project.save` and editing the JSON by hand aren't blocked. A
  project keeps the mark. 38 clips add 64 MB of JSON the library reads when it lists items; how much slower that makes
  listing wasn't measured.
- **Left out:** a Live motion target set made from these clips. The owner's choice didn't ask for it.

Review: `target/shots/2026-10-31/sample-clips/panel-before-download.png`, `panel-accepted.png` and
`panel-installed.png`.
