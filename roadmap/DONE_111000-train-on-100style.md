# Train on 100STYLE

- **Status:** done (owner accepted after testing, 2026-10-29)
- **Requires:** 110000
- **Source:** old `ROADMAP.md` row 2

## What
Cut library clips from 100STYLE and retrain `parts` with it (CC BY 4.0, so its clips can ship).

## Acceptance
The new model is no worse on held-out takes at 15, 30 and 45 frames; the card in `weights.json`
lists the set; the `ac-ai` goldens are regenerated.

## Result (2026-10-29)
Retrained on the free-licence sets only (112000). Mean joint error at 15 / 30 / 45 frames on the
free sets' held-out takes: 1.17 / 2.18 / 3.33 cm (the previous model 1.57 / 3.06 / 4.90; plain
interpolation 3.73 / 7.79 / 11.51). The card lists 100STYLE; the goldens are regenerated.
