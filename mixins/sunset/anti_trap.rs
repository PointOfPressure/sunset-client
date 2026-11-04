// No Interact's Anti trap: entities a trap TPA baits you with (armor stands and minecarts by default; item frames,
// boats and end crystals on their switches) gone within Trap range: their spawns dropped as they arrive (Drop spawns)
// and those loaded already taken out each tick (Remove loaded); Anti sign drops the server's sign editor, so a sign
// put in your face can't hold you in its screen.
use super::Game;
use super::sunset::{Module, with};

const MINECARTS: [&str; 7] = ["minecart", "chest_minecart", "command_block_minecart", "furnace_minecart", "hopper_minecart", "spawner_minecart", "tnt_minecart"];
const WOODS: [&str; 10] = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak", "bamboo"];

fn kinds(m: &Module) -> Vec<String> {
    let mut out = Vec::new();
    let mut add = |name: &str| out.push(format!("minecraft:{name}"));
    if m.b("Hide armor stands") {
        add("armor_stand");
    }
    if m.b("Hide minecarts") {
        MINECARTS.iter().for_each(|n| add(n));
    }
    if m.b("Hide item frames") {
        add("item_frame");
        add("glow_item_frame");
    }
    if m.b("Hide boats") {
        for wood in WOODS {
            let (boat, chest) = if wood == "bamboo" { ("raft", "chest_raft") } else { ("boat", "chest_boat") };
            add(&format!("{wood}_{boat}"));
            add(&format!("{wood}_{chest}"));
        }
    }
    if m.b("Hide end crystals") {
        add("end_crystal");
    }
    out
}

pub(super) fn tick(game: &mut Game<'_>) {
    use horizon_client::net::sunset_packets as tap;
    let m = with(|s| s.module("No Interact").filter(|m| m.on && m.b("Anti trap")).cloned());
    let (Some(m), Some(p), Some(remote)) = (m, game.player.as_ref(), game.remote.as_ref()) else {
        tap::trap_off();
        return;
    };
    let at = [p.position.x, p.position.y, p.position.z];
    let range = m.n("Trap range");
    let names = kinds(&m);
    let ids = remote.builtin.get("minecraft:entity_type").map_or_else(Vec::new, |r| names.iter().filter_map(|n| r.id(n)).collect());
    tap::trap(if m.b("Drop spawns") { ids } else { Vec::new() }, at, range, m.b("Anti sign"));
    if !m.b("Remove loaded") {
        return;
    }
    let near = |x: f64, y: f64, z: f64| (x - at[0]).powi(2) + (y - at[1]).powi(2) + (z - at[2]).powi(2) <= range * range;
    let hide = |n: &str| names.iter().any(|k| k == n);
    // Never the cart or boat you ride.
    let riding = game.riding;
    if hide("minecraft:armor_stand") {
        game.armor_stands.retain(|e| !near(e.position.x, e.position.y, e.position.z));
    }
    if hide("minecraft:minecart") {
        game.carts.retain(|e| riding == Some(e.id) || !near(e.position.x, e.position.y, e.position.z));
    }
    if hide("minecraft:item_frame") {
        game.item_frames.retain(|e| !near(f64::from(e.pos[0]) + 0.5, f64::from(e.pos[1]) + 0.5, f64::from(e.pos[2]) + 0.5));
    }
    if hide("minecraft:oak_boat") {
        game.boats.retain(|e| riding == Some(e.id) || !near(e.position.x, e.position.y, e.position.z));
    }
    if hide("minecraft:end_crystal") {
        game.crystals.retain(|e| !near(e.position.x, e.position.y, e.position.z));
    }
}
