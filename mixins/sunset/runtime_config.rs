// Runtime part (see runtime.rs): reading and merging the config and profiles, presets' previews.

use super::sunset::*;

// What a preset turns on and off, and the number of settings it changes.
#[derive(Clone, Debug, Default)]
pub(super) struct Preview {
    pub(super) on: Vec<&'static str>,
    pub(super) off: Vec<&'static str>,
    pub(super) settings: usize,
}

// Offhand's settings from before Sunset's Auto Totem (0.9): set_field turns Health, Refill and Swap delay into the new
// ones, and the rest are gone.
pub(super) const OFFHAND_RETIRED: [&str; 10] =
    ["health", "elytra", "fall", "explosion", "refill", "reactiondelay", "swapdelay", "closedelay", "switchbackdelay", "doublehand"];

// Settings that are gone: Client Sort's old orders, lists and pace (0.9), PvP Bot's City (with Auto City, after
// 1.1.0). Their lines go.
const DROPPED: [(&str, &str); 11] = [
    ("clientsort", "order"), ("clientsort", "shiftorder"), ("clientsort", "ctrlorder"), ("clientsort", "altorder"),
    ("clientsort", "hotbar"), ("clientsort", "startitems"), ("clientsort", "enditems"), ("clientsort", "clickspertick"),
    ("pvpbot", "city"),
    // Ghost Hand's Hits and Cut through are Hit Through Walls (merge_lines).
    ("ghosthand", "hits"), ("ghosthand", "cutthrough"),
];

// Offhand took in Auto Totem, Auto Inventory Totem and Hover Totem, Fast Use took in FastXP, Velocity took in
// Knockback Delay, Kill Aura and Trigger Bot took in Auto Shield and Auto Weapon took in Shield Breaker, Mace and
// Spear Swap (0.9): a config or profile from before has their
// lines turned into the new module's. FastXP's are Throwables; Fast Use is on if either was (only for throwables, with
// an empty list, when Fast Use itself was off).
// Offhand is on if any of its three was, with the first key bound; the old Offhand's Item when it was on, else a totem
// always (Auto Totem Strict) or only when needed (None), else the new default; Refill is Auto Inventory Totem's Mode.
// Modules renamed since go by their new names first, and a config from before Extra Modules lists the extras it uses.
// Extra Modules is Modules (its switches the same; Pearl Catch shows unless switched off since).
// A profile as this version reads it: the lines it loads, and those it left out (what each was).
#[derive(Debug, Default)]
pub(super) struct Checked {
    pub(super) body: String,
    pub(super) skipped: Vec<String>,
}

impl Checked {
    // What was left out, for the message after a load or an import ("" when nothing was).
    pub(super) fn skipped_note(&self) -> String {
        match self.skipped.as_slice() {
            [] => String::new(),
            [one] => format!(" Left out a line this version doesn't know: {one}."),
            [first, rest @ ..] => format!(" Left out {} lines this version doesn't know, the first {first}.", rest.len() + 1),
        }
    }
}

// What a profile never carries: your friends (ClickGUI's Friends list: a profile from before you met someone would
// take them off it) and Panic's switch (a profile with it on would shut Sunset down as it loads).
pub(super) fn not_in_profiles(key: &str) -> bool {
    // Sunset Presence is only ever set by hand: a shared profile can't turn sharing on (or analytics off). ClickGUI's
    // switch is whether the menu is open (mirror_gui), never a profile's.
    matches!(key.trim(), "clickgui.friends" | "clickgui.on" | "panic.on") || key.trim().starts_with("sunsetpresence.")
}

pub(super) fn merged_modules(text: &str) -> std::borrow::Cow<'_, str> {
    let renamed = renamed_modules(text);
    let renamed = match elytra_boost_joined(&renamed) {
        Some(joined) => std::borrow::Cow::Owned(joined),
        None => renamed,
    };
    let owned = match merge_lines(&renamed) {
        std::borrow::Cow::Owned(out) => Some(out),
        std::borrow::Cow::Borrowed(_) => None,
    };
    moved_messages(super::sunset_lag::migrated(listed_extras(owned.map_or(renamed, std::borrow::Cow::Owned))))
}

