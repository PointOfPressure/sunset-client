// Runtime part (see runtime.rs): the click GUI: its state, layout and hit testing.
use crate::game::sunset_compat::SunsetGame as _;

use super::{Game, Screen};
use super::sunset_text_edit::{Draft, EditKey};
use std::sync::atomic::Ordering;
use super::sunset::*;

// ---------------------------------------------------------------- click GUI
//
// Sunset's style: every category as a panel at once, over the world. Left
// click turns a module on or off, right click opens its settings under it;
// headers drag the panels (right click folds one). Numbers are sliders. The
// game keeps running (single player does not pause), the pointer is free and
// the window's keys and clicks come here while it is open.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Panel {
    // Where it is drawn this frame (hit tests read it too): worked out from `home` by the layout (draw_gui), never
    // kept.
    pub(super) x: f32,
    pub(super) y: f32,
    // Its place of your own, as saved: x spaced for the Panel width you asked for, and y. A window too small for it
    // (F11, a resize, a minimise on the way) draws it elsewhere without moving this, so it comes back where it was;
    // only a drag (or a new Panel width) changes it. x is negative until the first frame places it.
    pub(super) home: [f32; 2],
    pub(super) collapsed: bool,
    pub(super) scroll: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Drag {
    Panel(usize, f32, f32),
    // The row and scale at the press: a setting that resizes the GUI must not move the slider under the pointer.
    Slider(usize, usize, [f32; 4], f32),
    // The settings window by its title, where in it the pointer took it.
    Window(f32, f32),
    // A HUD element (Coordinates, Compass, Armor HUD), where in it the pointer took it (in the HUD's own units).
    Hud(&'static str, f32, f32),
}

#[derive(Clone, Debug)]
pub(super) struct Gui {
    pub(super) profile_menu: Option<super::sunset_profile_menu::Menu>,
    pub(super) open: bool,
    // Esc or the menu key closes it at the next frame, so the same key
    // press can't also reach the game (Esc would open the pause menu).
    pub(super) closing: bool,
    pub(super) panels: Vec<Panel>,
    // The Panel width the panels' homes are spaced for (0 before the first frame): a new one spreads or closes them up.
    pub(super) laid_for: f32,
    pub(super) expanded: Vec<usize>,
    pub(super) drag: Option<Drag>,
    // The height of the GUI the panels last drew in.
    pub(super) view_h: f32,
    // Typed letters: only the modules whose names contain them show.
    pub(super) search: String,
    // Ctrl+A: the next letter replaces the whole search, Backspace clears it.
    pub(super) select_all: bool,
    // The text editor over the panels (a left click on a text setting), the GUI's width when it last drew, and
    // whether the key just pressed was the editor's already (from the window's own event, with its text).
    pub(super) editor: Option<Editor>,
    // A module's settings in their own window (ClickGUI Settings: Window).
    pub(super) window: Option<Win>,
    pub(super) view_w: f32,
    pub(super) typed: bool,
    // Ctrl or Cmd held, as the window last said.
    shortcut: bool,
}

// A text setting being edited: the setting changes only when Apply is clicked (or Enter pressed).
#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub(super) module: usize,
    pub(super) setting: usize,
    // A value from .set or the config that the editor can't hold (too long, more than one line) is shown, not cut.
    pub(super) draft: Result<Draft, &'static str>,
    // A number typed in its own slider row, instead of the text editor's box over the panels.
    pub(super) inline: bool,
    // A list of blocks, items or entities: picked from the registries, the draft being the search.
    pub(super) list: Option<super::sunset_picker::Picker>,
}

pub(super) fn editor_rect(w: f32, h: f32) -> [f32; 4] {
    let width = (w - 8.0).clamp(1.0, 320.0);
    [(w - width) / 2.0, ((h - 76.0) / 2.0).max(0.0), width, 76.0]
}

