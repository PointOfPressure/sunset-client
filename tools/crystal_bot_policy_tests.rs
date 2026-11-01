#[path = "../mixins/sunset/crystal_bot_policy.rs"]
mod policy;
use policy::*;








#[test]
fn module_ownership_never_restores_over_user_changes() {
    let mut current = true;
    let mut lease = Owned::new(false, true);
    assert!(lease.update(&mut current, false));
    assert!(lease.restore(&mut current));
    assert!(!current);
    let mut current = false;
    let mut lease = Owned::new(false, true);
    assert!(!lease.update(&mut current, true));
    assert!(!lease.restore(&mut current));
    assert!(!current);
}
#[test]
fn movement_rejects_unknown_drops_and_caps_turns() {
    assert!(support_safe(true, Some(0.5), false));
    assert!(!support_safe(true, None, false));
    assert!(!support_safe(true, Some(2.0), false));
    assert!(!support_safe(true, Some(0.5), true));
    let d = camera_step([179.0, 0.0], [-179.0, 0.0], 1.0).unwrap();
    assert_eq!(d, [2.0, 0.0]);
    let d = camera_step([0.0, 0.0], [90.0, 40.0], 1.0).unwrap();
    assert!((d[0].hypot(d[1]) - 18.0).abs() < 1e-9);
}


#[test]
fn repeated_gui_pause_keeps_false_owned_until_a_real_user_override() {
    let mut value = true;
    let mut context = Some(Owned::new(false, true));
    for _ in 0..4 {
        assert!(context.as_mut().unwrap().update(&mut value, false));
    }
    assert!(!value);
    assert!(context.is_some());
    value = true;
    assert!(!context.as_mut().unwrap().update(&mut value, false));
    assert!(!context.take().unwrap().restore(&mut value));
    assert!(value);
}

#[test]
fn partial_restore_does_not_erase_entities_set_during_chat() {
    let mut enabled = false;
    let on = Owned::new(true, false);
    let mut entities = "player";
    let mut owned = Owned::new("@all", "player");
    assert!(owned.update(&mut entities, "player"));
    entities = "husk";
    assert!(!owned.update(&mut entities, "player"));
    assert!(on.restore(&mut enabled));
    assert!(enabled);
    assert!(!owned.restore(&mut entities));
    assert_eq!(entities, "husk");
}

#[test]
fn serialized_config_projects_only_owned_fields_and_keeps_user_edits() {
    let crystal = Owned::new(false, true);
    let anchor = Owned::new(true, false);
    let hotbar = Owned::new(false, true);
    let entities = Owned::new("@all", "player");
    let live = (true, false, true, "husk", 16.0);
    let mut saved = live;
    crystal.baseline(&mut saved.0);
    anchor.baseline(&mut saved.1);
    hotbar.baseline(&mut saved.2);
    assert!(!entities.baseline(&mut saved.3));
    let text = format!(
        "crystal={}\nanchor={}\nhotbar={}\nEntities={}\nMaxSelf={}\n",
        saved.0, saved.1, saved.2, saved.3, saved.4
    );
    assert_eq!(
        text,
        "crystal=false\nanchor=true\nhotbar=false\nEntities=husk\nMaxSelf=16\n"
    );
    assert_eq!(live, (true, false, true, "husk", 16.0));
    let mut again = live;
    crystal.baseline(&mut again.0);
    anchor.baseline(&mut again.1);
    hotbar.baseline(&mut again.2);
    assert_eq!(again, saved);
}

#[test]
fn yielded_persistence_keeps_later_explicit_user_value() {
    let mut lease = Owned::new(false, true);
    let mut value = false;
    assert!(!lease.update(&mut value, true));
    value = true;
    assert!(!lease.baseline(&mut value));
    assert!(value);
}

#[test]
fn same_temporary_value_user_edit_survives_restore_then_write() {
    let mut intent = Intent::default();
    let mut value = true;
    let mut context = Some(Owned::new(false, true)); // Bot enabled hotbar totems.
    assert!(intent.editing(false, context.is_some()));
    assert!(context.take().unwrap().restore(&mut value));
    assert!(!value);
    value = true; // Explicit .set true matches the old temporary value.
    let mut persisted = value;
    if let Some(lease) = &context {
        lease.baseline(&mut persisted);
    }
    assert_eq!(format!("UseHotbar={persisted}\n"), "UseHotbar=true\n");
    assert!(intent.yielded);
    assert!(context.is_none());
}

#[test]
fn own_settings_and_immediate_off_on_release_the_intent_latch() {
    let mut intent = Intent::default();
    assert!(!intent.editing(false, false));
    assert!(intent.editing(false, true));
    assert!(intent.yielded);
    intent.disabled();
    assert!(!intent.yielded); // No game tick is needed.
    assert!(intent.editing(false, true));
    assert!(intent.yielded);
    assert!(intent.editing(true, false));
    assert!(!intent.yielded);
}
