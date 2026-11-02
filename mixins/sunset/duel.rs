// Ported from Sayer's Duelkit (MIT, github.com/PointOfPressure/duelkit): Wind Jump, Hold to Eat, Pearl Catch and
// Loadouts.
use super::sunset::{Canvas, Category, Module, Shape, hold, say, with};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::menu::Input;
use horizon_client::projectiles::ProjectileKind;
use horizon_protocol::packets::play::{self, Serverbound as S};
use std::sync::Mutex;
use std::time::Instant;
use winit::event::MouseButton;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Wind Jump", Category::Movement, "Jumps with wind charges.")
            .num("Min pitch", 60.0, 0.0, 90.0, 5.0)
            .num("Window (ms)", 1500.0, 200.0, 5000.0, 100.0),
        Module::new("Hold to Eat", Category::Player, "Eats from a held hotbar key.")
            .bool("Golden apples", true)
            .bool("Any food", false),
        Module::new("Pearl Catch", Category::Movement, "Catches pearls with wind charges.")
            .num("Min look up", 45.0, 0.0, 90.0, 5.0)
            .bool("Auto", true)
            .num("Max delay", 10.0, 1.0, 20.0, 1.0)
            .num("Delay", 2.0, 1.0, 20.0, 1.0)
            .bool("At feet", false),
        Module::new("Loadouts", Category::Player, "Hold K for your saved hotbar layouts.")
            .text("Menu key", "K"),
    ]
}

// Duelkit's numbers: a pearl leaves at 1.5 blocks a tick and slows by 0.99 with 0.03 gravity; a wind charge flies
// straight at 1.5 with neither; its blast (radius 1.2) pushes with 1.22.
const THROW: f64 = 1.5;
const RADIUS: f64 = 1.2;
const KICK: f64 = 1.22;

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn pearl_step(pos: [f64; 3], vel: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let pos = add(pos, vel);
    (pos, add(scale(vel, 0.99), [0.0, -0.03, 0.0]))
}

// The earliest tick a wind charge thrown now from `from` (inheriting `inherit` a tick) meets the pearl: the direction
// to throw it and how high the kicked pearl goes.
fn intercept(from: [f64; 3], inherit: [f64; 3], pearl: [f64; 3], speed: [f64; 3]) -> Option<([f64; 3], f64)> {
    let (mut pos, mut vel) = (pearl, speed);
    for k in 1..=40 {
        (pos, vel) = pearl_step(pos, vel);
        let need = add(add(pos, scale(from, -1.0)), scale(inherit, -f64::from(k)));
        let len = need.iter().map(|v| v * v).sum::<f64>().sqrt();
        if len <= THROW * f64::from(k) && len > 0.0 {
            let dir = scale(need, 1.0 / len);
            let kicked = add(vel, scale(dir, KICK * (1.0 - 0.2 / (2.0 * RADIUS))));
            let (mut y, mut vy) = (pos[1], kicked[1]);
            while vy > 0.0 {
                y += vy;
                vy = vy * 0.99 - 0.03;
            }
            return Some((dir, y));
        }
    }
    None
}

// Your eyes, a pearl's start and speed thrown with your look and movement, and the movement a charge inherits.
fn throw_start(game: &Game<'_>) -> Option<([f64; 3], [f64; 3], [f64; 3], bool)> {
    let p = game.player.as_ref()?;
    let eye = game.eye_position()?;
    let look = super::view_vector(game.yaw, game.pitch);
    let ground = p.ground.on_ground;
    let move_ = [p.velocity.x, if ground { 0.0 } else { p.velocity.y }, p.velocity.z];
    Some((eye, add(scale(look, THROW), move_), move_, ground))
}

// Auto: the delay (ticks) whose catch sends the pearl highest, simulated for both throws; None if none catches it.
fn best_delay(game: &Game<'_>, max: u32) -> Option<u32> {
    let (eye, pearl_speed, move_, ground) = throw_start(game)?;
    let (mut me, mut mv) = (eye, move_);
    let (mut pp, mut pv) = (add(eye, [0.0, -0.1, 0.0]), pearl_speed);
    let mut best: Option<(u32, f64)> = None;
    for d in 1..=max {
        if !ground {
            me = add(me, mv);
            mv = [mv[0] * 0.91, (mv[1] - 0.08) * 0.98, mv[2] * 0.91];
        }
        (pp, pv) = pearl_step(pp, pv);
        let inherit = if ground { move_ } else { mv };
        if let Some((_, apex)) = intercept(add(me, [0.0, -0.1, 0.0]), inherit, pp, pv)
            && best.is_none_or(|b| apex > b.1)
        {
            best = Some((d, apex));
        }
    }
    best.map(|b| b.0)
}

