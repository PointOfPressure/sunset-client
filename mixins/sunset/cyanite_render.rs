// Extra render controls and profession-aware villager markers.
use super::Game;
use super::sunset::{Camera, Canvas, Category, Module};
use super::sunset_util::dist;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new("FOV Changer", Category::Render, "Sets the field of view, including values outside vanilla's slider.")
            .num("FOV", 100.0, 10.0, 170.0, 1.0),
        Module::new("Villager ESP", Category::Render, "Marks villagers by profession and trading level.")
            .pick("Show", &["All", "Employed", "Librarians", "Master level"])
            .bool("Babies", true).bool("Zombie villagers", true)
            .bool("Wandering traders", false).bool("Iron golems", false)
            .num("Range", 128.0, 16.0, 512.0, 16.0)
            .bool("Labels", true).num("Label range", 48.0, 8.0, 256.0, 8.0)
            .bool("Profession colors", true).colour("Color", "Green"),
    ]
}

pub(super) fn fov(game: &Game<'_>) -> Option<f32> {
    game.native_on(super::sunset::ID).then(|| {
        super::sunset::with(|s| s.module("FOV Changer").filter(|m| m.on).map(|m| m.n("FOV") as f32))
    }).flatten()
}

pub(super) fn draw(game: &Game<'_>, cam: &Camera, mods: &[Module], out: &mut impl Canvas, lw: f32) {
    let Some(m) = mods.iter().find(|m| m.name == "Villager ESP" && m.on) else { return };
    for mob in &game.mobs {
        let villager = mob.kind.id == "minecraft:villager";
        let wanted = villager
            || (mob.kind.id == "minecraft:zombie_villager" && m.b("Zombie villagers"))
            || (mob.kind.id == "minecraft:wandering_trader" && m.b("Wandering traders"))
            || (mob.kind.id == "minecraft:iron_golem" && m.b("Iron golems"));
        if !wanted || (!m.b("Babies") && mob.is_baby()) { continue; }
        let profession = horizon_client::mobs::render::PROFESSIONS
            .get(mob.extra.variant2 as usize).copied().unwrap_or("none");
        if villager && match m.p("Show") {
            "Employed" => profession == "none" || profession == "nitwit",
            "Librarians" => profession != "librarian",
            "Master level" => mob.extra.variant3 < 5,
            _ => false,
        } { continue; }
        let at = [mob.position.x, mob.position.y, mob.position.z];
        let distance = dist(cam.pos, at);
        if distance > m.n("Range") { continue; }
        let color = if m.b("Profession colors") && villager {
            match profession {
                "librarian" => 0xFFFF_5050, "farmer" => 0xFFFF_CC50,
                "armorer" | "weaponsmith" | "toolsmith" => 0xFF70_AFFF,
                "cleric" => 0xFFC0_70FF, _ => 0xFF60_E090,
            }
        } else { m.c("Color") };
        let half = if mob.kind.id == "minecraft:iron_golem" { 0.7 } else { 0.3 };
        let height = if mob.is_baby() { 1.0 } else if mob.kind.id == "minecraft:iron_golem" { 2.7 } else { 1.95 };
        super::sunset_visual::draw_box(cam, out, &super::sunset_visual::Look::BOXED,
            [at[0] - half, at[1], at[2] - half], [at[0] + half, at[1] + height, at[2] + half], color, lw);
        if m.b("Labels") && distance <= m.n("Label range")
            && let Some([x, y]) = cam.project([at[0], at[1] + height + 0.15, at[2]])
        {
            let name = if villager {
                format!("{} L{} ({:.0}m)", profession, mob.extra.variant3.max(1), distance)
            } else {
                format!("{} ({:.0}m)", super::sunset_util::short(&mob.kind.id), distance)
            };
            out.text(&name, [x - game.native_text_width(&name) / 2.0, y], color);
        }
    }
}