// ClickGUI's Toggle messages and Module messages moved to Notifications (1.3), as Module toggles and Module messages.
pub(super) const MOVED_MESSAGES: [(&str, &str); 2] =
    [("clickgui.togglemessages=", "notifications.moduletoggles="), ("clickgui.modulemessages=", "notifications.modulemessages=")];

fn moved_messages(text: std::borrow::Cow<'_, str>) -> std::borrow::Cow<'_, str> {
    if !text.lines().any(|l| MOVED_MESSAGES.iter().any(|(from, _)| l.starts_with(from))) {
        return text;
    }
    let lines: Vec<String> = text
        .lines()
        .map(|l| match MOVED_MESSAGES.iter().find(|(from, _)| l.starts_with(from)) {
            Some((from, to)) => format!("{to}{}", &l[from.len()..]),
            None => l.to_owned(),
        })
        .collect();
    std::borrow::Cow::Owned(lines.join("\n"))
}

// Modules with new names (0.9): Auto Steal is Chest Stealer, No Mining Trace is Mine Through Mobs; Inventory Cleaner is
// Inventory Manager, Mob Gear ESP is Gear ESP (1.0); Key Throw is Tap Use and Hold to Eat is Eat Tap (after 1.1.0);
// Surround is BlockIn and Speed Mine is Fast Break (1.3).
pub(super) const RENAMED: [(&str, &str); 10] = [("autosteal", "cheststealer"), ("nominingtrace", "minethroughmobs"), ("swordpvpbot", "pvpbot"),
    ("extramodules", "modules"), ("inventorycleaner", "inventorymanager"), ("mobgearesp", "gearesp"), ("keythrow", "tapuse"),
    ("holdtoeat", "eattap"), ("surround", "blockin"), ("speedmine", "fastbreak")];

// Auto Armor joined Inventory Manager (1.0): its on/off is the Auto Armor setting, its key Auto Armor key, its Delay
// and Mode Armor delay and Armor mode; the rest keep their names.
fn auto_armor_line(l: &str) -> Option<String> {
    let (field, value) = l.strip_prefix("autoarmor.")?.split_once('=')?;
    let field = match field {
        "on" => "autoarmor",
        "key" => "autoarmorkey",
        "delayticks" => "armordelayticks",
        "mode" => "armormode",
        other => other,
    };
    let value = if field == "autoarmorkey" && value == "none" { "None" } else { value };
    Some(format!("inventorymanager.{field}={value}"))
}

// The Modules list's line for a module that was renamed (`modules.inventorycleaner=` is now
// `modules.inventorymanager=`), or None for one that is gone (Auto Armor: it is Inventory Manager's now, and that has
// its own line). Other lines: Some(unchanged).
fn modules_list_line(l: &str) -> Option<String> {
    let Some((field, value)) = l.strip_prefix("modules.").and_then(|r| r.split_once('=')) else {
        return Some(l.to_owned());
    };
    if field == "autoarmor" {
        return None;
    }
    match RENAMED.iter().find(|(from, _)| *from == field) {
        Some((_, to)) => Some(format!("modules.{to}={value}")),
        None => Some(l.to_owned()),
    }
}

fn renamed_modules(text: &str) -> std::borrow::Cow<'_, str> {
    let old = |l: &str| RENAMED.iter().find(|(from, _)| l.split_once('.').is_some_and(|(id, _)| id == *from));
    let listed_old = |l: &str| modules_list_line(l).as_deref() != Some(l);
    if !text.lines().any(|l| old(l).is_some() || l.starts_with("autoarmor.") || listed_old(l)) {
        return std::borrow::Cow::Borrowed(text);
    }
    let lines: Vec<String> = text
        .lines()
        .filter(|l| l.trim() != "extramodules.pearlcatch=false")
        .filter_map(modules_list_line)
        .map(|l| match old(&l) {
            Some((from, to)) => format!("{to}{}", &l[from.len()..]),
            None => auto_armor_line(&l).unwrap_or_else(|| l.clone()),
        })
        .collect();
    std::borrow::Cow::Owned(lines.join("\n"))
}

