// The game's helpers for native mods that Sunset was written against (Horizon nightly 9f42064), on this game build's
// own: `on`, `playing`, `setting` and `set_setting` took their place. An extension trait, as a support module can't
// add inherent methods to the game's type; the modules that use them import it.
use super::Game;
use horizon_items::equipment::{CHEST, FEET, HEAD, LEGS, MAINHAND, OFFHAND};
use std::sync::Arc;

// A mob's equipment as Sunset read it (ids per hand, armour head to feet), from this build's slots.
pub(crate) trait SunsetExtra {
    fn sunset_main_hand(&self) -> Option<Arc<str>>;
    fn sunset_off_hand(&self) -> Option<Arc<str>>;
    // Armour `i`, head (0) to feet (3).
    fn sunset_armor(&self, i: usize) -> Option<&horizon_client::items::ItemStack>;
    fn sunset_armor_id(&self, i: usize) -> Option<Arc<str>>;
    // Using the item in a hand that holds a shield (the use's delay is the caller's to judge).
    fn sunset_blocking(&self) -> bool;
}

const ARMOR: [usize; 4] = [HEAD, CHEST, LEGS, FEET];

impl SunsetExtra for horizon_client::mobs::Extra {
    fn sunset_main_hand(&self) -> Option<Arc<str>> {
        self.slot_stack(MAINHAND).map(|s| s.item.clone())
    }
    fn sunset_off_hand(&self) -> Option<Arc<str>> {
        self.slot_stack(OFFHAND).map(|s| s.item.clone())
    }
    fn sunset_armor(&self, i: usize) -> Option<&horizon_client::items::ItemStack> {
        ARMOR.get(i).and_then(|&slot| self.slot_stack(slot))
    }
    fn sunset_armor_id(&self, i: usize) -> Option<Arc<str>> {
        self.sunset_armor(i).map(|s| s.item.clone())
    }
    fn sunset_blocking(&self) -> bool {
        let hand = if self.using_off_hand { OFFHAND } else { MAINHAND };
        self.using_item > 0 && self.slot_stack(hand).is_some_and(|s| s.item.ends_with("shield"))
    }
}

// The held keys (horizon_client::held::HeldKeys) as Sunset changes them, a set of keys as before the game told the
// devices apart: a key Sunset holds is held as the keyboard holds it (HeldKeys::insert), and letting a key go lets it
// go whatever held it. Never Source::Mod: Sunset gives back a key you hold by holding it again, and the keyboard
// letting go releases only its own hold, so a key held as a mod's stayed down after you let it go (Telly kept
// walking and sprinting after it was turned off).
pub(crate) fn key_down(keys: &mut horizon_client::held::HeldKeys, key: winit::keyboard::KeyCode) {
    keys.insert(key);
}
pub(crate) fn key_up(keys: &mut horizon_client::held::HeldKeys, key: &winit::keyboard::KeyCode) {
    keys.remove(key);
}
pub(crate) fn keys_clear(keys: &mut horizon_client::held::HeldKeys) {
    keys.clear();
}

// Puts a copy of the local player's gear on a mob (Fake Player's Copy inventory).
pub(crate) fn wear(extra: &mut horizon_client::mobs::Extra, off_hand: Option<horizon_client::items::ItemStack>, armor: &[Option<horizon_client::items::ItemStack>; 4]) {
    extra.set_slot_stack(OFFHAND, off_hand);
    for (i, slot) in ARMOR.into_iter().enumerate() {
        extra.set_slot_stack(slot, armor[i].clone());
    }
}

pub(crate) trait SunsetGame {
    // Whether mod `mod_id` is turned on.
    fn native_on(&self, mod_id: &str) -> bool;
    // Playing in a world, no screen over it.
    fn native_playing(&self) -> bool;
    // A mod's kept setting `key`.
    fn native_setting(&self, mod_id: &str, key: &str) -> Option<String>;
    // Keeps a mod's setting `key`.
    fn native_set_setting(&self, mod_id: &str, key: &str, value: &str);
    // The key a mod's key mapping `name` is bound to, kept as Sunset's own setting (`key.<name>`): `default` until the
    // player binds another.
    fn native_mod_key(&self, mod_id: &str, name: &str, default: &str) -> String;
    // This game is the window's, not a server's (a LAN host's or the integrated server's own).
    fn sunset_window(&self) -> bool;
    // A text's width as Sunset's own text draws it (native_text): Sunset's font when it is on, else the game's.
    fn sunset_text_width(&self, text: &str) -> f32;
    // Whether block `block` (its id) is in block tag `tag`.
    fn sunset_block_in_tag(&self, tag: &str, block: &str) -> bool;
    // A mob's worn armour and toughness, from its armour's item data.
    fn sunset_worn_armor(&self, mob: &horizon_client::mobs::Mob) -> (f32, f32);
    // The local world's seed (single player and LAN hosting), as the server's level has it.
    fn sunset_local_seed(&self) -> Option<i64>;
}

impl SunsetGame for Game<'_> {
    fn native_on(&self, mod_id: &str) -> bool {
        self.on(mod_id)
    }
    fn native_playing(&self) -> bool {
        self.playing()
    }
    fn native_setting(&self, mod_id: &str, key: &str) -> Option<String> {
        self.setting(mod_id, key)
    }
    fn native_set_setting(&self, mod_id: &str, key: &str, value: &str) {
        self.set_setting(mod_id, key, value);
    }
    fn native_mod_key(&self, mod_id: &str, name: &str, default: &str) -> String {
        self.setting(mod_id, &format!("key.{name}")).unwrap_or_else(|| default.to_owned())
    }
    fn sunset_window(&self) -> bool {
        self.options.execution_role == super::execution_role::ExecutionRole::Client
    }
    fn sunset_text_width(&self, text: &str) -> f32 {
        crate::game::sunset_font::ensure(&self.gui.atlas.font);
        if crate::game::sunset_font::enabled() {
            self.gui.atlas.font.width_text(&crate::game::sunset_font::text(text))
        } else {
            horizon_client::hud::HudBatch::text_width(&self.gui.atlas, text)
        }
    }
    fn sunset_block_in_tag(&self, tag: &str, block: &str) -> bool {
        let tag = tag.strip_prefix('#').unwrap_or(tag);
        self.res.items().block_tags.get(tag).is_some_and(|s| s.contains(block))
    }
    fn sunset_worn_armor(&self, mob: &horizon_client::mobs::Mob) -> (f32, f32) {
        let items = self.res.items();
        (0..4)
            .filter_map(|i| mob.extra.sunset_armor(i))
            .filter_map(|stack| items.get(&stack.item))
            .fold((0.0, 0.0), |(a, t), info| {
                let sum = |name: &str| info.modifiers.iter().filter(|m| m.attribute == name).map(|m| m.amount as f32).sum::<f32>();
                (a + sum("minecraft:armor"), t + sum("minecraft:armor_toughness"))
            })
    }
    fn sunset_local_seed(&self) -> Option<i64> {
        self.remote.is_none().then_some(self.level.seed)
    }
}
