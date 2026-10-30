# Motion from text

- **Status:** done
- **Requires:** none
- **Source:** old `ROADMAP.md` row 8

## What
An optional helper (`tools/textmotion/setup_textmotion.py`, Python, installed outside the repo;
owner's decision 2026-10-28) makes motion from a prompt: `text.generate` writes a new stretch from
the playhead, `text.fill` fills between two of your keys held to both, `text.helper` loads or
stops it. The keys are AI keys tagged with the prompt, which edits next to them leave as they are
and say so. The window makes the motion on a thread, and the app runs as before without the
install.

## Acceptance
With the install, "A person waves hello with their right hand." fills a 2-second gap between two
standing keys and generates 3 seconds from the playhead, and the contact sheets show the wave
meeting both keys; without it, the panel and every `text.*` command say how to install it;
`fixtures/text-motion.json` and `crates/ac-textmotion/tests` pass.
