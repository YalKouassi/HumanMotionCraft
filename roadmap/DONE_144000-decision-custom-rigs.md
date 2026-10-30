# Decision: one rig inside the app, retarget on export

- **Status:** done (decided by the owner, 2026-10-29)
- **Requires:** none
- **Source:** old `ROADMAP.md` open decision 1 ("custom rigs: retargeting onto your own humanoid
  skeleton, or only the built-in rig")

## Decision
The app always animates the built-in 104-bone rig. A person's own humanoid character is reached
by retargeting **at export** (and, later, at import), never by editing on their skeleton.

## Why
- The rig already covers a humanoid body in full: a 5-bone spine, 3 neck bones, head and eyes,
  twist bones in the arms and legs, fingers with metacarpals, and five toes a foot. Most character
  rigs are a subset of it, so a retarget mostly drops or merges bones. It has no face, prop or
  helper bones.
- The AI model (19 solved bones, hip height), the physics pass, the joint ranges and
  impossible-pose check (`assets/rig/ranges.json`), IK limits, the library, the training
  retargets and the parity goldens are all built and checked on this one skeleton. A rig inside
  the app per character would mean proving all of them again for each one.
- The retarget becomes one pure, testable step at the edge, reached through commands like
  everything else.

## Costs and how they are handled
| Cost | Handling |
|---|---|
| Other proportions break contacts (feet slide or sink, hands miss) | Scale the root path by the hip-height ratio, leg IK on planted frames, optional hand targets (146000) |
| You don't see your own character while animating | A live preview of the character driven through the retarget (147000) |
| Rest poses and bone axes differ (A-pose, T-pose, bone roll) | A rest-pose alignment per character, saved with its bone mapping (146000) |
| Bones we don't have (face, tail, props) | Left at rest, and the export result says so (Rule 1) |
| Engines often want a root bone on the floor; ours is the hips | An export option that adds one (122500) |

## The workflow
1. Animate on the built-in rig, as now.
2. Once per character: import its skeleton (.glb); the bone mapping is guessed from the names,
   you correct it, the rest poses are aligned; the mapping is saved.
3. Preview the character playing the clip, with problem frames marked.
4. Export (`clip.export` / `clip.export_batch`) with the mapping as the target, plus the root and
   foot-lock options.

## Later, optional
The built-in rig could take on a character's bone lengths while you animate, so contacts are right
before export. The AI reads rotations and a root position in hip heights, so it mostly copes, but
this is a bigger change and is not planned yet.