// Apply and Cancel.
pub(super) fn editor_buttons(r: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let width = ((r[2] - 18.0) / 2.0).clamp(1.0, 56.0);
    ([r[0] + 6.0, r[1] + 54.0, width, 15.0], [r[0] + 12.0 + width, r[1] + 54.0, width, 15.0])
}

// The editor's draft into its setting, and the editor closed.
// A number goes in rounded to its step and kept in its range; what is not a number keeps the field open, in red.
pub(super) fn apply_editor(s: &mut State) {
    let Some(mut edit) = s.gui.editor.take() else {
        return;
    };
    // A picker changed its list as it was clicked: Enter only closes it.
    let (Ok(draft), None) = (&mut edit.draft, &edit.list) else {
        return;
    };
    match s.modules.get_mut(edit.module).and_then(|m| m.settings.get_mut(edit.setting)).map(|st| &mut st.value) {
        Some(Value::Text(t)) => *t = draft.text().to_owned(),
        Some(Value::Num { v, min, max, step }) => match draft.text().trim().parse::<f64>() {
            Ok(n) if n.is_finite() => *v = round_to(n, *step).clamp(*min, *max),
            _ => {
                draft.error = Some("not a number");
                s.gui.editor = Some(edit);
            }
        },
        _ => {}
    }
}

pub(super) fn edit_key(name: &str) -> Option<EditKey> {
    Some(match name {
        "key.keyboard.left" => EditKey::Left,
        "key.keyboard.right" => EditKey::Right,
        "key.keyboard.home" => EditKey::Home,
        "key.keyboard.end" => EditKey::End,
        "key.keyboard.backspace" => EditKey::Backspace,
        "key.keyboard.delete" => EditKey::Delete,
        _ => return None,
    })
}

// The window's key events while the editor is open: the text the keyboard layout makes goes into the draft (a real
// keyboard's keys come here first, then to gui_key, which leaves them be).
pub(super) fn text_event(event: &winit::event::WindowEvent) {
    use winit::event::{ElementState, WindowEvent};
    use winit::keyboard::{Key, NamedKey};
    with(|s| {
        if let WindowEvent::ModifiersChanged(m) = event {
            s.gui.shortcut = m.state().control_key() && !m.state().alt_key() || m.state().super_key();
        }
        let WindowEvent::KeyboardInput { event, is_synthetic: false, .. } = event else {
            return;
        };
        if event.state != ElementState::Pressed || !s.gui.open || s.gui.editor.is_none() {
            return;
        }
        s.gui.typed = true;
        let key = match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                s.gui.editor = None;
                return;
            }
            Key::Named(NamedKey::Enter) => {
                apply_editor(s);
                return;
            }
            Key::Named(NamedKey::ArrowLeft) => Some(EditKey::Left),
            Key::Named(NamedKey::ArrowRight) => Some(EditKey::Right),
            Key::Named(NamedKey::Home) => Some(EditKey::Home),
            Key::Named(NamedKey::End) => Some(EditKey::End),
            Key::Named(NamedKey::Backspace) => Some(EditKey::Backspace),
            Key::Named(NamedKey::Delete) => Some(EditKey::Delete),
            Key::Character(c) if s.gui.shortcut && c.eq_ignore_ascii_case("a") => Some(EditKey::SelectAll),
            _ => None,
        };
        let shortcut = s.gui.shortcut;
        let Some(Editor { draft: Ok(draft), list, .. }) = s.gui.editor.as_mut() else {
            return;
        };
        match (key, event.text.as_deref()) {
            (Some(k), _) => draft.key(k),
            (None, Some(text)) if !shortcut && !text.chars().any(char::is_control) => draft.insert(text),
            _ => {}
        }
        // A new search starts at the top of the list.
        if let Some(picker) = list {
            picker.scroll = 0.0;
        }
    });
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            profile_menu: None,
            open: false,
            closing: false,
            select_all: false,
            editor: None,
            window: None,
            view_w: 0.0,
            typed: false,
            shortcut: false,
            panels: (0..CATEGORIES.len())
                .map(|_| Panel { x: -1.0, y: 32.0, home: [-1.0, 32.0], collapsed: false, scroll: 0.0 })
                .collect(),
            laid_for: 0.0,
            expanded: Vec::new(),
            drag: None,
            view_h: 240.0,
            search: String::new(),
        }
    }
}