#[derive(Default)]
struct Duel {
    // Wind Jump: when the wind charge went.
    thrown: Option<Instant>,
    // Hold to Eat: the slot eaten from, its key, and the slot to go back to.
    eating: Option<(usize, String, usize)>,
    // Pearl Catch: the tick the wind charge goes.
    catch_at: Option<u64>,
    // Loadouts: the Menu key is held; a loadout's number key selected its slot, to undo (that slot, the one before).
    listing: bool,
    undo: Option<(usize, usize)>,
}

static DUEL: Mutex<Option<Duel>> = Mutex::new(None);

fn duel<R>(f: impl FnOnce(&mut Duel) -> R) -> R {
    f(DUEL.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Duel::default))
}

fn module(name: &str) -> Option<Module> {
    with(|s| s.module(name).filter(|m| m.on).cloned())
}

// The use hook: a wind charge thrown from the ground looking down far enough.
pub(super) fn used(game: &Game<'_>, slot: usize) {
    if !game.native_window() || !game.native_on(super::sunset::ID) {
        return;
    }
    let Some(stack) = game.stack_in(slot) else {
        return;
    };
    let cooling = game.res.items().get(&stack.item).is_some_and(|i| {
        game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(stack, i.use_cooldown.as_ref()))
    });
    if let Some(m) = module("Pearl Catch")
        && stack.item.ends_with(":ender_pearl")
        && !cooling
        && -f64::from(game.pitch) >= m.n("Min look up")
    {
        let delay = if m.b("Auto") { best_delay(game, m.n("Max delay") as u32) } else { Some(m.n("Delay") as u32) };
        duel(|d| d.catch_at = delay.map(|t| game.ticks + u64::from(t)));
    }
    if stack.item.ends_with(":wind_charge")
        && !cooling
        && game.player.as_ref().is_some_and(|p| p.ground.on_ground)
        && module("Wind Jump").is_some_and(|m| f64::from(game.pitch) >= m.n("Min pitch"))
    {
        duel(|d| d.thrown = Some(Instant::now()));
    }
}

// The explosion hook, before the game adds the push: the jump (velocity up 0.42, and a sprint jump's 0.2 forward)
// goes first.
pub(super) fn blast(game: &mut Game<'_>, push: Option<[f64; 3]>) {
    let Some(thrown) = duel(|d| d.thrown) else {
        return;
    };
    let window = module("Wind Jump").map_or(0.0, |m| m.n("Window (ms)"));
    if thrown.elapsed().as_secs_f64() * 1000.0 > window {
        duel(|d| d.thrown = None);
        return;
    }
    let yaw = f64::from(game.yaw).to_radians();
    if push.is_none_or(|p| p[1] <= 0.0) {
        return;
    }
    let Some(p) = game.player.as_mut().filter(|p| p.ground.on_ground) else {
        return;
    };
    duel(|d| d.thrown = None);
    p.velocity.y = p.velocity.y.max(0.42);
    if p.sprinting {
        p.velocity.x -= yaw.sin() * 0.2;
        p.velocity.z += yaw.cos() * 0.2;
    }
}

fn edible(game: &Game<'_>, m: &Module, slot: usize) -> bool {
    game.inventory[slot].as_ref().is_some_and(|s| {
        if s.item.ends_with(":golden_apple") || s.item.ends_with(":enchanted_golden_apple") {
            m.b("Golden apples")
        } else {
            m.b("Any food") && game.res.items().get(&s.item).is_some_and(|i| i.food.is_some())
        }
    })
}

