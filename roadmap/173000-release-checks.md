# Real-world checks before the next release

- **Status:** not started; needs the owner's machine and hands
- **Requires:** none
- **Source:** what closing 124000, 157000, 158000 and 165000 left unrun (2026-10-31)

## What
Paths people hit that have never been run for real: the zip on a clean machine, the pose fill install, the
sample download and a controller take.

## Acceptance
- `tools/release_check.wsb` passes in Windows Sandbox on the release zip.
- A real `posefill.install` from the panel with Stop and resume, and no `assets/motions/` in the folder afterwards.
- A real `posefill.samples` download once its licence is accepted.
- A controller take in live motion (158000).
