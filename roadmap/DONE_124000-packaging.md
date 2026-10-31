# Packaging

- **Status:** done (2026-10-31)
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

## Result
Built 2026-10-31: a scripted, repeatable check of the zip, and two packaging bugs it found by reading
the workflow against the setups, fixed in `release.yml`.

**The check: `tools/release_check.ps1`.** Takes the zip (or a folder with one zip, or an unzipped
release) and a scratch folder, unzips, and runs the app and `ac` from the release with a minimal
environment: PATH is Windows' own folders only, `AC_FFMPEG` unset, home, `APPDATA`, `LOCALAPPDATA` and
`TEMP` under the scratch, and `AC_TEXTMOTION_DIR` / `AC_POSEFILL_DIR` pointed at empty scratch folders.
Each check prints PASS or FAIL with its evidence, the log goes to `<scratch>\release_check.log`, and it
exits 1 on any FAIL. It runs on Windows PowerShell 5.1 (what Windows Sandbox has) and PowerShell 7.

| # | Check | What proves it |
|---|---|---|
| 1 | contents | `tools/release_contents.ps1`: every path in `tools/release_manifest.txt` is in the zip, one top folder, `/` separators, nothing forbidden (`*.bvh`, `*.npz`, `*.fbx`, `training/data/*`, `assets/motions/*`) |
| 2 | version | `humanmotioncraft.exe`'s ProductVersion equals the version in the zip's name |
| 3 | clean path | `where.exe ffmpeg python py uv cargo rustc` under the minimal PATH finds nothing |
| 4 | app | `humanmotioncraft.exe --library <scratch> --script <clip.open Locomotion/Walk, frame.set 10> --screenshot <png> --size 1280x800` exits 0 and writes a PNG |
| 5 | ac script | `ac.exe --jsonl script`: `session.info`, `clip.open`, `frame.set`, `bone.world`, `clip.picture` (PNG), `clip.video` to GIF: 6 JSON results, the files have PNG and GIF headers |
| 6 | no ffmpeg | `clip.video` to `.mp4` exits non-zero with a message naming ffmpeg, no crash markers, no file |
| 7 | text.install | `run text.install {"action":"check"}`: state `not installed`, requirement `Source tree` ok (downloads nothing) |
| 8 | posefill.install | the same for pose fill |

`text.install` and `posefill.install` already had the download-free mode (`action: check`, the default,
which reports `requirements[]`), so no command changed.

**The contents check without a machine: `tools/release_contents.ps1` and `tools/release_manifest.txt`.**
The manifest lists what the "release folder" step of `release.yml` stages (files, folders that must be
non-empty, and forbidden patterns); the script compares it with the zip's entry list (or a folder).
`release.yml` runs it right after making the zip (a four-line `pwsh` step), so a staged file missing from
the manifest's list, or a listed file missing from the zip, fails the release.

**Bugs found and fixed (in `release.yml`, 2026-10-31).** Found by reading the setups against the staging
step; neither needed a machine:
- `setup_posefill.py`'s `RIG_MODULES` copies `bvh.py` too, which the zip did not hold, so
  `posefill.install` on a download would have crashed at step 4 (a `FileNotFoundError`). Now shipped.
- `posefill.samples` (165000) runs `tools/posefill/setup_samples.py`, which imports `setup_posefill` and
  runs `convert_samples.py`; neither was shipped, so the sample-clip download would have refused with
  "the source tree was not found". Both now shipped.
A zip staged without them fails check 1 (tried: `2 of 28 missing: tools/posefill/setup_samples.py,
training/ac_train/bvh.py`).

**Run on this machine, 2026-10-31**, on a zip staged exactly as `release.yml` does (with the two fixes)
around the exes built today: 8 passed, 0 failed, under both PowerShell 7.6 and Windows PowerShell 5.1.
The app started in 1.7 s and wrote a 551 KB screenshot showing Locomotion/Walk at frame 10; `ac` ran
the six commands in 1.3 s (session.info: 0.2.0, rig `b09a19200730edb6`, model `parts`, 164 commands;
Walk 33 frames; GIF 11 KB); the MP4 refusal was `clip.video: MP4 needs ffmpeg, which could not be
started (...): install it, or set AC_FFMPEG to its path; GIF needs nothing`, exit 2, no file; both
installs reported the shipped `setup_*.py` as the source tree with nothing installed.

**What this machine can't prove.** The app also looks for ffmpeg at `C:\tools\ffmpeg\bin\ffmpeg.exe`
(`ac_render::anim::ffmpeg`, the development fallback), which exists here, so check 6 pointed
`AC_FFMPEG` at a missing file for that one run and says so in its evidence. `nvidia-smi` lives in
`System32` on a machine with an NVIDIA driver, so the GPU requirement showed found. Both are what a
clean machine would not have; Windows Sandbox is the real test.

### How the owner runs it on a clean machine
1. Build the zip: a manual run of the release workflow (Actions, release, Run workflow) uploads
   `release-v<version>` as an artifact; download it and put the one zip in `dist\` of the checkout.
2. Windows Sandbox (turn on "Windows Sandbox" in Windows features once): `mkdir target\release-check`,
   then double-click `tools\release_check.wsb`. It maps `dist\` and `tools\` read-only, has no network
   and nothing installed, runs the check at logon and leaves the window open with the PASS and FAIL
   lines; the log and the pictures land in `target\release-check` on the host. If the checkout is not
   `D:\CreativeFusion\AnimationCraft`, edit the three `HostFolder` lines first.
3. On any other machine: `powershell -ExecutionPolicy Bypass -File tools\release_check.ps1 -Zip <zip>
   -Scratch <folder>` (PowerShell 7 or 5.1). Exit 0 and `8 passed, 0 failed` is the acceptance.

The item stays "in progress" until the owner has seen the eight PASS lines in the Sandbox or on a real
clean machine. A FAIL there is a packaging bug for this item.

**Decisions made without asking.** The check is PowerShell, not a command: it tests the shipped exes
from outside, as a person would. The manifest duplicates the staging list by hand (the check catches a
listed file missing from the zip; the comment on both says to change them together). The development
fallback path for ffmpeg stays in the app. No `.wsb` for the integration worktree: the config names the
owner's checkout.

## Closed (2026-10-31)
Closed by the owner. The scripted zip check passes; the owner's run in Windows Sandbox or on a clean machine was not made.