// The key hook: Loadouts' keys; Hold to Eat: the hotbar key of food pressed starts eating it.
pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    loadout_key(game, key, pressed);
    if duel(|d| d.listing) {
        return;
    }
    let Some(m) = module("Hold to Eat") else {
        return;
    };
    if !pressed || game.screen != Screen::Playing || duel(|d| d.eating.is_some()) {
        return;
    }
    let Some(slot) = (0..9).find(|i| game.settings.keys.get(&format!("key.hotbar.{}", i + 1)) == key) else {
        return;
    };
    if edible(game, &m, slot) {
        let back = game.selected;
        duel(|d| d.eating = Some((slot, key.to_owned(), back)));
    }
}

// Pearl Catch's wind charge: from the offhand or the hotbar (off cooldown), aimed at your pearl (Auto), straight
// down (At feet) or along your look. The use packet carries the aim; your view does not move.
fn catch(game: &mut Game<'_>, m: &Module) {
    let ready = |game: &Game<'_>, slot: usize| {
        game.stack_in(slot).is_some_and(|s| {
            s.item.ends_with(":wind_charge")
                && !game.res.items().get(&s.item).is_some_and(|i| {
                    game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(s, i.use_cooldown.as_ref()))
                })
        })
    };
    let Some(slot) = std::iter::once(util::OFFHAND).chain(0..9).find(|&s| ready(game, s)) else {
        return;
    };
    let (mut yaw, mut pitch) = (game.yaw, game.pitch);
    if m.b("At feet") {
        pitch = 90.0;
    } else if m.b("Auto")
        && let Some((eye, _, move_, _)) = throw_start(game)
        && let Some(pearl) = game
            .creatures
            .projectiles
            .iter()
            .filter(|p| p.kind == ProjectileKind::EnderPearl && util::dist(eye, [p.position.x, p.position.y, p.position.z]) < 48.0)
            .max_by_key(|p| p.id)
        && let Some((dir, _)) = intercept(
            add(eye, [0.0, -0.1, 0.0]),
            move_,
            [pearl.position.x, pearl.position.y, pearl.position.z],
            [pearl.velocity.x, pearl.velocity.y, pearl.velocity.z],
        )
    {
        yaw = (-dir[0]).atan2(dir[2]).to_degrees() as f32;
        pitch = -dir[1].asin().to_degrees() as f32;
    }
    let back = game.selected;
    if slot != util::OFFHAND {
        hold(game, slot);
    }
    let sequence = util::sequence(game);
    game.send(S::UseItem(play::UseItem { hand: i32::from(slot == util::OFFHAND), sequence, yaw, pitch }));
    game.swing_arm();
    hold(game, back);
}

pub(super) fn tick(game: &mut Game<'_>) {
    if let Some(at) = duel(|d| d.catch_at)
        && game.ticks >= at
    {
        duel(|d| d.catch_at = None);
        if let Some(m) = module("Pearl Catch")
            && game.screen == Screen::Playing
        {
            catch(game, &m);
        }
    }
    if let Some((from, back)) = duel(|d| d.undo.take())
        && game.selected == from
    {
        hold(game, back);
    }
    let Some((slot, key, back)) = duel(|d| d.eating.clone()) else {
        return;
    };
    let held = game.physical_keys.iter().any(|k| horizon_client::keybinds::key_name(*k) == Some(key.as_str()));
    let go_on = module("Hold to Eat").is_some_and(|m| edible(game, &m, slot));
    if held && go_on && game.screen == Screen::Playing {
        hold(game, slot);
        if game.using.is_none() {
            let _ = game.play_mouse(MouseButton::Right, true);
        }
        return;
    }
    // Let go (or ran out): stop eating, and the slot from before comes back.
    let _ = game.play_mouse(MouseButton::Right, false);
    hold(game, back);
    duel(|d| d.eating = None);
}

// Loadouts, as Duelkit's: each "name|slot,...(nine)|offhand", a slot "-" (left alone) or an item id with its custom
// name after a "~" when it had one (names %-escaped). Kept in loadouts.txt in the mod's data folder.
struct Slot {
    item: String,
    name: Option<String>,
}

struct Loadout {
    name: String,
    hotbar: Vec<Option<Slot>>,
    offhand: Option<Slot>,
}

fn escape(s: &str) -> String {
    s.replace('%', "%25").replace('|', "%7C").replace(',', "%2C").replace('~', "%7E").replace('\n', " ")
}

fn unescape(s: &str) -> String {
    s.replace("%7E", "~").replace("%2C", ",").replace("%7C", "|").replace("%25", "%")
}

