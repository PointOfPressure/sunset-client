// Crystal Aura and Anchor Aura, run together: one acts at a time (the better hit first), and the box each would use
// next is drawn.
use super::sunset::{self, Module};
use super::{Game, Screen};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
struct View {
    session: Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>,
    dim: super::Dim,
    pos: [f64; 3],
    from: [f64; 3],
    moved: Instant,
    shown: Instant,
    damage: f64,
    own: f64,
    tick: u64,
}
static VIEWS: Mutex<[Option<View>; 2]> = Mutex::new([None, None]);
static CRYSTAL_ON: AtomicBool = AtomicBool::new(false);
pub(super) fn show(game: &Game<'_>, anchor: bool, pos: [i32; 3], rating: super::sunset_explosive_choice::Rating) {
    let Some(remote) = game.remote.as_ref() else {
        return;
    };
    let now = Instant::now();
    let pos = pos.map(f64::from);
    let mut guard = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let previous = guard[usize::from(anchor)].as_ref().filter(|v| {
        v.dim == game.dim
            && Arc::ptr_eq(&v.session, &remote.session.counters)
            && v.shown.elapsed() < Duration::from_millis(500)
    });
    let (from, moved) =
        previous.map_or((pos, now), |v| if v.pos == pos { (v.from, v.moved) } else { (slide(v, now), now) });
    guard[usize::from(anchor)] = Some(View {
        session: remote.session.counters.clone(),
        dim: game.dim,
        pos,
        from,
        moved,
        shown: now,
        damage: rating.full_damage,
        own: rating.self_damage,
        tick: game.ticks,
    });
}
fn slide(view: &View, now: Instant) -> [f64; 3] {
    let fraction = (now.duration_since(view.moved).as_secs_f64() / 0.12).clamp(0.0, 1.0);
    let progress = 1.0 - (1.0 - fraction).powi(3);
    std::array::from_fn(|i| view.from[i] + (view.pos[i] - view.from[i]) * progress)
}
pub(super) fn clear_view(anchor: bool) {
    VIEWS.lock().unwrap_or_else(|e| e.into_inner())[usize::from(anchor)] = None;
}
pub(super) fn draw(
    game: &Game<'_>,
    camera: &sunset::Camera,
    modules: &[Module],
    out: &mut impl sunset::Canvas,
    _width: f32,
) {
    let now = Instant::now();
    let views = VIEWS.lock().unwrap_or_else(|e| e.into_inner()).each_ref().map(|v| {
        v.as_ref()
            .filter(|v| {
                v.dim == game.dim
                    && game.ticks >= v.tick
                    && now.duration_since(v.shown) < Duration::from_millis(500)
                    && game.remote.as_ref().is_some_and(|r| Arc::ptr_eq(&r.session.counters, &v.session))
            })
            .map(|v| {
                (
                    slide(v, now),
                    v.damage,
                    v.own,
                    ((0.5 - now.duration_since(v.shown).as_secs_f64()) / 0.35).clamp(0.0, 1.0),
                )
            })
    });
    for (index, name) in ["Crystal Aura", "Anchor Aura"].iter().enumerate() {
        let Some(module) = modules.iter().find(|m| m.name == *name && m.on && m.b("Render")) else {
            continue;
        };
        let Some((lo, damage, own, alpha)) = views[index] else {
            continue;
        };
        let color = module.t("Render color");
        let color = color
            .strip_prefix('#')
            .or_else(|| color.strip_prefix("0x"))
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .or_else(|| color.parse::<i32>().ok().map(|n| n as u32))
            .map(|n| if n <= 0x00ff_ffff { n | 0xff00_0000 } else { n })
            .unwrap_or(0xffff_ffff);
        let color = (((f64::from(color >> 24) * alpha).round() as u32) << 24) | (color & 0xffffff);
        super::sunset_visual::draw_box(
            camera,
            out,
            &super::sunset_visual::Look::BOXED,
            lo,
            lo.map(|v| v + 1.0),
            color,
            1.5,
        );
        let top = [lo[0] + 0.5, lo[1] + 1.25, lo[2] + 0.5];
        if let Some([x, y]) = camera.project(top) {
            let label = format!("{damage:.1}  self {own:.1}");
            let width = game.native_text_width(&label);
            out.text(&label, [x - width * 0.5, y], color & 0xffffff);
        }
    }
}

