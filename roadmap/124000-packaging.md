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

## Notes
2026-10-31, preparing 0.2.0: the release zip held the app, `assets/` and the licence files only, so
`text.install` and `posefill.install` refused on a download ("the source tree was not found").
`.github/workflows/release.yml` now also ships both setups, the helper scripts, the rig maths they copy
and `crates/ac-core/src/product.rs`, which the setups read the product's name from, in the source tree's
layout. The clean-machine check of this item's acceptance is still to do.