// A config or profile with no Modules lines (from before Extra Modules) has the hidden modules it turns on or binds
// listed.
fn listed_extras(text: std::borrow::Cow<'_, str>) -> std::borrow::Cow<'_, str> {
    if text.lines().any(|l| l.starts_with("modules.")) {
        return text;
    }
    let used: Vec<String> = super::sunset_extra_modules::HIDDEN
        .iter()
        .map(|name| norm(name))
        .filter(|extra| {
            let last = |field: &str| {
                text.lines().rev().find_map(|l| l.strip_prefix(&format!("{extra}.{field}=")).map(str::trim))
            };
            last("on") == Some("true") || last("key").is_some_and(|k| k != "none")
        })
        .collect();
    if used.is_empty() {
        return text;
    }
    let mut out = text.into_owned();
    for extra in used {
        out.push_str(&format!("\nmodules.{extra}=true"));
    }
    std::borrow::Cow::Owned(out)
}

// Elytra Boost joined Elytra Fly (1.3) as its Boost mode: its Boost and Max speed carry over, and an Elytra Boost in
// use (on or bound) with Elytra Fly not (off and unbound) makes Elytra Fly a Boost one, on, bound and listed as it
// was. Its other lines go. None: nothing of Elytra Boost's in the text.
fn elytra_boost_joined(text: &str) -> Option<String> {
    const OLD: &str = "elytraboost.";
    if !text.lines().any(|l| l.starts_with(OLD) || l.starts_with("modules.elytraboost=")) {
        return None;
    }
    let last = |key: &str| text.lines().rev().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')).map(str::trim));
    let used = |id: &str| last(&format!("{id}.on")) == Some("true") || last(&format!("{id}.key")).is_some_and(|k| k != "none");
    let mut out: Vec<String> = text
        .lines()
        .filter(|l| !l.starts_with(OLD) && !l.starts_with("modules.elytraboost="))
        .map(str::to_owned)
        .collect();
    for field in ["boost", "maxspeed"] {
        if let Some(v) = last(&format!("{OLD}{field}")) {
            out.push(format!("elytrafly.{field}={v}"));
        }
    }
    if used("elytraboost") && !used("elytrafly") {
        out.push("elytrafly.mode=Boost".into());
        for field in ["on", "key"] {
            if let Some(v) = last(&format!("{OLD}{field}")) {
                out.push(format!("elytrafly.{field}={v}"));
            }
        }
        if let Some(v) = last("modules.elytraboost") {
            out.push(format!("modules.elytrafly={v}"));
        }
    }
    Some(out.join("\n"))
}

// Which of the menu's hidden-by-default lists a config was saved with: 2 is 1.3's (HIDDEN_SINCE_1_3 joined it).
pub(super) const MENU_HIDDEN: &str = "menu-hidden";
pub(super) const MENU_HIDDEN_LIST: u32 = 2;

// A config saved before 1.3 lists Self Trap, Anvil, Quiver, PvP Bot, Middle Click Extra, Enderman Look and Spawner
// Alerts shown (its Modules lines hold every switch): they go hidden, once, unless it turns one on or binds it. A
// config saved since says menu-hidden=2, so a module shown again stays shown. One with no Modules lines at all is
// listed_extras's.
pub(super) fn hidden_since_1_3(text: &str) -> std::borrow::Cow<'_, str> {
    let saved_with = |l: &str| l.strip_prefix(MENU_HIDDEN).and_then(|r| r.strip_prefix('=')).and_then(|v| v.trim().parse::<u32>().ok());
    if text.lines().filter_map(saved_with).any(|n| n >= MENU_HIDDEN_LIST) || !text.lines().any(|l| l.starts_with("modules.")) {
        return std::borrow::Cow::Borrowed(text);
    }
    let last = |id: &str, field: &str| text.lines().rev().find_map(|l| l.strip_prefix(&format!("{id}.{field}=")).map(str::trim));
    let mut out = text.to_owned();
    for id in super::sunset_extra_modules::HIDDEN_SINCE_1_3.iter().map(|name| norm(name)) {
        if last(&id, "on") != Some("true") && last(&id, "key").is_none_or(|k| k == "none") {
            out.push_str(&format!("\nmodules.{id}=false"));
        }
    }
    std::borrow::Cow::Owned(out)
}

