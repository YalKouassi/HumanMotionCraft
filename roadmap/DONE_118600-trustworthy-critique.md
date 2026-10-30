# A review that can be trusted

- **Status:** done (2026-10-30)
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

## Built (2026-10-30)
`crates/ac-cli/tests/critique_builtins.rs` runs the acceptance over all 55 built-ins, headless. Nobody
has judged the remaining findings by eye.

| Worst finding per clip | Before | Now |
|---|---|---|
| high | 26 | 6 |
| medium | 25 | 18 |
| low | 1 | 26 |
| none | 3 | 5 |

- **Limits:** now `clip.possible`'s own level: unusual is low, impossible is high, typical is nothing.
- **Pops:** measured on body joints only, as the change of speed at a key beyond the frames near it.
- **Fixes:** each runs on a copy first (20 at most), and is dropped, with a note in the message, if it
  doesn't lessen its own finding or worsens another kind. 14 are offered across the built-ins; the
  test runs each for real.
- **Clean list, agreed by the owner:** `Locomotion/Walk` and the recorded `Locomotion/Walk 12` to `17`.
- **`Locomotion/Walk`:** its medium was a false alarm. A foot on the floor slid back at about 0.9 m/s
  under a body going nowhere, so no foot counted as planted and the gap was measured against the
  wrong foot. A body that does this for three quarters of a second now has no gap (`weight.rs`;
  by-hand test in `crates/ac-core/tests/weight.rs`). Only this clip's rows changed in the ratchet.
- **Lifting the feet:** in the fixture's clip it stretches the slide from 12 to 13.15 cm, so that
  finding now offers no fix.
- **Still flagged:** high on 6 clips: a pop on `Action/Jump`, and sliding on `Dance/Dance 8`,
  `Locomotion/Side step 13`, `Locomotion/Turn in place 5`, `Martial arts/Fight 7` and
  `On the floor/On the ground 10`. A turn pivots on its foot, so some of these may be false alarms
  the owner should judge. Capture stays medium on `Action/Jump`, `Locomotion/Side step 9`,
  `Locomotion/Walk in a circle 5` and `Sitting/Sit down and stand up 5`, not investigated.
