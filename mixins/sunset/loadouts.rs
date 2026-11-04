// Loadouts, implemented for Sunset and grown into inventory profiles: a
// loadout keeps the hotbar and the off hand, with Armour what you wear, with Full layout the main inventory too, and
// the Locked slots of the time. Equipping puts every saved item back where it was; Locked slots are left alone by every
// Sunset module that moves items; Refill broken tools puts the same tool from the inventory in the hotbar slot whose
// tool just broke; Auto hotbar keeps hotbar slots filled by rules. Loadouts are shared as files
// through the exports and imports folders, as profiles are.
use super::sunset::{Canvas, Category, Module, Shape, say, with};
use super::sunset_util as util;
use super::{Game, Screen};
use horizon_client::items::ItemStack;
use horizon_client::menu::Input;
use std::sync::Mutex;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Loadouts", Category::Player, "Hold K for your saved inventory layouts.")
            .text("Menu key", "K")
            .bool("Armour", true)
            .bool("Full layout", false)
            .text("Locked slots", "")
            .bool("Refill broken tools", true)
            .bool("Auto hotbar", false)
            .pick("Hotbar mode", &["Inventory open", "Always"])
            .text("Hotbar rules", "1:sword,2:blocks,9:ender_pearl")
            .num("Hotbar delay (ms)", 110.0, 0.0, 300.0, 10.0),
    ]
}

#[derive(Default)]
struct Panel {
    // The Menu key is held; a loadout's number key selected its slot, to undo (that slot, the one before).
    listing: bool,
    undo: Option<(usize, usize)>,
    // Refill broken tools: each hotbar slot's tool last tick, and whether it was a use from breaking.
    tools: [Option<(String, bool)>; 9],
    // Auto hotbar: not before this tick again.
    hotbar_next: u64,
}

static PANEL: Mutex<Option<Panel>> = Mutex::new(None);

fn panel<R>(f: impl FnOnce(&mut Panel) -> R) -> R {
    f(PANEL.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Panel::default))
}

fn module() -> Option<Module> {
    with(|s| s.module("Loadouts").filter(|m| m.on).cloned())
}

// The panel is open (Hold to Eat leaves the number keys to it then).
pub(super) fn listing() -> bool {
    panel(|p| p.listing)
}

// "1 2 10" (or with commas): inventory slots by number, the hotbar 1 to 9 and the main inventory 10 to 36.
fn slot_numbers(text: &str) -> [bool; 36] {
    let mut out = [false; 36];
    for n in text.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse::<usize>().ok()) {
        if (1..=36).contains(&n) {
            out[n - 1] = true;
        }
    }
    out
}

// The inventory slots (0-8 hotbar, 9-35 main) no Sunset module moves items into or out of, while Loadouts is on.
pub(super) fn locked_slots() -> [bool; 36] {
    with(|s| s.module("Loadouts").filter(|m| m.on).map(|m| slot_numbers(m.t("Locked slots")))).unwrap_or([false; 36])
}

pub(super) fn locked(slot: usize) -> bool {
    slot < 36 && locked_slots()[slot]
}

// Each loadout a line "name|hotbar|offhand|armour|main|locked": nine hotbar slots, then four armour slots (head to
// feet) and the 27 of the main inventory (both empty when not kept), comma separated, each "-" (left alone) or an
// item id with its custom name after a "~" when it had one (names %-escaped); then the locked slot numbers. The profile's
// lines have the first three fields only. Kept in loadouts.txt in the mod's data folder.
struct Slot {
    item: String,
    name: Option<String>,
}

struct Loadout {
    name: String,
    hotbar: Vec<Option<Slot>>,
    offhand: Option<Slot>,
    armour: Vec<Option<Slot>>,
    main: Vec<Option<Slot>>,
    // None in older profile lines: equipping one leaves the locked slots as they are.
    locked: Option<String>,
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
    let slots = |s: Option<&str>| s.filter(|s| !s.is_empty()).map_or_else(Vec::new, |s| s.split(',').map(slot).collect());
    let hotbar = slots(parts.next());
    let offhand = slot(parts.next()?);
    let armour = slots(parts.next());
    let main = slots(parts.next());
    let locked = parts.next().map(str::to_owned);
    Some(Loadout { name, hotbar, offhand, armour, main, locked })
}

