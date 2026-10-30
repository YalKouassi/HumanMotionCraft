// POST {"token"}: the captcha token the browser page (`web/verify.html`) handed the app. Checked
// with Cloudflare Turnstile; on success the account is verified for good and may share and report.

import { admin, caller, env, guard, log, Refusal, reply, serve, today } from "../_shared/common.ts";

const PER_ADDRESS = 10;

serve(async (req) => {
  if (req.method !== "POST") throw new Refusal(405, "use POST");
  const db = admin();
  const who = await caller(db, req);
  await guard(db, who, false);
  if ((await today(db, who, "verify")).address >= PER_ADDRESS) {
    throw new Refusal(429, "too many new accounts from here today; try again tomorrow");
  }
  const { token } = await req.json().catch(() => ({}));
  if (typeof token !== "string" || !token) throw new Refusal(400, "token is required");

  const form = new FormData();
  form.set("secret", env("TURNSTILE_SECRET"));
  form.set("response", token);
  if (who.ip) form.set("remoteip", who.ip);
  const res = await fetch("https://challenges.cloudflare.com/turnstile/v0/siteverify", {
    method: "POST",
    body: form,
  });
  const outcome = await res.json();
  if (!outcome.success) throw new Refusal(403, "the captcha didn't pass; try again");

  const { error } = await db.from("profiles").update({ verified_at: new Date().toISOString() })
    .eq("id", who.user.id);
  if (error) throw error;
  await log(db, who, "verify", null);
  return reply(200, { verified: true });
});
