// Sunset Presence ingest Worker. Two separate jobs that share nothing but this file:
//
// POST /v1/a        anonymous analytics: one data point in Analytics Engine per ping. No ID of any kind: a ping says
//                   the Sunset version, the OS family and whether it is this install's first ping of the day / ISO
//                   week / month / ever (the client keeps those dates locally). Counts come from summing the flags.
// POST /v1/p/beat   presence (opt-in): a session in the room of one server (the client sends a SHA-256 of the server's
//                   address, never the address). Held in the room's Durable Object's memory only, never stored.
// POST /v1/p/leave  ends a presence session at once.
//
// Developers are recognised by a personal key (Authorization: Bearer) checked against the DEV_KEYS secret, a JSON
// object { "<name>": "<sha256 hex of the key>" }. Revoke by removing the entry (wrangler secret put DEV_KEYS).
// Nothing here logs request bodies, IPs or UUIDs. The IP is only the rate limiter's key.

import { DurableObject } from "cloudflare:workers";

const MAX_BODY = 4096;
const HEX = (n) => new RegExp(`^[0-9a-f]{${n}}$`);
const ROOM_RE = HEX(64);
const UUID_RE = HEX(32);
const TOKEN_RE = HEX(32);
const SEEN_RE = HEX(16);
const VERSION_RE = /^[0-9A-Za-z.+\- ]{1,32}$/;
const OSES = new Set(["linux", "windows", "macos", "other"]);

const empty = (status, headers) => new Response(null, { status, headers });
const json = (body, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });

async function sha256hex(text) {
  const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function randomHex(bytes) {
  return [...crypto.getRandomValues(new Uint8Array(bytes))].map((b) => b.toString(16).padStart(2, "0")).join("");
}

// The developer a key belongs to, or null. A key's hash is compared, never the key.
export async function devName(env, auth) {
  if (!auth) return null;
  const m = /^Bearer ([A-Za-z0-9_\-]{24,128})$/.exec(auth);
  if (!m) return undefined;
  let keys;
  try {
    keys = JSON.parse(env.DEV_KEYS || "{}");
  } catch {
    return undefined;
  }
  const hash = await sha256hex(m[1]);
  for (const [name, h] of Object.entries(keys)) if (h === hash) return name;
  return undefined;
}

async function readBody(req) {
  const len = Number(req.headers.get("content-length") || 0);
  if (len > MAX_BODY) return null;
  const text = await req.text();
  if (text.length > MAX_BODY) return null;
  try {
    const body = JSON.parse(text);
    return body && typeof body === "object" && !Array.isArray(body) ? body : null;
  } catch {
    return null;
  }
}

export function analyticsPoint(body) {
  const { e, v, os, f } = body;
  if ((e !== "start" && e !== "beat") || typeof v !== "string" || !VERSION_RE.test(v) || !OSES.has(os)) return null;
  if (!Number.isInteger(f) || f < 0 || f > 15) return null;
  return { blobs: [e, v, os], doubles: [1, f & 1, (f >> 1) & 1, (f >> 2) & 1, (f >> 3) & 1], indexes: ["sunset"] };
}

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    if (url.pathname === "/health") return json({ ok: true });
    if (req.method !== "POST") return empty(404);
    const ip = req.headers.get("cf-connecting-ip") || "unknown";
    const analytics = url.pathname === "/v1/a";
    const presence = url.pathname === "/v1/p/beat" || url.pathname === "/v1/p/leave";
    if (!analytics && !presence) return empty(404);
    const limiter = analytics ? env.LIMIT_A : env.LIMIT_P;
    if (limiter && !(await limiter.limit({ key: ip })).success) return empty(429, { "retry-after": "60" });
    const body = await readBody(req);
    if (!body) return empty(400);

    if (analytics) {
      const point = analyticsPoint(body);
      if (!point) return empty(400);
      // Analytics Engine not bound (not yet enabled on the account): the client backs off.
      if (!env.AE) return empty(503);
      env.AE.writeDataPoint(point);
      return empty(204);
    }

    const { room, uuid, token, seen } = body;
    if (typeof room !== "string" || !ROOM_RE.test(room)) return empty(400);
    if (token !== undefined && (typeof token !== "string" || !TOKEN_RE.test(token))) return empty(400);
    const stub = env.ROOMS.get(env.ROOMS.idFromName(room));
    if (url.pathname === "/v1/p/leave") {
      if (token) await stub.leave(token);
      return empty(204);
    }
    if (typeof uuid !== "string" || !UUID_RE.test(uuid)) return empty(400);
    if (seen !== undefined && (!Array.isArray(seen) || seen.length > 16 || !seen.every((s) => typeof s === "string" && SEEN_RE.test(s))))
      return empty(400);
    const dev = await devName(env, req.headers.get("authorization"));
    if (dev === undefined) return empty(403);
    const out = await stub.beat({ uuid, token: token ?? null, seen: seen ?? null, dev });
    return out ? json(out) : empty(503);
  },
};