// Row heights, Comfortable or Compact (ClickGUI's Spacing): a header, a row, and a slider, whose track is under its text.
fn spaced(comfortable: f32, compact: f32) -> f32 {
    if LOOK[10].load(Ordering::Relaxed) == 0 { comfortable } else { compact }
}
pub(super) fn head_h() -> f32 {
    spaced(16.0, 13.0)
}
pub(super) fn row_hh() -> f32 {
    spaced(14.0, 11.0)
}
fn slider_h() -> f32 {
    spaced(19.0, 15.0)
}
// To the right of what the rows draw (the scroll marker sits there).
pub(super) const RIGHT_PAD: f32 = 7.0;
// The panel's own space under its last row: the last row's text is as far from the bottom as the header's from the top.
pub(super) fn body_pad() -> f32 {
    (head_h() - row_hh()) / 2.0
}
// Text is this far down a row of its height.
pub(super) const fn text_y(h: f32) -> f32 {
    (h - 8.0) / 2.0
}
// A row of panels still narrow enough to read, in GUI pixels at scale 1: GUI scale stops where this fills the window.
pub(super) const MIN_ROW: f32 = 4.0 + 5.0 * 70.0 + 4.0 * 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Hit {
    Header(usize),
    Module(usize),
    Bind(usize),
    Setting(usize, usize),
    // The settings window: anywhere on it, its title (to drag it) and its close button.
    WinBody,
    WinTitle,
    WinClose,
    // The Dashboard style's own parts.
    Dash(super::sunset_dashboard::Part),
}

// What the Dashboard style reads of the menu.
pub(super) struct DashMenu<'a> {
    pub(super) search: &'a str,
    pub(super) select_all: bool,
    pub(super) view_w: f32,
    pub(super) view_h: f32,
}

pub(super) fn dash_menu(s: &State) -> DashMenu<'_> {
    DashMenu { search: &s.gui.search, select_all: s.gui.select_all, view_w: s.gui.view_w, view_h: s.gui.view_h }
}

pub(super) fn dash_search_clear(s: &mut State) {
    s.gui.search.clear();
    s.gui.select_all = false;
}


