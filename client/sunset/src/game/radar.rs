// Native-resolution player radar.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset::{Category, Module, accent, own_id};
use super::sunset_radar_math::{
    Book, Fix, Op, circle_point, circle_segments, compass, fitted_size, offset, pixel_width, project,
};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(
            "Radar",
            Category::Render,
            "Shows nearby players.",
        )
        .pick("Mode", &["Round", "Potato"])
        .num("Range", 64.0, 8.0, 256.0, 1.0)
        .num("Size", 100.0, 80.0, 320.0, 1.0)
        .bool("Rotate", true)
        // Players the server's locator bar gives but you can't see: hollow dots in range, marks on the rim past it
        // (a bearing only past 332 blocks). Carved pumpkins and mob heads hide players from it.
        .bool("Locator", true)
        .colour("Players color", "White")
        // Its place: the bottom left corner, or where the chat or the menu dragged it.
        .pick("Radar position", &["Default", "Custom"])
        .num("Radar x", 0.0, 0.0, 1.0, 0.001)
        .num("Radar y", 1.0, 0.0, 1.0, 0.001),
    ]
}

// ---- Locator radar: the waypoints the server sends for you, from the packets as they come (the game keeps its own
// list private).
struct Locator {
    book: Book,
    epoch: Instant,
    // No alerts until then: the waypoints a join or a new world brings all at once.
    quiet_until: Instant,
}

static LOCATOR: Mutex<Option<Locator>> = Mutex::new(None);

pub(crate) fn locator_seen(event: &horizon_client::net::Event) {
    use horizon_client::net::Event;
    use horizon_protocol::packets::play::{Clientbound as C, WaypointId, WaypointTarget, waypoint_operation};
    let packet = match event {
        Event::Joined { .. } => None,
        Event::Packet(p) if matches!(**p, C::Login(_) | C::TrackedWaypoint(_)) => Some(&**p),
        _ => return,
    };
    let now = Instant::now();
    let mut guard = LOCATOR.lock().unwrap_or_else(|e| e.into_inner());
    let l = guard.get_or_insert_with(|| Locator { book: Book::default(), epoch: now, quiet_until: now });
    let Some(C::TrackedWaypoint(p)) = packet else {
        l.book.clear();
        l.quiet_until = now + Duration::from_secs(3);
        return;
    };
    let WaypointId::Uuid(uuid) = p.id else {
        return;
    };
    let op = match p.operation {
        waypoint_operation::TRACK => Op::Track,
        waypoint_operation::UNTRACK => Op::Untrack,
        waypoint_operation::UPDATE => Op::Update,
        _ => return,
    };
    let fix = match p.target {
        WaypointTarget::Block(at) => Some(Fix::Block(at)),
        WaypointTarget::Chunk(at) => Some(Fix::Chunk(at)),
        WaypointTarget::Azimuth(angle) => Some(Fix::Bearing(angle)),
        WaypointTarget::Empty => None,
    };
    let ms = u64::try_from(now.duration_since(l.epoch).as_millis()).unwrap_or(u64::MAX);
    l.book.apply(op, uuid, fix, ms, now >= l.quiet_until);
}

// The players the locator bar gives that are not tracked here as entities (and not you).
fn located(game: &Game<'_>) -> Vec<Fix> {
    let own = super::remote::sunset_radar_players::own_uuid(game);
    let players = LOCATOR.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|l| l.book.players.clone()).unwrap_or_default();
    players
        .into_iter()
        .filter(|(uuid, _)| Some(*uuid) != own && !super::remote::sunset_radar_players::tracked(game, *uuid))
        .map(|(_, fix)| fix)
        .collect()
}

// Notifier's Locator: the players the server brought closer since the last call, as lines to show.
pub(super) fn locator_alerts(game: &Game<'_>) -> Vec<String> {
    let closer = LOCATOR.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(|l| std::mem::take(&mut l.book.closer)).unwrap_or_default();
    let Some(player) = game.player.as_ref() else {
        return Vec::new();
    };
    let own = super::remote::sunset_radar_players::own_uuid(game);
    let (x, z) = (player.position.x, player.position.z);
    closer
        .into_iter()
        .filter(|(uuid, ..)| Some(*uuid) != own)
        .map(|(uuid, _, fix)| {
            let name = super::remote::sunset_radar_players::name(game, uuid)
                .unwrap_or_else(|| format!("{:08x}", (uuid >> 96) as u32));
            let distance = fix.offset_from(x, z).map_or(0.0, |[dx, dz]| dx.hypot(dz));
            let towards = fix.direction_from(x, z).map_or("", compass);
            if fix.rank() >= 3 {
                format!("{name} is in view range ({distance:.0} m {towards})")
            } else {
                format!("{name} is within 332 blocks (about {distance:.0} m {towards})")
            }
        })
        .collect()
}

