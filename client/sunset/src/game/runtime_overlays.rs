// Runtime part (see runtime.rs): world and entity overlays, trajectories and storage.
use crate::game::sunset_compat::SunsetGame as _;

use super::Game;
use super::sunset_visual::{Draw, Kinds, Look, distance_colour, draw_box, faded, potato};
use std::fmt::Write as _;
use super::sunset::*;

pub(super) fn world_overlays(game: &Game<'_>, cam: &Camera, mods: &[Module], lw: f32, out: &mut impl Canvas) {
    super::sunset_survival_tools::draw(game, cam, mods, out, lw);
    super::sunset_explosive_aura::draw(game, cam, mods, out, lw);
    super::sunset_crystal_aura::draw(game, cam, mods, out, lw);
    super::sunset_lag::draw(game, cam, mods, out, lw);
    super::sunset_backtrack::draw(cam, mods, out, lw);
    super::sunset_block_overlay::overlay(game, cam, mods, out, lw);
    super::sunset_util::draw_shown(cam, out, lw);
    super::sunset_effects::draw(game, cam, mods, out);
    super::sunset_render::overlay(game, cam, mods, out, lw);
    super::sunset_meshcache_warm::draw(game, cam, mods, lw, out);
    super::sunset_new_chunks::draw(cam, mods, out);
    super::sunset_ore_sim::draw(game, cam, mods, out, lw);
    super::sunset_baritone_draw::draw(game, cam, mods, out, lw);
    super::sunset_hunting::draw(cam, mods, out, lw);
    super::sunset_light_scan::draw(cam, mods, out, lw);
    super::sunset_sound_detector::draw(game, cam, mods, out, lw);
    super::sunset_sign_historian::draw(game, cam, mods, out, lw);
    super::sunset_use_tracker::draw(game, cam, mods, out, lw);
    let get = |n: &str| mods.iter().find(|m| m.name == n && m.on);
    if let Some(m) = get("Nuker") {
        let (look, argb) = (Look::of(m), m.c("Broken color"));
        with(|s| {
            if let Some([lo, hi]) = s.nuke_box.filter(|_| m.b("Bounding box") && m.p("Mode") != "Smash") {
                draw_box(cam, out, &look, lo, hi, m.c("Box color"), lw);
            }
            for &(pos, left) in &s.nuke_broken {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                let a = ((argb >> 24) * u32::from(left) / 8) << 24;
                draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], a | (argb & 0x00FF_FFFF), lw);
            }
        });
    }
    if let Some(m) = get("Scaffold").filter(|m| m.b("Render")) {
        let (look, argb) = (Look::of(m), m.c("Color"));
        with(|s| {
            for &(pos, left) in &s.scaffold_placed {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                let a = ((argb >> 24) * u32::from(left) / 8) << 24;
                draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], a | (argb & 0x00FF_FFFF), lw);
            }
        });
    }
    let alpha = game.clock.camera_fraction();
    let lerp = |a: f64, b: f64| a + alpha * (b - a);
    let dist = |at: [f64; 3]| {
        let d = [at[0] - cam.pos[0], at[1] - cam.pos[1], at[2] - cam.pos[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    let (esp, tracers, tags) = (get("ESP"), get("Tracers"), get("Nametags"));
    // The players Nametags tags this frame, and where: Gear ESP's rows go above them.
    let mut tagged: Vec<(u32, [f32; 2])> = Vec::new();
    if esp.is_some() || tracers.is_some() || tags.is_some() {
        entity_overlays(game, cam, [esp, tracers, tags], lw, out, &mut tagged);
    }
    super::sunset_gear::draw(game, cam, mods, out, lw, &tagged);
    // Text drawn from one buffer, written over for each label.
    let mut text = String::new();

    if let Some(m) = esp.filter(|m| m.b("Items")) {
        let (look, colour, range, fade) = (Look::of(m), m.c("Items color"), range_of(m), m.n("Fade distance"));
        let (names, lines, filter, list) = (m.b("Item names"), m.b("Item tracers"), m.p("Item filter"), m.t("Item list"));
        let middle = [cam.w as f32 / 2.0, cam.h as f32 / 2.0];
        for item in &game.dropped {
            if filter != "All" && super::sunset_util::listed(list, &item.stack.item) != (filter == "Whitelist") {
                continue;
            }
            let at = [
                lerp(item.previous.x, item.position.x),
                lerp(item.previous.y, item.position.y),
                lerp(item.previous.z, item.position.z),
            ];
            let d = dist(at);
            if d > range {
                continue;
            }
            let lo = [at[0] - 0.125, at[1], at[2] - 0.125];
            let hi = [at[0] + 0.125, at[1] + 0.25, at[2] + 0.125];
            let colour = faded(colour, d, fade);
            draw_box(cam, out, &look, lo, hi, colour, lw);
            let Some([x, y]) = (names || lines).then(|| cam.project([at[0], at[1] + 0.25, at[2]])).flatten() else {
                continue;
            };
            if lines {
                line(out, middle, [x, y], colour, lw);
            }
            // Its name as the game shows it (a custom name, else the translated one) and the stack's count.
            if names {
                text.clear();
                text.push_str(&horizon_client::item_tooltip::name_line(&item.stack, &|key| game.pack.names.display_get(key)));
                if item.stack.count > 1 {
                    let _ = write!(text, " x{}", item.stack.count);
                }
                let tw = game.sunset_text_width(&text);
                out.fill([x - tw / 2.0 - 2.0, y - 14.0, tw + 4.0, 12.0], super::sunset::pal().label_bg);
                out.text(&text, [x - tw / 2.0, y - 12.0], colour & 0x00FF_FFFF);
            }
        }
    }

    if let Some(m) = get("Storage ESP") {
        let (look, range, fade, max) = (Look::of(m), m.n("Range"), m.n("Fade distance"), m.n("Max") as usize);
        let want = [m.b("Ender chests"), m.b("Chests"), m.b("Barrels"), m.b("Shulkers"), m.b("Other")];
        let facts = game.res.facts();
        let mut drawn = 0;
        for &(x, y, z) in game.world.block_entities.keys() {
            let at = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
            let d = dist(at);
            // Out of range or off screen: nothing more is looked up.
            if d > range || !cam.sees(at, 0.87) {
                continue;
            }
            let name = &facts.get(game.world.block(x, y, z)).name;
            let Some(col) = storage_colour(name) else {
                continue;
            };
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            let group = if short == "ender_chest" {
                0
            } else if short.ends_with("chest") {
                1
            } else if short == "barrel" {
                2
            } else if short.ends_with("shulker_box") {
                3
            } else {
                4
            };
            if !want[group] {
                continue;
            }
            let inset = if name.ends_with("chest") { 0.0625 } else { 0.0 };
            let (lo, hi) = match chest_pair(game, [x, y, z]) {
                Some(Some(pair)) => pair,
                Some(None) => continue,
                None => (
                    [f64::from(x) + inset, f64::from(y), f64::from(z) + inset],
                    [f64::from(x + 1) - inset, f64::from(y + 1) - inset * 2.0, f64::from(z + 1) - inset],
                ),
            };
            if draw_box(cam, out, &look, lo, hi, faded(col, d, fade), lw) {
                drawn += 1;
                if drawn >= max {
                    break;
                }
            }
        }
    }

    if let Some(m) = get("Breadcrumbs") {
        let (dots, colour) = (m.p("Mode") == "Potato", 0xFF00_0000 | accent());
        let now = game.player.as_ref().map(|p| [p.position.x, p.position.y + 0.1, p.position.z]);
        with(|s| {
            let mut last: Option<[f32; 2]> = None;
            // To where you are now at the end.
            for point in s.crumbs.iter().chain(now.as_ref()) {
                if dots {
                    potato(cam, out, *point, colour, lw);
                    continue;
                }
                let next = cam.project(*point);
                if let (Some(a), Some(b)) = (last, next) {
                    line(out, a, b, colour, lw);
                }
                last = next;
            }
        });
    }

    if let Some(m) = get("Block ESP") {
        let (look, range, max) = (Look::of(m), m.n("Range") * 1.5, m.n("Max") as usize);
        with(|s| {
            let mut drawn = 0;
            for &(pos, col) in &s.blocks.found {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                if dist([lo[0] + 0.5, lo[1] + 0.5, lo[2] + 0.5]) > range {
                    continue;
                }
                if draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0], col, lw) {
                    drawn += 1;
                    if drawn >= max {
                        break;
                    }
                }
            }
        });
    }

    if let Some(m) = get("Logout Spots") {
        let look = Look::of(m);
        let here = game.native_player().map(|p| p.dimension);
        with(|s| {
            for sp in s.spots.iter().filter(|sp| Some(sp.dim.as_str()) == here) {
                let lo = [sp.pos[0] - 0.3, sp.pos[1], sp.pos[2] - 0.3];
                let hi = [sp.pos[0] + 0.3, sp.pos[1] + 1.8, sp.pos[2] + 0.3];
                if !draw_box(cam, out, &look, lo, hi, 0xFFFF_5050, lw) || look.draw == Draw::Potato {
                    continue;
                }
                if let Some([x, y]) = cam.project([sp.pos[0], sp.pos[1] + 2.2, sp.pos[2]]) {
                    text.clear();
                    let _ = write!(text, "{} logged out {}m ago", sp.name, game.ticks.saturating_sub(sp.at) / 1200);
                    let tw = game.sunset_text_width(&text);
                    out.fill([x - tw / 2.0 - 2.0, y - 2.0, tw + 4.0, 12.0], super::sunset::pal().label_bg);
                    out.text(&text, [x - tw / 2.0, y], 0xFF5555);
                }
            }
        });
    }

    if let Some(m) = get("Spawner Alerts").filter(|m| m.b("Labels")) {
        super::sunset_world::spawner_labels(game, cam, m.n("Range"), out);
    }

    if let Some(m) = get("Trajectories") {
        trajectory(game, cam, m, out, lw);
    }

    if let Some(m) = get("Hole ESP") {
        let (look, height, max) = (Look::of(m), m.n("Height").max(0.02), m.n("Max") as usize);
        let colours = [m.c("Bedrock color"), m.c("Obsidian color"), m.c("Mixed color")];
        with(|s| {
            let mut drawn = 0;
            for &(pos, kind) in &s.holes.found {
                let lo = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                if draw_box(cam, out, &look, lo, [lo[0] + 1.0, lo[1] + height, lo[2] + 1.0], colours[usize::from(kind)], lw) {
                    drawn += 1;
                    if drawn >= max {
                        break;
                    }
                }
            }
        });
    }

    if let Some(m) = get("Waypoints") {
        let (range, dots, colour) = (m.n("Range"), m.p("Mode") == "Potato", 0xFF00_0000 | accent());
        let here = game.native_player().map(|p| p.dimension);
        with(|s| {
            for w in s.waypoints.iter().filter(|w| Some(w.dim.as_str()) == here) {
                let d = dist(w.pos);
                if d > range {
                    continue;
                }
                if dots {
                    potato(cam, out, w.pos, colour, lw);
                    continue;
                }
                let Some([x, y]) = cam.project(w.pos).filter(|p| cam.on_screen(*p, 200.0)) else {
                    continue;
                };
                text.clear();
                let _ = write!(text, "{} {}m", w.name, d.round());
                let tw = game.sunset_text_width(&text);
                out.fill([x - tw / 2.0 - 2.0, y - 2.0, tw + 4.0, 12.0], super::sunset::pal().label_bg);
                out.text(&text, [x - tw / 2.0, y], accent());
                out.fill([x - 1.0, y + 11.0, 2.0, 4.0], colour);
            }
        });
    }
}

