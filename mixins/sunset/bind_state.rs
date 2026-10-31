// Binds from Rust's pull request #5: key and mouse button names, and which binds are held down (a Hold bind keeps its
// module on only while its key or button is down). No game or window types here.

use std::collections::HashSet;

// Modules that only draw or read their own on state each frame: on and off again at once leaves nothing behind (no
// camera, abilities, mesh or engine setting to put back), so they can be held.
pub(super) fn holds(module: &str) -> bool {
    matches!(
        module,
        "Zoom" | "Radar" | "ESP" | "Tracers" | "Nametags" | "Storage ESP" | "Block ESP" | "HUD" | "Trajectories" | "Waypoints"
            | "Breadcrumbs" | "Logout Spots" | "Hole ESP" | "Tunnel ESP" | "Void ESP" | "Light Overlay"
    )
}

// A key or button typed in a command (`r`, `rshift`, `f6`, `mouse4`, `key.keyboard.r`) as the game names it.
pub(super) fn canonical(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_lowercase();
    let mouse = match text.as_str() {
        "mouse1" | "mouseleft" | "lmb" | "key.mouse.1" | "key.mouse.left" => Some("left"),
        "mouse2" | "mouseright" | "rmb" | "key.mouse.2" | "key.mouse.right" => Some("right"),
        "mouse3" | "mousemiddle" | "mmb" | "key.mouse.3" | "key.mouse.middle" => Some("middle"),
        "mouse4" | "mouseback" | "key.mouse.back" | "key.mouse.4" => Some("4"),
        "mouse5" | "mouseforward" | "key.mouse.forward" | "key.mouse.5" => Some("5"),
        _ => None,
    };
    if let Some(name) = mouse {
        return Some(format!("key.mouse.{name}"));
    }
    if let Some(number) = text.strip_prefix("mouse").or_else(|| text.strip_prefix("key.mouse.")) {
        let number = number.parse::<u16>().ok()?;
        return (number >= 4).then(|| format!("key.mouse.{number}"));
    }
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
        "space" | "tab" | "enter" | "up" | "down" | "left" | "right" | "insert" | "home" | "end" | "delete" | "backspace"
        | "escape" => text.as_str(),
        _ if text.chars().count() == 1 => text.as_str(),
        _ if text.starts_with('f') && text[1..].parse::<u32>().is_ok() => text.as_str(),
        _ if text.starts_with("kp") => return Some(format!("key.keyboard.keypad.{}", &text[2..])),
        _ => return None,
    };
    Some(format!("key.keyboard.{name}"))
}

// A module a Hold bind turned on, and whether it was on before.
#[derive(Debug, PartialEq)]
pub(super) struct Held {
    pub(super) module: usize,
    pub(super) key: String,
    pub(super) previous: bool,
}

#[derive(Default)]
pub(super) struct Inputs {
    down: HashSet<String>,
    // Down since a screen or the menu had it: not a fresh press until let go.
    blocked: HashSet<String>,
    // Pressed by the window coming back into focus, not by the player.
    synthetic: HashSet<String>,
    held: Vec<Held>,
}

impl Inputs {
    // Whether this is a fresh press (not a repeat, not one a screen had).
    pub(super) fn press(&mut self, key: &str) -> bool {
        !self.blocked.contains(key) && self.down.insert(key.to_owned())
    }

    pub(super) fn block(&mut self, key: &str) {
        self.down.insert(key.to_owned());
        self.blocked.insert(key.to_owned());
    }

    pub(super) fn synthetic(&mut self, key: &str) {
        self.block(key);
        self.synthetic.insert(key.to_owned());
    }

    pub(super) fn is_synthetic(&self, key: &str) -> bool {
        self.synthetic.contains(key)
    }

    pub(super) fn hold(&mut self, module: usize, key: &str, previous: bool) {
        self.held.push(Held { module, key: key.to_owned(), previous });
    }

    // Let go: the holds it made end.
    pub(super) fn release(&mut self, key: &str) -> Vec<Held> {
        self.down.remove(key);
        self.blocked.remove(key);
        self.synthetic.remove(key);
        self.take_where(|held| held.key == key)
    }

