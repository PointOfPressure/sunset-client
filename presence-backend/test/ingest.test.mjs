// Integration test of the ingest Worker under `wrangler dev` (local, nothing deployed):
//   node --test presence-backend/test/ingest.test.mjs
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const PORT = 8799;
const BASE = `http://127.0.0.1:${PORT}`;
const KEY = "test-developer-key-0123456789abcdef";
const sha = (s) => createHash("sha256").update(s).digest("hex");
const DEV_KEYS = JSON.stringify({ tester: sha(KEY) });
const ROOM = sha("sunset-presence:play.example.net:25565");
const ROOM2 = sha("sunset-presence:other.example.net:25565");
const USER = "11111111111111111111111111111111";
const USER2 = "22222222222222222222222222222222";
const DEV = "dddddddddddddddddddddddddddddddd";
let proc;
// Each run starts from empty local state (rooms, rate limits).
const state = mkdtempSync(join(tmpdir(), "sunset-presence-test-"));

const post = (path, body, headers = {}) =>
  fetch(BASE + path, { method: "POST", headers: { "content-type": "application/json", ...headers }, body: typeof body === "string" ? body : JSON.stringify(body) });
const beat = async (body, key) => {
  const r = await post("/v1/p/beat", body, key ? { authorization: `Bearer ${key}` } : {});
  return { status: r.status, body: r.status === 200 ? await r.json() : null };
};
const answer = (nonce, uuids) => uuids.map((u) => sha(`${nonce}:${u}`).slice(0, 16));

before(async () => {
  const busy = await fetch(BASE + "/health").then(() => true, () => false);
  if (busy) throw new Error(`port ${PORT} is already in use`);
  const dir = fileURLToPath(new URL("../ingest", import.meta.url));
  proc = spawn("npx", ["wrangler", "dev", "--persist-to", state, "--port", String(PORT), "--var", `DEV_KEYS:${DEV_KEYS}`], { cwd: dir, stdio: "ignore", detached: true });
  for (let i = 0; i < 120; i++) {
    try {
      if ((await fetch(BASE + "/health")).ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("wrangler dev did not start");
});
after(() => {
  // The whole group: npx, wrangler and its workerd.
  if (proc) process.kill(-proc.pid, "SIGKILL");
  rmSync(state, { recursive: true, force: true });
});

test("health and unknown routes", async () => {
  assert.equal((await fetch(BASE + "/health")).status, 200);
  assert.equal((await fetch(BASE + "/v1/a")).status, 404);
  assert.equal((await post("/v1/nope", {})).status, 404);
});

test("analytics accepts only the documented shape", async () => {
  assert.equal((await post("/v1/a", { e: "start", v: "1.3.0", os: "linux", f: 15 })).status, 204);
  assert.equal((await post("/v1/a", { e: "beat", v: "1.3.0", os: "windows", f: 0 })).status, 204);
  assert.equal((await post("/v1/a", { e: "beat", v: "1.3.0", os: "haiku", f: 0 })).status, 400);
  assert.equal((await post("/v1/a", { e: "beat", v: "1.3.0", os: "linux", f: 16 })).status, 400);
  assert.equal((await post("/v1/a", { e: "beat", v: "<script>", os: "linux", f: 0 })).status, 400);
  assert.equal((await post("/v1/a", "not json")).status, 400);
  assert.equal((await post("/v1/a", { e: "beat", v: "x".repeat(5000), os: "linux", f: 0 })).status, 400);
});

test("analytics is rate limited per address", async () => {
  let limited = false;
  for (let i = 0; i < 14 && !limited; i++) limited = (await post("/v1/a", { e: "beat", v: "1.3.0", os: "linux", f: 0 })).status === 429;
  assert.ok(limited);
});

test("presence: only users who saw the developer are shown to them", async () => {
  // Bad input and bad keys.
  assert.equal((await beat({ room: "abc", uuid: USER })).status, 400);
  assert.equal((await beat({ room: ROOM, uuid: "XYZ" })).status, 400);
  assert.equal((await beat({ room: ROOM, uuid: DEV }, "wrong-key-wrong-key-wrong-key-00")).status, 403);

  const u1 = await beat({ room: ROOM, uuid: USER });
  assert.equal(u1.status, 200);
  assert.equal(u1.body.beat, 120);
  assert.deepEqual(u1.body.challenges, []);
  assert.equal(u1.body.users, undefined);

  const d1 = await beat({ room: ROOM, uuid: DEV }, KEY);
  assert.equal(d1.body.beat, 60);
  assert.deepEqual(d1.body.users, []);

  // The user is challenged with the developer's hash; a wrong answer shows nothing.
  const u2 = await beat({ room: ROOM, uuid: USER, token: u1.body.token });
  assert.equal(u2.body.token, u1.body.token);
  assert.deepEqual(u2.body.challenges, answer(u2.body.nonce, [DEV]));
  await beat({ room: ROOM, uuid: USER, token: u1.body.token, seen: ["0123456789abcdef"] });
  assert.deepEqual((await beat({ room: ROOM, uuid: DEV, token: d1.body.token }, KEY)).body.users, []);

  // A second user claims the room but never saw the developer.
  const v = await beat({ room: ROOM, uuid: USER2, seen: [] });
  // The right answer shows the user.
  await beat({ room: ROOM, uuid: USER, token: u1.body.token, seen: u2.body.challenges });
  assert.deepEqual((await beat({ room: ROOM, uuid: DEV, token: d1.body.token }, KEY)).body.users, [USER]);

  // Another room's user answering the same hash shows nothing (no developer there to challenge).
  const other = await beat({ room: ROOM2, uuid: USER2 });
  assert.deepEqual(other.body.challenges, []);
  await beat({ room: ROOM2, uuid: USER2, token: other.body.token, seen: u2.body.challenges });

  // Leaving ends it at once.
  assert.equal((await post("/v1/p/leave", { room: ROOM, token: u1.body.token })).status, 204);
  assert.deepEqual((await beat({ room: ROOM, uuid: DEV, token: d1.body.token }, KEY)).body.users, []);
  // A token used for another player starts a new session.
  const w = await beat({ room: ROOM, uuid: USER, token: v.body.token });
  assert.notEqual(w.body.token, v.body.token);
});