// ESP's, Tracers' and Nametags' Range: 0 (shown "All") draws everything loaded, else a limit up to this.
pub(super) const ALL_RANGE_MAX: f64 = 512.0;

// A Range setting as a distance: 0 is no limit.
fn range_of(m: &Module) -> f64 {
    match m.n("Range") {
        r if r <= 0.0 => f64::INFINITY,
        r => r,
    }
}

// The settings ESP, Tracers and Nametags draw with, read once a frame.
struct EntityLook {
    pub(super) kinds: Kinds,
    pub(super) look: Look,
    pub(super) range: f64,
    pub(super) max: usize,
    fade: f64,
    potato: bool,
}

impl EntityLook {
    pub(super) fn of(m: &Module) -> Self {
        let look = Look::of(m);
        Self {
            kinds: Kinds::of(m),
            potato: look.draw == Draw::Potato,
            look,
            range: range_of(m),
            max: m.n("Max") as usize,
            fade: m.n("Fade distance"),
        }
    }
}

// ESP, Tracers and Nametags in one pass over the mobs: those in range of any of them, nearest first when a Max
// leaves some out.
// ESP on one entity, as entity_overlays draws it for a mob in the world (its look, kind colours, fade, shield colours
// and 2D extras), but for any `mob`, in or out of the world, at distance `d`: the Sidebar menu's ESP preview. Friends
// and the murderer finder are left out (they are about who it is). True when anything was drawn.
#[allow(clippy::too_many_arguments)]
pub(super) fn esp_one(game: &Game<'_>, cam: &Camera, out: &mut impl Canvas, m: &Module, mob: &horizon_client::mobs::Mob, lo: [f64; 3], hi: [f64; 3], d: f64, lw: f32) -> bool {
    let l = EntityLook::of(m);
    let kind = kind_of(mob);
    if !l.kinds.wants(kind, &mob.kind.id) {
        return false;
    }
    let col = if l.potato { l.kinds.colour(kind, false) } else { faded(l.kinds.colour(kind, false), d, l.fade) };
    let two = super::sunset_esp2d::TwoD::of(m);
    let col = match (!l.potato).then(|| super::sunset_esp2d::shield_colour(game, &two, mob)).flatten() {
        Some(c) => faded(c, d, l.fade),
        None => col,
    };
    if l.look.draw == Draw::Box2d && two.extras() {
        super::sunset_esp2d::draw(game, cam, out, &two, mob, lo, hi, d, col, lw)
    } else {
        draw_box(cam, out, &l.look, lo, hi, col, lw)
    }
}

