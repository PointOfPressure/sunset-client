// The launcher's test plugin, through Sunset's plugin API only. Once it runs you see three things:
// - in chat, a line from it each time you join a world (and `.testplugin` answers);
// - at the top of the screen, a small badge with a pulsing dot;
// - in the world, the block under your feet boxed in orange while its "Test Beacon" module is on (it turns that on
//   the first time it runs; turn it off and on in the ClickGUI or with `.t Test Beacon`).
use super::Game;
use super::sunset_api as api;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const _: () = api::needs(3);

const ID: &str = "launcher_test";
const VERSION: &str = "1.0.0";
const BEACON: &str = "Test Beacon";
const BADGE: &str = "Test plugin running";

// Said in chat once per world joined.
static GREETED: AtomicBool = AtomicBool::new(false);
static SINCE: OnceLock<Instant> = OnceLock::new();

pub(crate) fn tick(game: &mut Game<'_>) {
    api::plugin(game, ID, VERSION, setup);
}

fn setup(p: &mut api::Plugin) {
    SINCE.get_or_init(Instant::now);
    p.module(api::Module::new(BEACON, api::Category::Render, "The launcher's test plugin: boxes the block under your feet."));
    p.command("testplugin", "says that the launcher's test plugin is built in", |game, _args| {
        let beacon = if api::is_on(BEACON) { "on" } else { "off" };
        api::chat(game, &format!("Test plugin {VERSION} is running (Test Beacon {beacon})."));
    });
    p.on_tick(|game| {
        if GREETED.swap(true, Ordering::Relaxed) {
            return;
        }
        api::chat(game, "Test plugin loaded: the Sunset launcher built it into this game. Type .testplugin");
        // The first run turns the beacon on, so the world effect shows without a click; later runs keep your choice.
        if api::load(game, ID, "seen").is_none() {
            let _ = api::toggle(game, BEACON, true);
            let _ = api::save(game, ID, "seen", "1");
        }
    });
    p.on_disconnect(|_game, _why| GREETED.store(false, Ordering::Relaxed));
    p.on_hud(|game, draw| {
        let [w, _] = draw.size();
        let text_w = api::text_width(game, BADGE);
        let (bw, bh) = (text_w + 22.0, 14.0);
        let (x, y) = ((w - bw) / 2.0, 4.0);
        draw.rect([x, y, bw, bh], 0xc0101014);
        draw.rect([x, y + bh - 1.0, bw, 1.0], 0xffdb5000);
        // A dot that pulses about once a second.
        let t = SINCE.get().map_or(0.0, |s| s.elapsed().as_secs_f32());
        let a = (0.55 + 0.45 * (t * std::f32::consts::TAU).sin()) * 255.0;
        draw.rect([x + 6.0, y + 4.0, 5.0, 5.0], ((a as u32) << 24) | 0x00f26a12);
        draw.label(BADGE, [x + 16.0, y + 3.0], 0xf2f2f2);
    });
    p.on_world(|game, draw| {
        if !api::is_on(BEACON) {
            return;
        }
        let Some(me) = api::me(game) else { return };
        let under = [me.pos[0].floor() as i32, (me.pos[1] - 0.2).floor() as i32, me.pos[2].floor() as i32];
        draw.block(under, 0xffdb5000);
    });
}
