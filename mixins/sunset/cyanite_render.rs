// Profession-aware filters extend Sunset's existing ESP module.
use super::sunset::Module;

pub(super) fn villager_wanted(m: &Module, mob: &horizon_client::mobs::Mob) -> bool {
    if mob.kind.id != "minecraft:villager" { return true; }
    if mob.is_baby() && !m.b("Villager babies") { return false; }
    let profession = horizon_client::mobs::render::PROFESSIONS
        .get(mob.extra.variant2 as usize).copied().unwrap_or("none");
    match m.p("Villager filter") {
        "Employed" => profession != "none" && profession != "nitwit",
        "Librarians" => profession == "librarian",
        "Master level" => mob.extra.variant3 >= 5,
        _ => true,
    }
}

pub(super) fn villager_color(m: &Module, mob: &horizon_client::mobs::Mob) -> Option<u32> {
    if !m.b("Villager profession colors") || mob.kind.id != "minecraft:villager" { return None; }
    let profession = horizon_client::mobs::render::PROFESSIONS
        .get(mob.extra.variant2 as usize).copied().unwrap_or("none");
    Some(match profession {
        "librarian" => 0xFFFF_5050, "farmer" => 0xFFFF_CC50,
        "armorer" | "weaponsmith" | "toolsmith" => 0xFF70_AFFF,
        "cleric" => 0xFFC0_70FF, _ => 0xFF60_E090,
    })
}
