// The Official collection (roadmap 151000): the built-in clips that moved out of the app, published
// under the Official account at once (no checker: they are the app's own, already checked).
// Run from services/community with the secrets loaded:
//   npx deno run --allow-env --allow-net --allow-read --allow-write scripts/official.ts \
//     official/ids.txt <strips dir> [--dry]
// <strips dir> holds `ac strips` pictures: `ac strips ../../assets/library official/ids.txt --out <dir>`.
//
// Each clip's file goes up as it is in assets/library, gzipped; its strip is the preview. What was
// published goes into ../../assets/library/moved.json (library id -> clip, licence, credit), which
// the app ships so `library.sync` keeps a moved clip as a community install. Run again, it skips
// what moved.json already holds and the server still has: it can be resumed after a failure.

import { AwsClient } from "npm:aws4fetch@1";

const REF = "ksnxwkxaunxutkmdhjzm";
const SERVER = `https://${REF}.supabase.co`;
const LIBRARY = "../../assets/library/animations";
const MOVED = "../../assets/library/moved.json";
const OFFICIAL = "Official";
const OFFICIAL_EMAIL = "official@humanmotioncraft.invalid";
const MAX_PREVIEW = 1024 * 1024;

// Licence, credit and description by the clip's `dataset` (ATTRIBUTION.md has the sources).
const SOURCES: Record<string, { licence: string; credit: string; description: string }> = {
  cmu: {
    licence: "CMU-MOCAP",
    credit: "mocap.cs.cmu.edu; the database was created with funding from NSF EIA-0196217",
    description: "Motion capture from the CMU Graphics Lab Motion Capture Database.",
  },
  "100style": {
    licence: "CC-BY-4.0",
    credit: "100STYLE (Ian Mason, Sebastian Starke, Taku Komura); retargeted and cut into clips",
    description: "Motion capture from the 100STYLE dataset of walking styles.",
  },
  kimodo: {
    licence: "CC0-1.0",
    credit: "Generated with NVIDIA Kimodo",
    description: "Generated from a text prompt with NVIDIA's Kimodo model.",
  },
};

type Moved = {
  clip: string;
  title: string;
  author: string;
  licence: string;
  credit: string;
};

const env = (name: string) => {
  const v = Deno.env.get(name);
  if (!v) throw new Error(`missing ${name} (load .env.local)`);
  return v;
};

async function serviceKey(): Promise<string> {
  const res = await fetch(`https://api.supabase.com/v1/projects/${REF}/api-keys?reveal=true`, {
    headers: { Authorization: `Bearer ${env("SUPABASE_ACCESS_TOKEN")}` },
  });
  if (!res.ok) throw new Error(`api-keys: ${res.status} ${await res.text()}`);
  const keys = (await res.json()) as { name: string; api_key: string }[];
  const k = keys.find((k) => k.name === "service_role");
  if (!k) throw new Error("no service_role key");
  return k.api_key;
}

const KEY = await serviceKey();