// Other names a module is known by, so the search finds it under either.
pub(super) fn aliases(name: &str) -> &'static str {
    match name {
        "Velocity" => "knockback delay knockbackdelay",
        "BlockIn" => "surround block in feet obsidian",
        "Fast Break" => "speed mine speedmine fast mine fastmine",
        "Hit Through Walls" => "attack through blocks wall hits wallhits through walls ghost hand hits",
        "PvP Bot" => "sword pvp bot swordpvpbot crystal pvp bot crystalpvpbot crystal bot crystalbot",
        "Auto Web" => "cobweb placer cobwebplacer web key",
        "Shield" => "auto shield autoshield instant shield instantshield block",
        "Pearl Catch" => "auto pearl catch autopearlcatch fast pearl catch fastpearlcatch diagonal pearl catch diagonalpearlcatch",
        "Tap Use" => "key throw keythrow instant crossbow instantcrossbow key pearl keypearl key wind charge keywindcharge short pearl shortpearl auto short pearl autoshortpearl short wind charge shortwindcharge pearl wind charge pearlwindcharge pearl follow pearlfollow",
        "Eat Tap" => "hold to eat holdtoeat eat key eatkey golden apple gap",
        "Wind Jump" => "elytra wind charge elytrawindcharge wind charge aura windchargeaura deflect",
        "Fake Lag" => "fakelag choke lag delay pulse dynamic",
        "Backtrack" => "backtrack back track",
        "Aim Assist" => "silent aim silentaim hit flick hitflick",
        "No Interact" => "hit fix hitfix no miss delay nomissdelay anti action antiaction ghost place ghostplace anti trap antitrap tpa trap sign",
        "Sprint" => "keep sprint keepsprint",
        "W Tap" => "wtap w-tap s tap",
        "Jump Reset" => "auto jump reset jumpreset",
        "New Chunks" => "newer new chunks chunk trail trouser palette",
        "Ore Sim" => "ore simulation oresimulation seed xray anti xray antixray ores diamonds",
        "Rotations" => "rotation no rotate norotate yaw lock pitch lock look lock",
        "Player Activity Detector" => "online player activity detector",
        "Base Finder" => "trouser stash base hunting",
        "Stash Finder" => "stash chests base hunting",
        "Gear ESP" => "mob gear esp player gear armor armour inventory items equipment nametags",
        "Chest Swap" => "auto elytra autoelytra elytra take off takeoff firework rocket",
        "Safe Walk" => "fast bridge fastbridge edge sneak bridge",
        "Parkour" => "edge jump auto jump edge",
        "Step" => "step up stepup stairs reverse step step down stepdown",
        "ClickGUI" => "dropdown dashboard style gui menu color colour theme accent opacity width dim blur background indent scale size zoom bigger radius corner round",
        "Glide" => "slow fall slowfall",
        "Hand View" => "small hands hand sway viewmodel item position totem pop totem size hand shadow hand light animations swing animation slow swing old animations equip totem animation 1.8 animations 1.7 animations block animation blockhit sword block autoblock animation",
        "Zoom" => "wi zoom spyglass",
        "Panic" => "disable all off hide",
        "Debug Report" => "debug report bug report clipboard copy log flags anticheat alerts packets",
        "Streamer Mode" => "streamer mode name protect nameprotect hide names nick nickname privacy",
        "Client Spoofer" => "client spoofer brand spoof client brand vanilla fabric lunar client apollo badlion labymod feather forge neoforge channels",
        "Chest Stealer" => "auto steal autosteal loot",
        "Liquid Filler" => "auto drain autodrain drain bucket scoop water lava cobweb",
        "Auto Tool" => "auto pickaxe autopickaxe tool swap pickaxe below",
        "ESP" => "2d esp twodesp 2d box corners brackets health bar glow shield renderer shieldrenderer shield colors murderer finder murder mystery murdererfinder callout",
        "Nametags" => "health indicators healthindicators health colors bands",
        "Atmosphere" => "ambience fog color fog distance custom fog sky horizon",
        "Effects" => "jump circles jumpcircles trails footsteps hit effect hiteffect hit particles totem pop particles weather rain snow fireflies embers ambience",
        "Auto Gap" => "auto pot autopot splash potion healing pot",
        "Auto Replenish" => "refill potion refill potionrefill hotbar refill",
        "Inventory Manager" => "inventory manager invmanager inv manager cleaner junk drop clean sort hotbar layout duplicates auto loot autoloot loot key slots sword slot block slot",
        "Mine Through Mobs" => "no mining trace nominingtrace",
        "GUI Move" => "inventory move invmove gui walk",
        "Flight" => "fly",
        "Speed" => "bhop bunnyhop bunny hop strafe low hop",
        "Kill Aura" => "aura killaura target strafe",
        "Break Delay" => "fast break nuker delay",
        "Fast Place" => "fastplace place speed delay",
        "Fast Use" => "fastuse fastxp fast xp exp bottles throw throwables",
        "Fullbright" => "gamma night vision",
        "Criticals" => "crits",
        "Offhand" => "totem auto totem inventory totem hover totem crystal gapple double hand predict double hand mainhand totem",
        "Auto Weapon" => "shield breaker axe sword swap best weapon swap hit totem hit",
        "Auto Spear" => "spear lunge instant lunge reach poke spear reach extended reach damage indicator spear swap",
        "Elytra Target" => "elytratarget elytra target glide chase firework rocket kill aura aim",
        "Auto Mace" => "mace smash stun slam axe density breach trigger silent aim elytra takeoff hitbox swap back",
        "Crystal Aura" => "crystal macro auto hit crystal autohitcrystal auto obby crystal perfect timing",
        "Cart" => "tnt minecart cart pvp auto cart cart aura xbow cart crossbow safe cart cover refill cart refill pearl catch",
        "Anchor Aura" => "anchor macro double anchor anchor placer anchor exploder safe anchor anchor pearl catch anchorpearlcatch",
        "Anvil" => "anti anvil self anvil auto anvil",
        "Xray" => "x-ray",
        "Entity Control" => "boat fly boatfly entity speed entity fly horse ride vehicle",
        "Clear Water" => "water opacity see through transparent",
        "No Render" => "no hide fog fog water darkness blindness overlay particles particle render optimizer renderoptimizer hurt cam hurtcam scoreboard titles title hotbar text held item name",
        _ => "",
    }
}