fn parse(line: &str) -> Option<Loadout> {
    let mut parts = line.split('|');
    let name = unescape(parts.next()?);
    let slot = |s: &str| {
        (s != "-" && !s.is_empty()).then(|| match s.split_once('~') {
            Some((item, name)) => Slot { item: item.to_owned(), name: Some(unescape(name)) },
            None => Slot { item: s.to_owned(), name: None },
        })
    };
    let hotbar = parts.next()?.split(',').map(slot).collect();
    let offhand = slot(parts.next()?);
    Some(Loadout { name, hotbar, offhand })
}

// A loadout of what you hold now: the hotbar and the off hand, named.
fn capture(game: &Game<'_>, name: &str) -> String {
    let slot = |s: Option<&horizon_client::items::ItemStack>| match s {
        None => "-".to_owned(),
        Some(s) => match s.custom_name() {
            Some(n) => format!("{}~{}", s.item, escape(&n)),
            None => s.item.to_string(),
        },
    };
    let hotbar: Vec<String> = (0..9).map(|i| slot(game.inventory[i].as_ref())).collect();
    format!("{}|{}|{}", escape(name), hotbar.join(","), slot(game.offhand.as_ref()))
}

fn loadouts() -> Vec<Loadout> {
    with(|s| s.loadouts.iter().filter_map(|l| parse(l)).collect())
}

fn file(game: &Game<'_>) -> std::path::PathBuf {
    game.native_data_dir(super::sunset::ID).join("loadouts.txt")
}

// Read once with the config: the file's loadouts, or (the first time) the ones the config kept, written to it.
pub(super) fn load_loadouts(game: &Game<'_>) {
    match std::fs::read_to_string(file(game)) {
        Ok(text) => with(|s| s.loadouts = text.lines().filter(|l| parse(l).is_some()).map(str::to_owned).collect()),
        Err(_) if with(|s| !s.loadouts.is_empty()) => store(game),
        Err(_) => {}
    }
}

// The loadouts written to their file (never through a link).
fn store(game: &Game<'_>) {
    let path = file(game);
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return;
    }
    let text: String = with(|s| s.loadouts.iter().map(|l| format!("{l}\n")).collect());
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, text);
}

fn find_loadout(list: &[Loadout], what: &str) -> Option<usize> {
    what.parse::<usize>()
        .ok()
        .filter(|n| (1..=list.len()).contains(n))
        .map(|n| n - 1)
        .or_else(|| list.iter().position(|l| l.name.eq_ignore_ascii_case(what)))
}

// 0: not it; 1: the same item; 2: the same item and name (or a slot saved without a name).
fn score(want: &Slot, have: Option<&horizon_client::items::ItemStack>) -> u8 {
    match have {
        Some(h) if *h.item == *want.item => {
            if want.name.is_none() || h.custom_name().as_deref() == want.name.as_deref() { 2 } else { 1 }
        }
        _ => 0,
    }
}

// Puts each saved item in its hotbar slot (and the offhand) with the clicks of a number key pressed over it in the
// inventory, from the slot that matches it best (the name too, before just the item); slots that already hold theirs
// stay.
fn equip(game: &mut Game<'_>, l: &Loadout) {
    if !util::inventory_idle(game) {
        return;
    }
    let mut done = [false; 36];
    let best = |game: &Game<'_>, want: &Slot, done: &[bool; 36]| {
        (0..36)
            .filter(|&k| !done[k])
            .filter(|&k| !(k < 9 && l.hotbar.get(k).and_then(|w| w.as_ref()).is_some_and(|w| score(w, game.inventory[k].as_ref()) == 2)))
            .map(|k| (score(want, game.inventory[k].as_ref()), k))
            .filter(|&(s, _)| s > 0)
            .max_by_key(|&(s, k)| (s, std::cmp::Reverse(k)))
            .map(|(_, k)| k)
    };
    for (i, want) in l.hotbar.iter().enumerate().take(9) {
        let Some(want) = want else {
            continue;
        };
        if score(want, game.inventory[i].as_ref()) == 2 {
            done[i] = true;
            continue;
        }
        let Some(from) = best(game, want, &done) else {
            continue;
        };
        game.remote_menu_ops(&[(Some(from), i as i32, Input::Swap)]);
        done[i] = true;
    }
    if let Some(want) = &l.offhand
        && score(want, game.offhand.as_ref()) < 2
        && let Some(from) = best(game, want, &done)
    {
        game.remote_menu_ops(&[(Some(from), 40, Input::Swap)]);
    }
}

