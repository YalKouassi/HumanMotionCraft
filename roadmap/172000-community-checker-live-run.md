# Watch the community checker on a live run

- **Status:** not started; needs the owner, who holds `CHECKER_SECRET`
- **Requires:** none
- **Source:** the 2026-10-31 review of 150000; the owner's decision

## What
No live run has been watched. Set `CHECKER_DISPATCH_TOKEN` on the server, upload a test clip and watch it
through. The first run with a clip pending builds cold.

## Acceptance
A good clip is published within one run, an impossible clip fails with a reason naming the frame and joints,
and a run with nothing pending ends in under a minute. The test clips are removed afterwards.