fn rgba(argb: u32) -> u32 {
    let [a, r, g, b] = argb.to_be_bytes();
    horizon_client::hud::rgba(r, g, b, a)
}

// Solid-atlas vector quads enter the existing HUD batch directly. The only
// edge fade is one physical pixel of coverage, not a blur or low-res texture.
fn band(
    game: &Game<'_>,
    batch: &mut HudBatch,
    center: [f32; 2],
    inner: f32,
    outer: f32,
    segments: usize,
    color: u32,
    feather: bool,
) {
    for i in 0..segments {
        let corners = [
            circle_point(center, inner, i, segments),
            circle_point(center, outer, i, segments),
            circle_point(center, outer, i + 1, segments),
            circle_point(center, inner, i + 1, segments),
        ];
        let start = batch.vertices.len();
        batch.fill_quad(&game.gui.atlas, corners, rgba(color));
        if feather {
            // fill_quad's two triangles use corners 0,1,2,0,2,3.
            for (vertex, corner) in batch.vertices[start..].iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                if corner == 1 || corner == 2 {
                    vertex.color &= 0x00FF_FFFF;
                }
            }
        }
    }
}

fn disc(game: &Game<'_>, batch: &mut HudBatch, center: [f32; 2], radius: f32, color: u32) {
    let pixel = pixel_width(batch.scale);
    let inner = (radius - pixel * 0.5).max(0.0);
    let segments = circle_segments(radius, batch.scale);
    // Two adjacent fan triangles share one quad; no temporary vertex vector.
    for i in (0..segments).step_by(2) {
        batch.fill_quad(
            &game.gui.atlas,
            [
                center,
                circle_point(center, inner, i, segments),
                circle_point(center, inner, i + 1, segments),
                circle_point(center, inner, i + 2, segments),
            ],
            rgba(color),
        );
    }
    band(
        game,
        batch,
        center,
        inner,
        radius + pixel * 0.5,
        segments,
        color,
        true,
    );
}

// Where the radar goes this frame (x, y, its size), worked out before the HUD's layout editor looks so it can be taken:
// in the bottom left corner, or where it was dragged (Custom). None: not drawn this frame.
pub(super) fn place(game: &Game<'_>, w: f32, h: f32, corners: &mut Corners, modules: &[Module]) -> Option<[f32; 3]> {
    let module = modules.iter().find(|m| m.name == "Radar" && m.on)?;
    game.player.as_ref()?;
    let custom = module.p("Radar position") == "Custom";
    if game.title.is_some() || (!custom && !game.native_corner_shown(Corner::BottomLeft)) {
        return None;
    }
    let size = fitted_size(module.n("Size") as f32, w, h)?;
    let (x, y) = if custom {
        // Its label under it (12 more) belongs to its place.
        let [x, y] = super::sunset_hud_place::custom([module.n("Radar x"), module.n("Radar y")], [size, size + 12.0], [w, h], false);
        (x, y)
    } else {
        let (x, y) = corners.place(Corner::BottomLeft, size, size + 12.0);
        // A preceding native panel can consume the corner. Avoid drawing offscreen.
        if x < 0.0 || y < 2.0 || x + size > w {
            return None;
        }
        (x, y)
    };
    super::sunset_hud_bars::place("Radar", [x, y, size, size + 12.0], w, h);
    Some([x, y, size])
}

