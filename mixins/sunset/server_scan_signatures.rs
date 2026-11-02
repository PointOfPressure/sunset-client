// Conservative signatures: a command can be hidden, aliased, or registered by another plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    ServerSoftware,
    Plugin,
    AntiCheat,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Strength {
    Reported,
    Strong,
    Possible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub name: &'static str,
    pub kind: Kind,
    pub strength: Strength,
}

pub(crate) fn brand(raw: &str) -> Option<Match> {
    let family = raw
        .trim()
        .split([' ', '(', '/', '-'])
        .next()?
        .to_ascii_lowercase();
    let name = match family.as_str() {
        "vanilla" => "Vanilla",
        "paper" => "Paper",
        "spigot" => "Spigot",
        "purpur" => "Purpur",
        "fabric" => "Fabric",
        "forge" => "Forge",
        "horizon" => "Horizon",
        _ => return None,
    };
    Some(Match {
        name,
        kind: Kind::ServerSoftware,
        strength: Strength::Reported,
    })
}

pub(crate) fn command(root: &str) -> Option<Match> {
    let root = root.to_ascii_lowercase();
    // LuckPerms documents /lp and /luckperms. Grim documents /grim.
    let (name, kind, strength) = match root.as_str() {
        "luckperms:luckperms" | "luckperms:lp" => ("LuckPerms", Kind::Plugin, Strength::Strong),
        "luckperms" | "lp" => ("LuckPerms", Kind::Plugin, Strength::Possible),
        // These public command roots are documented by their respective projects.
        // Other plugins can register aliases, so an unnamespaced root is only Possible.
        "worldedit" | "we" => ("WorldEdit", Kind::Plugin, Strength::Possible),
        "region" | "rg" => ("WorldGuard", Kind::Plugin, Strength::Possible),
        "coreprotect" | "co" => ("CoreProtect", Kind::Plugin, Strength::Possible),
        "grim" => ("GrimAC", Kind::AntiCheat, Strength::Possible),
        _ => return None,
    };
    Some(Match {
        name,
        kind,
        strength,
    })
}

pub(crate) fn query_plugin(raw: &str) -> Match {
    let token = raw
        .trim()
        .split([' ', '('])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let (name, kind) = match token.as_str() {
        "essentials" | "essentialsx" => ("EssentialsX", Kind::Plugin),
        "luckperms" => ("LuckPerms", Kind::Plugin),
        "worldedit" => ("WorldEdit", Kind::Plugin),
        "worldguard" => ("WorldGuard", Kind::Plugin),
        "vault" => ("Vault", Kind::Plugin),
        "coreprotect" => ("CoreProtect", Kind::Plugin),
        "grim" | "grimac" => ("GrimAC", Kind::AntiCheat),
        "matrix" => ("Matrix", Kind::AntiCheat),
        "vulcan" => ("Vulcan", Kind::AntiCheat),
        "spartan" => ("Spartan", Kind::AntiCheat),
        "intave" => ("Intave", Kind::AntiCheat),
        "nocheatplus" => ("NoCheatPlus", Kind::AntiCheat),
        _ => ("Unknown", Kind::Unknown),
    };
    Match {
        name,
        kind,
        strength: Strength::Reported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_brand_is_reported_and_not_authenticated() {
        assert_eq!(
            brand("Paper"),
            Some(Match {
                name: "Paper",
                kind: Kind::ServerSoftware,
                strength: Strength::Reported
            })
        );
        assert_eq!(brand("mystery proxy"), None);
    }

    #[test]
    fn command_namespaces_are_specific_but_ambiguous_aliases_are_not() {
        assert_eq!(command("luckperms:lp").unwrap().name, "LuckPerms");
        assert_eq!(command("lp").unwrap().strength, Strength::Possible);
        assert_eq!(command("co").unwrap().strength, Strength::Possible);
        assert_eq!(command("rg").unwrap().name, "WorldGuard");
        assert_eq!(command("plugins"), None);
        assert_eq!(command("ncp"), None);
    }

    #[test]
    fn exact_query_names_can_report_anticheats_but_unknown_stays_unknown() {
        assert_eq!(query_plugin("GrimAC").kind, Kind::AntiCheat);
        assert_eq!(query_plugin("NoCheatPlus").kind, Kind::AntiCheat);
        assert_eq!(query_plugin("MysteryGuard").kind, Kind::Unknown);
        assert_eq!(query_plugin("MysteryGuard").strength, Strength::Reported);
    }
}
