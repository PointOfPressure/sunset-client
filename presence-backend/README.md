# Sunset Presence backend

Two Cloudflare Workers on the maintainer's account. The client side is `client/mixins/sunset/presence.rs`; the privacy
summary is in the main README.

| Piece | Why |
|---|---|
| `ingest/` Worker `sunset-presence` (public) | Takes analytics pings and presence beats; validation, rate limits, developer-key check. |
| Workers Analytics Engine `sunset_analytics` | Aggregate counts and trends. No database to run; 90-day retention. |
| Durable Object `Room` (one per server) | Presence sessions need one consistent place per server to match players and developers. Sessions are rows in the object's own SQLite storage, deleted on leave or expiry (2.5 min developers, 5 min players). |
| `dash/` Worker `sunset-presence-dash` + Static Assets | The private dashboard page and its `/api/stats` (Analytics Engine SQL, cached 5 min). |
| Password | HTTP Basic auth on every dashboard request, checked by the Worker against the `DASH_PASSWORD` secret (any username). Unset, it refuses everything. |

No D1, no KV: nothing needs them.

## What is sent

- `POST /v1/a` `{e: "start"|"beat", v: "<version>", os: "linux"|"windows"|"macos"|"other", f: 0..15}`, every 10 minutes
  in a world. `f` bits: first ping today (1), this ISO week (2), this month (4), ever (8), from dates kept in a local
  file. No ID. Stored as one Analytics Engine data point.
- `POST /v1/p/beat` `{room, uuid, token?, seen?}` with presence on: `room` = SHA-256 of `sunset-presence:<address>`
  (lower case, default port dropped), `uuid` = your player UUID, `seen` = the challenge hashes you matched. Developers
  add `Authorization: Bearer <key>`. Every 2 min (developers 1 min).
- `POST /v1/p/leave` `{room, token}` when presence is turned off or you leave the server.

## Same-server check

A developer is shown a player only if (1) both have a session in the same room, (2) the player's own client found the
developer's UUID in its own tab list (it answers a challenge: SHA-256 of the room's nonce and the developer's UUID,
cut to 16 hex), and (3) the developer's client finds the player in its own tab list before drawing the icon.

It guarantees: a developer can't see players on a server they aren't on (a player's client has to see them there);
non-developers get nothing about players; a refused or revoked key gets nothing. It can't guarantee: that the address
is the "same server" behind a proxy network (players on different backend servers of one network, sharing a tab list,
match); that a modified client tells the truth about its own tab list (it can only lie about itself). Servers that
hide the tab list never match (fails closed). A non-developer who claims a room and knows a developer's UUID can test
whether that developer is on that server right now: the trade-off for never sending bystanders' UUIDs anywhere.
The address hash and UUIDs are not anonymous (known addresses and UUIDs can be hashed and compared); they are kept
only for the session's lifetime and never reach Analytics Engine, logs or the dashboard.

## Deploy

```sh
# Once: enable Analytics Engine at https://dash.cloudflare.com/?to=/:account/workers/analytics-engine
cd presence-backend/ingest && npx wrangler deploy
cd ../dash && npx wrangler deploy
```

Dashboard secrets (once): `npx wrangler secret put AE_TOKEN` (an API token with *Account Analytics: Read*) and
`npx wrangler secret put ACCOUNT_ID`, and the page password: `npx wrangler secret put DASH_PASSWORD`.

## Developer keys

```sh
key=$(python3 -c "import secrets;print(secrets.token_urlsafe(32))")      # give this to the developer, privately
printf '%s' "$key" | sha256sum                                           # its hash goes in DEV_KEYS
printf '{"sayer":"<hash>","other":"<hash>"}' | npx wrangler secret put DEV_KEYS   # in ingest/
```

`DEV_KEYS` holds hashes only. Revoke a developer by putting the list back without them (takes effect within seconds).
The developer saves the key as `presence-dev-key` in Sunset's data folder and turns on Share in-game presence.

## Tests

```sh
node --test presence-backend/test/*.test.mjs        # ingest under `wrangler dev` (local), dashboard units
SUNSET_DEV_KEY=<key> node presence-backend/test/live.mjs   # against the deployed Worker, ~3 min
```

## Cost (Cloudflare prices as of November 2026)

Assumed per install: 1 hour played a day, 20% with presence on, on a server 80% of that time. That is about 370 Worker
requests, 190 Durable Object requests (about 100 SQLite rows written) and 180 Analytics Engine points a month.

| Installs | Worker requests / month | DO requests | AE points | Plan | Cost / month |
|---|---|---|---|---|---|
| 100 | 37k | 19k | 18k | Free | $0 |
| 1k | 370k (12k/day) | 190k | 180k | Free | $0 |
| 10k | 3.7M (123k/day, over Free's 100k/day) | 1.9M | 1.8M | Paid | about $5.15 |
| 100k | 37M | 19M | 18M | Paid | about $18 ($5 + $8.10 requests + $2.70 DO + $2 AE) |

Paid prices used: Workers $5/month with 10M requests (+$0.30/M) and 30M CPU ms; Durable Objects 1M requests (+$0.15/M),
400k GB-s, 50M rows written (+$1/M); Analytics Engine 10M points (+$0.25/M; not billed yet) and 1M queries. A beat
writes its session row only when the answer changed or half its time is gone, so rows written stay under the included
50M even at 100k installs with everyone on presence. The dashboard is cached for 5 minutes (6 queries per refresh) and
costs nothing measurable. Everyone opting in to presence would add about $34 at 100k installs (requests and DO
requests).