// A loadout of what you have now, named: the hotbar and the off hand, and what Armour and Full layout keep.
fn capture(game: &Game<'_>, m: &Module, name: &str) -> String {
    let slot = |s: Option<&ItemStack>| match s {
        None => "-".to_owned(),
        Some(s) => match s.custom_name() {
            Some(n) => format!("{}~{}", s.item, escape(&n)),
            None => s.item.to_string(),
        },
    };
    let row = |slots: &mut dyn Iterator<Item = Option<&ItemStack>>| slots.map(slot).collect::<Vec<_>>().join(",");
    let hotbar = row(&mut (0..9).map(|i| game.inventory[i].as_ref()));
    let armour = if m.b("Armour") { row(&mut game.armor.iter().map(Option::as_ref)) } else { String::new() };
    let main = if m.b("Full layout") { row(&mut (9..36).map(|i| game.inventory[i].as_ref())) } else { String::new() };
    let locked = slot_numbers(m.t("Locked slots"));
    let locked = (0..36).filter(|&i| locked[i]).map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(" ");
    format!("{}|{hotbar}|{}|{armour}|{main}|{locked}", escape(name), slot(game.offhand.as_ref()))
}

fn loadouts() -> Vec<Loadout> {
    with(|s| s.loadouts.iter().filter_map(|l| parse(l)).collect())
}

fn file(game: &Game<'_>) -> std::path::PathBuf {
    game.native_data_dir(super::sunset::ID).join("loadouts.txt")
}

// Read once with the config: the file's loadouts, or (the first time) the ones the config kept, written to it.
pub(super) fn load(game: &Game<'_>) {
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
fn score(want: &Slot, have: Option<&ItemStack>) -> u8 {
    match have {
        Some(h) if *h.item == *want.item => {
            if want.name.is_none() || h.custom_name().as_deref() == want.name.as_deref() { 2 } else { 1 }
        }
        _ => 0,
    }
}

// What the loadout wants in inventory slot `k` (0-8 hotbar, 9-35 main), if anything.
fn wanted(l: &Loadout, k: usize) -> Option<&Slot> {
    if k < 9 { l.hotbar.get(k) } else { l.main.get(k - 9) }.and_then(Option::as_ref)
}

// Puts each saved item in its slot from the inventory slot that matches it best (the name too, before just the item),
// slots that already hold theirs staying: the hotbar and the off hand with the clicks of a number key pressed over it
// in the inventory, using slot swaps; armour and the main inventory picked up, put down and the stack it met put back.
// Then the loadout's locked slots are the ones locked (a loadout with no lock list leaves them).
fn equip(game: &mut Game<'_>, l: &Loadout) {
    if !util::inventory_idle(game) {
        return;
    }
    let mut done = [false; 36];
    let best = |game: &Game<'_>, want: &Slot, done: &[bool; 36]| {
        (0..36)
            .filter(|&k| !done[k])
            .filter(|&k| !wanted(l, k).is_some_and(|w| score(w, game.inventory[k].as_ref()) == 2))
            .map(|k| (score(want, game.inventory[k].as_ref()), k))
            .filter(|&(s, _)| s > 0)
            .max_by_key(|&(s, k)| (s, std::cmp::Reverse(k)))
            .map(|(_, k)| k)
    };
    let place = |game: &mut Game<'_>, i: usize, done: &mut [bool; 36]| {
        if let Some(want) = wanted(l, i)
            && score(want, game.inventory[i].as_ref()) < 2
            && let Some(from) = best(game, want, done)
        {
            if i < 9 {
                game.remote_menu_ops(&[(Some(from), i as i32, Input::Swap)]);
            } else {
                game.remote_menu_ops(&[(Some(from), 0, Input::Pickup), (Some(i), 0, Input::Pickup), (Some(from), 0, Input::Pickup)]);
            }
        }
        done[i] = wanted(l, i).is_some();
    };
    for i in 0..9 {
        place(game, i, &mut done);
    }
    // The off hand and the armour before the main inventory takes its pick.
    if let Some(want) = &l.offhand
        && score(want, game.offhand.as_ref()) < 2
        && let Some(from) = best(game, want, &done)
    {
        game.remote_menu_ops(&[(Some(from), 40, Input::Swap)]);
    }
    for (a, want) in l.armour.iter().enumerate().take(4) {
        if let Some(want) = want
            && score(want, game.armor[a].as_ref()) < 2
            && let Some(from) = best(game, want, &done)
        {
            game.remote_menu_ops(&[(Some(from), 0, Input::Pickup), (Some(500 + a), 0, Input::Pickup), (Some(from), 0, Input::Pickup)]);
        }
    }
    for i in 9..36 {
        place(game, i, &mut done);
    }
    if let Some(locked) = &l.locked {
        super::sunset::set_setting(game, "loadouts.lockedslots", locked);
    }
}

fn equip_row(game: &mut Game<'_>, row: usize) {
    let list = loadouts();
    if let Some(l) = list.get(row) {
        equip(game, l);
        game.titles.set_overlay(format!("Equipped {}", l.name).into(), false);
    }
}

