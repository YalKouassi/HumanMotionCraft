// What every community function shares: the service-role database client, the caller's account,
// the R2 bucket and JSON replies.

import { createClient, type SupabaseClient, type User } from "npm:@supabase/supabase-js@2";
import { AwsClient } from "npm:aws4fetch@1";

export function env(name: string): string {
  const value = Deno.env.get(name);
  if (!value) throw new Error(`missing secret ${name}`);
  return value;
}

/// The service-role client: bypasses row-level security, so only these functions hold it.
export function admin(): SupabaseClient {
  const keys = Deno.env.get("SUPABASE_SECRET_KEYS");
  const key = keys ? JSON.parse(keys).default : env("SUPABASE_SERVICE_ROLE_KEY");
  return createClient(env("SUPABASE_URL"), key, { auth: { persistSession: false } });
}

export function reply(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/// A refusal the caller can show as it is.
export class Refusal extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

/// Runs `handle`, turning a `Refusal` into its status and anything else into a 500.
export function serve(handle: (req: Request) => Promise<Response>) {
  Deno.serve(async (req) => {
    try {
      return await handle(req);
    } catch (e) {
      if (e instanceof Refusal) return reply(e.status, { error: e.message });
      console.error(e);
      return reply(500, { error: "something went wrong on the server" });
    }
  });
}

/// Who is calling: their account (anonymous ones included) and the address they call from.
export type Caller = { user: User; ip: string | null };

export async function caller(db: SupabaseClient, req: Request): Promise<Caller> {
  const token = req.headers.get("authorization")?.replace(/^Bearer\s+/i, "");
  if (!token) throw new Refusal(401, "sign in first");
  const { data, error } = await db.auth.getUser(token);
  if (error || !data.user) throw new Refusal(401, "sign in first");
  const ip = req.headers.get("x-forwarded-for")?.split(",")[0].trim() || null;
  return { user: data.user, ip };
}

/// Refuses a blocked account or address, and (with `verified`) one that hasn't passed the captcha.
export async function guard(db: SupabaseClient, who: Caller, verified: boolean): Promise<void> {
  const match = who.ip
    ? `user_id.eq.${who.user.id},ip.eq."${who.ip}"`
    : `user_id.eq.${who.user.id}`;
  const { data: blocks, error } = await db.from("blocks").select("expires_at").or(match);
  if (error) throw error;
  if (blocks.some((b) => !b.expires_at || new Date(b.expires_at) > new Date())) {
    throw new Refusal(403, "this account can't share or report");
  }
  if (!verified) return;
  const { data } = await db.from("profiles").select("verified_at").eq("id", who.user.id).single();
  if (!data?.verified_at) throw new Refusal(403, "captcha needed");
}

/// How many times the account, and the address, did `action` in the last day.
export async function today(
  db: SupabaseClient,
  who: Caller,
  action: string,
): Promise<{ account: number; address: number }> {
  const since = new Date(Date.now() - 24 * 3600 * 1000).toISOString();
  const count = async (column: string, value: string | null) => {
    if (!value) return 0;
    const { count, error } = await db.from("activity").select("id", { count: "exact", head: true })
      .eq(column, value).eq("action", action).gte("at", since);
    if (error) throw error;
    return count ?? 0;
  };
  return { account: await count("user_id", who.user.id), address: await count("ip", who.ip) };
}

/// Records what the caller did, for limits and for maintainers chasing spam.
export async function log(
  db: SupabaseClient,
  who: Caller,
  action: "verify" | "upload" | "report" | "remove",
  clipId: string | null,
): Promise<void> {
  const { error } = await db.from("activity")
    .insert({ user_id: who.user.id, ip: who.ip, action, clip_id: clipId });
  if (error) console.error("logging", action, error);
}

/// The public bucket the clips and previews live in.
export class Bucket {
  private aws = new AwsClient({
    accessKeyId: env("R2_ACCESS_KEY_ID"),
    secretAccessKey: env("R2_SECRET_ACCESS_KEY"),
    service: "s3",
    region: "auto",
  });
  private base = `${env("R2_ACCOUNT_ENDPOINT")}/${env("R2_BUCKET_PUBLIC")}`;

  async put(key: string, body: Uint8Array<ArrayBuffer>, type: string): Promise<void> {
    const res = await this.aws.fetch(`${this.base}/${key}`, {
      method: "PUT",
      body,
      headers: { "content-type": type, "cache-control": "public, max-age=31536000, immutable" },
    });
    if (!res.ok) throw new Error(`R2 put ${key}: ${res.status} ${await res.text()}`);
  }

  async delete(key: string): Promise<void> {
    const res = await this.aws.fetch(`${this.base}/${key}`, { method: "DELETE" });
    if (!res.ok && res.status !== 404) throw new Error(`R2 delete ${key}: ${res.status}`);
  }

  /// Where anyone can download `key`.
  static url(key: string): string {
    return `${env("R2_PUBLIC_URL")}/${key}`;
  }
}

/// Deletes a clip's files and marks it `status` (failed or removed).
export async function retire(
  db: SupabaseClient,
  id: string,
  status: "failed" | "removed",
  reason: string | null,
): Promise<void> {
  const { data: clip, error } = await db.from("clips").select("file_key, preview_key")
    .eq("id", id).single();
  if (error || !clip) throw new Refusal(404, "no such clip");
  const bucket = new Bucket();
  await bucket.delete(clip.file_key);
  if (clip.preview_key) await bucket.delete(clip.preview_key);
  const { error: e } = await db.from("clips").update({ status, fail_reason: reason }).eq("id", id);
  if (e) throw e;
}

export async function sha256(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}