fn merge_lines(text: &str) -> std::borrow::Cow<'_, str> {
    let lines: Vec<(&str, &str, &str)> = text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let (id, field) = k.split_once('.')?;
            Some((id, field, v.trim()))
        })
        .collect();
    let get = |id: &str, field: &str| lines.iter().rev().find(|l| l.0 == id && l.1 == field).map(|l| l.2);
    let had = |id: &str| lines.iter().any(|l| l.0 == id);
    let on = |id: &str| get(id, "on") == Some("true");
    // The old Offhand had Min health where the new one has Health.
    let old_offhand = get("offhand", "minhealth").is_some();
    let gone = [
        "autototem", "autoinventorytotem", "hovertotem", "fastxp", "knockbackdelay", "autoshield", "shieldbreaker", "mace",
        "spearswap", "crystalmacro", "anchormacro", "antianvil", "selfanvil", "autoanvil", "anchor", "reversestep",
        // Potion Saver (gone in 0.9) only held the timer you see; the server's ran down all the same.
        "potionsaver",
        // Bow Spam and Anti Bed are gone; Rotation and No Rotate are Rotations' Lock and No rotate.
        "bowspam", "antibed", "rotation", "norotate",
        // Removed after 1.1.0.
        "burrow", "holefiller", "autocity", "arrowdodge", "targetesp",
    ];
    // A gone module's entry in the Modules list goes with it.
    let dropped = lines.iter().any(|&(id, field, _)| DROPPED.contains(&(id, field)) || (id == "modules" && gone.contains(&field)));
    // A config from before Auto Spear: Auto Weapon's spear lines go to it.
    let spear_moved = !had("autospear") && (get("autoweapon", "spear").is_some() || get("autoweapon", "mode") == Some("Spear"));
    if !old_offhand && !dropped && !spear_moved && !gone.iter().any(|id| had(id)) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out: Vec<String> = text
        .lines()
        .filter(|l| {
            let key = l.split_once('=').map_or("", |(k, _)| k);
            let (id, field) = key.split_once('.').unwrap_or((key, ""));
            !gone.contains(&id)
                && !(old_offhand && id == "offhand")
                && !DROPPED.contains(&(id, field))
                && !(id == "modules" && gone.contains(&field))
        })
        .map(str::to_owned)
        .collect();
    // Ghost Hand's Hits (through anything) or Cut through (only passable blocks) is Hit Through Walls, on with Ghost
    // Hand if it ran, at any entity it hit before.
    let hits = get("ghosthand", "hits") == Some("true");
    if !had("hitthroughwalls") && (hits || get("ghosthand", "cutthrough") == Some("true")) {
        out.push(format!("hitthroughwalls.on={}", on("ghosthand")));
        out.push(format!("hitthroughwalls.mode={}", if hits { "Any block" } else { "Passable only" }));
        out.push("hitthroughwalls.entities=@all".into());
        out.push("hitthroughwalls.ignorepassive=false".into());
        out.push("hitthroughwalls.passivemobagefilter=Both".into());
    }
    // Knockback Delay is Velocity's Knockback delay: on alone, Velocity comes on with its own pushes left alone.
    if had("knockbackdelay") && get("velocity", "knockbackdelay").is_none() {
        let delay = on("knockbackdelay");
        out.push(format!("velocity.knockbackdelay={delay}"));
        if delay && !on("velocity") {
            out.push("velocity.on=true".into());
            for field in ["knockback", "explosions", "liquids", "entitypush", "sinking"] {
                out.push(format!("velocity.{field}=false"));
            }
        }
        if get("velocity", "key").is_none_or(|k| k == "none")
            && let Some(key) = get("knockbackdelay", "key")
        {
            out.push(format!("velocity.key={key}"));
        }
        for (old, new) in [
            ("chance", "delaychance"), ("airdelayminms", "airdelayminms"), ("airdelaymaxms", "airdelaymaxms"),
            ("grounddelayminms", "grounddelayminms"), ("grounddelaymaxms", "grounddelaymaxms"),
            ("notinwater", "delaynotinwater"), ("onlynearenemies", "delayonlynearenemies"),
        ] {
            if let Some(value) = get("knockbackdelay", old) {
                out.push(format!("velocity.{new}={value}"));
            }
        }
    }
    // Auto Shield is Kill Aura's and Trigger Bot's Shield: on in both if it was on, with its settings.
    if had("autoshield") {
        for id in ["killaura", "triggerbot"].into_iter().filter(|id| get(id, "shield").is_none()) {
            out.push(format!("{id}.shield={}", on("autoshield")));
            for field in ["onlywithaweapon", "onlynearatarget", "range", "reraisedelay"] {
                if let Some(v) = get("autoshield", field) {
                    out.push(format!("{id}.shield{field}={v}"));
                }
            }
        }
    }
    // Shield Breaker is Auto Weapon's Axe on shields (Best weapon the old Auto Weapon): Auto Weapon on if either was,
    // then with only their sections on, and the first key bound; Holding and slot back from Shield Breaker when it was
    // on. The old Mace's lines are dropped: its section left Auto Weapon for Auto Mace. Spear Swap is Auto Spear.
    if had("spearswap") && !had("autospear") {
        out.push(format!("autospear.on={}", on("spearswap")));
        for (old_field, field) in [
            ("key", "key"), ("lungekey", "lungekey"), ("reachpoke", "reachpoke"), ("spearreach", "spearreach"), ("holding", "holding"),
            ("switchback", "swapback"), ("backafter", "backafter"), ("shortestdelay", "shortestdelay"), ("longestdelay", "longestdelay"),
        ] {
            if let Some(v) = get("spearswap", old_field) {
                out.push(format!("autospear.{field}={v}"));
            }
        }
    }
    // Auto Weapon's Spear section (or Mode Spear) is Auto Spear: on with Auto Weapon if it ran, with its settings and
    // Auto Weapon's Holding and slot back.
    if spear_moved {
        let spear = on("autoweapon") && (get("autoweapon", "spear") == Some("true") || get("autoweapon", "mode") == Some("Spear"));
        out.push(format!("autospear.on={spear}"));
        for field in [
            "lungekey", "lungeonhit", "reachpoke", "spearreach", "damageindicator", "holding", "swapback", "backafter", "shortestdelay",
            "longestdelay",
        ] {
            if let Some(v) = get("autoweapon", field) {
                out.push(format!("autospear.{field}={v}"));
            }
        }
    }
    if had("shieldbreaker") {
        let old = ["autoweapon", "shieldbreaker"];
        let weapon = get("autoweapon", "bestweapon").is_none() && on("autoweapon");
        out.push(format!("autoweapon.on={}", old.iter().any(|id| on(id))));
        if let Some(key) = old.iter().filter_map(|id| get(id, "key")).find(|k| *k != "none") {
            out.push(format!("autoweapon.key={key}"));
        }
        if old.iter().any(|id| on(id)) {
            for (id, section) in [("autoweapon", "bestweapon"), ("shieldbreaker", "axeonshields")] {
                out.push(format!("autoweapon.{section}={}", if id == "autoweapon" { weapon } else { on(id) }));
            }
        }
        for (id, old_field, field) in [
            ("shieldbreaker", "mode", "shieldmode"), ("shieldbreaker", "shieldheld", "shieldheld"),
        ] {
            if let Some(v) = get(id, old_field) {
                out.push(format!("autoweapon.{field}={v}"));
            }
        }
        if let Some(first) = ["shieldbreaker"].into_iter().find(|id| on(id)) {
            for (old_field, field) in [("holding", "holding"), ("switchback", "swapback"), ("backafter", "backafter"), ("shortestdelay", "shortestdelay"), ("longestdelay", "longestdelay")] {
                if let Some(v) = get(first, old_field) {
                    out.push(format!("autoweapon.{field}={v}"));
                }
            }
        } else if weapon {
            // Auto Weapon alone switched from anything and stayed.
            out.push("autoweapon.holding=sword,axe,spear,mace,trident,empty,other".into());
            out.push("autoweapon.swapback=false".into());
        }
    }
    if had("fastxp") {
        let (fast, xp) = (on("fastuse"), on("fastxp"));
        out.push(format!("fastuse.on={}", fast || xp));
        if let Some(key) = ["fastuse", "fastxp"].iter().filter_map(|id| get(id, "key")).find(|k| *k != "none") {
            out.push(format!("fastuse.key={key}"));
        }
        if xp && !fast {
            out.push("fastuse.mode=Some".into());
            out.push("fastuse.listmode=Whitelist".into());
            out.push("fastuse.items=".into());
            out.push("fastuse.blocks=false".into());
        }
        out.push(format!("fastuse.throwables={xp}"));
        // FastXP's Delay 0 threw every 2 ticks; Throw delay counts the ticks from one throw to the next.
        if let Some(delay) = get("fastxp", "delay").and_then(|d| d.parse::<f64>().ok()) {
            out.push(format!("fastuse.throwdelay={}", delay + 2.0));
        }
        for field in ["experiencebottles", "enderpearls", "windcharges", "snowballsandeggs", "splashpotions", "offhand"] {
            if let Some(v) = get("fastxp", field) {
                out.push(format!("fastuse.{field}={v}"));
            }
        }
    }
    // Crystal Aura and Anchor Aura took in Crystal Macro and Anchor Macro as their Macro sections, Step took in Reverse
    // Step, and Anvil took in Anti Anvil, Self Anvil and Auto Anvil as sections; Anchor (Movement) is gone, its lines
    // with it. `id` took in `parts` (each old module with its section; Crystal Aura is its own Aura): it is on if any
    // was, with only those sections on. With none on, the sections keep their defaults, unless the one key bound was
    // another module's: that module's section alone then. The first key bound goes over; each `shared` setting is the
    // first module's that was on (else the first with one), and `fields` (old module, old setting, new setting) are
    // copied.
    {
        let mut merge = |id: &str, parts: &[(&str, &str)], shared: &[&str], fields: &[(&str, &str, &str)]| {
            if !parts.iter().any(|(old, _)| *old != id && had(old)) {
                return;
            }
            let any_on = parts.iter().any(|(old, _)| on(old));
            out.push(format!("{id}.on={any_on}"));
            let keyed: Vec<&str> =
                parts.iter().map(|(old, _)| *old).filter(|old| get(old, "key").is_some_and(|k| k != "none")).collect();
            if let Some(key) = keyed.first().and_then(|old| get(old, "key")) {
                out.push(format!("{id}.key={key}"));
            }
            let sections: Option<Vec<&str>> = if any_on {
                Some(parts.iter().map(|(old, _)| *old).filter(|old| on(old)).collect())
            } else if let [only] = keyed.as_slice()
                && *only != id
            {
                Some(vec![*only])
            } else {
                None
            };
            if let Some(sections) = sections {
                for (old, section) in parts {
                    out.push(format!("{id}.{section}={}", sections.contains(old)));
                }
            }
            for field in shared {
                let from = parts.iter().map(|(old, _)| *old).find(|old| on(old) && get(old, field).is_some())
                    .or_else(|| parts.iter().map(|(old, _)| *old).find(|old| get(old, field).is_some()));
                if let Some(v) = from.and_then(|old| get(old, field)) {
                    out.push(format!("{id}.{field}={v}"));
                }
            }
            for (old, old_field, field) in fields {
                if let Some(v) = get(old, old_field) {
                    out.push(format!("{id}.{field}={v}"));
                }
            }
        };
        merge(
            "crystalaura",
            &[("crystalaura", "aura"), ("crystalmacro", "macro")],
            &["placedelay", "breakdelay", "maxselfdamage", "autoobsidian", "switchback", "humanpace", "protectloot"],
            &[("crystalmacro", "obsidiandelay", "obsidiandelay")],
        );
        merge(
            "anchoraura",
            &[("anchoraura", "aura"), ("anchormacro", "macro")],
            &["protectloot"],
            &[
                ("anchormacro", "mode", "mode"), ("anchormacro", "delay", "delay"), ("anchormacro", "safeanchor", "safeanchor"),
                ("anchormacro", "ifunguarded", "ifunguarded"), ("anchormacro", "switchback", "switchback"),
            ],
        );
        merge(
            "rotations",
            &[("rotation", "lock"), ("norotate", "norotate")],
            &[],
            &[
                ("rotation", "yawlock", "yawlock"), ("rotation", "yawangle", "yawangle"),
                ("rotation", "pitchlock", "pitchlock"), ("rotation", "pitchangle", "pitchangle"),
            ],
        );
        // Step took in Reverse Step as its Reverse step section (0.9).
        merge("step", &[("step", "stepup"), ("reversestep", "reversestep")], &[], &[("reversestep", "speed", "fallspeed")]);
        merge(
            "anvil",
            &[("antianvil", "antianvil"), ("selfanvil", "selfanvil"), ("autoanvil", "autoanvil")],
            &["rotate"],
            &[
                ("autoanvil", "targetrange", "targetrange"), ("autoanvil", "height", "height"), ("autoanvil", "delay", "delay"),
                ("autoanvil", "placeatfeet", "placeatfeet"), ("autoanvil", "multiplace", "multiplace"),
            ],
        );
    }
    if !old_offhand && !gone[..3].iter().any(|id| had(id)) {
        return std::borrow::Cow::Owned(out.join("\n"));
    }
    let mut set = |field: &str, value: Option<&str>| {
        if let Some(v) = value {
            out.push(format!("offhand.{field}={v}"));
        }
    };
    let (offhand, totem, inv, hover) = (on("offhand"), on("autototem"), on("autoinventorytotem"), on("hovertotem"));
    set("on", Some(if offhand || totem || inv || hover { "true" } else { "false" }));
    set("key", ["offhand", "autototem", "autoinventorytotem", "hovertotem"].iter().filter_map(|id| get(id, "key")).find(|k| *k != "none"));
    let item = if offhand {
        get("offhand", "item")
    } else if totem && get("autototem", "mode") == Some("Strict") {
        Some("Totem")
    } else if totem || inv || hover {
        Some("None")
    } else {
        None
    };
    set("item", item);
    let (when, delay) = if offhand || !had("autototem") {
        ("offhand", get("offhand", "itemswitchdelay"))
    } else {
        ("autototem", get("autototem", "delayticks"))
    };
    set("health", get(when, if when == "offhand" { "minhealth" } else { "health" }));
    set("fall", get(when, if when == "offhand" { "falling" } else { "fall" }));
    set("elytra", get(when, "elytra"));
    set("explosion", get(when, "explosion"));
    set("delayticks", delay);
    for field in ["hotbar", "rightgapple", "swordgapple", "alwaysgaponsword", "alwayspotonsword", "swordpot"] {
        set(field, get("offhand", field));
    }
    set("refill", Some(if inv { get("autoinventorytotem", "mode").unwrap_or("Inventory") } else { "Off" }));
    let slots = if hover && !inv { "hovertotem" } else { "autoinventorytotem" };
    set("hotbarrefill", get(slots, "hotbarrefill"));
    set("hotbarslot", get(slots, "hotbarslot"));
    for field in ["reactiondelay", "swapdelay", "closedelay", "switchbackdelay", "doublehand"] {
        set(field, get("autoinventorytotem", field));
    }
    set("hover", Some(if hover { "true" } else { "false" }));
    std::borrow::Cow::Owned(out.join("\n"))
}