// Refill broken tools: a hotbar slot whose tool was a use from breaking last tick and is empty now gets the same tool
// from the inventory (the least worn), swapped in as a number key over it would.
pub(super) fn tick(game: &mut Game<'_>) {
    if let Some((from, back)) = panel(|p| p.undo.take())
        && game.selected == from
    {
        super::sunset::hold(game, back);
    }
    if let Some(m) = module().filter(|m| m.b("Auto hotbar")) {
        auto_hotbar(game, &m);
    }
    let Some(m) = module().filter(|m| m.b("Refill broken tools")) else {
        panel(|p| p.tools = Default::default());
        return;
    };
    let now: [Option<(String, bool)>; 9] = std::array::from_fn(|i| {
        let s = game.inventory[i].as_ref()?;
        let max = game.res.items().get(&s.item).map_or(0, |i| i.max_damage);
        (max > 0).then(|| (s.item.to_string(), max.saturating_sub(s.damage) <= 2))
    });
    let was = panel(|p| std::mem::replace(&mut p.tools, now.clone()));
    if !util::inventory_idle(game) {
        return;
    }
    let locked = slot_numbers(m.t("Locked slots"));
    for (i, (before, after)) in was.iter().zip(&now).enumerate() {
        let (Some((item, true)), None) = (before, after) else {
            continue;
        };
        let spare = (9..36)
            .filter(|&k| !locked[k])
            .filter_map(|k| game.inventory[k].as_ref().filter(|s| *s.item == **item).map(|s| (s.damage, k)))
            .min();
        if let Some((_, k)) = spare
            && !locked[i]
        {
            game.remote_menu_ops(&[(Some(k), i as i32, Input::Swap)]);
            panel(|p| p.tools[i] = None);
        }
    }
}

// What an Auto hotbar rule asks for: a kind (`sword`, `axe`, `pickaxe`, `shovel`, `hoe`, `blocks`, `food`) or an item
// name; for a weapon or tool kind the best of it (attack damage, then durability), else the biggest stack.
fn rule_score(game: &Game<'_>, rule: &str, s: &ItemStack) -> Option<f64> {
    let info = game.res.items().get(&s.item);
    let tag = |t: &str| game.res.items().item_tags.get(t).is_some_and(|set| set.contains(&*s.item));
    let tool = match rule {
        "sword" | "axe" | "pickaxe" | "shovel" | "hoe" => tag(&format!("minecraft:{rule}s")),
        "blocks" => return info.and_then(|i| i.block).is_some().then_some(f64::from(s.count)),
        "food" => return info.is_some_and(|i| i.food.is_some()).then_some(f64::from(s.count)),
        _ => return util::listed(rule, &s.item).then_some(f64::from(s.count)),
    };
    let info = info.filter(|_| tool)?;
    Some(f64::from(info.attack_damage) * 10_000.0 + f64::from(info.max_damage) - f64::from(s.damage))
}

