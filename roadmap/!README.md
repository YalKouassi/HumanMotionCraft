# How this folder works

Every piece of work, done or not, is one Markdown file here. There are no subfolders. This file
starts with `!` so it sorts above everything else; it is the only file without a number.

## Why

- **The folder listing is the roadmap.** `ls roadmap` (or any file explorer) shows, in order:
  this file, then what's left in pickup order, then what's done, then what was dropped. No
  separate index to keep in step.
- **Inserting never renames anything.** The numbers are spaced 1000 apart, so new work fits
  between two items by taking a number in the gap. No other file moves, no link breaks, and a
  `git log` of each file stays whole.
- **One file per item** means two people (or agents) adding work at the same time rarely touch the
  same file, so merges don't conflict the way one big `ROADMAP.md` did.
- **Prerequisites by number** survive renames: an item keeps its number when it becomes `DONE_` or
  `OBSOLETE_`, so every `Requires:` still points at it.

### Example: adding work in between

The folder has:

```
122000-engine-check-export.md
123000-project-files.md
```

The engine check shows that engines want a root bone on the floor, and that has to be built before
project files. Take the midpoint, 122500:

```
122000-engine-check-export.md
122500-export-ground-root-bone.md     
123000-project-files.md
```

Later something must go between 122000 and 122500: it takes 122250. Then 122125, 122062, and so
on. Each gap of 1000 takes about ten such splits before it runs out, and only then are the
not-done files re-spaced. With plain 1, 2, 3 numbering, the same insert would rename every file
after it and break every prerequisite that named them.

## Names

```
!README.md                            this file: sorts first
126000-ai-evaluation-suite.md         not done: digits sort before letters, so these come next
DONE_111000-train-on-100style.md      done
OBSOLETE_133000-ai-augmentation.md    dropped: "O" sorts after "D", so these come last
```

- **Six digits, always** (100000–999999), so the names sort correctly as text.
- **Slug:** lower-case kebab-case ASCII (letters, digits, `-`).
- **The number is the pickup order.** Among the not-done files, the lowest number whose
  prerequisites are all done is next.
- **A file keeps its number for life.** Finishing it only adds `DONE_` to the front
  (`git mv 126000-x.md DONE_126000-x.md`); dropping it only adds `OBSOLETE_`.

## Obsolete items

`OBSOLETE_` marks an item that won't be built: overtaken by other work, made pointless by a
decision, or split into new items. It is kept, not deleted, so the reasoning stays findable and
its number is never reused.

- Set the status to `obsolete (YYYY-MM-DD)` and add a `## Why obsolete` section: what replaced
  it, by number, or which decision ended it.
- Only the owner makes an item obsolete (or an agent with the owner's agreement).
- An item that `Requires:` an obsolete one needs its prerequisites rewritten, or becomes obsolete
  too.
- An idea that was rejected before it ever had a file gets no file; it is recorded where it was
  judged (e.g. `DONE_115000-autofill-review-recheck.md`).

## Numbers: sparse integer gaps

- **New item at the end:** the highest number in the folder plus 1000.
- **Between two items:** the midpoint (between 126000 and 127000 → 126500, then 126250 …).
- **Never reuse a number**, including those of `DONE_` and `OBSOLETE_` files.
- **Only when no whole number is left between two neighbours:** re-space the not-done files by
  1000 and update every `Requires:` that names an old number. `DONE_` and `OBSOLETE_` files are
  never renumbered.

## Inside a file

```markdown
# Title

- **Status:** not started | in progress | built, waiting for the owner | open (the owner's decision) | done (date) | obsolete (date)
- **Requires:** none | 126000, 129000
- **Source:** where it came from

## What
## Acceptance
```

- **Prerequisites are named by number**, which survives the `DONE_` and `OBSOLETE_` renames.
- **No acceptance line, no building.** Propose one and wait for the owner to agree.
- **`DONE_` only after the owner confirms.** Until then the status is "built, waiting for the
  owner".

## Where the rules live

- **The `roadmap` skill** (`.claude/skills/roadmap/SKILL.md`): the full rules, the template and
  quick checks. Load it before adding, reordering, finishing or dropping an item.
- **`AGENTS.md`**, sections "Start every session here" and "Roadmap": the repo-wide rule.
- **`ROADMAP.md`** at the repo root: a pointer to this folder.
