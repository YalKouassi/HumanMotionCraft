# Training pipeline

- **Status:** done (built by 2026-10-29)
- **Requires:** none

## What works
`training/`: dataset registry and fetch, retargeting, quality scan, the `ac-review` app, training,
weight export, library clip cutting, build stamps. The ACCAD (CC BY 3.0) and AddBiomechanics
(CC BY 4.0) sets are registered, retargeted and scanned. The review rejects 353 duplicate CMU
takes and cuts or rejects takes with impossible poses. `parts` is not retrained on the new sets
yet (134000).
