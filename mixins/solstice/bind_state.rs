// SPDX-License-Identifier: MIT
// Named input and momentary bind ownership. AI-assisted contribution.
// This module deliberately has no host or windowing dependencies.

use std::collections::HashSet;

// These effects query the current module state directly; they leave no
// camera, abilities, mesh, input, or global engine override to unwind.
pub(super) fn activation_modes(module: &str) -> &'static [&'static str] {
    match module {
        "Zoom" | "Radar" | "ESP" | "Tracers" | "Nametags" | "Storage ESP"
        | "Block ESP" | "HUD" | "Trajectories" | "Waypoints" | "Breadcrumbs" | "Logout Spots"
        | "Hole ESP" | "Tunnel ESP" | "Void ESP" | "Light Overlay" => &["Toggle", "Hold"],
        _ => &["Toggle"],
    }
}

pub(super) fn canonical(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_lowercase();
    let mouse = match text.as_str() {
        "mouse1" | "mouseleft" | "lmb" | "key.mouse.1" | "key.mouse.left" => Some("left"),
        "mouse2" | "mouseright" | "rmb" | "key.mouse.2" | "key.mouse.right" => Some("right"),
        "mouse3" | "mousemiddle" | "mmb" | "key.mouse.3" | "key.mouse.middle" => Some("middle"),
        "mouse4" | "mouseback" | "back" | "key.mouse.back" | "key.mouse.4" => Some("4"),
        "mouse5" | "mouseforward" | "forward" | "key.mouse.forward" | "key.mouse.5" => Some("5"),
        _ => None,
    };
    if let Some(name) = mouse {
        return Some(format!("key.mouse.{name}"));
    }
    if let Some(number) = text.strip_prefix("mouse").or_else(|| text.strip_prefix("key.mouse.")) {
        let number = number.parse::<u16>().ok()?;
        return (number >= 4).then(|| format!("key.mouse.{number}"));
    }
    // Preserve the upstream fully-qualified keyboard namespace.
    if text.starts_with("key.keyboard.") && text.len() > "key.keyboard.".len() {
        return Some(text);
    }
    let name = match text.as_str() {
        "rshift" | "rightshift" => "right.shift",
        "lshift" | "leftshift" | "shift" => "left.shift",
        "rctrl" | "rightctrl" | "rightcontrol" => "right.control",
        "lctrl" | "leftctrl" | "ctrl" | "control" => "left.control",
        "ralt" | "rightalt" => "right.alt",
        "lalt" | "leftalt" | "alt" => "left.alt",
        "return" => "enter",
        "caps" | "capslock" => "caps.lock",
        "pageup" => "page.up",
        "pagedown" => "page.down",
        "grave" | "`" => "grave.accent",
        "space" | "tab" | "enter" | "up" | "down" | "left" | "right" | "insert" | "home" | "end" | "delete" | "backspace" | "escape" => text.as_str(),
        _ if text.chars().count() == 1 => text.as_str(),
        _ if text.starts_with('f') && text[1..].parse::<u32>().is_ok() => text.as_str(),
        _ if text.starts_with("kp") => return Some(format!("key.keyboard.keypad.{}", &text[2..])),
        _ => return None,
    };
    Some(format!("key.keyboard.{name}"))
}

#[derive(Debug, PartialEq)]
pub(super) struct Held {
    pub module: usize,
    pub key: String,
    pub previous: bool,
}

#[derive(Default)]
pub(super) struct Inputs {
    down: HashSet<String>,
    blocked: HashSet<String>,
    synthetic: HashSet<String>,
    held: Vec<Held>,
}

impl Inputs {
    // Only fresh presses can activate. Keys pressed in a screen remain
    // blocked until release, even if that same event closes the screen.
    pub fn press(&mut self, key: &str) -> bool {
        !self.blocked.contains(key) && self.down.insert(key.to_owned())
    }

    pub fn block(&mut self, key: &str) {
        self.down.insert(key.to_owned());
        self.blocked.insert(key.to_owned());
    }

    pub fn synthetic(&mut self, key: &str) {
        self.block(key);
        self.synthetic.insert(key.to_owned());
    }

    pub fn is_synthetic(&self, key: &str) -> bool {
        self.synthetic.contains(key)
    }

    pub fn hold(&mut self, module: usize, key: &str, previous: bool) {
        self.held.push(Held { module, key: key.to_owned(), previous });
    }

    pub fn release(&mut self, key: &str) -> Vec<Held> {
        self.down.remove(key);
        self.blocked.remove(key);
        self.synthetic.remove(key);
        self.take_where(|held| held.key == key)
    }

    pub fn suspend(&mut self) -> Vec<Held> {
        self.blocked.extend(self.down.iter().cloned());
        std::mem::take(&mut self.held)
    }

    pub fn missing(&mut self, mut is_down: impl FnMut(&str) -> bool) -> Vec<Held> {
        let missing: Vec<String> = self.down.iter().filter(|key| !is_down(key)).cloned().collect();
        missing.iter().flat_map(|key| self.release(key)).collect()
    }

