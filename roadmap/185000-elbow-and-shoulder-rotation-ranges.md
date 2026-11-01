# Elbow and shoulder rotation ranges

- **Status:** not started
- **Requires:** none
- **Source:** limb clash analysis (2026-11-01); owner asked for it to be filed

## What
`assets/rig/ranges.json` has hip, knee, ankle, shoulder, trunk and neck, but no elbow and no shoulder
rotation. A route can use those two as freely as it likes without any check noticing: one test route
reached 120 degrees of shoulder rotation, while 99.5% of recorded poses stay under about 97. The app's
range check passed it.

Add both from published studies, and name the sources as the table already does. Recordings can give
the typical band, not the limit.

The rig's rest elbow is already bent 43.1 degrees: `lowerarm01` hinge +43 is straight and negative
means more bend (hinge axis L [.746, .066, .663], R [.746, -.066, -.663]). Express the range as real
bend from straight. In the recordings the hinge runs from about +43 to -99, so bend runs from 0 to
about 142.

Many hand-made poses aren't on that hinge: At ease's `lowerarm01` turn is mostly twist. Measure elbow
bend from where the joints are, not from one bone's rotation.

## Acceptance
- `ranges.json` has elbow bend and shoulder rotation, each with sources and typical and possible
  bands, and `ranges.py` and `crates/ac-core/src/anatomy.rs` agree on them in a test.
- The 50 built-in poses are all listed by level, and any pose reading impossible is shown to the owner
  before the limit is accepted.
- `clip.possible` flags a test clip with an over-rotated shoulder and an over-bent elbow.