fn entity_overlays(game: &Game<'_>, cam: &Camera, mods: [Option<&Module>; 3], lw: f32, out: &mut impl Canvas, tagged: &mut Vec<(u32, [f32; 2])>) {
    let [esp, tracers, tags] = mods.map(|m| m.map(EntityLook::of));
    let two = mods[0].map(super::sunset_esp2d::TwoD::of);
    // ESP's Murderer finder: its suspects in the Murderer color.
    let murderer = mods[0].filter(|m| m.b("Murderer finder")).map(|m| m.c("Murderer color"));
    let tags_module = mods[2];
    let banded = tags_module.is_some_and(|m| m.p("Health colors") == "Bands");
    let (tracer_target, tracer_bottom, tracer_by_distance) =
        mods[1].map_or(("Body", false, false), |m| (m.p("Target"), m.p("Start") == "Bottom", m.b("Distance colors")));
    let (tag_players, tag_mobs, tag_friends) =
        tags_module.map_or((false, false, false), |m| (m.b("Players"), m.b("Mobs"), !m.b("Ignore friends")));
    let tag_extra = tags_module.map_or(("", false), |m| (m.t("Entities"), m.p("List mode") == "Blacklist"));
    let (show_health, show_distance, tag_back, tag_colour) =
        tags_module.map_or((false, false, false, 0), |m| (m.b("Health"), m.b("Distance"), m.b("Background"), m.c("Name color") & 0x00FF_FFFF));
    let show_pops = tags_module.is_some_and(|m| m.b("Totem pops"));
    let looks = [&esp, &tracers, &tags];
    let range = looks.iter().filter_map(|l| l.as_ref().map(|l| l.range)).fold(0.0, f64::max);
    let alpha = game.clock.camera_fraction();
    let lerp = |a: f64, b: f64| a + alpha * (b - a);
    let own = own_id(game);
    // The mobs in range: distance, index, and the box where the frame draws them.
    let mut near: Vec<(f64, usize, [f64; 3], [f64; 3])> = Vec::with_capacity(game.mobs.len());
    for (i, mob) in game.mobs.iter().enumerate() {
        if !mob.alive() || mob.removed || Some(mob.id) == own {
            continue;
        }
        let b = mob.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let off = [
            lerp(mob.previous.x, mob.position.x) - mob.position.x,
            lerp(mob.previous.y, mob.position.y) - mob.position.y,
            lerp(mob.previous.z, mob.position.z) - mob.position.z,
        ];
        let lo = [lo.x + off[0], lo.y + off[1], lo.z + off[2]];
        let hi = [hi.x + off[0], hi.y + off[1], hi.z + off[2]];
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let d = super::sunset_util::dist(mid, cam.pos);
        if d <= range {
            near.push((d, i, lo, hi));
        }
    }
    if looks.iter().any(|l| l.as_ref().is_some_and(|l| l.max < near.len())) {
        near.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    }
    // Friends only matter to the full modes.
    let friendly = esp.as_ref().is_some_and(|l| !l.potato) || tracers.as_ref().is_some_and(|l| !l.potato) || tags.is_some();
    let names = if friendly { game.remote_player_names() } else { Vec::new() };
    let friends = if friendly { with(|s| s.friends.clone()) } else { Vec::new() };
    let centre = [cam.w as f32 / 2.0, cam.h as f32 / 2.0];
    let from = if tracer_bottom { [centre[0], cam.h as f32] } else { centre };
    let mut drawn = [0usize; 3];
    let (mut name, mut hp, mut tail) = (String::new(), String::new(), String::new());
    for &(d, i, lo, hi) in &near {
        let mob = &game.mobs[i];
        let kind = kind_of(mob);
        let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let friend = || kind == Kind::Player && is_friend(&friends, &names, mob.id);
        if let Some(l) = &esp
            && drawn[0] < l.max
            && d <= l.range
            && l.kinds.wants(kind, &mob.kind.id)
            && super::sunset_esp_villager::villager_wanted(mods[0].unwrap(), mob)
        {
            let col = if l.potato { l.kinds.colour(kind, false) } else { faded(l.kinds.colour(kind, friend()), d, l.fade) };
            let col = super::sunset_esp_villager::villager_color(mods[0].unwrap(), mob)
                .map_or(col, |color| faded(color, d, l.fade));
            let shield = two.as_ref().filter(|_| !l.potato).and_then(|t| super::sunset_esp2d::shield_colour(game, t, mob));
            let col = shield.map_or(col, |c| faded(c, d, l.fade));
            let col = match &murderer {
                Some(c) if kind == Kind::Player && super::sunset_murder::suspect(mob.id) => faded(*c, d, l.fade),
                _ => col,
            };
            let shown = match two.as_ref().filter(|t| l.look.draw == Draw::Box2d && t.extras()) {
                Some(t) => super::sunset_esp2d::draw(game, cam, out, t, mob, lo, hi, d, col, lw),
                None => draw_box(cam, out, &l.look, lo, hi, col, lw),
            };
            if shown {
                drawn[0] += 1;
                if mob.kind.id == "minecraft:villager" && mods[0].is_some_and(|m| m.b("Villager level label"))
                    && let Some([x, y]) = cam.project([mid[0], hi[1] + 0.15, mid[2]])
                {
                    let label = format!("L{}", mob.extra.villager_level.max(1));
                    out.text(&label, [x - game.sunset_text_width(&label) / 2.0, y], col);
                }
            }
        }
        if let Some(l) = &tracers
            && drawn[1] < l.max
            && d <= l.range
            && l.kinds.wants(kind, &mob.kind.id)
        {
            // Potato: the line alone, to the middle, in the kind's colour.
            let (aim, col) = if l.potato {
                (mid, l.kinds.colour(kind, false))
            } else {
                let aim = match tracer_target {
                    "Head" => [mid[0], hi[1] - 0.1, mid[2]],
                    "Feet" => [mid[0], lo[1], mid[2]],
                    _ => mid,
                };
                (aim, if tracer_by_distance { distance_colour(d) } else { l.kinds.colour(kind, friend()) })
            };
            if let Some(to) = cam.project(aim) {
                line(out, from, to, col, lw);
                drawn[1] += 1;
            }
        }
        if let Some(l) = &tags
            && drawn[2] < l.max
            && d <= l.range
            && super::sunset_util::kind_wanted(if kind == Kind::Player { tag_players } else { tag_mobs }, tag_extra.0, tag_extra.1, &mob.kind.id)
        {
            let top = [mid[0], hi[1] + 0.6, mid[2]];
            if !cam.sees(top, 1.0) {
                continue;
            }
            let friend = friend();
            if friend && !tag_friends {
                continue;
            }
            let Some([x, y]) = cam.project(top) else {
                continue;
            };
            drawn[2] += 1;
            super::sunset_util::name_into(game, mob, &mut name);
            let name_colour = if friend { 0x55FFFF } else { tag_colour };
            let nw = game.sunset_text_width(&name);
            if kind == Kind::Player {
                tagged.push((mob.id, [x, y - 2.0]));
            }
            // Potato: the name alone.
            if l.potato {
                out.text(&name, [x - nw / 2.0, y], name_colour);
                continue;
            }
            hp.clear();
            tail.clear();
            // Whole numbers written as integers: float formatting was a good part of a frame of 200 tags.
            // Bands: the health rounded up, red to 5, gold to 10, yellow to 15, green to 20, dark green over.
            if show_health && banded {
                let _ = write!(hp, "{}", mob.health.max(0.0).ceil() as i32);
            } else if show_health {
                let tenths = (mob.health * 10.0).round() as i32;
                let _ = write!(hp, "{}", tenths / 10);
                if tenths % 10 != 0 {
                    let _ = write!(hp, ".{}", (tenths % 10).abs());
                }
            }
            let popped = if show_pops { super::sunset_effects::pops(mob.id) } else { 0 };
            if popped > 0 {
                let _ = write!(tail, " -{popped}");
            }
            if show_distance {
                let _ = write!(tail, " {}m", d.round() as i64);
            }
            let (hw, tw) = (game.sunset_text_width(&hp), game.sunset_text_width(&tail));
            let gap = if hp.is_empty() { 0.0 } else { 4.0 };
            let total = nw + gap + hw + tw;
            let x0 = x - total / 2.0;
            if tag_back {
                out.fill([x0 - 2.0, y - 2.0, total + 4.0, 12.0], super::sunset::pal().label_bg);
            }
            out.text(&name, [x0, y], name_colour);
            if !hp.is_empty() {
                let colour = if banded {
                    match mob.health.ceil() {
                        h if h <= 5.0 => 0xFF5555,
                        h if h <= 10.0 => 0xFFAA00,
                        h if h <= 15.0 => 0xFFFF55,
                        h if h <= 20.0 => 0x55FF55,
                        _ => 0x00AA00,
                    }
                } else {
                    health_colour(mob.health / mob.kind.max_health.max(1.0))
                };
                out.text(&hp, [x0 + nw + gap, y], colour);
            }
            if !tail.is_empty() {
                out.text(&tail, [x0 + nw + gap + hw, y], 0xAAAAAA);
            }
        }
    }
}

