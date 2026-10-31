// Dropped-item ESP uses the existing ESP look and physical-resolution Canvas path.
use super::Game;
use super::sunset::{Camera, Canvas, Module, line};
use super::sunset_item_esp_math::{
    Filter, LABEL_LIMIT, Nearest, label_into, label_rect, live_stack, range_sq,
};
use super::sunset_visual::{Look, draw_box, faded};
use std::sync::Mutex;

#[derive(Default)]
struct Scratch {
    filter: Filter,
    nearest: Nearest,
    text: String,
}

static SCRATCH: Mutex<Option<Scratch>> = Mutex::new(None);

pub(super) fn draw(game: &Game<'_>, cam: &Camera, m: &Module, lw: f32, out: &mut impl Canvas) {
    // Called only for ESP + Items. Empty worlds and disabled drawing don't touch cached buffers.
    let (boxes, labels, tracers) = (m.b("Item boxes"), m.b("Item labels"), m.b("Item tracers"));
    if game.dropped.is_empty() || !(boxes || labels || tracers) {
        return;
    }
    let (look, colour, range, fade) = (
        Look::of(m),
        m.c("Items color"),
        m.n("Range"),
        m.n("Fade distance"),
    );
    let viewport = cam.viewport();
    let eye = cam.eye();
    let alpha = game.clock.camera_fraction();
    let at = |i: usize| {
        let item = &game.dropped[i];
        let lerp = |a: f64, b: f64| a + alpha * (b - a);
        [
            lerp(item.previous.x, item.position.x),
            lerp(item.previous.y, item.position.y),
            lerp(item.previous.z, item.position.z),
        ]
    };
    let mut guard = SCRATCH.lock().unwrap_or_else(|e| e.into_inner());
    let scratch = guard.get_or_insert_with(Scratch::default);
    let Scratch {
        filter,
        nearest,
        text,
    } = scratch;
    let filter_mode = m.p("Item filter");
    // All mode needs no parsing or per-item lookup. A change while All is selected is picked up
    // immediately when switching to Include/Exclude.
    if filter_mode != "All" {
        filter.update(m.t("Item IDs"));
    }
    nearest.begin(m.n("Items max") as usize);
    for (index, item) in game.dropped.iter().enumerate() {
        if !live_stack(item.health, item.stack.count, &item.stack.item) {
            continue;
        }
        let at = at(index);
        let Some(distance_sq) = range_sq(at, eye, range) else {
            continue;
        };
        // The enclosing ball of the original 0.25-block item box. Off-screen drops never
        // consume the nearest-result budget, and no custom name/translation is examined here.
        if !cam.sees([at[0], at[1] + 0.125, at[2]], 0.217)
            || !filter.allows(filter_mode, &item.stack.item)
        {
            continue;
        }
        nearest.consider(distance_sq, index);
    }
    nearest.finish();
    let label_max = (m.n("Item label max") as usize).min(LABEL_LIMIT);
    let mut occupied = [[0.0; 4]; LABEL_LIMIT];
    let mut label_count = 0;
    let mut label_attempts = 0;
    let background = m.b("Item label background");
    for candidate in &nearest.sorted {
        let item = &game.dropped[candidate.index];
        let at = at(candidate.index);
        let colour = faded(colour, candidate.distance_sq.sqrt(), fade);
        let lo = [at[0] - 0.125, at[1], at[2] - 0.125];
        let hi = [at[0] + 0.125, at[1] + 0.25, at[2] + 0.125];
        if boxes {
            draw_box(cam, out, &look, lo, hi, colour, lw);
        }
        let Some(point) = cam.project([at[0], at[1] + 0.25, at[2]]) else {
            continue;
        };
        if !cam.on_screen(point, 0.0) {
            continue;
        }
        if tracers {
            line(
                out,
                [viewport[0] / 2.0, viewport[1] / 2.0],
                point,
                colour,
                lw,
            );
        }
        if !labels || label_attempts >= label_max {
            continue;
        }
        label_attempts += 1;
        // Native hover-name handling includes custom names, translated item names and potions.
        // This native API returns a String; call it only for the bounded visible label candidates.
        let name = horizon_client::item_tooltip::name_line(&item.stack, &|key| {
            game.pack.names.display_get(key)
        });
        label_into(&name, &item.stack.item, item.stack.count, text);
        let Some(rect) = label_rect(
            point,
            game.native_text_width(text),
            viewport,
            &occupied[..label_count],
        ) else {
            continue;
        };
        occupied[label_count] = rect;
        label_count += 1;
        if background {
            out.fill(rect, 0x7800_0000);
        }
        out.text(text, [rect[0] + 2.0, rect[1] + 2.0], colour & 0x00FF_FFFF);
    }
}