async function api(path: string, init: RequestInit = {}): Promise<unknown> {
  const res = await fetch(`${SERVER}${path}`, {
    ...init,
    headers: {
      apikey: KEY,
      Authorization: `Bearer ${KEY}`,
      "Content-Type": "application/json",
      Prefer: "return=representation",
      ...(init.headers ?? {}),
    },
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${init.method ?? "GET"} ${path}: ${res.status} ${text}`);
  return text ? JSON.parse(text) : null;
}

/** The Official account's id, made the first time. */
async function official(dry: boolean): Promise<string> {
  const found = (await api("/rest/v1/profiles?select=id&official=is.true")) as { id: string }[];
  if (found.length > 0) return found[0].id;
  if (dry) return "(the Official account, made on a real run)";
  const password = crypto.randomUUID() + crypto.randomUUID();
  const user = (await api("/auth/v1/admin/users", {
    method: "POST",
    body: JSON.stringify({ email: OFFICIAL_EMAIL, password, email_confirm: true }),
  })) as { id: string };
  await api(`/rest/v1/profiles?id=eq.${user.id}`, {
    method: "PATCH",
    body: JSON.stringify({ official: true, display_name: OFFICIAL }),
  });
  console.log(`made the Official account ${user.id}`);
  return user.id;
}

async function gzip(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
  const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function sha256(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const d = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return Array.from(d, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** A word list the server accepts: lower case, letters, digits and hyphens, 24 each. */
function tag(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 24);
}

function titleOf(group: string, name: string): string {
  return group === "Walk styles" && !/walk/i.test(name) ? `${name} walk` : name;
}

function tagsOf(group: string, name: string, dataset: string): string[] {
  const kind = tag(name.replace(/\s*\d+$/, ""));
  const all = [
    tag(group === "Walk styles" ? "walk style" : group),
    group === "Walk styles" ? "walk" : kind,
    dataset === "kimodo" ? "generated" : "mocap",
  ];
  return [...new Set(all.filter((t) => t.length > 0))];
}

const aws = new AwsClient({
  accessKeyId: env("R2_ACCESS_KEY_ID"),
  secretAccessKey: env("R2_SECRET_ACCESS_KEY"),
  service: "s3",
  region: "auto",
});
const bucket = `${env("R2_ACCOUNT_ENDPOINT")}/${env("R2_BUCKET_PUBLIC")}`;

async function put(key: string, body: Uint8Array<ArrayBuffer>, type: string) {
  const res = await aws.fetch(`${bucket}/${key}`, {
    method: "PUT",
    body,
    headers: { "Content-Type": type },
  });
  if (!res.ok) throw new Error(`put ${key}: ${res.status} ${await res.text()}`);
}

async function readMoved(): Promise<Record<string, Moved>> {
  try {
    const m = JSON.parse(await Deno.readTextFile(MOVED));
    return m.animations ?? {};
  } catch (e) {
    if (e instanceof Deno.errors.NotFound) return {};
    throw e;
  }
}

async function writeMoved(moved: Record<string, Moved>) {
  const sorted = Object.fromEntries(Object.entries(moved).sort(([a], [b]) => (a < b ? -1 : 1)));
  await Deno.writeTextFile(MOVED, JSON.stringify({ animations: sorted }, null, 1) + "\n");
}

const [idsFile, stripsDir, ...rest] = Deno.args;
if (!idsFile || !stripsDir) throw new Error("official.ts <ids file> <strips dir> [--dry]");
const dry = rest.includes("--dry");
const ids = (await Deno.readTextFile(idsFile)).split("\n").map((l) => l.trim()).filter((l) => l);
const author = await official(dry);
const moved = await readMoved();
const live = new Set(
  ((await api(`/rest/v1/clips?select=id&status=eq.published&limit=10000`)) as { id: string }[]).map(
    (c) => c.id,
  ),
);
const start = Date.now();

for (const [i, id] of ids.entries()) {
  if (moved[id] && live.has(moved[id].clip)) {
    console.log(`already  ${id}`);
    continue;
  }
  const text = await Deno.readTextFile(`${LIBRARY}/${id}.clip.json`);
  const c = JSON.parse(text);
  const source = SOURCES[c.dataset];
  if (!source) throw new Error(`${id}: no licence for dataset ${c.dataset}`);
  const group = id.split("/")[0];
  const name = id.split("/").slice(1).join("/");
  const title = titleOf(group, name);
  const frames = Math.max(...c.keys.map((k: { frame: number }) => k.frame)) + 1;
  const gz = await gzip(new TextEncoder().encode(text));
  let preview: Uint8Array<ArrayBuffer> | null = null;
  try {
    preview = new Uint8Array(await Deno.readFile(`${stripsDir}/${id}.png`));
  } catch {
    console.log(`         ${id}: no strip, published without a preview`);
  }
  if (preview && preview.length > MAX_PREVIEW) {
    throw new Error(`${id}: its strip is over 1 MB`);
  }
  const clip = crypto.randomUUID();
  const row = {
    id: clip,
    author_id: author,
    title,
    description: source.description,
    tags: tagsOf(group, name, c.dataset),
    licence: source.licence,
    credit: source.credit,
    format_version: c.version,
    fps: c.fps,
    frames,
    size_bytes: gz.length,
    sha256: await sha256(gz),
    file_key: `clips/${clip}.clip.json.gz`,
    preview_key: preview ? `previews/${clip}.png` : null,
    status: "published",
    // the list's order, newest first, for "Newest"
    published_at: new Date(start - i * 1000).toISOString(),
  };
  if (dry) {
    console.log(`would    ${id} -> ${title} [${row.tags.join(", ")}] ${row.licence}`);
    continue;
  }
  await put(row.file_key, gz, "application/gzip");
  if (preview && row.preview_key) await put(row.preview_key, preview, "image/png");
  await api("/rest/v1/clips", { method: "POST", body: JSON.stringify(row) });
  moved[id] = { clip, title, author: OFFICIAL, licence: source.licence, credit: source.credit };
  await writeMoved(moved);
  console.log(`published ${id} -> ${clip}`);
}
console.log(`${Object.keys(moved).length} in moved.json`);
