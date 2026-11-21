// Unit tests of the dashboard Worker's password check and aggregation: node --test presence-backend/test/dash.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { passwordOk, summarise } from "../dash/src/index.js";

const req = (auth) => new Request("https://dash.example/", { headers: auth ? { authorization: auth } : {} });
const basic = (user, pass) => `Basic ${btoa(`${user}:${pass}`)}`;

test("password: only the configured password passes", async () => {
  const env = { DASH_PASSWORD: "hunter2:x" };
  assert.equal(await passwordOk(req(basic("", "hunter2:x")), env), true);
  assert.equal(await passwordOk(req(basic("anyone", "hunter2:x")), env), true);
  assert.equal(await passwordOk(req(basic("", "hunter2")), env), false);
  assert.equal(await passwordOk(req(basic("hunter2:x", "")), env), false);
  assert.equal(await passwordOk(req(), env), false);
  assert.equal(await passwordOk(req("Basic !!!"), env), false);
  assert.equal(await passwordOk(req("Bearer hunter2:x"), env), false);
  // Not configured: closed to everyone.
  assert.equal(await passwordOk(req(basic("", "")), {}), false);
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
