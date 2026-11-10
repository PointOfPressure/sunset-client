# Fake Lag: design notes

Fake Lag delays what the client sends. Since the rebuild (`mixins/sunset/lag.rs`, `lag_line.rs`) it is one ordered
line. The modes decide only when the line holds and for how long.

## The line's rules

- **Order is kept.** Packets leave in the order the client wrote them, with no exceptions. A line holding packets
  queues new ones behind; it never lets them overtake (`Line::decide`). A self-check counts any reorder, and the Debug
  Report flags it as a bug.
- **No move is merged or dropped.** The old `merged()` turned held moves into one and kept their `ClientTickEnd`s.
  Servers that replay ticks then see empty ticks followed by one long move. Blink still compresses on purpose: that
  is Blink's own policy, and Fake Lag yields to it.
- **Whole client ticks.** While holding, the line releases up to the last due `ClientTickEnd`, so a server tick sees
  complete client ticks. While draining, it releases everything due.
- **Bounds.** At most 256 packets and 1000 ms are held. Past either bound, what is held goes out; it is never dropped.
- **Lifecycle:**

  | Event | Policy |
  |---|---|
  | Teleport, respawn, new dimension (same session) | Flush in order, then `Resetting` until the answering tick ends |
  | Disconnect, transfer, reconnect, proxy reconfiguration | Discard, counted by reason |
  | Module off, or a mode's condition ends | Drain on schedule (`Releasing`) |
  | Blink, a combat lease, GUI Move closing | Flush at once (`yield`) |
  | Panic | Flush in order, then idle |

  Held packets are bound to a `net::Link`: the `Session`'s `counters` cell, held weakly so its address can't be
  reused, plus an epoch counting connection boundaries. Horizon 66f3981 has no `Session::serial()`.
- **One delivery path.** Direct sends and released packets both reach `sunset_combat_wire::delivered`, so every
  observer hears of each packet once, when it goes on the wire. Packets a module sends from inside a release are
  queued behind it, so there is no re-entrant overtaking.

## Server-side checks

| Anticheat | Source | Relevant behaviour | Confidence |
|---|---|---|---|
| HorizonAC (Horizon's server) | `crates/horizon-anticheat` at 66f3981 | A client tick is the packets between two `ClientTickEnd`s. Timer: 300 ms tolerance, at most 1000 ms banked. Prediction places `Pong`s among ticks. Combat counts swings per tick. A second move before `ClientTickEnd` is a protocol error (hub). | Confirmed from source |
| GrimAC 2.0 | GitHub source (`checks/impl/timer`) | Timer: +50 ms per tick packet, floor = transaction clock − 120 ms drift. NegativeTimer (experimental): 1200 ms drift. TickTimer (1.21.2+): flags a move with no tick end since the last one, or more than one move in a tick. | Confirmed from source; `isTickPacket` and the PacketOrder subchecks not read |
| Intave | docs.intave.de (Timer) | Its Timer can detect a single extra movement packet; the docs note latency fluctuation can false-flag. | Official docs, no source |
| Vulcan, Matrix | Changelogs, third-party configs | Vulcan uses transactions for Blink and PingSpoof; Matrix's BadPackets covers Timer. | Secondhand only |
| AntiGamingChair | No public material found | — | Unknown |

What follows from these:

- Delaying moves, tick ends and `Pong`s together keeps the transaction clock and the tick count consistent. That is
  what network latency looks like.
- Holding moves while `Pong`s go on time (or the reverse) is what NegativeTimer and transaction-based Blink checks
  look for. The line never splits them.
- Bursts are the visible cost: Pulse, Knockback, Release on action, and a yield all send a backlog at once. Delay with
  Release on action off and the module turned off gradually is the smoothest stream.

## Limitations

- `KeepAlive` is answered by Horizon's network thread, not `Game::send`, so it is never delayed. A server comparing
  keep-alive latency with transaction latency sees the difference.
- Incoming delays (Backtrack, Knockback Delay) delay `Ping`s or reorder pushes against them by design. A
  transaction-based velocity check can see that.
- No configuration passes every anticheat. These notes describe compatibility with the checks read above, not
  undetectability.