pub(super) fn row_h(s: &State, hit: Hit) -> f32 {
    match hit {
        Hit::Setting(i, j) if matches!(s.modules[i].settings[j].value, Value::Num { .. }) => slider_h(),
        _ => row_hh(),
    }
}

// The rows of one panel before scrolling: their offset from the top of its
// body, what they are, and how tall the whole body is.
pub(super) fn panel_body(s: &State, c: usize) -> (Vec<(f32, Hit)>, f32) {
    let mut rows = Vec::new();
    let mut y = 0.0;
    let listed = s.in_gui();
    let in_panel = s.modules.iter().enumerate().filter(|(_, m)| m.category == CATEGORIES[c] && listed(m)).map(|(i, _)| i);
    // Searching: what it finds, best first, each with the settings it matched shown under it while it is closed.
    let found = (!s.gui.search.trim().is_empty()).then(|| super::sunset_search::results(&s.modules, &s.gui.search));
    let order: Vec<usize> = match &found {
        Some(found) => super::sunset_search::ranked(found, in_panel),
        None => alphabetical(&s.modules, in_panel),
    };
    for i in order {
        let m = &s.modules[i];
        rows.push((y, Hit::Module(i)));
        y += row_hh();
        if !s.gui.expanded.contains(&i)
            && let Some(hit) = found.as_ref().and_then(|f| f[i].as_ref())
        {
            for &j in hit.settings.iter().filter(|&&j| m.setting_visible(j)) {
                let hit = Hit::Setting(i, j);
                rows.push((y, hit));
                y += row_h(s, hit);
            }
        }
        if s.gui.expanded.contains(&i) {
            rows.push((y, Hit::Bind(i)));
            y += row_hh();
            for j in 0..m.settings.len() {
                if m.setting_visible(j) {
                    let hit = Hit::Setting(i, j);
                    rows.push((y, hit));
                    y += row_h(s, hit);
                }
            }
        }
    }
    (rows, y)
}

// How much of a panel's body shows: what is left of the screen under it.
pub(super) fn body_room(s: &State, c: usize) -> f32 {
    (s.gui.view_h - s.gui.panels[c].y - head_h() - 14.0).max(row_hh() * 3.0)
}

// Each row of every panel in view, top to bottom: its rectangle and what it is. One part way in view is drawn cut.
pub(super) fn panel_rows(s: &State) -> Vec<([f32; 4], Hit)> {
    if super::sunset_dashboard::on() {
        return super::sunset_dashboard::rows(s);
    }
    let mut out = Vec::new();
    for c in 0..CATEGORIES.len() {
        let p = s.gui.panels[c];
        let (rows, _) = panel_body(s, c);
        // A search leaves out the panels with nothing that matches.
        if rows.is_empty() && !s.gui.search.is_empty() {
            continue;
        }
        out.push(([p.x, p.y, panel_w(), head_h()], Hit::Header(c)));
        if p.collapsed {
            continue;
        }
        let room = body_room(s, c);
        for (off, hit) in rows {
            let top = off - super::sunset_smooth::shown(c, p.scroll);
            let rh = row_h(s, hit);
            if top + rh <= 0.0 || top >= room {
                continue;
            }
            out.push(([p.x, p.y + head_h() + top, panel_w(), rh], hit));
        }
    }
    out
}

