// SPDX-License-Identifier: MIT
// Original implementation assisted by OpenAI Codex (GPT-6.1 Sol).
use super::*;
use crate::text_edit::{Draft, EditKey};

fn state() -> State {
    State {
        modules: vec![
            Module::new("Spam", Category::Player, "test")
                .text("Messages", "original")
                .bool("Flag", false)
                .num("Amount", 1.0, 0.0, 10.0, 1.0),
        ],
        friends: vec![],
        waypoints: vec![],
        gui: Gui { panels: vec![] },
    }
}

#[test]
fn explicit_apply_survives_real_config_serializer_and_loader() {
    let mut original = state();
    let value = "  /msg friend hello, world! | another=message: é 🦀  ";
    let mut draft = Draft::new(original.modules[0].t("Messages")).unwrap();
    draft.key(EditKey::SelectAll);
    draft.insert(value);
    assert_eq!(original.modules[0].t("Messages"), "original");
    // Same verbatim assignment as the GUI's Apply path.
    original.modules[0].settings[0].value = Value::Text(draft.value().unwrap().to_owned());
    let serialized = original.save_text();
    let mut loaded = state();
    loaded.load_text(&serialized);
    assert_eq!(loaded.modules, original.modules);
    assert_eq!(loaded.modules[0].t("Messages"), value);
    // Empty strings, all spaces and embedded '=' are valid text too.
    for value in ["", "   ", "=|", "a=b=c"] {
        original.modules[0].settings[0].value = Value::Text(value.to_owned());
        loaded.load_text(&original.save_text());
        assert_eq!(loaded.modules[0].t("Messages"), value);
    }
}

#[test]
fn command_parse_still_trims_and_nontext_config_rules_are_unchanged() {
    let mut state = state();
    assert!(state.modules[0].settings[0].parse("  chat command value  "));
    assert_eq!(state.modules[0].t("Messages"), "chat command value");
    state.load_text("spam.flag=  true  \nspam.amount=  4  \nspam.messages=  persisted value  \n");
    assert!(state.modules[0].b("Flag"));
    assert_eq!(state.modules[0].n("Amount"), 4.0);
    assert_eq!(state.modules[0].t("Messages"), "  persisted value  ");
}

#[test]
fn logical_navigation_drives_the_production_draft() {
    use crate::winit::keyboard::{Key, NamedKey};
    // The same logical actions can originate from navigation keys, layout
    // remappings, or the keypad with NumLock off.
    for (key, start_at_home, expected) in [
        (NamedKey::Home, false, "!abc"),
        (NamedKey::End, true, "abc!"),
        (NamedKey::ArrowLeft, false, "ab!c"),
        (NamedKey::ArrowRight, true, "a!bc"),
        (NamedKey::Backspace, false, "ab!"),
        (NamedKey::Delete, true, "!bc"),
    ] {
        let mut draft = Draft::new("abc").unwrap();
        if start_at_home { draft.key(EditKey::Home); }
        draft.key(text_edit_key(&Key::Named(key)).unwrap());
        draft.insert("!");
        assert_eq!(draft.value(), Ok(expected));
    }
    assert!(text_edit_key(&Key::Character("4".into())).is_none());
    assert!(text_edit_key(&Key::Named(NamedKey::Escape)).is_none());
    assert!(text_edit_key(&Key::Unidentified).is_none());
}
