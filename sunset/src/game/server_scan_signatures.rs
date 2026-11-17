// Conservative signatures: a command can be hidden, aliased, or registered by another plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    ServerSoftware,
    Proxy,
    Plugin,
    AntiCheat,
    Unknown,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::ServerSoftware => "Server software",
            Kind::Proxy => "Proxy",
            Kind::Plugin => "Plugin",
            Kind::AntiCheat => "Anti-cheat",
            Kind::Unknown => "Plugin (not in Sunset's list)",
        }
    }
}

// Declared order is best first: a finding keeps the best strength any of its sources gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Strength {
    Reported,
    Strong,
    Possible,
}

impl Strength {
    pub fn label(self) -> &'static str {
        match self {
            Strength::Reported => "Reported",
            Strength::Strong => "Strong",
            Strength::Possible => "Possible",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub name: &'static str,
    pub kind: Kind,
    pub strength: Strength,
}

const fn hit(name: &'static str, kind: Kind, strength: Strength) -> Match {
    Match { name, kind, strength }
}

// Bukkit registers every plugin command a second time as `<plugin name in lower case>:<command>`
// (spigot.yml `commands.send-namespaced`, on by default), so a namespace names the plugin itself.
const NAMESPACES: &[(&str, &str, Kind)] = &[
    ("essentials", "EssentialsX", Kind::Plugin),
    ("luckperms", "LuckPerms", Kind::Plugin),
    ("worldedit", "WorldEdit", Kind::Plugin),
    ("fastasyncworldedit", "FastAsyncWorldEdit", Kind::Plugin),
    ("worldguard", "WorldGuard", Kind::Plugin),
    ("coreprotect", "CoreProtect", Kind::Plugin),
    ("vault", "Vault", Kind::Plugin),
    ("viaversion", "ViaVersion", Kind::Plugin),
    ("protocollib", "ProtocolLib", Kind::Plugin),
    ("placeholderapi", "PlaceholderAPI", Kind::Plugin),
    ("grimac", "GrimAC", Kind::AntiCheat),
    ("vulcan", "Vulcan", Kind::AntiCheat),
    ("nocheatplus", "NoCheatPlus", Kind::AntiCheat),
    ("spartan", "Spartan", Kind::AntiCheat),
    // The server's own commands. Paper and its forks keep Bukkit's and Spigot's as well.
    ("bukkit", "Bukkit-based", Kind::ServerSoftware),
    ("spigot", "Spigot-based", Kind::ServerSoftware),
    ("paper", "Paper-based", Kind::ServerSoftware),
    ("purpur", "Purpur", Kind::ServerSoftware),
];

// Bare command names documented by each project. Another plugin can register the same name,
// so these are only Possible.
const COMMANDS: &[(&str, &str, Kind)] = &[
    ("essentials", "EssentialsX", Kind::Plugin),
    ("ess", "EssentialsX", Kind::Plugin),
    ("luckperms", "LuckPerms", Kind::Plugin),
    ("lp", "LuckPerms", Kind::Plugin),
    ("worldedit", "WorldEdit", Kind::Plugin),
    ("we", "WorldEdit", Kind::Plugin),
    ("worldguard", "WorldGuard", Kind::Plugin),
    ("region", "WorldGuard", Kind::Plugin),
    ("rg", "WorldGuard", Kind::Plugin),
    ("coreprotect", "CoreProtect", Kind::Plugin),
    ("co", "CoreProtect", Kind::Plugin),
    ("grim", "GrimAC", Kind::AntiCheat),
    ("grimac", "GrimAC", Kind::AntiCheat),
    ("vulcan", "Vulcan", Kind::AntiCheat),
    ("nocheatplus", "NoCheatPlus", Kind::AntiCheat),
    ("ncp", "NoCheatPlus", Kind::AntiCheat),
    ("spartan", "Spartan", Kind::AntiCheat),
    // Proxies add their own commands to the tree they forward.
    ("velocity", "Velocity", Kind::Proxy),
    ("bungee", "BungeeCord", Kind::Proxy),
];

// Names a server lists itself: Query's plugin list and /version's suggestions.
const LISTED: &[(&str, &str, Kind)] = &[
    ("essentials", "EssentialsX", Kind::Plugin),
    ("essentialsx", "EssentialsX", Kind::Plugin),
    ("luckperms", "LuckPerms", Kind::Plugin),
    ("worldedit", "WorldEdit", Kind::Plugin),
    ("fastasyncworldedit", "FastAsyncWorldEdit", Kind::Plugin),
    ("worldguard", "WorldGuard", Kind::Plugin),
    ("coreprotect", "CoreProtect", Kind::Plugin),
    ("vault", "Vault", Kind::Plugin),
    ("viaversion", "ViaVersion", Kind::Plugin),
    ("protocollib", "ProtocolLib", Kind::Plugin),
    ("placeholderapi", "PlaceholderAPI", Kind::Plugin),
    ("grim", "GrimAC", Kind::AntiCheat),
    ("grimac", "GrimAC", Kind::AntiCheat),
    ("matrix", "Matrix", Kind::AntiCheat),
    ("vulcan", "Vulcan", Kind::AntiCheat),
    ("spartan", "Spartan", Kind::AntiCheat),
    ("intave", "Intave", Kind::AntiCheat),
    ("nocheatplus", "NoCheatPlus", Kind::AntiCheat),
];

fn family(text: &str) -> Option<Match> {
    let family = text.trim().split([' ', '(', '/', '-']).next()?.to_ascii_lowercase();
    let (name, kind) = match family.as_str() {
        "vanilla" => ("Vanilla", Kind::ServerSoftware),
        "craftbukkit" => ("CraftBukkit", Kind::ServerSoftware),
        "spigot" => ("Spigot", Kind::ServerSoftware),
        "paper" => ("Paper", Kind::ServerSoftware),
        "folia" => ("Folia", Kind::ServerSoftware),
        "purpur" => ("Purpur", Kind::ServerSoftware),
        "pufferfish" => ("Pufferfish", Kind::ServerSoftware),
        "fabric" => ("Fabric", Kind::ServerSoftware),
        "quilt" => ("Quilt", Kind::ServerSoftware),
        "forge" => ("Forge", Kind::ServerSoftware),
        "neoforge" => ("NeoForge", Kind::ServerSoftware),
        "horizon" => ("Horizon", Kind::ServerSoftware),
        "velocity" => ("Velocity", Kind::Proxy),
        "bungeecord" => ("BungeeCord", Kind::Proxy),
        "waterfall" => ("Waterfall", Kind::Proxy),
        _ => return None,
    };
    Some(hit(name, kind, Strength::Reported))
}

// The brand the server declares: what it says, not proof. Proxies rewrite it:
// BungeeCord and Waterfall send "BungeeCord (git:...) <- Paper", Velocity sends "Paper (Velocity)".
pub(crate) fn brand(raw: &str) -> Vec<Match> {
    let (proxy, backend) = match raw.split_once(" <- ") {
        Some((proxy, backend)) => (Some(proxy), backend),
        None => (None, raw),
    };
    let mut out = Vec::new();
    let suffix = backend.trim().strip_suffix(')').and_then(|b| b.rsplit_once('(')).map(|(_, inner)| inner);
    for m in [proxy.and_then(family), suffix.and_then(family).filter(|m| m.kind == Kind::Proxy), family(backend)]
        .into_iter()
        .flatten()
    {
        if !out.iter().any(|o: &Match| o.name == m.name) {
            out.push(m);
        }
    }
    out
}

fn lookup(table: &[(&str, &'static str, Kind)], key: &str, strength: Strength) -> Option<Match> {
    table.iter().find(|(k, ..)| *k == key).map(|&(_, name, kind)| hit(name, kind, strength))
}

// A plugin namespace on a command root, other than the vanilla one.
pub(crate) fn namespace(root: &str) -> Option<&str> {
    let (ns, command) = root.split_once(':')?;
    let valid = !ns.is_empty()
        && !command.is_empty()
        && ns != "minecraft"
        && ns.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-.".contains(&b));
    valid.then_some(ns)
}

pub(crate) fn command(root: &str) -> Option<Match> {
    let root = root.to_ascii_lowercase();
    match namespace(&root) {
        Some(ns) => lookup(NAMESPACES, ns, Strength::Strong),
        None => lookup(COMMANDS, &root, Strength::Possible),
    }
}

// Channels a single project owns. Generic ones (bungeecord:main, minecraft:register) prove nothing.
pub(crate) fn channel(id: &str) -> Option<Match> {
    let (ns, path) = id.split_once(':')?;
    let (name, kind) = match ns {
        "worldedit" if path == "cui" => ("WorldEdit", Kind::Plugin),
        "fabric" => ("Fabric", Kind::ServerSoftware),
        "forge" | "fml" => ("Forge", Kind::ServerSoftware),
        "neoforge" => ("NeoForge", Kind::ServerSoftware),
        _ => return None,
    };
    Some(hit(name, kind, Strength::Strong))
}

// The plugin name of a listed entry such as "LuckPerms 5.4.102" or "GrimAC (2.3)".
pub(crate) fn listed_name(raw: &str) -> &str {
    raw.trim().split([' ', '(']).next().unwrap_or("")
}

pub(crate) fn query_plugin(raw: &str) -> Match {
    lookup(LISTED, &listed_name(raw).to_ascii_lowercase(), Strength::Reported)
        .unwrap_or(hit("Unknown", Kind::Unknown, Strength::Reported))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_brand_is_reported_and_not_authenticated() {
        assert_eq!(brand("Paper"), [hit("Paper", Kind::ServerSoftware, Strength::Reported)]);
        assert!(brand("mystery proxy").is_empty());
    }

    #[test]
    fn proxy_brands_name_both_the_proxy_and_the_backend() {
        let names = |raw| brand(raw).iter().map(|m| m.name).collect::<Vec<_>>();
        assert_eq!(names("BungeeCord (git:BungeeCord-Bootstrap:1.21-R0.1) <- Paper"), ["BungeeCord", "Paper"]);
        assert_eq!(names("Paper (Velocity)"), ["Velocity", "Paper"]);
        assert_eq!(brand("Paper (Velocity)")[0].kind, Kind::Proxy);
        assert_eq!(names("Purpur"), ["Purpur"]);
    }

    #[test]
    fn command_namespaces_are_specific_but_ambiguous_aliases_are_not() {
        assert_eq!(command("luckperms:lp").unwrap().name, "LuckPerms");
        assert_eq!(command("luckperms:lp").unwrap().strength, Strength::Strong);
        assert_eq!(command("essentials:home").unwrap().name, "EssentialsX");
        assert_eq!(command("grimac:grim").unwrap().kind, Kind::AntiCheat);
        assert_eq!(command("lp").unwrap().strength, Strength::Possible);
        assert_eq!(command("co").unwrap().strength, Strength::Possible);
        assert_eq!(command("rg").unwrap().name, "WorldGuard");
        assert_eq!(command("ncp").unwrap().strength, Strength::Possible);
        assert_eq!(command("plugins"), None);
        assert_eq!(command("minecraft:tp"), None);
        assert_eq!(namespace("minecraft:tp"), None);
        assert_eq!(namespace("mysteryplugin:open"), Some("mysteryplugin"));
        assert_eq!(namespace("bad:"), None);
    }

    #[test]
    fn only_project_owned_channels_are_evidence() {
        assert_eq!(channel("worldedit:cui").unwrap().name, "WorldEdit");
        assert_eq!(channel("fabric:registry/sync").unwrap().name, "Fabric");
        assert_eq!(channel("bungeecord:main"), None);
        assert_eq!(channel("minecraft:register"), None);
    }

    #[test]
    fn exact_query_names_can_report_anticheats_but_unknown_stays_unknown() {
        assert_eq!(query_plugin("GrimAC").kind, Kind::AntiCheat);
        assert_eq!(query_plugin("NoCheatPlus 3.17").kind, Kind::AntiCheat);
        assert_eq!(query_plugin("EssentialsX 2.20.1").name, "EssentialsX");
        assert_eq!(query_plugin("MysteryGuard").kind, Kind::Unknown);
        assert_eq!(query_plugin("MysteryGuard").strength, Strength::Reported);
        assert_eq!(listed_name("MysteryGuard 1.0"), "MysteryGuard");
    }
}
