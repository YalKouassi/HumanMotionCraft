# Community checker: publish or refuse shared clips

- **Status:** done (2026-10-31)
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

## Review (2026-10-31)
Reviewed `.github/workflows/community-check.yml`, `tools/community_check.py`,
`services/community/supabase/functions/checker/index.ts`, `_shared/common.ts` (`retire`) and
`upload/index.ts` (commit 119f430).
- **Fixed (high):** the Result said `clip.possible` was evaluated; it ran as a second `ac run` process
  (`tools/community_check.py:98` then), a fresh session with no clip open, so it measured an empty document
  and no impossible pose could ever fail a clip. `clip.open` and `clip.possible` now run in one
  `ac --jsonl script` on the same document, and the refusal names the frame and the joints (the breach rows
  carry `joint`, never the `name` the script read). Tests
  `test_the_checks_open_the_clip_then_measure_the_same_document`,
  `test_an_impossible_pose_names_the_frame_and_the_joints`.
- **Fixed (high):** a failed post of the *published* verdict (a 5xx, a timeout) fell into the one
  `except Exception` (`:182`) and posted `failed`, which deletes a good clip's files. Problems of the
  checker's own (download, the server's record, posting) now leave the clip pending, are printed and make
  the run exit non-zero; only the app's refusal marks a clip failed. Test
  `test_a_problem_of_the_checkers_own_leaves_the_clip_pending`.
- **Fixed (medium):** the download (`:66`) and `gzip.decompress` (`:168`) were unbounded, and a gzip failure
  fell back to plain text (a silent fallback). Capped at the upload's own 10 MB / 100 MB, must be gzip, and
  a bomb, a cut stream or a mismatch is refused with why. Test
  `test_the_download_is_capped_and_must_be_a_bounded_gzip`.
- **Fixed (medium):** a record with no sha256 skipped the hash check silently (`:161`); now left pending
  and said. The hash is checked before anything parses (`test_the_hash_is_checked_before_anything_parses`).
- **Fixed (medium):** the `ac` subprocesses had no timeout; 300 s now, and a timeout is a refusal that says
  so. The HTTP calls already had 30 s.
- **Fixed (medium, workflow):** no `permissions` block (the default token can write); `contents: read`
  now. The job built `ac` before asking, so a run with nothing pending could never meet the acceptance's
  "under a minute"; `--pending` asks first and the toolchain, cache and build are skipped at 0. The
  15-minute job timeout could not fit a cold Windows release build (`ci.yml` gives one 60); 45 now, 20 on
  the check step. `repository_dispatch: community-check` added: `upload/index.ts` already sends it when
  `CHECKER_DISPATCH_TOKEN` is set on the server, so the wake-up works the day the owner sets that token
  (my call; nothing changes without it).
- **Checked and fine:** the secret travels in a header and is never printed (errors print the endpoint's
  reply, not the request); the endpoint compares it in constant time; `retire` deletes the file and the
  preview and stores `fail_reason`; the reason is capped at 1000 on both sides; `pending` returns at most
  50. The clip reader the checker runs: `clipfile::load_clip` caps keys (100,000 a layer) and frames
  (10,000,000) and demands finite numbers; the file lands under a fixed name in a scratch library, so
  nothing in a clip chooses a path, and `ac` runs no code from a clip.
- **Not proven:** the Result records no live run, so "published within one scheduled run" waits for the
  owner (who holds `CHECKER_SECRET`) to watch one; the first run with a clip pending builds cold.

Accepted by the owner (2026-10-31).

## Decisions on the review (2026-10-31)
The owner: one watched live run, with `CHECKER_DISPATCH_TOKEN` set, in 172000.