    // A screen, the menu or lost focus: every hold ends, and what is down needs a new press.
    pub(super) fn suspend(&mut self) -> Vec<Held> {
        self.blocked.extend(self.down.iter().cloned());
        std::mem::take(&mut self.held)
    }

    // Keys let go of where no release came (a screen took it): from what is physically down.
    pub(super) fn missing(&mut self, mut is_down: impl FnMut(&str) -> bool) -> Vec<Held> {
        let missing: Vec<String> = self.down.iter().filter(|key| !is_down(key)).cloned().collect();
        missing.iter().flat_map(|key| self.release(key)).collect()
    }

    pub(super) fn take_where(&mut self, mut predicate: impl FnMut(&Held) -> bool) -> Vec<Held> {
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

    // The on state to save: a module on only because it is held is saved as it was before.
    pub(super) fn saved_on(&self, module: usize, current: bool) -> bool {
        self.held.iter().find(|held| held.module == module).map_or(current, |held| current && held.previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_match_the_game_names() {
        for (alias, expected) in [
            ("LMB", "key.mouse.left"),
            ("mouse2", "key.mouse.right"),
            ("mmb", "key.mouse.middle"),
            ("mouseback", "key.mouse.4"),
            ("mouseforward", "key.mouse.5"),
            ("key.mouse.back", "key.mouse.4"),
            ("mouse6", "key.mouse.6"),
            ("rshift", "key.keyboard.right.shift"),
            ("`", "key.keyboard.grave.accent"),
            ("r", "key.keyboard.r"),
            ("kp1", "key.keyboard.keypad.1"),
            ("f6", "key.keyboard.f6"),
        ] {
            assert_eq!(canonical(alias).as_deref(), Some(expected), "{alias}");
        }
        assert!(canonical("mouse0").is_none());
        assert!(canonical("key.mouse.unknown").is_none());
    }

    #[test]
    fn a_repeat_is_not_a_press_and_a_release_rearms() {
        let mut inputs = Inputs::default();
        assert!(inputs.press("r"));
        assert!(!inputs.press("r"));
        assert!(inputs.release("r").is_empty());
        assert!(inputs.press("r"));
    }

    #[test]
    fn a_hold_is_never_saved_and_its_release_puts_back_the_state_before() {
        let mut inputs = Inputs::default();
        assert!(inputs.press("mouse4"));
        inputs.hold(1, "mouse4", false);
        inputs.hold(2, "mouse4", true);
        assert!(!inputs.saved_on(1, true));
        assert!(inputs.saved_on(2, true));
        assert!(!inputs.saved_on(2, false));
        let released = inputs.release("mouse4");
        assert_eq!(released.len(), 2);
        assert!(released.iter().any(|h| h.module == 1 && !h.previous));
        assert!(released.iter().any(|h| h.module == 2 && h.previous));
    }

    #[test]
    fn suspending_ends_holds_and_needs_a_new_press() {
        let mut inputs = Inputs::default();
        assert!(inputs.press("r"));
        inputs.hold(0, "r", false);
        assert_eq!(inputs.suspend().len(), 1);
        assert!(!inputs.press("r"));
        inputs.release("r");
        assert!(inputs.press("r"));
    }

    #[test]
    fn a_press_a_screen_had_or_focus_made_is_no_press_until_let_go() {
        let mut inputs = Inputs::default();
        inputs.block("enter");
        assert!(!inputs.press("enter"));
        inputs.release("enter");
        assert!(inputs.press("enter"));
        inputs.synthetic("r");
        assert!(inputs.is_synthetic("r") && !inputs.press("r"));
        inputs.release("r");
        assert!(!inputs.is_synthetic("r") && inputs.press("r"));
    }

    #[test]
    fn a_missed_release_is_found_from_what_is_down() {
        let mut inputs = Inputs::default();
        inputs.press("r");
        inputs.press("mouse4");
        inputs.hold(0, "r", false);
        inputs.hold(1, "mouse4", false);
        let released = inputs.missing(|key| key == "r");
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].module, 1);
    }

    #[test]
    fn only_modules_that_only_draw_can_be_held() {
        assert!(holds("Zoom") && holds("ESP"));
        for module in ["Reach", "Air Place", "Flight", "Step", "Freecam", "Timer", "Xray", "No Render", "Fullbright"] {
            assert!(!holds(module), "{module}");
        }
    }
}
