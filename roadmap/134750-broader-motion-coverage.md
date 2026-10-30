# Broader motion coverage, where evaluation finds gaps

- **Status:** not started; the acceptance line needs the owner's agreement
- **Requires:** 126500, 134500
- **Source:** 115100

## What
Not more data for its own sake. Take the weakest slices from 126500, with how many training minutes
each has, and add data only for those: first from sets already registered, then from new ones.
Free-licence sets only (112000): a non-commercial set never goes in the `REGISTRY`, the docs or a
release.

## Acceptance
The result lists the two weakest slices with their minutes and what was added; a retrain improves
those slices on held-out subjects with no other slice getting clearly worse; `ATTRIBUTION.md` and
the manifest have a row for each new set.