// A row of a scrolling list, cut to the part of it in view.
pub(super) fn in_view([x, y, w, h]: [f32; 4], [top, bottom]: [f32; 2]) -> Option<[f32; 4]> {
    let (y0, y1) = (y.max(top), (y + h).min(bottom));
    (y1 > y0).then_some([x, y0, w, y1 - y0])
}

// The rows the pointer can hit: only what shows of each.
fn panel_hits(s: &State) -> Vec<([f32; 4], Hit)> {
    let mut body = super::sunset_dashboard::list_y(s).unwrap_or([f32::MIN, f32::MAX]);
    panel_rows(s)
        .into_iter()
        .filter_map(|(r, hit)| {
            if let Hit::Header(c) = hit {
                body = [r[1] + r[3], r[1] + r[3] + body_room(s, c)];
                return Some((r, hit));
            }
            in_view(r, body).map(|r| (r, hit))
        })
        .collect()
}

// What the pointer can hit: the panels' rows, then the settings window over them.
pub(super) fn gui_rows(s: &State) -> Vec<([f32; 4], Hit)> {
    let mut rows = super::sunset_dashboard::hits(s);
    rows.extend(panel_hits(s));
    rows.extend(window_hits(s));
    rows
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Win {
    pub(super) module: usize,
    // Where it is drawn this frame, from `home` (where it was opened or dragged to) kept on the screen.
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) home: [f32; 2],
    pub(super) scroll: f32,
    // As it last drew: its description's height, its rows' height and how much of them shows.
    pub(super) about_h: f32,
    pub(super) content: f32,
    pub(super) room: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum WinRow {
    Group(&'static str),
    Row(Hit),
}

pub(super) fn win_w(s: &State) -> f32 {
    (panel_w() * 1.6).max(150.0).min(s.gui.view_w - 8.0)
}

// The window's rows under its title and description, each group under its name: the settings (in the module's sections,
// or all under General), then the bind.
pub(super) fn win_body(s: &State, i: usize) -> (Vec<(f32, WinRow)>, f32) {
    let m = &s.modules[i];
    let (bind, settings): (Vec<usize>, Vec<usize>) =
        (0..m.settings.len()).filter(|&j| m.setting_visible(j)).partition(|&j| m.settings[j].name == "Bind mode");
    let mut groups: Vec<(&'static str, Vec<Hit>)> = Vec::new();
    for j in settings {
        let title = m.group_of(j).unwrap_or("General");
        match groups.last_mut() {
            Some((last, hits)) if *last == title => hits.push(Hit::Setting(i, j)),
            _ => groups.push((title, vec![Hit::Setting(i, j)])),
        }
    }
    groups.push(("Bind", std::iter::once(Hit::Bind(i)).chain(bind.into_iter().map(|j| Hit::Setting(i, j))).collect()));
    let mut rows = Vec::new();
    let mut y = 0.0;
    for (name, hits) in groups.into_iter().filter(|(_, hits)| !hits.is_empty()) {
        rows.push((y, WinRow::Group(name)));
        y += row_hh();
        for hit in hits {
            rows.push((y, WinRow::Row(hit)));
            y += row_h(s, hit);
        }
    }
    (rows, y)
}

pub(super) fn win_frame(s: &State, w: &Win) -> [f32; 4] {
    [w.x, w.y, win_w(s), head_h() + w.about_h + w.content.min(w.room) + body_pad()]
}

pub(super) fn win_close(frame: [f32; 4]) -> [f32; 4] {
    [frame[0] + frame[2] - head_h(), frame[1], head_h(), head_h()]
}

// The window's rows in view, and where.
pub(super) fn win_rows(s: &State) -> Vec<([f32; 4], WinRow)> {
    let Some(w) = s.gui.window else {
        return Vec::new();
    };
    let [x, y, ww, _] = win_frame(s, &w);
    let top = y + head_h() + w.about_h;
    win_body(s, w.module)
        .0
        .into_iter()
        .filter_map(|(off, row)| {
            let rh = match row {
                WinRow::Row(hit) => row_h(s, hit),
                WinRow::Group(_) => row_hh(),
            };
            let at = off - super::sunset_smooth::shown(super::sunset_smooth::WINDOW, w.scroll);
            (at + rh > 0.0 && at < w.room).then_some(([x, top + at, ww, rh], row))
        })
        .collect()
}

// What a click on the window hits: before anything under it.
fn window_hits(s: &State) -> Vec<([f32; 4], Hit)> {
    let Some(w) = s.gui.window else {
        return Vec::new();
    };
    let frame = win_frame(s, &w);
    let mut out = vec![(frame, Hit::WinBody), ([frame[0], frame[1], frame[2], head_h()], Hit::WinTitle), (win_close(frame), Hit::WinClose)];
    let top = frame[1] + head_h() + w.about_h;
    out.extend(win_rows(s).into_iter().filter_map(|(r, row)| match row {
        WinRow::Row(hit) => in_view(r, [top, top + w.room]).map(|r| (r, hit)),
        WinRow::Group(_) => None,
    }));
    out
}

// A number field's width, for its text.
pub(super) fn field_width(game: &Game<'_>, text: &str) -> f32 {
    (game.sunset_text_width(text) + 6.0).max(24.0)
}

// A description in lines that fit `width`.
pub(super) fn wrap(game: &Game<'_>, text: &str, width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(l) if game.sunset_text_width(&format!("{l} {word}")) <= width => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

// The pointer in the GUI's own coordinates, which GUI scale divides.
pub(super) fn gui_cursor(game: &Game<'_>) -> (f32, f32) {
    let scale = gui_scale();
    (game.cursor.0 / scale, game.cursor.1 / scale)
}

// A panel's scroll moved by some rows: it always starts on a row, and stops at the first that shows the panel's end.
pub(super) fn snap_scroll(rows: &[(f32, Hit)], content: f32, room: f32, scroll: f32, by: i32) -> f32 {
    let max = (content - room).max(0.0);
    let stops = rows.iter().position(|(off, _)| *off >= max - 0.01).map_or(rows.len(), |i| i + 1);
    let Some(last) = stops.checked_sub(1) else {
        return 0.0;
    };
    let at = rows[..stops].iter().position(|(off, _)| *off >= scroll - 0.01).unwrap_or(last);
    rows[(at as i32 + by).clamp(0, last as i32) as usize].0
}

// The wheel over a panel scrolls it: the top one, where panels overlap.
pub(super) fn gui_wheel(game: &Game<'_>, steps: f32) {
    let at = gui_cursor(game);
    with(|s| {
        // An open picker's list scrolls, wherever the pointer is.
        let (view_w, view_h) = (s.gui.view_w, s.gui.view_h);
        if let Some(menu) = &mut s.gui.profile_menu {
            menu.scroll((-steps * 3.0) as i32, view_w, view_h);
            return;
        }
        if let Some(Editor { module, setting, draft: Ok(draft), list: Some(picker), .. }) = &mut s.gui.editor {
            use super::sunset_picker as pick;
            let count = match &s.modules[*module].settings[*setting].value {
                Value::Text(t) => pick::rows(picker, t, draft.text()).len(),
                _ => 0,
            };
            let max = pick::max_scroll(count, pick::rect(view_w, view_h));
            picker.scroll = (picker.scroll - steps * pick::ROW * 3.0).clamp(0.0, max);
            return;
        }
        // Over the settings window: it scrolls.
        if let Some(w) = s.gui.window
            && inside(win_frame(s, &w), at)
        {
            let max = (w.content - w.room).max(0.0);
            s.gui.window = Some(Win { scroll: (w.scroll - steps * row_hh() * 2.0).clamp(0.0, max), ..w });
            return;
        }
        if super::sunset_dashboard::on() {
            super::sunset_dashboard::wheel(s, steps, at);
            return;
        }
        for c in (0..CATEGORIES.len()).rev() {
            let p = s.gui.panels[c];
            let (rows, content) = panel_body(s, c);
            if rows.is_empty() && !s.gui.search.is_empty() {
                continue;
            }
            let room = body_room(s, c);
            let shown = if p.collapsed { 0.0 } else { content.min(room) + body_pad() };
            if inside([p.x, p.y, panel_w(), head_h() + shown], at) {
                let by = -steps * 2.0;
                let by = if by > 0.0 { by.ceil() } else { by.floor() } as i32;
                s.gui.panels[c].scroll = snap_scroll(&rows, content, room, p.scroll, by);
                return;
            }
        }
    });
}

pub(super) fn inside(r: [f32; 4], (x, y): (f32, f32)) -> bool {
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

pub(super) fn hit_at(s: &State, at: (f32, f32)) -> Option<([f32; 4], Hit)> {
    // The last drawn is the one on top, where panels overlap.
    gui_rows(s).into_iter().rfind(|(r, _)| inside(*r, at))
}

// The ClickGUI module is on exactly while the menu is open, however it opened (the menu key, .menu, the Options
// button, its own bind) or closed (Esc, the menu key, leaving the world, Panic). Its switch is the menu's state, never
// a toggle message of its own.
pub(super) fn mirror_gui(s: &mut State) {
    let open = s.gui.open;
    if let Some(m) = s.modules.iter_mut().find(|m| m.name == "ClickGUI") {
        m.on = open;
    }
}

pub(super) fn open_gui(game: &mut Game<'_>) {
    if !running() { return; }
    release_binds();
    if !game.native_playing() {
        return;
    }
    ensure_loaded(game);
    // The menu is drawn with the HUD: opened while F1 hides it, the HUD comes back.
    game.ui.hide_gui = false;
    game.keys.clear();
    game.capture(false);
    with(|s| {
        s.gui.open = true;
        s.gui.closing = false;
        s.gui.drag = None;
        s.binding = None;
        mirror_gui(s);
    });
    super::sunset_dashboard::opened();
}

pub(super) fn close_gui(game: &mut Game<'_>) {
    with(|s| {
        s.gui.profile_menu = None;
        s.gui.open = false;
        s.gui.closing = false;
        s.gui.drag = None;
        s.gui.editor = None;
        s.gui.window = None;
        s.binding = None;
        mirror_gui(s);
    });
    save(game);
    if game.screen == Screen::Playing {
        game.capture(true);
    }
}

// The number's own line above a slider: clicks lower than this go to the slider, so the end of the track (and its knob,
// which reaches a little above it) slides instead of opening the number for typing.
pub(super) fn number_band(r: [f32; 4]) -> f32 {
    r[1] + row_hh() - 4.0
}

// Where a slider's track starts in its row, and how long it is.
pub(super) fn slider_track(r: [f32; 4]) -> (f32, f32) {
    let x0 = r[0] + indent() + 4.0;
    (x0, (r[0] + r[2] - RIGHT_PAD - x0).max(1.0))
}

// A slider's value from the pointer's place along its row.
pub(super) fn slide(s: &mut State, i: usize, j: usize, r: [f32; 4], x: f32) -> bool {
    let Some(setting) = s.modules[i].settings.get_mut(j) else {
        return false;
    };
    if let Value::Num { v, min, max, step } = &mut setting.value {
        let (x0, track) = slider_track(r);
        let t = ((x - x0) / track).clamp(0.0, 1.0);
        let n = round_to(*min + f64::from(t) * (*max - *min), *step).clamp(*min, *max);
        let changed = (n - *v).abs() > 1e-9;
        *v = n;
        return changed;
    }
    false
}
