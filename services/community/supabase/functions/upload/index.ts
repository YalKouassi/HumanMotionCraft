// POST multipart/form-data from the app's "Share to community":
//   meta     JSON {title, description?, tags?, licence, credit?, display_name?}
//   clip     the clip file, gzipped
//   preview  optional looping preview, WebP
// Light checks here (size, gzip, the clip's shape); the checker runs the app's own checks before
// the clip is published. Replies {id, status: "checking"}.

import { admin, Bucket, caller, env, Refusal, reply, serve, sha256 } from "../_shared/common.ts";

const MAX_BODY = 12 * 1024 * 1024;
const MAX_CLIP = 10 * 1024 * 1024;
const MAX_UNZIPPED = 100 * 1024 * 1024;
const MAX_PREVIEW = 1024 * 1024;
const UPLOADS_PER_DAY = 20;
const LICENCES = ["CC0-1.0", "CC-BY-4.0"];

serve(async (req) => {
  if (req.method !== "POST") throw new Refusal(405, "use POST");
  const length = Number(req.headers.get("content-length"));
  if (!length) throw new Refusal(411, "send the upload's length");
  if (length > MAX_BODY) throw new Refusal(413, "the upload is too big (12 MB at most)");

  const db = admin();
  const user = await caller(db, req, true);

  const since = new Date(Date.now() - 24 * 3600 * 1000).toISOString();
  const { count } = await db.from("clips").select("id", { count: "exact", head: true })
    .eq("author_id", user.id).gte("created_at", since);
  if ((count ?? 0) >= UPLOADS_PER_DAY) {
    throw new Refusal(429, `you can share ${UPLOADS_PER_DAY} clips a day; try again tomorrow`);
  }

  const form = await req.formData();
  const meta = readMeta(form.get("meta"));
  const clipFile = form.get("clip");
  if (!(clipFile instanceof File)) throw new Refusal(400, "the clip file is missing");
  if (clipFile.size > MAX_CLIP) throw new Refusal(413, "the clip is too big (10 MB gzipped)");
  const gz = new Uint8Array(await clipFile.arrayBuffer());
  const shape = readClip(await gunzip(gz));

  const previewFile = form.get("preview");
  let preview: Uint8Array<ArrayBuffer> | null = null;
  if (previewFile instanceof File) {
    if (previewFile.size > MAX_PREVIEW) throw new Refusal(413, "the preview is too big (1 MB)");
    preview = new Uint8Array(await previewFile.arrayBuffer());
    if (!isWebp(preview)) throw new Refusal(400, "the preview must be a WebP image");
  }

  if (meta.display_name) {
    await db.from("profiles").update({ display_name: meta.display_name }).eq("id", user.id);
  }

  const id = crypto.randomUUID();
  const fileKey = `clips/${id}.clip.json.gz`;
  const previewKey = preview ? `previews/${id}.webp` : null;
  const { error } = await db.from("clips").insert({
    id,
    author_id: user.id,
    title: meta.title,
    description: meta.description,
    tags: meta.tags,
    licence: meta.licence,
    credit: meta.credit,
    format_version: shape.version,
    fps: shape.fps,
    frames: shape.frames,
    size_bytes: gz.length,
    sha256: await sha256(gz),
    file_key: fileKey,
    preview_key: previewKey,
  });
  if (error) throw error;

  const bucket = new Bucket();
  try {
    await bucket.put(fileKey, gz, "application/gzip");
    if (preview && previewKey) await bucket.put(previewKey, preview, "image/webp");
  } catch (e) {
    await bucket.delete(fileKey).catch(() => {});
    await db.from("clips").delete().eq("id", id);
    throw e;
  }

  await wakeChecker();
  return reply(201, { id, status: "checking" });
});

type Meta = {
  title: string;
  description: string;
  tags: string[];
  licence: string;
  credit: string;
  display_name: string | null;
};

