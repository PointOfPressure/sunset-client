// ESP's villager settings: which villagers it shows (all, employed, librarians, master level, babies or not) and
// a colour per profession.
use super::sunset::Module;
use horizon_client::mobs::Mob;

fn profession(mob: &Mob) -> String {
    let id = mob.extra.profession.id();
    id.strip_prefix("minecraft:").unwrap_or(&id).to_owned()
}

pub(super) fn villager_wanted(m: &Module, mob: &Mob) -> bool {
    if mob.kind.id != "minecraft:villager" { return true; }
    if mob.is_baby() && !m.b("Villager babies") { return false; }
    match m.p("Villager filter") {
        "Employed" => !matches!(profession(mob).as_str(), "none" | "nitwit"),
        "Librarians" => profession(mob) == "librarian",
        "Master level" => mob.extra.villager_level >= 5,
        _ => true,
    }
}

pub(super) fn villager_color(m: &Module, mob: &Mob) -> Option<u32> {
    if !m.b("Villager profession colors") || mob.kind.id != "minecraft:villager" { return None; }
    Some(match profession(mob).as_str() {
        "librarian" => 0xFFFF_5050,
        "farmer" => 0xFFFF_CC50,
        "armorer" | "weaponsmith" | "toolsmith" => 0xFF70_AFFF,
        "cleric" => 0xFFC0_70FF,
        _ => 0xFF60_E090,
    })
}
