#!/usr/bin/env python3
"""Compile production GUI lifecycle functions and both hooks against a tiny host.

No game, network, Cargo target, or graphics dependency is used. The fixture only
supplies Game/state boundaries; open/close, F1 ownership and per-frame cleanup
are extracted unchanged from runtime.rs. Manifest assertions cover hook wiring.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "mixins/sunset"
RUSTC = os.environ.get("SUNSET_TEST_RUSTC", os.environ.get("RUSTC", "rustc"))


def item(source: str, signature: str) -> str:
    start = source.index(signature)
    opening = source.index("{", start)
    depth, cursor = 1, opening + 1
    while depth:
        if source[cursor] == "{":
            depth += 1
        elif source[cursor] == "}":
            depth -= 1
        cursor += 1
    return source[start:cursor]


HARNESS = r'''
#![forbid(unsafe_code)]
#![allow(dead_code)]
mod game {
    use std::{cell::RefCell, marker::PhantomData};
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Screen { Playing, Pause }
    #[derive(Default)]
    pub struct Ui { pub hide_gui: bool }
    pub struct Game<'a> {
        pub window: bool, pub enabled: bool, pub screen: Screen,
        pub title: Option<()>, pub player: Option<()>, pub focused: bool,
        pub ui: Ui, pub keys: Vec<()>, pub captures: Vec<bool>,
        pub marker: PhantomData<&'a ()>,
    }
    impl Default for Game<'_> {
        fn default() -> Self {
            Self { window: true, enabled: true, screen: Screen::Playing,
                title: None, player: Some(()), focused: true, ui: Ui::default(),
                keys: vec![()], captures: vec![], marker: PhantomData }
        }
    }
    impl Game<'_> {
        fn native_window(&self) -> bool { self.window }
        fn native_on(&self, _: &str) -> bool { self.enabled }
        fn native_playing(&self) -> bool { self.screen == Screen::Playing && self.title.is_none() }
        fn capture(&mut self, capture: bool) { self.captures.push(capture); }
        pub fn f1(&mut self) {
            // Fixture for the host's cancellable HEAD dispatch and tiny body.
            if sunset_gui_visibility(self).is_continue() {
                self.ui.hide_gui = !self.ui.hide_gui;
            }
        }
        pub fn frame(&mut self) { sunset_gui_frame(self); }
    }
    __VISIBILITY_HOOK__
    __FRAME_HOOK__
    pub mod sunset {
        use super::*;
        const ID: &str = "sunset";
        #[derive(Default)]
        pub struct Gui {
            pub open: bool, pub closing: bool, pub drag: Option<()>,
            pub editor: Option<String>, pub window: Option<()>, pub typed: bool,
        }
        #[derive(Default)]
        pub struct State { pub gui: Gui, pub binding: Option<usize>, pub saves: usize, pub releases: usize }
        thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
        pub fn with<T>(f: impl FnOnce(&mut State) -> T) -> T { STATE.with(|s| f(&mut s.borrow_mut())) }
        fn release_binds() { with(|s| s.releases += 1); }
        fn ensure_loaded(_: &Game<'_>) {}
        fn save(_: &Game<'_>) { with(|s| s.saves += 1); }
        __PRODUCTION__
    }
#[cfg(test)]
mod tests {
use super::{Game, Screen, sunset::*};

fn menu() -> Game<'static> {
    with(|s| *s = Default::default());
    let mut game = Game::default();
    open_gui(&mut game);
    with(|s| {
        s.gui.window = Some(());
        s.gui.editor = Some("draft value".into());
        s.gui.drag = Some(());
        s.gui.typed = true;
        s.binding = Some(4);
    });
    game
}
fn assert_clean() {
    with(|s| {
        assert!(!s.gui.open && !s.gui.closing && !s.gui.typed);
        assert!(s.gui.drag.is_none() && s.gui.window.is_none() && s.gui.editor.is_none());
        assert!(s.binding.is_none());
        assert_eq!(s.saves, 1);
    });
}
#[test]
fn f1_preserves_settings_editor_and_input_ownership() {
    let mut game = menu();
    for _ in 0..3 { game.f1(); game.frame(); }
    assert!(!game.ui.hide_gui);
    with(|s| {
        assert!(s.gui.open && s.gui.window.is_some());
        assert_eq!(s.gui.editor.as_deref(), Some("draft value"));
        assert!(s.gui.drag.is_some());
        assert_eq!(s.binding, Some(4));
        assert_eq!(s.saves, 0);
    });
    assert_eq!(game.captures, [false]);
}
#[test]
fn f1_without_sunset_menu_toggles_normally() {
    with(|s| *s = Default::default());
    let mut game = Game::default();
    game.f1(); assert!(game.ui.hide_gui);
    game.f1(); assert!(!game.ui.hide_gui);
}
#[test]
fn opening_after_f1_restores_hud_before_taking_input() {
    with(|s| *s = Default::default());
    let mut game = Game::default();
    game.f1();
    open_gui(&mut game);
    assert!(!game.ui.hide_gui && gui_open() && game.keys.is_empty());
    assert_eq!(game.captures, [false]);
}
#[test]
fn deferred_close_finishes_without_any_hud_call() {
    let mut game = menu();
    with(|s| s.gui.closing = true);
    game.frame(); assert_clean();
    assert_eq!(game.captures, [false, true]);
    game.frame();
    with(|s| assert_eq!(s.saves, 1));
}
#[test]
fn externally_hidden_hud_releases_invisible_menu() {
    let mut game = menu();
    game.ui.hide_gui = true;
    game.frame(); assert_clean();
    assert_eq!(game.captures, [false, true]);
    assert!(game.ui.hide_gui);
}
#[test]
fn leaving_play_screen_does_not_capture_pointer() {
    let mut game = menu();
    game.screen = Screen::Pause;
    game.frame(); assert_clean();
    assert_eq!(game.captures, [false, false]);
}
#[test]
fn title_over_playing_screen_does_not_capture_pointer() {
    let mut game = menu();
    game.title = Some(());
    game.frame(); assert_clean();
    assert_eq!(game.captures, [false, false]);
}
#[test]
fn world_unload_does_not_capture_pointer() {
    let mut game = menu();
    game.player = None;
    game.frame(); assert_clean();
    assert_eq!(game.captures, [false, false]);
}
#[test]
fn disabled_mod_releases_menu_and_leaves_f1_to_host() {
    let mut game = menu();
    game.enabled = false;
    game.f1(); assert!(game.ui.hide_gui);
    game.frame(); assert_clean();
    assert_eq!(game.captures, [false, false]);
}
#[test]
fn unfocused_close_does_not_capture_pointer() {
    let mut game = menu();
    game.focused = false;
    with(|s| s.gui.closing = true);
    game.frame(); assert_clean();
    assert_eq!(game.captures, [false, false]);
}
#[test]
fn server_frame_and_f1_leave_client_menu_alone() {
    let mut game = menu();
    game.window = false;
    with(|s| s.gui.closing = true);
    game.f1(); game.frame();
    with(|s| {
        assert!(s.gui.open && s.gui.closing && s.gui.editor.is_some());
        assert_eq!(s.saves, 0);
    });
    assert_eq!(game.captures, [false]);
}
#[test]
fn direct_server_open_and_close_preserve_client_gui_and_binds() {
    let mut game = menu();
    game.window = false;
    open_gui(&mut game);
    close_gui(&mut game);
    with(|s| {
        assert!(s.gui.open && s.gui.window.is_some() && s.gui.drag.is_some());
        assert_eq!(s.gui.editor.as_deref(), Some("draft value"));
        assert_eq!(s.binding, Some(4));
        assert_eq!(s.releases, 1);
        assert_eq!(s.saves, 0);
    });
    assert_eq!(game.captures, [false]);
}
#[test]
fn invalid_context_cannot_open_or_unhide_menu() {
    for reason in 0..5 {
        with(|s| *s = Default::default());
        let mut game = Game::default();
        game.ui.hide_gui = true;
        match reason {
            0 => game.window = false, 1 => game.enabled = false,
            2 => game.player = None, 3 => game.screen = Screen::Pause,
            _ => game.title = Some(()),
        }
        open_gui(&mut game);
        assert!(!gui_open() && game.ui.hide_gui && game.captures.is_empty());
        with(|s| assert_eq!(s.releases, 0));
    }
}
}
}
'''


def main():
    runtime = (SOURCE / "runtime.rs").read_text(encoding="utf-8")
    manifest = tomllib.loads((SOURCE / "horizon-mixin.toml").read_text(encoding="utf-8"))
    hooks = {hook["id"]: hook for hook in manifest["injection"]}
    for name, target in (
        ("gui_frame", "horizon_client::game::native_hooks::Game<'_>::native_frame"),
        ("gui_visibility", "horizon_client::game::chat::Game<'_>::toggle_hide_gui"),
    ):
        assert hooks[name]["kind"] == "HEAD" and hooks[name]["target"] == target
        assert hooks[name]["helper"] == name + ".rs"
        assert hooks[name]["bindings"] == [{"name": "game", "ty": "&mut crate::game::Game<'_>", "value": "&mut *self"}]
    assert hooks["gui_visibility"]["cancellable"] is True
    production = "\n\n".join(item(runtime, "pub(super) fn " + name + "(") for name in (
        "open_gui", "close_gui", "gui_keep_visible", "gui_frame", "gui_open"))
    # Lifecycle ownership must not drift back into a hook that F1 can suppress.
    assert "close_gui(game)" not in item(runtime, "pub(super) fn hud(")
    source = HARNESS.replace("__PRODUCTION__", production)
    source = source.replace("__VISIBILITY_HOOK__", (SOURCE / "gui_visibility.rs").read_text(encoding="utf-8"))
    source = source.replace("__FRAME_HOOK__", (SOURCE / "gui_frame.rs").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="sunset-gui-visibility-") as temporary:
        stage = Path(temporary)
        harness = stage / "gui_visibility_tests.rs"
        binary = stage / ("gui_visibility_tests.exe" if os.name == "nt" else "gui_visibility_tests")
        harness.write_text(source, encoding="utf-8")
        subprocess.run([RUSTC, "--edition=2024", "--test", str(harness), "-o", str(binary)], check=True, cwd=ROOT)
        subprocess.run([str(binary), "--test-threads=1", "--nocapture"], check=True, cwd=ROOT)


if __name__ == "__main__":
    main()