// Auto hotbar: each rule `slot:what` (slots 1 to 9) gets the best match from the inventory
// (another hotbar slot no rule wants it for, or the main inventory) swapped in, one swap each Hotbar delay, with
// your inventory open (Inventory open) or with no screen (Always). Locked slots are left alone.
fn auto_hotbar(game: &mut Game<'_>, m: &Module) {
    let open = game.screen == Screen::Inventory;
    let ready = if m.p("Hotbar mode") == "Always" { util::inventory_idle(game) } else { open };
    if !ready || game.ticks < panel(|p| p.hotbar_next) {
        return;
    }
    let rules: Vec<(usize, String)> = m
        .t("Hotbar rules")
        .split(',')
        .filter_map(|r| {
            let (slot, what) = r.split_once(':')?;
            let slot = slot.trim().parse::<usize>().ok().filter(|n| (1..=9).contains(n))? - 1;
            Some((slot, util::short(what.trim()).to_owned()))
        })
        .collect();
    let locked = locked_slots();
    let claimed = |k: usize, s: &ItemStack| rules.iter().any(|(slot, rule)| *slot == k && rule_score(game, rule, s).is_some());
    for (slot, rule) in &rules {
        if locked[*slot] {
            continue;
        }
        let have = game.inventory[*slot].as_ref().and_then(|s| rule_score(game, rule, s));
        let best = (0..36)
            .filter(|&k| k != *slot && !locked[k])
            .filter_map(|k| {
                let s = game.inventory[k].as_ref()?;
                (k >= 9 || !claimed(k, s)).then_some(())?;
                Some((rule_score(game, rule, s)?, k))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((score, k)) = best
            && have.is_none_or(|h| score > h)
        {
            game.remote_menu_ops(&[(Some(k), *slot as i32, Input::Swap)]);
            let wait = (m.n("Hotbar delay (ms)") / 50.0).round() as u64;
            panel(|p| p.hotbar_next = game.ticks + wait.max(1));
            return;
        }
    }
}

// The panel's measures, as follows: rows 40 apart, each a name over a hotbar strip 182 by 22 with the off hand's
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
pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    let Some(m) = module() else {
        panel(|p| p.listing = false);
        return;
    };
    if super::sunset::key_from_text(m.t("Menu key")).as_deref() == Some(key) {
        let was = listing();
        let now = pressed && game.screen == Screen::Playing && !super::sunset::gui_open();
        if now == was {
            return;
        }
        panel(|p| p.listing = now);
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
    if !pressed || !listing() {
        return;
    }
    let Some(n) = (0..9).find(|i| game.settings.keys.get(&format!("key.hotbar.{}", i + 1)) == key) else {
        return;
    };
    if n < loadouts().len() {
        let back = game.selected;
        panel(|p| p.undo = Some((n, back)));
        equip_row(game, n);
    }
}

// A mouse press while the panel is open: a row clicked equips it, right-clicked deletes it; Save current keeps what
// you have as "Loadout N". Every press stops here while it is open.
pub(super) fn press(game: &mut Game<'_>, right: bool) -> bool {
    let Some(m) = module().filter(|_| listing()) else {
        return false;
    };
    let (w, h) = view();
    let count = loadouts().len();
    if !right && inside(save_button(w, h, count), game.cursor) {
        let line = capture(game, &m, &format!("Loadout {}", count + 1));
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

// The panel while the Menu key is held: a title, each loadout's number and name (and what more it keeps) over a hotbar
// of its items with the off hand's to the left (the row under the pointer lit), Save current, and how to use it.
pub(super) fn draw(game: &Game<'_>, w: f32, h: f32, out: &mut Vec<Shape>) {
    if !listing() || module().is_none() {
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
        let more: Vec<&str> = [("armour", !l.armour.is_empty()), ("inventory", !l.main.is_empty()), ("locked slots", l.locked.as_ref().is_some_and(|l| !l.is_empty()))]
            .into_iter()
            .filter_map(|(what, kept)| kept.then_some(what))
            .collect();
        if !more.is_empty() {
            let text = more.join(" \u{b7} ");
            out.text(&text, [x + BAR_W - game.native_text_width(&text), y], 0xA0A0A0);
        }
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

// .loadout save [name] | delete <n|name> | list | export <n|name> | import <file> | <n|name>
pub(super) fn command(game: &mut Game<'_>, args: &[&str]) {
    let list = loadouts();
    let m = with(|s| s.module("Loadouts").cloned());
    match (args.get(1).map(|a| a.to_ascii_lowercase()).as_deref(), args.get(2..).map(|a| a.join(" "))) {
        (Some("save" | "add"), name) => {
            let Some(m) = m else {
                return;
            };
            let name = name.filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Loadout {}", list.len() + 1));
            let line = capture(game, &m, &name);
            keep(game, &name, line);
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
        (Some("export"), Some(what)) => match find_loadout(&list, &what) {
            Some(i) => {
                let line = with(|s| s.loadouts.iter().find(|l| parse(l).is_some_and(|l| l.name == list[i].name)).cloned());
                let done = super::sunset_profiles::name(&list[i].name).and_then(|file| {
                    let store = super::sunset_profiles::Store::new(game.native_data_dir(super::sunset::ID))?;
                    store.share(&format!("{file}.loadout"), &format!("{}\n", line.unwrap_or_default()))
                });
                match done {
                    Ok(path) => say(game, &format!("Loadout {} is in {}", list[i].name, path.display())),
                    Err(e) => say(game, &format!("Loadout: {e}.")),
                }
            }
            None => say(game, &format!("No loadout {what}")),
        },
        (Some("import"), Some(what)) => {
            let read = super::sunset_profiles::name(&what).and_then(|file| {
                let store = super::sunset_profiles::Store::new(game.native_data_dir(super::sunset::ID))?;
                store.shared(&format!("{file}.loadout"))
            });
            match read.map(|text| text.lines().find(|l| parse(l).is_some()).map(str::to_owned)) {
                Ok(Some(line)) => {
                    let name = parse(&line).map(|l| l.name).unwrap_or_default();
                    keep(game, &name, line);
                    say(game, &format!("Imported loadout {name}"));
                }
                Ok(None) => say(game, &format!("Loadout: {what} holds no loadout.")),
                Err(e) => say(game, &format!("Loadout: {e}.")),
            }
        }
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

// A loadout kept under its name, in place of one of the same name.
fn keep(game: &Game<'_>, name: &str, line: String) {
    with(|s| {
        s.loadouts.retain(|l| parse(l).is_none_or(|l| !l.name.eq_ignore_ascii_case(name)));
        s.loadouts.push(line);
    });
    store(game);
}
