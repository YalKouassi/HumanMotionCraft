# Steer it live with NVIDIA's sample poses

- **Status:** built, waiting for the owner
- **Requires:** 158000, 165000
- **Source:** owner request (2026-11-01): the demo looks better; offer its poses behind a disclaimer, and don't block exporting

## What
A "Use NVIDIA's sample poses" checkbox under Steer it live, off by default and offered only once the
person has downloaded the sample clips. `live.start {targets: "samples"}` aims at a target set made on
their machine from their download, kept in the download's folder and deleted with it. The licence
terms and link show beside the checkbox. What is kept is marked evaluation-only.

**Owner's decision (2026-11-01), changing 165000:** export isn't blocked: "the user can do what they
please. they might export for evaluation reasons". `clip.export`, `clip.export_batch`, `clip.write`,
`clip.video`, `clip.picture`, `clip.sheet`, `ac video` and `--record` now work on marked motion.
`clip.export` adds a note that the licence still applies and the file doesn't carry the mark.

**Kept:** the mark itself, community upload refused (it would make the motion available to others from
our server, which §2(c) forbids), training skipping marked clips, and nothing from the clips in the
repo or release.

## Acceptance
- With the download, a take aims at poses from its plain clips only, and a kept range marks the animation.
- Without the download, the option isn't offered and `live.start` says how to get it.
- Export and video work on marked motion, with the note; community upload still refuses.
- `fixtures/live-sample-poses.json`, `fixtures/sample-clips.json`, `cargo xtask check`.
- The owner compares `target/shots/2026-11-01/sample-poses/ours-vs-sample.mp4` and approves.

## Result
Built 2026-11-01. Run on the RTX 3080 Ti through the app's commands, with the same scripted stick both
ways, 376 frames: about 30 ms a stretch either way (at most 41 and 44 ms), no frames held or late.

Chosen: stand `idle_loop_001__A268`, walk `Neutral_walk_forward_002__A057` (1.34 m/s), run
`run_loop_180_R_003__A326` (3.92 m/s). No jog qualified, so 2 m/s blends walk and run. The first run
chose a neck warm-up for stand, so crouch, warm-up and legs-relax clips are left out.

**Not the real download:** the owner's isn't on this machine now, and accepting the licence is theirs. The
run used 10 clips converted from the local reference copy, since deleted. So the download-to-checkbox
path hasn't been run end to end. **Not tried:** the checkbox clicked with real keys or a controller. Also
no style targets (zombie and so on) are offered. The rig conversion is unchanged, so its losses still apply.
