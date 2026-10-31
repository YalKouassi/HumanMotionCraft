# Port the owner's FBX animations into a private clip library

- **Status:** not started; the acceptance below is proposed and waits for the owner to agree
- **Requires:** 152000
- **Source:** owner request (2026-10-31)

## What
Bring the owner's whole collection of humanoid FBX animations onto the rig as `.clip.json`: 6,489
files from 27 purchased packs, listed in an inventory the owner keeps outside the repo. Each clip is
oriented, retargeted and checked on our rig. 152000 built the importer for one file; this item is
the bulk port and whatever real files show that importer still lacks.

**Private.** The clips and the manifest live outside the repo. They are never published to the
community library and never bundled in a release. Generic importer code may go in the repo; pack
names, paths and clips may not. Fixtures stay generated (152000).

**No FBX copied.** Only the clips are written. The manifest records each clip's source path and hash,
so a port can be re-run while the source drive is attached and a finished clip doesn't need it.

**Not for training.** Every clip is left out of training by default. The owner may choose single
clips later, and only those clips are used. That choice has to be squared first with 152000's rule
that store packs stay out of `training/` unless their publisher has consented in writing.

### What real files show (2026-10-31, today's `ac`, no clip kept)
- **Most files don't import yet.** 24 of a random 150 binary files imported, and 5 of 27 files taken
  one per pack.
- **Bone names are the main cause.** 111 of the 150 were refused as not humanoid. The importer
  doesn't know names such as `Bip001 Pelvis`, `B_Pelvis` or `<prefix> Pelvis`.
- **Unity's `.meta` files already hold the answer.** 6,316 of the 6,333 `.meta` files beside the FBX
  carry a humanoid bone map, including all 46 name refusals checked. Read that map first and guess
  from names only without one.
- **Text FBX.** 664 of the files are text (ASCII), which the reader refuses.
- **Old binary files.** 520 files are FBX 6.1. 11 of the 150 were refused for having no
  `GlobalSettings`, which fits that share, but the link isn't confirmed. Their axes and units need
  another source.
- **`ImageData`.** 3 of the 150 were refused on a property type the reader doesn't know (`c`) in a
  node the animation doesn't need.
- **One file, many clips.** At least 187 `.meta` files split one take into several clips, up to 90
  in one file. The import reads the take as one long clip (one came in at 2,889 frames). Use the
  `.meta` frame ranges, names and loop flags.
- **Duplicates.** Whole packs are present twice, and a bundle repeats packs that are also listed on
  their own. There are 6,116 distinct file names among the 6,489. Port one copy of each and record
  the others.
- **Possible wrong poses.** On 20 of the 24 imported files the importer reads an unsupported scale
  inheritance type as the default. On 7 it reads only the first animation layer or first take. Find
  out whether these change the result, then fix them or refuse the file.

### Orienting and fitting
- Every clip starts facing -Y, and left stays left. Keep root motion, and say which clips are in
  place.
- Hanging, climbing and swinging clips have no floor. They must not be pushed down to z = 0 or
  flagged as a bad floor.
- Fingers, face and props stay at rest, as in 152000. A weapon clip keeps its stance without the
  weapon.
- Review with physics off: the physics pass sits lying bodies up.

### Running it
- A resumable batch driven by the manifest. `clip.import_batch` takes one folder and one `joints`
  correction, which doesn't fit 27 packs.
- The manifest gives every source file one outcome: imported (with its clips), duplicate, or refused
  (with the reason). Nothing is dropped silently.
- Clips are grouped by pack and action in the library. The library panel has about 220 clips today,
  so check that it copes with several thousand.

## Acceptance (proposed)
- **Accounted for:** every file in the inventory has one manifest outcome, the counts add up, and
  the owner agrees every refused group.
- **Reads:** text FBX and FBX 6.1 either import or are refused with a reason, and each case has a
  generated fixture. A generated file with a Unity bone map and clip ranges imports as named clips.
- **Automatic checks on every clip:** values are finite, the first frame faces -Y within 15°, the
  feet aren't swapped, bone lengths are the rig's, and the feet rest on z = 0 where the source's rest
  on its floor. Failures are listed in the manifest and left out of the library until fixed.
- **Owner's eye:** clips are screened and grouped, with one short numbered clip per group beside its
  source (physics off). The owner answers with the numbers to reject, and the reject list is fixed or
  refused before the item is done.
- **Private:** `git status` shows no clip, manifest, pack name or path. A test shows these clips
  can't be uploaded to the community library. A test shows `ac_train` reads none of them unless a
  clip was chosen by name.
- **Re-run:** running the port again changes nothing and skips finished files.
- **Library:** the panel opens, searches and plays a clip from the full set without a visible stall.
