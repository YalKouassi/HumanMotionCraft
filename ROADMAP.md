# HumanMotionCraft roadmap

The roadmap is the `roadmap/` folder: one Markdown file per piece of work, done or not. Rules for
agents live in `AGENTS.md`; licences and sources in `ATTRIBUTION.md` and `NOTICE`.

```
roadmap/126000-ai-evaluation-suite.md        not done: listed first, lowest number is next
roadmap/DONE_111000-train-on-100style.md     done (only after the owner confirms)
```

- Six-digit numbers, spaced by 1000; an item between two others takes the midpoint (126500).
- A file keeps its number for life; finishing it adds the `DONE_` prefix, dropping it adds
  `OBSOLETE_` (kept, so the reasoning stays findable; these sort last).
- Each file names its prerequisites by number (`Requires:`), and has an acceptance line before it
  is built.
- No subfolders. `roadmap/!README.md` sorts to the top and explains the convention.

The full rules are in the `roadmap` skill (`.claude/skills/roadmap/SKILL.md`).