function readMeta(raw: FormDataEntryValue | null): Meta {
  if (typeof raw !== "string") throw new Refusal(400, "the clip's details are missing");
  let m: Record<string, unknown>;
  try {
    m = JSON.parse(raw);
  } catch {
    throw new Refusal(400, "the clip's details aren't valid JSON");
  }
  const text = (v: unknown, name: string, max: number, required = false): string => {
    const s = typeof v === "string" ? v.trim() : "";
    if (required && !s) throw new Refusal(400, `${name} is required`);
    if (s.length > max) throw new Refusal(400, `${name} is too long (${max} characters at most)`);
    return s;
  };
  const licence = text(m.licence, "licence", 20, true);
  if (!LICENCES.includes(licence)) {
    throw new Refusal(400, `licence must be one of ${LICENCES.join(", ")}`);
  }
  const tags = [
    ...new Set(
      (Array.isArray(m.tags) ? m.tags : []).map((t) =>
        typeof t === "string" ? t.trim().toLowerCase().replace(/\s+/g, " ") : ""
      ),
    ),
  ].filter((t) => t);
  if (tags.length > 10) throw new Refusal(400, "10 tags at most");
  for (const t of tags) {
    if (t.length > 24 || !/^[a-z0-9][a-z0-9 -]*$/.test(t)) {
      throw new Refusal(400, `tag "${t}": letters, digits, spaces and hyphens, 24 at most`);
    }
  }
  const name = text(m.display_name, "display name", 40);
  return {
    title: text(m.title, "title", 80, true),
    description: text(m.description, "description", 2000),
    tags,
    licence,
    credit: text(m.credit, "credit", 200),
    display_name: name || null,
  };
}

async function gunzip(gz: Uint8Array<ArrayBuffer>): Promise<Uint8Array> {
  if (gz[0] !== 0x1f || gz[1] !== 0x8b) throw new Refusal(400, "the clip must be gzipped");
  const reader = new Blob([gz]).stream().pipeThrough(new DecompressionStream("gzip"))
    .getReader();
  const parts: Uint8Array[] = [];
  let total = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.length;
      if (total > MAX_UNZIPPED) throw new Refusal(413, "the clip is too big unzipped (100 MB)");
      parts.push(value);
    }
  } catch (e) {
    if (e instanceof Refusal) throw e;
    throw new Refusal(400, "the clip isn't a valid gzip file");
  }
  const out = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/// The clip's shape: a clip file of any of the app's names, with bones and keyed frames.
function readClip(bytes: Uint8Array): { version: number; fps: number; frames: number } {
  let c: Record<string, unknown>;
  try {
    c = JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    throw new Refusal(400, "the clip isn't valid JSON");
  }
  const bad = (why: string) => new Refusal(400, `not a clip file: ${why}`);
  if (typeof c.format !== "string" || !c.format.endsWith(".clip")) throw bad("format");
  if (!Number.isInteger(c.version) || (c.version as number) < 1) throw bad("version");
  if (typeof c.fps !== "number" || c.fps <= 0 || c.fps > 240) throw bad("fps");
  if (!Array.isArray(c.bones) || c.bones.length === 0) throw bad("bones");
  if (!Array.isArray(c.keys) || c.keys.length === 0) throw bad("keys");
  let last = 0;
  for (const k of c.keys) {
    const f = (k as Record<string, unknown>)?.frame;
    if (!Number.isInteger(f) || (f as number) < 0) throw bad("a key's frame");
    last = Math.max(last, f as number);
  }
  return { version: c.version as number, fps: c.fps, frames: last + 1 };
}

function isWebp(b: Uint8Array): boolean {
  const ascii = (from: number, to: number) => String.fromCharCode(...b.subarray(from, to));
  return b.length > 12 && ascii(0, 4) === "RIFF" && ascii(8, 12) === "WEBP";
}

/// Starts the checker at once (a GitHub workflow); its schedule catches anything this misses.
async function wakeChecker(): Promise<void> {
  const token = Deno.env.get("CHECKER_DISPATCH_TOKEN");
  if (!token) return;
  await fetch(`https://api.github.com/repos/${env("CHECKER_REPO")}/dispatches`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      accept: "application/vnd.github+json",
      "user-agent": "community-upload",
    },
    body: JSON.stringify({ event_type: "community-check" }),
  }).catch((e) => console.error("waking the checker:", e));
}
