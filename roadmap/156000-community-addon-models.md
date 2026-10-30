# Add-on models in the community library

- **Status:** not started; official models only, with no way for anyone else to share one (owner, 2026-10-30)
- **Requires:** 155000, 150000
- **Source:** owner request (2026-10-30)

## What
Browse and install add-on models from the Community tab beside clips, so the app stays small and only
the base ships with it.

**Official models only.** We publish them with a script, as
`services/community/scripts/official.ts` does for clips. There is no "share model" option: the app
offers none, `community.share` stays clips only, and the server's `upload` function refuses anything
but a clip. The checker can test a model's format and numbers but not what it was trained on, and
sharing has no manual approval (150000), so a model trained on data whose licence forbids it would
look like any other.

- **Server:** a `kind` for clip or model on the list, the base's tag and hash, and the model's card
  fields. A model is about 19.5 MB and the upload limit is 12 MB
  (`supabase/functions/upload/index.ts:23`): the script writes to R2 directly and this limit stays.
- **Files:** r2.dev is for light use, so move to a custom domain before offering files this size.
- **App:** `community.install` puts a model in the app's own models folder (`ac_io::library::app_models_dir`)
  after checking its hash; 155000 then lists it. Rating and reporting work as they do for clips.
- **Safety:** weights are read by our own Rust loader and are data, not code. Every number and
  shape is hostile input: caps, finite checks, a refusal with a reason.
- **Preview:** a model has no clip to play, so its row plays a sample fill.

Opening uploads to everyone is a separate decision for the owner, once there's a way to answer "what was
this trained on?".

## Acceptance
- A fixture on the fake backend browses a model, installs it, and has `ai.models` list it. A file with the
  wrong hash, a broken card or a non-finite weight is refused, with nothing installed.
- A live test publishes one official add-on, then installs it in the app and fills with it.
- `community.share` and the `upload` function both refuse a model, and the app shows no option to share one.
- Clips and their fixtures behave as before.
- The owner approves the tab with a model in it.
