# Community library: browse, install, rate, share, report

- **Status:** built, waiting for the owner's look (acceptance agreed by the owner, 2026-10-30;
  screenshots in `target/shots/2026-10-30/community/`)
- **Added** (owner, 2026-10-30): a pop-out button beside the tab opens the community in a
  resizable window of its own, with a gallery of large playing cards (size slider) or the rows,
  and the selected clip's details at the side.
- **Requires:** none (the server is built: `services/community/`)
- **Source:** owner request, 2026-10-30 ("work on the community tab")

## What
A Community tab in the Library panel that reaches the server in `services/community/` (Supabase
for the clip list, accounts, ratings and reports; clip files on R2). No email: the app signs in
anonymously on first need; sharing and reporting need one captcha in the browser.

**Crate.** A new `crates/ac-community`, standing on `ac-core` and `ac-io` (it installs into the
library), used by `ac-cli` and `ac-app`, registered through `SessionBuilder::commands`. HTTP with
`ureq` (blocking, rustls). `Backend` is the seam, as `Engine` is for `text.*`: `Http` talks to
the server, `Folder` is a fake laid out like it (a `clips.json` and the clip files) for tests and
fixtures. The window runs network calls off its UI thread, like `text.*`.

**Commands** (none undoable: they change the library or the server, not the document):

| Id | Inputs | Outputs |
|---|---|---|
| `community.browse` | `search`, `tags`, `sort` (top, new, downloads), `page` ≥ 0, `limit` 1–100 | `clips`: id, title, author, tags, licence, credit, fps, frames, seconds, rating, ratings, downloads, preview, installed |
| `community.fetch` | `id` | the clip, unpacked into a cache, for the popover to play (not in the library) |
| `community.install` | `id` | the library id (`Community/<title>`; a clash gets a number); the hash is checked; `origin` `community:<id>` and the credit kept |
| `community.rate` | `id`, `stars` 1–5 | `rating`, `ratings` |
| `community.report` | `id`, `reason` ≤ 500 | `reported` |
| `community.verify` | `token` | `verified` (the window gets the token from the browser page) |
| `community.share` | `id` (one of your animations), `title` ≤ 80, `description`, `tags` ≤ 10, `licence` (CC0-1.0, CC-BY-4.0), `credit`, `name` | `clip` id, `status` checking |
| `community.mine` | — | your uploads with `status` and `reason` |
| `community.remove` | `id` | `status` removed |
| `community.account` | — | `signed_in`, `verified`, `name` |

**Window.**
- Library: a third tab, Community, beside Animations and Poses: search, tag chips, sort, rows with
  a thumbnail, stars and downloads, and My uploads (status, reason, Remove).
- Selecting a clip opens the same preview popover as the Library, playing the fetched clip, with
  Install, stars and Report.
- "Share to community…" on your own animations opens a form (title, tags, licence, credit, your
  name).
- The first Share or Report opens `verify.html` in the browser and listens once on `127.0.0.1`
  for the token.
- Offline: says the server can't be reached and shows the last list it fetched.

**Server additions.** A public `settings` row giving the files URL and the captcha page, so the
app never builds in `r2.dev` (the domain can change later without breaking old installs). The
preview is the clip's frames side by side as one PNG strip, the same kind the Library draws for
its thumbnails (`thumb_png` in `crates/ac-app/src/app/library.rs`), rendered when sharing.

**Animated previews (owner's request, 2026-10-30).** Rows in the Community tab play their strip
while they are on screen, through the Library's strip player (`library/hover.rs`). The popover
plays the clip itself through the Library's player.

**Updates.** An installed clip stays as it was installed; following the author's changes, as
built-ins follow the app, is later work (owner agreed, 2026-10-30).

**The account** lives in the app's config folder (`community.json`: the refresh token), not in
the library.

## Acceptance
- `fixtures/community.json` on the `Folder` backend, with hand-worked expectations:
  - `browse` orders (top by rating then count, new by date, downloads) and every search word
    matching;
  - `install` writes `Community/<title>` with its origin and credit, a second install of the
    same title gets `Community/<title> 2`, and a clip whose hash doesn't match is refused;
  - `rate` refuses 0 and 6 and your own clip;
  - `report` before `verify` refuses and says why;
  - `share` refuses a pose, a missing licence and an 81-character title;
  - `mine` lists checking, published and failed with the reason.
  - `errors`: bad input refused, nothing changed.
- A live smoke test (`cargo test -p ac-community -- --ignored live`, not in CI) against the real
  server: sign in, browse, and rate a test clip, then clean up.
- `cargo xtask check` passes.
- Screenshots in `target/shots/<date>/community/`: the tab (and a short video of rows playing), the popover on a clip, the share form,
  My uploads with a checking and a failed clip, and the offline message.
- The owner shares one clip from the app, ticks the captcha, and sees it go live (needs 150000 to
  publish it).