    pub fn take_where(&mut self, mut predicate: impl FnMut(&Held) -> bool) -> Vec<Held> {
        let mut released = Vec::new();
        let mut i = 0;
        while i < self.held.len() {
            if predicate(&self.held[i]) {
                released.push(self.held.swap_remove(i));
            } else {
                i += 1;
            }
        }
        released
    }

    pub fn saved_on(&self, module: usize, current: bool) -> bool {
        self.held.iter().find(|held| held.module == module).map_or(current, |held| current && held.previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_match_host_mouse_names_and_keyboard_defaults() {
        for (alias, expected) in [
            ("LMB", "key.mouse.left"), ("mouse2", "key.mouse.right"),
            ("mmb", "key.mouse.middle"), ("mouseback", "key.mouse.4"),
            ("mouseforward", "key.mouse.5"), ("key.mouse.back", "key.mouse.4"),
            ("mouse6", "key.mouse.6"), ("rshift", "key.keyboard.right.shift"),
            ("`", "key.keyboard.grave.accent"), ("r", "key.keyboard.r"),
            ("kp1", "key.keyboard.keypad.1"), ("f6", "key.keyboard.f6"),
        ] {
            assert_eq!(canonical(alias).as_deref(), Some(expected), "{alias}");
        }
        assert_eq!(canonical("key.keyboard.right.shift").as_deref(), Some("key.keyboard.right.shift"));
        assert!(canonical("mouse0").is_none());
        assert!(canonical("key.mouse.unknown").is_none());
    }

    #[test]
    fn repeated_presses_do_not_toggle_again_and_release_rearms() {
        let mut inputs = Inputs::default();
        assert!(inputs.press("r"));
        assert!(!inputs.press("r"));
        assert!(inputs.release("r").is_empty());
        assert!(inputs.press("r"));
    }

    #[test]
    fn hold_release_restores_manual_state_and_never_persists_temporary_on() {
        let mut inputs = Inputs::default();
        assert!(inputs.press("mouse4"));
        inputs.hold(1, "mouse4", false);
        inputs.hold(2, "mouse4", true);
        assert!(!inputs.saved_on(1, true));
        assert!(inputs.saved_on(2, true));
        // An automatic safety shutdown takes precedence over a prior manual ON.
        assert!(!inputs.saved_on(2, false));
        let released = inputs.release("mouse4");
        assert_eq!(released.len(), 2);
        assert!(released.iter().any(|h| h.module == 1 && !h.previous));
        assert!(released.iter().any(|h| h.module == 2 && h.previous));
        assert!(inputs.release("mouse4").is_empty());
    }

    #[test]
    fn suspension_releases_holds_and_requires_a_new_press() {
        let mut inputs = Inputs::default();
        assert!(inputs.press("r"));
        inputs.hold(0, "r", false);
        assert_eq!(inputs.suspend().len(), 1);
        assert!(!inputs.press("r"));
        inputs.release("r");
        assert!(inputs.press("r"));
    }

    #[test]
    fn screen_closing_press_and_focus_synthetic_press_stay_blocked() {
        let mut inputs = Inputs::default();
        inputs.block("enter");
        assert!(!inputs.press("enter"));
        inputs.release("enter");
        assert!(inputs.press("enter"));
        inputs.synthetic("r");
        assert!(inputs.is_synthetic("r"));
        assert!(!inputs.press("r"));
        inputs.release("r");
        assert!(!inputs.is_synthetic("r"));
        assert!(inputs.press("r"));
    }

    #[test]
    fn missing_release_is_recovered_from_physical_state() {
        let mut inputs = Inputs::default();
        inputs.press("r");
        inputs.press("mouse4");
        inputs.hold(0, "r", false);
        inputs.hold(1, "mouse4", false);
        let released = inputs.missing(|key| key == "r");
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].module, 1);
        assert!(inputs.missing(|key| key == "r").is_empty());
    }

    #[test]
    fn rebinding_or_changing_mode_releases_only_its_owned_hold() {
        let mut inputs = Inputs::default();
        inputs.hold(1, "r", false);
        inputs.hold(2, "r", false);
        let released = inputs.take_where(|held| held.module == 1);
        assert_eq!(released.len(), 1);
        assert_eq!(inputs.release("r")[0].module, 2);
    }

    #[test]
    fn only_direct_state_effects_offer_hold_activation() {
        assert_eq!(activation_modes("Zoom"), &["Toggle", "Hold"]);
        assert_eq!(activation_modes("ESP"), &["Toggle", "Hold"]);
        assert_eq!(activation_modes("Radar"), &["Toggle", "Hold"]);
        for module in ["Reach", "Air Place", "Flight", "Step", "Freecam", "Free Look", "Timer", "No Slow", "Xray", "No Render", "Fullbright", "Example plugin"] {
            assert_eq!(activation_modes(module), &["Toggle"], "{module}");
        }
    }
}
