// Maintenance for the public bucket, run from services/community with the secrets loaded:
//   npx deno run --allow-env --allow-net --allow-read scripts/bucket.ts list
//   ... scripts/bucket.ts delete <key>...
//   ... scripts/bucket.ts orphans [--delete]   files no clip row points at
//   ... scripts/bucket.ts page   uploads web/verify.html as verify.html, with TURNSTILE_SITE_KEY

import { AwsClient } from "npm:aws4fetch@1";

const env = (name: string) => {
  const v = Deno.env.get(name);
  if (!v) throw new Error(`missing ${name} (load .env.local)`);
  return v;
};
const aws = new AwsClient({
  accessKeyId: env("R2_ACCESS_KEY_ID"),
  secretAccessKey: env("R2_SECRET_ACCESS_KEY"),
  service: "s3",
  region: "auto",
});
const base = `${env("R2_ACCOUNT_ENDPOINT")}/${env("R2_BUCKET_PUBLIC")}`;

async function list(): Promise<string[]> {
  const keys: string[] = [];
  let token = "";
  for (;;) {
    const q = new URLSearchParams({ "list-type": "2" });
    if (token) q.set("continuation-token", token);
    const xml = await (await aws.fetch(`${base}?${q}`)).text();
    for (const m of xml.matchAll(/<Key>([^<]+)<\/Key>/g)) keys.push(m[1]);
    const next = xml.match(/<NextContinuationToken>([^<]+)</);
    if (!next) return keys;
    token = next[1];
  }
}

async function remove(key: string) {
  const res = await aws.fetch(`${base}/${key}`, { method: "DELETE" });
  console.log(`delete ${key}: ${res.status}`);
}

/// Keys the clips table points at, read through the management API.
async function referenced(): Promise<Set<string>> {
  const res = await fetch(
    "https://api.supabase.com/v1/projects/ksnxwkxaunxutkmdhjzm/database/query",
    {
      method: "POST",
      headers: {
        authorization: `Bearer ${env("SUPABASE_ACCESS_TOKEN")}`,
        "content-type": "application/json",
      },
      body: JSON.stringify({
        query: "select file_key as k from public.clips where status <> 'removed' and " +
          "status <> 'failed' union select preview_key from public.clips where " +
          "preview_key is not null and status <> 'removed' and status <> 'failed'",
      }),
    },
  );
  const rows: { k: string }[] = await res.json();
  return new Set(rows.map((r) => r.k));
}

const [command, ...rest] = Deno.args;
if (command === "list") {
  for (const k of await list()) console.log(k);
} else if (command === "delete") {
  for (const k of rest) await remove(k);
} else if (command === "orphans") {
  const keep = await referenced();
  const orphans = (await list()).filter((k) =>
    (k.startsWith("clips/") || k.startsWith("previews/")) && !keep.has(k)
  );
  for (const k of orphans) rest.includes("--delete") ? await remove(k) : console.log(k);
} else if (command === "page") {
  const html = (await Deno.readTextFile("web/verify.html"))
    .replaceAll("__SITE_KEY__", env("TURNSTILE_SITE_KEY"));
  const res = await aws.fetch(`${base}/verify.html`, {
    method: "PUT",
    body: html,
    headers: { "content-type": "text/html; charset=utf-8", "cache-control": "public, max-age=300" },
  });
  console.log(`verify.html: ${res.status} → ${env("R2_PUBLIC_URL")}/verify.html`);
} else {
  console.log("usage: bucket.ts list | delete <key>... | orphans [--delete] | page");
}
