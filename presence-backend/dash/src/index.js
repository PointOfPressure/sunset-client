// Sunset Presence dashboard Worker: the page (static assets) and /api/stats, aggregates only, from Analytics Engine.
// Nothing here can see presence sessions, UUIDs or servers: they never reach Analytics Engine.

const CACHE_SECONDS = 300;
const enc = new TextEncoder();

// HTTP Basic auth against the DASH_PASSWORD secret (any username). Unset: closed to everyone.
export async function passwordOk(req, env) {
  if (!env.DASH_PASSWORD) return false;
  const h = req.headers.get("authorization") || "";
  if (!h.startsWith("Basic ")) return false;
  let given;
  try {
    const raw = atob(h.slice(6).trim());
    given = raw.slice(raw.indexOf(":") + 1);
  } catch {
    return false;
  }
  // Compare digests so the time taken doesn't depend on where the strings differ.
  const [a, b] = await Promise.all([given, env.DASH_PASSWORD].map(async (x) => new Uint8Array(await crypto.subtle.digest("SHA-256", enc.encode(x)))));
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

const SQL = {
  now: "SELECT SUM(_sample_interval) AS n FROM sunset_analytics WHERE timestamp > NOW() - INTERVAL '10' MINUTE",
  hour: "SELECT SUM(_sample_interval) AS n FROM sunset_analytics WHERE timestamp > NOW() - INTERVAL '1' HOUR",
  slots:
    "SELECT toStartOfInterval(timestamp, INTERVAL '10' MINUTE) AS t, SUM(_sample_interval) AS n FROM sunset_analytics " +
    "WHERE timestamp > NOW() - INTERVAL '90' DAY GROUP BY t ORDER BY t",
  days:
    "SELECT toStartOfInterval(timestamp, INTERVAL '1' DAY) AS d, SUM(_sample_interval * double2) AS day, " +
    "SUM(_sample_interval * double3) AS week, SUM(_sample_interval * double4) AS month, SUM(_sample_interval * double5) AS first " +
    "FROM sunset_analytics WHERE timestamp > NOW() - INTERVAL '90' DAY GROUP BY d ORDER BY d",
  versions:
    "SELECT blob2 AS k, SUM(_sample_interval * double2) AS n FROM sunset_analytics WHERE timestamp > NOW() - INTERVAL '7' DAY " +
    "GROUP BY k ORDER BY n DESC LIMIT 20",
  oses:
    "SELECT blob3 AS k, SUM(_sample_interval * double2) AS n FROM sunset_analytics WHERE timestamp > NOW() - INTERVAL '7' DAY " +
    "GROUP BY k ORDER BY n DESC LIMIT 10",
};

async function query(env, sql) {
  const r = await fetch(`https://api.cloudflare.com/client/v4/accounts/${env.ACCOUNT_ID}/analytics_engine/sql`, {
    method: "POST",
    headers: { authorization: `Bearer ${env.AE_TOKEN}` },
    body: sql,
  });
  if (!r.ok) throw new Error(`Analytics Engine ${r.status}`);
  return (await r.json()).data || [];
}

const num = (x) => Number(x) || 0;
const dayOf = (t) => String(t).slice(0, 10);
function isoWeek(day) {
  const d = new Date(`${day}T00:00:00Z`);
  const thursday = new Date(d);
  thursday.setUTCDate(d.getUTCDate() + 3 - ((d.getUTCDay() + 6) % 7));
  const jan4 = new Date(Date.UTC(thursday.getUTCFullYear(), 0, 4));
  const week = 1 + Math.round(((thursday - jan4) / 86_400_000 - 3 + ((jan4.getUTCDay() + 6) % 7)) / 7);
  return `${thursday.getUTCFullYear()}-W${String(week).padStart(2, "0")}`;
}

// The page's numbers from the query rows (exported for the test).
export function summarise({ now, hour, slots, days, versions, oses }) {
  const peakByDay = new Map();
  for (const s of slots) peakByDay.set(dayOf(s.t), Math.max(peakByDay.get(dayOf(s.t)) || 0, num(s.n)));
  const cutoff = (ms) => new Date(Date.now() - ms).toISOString();
  const peakSince = (ms) => slots.filter((s) => new Date(String(s.t).replace(" ", "T") + (String(s.t).endsWith("Z") ? "" : "Z")).toISOString() > cutoff(ms)).reduce((m, s) => Math.max(m, num(s.n)), 0);
  const sumBy = (key, field) => {
    const m = new Map();
    for (const d of days) m.set(key(dayOf(d.d)), (m.get(key(dayOf(d.d))) || 0) + num(d[field]));
    return [...m].map(([k, n]) => ({ k, n }));
  };
  const weeks = sumBy(isoWeek, "week");
  const months = sumBy((d) => d.slice(0, 7), "month");
  const daily = days.map((d) => ({ k: dayOf(d.d), n: num(d.day) }));
  return {
    activeNow: num(now[0]?.n),
    pingsLastHour: num(hour[0]?.n),
    peak24h: peakSince(86_400_000),
    peak7d: peakSince(7 * 86_400_000),
    peak90d: slots.reduce((m, s) => Math.max(m, num(s.n)), 0),
    today: daily.at(-1)?.n || 0,
    thisWeek: weeks.at(-1)?.n || 0,
    thisMonth: months.at(-1)?.n || 0,
    newInstalls30d: days.filter((d) => new Date(dayOf(d.d)) > new Date(Date.now() - 30 * 86_400_000)).reduce((s, d) => s + num(d.first), 0),
    daily,
    weekly: weeks,
    monthly: months,
    peakDaily: [...peakByDay].map(([k, n]) => ({ k, n })),
    versions: versions.map((v) => ({ k: v.k, n: num(v.n) })),
    oses: oses.map((v) => ({ k: v.k, n: num(v.n) })),
  };
}

async function stats(env, ctx) {
  const cache = caches.default;
  const key = new Request("https://sunset-presence-dash.cache/stats/v1");
  const hit = await cache.match(key);
  if (hit) return hit;
  const started = Date.now();
  let ingest = { ok: false, ms: null };
  try {
    const r = await fetch(`${env.INGEST_URL}/health`);
    ingest = { ok: r.ok, ms: Date.now() - started };
  } catch {}
  let body;
  try {
    const names = Object.keys(SQL);
    const rows = await Promise.all(names.map((n) => query(env, SQL[n])));
    body = { ...summarise(Object.fromEntries(names.map((n, i) => [n, rows[i]]))), health: { ingest, analytics: true }, at: new Date().toISOString() };
  } catch (e) {
    body = { health: { ingest, analytics: false, error: String(e.message || e) }, at: new Date().toISOString() };
  }
  const res = new Response(JSON.stringify(body), {
    headers: { "content-type": "application/json", "cache-control": `private, max-age=${CACHE_SECONDS}` },
  });
  if (body.health.analytics) ctx.waitUntil(cache.put(key, new Response(res.clone().body, { headers: { "cache-control": `max-age=${CACHE_SECONDS}` } })));
  return res;
}

export default {
  async fetch(req, env, ctx) {
    if (!(await passwordOk(req, env)))
      return new Response("Password required", { status: 401, headers: { "www-authenticate": 'Basic realm="Sunset Presence", charset="UTF-8"', "cache-control": "no-store" } });
    const url = new URL(req.url);
    if (url.pathname === "/api/stats") return stats(env, ctx);
    return env.ASSETS.fetch(req);
  },
};
