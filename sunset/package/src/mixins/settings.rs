// Sunset's page in Options > Mods: the menu's buttons as the page's rows (its Config button shows while it has rows).
fn sunset_settings(value: Vec<NativeSetting>, game: &Game<'_>, mod_id: &str) -> Vec<NativeSetting> {
    if mod_id != crate::game::sunset::ID {
        return value;
    }
    crate::game::sunset_runtime_menu::page_rows(game)
        .into_iter()
        .enumerate()
        .map(|(i, (label, on))| NativeSetting {
            key: format!("menu.{i}"),
            label,
            tooltip: None,
            value: NativeSettingValue::Toggle(on),
        })
        .collect()
}
