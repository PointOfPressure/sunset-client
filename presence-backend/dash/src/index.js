// Sunset Presence dashboard Worker: the page (static assets) and /api/stats, aggregates only, from Analytics Engine.
// Nothing here can see presence sessions, UUIDs or servers: they never reach Analytics Engine.

const CACHE_SECONDS = 300;
let certs = { at: 0, keys: [] };

const b64url = (s) => Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(s.length / 4) * 4, "=")), (c) => c.charCodeAt(0));
const b64json = (s) => JSON.parse(new TextDecoder().decode(b64url(s)));

// A Cloudflare Access token for this application, signed by the team's current keys and unexpired.
export async function accessOk(req, env, now = Date.now(), fetchCerts = fetch) {
  if (!env.ACCESS_TEAM || !env.ACCESS_AUD) return false;
  const jwt = req.headers.get("cf-access-jwt-assertion");
  const parts = jwt ? jwt.split(".") : [];
  if (parts.length !== 3) return false;
  try {
    const [h, p, sig] = parts;
    const header = b64json(h);
    const payload = b64json(p);
    const issuer = `https://${env.ACCESS_TEAM}.cloudflareaccess.com`;
    const aud = Array.isArray(payload.aud) ? payload.aud : [payload.aud];
    if (header.alg !== "RS256" || payload.iss !== issuer || !aud.includes(env.ACCESS_AUD)) return false;
    if (typeof payload.exp !== "number" || payload.exp * 1000 < now) return false;
    if (now - certs.at > 3_600_000 || !certs.keys.some((k) => k.kid === header.kid)) {
      const r = await fetchCerts(`${issuer}/cdn-cgi/access/certs`);
      certs = { at: now, keys: (await r.json()).keys || [] };
    }
    const jwk = certs.keys.find((k) => k.kid === header.kid);
    if (!jwk) return false;
    const key = await crypto.subtle.importKey("jwk", jwk, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["verify"]);
    return await crypto.subtle.verify("RSASSA-PKCS1-v1_5", key, b64url(sig), new TextEncoder().encode(`${h}.${p}`));
  } catch {
    return false;
  }
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
    if (!(await accessOk(req, env))) return new Response("Forbidden", { status: 403 });
    const url = new URL(req.url);
    if (url.pathname === "/api/stats") return stats(env, ctx);
    return env.ASSETS.fetch(req);
  },
};
