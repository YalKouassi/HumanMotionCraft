# Community checker: publish or refuse shared clips

- **Status:** built, waiting for the owner (2026-10-30)
- **Requires:** none
- **Source:** owner request, 2026-10-30 (no manual approval; `CHECKER_SECRET` is set on GitHub;
  no instant wake-up, so the schedule alone)

## What
Shared clips stay `checking` until something runs the app's own checks on them. A GitHub workflow
`.github/workflows/community-check.yml`:
- runs every 15 minutes and on demand;
- builds `ac` (cached between runs);
- asks the `checker` function for pending clips;
- for each one, downloads it, checks the hash, and runs `clip.open` (the format) and
  `clip.possible` (no impossible poses);
- sends `result`: published, or failed with the reason the app gave.

## Acceptance
- A valid clip shared to the live server goes `published` within one scheduled run.
- A clip with an impossible pose, and a file that isn't a clip, go `failed` with the reason, and
  their files are deleted.
- A run with nothing pending exits in under a minute after the build cache.

## Result
2026-10-30. Workflow `.github/workflows/community-check.yml` and engine runner `tools/community_check.py`
added. Checks pending clips via the `/functions/v1/checker` endpoint with `CHECKER_SECRET`, validates
downloaded file integrity against sha256, verifies loading through `ac run clip.open`, and evaluates
joint limits through `ac run clip.possible` before marking `published` or `failed`.
