---
name: roadmap
description: Use whenever you read, add, reorder, finish, drop or rename roadmap work in HumanMotionCraft (the `roadmap/` folder; one Markdown file per item, named `NNNNNN-slug.md`, `DONE_NNNNNN-slug.md` once done, or `OBSOLETE_NNNNNN-slug.md` once dropped). Gives the naming rules, the sparse numbering, prerequisites, the file template and how to mark an item done or obsolete.
---

# The roadmap folder

Every piece of work, done or not, is one Markdown file in `roadmap/`. There are no subfolders. The
one other file is `roadmap/!README.md`: the convention in short, sorted to the top by its `!`. Keep
it in step with this skill when a rule changes; its "Why" section explains the scheme with an
example. `ROADMAP.md` at the repo root only points here.

## Names

```
roadmap/NNNNNN-short-slug.md         not done (sorts first: digits come before "D")
roadmap/DONE_NNNNNN-short-slug.md    done
roadmap/OBSOLETE_NNNNNN-short-slug.md  dropped (sorts last: "O" comes after "D")
```

- `NNNNNN` is **exactly six digits** (100000–999999), so names sort correctly as text.
- The slug is lower-case kebab-case ASCII: letters, digits and `-` only. No spaces, no `_` in the
  slug.
- A file **keeps its number for life**. Finishing it only adds the `DONE_` prefix
  (`git mv roadmap/126000-ai-evaluation-suite.md roadmap/DONE_126000-ai-evaluation-suite.md`);
  dropping it only adds `OBSOLETE_`.
- The number is the **pickup order**: among not-done files, the lowest number whose prerequisites
  are all done is next.

## Numbers: sparse integer gaps

- A new item at the end gets the highest number in the folder (done or not) plus **1000**.
- An item between two others gets the **midpoint**: between 126000 and 127000 → 126500, then
  126250, and so on. Round down to a whole number.
- Never reuse a number, including those of `DONE_` and `OBSOLETE_` files.
- Only when no whole number is left between two neighbours (e.g. 126001 and 126002), re-space:
  renumber the not-done files in order by steps of 1000 from just after the last done number,
  `git mv` each one, and update every reference to an old number (`rg -n "\b<old>\b" roadmap`).
  `DONE_` and `OBSOLETE_` files are never renumbered. Re-spacing is its own commit.

## Prerequisites

Each file says what must be done first, **by number** (numbers survive the `DONE_` and `OBSOLETE_` renames; slugs
may be added after the number for readability):

```
- **Requires:** 126000, 129000
```

`none` when there is nothing. A prerequisite must exist in `roadmap/`. Don't start an item whose
prerequisites aren't all `DONE_`, unless the owner says so.

## Template

```markdown
# Title

- **Status:** not started | in progress | built, waiting for the owner | owner's decision first | open (the owner's decision) | done (date) | obsolete (date)
- **Requires:** none | NNNNNN, NNNNNN
- **Source:** where it came from (a review, an owner request, an old roadmap row)

## What
What to build and why, with `path:line` evidence where it helps.

## Acceptance
How we know it's done: commands, fixtures, numbers, screenshots.
```

Decisions use `## Question` (open) or `## Decision` (made) in place of What.

## Rules

- **An item with no acceptance line can't be built.** Propose one in the file and wait for the
  owner to agree.
- **Built, then confirmed.** When the work is built and its images are ready, set the status to
  `built, waiting for the owner`. Rename to `DONE_` **only after the owner confirms**, and set the
  status to `done (YYYY-MM-DD)` with the result (numbers, screenshots) under `## Result`.
- **Obsolete, not deleted.** An item that won't be built (overtaken, ended by a decision, split
  into new items) is renamed `OBSOLETE_`, its status set to `obsolete (YYYY-MM-DD)`, with a
  `## Why obsolete` section naming what replaced it (by number) or the decision that ended it.
  Only the owner makes an item obsolete (or an agent with the owner's agreement). Items that
  `Requires:` it get their prerequisites rewritten, or become obsolete too.
- One item per file. Split a big item into numbered files with prerequisites; don't nest.
- A finding that changes an item (new evidence, a correction) is written into that item's file,
  with the date, not left in chat.
- Ideas rejected before they ever had a file are recorded where they were judged (e.g. `DONE_115000-autofill-review-recheck.md`)
  and get no file of their own.
- Commit roadmap changes with the work they describe, or on their own when only the plan changed.

## Quick checks

```
ls roadmap                                  # not-done first, in pickup order
ls roadmap | rg '^[0-9]'                    # only what's left
rg -n "Requires:.*\b126000\b" roadmap       # what waits on 126000
ls roadmap | rg -v '^(!README\.md|(DONE_|OBSOLETE_)?[0-9]{6}-[a-z0-9]+(-[a-z0-9]+)*\.md)$'   # names breaking the rule (should print nothing)
```
