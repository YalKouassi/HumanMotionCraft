# Decision: train on free-licence sets only

- **Status:** done (decided 2026-10-28, done 2026-10-29)
- **Requires:** none
- **Source:** old `ROADMAP.md` open decision 2

## Decision
`parts` itself is retrained on the free-licence sets only (CMU, 100STYLE, the Kimodo-generated
set, the CC0 loops; ACCAD and AddBiomechanics are registered since). The repo supplies only free
sets; a non-commercial set is one a user adds and trains on alone (`--noncommercial`, the app's
own models folder). LaFAN1 and AI4Animation were removed from the registry, the docs and the
local data.