pub(super) use super::sunset_explosive_choice::Rating;
use super::sunset_explosive_choice::prefer_first;

pub(super) fn can_act(game: &Game<'_>) -> bool {
    game.native_window()
        && game.native_on(sunset::ID)
        && game.native_playing()
        && game.screen == Screen::Playing
        && game.remote.is_some()
        && !sunset::gui_open()
        && !game.spectator()
        && game.selected < 9
        && game.using.is_none()
        && game.mining.is_none()
        && super::remote::sunset_remote::breaking(game).is_none()
        && !super::sunset_player::gap_eating()
        && !super::sunset_explosive_tactics::use_conflicts(game)
        && super::sunset_util::inventory_idle(game)
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && sunset::CRIT_STAGE.load(Ordering::Relaxed) == 0
        && super::sunset_combat_control::owner().is_none_or(|o| o == "Crystal Aura" || o == "Anchor Aura")
        && !sunset::with(|s| ["Blink", "Nuker", "Scaffold"].iter().any(|name| s.is_on(name)))
}

pub(super) fn tick(game: &mut Game<'_>, modules: &[Module], ticks: u64) {
    super::sunset_prediction::tick(game);
    let crystal = modules.iter().find(|m| m.name == "Crystal Aura" && m.on);
    let anchor = modules.iter().find(|m| m.name == "Anchor Aura" && m.on);
    if CRYSTAL_ON.swap(crystal.is_some(), Ordering::Relaxed) && crystal.is_none() {
        super::sunset_crystal_aura::disabled(game);
    }
    if crystal.is_none() && anchor.is_none() {
        super::sunset_crystal_aura::cleanup_shared(game);
    }
    if anchor.is_none() {
        super::sunset_anchor_aura::disabled(game);
    }
    if crystal.is_none() && anchor.is_none() {
        return;
    }
    if !super::sunset_explosive_tactics::refresh(game) || !can_act(game) {
        if crystal.is_some() {
            super::sunset_crystal_aura::paused(game);
        }
        if anchor.is_some() {
            super::sunset_anchor_aura::paused(game);
        }
        return;
    }
    if crystal.is_some()
        && !super::sunset_crystal_aura::owns_selection(game)
        && !super::sunset_explosive_tactics::may_take_hand(game, anchor.is_some())
    {
        super::sunset_crystal_aura::idle(game);
        super::sunset_anchor_aura::idle(game);
        return;
    }
    // Prepare each at most once, then claim only the chosen action. A pending
    // server acknowledgement is not a new inventory/rotation owner.
    let c = crystal.and_then(|m| super::sunset_crystal_aura::prepare(game, Some(m), ticks)).filter(|r| r.valid());
    super::sunset_anchor_aura::rival(if crystal.is_some() { super::sunset_crystal_aura::opening_value() } else { 0.0 });
    let a = anchor.and_then(|m| super::sunset_anchor_aura::prepare(game, m, ticks)).filter(|r| r.valid());
    let crystal_first = match (c, a) {
        (Some(c), Some(a)) => prefer_first(c, a, ticks % 2 == 0),
        (Some(_), None) => true,
        _ => false,
    };
    if c.is_none() && a.is_none() {
        let c = super::sunset_crystal_aura::lookahead();
        let a = super::sunset_anchor_aura::lookahead();
        let first = match (c, a) {
            (Some(c), Some(a)) => prefer_first(c, a, true),
            (Some(_), None) => true,
            _ => false,
        };
        if first {
            if let Some(m) = crystal {
                super::sunset_crystal_aura::advance_look(game, m);
            }
        } else if let Some(m) = anchor {
            super::sunset_anchor_aura::advance_look(game, m);
        }
        return;
    }
    if crystal_first {
        if let Some(m) = crystal {
            super::sunset_crystal_aura::execute(game, m, ticks);
        }
    } else if a.is_some() {
        if let Some(m) = anchor {
            super::sunset_anchor_aura::execute(game, m, ticks);
        }
    }
}
