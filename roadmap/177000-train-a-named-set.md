# Train a named set; the card records it

- **Status:** built, waiting for the owner
- **Source:** owner request (2026-11-01)
- **Requires:** 174000, 176000

## What
`ac_train.train --set <name>` reads a set file as `--takes`, reading library clips as a set, and the model card records the set's name, version and hash, giving the what's-in-this-model view. A public set with a purchased or unmarked clip is refused, and such a model can't be exported into the repo.

## Acceptance
Tests without training a real model: `--dry-run` on a set prints its takes and minutes; the card fields are written; the refusals fire. No clip is read that the set doesn't name.

## Result
Built 2026-11-01. This worktree ran nothing, so no test has passed yet; the integration worktree runs them. No model was trained.
- **`ac_train.trainset`.** `train --set <name> [--set-folder] [--library] [--dry-run]`. The dry run lists each clip with minutes, source and labels, then the usual table. The tag defaults to `set-<name>`, so a set can't overwrite `parts`.
- **Card.** `training_set` holds name, version, hash and totals, with no clip or pack name, and library packs stay out of `holdout`. `clips.json` beside it names the clips and is never exported. `python -m ac_train.trainset <tag>` prints both.
- **Refused.** A public set keeping a purchased, unmarked or non-commercial clip; library clips or marked packs inside a checkout; a set that keeps nothing; a kept clip that can't be found; an evaluation-only, wrong-rig or linking clip; exporting into the repo a model of a private set or one that read a purchased clip.
- **Found on the way.** `Clip.from_dict` refused every clip the app writes today, because it expected only the old format name, so `datasets --clips` skipped them. It now accepts any `*.clip`.

**Decided here:**
- The registry's free-licence sets count as `free`, not purchased, or no public set could hold a free take with no sources file. The review app's totals (176000) should count them alike.
- Only kept clips count for the public refusal, since a rejected clip trains nothing.
- A set's labels replace the review's; its cuts add to the scan's and the review's. A rejected take stays out.
- Library clips are held out by pack, and an in-place one is left out and listed.

**Still the owner's:** 164000 says no clip of the collection trains anything until the owner names it and that pack's licence is checked. Nothing here checks a licence: a kept clip counts as named.