// One server's presence sessions, in the object's own SQLite storage (an idle object can be evicted from memory between
// beats). A session row is deleted when it leaves or expires (an alarm clears expired ones).
const USER_TTL = 300_000;
const DEV_TTL = 150_000;
const USER_BEAT = 120;
const DEV_BEAT = 60;
const MAX_SESSIONS = 500;

export class Room extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec("CREATE TABLE IF NOT EXISTS s (token TEXT PRIMARY KEY, uuid TEXT NOT NULL, dev TEXT, expires INTEGER NOT NULL, sees TEXT NOT NULL DEFAULT '')");
    this.sql.exec("CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL)");
    const row = this.sql.exec("SELECT v FROM meta WHERE k = 'nonce'").toArray()[0];
    this.nonce = row ? row.v : randomHex(16);
    if (!row) this.sql.exec("INSERT INTO meta (k, v) VALUES ('nonce', ?)", this.nonce);
  }

  async beat({ uuid, token, seen, dev }) {
    const now = Date.now();
    this.sql.exec("DELETE FROM s WHERE expires <= ?", now);
    let s = token ? this.sql.exec("SELECT * FROM s WHERE token = ?", token).toArray()[0] : undefined;
    if (!s || s.uuid !== uuid || (s.dev ?? null) !== dev) {
      // One session per player in a room.
      this.sql.exec("DELETE FROM s WHERE token = ? OR uuid = ?", token ?? "", uuid);
      if (this.sql.exec("SELECT COUNT(*) AS n FROM s").one().n >= MAX_SESSIONS) return null;
      token = randomHex(16);
      s = { token, uuid, dev, sees: "" };
      this.sql.exec("INSERT INTO s (token, uuid, dev, expires) VALUES (?, ?, ?, 0)", token, uuid, dev);
    }
    const devs = this.sql.exec("SELECT uuid FROM s WHERE dev IS NOT NULL AND uuid != ?", uuid).toArray().map((r) => r.uuid);
    const challenges = [];
    for (const d of devs) challenges.push({ uuid: d, hash: (await sha256hex(`${this.nonce}:${d}`)).slice(0, 16) });
    // The answer to the challenges: which of the developers here this player's own tab list shows.
    let sees = s.sees;
    if (seen) {
      const set = new Set(seen);
      sees = challenges.filter((c) => set.has(c.hash)).map((c) => c.uuid).join(",");
    }
    // A row is written only when its answer changed or half its time is gone (row writes are what a room costs).
    const ttl = dev ? DEV_TTL : USER_TTL;
    const expires = now + ttl;
    if (sees !== s.sees || !(s.expires > now + ttl / 2)) this.sql.exec("UPDATE s SET expires = ?, sees = ? WHERE token = ?", expires, sees, token);
    if ((await this.ctx.storage.getAlarm()) === null) await this.ctx.storage.setAlarm(expires + 1000);
    const out = { token, beat: dev ? DEV_BEAT : USER_BEAT, nonce: this.nonce, challenges: challenges.map((c) => c.hash).sort() };
    if (dev) {
      // Only players whose own client saw this developer in its tab list on this server.
      out.users = this.sql
        .exec("SELECT uuid, sees FROM s WHERE token != ?", token)
        .toArray()
        .filter((r) => r.sees.split(",").includes(uuid))
        .map((r) => r.uuid);
    }
    return out;
  }

  async leave(token) {
    this.sql.exec("DELETE FROM s WHERE token = ?", token);
  }

  async alarm() {
    const now = Date.now();
    this.sql.exec("DELETE FROM s WHERE expires <= ?", now);
    // Empty: only the room's nonce is left (no player data); no alarm until the next session.
    const next = this.sql.exec("SELECT MIN(expires) AS t FROM s").one().t;
    if (next !== null) await this.ctx.storage.setAlarm(next + 1000);
  }
}