fn equip_row(game: &mut Game<'_>, row: usize) {
    let list = loadouts();
    if let Some(l) = list.get(row) {
        equip(game, l);
        game.titles.set_overlay(format!("Equipped {}", l.name).into(), false);
    }
}

// The panel's measures, as Duelkit's: rows 40 apart, each a name over a hotbar strip 182 by 22 with the off hand's
// slot to its left.
const ROW: f32 = 40.0;
const BAR_W: f32 = 182.0;

fn top(h: f32, count: usize) -> f32 {
    (h / 2.0 - count as f32 * ROW / 2.0 - 10.0).max(20.0)
}

fn bar_x(w: f32) -> f32 {
    w / 2.0 - BAR_W / 2.0 + 12.0
}

fn row_at(w: f32, h: f32, count: usize, (mx, my): (f32, f32)) -> Option<usize> {
    let (top, x) = (top(h, count), bar_x(w));
    (0..count).find(|&i| {
        let y = top + i as f32 * ROW;
        mx >= x - 26.0 && mx <= x + BAR_W && my >= y && my <= y + ROW - 4.0
    })
}

// The Save current button, under the rows.
fn save_button(w: f32, h: f32, count: usize) -> [f32; 4] {
    [w / 2.0 - 40.0, top(h, count) + count as f32 * ROW + 2.0, 80.0, 14.0]
}

fn inside(r: [f32; 4], (x, y): (f32, f32)) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

// The screen's size in the HUD's pixels, as the panel was last drawn.
static VIEW: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));

fn view() -> (f32, f32) {
    *VIEW.lock().unwrap_or_else(|e| e.into_inner())
}

// Loadouts' keys: the Menu key held opens the panel (the pointer free); let go over a row, it equips that one. A
// hotbar number key equips that loadout instead.
fn loadout_key(game: &mut Game<'_>, key: &str, pressed: bool) {
    let Some(m) = module("Loadouts") else {
        duel(|d| d.listing = false);
        return;
    };
    if super::sunset::key_from_text(m.t("Menu key")).as_deref() == Some(key) {
        let was = duel(|d| d.listing);
        let now = pressed && game.screen == Screen::Playing && !super::sunset::gui_open();
        if now == was {
            return;
        }
        duel(|d| d.listing = now);
        if now {
            game.keys.clear();
            game.capture(false);
            return;
        }
        let (w, h) = view();
        if let Some(row) = row_at(w, h, loadouts().len(), game.cursor) {
            equip_row(game, row);
        }
        if game.screen == Screen::Playing {
            game.capture(true);
        }
        return;
    }
    if !pressed || !duel(|d| d.listing) {
        return;
    }
    let Some(n) = (0..9).find(|i| game.settings.keys.get(&format!("key.hotbar.{}", i + 1)) == key) else {
        return;
    };
    if n < loadouts().len() {
        let back = game.selected;
        duel(|d| d.undo = Some((n, back)));
        equip_row(game, n);
    }
}

// A mouse press while the panel is open: a row clicked equips it, right-clicked deletes it; Save current keeps what
// you hold as "Loadout N". Every press stops here while it is open.
pub(super) fn loadout_press(game: &mut Game<'_>, right: bool) -> bool {
    if !duel(|d| d.listing) {
        return false;
    }
    let (w, h) = view();
    let count = loadouts().len();
    if !right && inside(save_button(w, h, count), game.cursor) {
        let line = capture(game, &format!("Loadout {}", count + 1));
        with(|s| s.loadouts.push(line));
        store(game);
    } else if let Some(row) = row_at(w, h, count, game.cursor) {
        if right {
            with(|s| {
                s.loadouts.remove(row);
            });
            store(game);
        } else {
            equip_row(game, row);
        }
    }
    true
}

