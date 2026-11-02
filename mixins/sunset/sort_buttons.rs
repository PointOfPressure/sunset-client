// Client Sort's buttons, in place of Horizon's one sort button per part of the screen: three small ones stacked down
// its right edge, Sort, Fill Stacks and Restock. A child of the tweaks so it reads Horizon's own list of the parts.
use super::Game;
use horizon_client::hud::{HudBatch, rgba};

const SIDE: f32 = 9.0;
// From the top of one button to the next.
const STEP: f32 = 10.0;
// Their left edge: 4 in from the screen's right one (176), as ClientSort's offset puts them.
const X: f32 = 172.0;

// Each button: its corner, which of the three it is, and the slots of its part. Horizon's parts, and your main
// inventory's always (on a server Horizon gives your own inventory none).
fn buttons(game: &Game<'_>) -> Vec<((f32, f32), usize, Vec<usize>)> {
    let mut parts = game.sort_buttons();
    if game.screen == super::Screen::Inventory
        && game.station.is_none()
        && !game.mount_active()
        && !parts.iter().any(|(_, slots)| slots.first() == Some(&9))
    {
        parts.push(((160.0, game.screen_height() - 95.0), (9..36).collect()));
    }
    parts
        .into_iter()
        .flat_map(|((_, y), slots)| (0..3).map(move |op| ((X, y + op as f32 * STEP), op, slots.clone())))
        .collect()
}

pub(crate) fn draw(game: &Game<'_>, batch: &mut HudBatch, ox: f32, oy: f32) {
    let atlas = &game.gui.atlas;
    let dark = rgba(55, 55, 55, 255);
    for ((x, y), op, _) in buttons(game) {
        let (x, y) = (ox + x, oy + y);
        let (cx, cy) = game.cursor;
        let hovered = cx >= x && cx < x + SIDE && cy >= y && cy < y + SIDE;
        batch.fill(atlas, x, y, SIDE, SIDE, dark);
        batch.fill(atlas, x + 1.0, y + 1.0, SIDE - 2.0, SIDE - 2.0, if hovered { rgba(220, 220, 220, 255) } else { rgba(160, 160, 160, 255) });
        match op {
            // Sort: three bars, shorter going down.
            0 => {
                for (i, w) in [5.0, 4.0, 3.0].into_iter().enumerate() {
                    batch.fill(atlas, x + 2.0, y + 2.0 + 2.0 * i as f32, w, 1.0, dark);
                }
            }
            // Fill Stacks: a plus.
            1 => {
                batch.fill(atlas, x + 2.0, y + 4.0, 5.0, 1.0, dark);
                batch.fill(atlas, x + 4.0, y + 2.0, 1.0, 5.0, dark);
            }
            // Restock: an arrow across.
            _ => {
                batch.fill(atlas, x + 2.0, y + 4.0, 5.0, 1.0, dark);
                batch.fill(atlas, x + 5.0, y + 3.0, 1.0, 3.0, dark);
                batch.fill(atlas, x + 4.0, y + 2.0, 1.0, 1.0, dark);
                batch.fill(atlas, x + 4.0, y + 6.0, 1.0, 1.0, dark);
            }
        }
    }
}

// A left press on one of them: its operation queued (true), else false.
pub(crate) fn press(game: &mut Game<'_>) -> bool {
    let Some(r) = game.renderer.as_ref() else {
        return false;
    };
    let batch = HudBatch::new([r.config.width, r.config.height]);
    let (ox, oy) = game.inventory_origin(&batch);
    let (cx, cy) = game.cursor;
    let hit = buttons(game).into_iter().find(|((x, y), ..)| {
        let (x, y) = (ox + x, oy + y);
        cx >= x && cx < x + SIDE && cy >= y && cy < y + SIDE
    });
    let Some((_, op, slots)) = hit else {
        return false;
    };
    crate::game::sunset_extras::operate(game, op, &slots);
    true
}