// The radar at the place `place` gave it.
pub(super) fn draw(game: &Game<'_>, batch: &mut HudBatch, [x, y, size]: [f32; 3], modules: &[Module]) {
    let Some(module) = modules.iter().find(|m| m.name == "Radar" && m.on) else {
        return;
    };
    let Some(player) = game.player.as_ref() else {
        return;
    };
    let center = [x + size / 2.0, y + size / 2.0];
    let radius = size / 2.0 - 1.0;
    let reach = radius - 17.0;
    let pixel = pixel_width(batch.scale);
    let rotate = module.b("Rotate");
    let yaw = f64::from(game.yaw);
    let range = module.n("Range");
    let alpha = game.clock.camera_fraction();
    let lerp = |previous: f64, current: f64| previous + alpha * (current - previous);
    let local = [
        lerp(game.previous_position.x, player.position.x),
        lerp(game.previous_position.z, player.position.z),
    ];
    let own = own_id(game);
    let color = module.c("Players color");
    // The players on it: where on the radar, from its middle.
    let dots = game
        .mobs
        .iter()
        .filter(|other| {
            other.kind.id == "minecraft:player"
                && Some(other.id) != own
                && other.alive()
                && !other.removed
                && !other.is_invisible()
                && !super::remote::sunset_radar_players::spectator(game, other.id)
        })
        .filter_map(|other| {
            let dx = lerp(other.previous.x, other.position.x) - local[0];
            let dz = lerp(other.previous.z, other.position.z) - local[1];
            project(dx, dz, yaw, rotate, range, reach - 3.0)
        });
    // Locator players: inside the range where the server puts them (a chunk's middle when it gives only the chunk),
    // else a mark on the rim towards them. True: a chunk or a bearing, not an exact block.
    let fixes = if module.b("Locator") { located(game) } else { Vec::new() };
    let marks: Vec<([f32; 2], bool, bool)> = fixes
        .iter()
        .filter_map(|fix| {
            let rough = fix.rank() < 3;
            if let Some([dx, dz]) = fix.offset_from(local[0], local[1])
                && let Some(at) = project(dx, dz, yaw, rotate, range, reach - 3.0)
            {
                return Some((at, false, rough));
            }
            let [dx, dz] = fix.direction_from(local[0], local[1])?;
            let [ox, oy] = offset(dx, dz, yaw, rotate);
            Some(([ox as f32 * (reach - 1.0), oy as f32 * (reach - 1.0)], true, rough))
        })
        .collect();
    let dim = (color & 0x00FF_FFFF) | 0x8000_0000;
    if module.p("Mode") == "Potato" {
        game.native_fill(batch, [x, y, size, size], 0xD016_1B24);
        for [ox, oy] in dots {
            game.native_fill(batch, [center[0] + ox - 2.0, center[1] + oy - 2.0, 4.0, 4.0], color);
        }
        for ([ox, oy], _, rough) in &marks {
            let (px, py) = (center[0] + ox, center[1] + oy);
            game.native_fill(batch, [px - 2.0, py - 2.0, 4.0, 4.0], if *rough { dim } else { color });
            game.native_fill(batch, [px - 1.0, py - 1.0, 2.0, 2.0], 0xFF16_1B24);
        }
        game.native_fill(batch, [center[0] - 2.0, center[1] - 2.0, 4.0, 4.0], 0xFF00_0000 | accent());
        return;
    }
    // Keep the background fringe under the solid rim, so only one layer
    // contributes coverage at the outer device-pixel edge.
    disc(game, batch, center, radius - pixel, 0xD016_1B24);
    band(
        game,
        batch,
        center,
        radius - pixel * 1.5,
        radius - pixel * 0.5,
        circle_segments(radius, batch.scale),
        0xFF58_6371,
        false,
    );
    band(
        game,
        batch,
        center,
        radius - pixel * 0.5,
        radius + pixel * 0.5,
        circle_segments(radius, batch.scale),
        0xFF58_6371,
        true,
    );
    game.native_fill(
        batch,
        [center[0] - reach, center[1], reach * 2.0, pixel],
        0xFF35_3E49,
    );
    game.native_fill(
        batch,
        [center[0], center[1] - reach, pixel, reach * 2.0],
        0xFF35_3E49,
    );

    for (label, dx, dz) in [
        ("N", 0.0, -1.0),
        ("E", 1.0, 0.0),
        ("S", 0.0, 1.0),
        ("W", -1.0, 0.0),
    ] {
        let [ox, oy] = offset(dx, dz, yaw, rotate);
        let at = [
            center[0] + ox as f32 * (radius - 8.0) - game.sunset_text_width(label) / 2.0,
            center[1] + oy as f32 * (radius - 8.0) - 4.0,
        ];
        game.native_text(
            batch,
            label,
            at,
            if label == "N" { accent() } else { 0xC6CFD9 },
            true,
        );
    }

    let mut count = 0;
    for [ox, oy] in dots {
        let dot = [center[0] + ox, center[1] + oy];
        disc(game, batch, dot, 3.2, 0xFF09_0D13);
        disc(game, batch, dot, 2.3, color);
        count += 1;
    }
    for ([ox, oy], rim, rough) in &marks {
        let at = [center[0] + ox, center[1] + oy];
        let ring = if *rough { dim } else { color };
        // Hollow: seen by the locator bar, not as an entity; on the rim a smaller mark.
        let r = if *rim { 2.4 } else { 3.0 };
        disc(game, batch, at, r + 0.9, 0xFF09_0D13);
        disc(game, batch, at, r, ring);
        disc(game, batch, at, r - 1.2, 0xFF16_1B24);
    }
    disc(game, batch, center, 3.0, 0xFF00_0000 | accent());
    let [fx, fy] = if rotate {
        [0.0, -1.0]
    } else {
        let (sin, cos) = yaw.to_radians().sin_cos();
        [-sin as f32, cos as f32]
    };
    // Short heading marker distinguishes the local center from other players.
    for i in 4..8 {
        game.native_fill(
            batch,
            [
                center[0] + fx * i as f32 - 0.6,
                center[1] + fy * i as f32 - 0.6,
                1.2,
                1.2,
            ],
            0xFF00_0000 | accent(),
        );
    }
    let caption = if marks.is_empty() {
        format!("Radar  {range:.0}m  {count} players")
    } else {
        format!("Radar  {range:.0}m  {count} players  +{} located", marks.len())
    };
    game.native_text(batch, &caption, [x, y + size + 2.0], 0xC6CFD9, true);
}