// The panel while the Menu key is held: a title, each loadout's number and name over a hotbar of its items with the
// off hand's to the left (the row under the pointer lit), Save current, and how to use it.
pub(super) fn draw_loadouts(game: &Game<'_>, w: f32, h: f32, out: &mut Vec<Shape>) {
    if !duel(|d| d.listing) || module("Loadouts").is_none() {
        return;
    }
    *VIEW.lock().unwrap_or_else(|e| e.into_inner()) = (w, h);
    let list = loadouts();
    let (top, x) = (top(h, list.len()), bar_x(w));
    let centred = |out: &mut Vec<Shape>, text: &str, y: f32, colour: u32| {
        out.text(text, [(w - game.native_text_width(text)) / 2.0, y], colour);
    };
    centred(out, if list.is_empty() { "No loadouts yet" } else { "Loadouts" }, top - 14.0, 0xFFFFFF);
    let hover = row_at(w, h, list.len(), game.cursor);
    // A vanilla-like slot: dark edge, grey face.
    let slot = |out: &mut Vec<Shape>, x: f32, y: f32| {
        out.fill([x, y, 22.0, 22.0], 0xFF1E_1E1E);
        out.fill([x + 1.0, y + 1.0, 20.0, 20.0], 0xFF8B_8B8B);
        out.fill([x + 2.0, y + 2.0, 18.0, 18.0], 0xFF37_3737);
    };
    for (i, l) in list.iter().enumerate() {
        let y = top + i as f32 * ROW;
        if hover == Some(i) {
            out.fill([x - 28.0, y - 2.0, BAR_W + 30.0, ROW - 2.0], 0x40FF_FFFF);
        }
        let name = if i < 9 { format!("{}  {}", i + 1, l.name) } else { l.name.clone() };
        out.text(&name, [x, y], if hover == Some(i) { 0xFFFF55 } else { 0xE0E0E0 });
        out.fill([x, y + 10.0, BAR_W, 22.0], 0xFF1E_1E1E);
        for k in 0..9 {
            slot(out, x + k as f32 * 20.0, y + 10.0);
        }
        for (k, item) in l.hotbar.iter().take(9).enumerate() {
            if let Some(item) = item {
                out.push(Shape::Item(item.item.as_str().into(), [x + 3.0 + k as f32 * 20.0, y + 13.0], 16.0));
            }
        }
        slot(out, x - 26.0, y + 10.0);
        if let Some(item) = &l.offhand {
            out.push(Shape::Item(item.item.as_str().into(), [x - 23.0, y + 13.0], 16.0));
        }
    }
    let b = save_button(w, h, list.len());
    out.push(Shape::Round(b, 3.0, if inside(b, game.cursor) { 0xC050_5050 } else { 0xC030_3030 }));
    centred(out, "Save current", b[1] + 3.0, 0xFFFFFF);
    centred(out, "Release K over one to equip  \u{b7}  right-click deletes", h - 46.0, 0xA0A0A0);
}

// .loadout save [name] | delete <n|name> | list | <n|name>
pub(super) fn loadout_command(game: &mut Game<'_>, args: &[&str]) {
    let list = loadouts();
    match (args.get(1).map(|a| a.to_ascii_lowercase()).as_deref(), args.get(2..).map(|a| a.join(" "))) {
        (Some("save" | "add"), name) => {
            let name = name.filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Loadout {}", list.len() + 1));
            let line = capture(game, &name);
            with(|s| {
                s.loadouts.retain(|l| parse(l).is_none_or(|l| !l.name.eq_ignore_ascii_case(&name)));
                s.loadouts.push(line);
            });
            store(game);
            say(game, &format!("Saved loadout {name}"));
        }
        (Some("delete" | "del" | "remove" | "rm"), Some(what)) => match find_loadout(&list, &what) {
            Some(i) => {
                let name = list[i].name.clone();
                with(|s| s.loadouts.retain(|l| parse(l).is_none_or(|l| l.name != name)));
                store(game);
                say(game, &format!("Deleted loadout {name}"));
            }
            None => say(game, &format!("No loadout {what}")),
        },
        (Some("list") | None, _) => {
            if list.is_empty() {
                say(game, "No loadouts yet. Use .loadout save <name>");
            }
            for (i, l) in list.iter().enumerate() {
                say(game, &format!("{}: {}", i + 1, l.name));
            }
        }
        (Some(_), _) => {
            let what = args[1..].join(" ");
            match find_loadout(&list, &what) {
                Some(i) => equip_row(game, i),
                None => say(game, &format!("No loadout {what}")),
            }
        }
    }
}
