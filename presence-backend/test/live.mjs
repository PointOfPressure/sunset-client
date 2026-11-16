// Live check against the deployed ingest Worker: sessions survive the room object going idle between beats.
//   SUNSET_DEV_KEY=<a developer key> node presence-backend/test/live.mjs [base url] [idle seconds]
import { createHash, randomBytes } from "node:crypto";
const BASE = process.argv[2] || "https://sunset-presence.s4yerh4rris0n.workers.dev";
// Under the developer TTL (150 s), well over the ~10 s after which an idle object may be evicted.
const IDLE = Number(process.argv[3] || 130);
const KEY = process.env.SUNSET_DEV_KEY;
const sha = (s) => createHash("sha256").update(s).digest("hex");
const ROOM = sha(`sunset-presence:live-test-${randomBytes(8).toString("hex")}`);
const USER = randomBytes(16).toString("hex"), DEV = randomBytes(16).toString("hex");
const beat = async (body, key) => {
  const r = await fetch(`${BASE}/v1/p/beat`, { method: "POST", headers: key ? { authorization: `Bearer ${key}` } : {}, body: JSON.stringify(body) });
  if (r.status !== 200) throw new Error(`beat ${r.status}`);
  return r.json();
};
const check = (ok, what) => { console.log(`${ok ? "ok" : "FAIL"} ${what}`); if (!ok) process.exitCode = 1; };
const u = await beat({ room: ROOM, uuid: USER });
const d = await beat({ room: ROOM, uuid: DEV }, KEY);
const u2 = await beat({ room: ROOM, uuid: USER, token: u.token });
const hash = sha(`${u2.nonce}:${DEV}`).slice(0, 16);
check(u2.challenges.includes(hash), "user is challenged with the developer");
await beat({ room: ROOM, uuid: USER, token: u.token, seen: [hash] });
check((await beat({ room: ROOM, uuid: DEV, token: d.token }, KEY)).users.includes(USER), "developer sees the user at once");
await new Promise((r) => setTimeout(r, IDLE * 1000));
const d2 = await beat({ room: ROOM, uuid: DEV, token: d.token }, KEY);
check(d2.token === d.token, `developer's session kept after ${IDLE} s idle`);
check(d2.users.includes(USER), `user still shown after ${IDLE} s idle`);
await fetch(`${BASE}/v1/p/leave`, { method: "POST", body: JSON.stringify({ room: ROOM, token: u.token }) });
check((await beat({ room: ROOM, uuid: DEV, token: d.token }, KEY)).users.length === 0, "leave removes the user at once");
await fetch(`${BASE}/v1/p/leave`, { method: "POST", body: JSON.stringify({ room: ROOM, token: d.token }) });
