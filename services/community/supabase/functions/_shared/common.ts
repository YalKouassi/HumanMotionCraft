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

/// The signed-in caller. Anonymous accounts (which can rate) are refused when `named` is set.
export async function caller(db: SupabaseClient, req: Request, named: boolean): Promise<User> {
  const token = req.headers.get("authorization")?.replace(/^Bearer\s+/i, "");
  if (!token) throw new Refusal(401, "sign in first");
  const { data, error } = await db.auth.getUser(token);
  if (error || !data.user) throw new Refusal(401, "sign in first");
  if (named && data.user.is_anonymous) {
    throw new Refusal(403, "sharing needs an account with an email address");
  }
  return data.user;
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