// Where what the player holds would land: a bow being drawn, each projectile in a loaded crossbow,
// or a throwable, as Show lets through; with Pearls in flight, every pearl in the air too, and with Landing time how
// long each has left. Potato: only a square where it lands.
pub(super) fn trajectory(game: &Game<'_>, cam: &Camera, m: &Module, out: &mut impl Canvas, lw: f32) {
    let potato_only = m.p("Mode") == "Potato";
    let colour = 0xFF00_0000 | accent();
    if m.b("Pearls in flight") {
        let ids: Vec<u32> = game
            .creatures
            .projectiles
            .iter()
            .filter(|p| p.kind == horizon_client::projectiles::ProjectileKind::EnderPearl && !p.stuck)
            .map(|p| p.id)
            .collect();
        for id in ids {
            let Some(f) = super::sunset_throws::pearl_landing(game, id, 400, true) else {
                continue;
            };
            draw_path(cam, out, &f.path, potato_only, colour, lw);
            if m.b("Landing time")
                && let Some([x, y]) = cam.project(f.land).filter(|p| cam.on_screen(*p, 50.0))
            {
                let tag = format!("{:.1}s", f64::from(f.ticks) / 20.0);
                let tw = game.sunset_text_width(&tag);
                out.fill([x - tw / 2.0 - 2.0, y - 14.0, tw + 4.0, 12.0], super::sunset::pal().label_bg);
                out.text(&tag, [x - tw / 2.0, y - 12.0], colour);
            }
        }
    }
    let Some(stack) = game.held() else {
        return;
    };
    let name = stack.item.strip_prefix("minecraft:").unwrap_or(&stack.item);
    let show = m.p("Show");
    let wanted = match name {
        "bow" => matches!(show, "All" | "Bows" | "Bow"),
        "crossbow" => matches!(show, "All" | "Bows" | "Crossbow"),
        "ender_pearl" => matches!(show, "All" | "Pearl"),
        _ => show == "All",
    };
    if !wanted {
        return;
    }
    let charged = if name == "crossbow" {
        stack.data().map_or(&[][..], |d| d.charged.as_slice())
    } else {
        &[]
    };
    let (speed, gravity, drag): (f64, f64, f64) = match name {
        "bow" => match game.using {
            Some((_, ticks)) => {
                let t = f64::from(ticks) / 20.0;
                (((t * t + t * 2.0) / 3.0).min(1.0) * 3.0, 0.05, 0.99)
            }
            None => return,
        },
        "crossbow" if !charged.is_empty() => {
            let power = if charged.iter().any(|a| &*a.item == "minecraft:firework_rocket") { 1.6 } else { 3.15 };
            (power, 0.05, 0.99)
        }
        "trident" => (2.5, 0.05, 0.99),
        "ender_pearl" | "snowball" | "egg" => (1.5, 0.03, 0.99),
        "wind_charge" => (1.5, 0.0, 1.0),
        "splash_potion" | "lingering_potion" => (0.5, 0.05, 0.99),
        "experience_bottle" => (0.7, 0.07, 0.99),
        _ => return,
    };
    let Some(eye) = game.eye_position() else {
        return;
    };
    let count = charged.len().max(1);
    let step = if count > 1 {
        // The charge supplies the shot count. The enchantment's projectile_spread is the half-angle.
        let spread = game.stations().enchantments.modify_value(
            Some(stack), "minecraft:projectile_spread", &Default::default(), &mut horizon_core::random::LegacyRandom::new(0), 0.0,
        );
        2.0 * f64::from(spread.max(0.0)).to_radians() / (count - 1) as f64
    } else {
        0.0
    };
    let (sin_yaw, cos_yaw) = f64::from(game.yaw).to_radians().sin_cos();
    let (sin_pitch, cos_pitch) = f64::from(game.pitch).to_radians().sin_cos();
    let forward = glam::DVec3::new(-sin_yaw * cos_pitch, -sin_pitch, cos_yaw * cos_pitch);
    let side = glam::DVec3::new(cos_yaw, 0.0, sin_yaw);
    // Java fires from the middle out, alternating sides. Even-sized loads straddle the centre.
    let offset = if count.is_multiple_of(2) { step * 0.5 } else { 0.0 };
    let mut path = Vec::with_capacity(201);
    for index in 0..count {
        let pair = index.div_ceil(2) as f64 * step;
        let angle = if index.is_multiple_of(2) { offset + pair } else { offset - pair };
        let (sin, cos) = angle.sin_cos();
        let mut vel = ((forward * cos + side * sin) * speed).to_array();
        let rocket = charged.get(index).is_some_and(|a| &*a.item == "minecraft:firework_rocket");
        let (height, gravity, drag) = if rocket { (0.15, 0.0, 1.0) } else { (0.1, gravity, drag) };
        let mut pos = [eye[0], eye[1] - height, eye[2]];
        path.clear();
        path.push(pos);
        for _ in 0..200 {
            for i in 0..3 {
                pos[i] += vel[i];
                vel[i] *= drag;
            }
            vel[1] -= gravity;
            path.push(pos);
            if solid_at(game, pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32) || pos[1] < -80.0 {
                break;
            }
        }
        draw_path(cam, out, &path, potato_only, colour, lw);
    }
}

