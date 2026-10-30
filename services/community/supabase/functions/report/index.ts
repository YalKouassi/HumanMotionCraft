// POST {"id", "reason"?} from a verified account: reports a published clip that isn't the
// caller's. Reports from three different addresses hide it (the `count_report` trigger).

import { admin, caller, guard, log, Refusal, reply, serve, today } from "../_shared/common.ts";

const PER_ADDRESS = 20;

serve(async (req) => {
  if (req.method !== "POST") throw new Refusal(405, "use POST");
  const db = admin();
  const who = await caller(db, req);
  await guard(db, who, true);
  if ((await today(db, who, "report")).address >= PER_ADDRESS) {
    throw new Refusal(429, "that's as many reports as can be sent in a day");
  }
  const { id, reason } = await req.json().catch(() => ({}));
  if (typeof id !== "string") throw new Refusal(400, "id is required");
  const why = typeof reason === "string" ? reason.trim().slice(0, 500) : "";

  const { data: clip } = await db.from("clips").select("author_id, status").eq("id", id).single();
  if (!clip || clip.status !== "published") throw new Refusal(404, "no such clip");
  if (clip.author_id === who.user.id) throw new Refusal(400, "that's your own clip");

  const { error } = await db.from("reports")
    .insert({ clip_id: id, user_id: who.user.id, reason: why, ip: who.ip });
  if (error?.code === "23505") return reply(200, { id, reported: true });
  if (error) throw error;
  await log(db, who, "report", id);
  return reply(201, { id, reported: true });
});
