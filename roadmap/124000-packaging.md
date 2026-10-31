# Packaging

- **Status:** not started
- **Requires:** 114000
- **Source:** old `ROADMAP.md` row 5

## What
A release build: one `.exe` with its assets, ready to download. It must ship `tools/textmotion`
and the training rig-maths modules its setup copies (114000), and `tools/posefill` the same way
(157000: `setup_posefill.py`, `helper.py`, `engine.py`; its setup copies `rig.py`, `quat.py` and
`retarget.py` from `training/ac_train` too).

## Acceptance
The zip runs on a clean Windows machine with no Rust, Python or ffmpeg installed (video export
then explains what is missing).