// A flight drawn as a line ending in a small box where it lands, or (Potato) a square there alone.
fn draw_path(cam: &Camera, out: &mut impl Canvas, path: &[[f64; 3]], potato_only: bool, colour: u32, lw: f32) {
    let Some(&end) = path.last() else {
        return;
    };
    if potato_only {
        potato(cam, out, end, colour, lw);
        return;
    }
    for pair in path.windows(2) {
        if let (Some(a), Some(b)) = (cam.project(pair[0]), cam.project(pair[1])) {
            line(out, a, b, colour, lw);
        }
    }
    draw_box(cam, out, &Look::BOXED, [end[0] - 0.25, end[1] - 0.25, end[2] - 0.25], [end[0] + 0.25, end[1] + 0.25, end[2] + 0.25], colour, lw);
}

// A double chest (or trapped or copper one) is one box: each half's outline reaches the side its other half is on.
// The half to the west or north draws the pair's box (Some(Some(box))), the other half nothing (Some(None)); a single
// chest or any other block: None.
fn chest_pair(game: &Game<'_>, p: [i32; 3]) -> Option<Option<([f64; 3], [f64; 3])>> {
    use super::sunset_util::facts;
    let here = facts(game, p);
    if !here.name.ends_with("chest") {
        return None;
    }
    let b = *here.outline.first()?;
    let side = if b[0] <= 0.0 { [-1, 0] } else if b[3] >= 1.0 { [1, 0] } else if b[2] <= 0.0 { [0, -1] } else if b[5] >= 1.0 { [0, 1] } else { return None };
    let at = [p[0] + side[0], p[1], p[2] + side[1]];
    let there = facts(game, at);
    let o = *there.outline.first()?;
    // The other half reaches back.
    let back = match side {
        [-1, 0] => o[3] >= 1.0,
        [1, 0] => o[0] <= 0.0,
        [0, -1] => o[5] >= 1.0,
        _ => o[2] <= 0.0,
    };
    if there.name != here.name || !back {
        return None;
    }
    if side[0] < 0 || side[1] < 0 {
        return Some(None);
    }
    let corner = |q: [i32; 3], f: [f64; 3]| [f64::from(q[0]) + f[0], f64::from(q[1]) + f[1], f64::from(q[2]) + f[2]];
    Some(Some((corner(p, [b[0], b[1], b[2]]), corner(at, [o[3], o[4], o[5]]))))
}

pub(super) fn storage_colour(name: &str) -> Option<u32> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    Some(if n == "ender_chest" {
        0xFFB0_50FF
    } else if n == "chest" || n == "trapped_chest" || n.ends_with("copper_chest") {
        0xFFFF_A040
    } else if n.ends_with("shulker_box") {
        0xFFFF_70C0
    } else if n == "barrel" {
        0xFFC0_8040
    } else if matches!(n, "hopper" | "dropper" | "dispenser" | "furnace" | "blast_furnace" | "smoker" | "crafter") {
        0xFFB0_B0B0
    } else {
        return None;
    })
}

// A line, as one slanted quad.
pub(super) fn line(out: &mut impl Canvas, from: [f32; 2], to: [f32; 2], argb: u32, lw: f32) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len < 1.0 {
        return;
    }
    let half = lw.max(1.0) / 2.0;
    let (nx, ny) = (-dy / len * half, dx / len * half);
    out.quad(
        [[from[0] + nx, from[1] + ny], [to[0] + nx, to[1] + ny], [to[0] - nx, to[1] - ny], [from[0] - nx, from[1] - ny]],
        argb,
    );
}
