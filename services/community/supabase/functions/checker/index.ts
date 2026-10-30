// The checker's side door (header `x-checker-secret`), used by the GitHub workflow that runs the
// app's own checks on new clips:
//   {"action": "pending"}                          → {clips: [{id, url}]} still waiting
//   {"action": "result", "id", "ok", "reason"?}    → publishes the clip, or deletes its files and
//                                                    marks it failed with the reason

import { admin, Bucket, env, Refusal, reply, retire, serve } from "../_shared/common.ts";

serve(async (req) => {
  if (req.method !== "POST") throw new Refusal(405, "use POST");
  if (!same(req.headers.get("x-checker-secret") ?? "", env("CHECKER_SECRET"))) {
    throw new Refusal(401, "not the checker");
  }
  const body = await req.json().catch(() => ({}));
  const db = admin();

  if (body.action === "pending") {
    const { data, error } = await db.from("clips").select("id, file_key")
      .eq("status", "checking").order("created_at").limit(50);
    if (error) throw error;
    return reply(200, { clips: data.map((c) => ({ id: c.id, url: Bucket.url(c.file_key) })) });
  }

  if (body.action === "result") {
    if (typeof body.id !== "string") throw new Refusal(400, "id is required");
    const { data: clip } = await db.from("clips").select("status").eq("id", body.id).single();
    if (!clip) throw new Refusal(404, "no such clip");
    if (clip.status !== "checking") return reply(200, { id: body.id, status: clip.status });
    if (body.ok === true) {
      const { error } = await db.from("clips")
        .update({ status: "published", published_at: new Date().toISOString() })
        .eq("id", body.id);
      if (error) throw error;
      return reply(200, { id: body.id, status: "published" });
    }
    const reason = typeof body.reason === "string" && body.reason
      ? body.reason.slice(0, 1000)
      : "the clip didn't pass the checks";
    await retire(db, body.id, "failed", reason);
    return reply(200, { id: body.id, status: "failed" });
  }

  throw new Refusal(400, 'action must be "pending" or "result"');
});

/// Compares secrets in time that doesn't depend on where they differ.
function same(a: string, b: string): boolean {
  const x = new TextEncoder().encode(a);
  const y = new TextEncoder().encode(b);
  let diff = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return diff === 0;
}
