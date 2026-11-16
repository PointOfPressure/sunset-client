// Unit tests of the dashboard Worker's Access check and aggregation: node --test presence-backend/test/dash.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { accessOk, summarise } from "../dash/src/index.js";

const env = { ACCESS_TEAM: "sunset", ACCESS_AUD: "aud-1" };
const enc = (o) => Buffer.from(JSON.stringify(o)).toString("base64url");
const { publicKey, privateKey } = await crypto.subtle.generateKey(
  { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
const jwk = { ...(await crypto.subtle.exportKey("jwk", publicKey)), kid: "k1" };
const fetchCerts = async () => ({ json: async () => ({ keys: [jwk] }) });
async function token(payload, kid = "k1", key = privateKey) {
  const h = enc({ alg: "RS256", kid }), p = enc(payload);
  const sig = Buffer.from(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(`${h}.${p}`))).toString("base64url");
  return `${h}.${p}.${sig}`;
}
const req = (jwt) => new Request("https://dash.example/", { headers: jwt ? { "cf-access-jwt-assertion": jwt } : {} });
const now = Date.now();
const good = { iss: "https://sunset.cloudflareaccess.com", aud: ["aud-1"], exp: now / 1000 + 600 };

test("Access: only a valid token for this app passes", async () => {
  assert.equal(await accessOk(req(await token(good)), env, now, fetchCerts), true);
  assert.equal(await accessOk(req(), env, now, fetchCerts), false);
  assert.equal(await accessOk(req("a.b.c"), env, now, fetchCerts), false);
  assert.equal(await accessOk(req(await token({ ...good, aud: ["other"] })), env, now, fetchCerts), false);
  assert.equal(await accessOk(req(await token({ ...good, iss: "https://evil.cloudflareaccess.com" })), env, now, fetchCerts), false);
  assert.equal(await accessOk(req(await token({ ...good, exp: now / 1000 - 1 })), env, now, fetchCerts), false);
  assert.equal(await accessOk(req(await token(good, "k2")), env, now, fetchCerts), false);
  // Signed by another key.
  const other = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign"]);
  assert.equal(await accessOk(req(await token(good, "k1", other.privateKey)), env, now, fetchCerts), false);
  // Not configured: closed to everyone.
  assert.equal(await accessOk(req(await token(good)), { ACCESS_TEAM: "", ACCESS_AUD: "" }, now, fetchCerts), false);
});

test("summarise: actives from flags, peaks from 10-minute slots", () => {
  const t = (ms) => new Date(Date.now() - ms).toISOString().replace("T", " ").slice(0, 19);
  const s = summarise({
    now: [{ n: "7" }],
    hour: [{ n: "40" }],
    slots: [{ t: t(2 * 86_400_000), n: "50" }, { t: t(3_600_000), n: "9" }, { t: t(600_000), n: "7" }],
    days: [
      { d: "2026-11-09 00:00:00", day: "10", week: "10", month: "2", first: "1" },
      { d: "2026-11-15 00:00:00", day: "12", week: "3", month: "1", first: "0" },
      { d: "2026-11-16 00:00:00", day: "11", week: "4", month: "0", first: "2" },
    ],
    versions: [{ k: "1.3.0", n: "20" }],
    oses: [{ k: "linux", n: "5" }],
  });
  assert.equal(s.activeNow, 7);
  assert.equal(s.peak24h, 9);
  assert.equal(s.peak90d, 50);
  assert.equal(s.today, 11);
  // 2026-11-09 is in W46; 15th is W46 too (Sunday), 16th starts W47.
  assert.deepEqual(s.weekly, [{ k: "2026-W46", n: 13 }, { k: "2026-W47", n: 4 }]);
  assert.equal(s.thisWeek, 4);
  assert.equal(s.thisMonth, 3);
  assert.deepEqual(s.versions, [{ k: "1.3.0", n: 20 }]);
  const empty = summarise({ now: [], hour: [], slots: [], days: [], versions: [], oses: [] });
  assert.equal(empty.activeNow + empty.peak90d + empty.today + empty.thisMonth, 0);
});
