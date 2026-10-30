# A review that can be trusted

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 116200
- **Source:** 115100 (measured)

## What
`clip.critique` isn't ready to put in front of people (`crates/ac-core/src/critique.rs`):
- **It flags nearly everything:** 51 of 55 built-ins get a medium or high finding, 26 of them high.
- **Limits disagree with `clip.possible`:** 25 clips have medium or high limit findings while
  `clip.possible` reads them typical or unusual. A review should use the three levels of 120000.
- **Pops are noisy:** 655 findings, led by eyes and fingers, whose speed comes from the body above
  them. Measure pops on what a person would see as one.
- **Its fixes aren't checked:** see 116200.

Each finding also needs to say why it happens as well as what. Each fix is measured on a copy
(`edit.dry_run`) and offered only if it improves its own finding without making another worse.

## Acceptance
On the built-ins: the owner agrees a clean list (at least `Locomotion/Walk` once 115600 is done, and
the recorded walks) that gets no medium or high finding; no limit finding contradicts `clip.possible`;
a test runs every suggested batch on every built-in and checks each improves its finding and worsens
none; `fixtures/clip-critique.json` is updated by hand-worked numbers.
