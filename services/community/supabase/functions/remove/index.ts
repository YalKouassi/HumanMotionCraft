// POST {"id"} from the clip's author: deletes its files and marks it removed. Ratings and reports
// stay with the row, so a removed clip can't come back with a clean slate.

import { admin, caller, Refusal, reply, retire, serve } from "../_shared/common.ts";

serve(async (req) => {
  if (req.method !== "POST") throw new Refusal(405, "use POST");
  const db = admin();
  const user = await caller(db, req, false);
  const { id } = await req.json().catch(() => ({}));
  if (typeof id !== "string") throw new Refusal(400, "id is required");

  const { data: clip } = await db.from("clips").select("author_id, status").eq("id", id).single();
  if (!clip || clip.author_id !== user.id) throw new Refusal(404, "no such clip of yours");
  if (clip.status === "removed") return reply(200, { id, status: "removed" });

  await retire(db, id, "removed", null);
  return reply(200, { id, status: "removed" });
});
