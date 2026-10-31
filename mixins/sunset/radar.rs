// Native-resolution player radar.
// Anubis (4ldenz) was a behavior reference.

use super::Game;
use super::sunset::{Category, Module, accent, own_id};
use super::sunset_radar_math::{
    Projection, circle_point, circle_segments, fitted_size, pixel_width,
};
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(
            "Radar",
            Category::Render,
            "Shows nearby players on a round radar. Potato: a plain square with a dot for each player.",
        )
        .pick("Mode", &["Round", "Potato"])
        .num("Range", 64.0, 8.0, 256.0, 1.0)
        .num("Size", 100.0, 80.0, 320.0, 1.0)
        .bool("Rotate", true)
        .colour("Players color", "White"),
    ]
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
    let mut previous = [circle_point(center, inner, 0, segments), circle_point(center, outer, 0, segments)];
    let color = rgba(color);
    for i in 0..segments {
        let next = [circle_point(center, inner, i + 1, segments), circle_point(center, outer, i + 1, segments)];
        let corners = [
            previous[0], previous[1], next[1], next[0],
        ];
        let start = batch.vertices.len();
        batch.fill_quad(&game.gui.atlas, corners, color);
        if feather {
            // fill_quad's two triangles use corners 0,1,2,0,2,3.
            for (vertex, corner) in batch.vertices[start..].iter_mut().zip([0, 1, 2, 0, 2, 3]) {
                if corner == 1 || corner == 2 {
                    vertex.color &= 0x00FF_FFFF;
                }
            }
        }
        previous = next;
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

pub(super) fn draw(
    game: &Game<'_>,
    batch: &mut HudBatch,
    corners: &mut Corners,
    modules: &[Module],
) {
    let Some(module) = modules.iter().find(|m| m.name == "Radar" && m.on) else {
        return;
    };
    let Some(player) = game.player.as_ref() else {
        return;
    };
    if game.title.is_some() || !game.native_corner_shown(Corner::BottomLeft) {
        return;
    }
    let Some(size) = fitted_size(module.n("Size") as f32, batch.width, batch.height) else {
        return;
    };
    let (x, y) = corners.place(Corner::BottomLeft, size, size + 12.0);
    // A preceding native panel can consume the corner. Avoid drawing offscreen.
    if x < 0.0 || y < 2.0 || x + size > batch.width {
        return;
    }
    let center = [x + size / 2.0, y + size / 2.0];
    let radius = size / 2.0 - 1.0;
    let reach = radius - 17.0;
    let pixel = pixel_width(batch.scale);
    let rotate = module.b("Rotate");
    let yaw = f64::from(game.yaw);
    let range = module.n("Range");
    let Some(projection) = Projection::new(yaw, rotate, range, reach - 3.0) else {
        return;
    };
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
            projection.project(dx, dz)
        });
    if module.p("Mode") == "Potato" {
        game.native_fill(batch, [x, y, size, size], 0xD016_1B24);
        for [ox, oy] in dots {
            game.native_fill(batch, [center[0] + ox - 2.0, center[1] + oy - 2.0, 4.0, 4.0], color);
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
        let [ox, oy] = projection.offset(dx, dz);
        let at = [
            center[0] + ox as f32 * (radius - 8.0) - game.native_text_width(label) / 2.0,
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
    let caption = format!("Radar  {range:.0}m  {count} players");
    game.native_text(batch, &caption, [x, y + size + 2.0], 0xC6CFD9, true);
}
