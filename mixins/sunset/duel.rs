// Ported from Sayer's Duelkit (MIT, github.com/PointOfPressure/duelkit): Wind Jump and Hold to Eat.
use super::sunset::{Category, Module, hold, with};
use super::{Game, Screen};
use std::sync::Mutex;
use std::time::Instant;
use winit::event::MouseButton;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("Wind Jump", Category::Movement, "A wind charge you throw on the ground, looking down at least Min pitch: as its blast reaches you, you jump first, so the knockback adds to the jump instead of replacing it. Aiming is up to you.")
            .num("Min pitch", 60.0, 0.0, 90.0, 5.0)
            .num("Window (ms)", 1500.0, 200.0, 5000.0, 100.0),
        Module::new("Hold to Eat", Category::Player, "Hold the hotbar key of your golden apples: it switches to them and eats; let go and your slot comes back. Any food: every food's key does it.")
            .bool("Golden apples", true)
            .bool("Any food", false),
    ]
}

#[derive(Default)]
struct Duel {
    // Wind Jump: when the wind charge went.
    thrown: Option<Instant>,
    // Hold to Eat: the slot eaten from, its key, and the slot to go back to.
    eating: Option<(usize, String, usize)>,
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
    let Some(m) = module("Wind Jump") else {
        return;
    };
    let Some(stack) = game.stack_in(slot) else {
        return;
    };
    let cooling = game.res.items().get(&stack.item).is_some_and(|i| {
        game.cooldowns.is_on_cooldown(&horizon_client::cooldowns::group(stack, i.use_cooldown.as_ref()))
    });
    if stack.item.ends_with(":wind_charge")
        && !cooling
        && f64::from(game.pitch) >= m.n("Min pitch")
        && game.player.as_ref().is_some_and(|p| p.ground.on_ground)
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

// Hold to Eat, from the key hook: the hotbar key of food pressed starts eating it.
pub(super) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
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

pub(super) fn tick(game: &mut Game<'_>) {
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
