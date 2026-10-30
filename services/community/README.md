# Community backend

The server side of the community library: people share clips, rate them and report bad ones.
Supabase holds accounts, the clip list, ratings, reports and the activity log, and runs the Edge
Functions; Cloudflare R2 holds the clip files, previews and the captcha page, which anyone
downloads from the bucket's public URL. The app itself stays Rust; this folder is SQL and
TypeScript (Deno) and is never part of a build.

## Accounts: anonymous, no email
There are no email accounts (owner's decision, 2026-10-30). The app signs in anonymously the first
time it needs to; that account lives in the app's settings on that computer.
- **Rating** needs only the anonymous account.
- **Sharing and reporting** need the account to have passed a captcha once: the app opens
  `verify.html` in the browser, Turnstile checks the person, the page hands the token back to the
  app on `127.0.0.1`, and the app sends it to `verify`. The account is then verified for good.
- **Spam** is handled by the activity log (account and address of every verify, upload, report and
  removal), limits per account and per address, and blocks (below).

| Path | What |
|---|---|
| `supabase/migrations/` | tables, grants, row-level security, triggers, `block_author` |
| `supabase/functions/verify` | `POST {token}`: checks the captcha with Turnstile and verifies the account |
| `supabase/functions/upload` | `POST` multipart (`meta` JSON, `clip` gzipped, optional `preview` WebP) from a verified account; light checks, files to R2, the clip `checking` |
| `supabase/functions/report` | `POST {id, reason?}` from a verified account |
| `supabase/functions/remove` | the author removes their clip: files deleted, row marked `removed` |
| `supabase/functions/checker` | the checker's door (header `x-checker-secret`): `pending` lists clips waiting, `result` publishes one or deletes its files and marks it `failed` |
| `supabase/functions/_shared/common.ts` | service-role client, caller, blocks and captcha check, limits, log, R2 |
| `web/verify.html` | the captcha page, uploaded to the bucket by `scripts/bucket.ts page` |
| `scripts/bucket.ts` | bucket upkeep: `list`, `delete`, `orphans [--delete]`, `page` |
| `.env.local` | secrets, gitignored: `SUPABASE_ACCESS_TOKEN`, `R2_*`, `CHECKER_SECRET`, `TURNSTILE_SITE_KEY`, `TURNSTILE_SECRET` |

## Limits
| What | Per account | Per address |
|---|---|---|
| New anonymous accounts | | 10 an hour (Auth settings) |
| Captcha verifications | | 10 a day |
| Uploads | 20 a day | 30 a day |
| Reports | | 20 a day |

## How a clip goes live
1. The app uploads it to `upload`; it is `checking` and only its author sees it.
2. The checker (a GitHub workflow running the app's own checks; not built yet) asks `checker` for
   `pending` clips, downloads each and sends `result`.
3. Passed: `published`, visible to everyone. Failed: files deleted, `fail_reason` says why.
4. Reports from three different addresses hide it (`hidden`) until a maintainer sets the status.

## Dealing with a spammer (dashboard → SQL editor)
- See what an account or address did: `select * from activity where user_id = '<id>' order by at desc;`
  (or `where ip = '<address>'`).
- Block a clip's author, every address they used, and hide all their clips:
  `select block_author('<clip id>', 'spam');`
- Block by hand: `insert into blocks (user_id, ip, reason, expires_at) values (...)`; either id or
  address may be empty, and an empty `expires_at` blocks for good. Delete the row to unblock.
- Blocking an address blocks everyone behind it (a school or office shares one), so prefer
  `expires_at` for address blocks.
- Hidden clips keep their files; `scripts/bucket.ts orphans --delete` clears files of removed
  ones that were left behind.

## What clients read and write directly (REST, publishable key)
- `clips`: published ones, plus the caller's own. The author's name:
  `select=*,author:profiles!clips_author_id_fkey(display_name)`.
- `ratings`: 1–5 stars, one per account per clip, not your own clip.
- `profiles`: the caller's own `display_name`.
- `rpc/count_download`: after installing a clip.

Everything else is closed: tables aren't exposed unless a migration grants them, and every table
has row-level security.

## Deploying
From this folder, with the secrets loaded (`set -a; . ./.env.local; set +a` in bash):

```
npx supabase link --project-ref ksnxwkxaunxutkmdhjzm
npx supabase db push --linked
npx supabase functions deploy --use-api
npx supabase secrets set --env-file <file with only the R2_*, CHECKER_SECRET and TURNSTILE_SECRET lines>
npx deno run --allow-env --allow-net --allow-read scripts/bucket.ts page
```

Anonymous sign-ins are on for the project (Auth settings), at 10 new accounts an hour per address.
