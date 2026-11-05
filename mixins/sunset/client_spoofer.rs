// Client Spoofer (Client): the client brand (minecraft:brand) Horizon sends once per connection, as configuration
// starts, names another client instead: Vanilla, Fabric, Lunar Client, Badlion, LabyMod, Feather, Forge, NeoForge or
// a Custom string, and Per server can name one for a server and its subdomains. The settings are read as a connection
// starts (a join, a reconnect, a transfer), so a change applies on the next one; single player keeps Horizon's own.
// Only the brand changes; Diagnostics names what else a server can still see.

use super::Game;
use super::sunset::{Category, Module};
use horizon_client::net::sunset_client_brand::{self as wire, Plan};

pub(super) const NAME: &str = "Client Spoofer";
const PROFILES: &[&str] =
    &["Horizon", "Vanilla", "Fabric", "Lunar Client", "Badlion", "LabyMod", "Feather", "Forge", "NeoForge", "Custom"];
// Lunar Client's own example of its build string (v<major>.<minor>.<patch>-<year><week>).
const LUNAR_VERSION: &str = "v2.12.0-2349";
const MAX_BRAND: usize = 128;

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Client, "Names another client to servers. Applies on the next connection.")
            .pick("Client", PROFILES)
            .text("Custom brand", "vanilla")
            .text("Lunar version", LUNAR_VERSION)
            .bool("Lunar Fabric tag", true)
            .text("Per server", "")
            .bool("Diagnostics", true),
    ]
}

// A server being joined: its brand from the settings as they are now (none with the module off or on Horizon).
pub(crate) fn connecting(address: &str) {
    wire::next(super::sunset::with(|s| s.module(NAME).filter(|m| m.on).and_then(|m| plan(m, address))));
}

// Diagnostics: once in the world, what went out and what a server can still tell apart.
pub(super) fn tick(game: &mut Game<'_>, module: Option<&Module>) {
    if game.player.is_none() {
        return;
    }
    let Some(plan) = wire::sent() else {
        return;
    };
    if !module.is_some_and(|m| m.b("Diagnostics")) {
        return;
    }
    super::sunset::say(game, &format!("Client Spoofer: sent \"{}\" ({}) to {}.", plan.brand, plan.profile, plan.host));
    let mut gaps = vec!["the known packs answer stays empty (vanilla names minecraft:core)"];
    if let Some(gap) = unsent(plan.profile) {
        gaps.push(gap);
    }
    if game.remote.as_ref().is_some_and(|r| r.brand.to_ascii_lowercase().contains("horizon")) {
        gaps.push("Horizon's server checks Horizon's content identity, so it can tell anyway");
    }
    gaps.extend(plan.notes);
    super::sunset::say(game, &format!("Not covered: {}.", gaps.join("; ")));
}

fn plan(m: &Module, address: &str) -> Option<Plan> {
    let host = host(address);
    let (listed, unread) = per_server(m.t("Per server"), &host);
    let profile = listed.unwrap_or_else(|| m.p("Client"));
    let mut notes = Vec::new();
    if unread {
        notes.push("a Per server entry could not be read (host=Client, comma separated)");
    }
    let brand = brand(profile, m.t("Custom brand"), m.t("Lunar version"), m.b("Lunar Fabric tag"), &mut notes)?;
    Some(Plan { brand, profile, host, notes })
}

// The brand each client sends: Fabric Loader's branding ("fabric", and ",fabric" after another client's own), Forge's
// and NeoForge's BrandingControl, LabyMod 4's, Lunar Client's published lunarclient:<version> format; Feather's and
// Badlion's as server-side client detectors match them.
fn brand(profile: &str, custom: &str, lunar: &str, lunar_fabric: bool, notes: &mut Vec<&'static str>) -> Option<String> {
    Some(match profile {
        "Vanilla" => "vanilla".into(),
        "Fabric" => "fabric".into(),
        "Lunar Client" => {
            let version = lunar.trim();
            let version = if lunar_version(version) {
                version
            } else {
                notes.push("Lunar version unusable (letters, digits and . - _ + only), its example sent");
                LUNAR_VERSION
            };
            let mut brand = format!("lunarclient:{version}");
            if lunar_fabric {
                brand.push_str(",fabric");
            }
            brand
        }
        "Badlion" => "badlion".into(),
        "LabyMod" => "labymod".into(),
        "Feather" => "Feather Fabric".into(),
        "Forge" => "forge".into(),
        "NeoForge" => "neoforge".into(),
        "Custom" => {
            let custom = custom.trim();
            if usable(custom) {
                custom.into()
            } else {
                notes.push("Custom brand unusable (empty, over 128 characters or control characters), vanilla sent");
                "vanilla".into()
            }
        }
        _ => return None,
    })
}

// What the profile's own client would send besides its brand, which Horizon does not.
fn unsent(profile: &str) -> Option<&'static str> {
    Some(match profile {
        "Fabric" => "as Fabric Loader alone (no Fabric API c:version or c:register)",
        "Lunar Client" => "no lunar:apollo channel registration",
        "LabyMod" => "no LabyMod server API version packet",
        "Feather" => "no Feather mod channels",
        "Forge" => "no FORGE handshake marker or Forge mod handshake",
        "NeoForge" => "no NeoForge channel negotiation",
        "Badlion" => "Badlion's brand is from client detectors, not its own code",
        _ => return None,
    })
}

fn usable(brand: &str) -> bool {
    !brand.is_empty() && brand.chars().count() <= MAX_BRAND && !brand.chars().any(char::is_control)
}

fn lunar_version(v: &str) -> bool {
    !v.is_empty() && v.len() <= 32 && v.chars().all(|c| c.is_ascii_alphanumeric() || ".-_+".contains(c))
}

// The host of an address, lower case, without its port.
fn host(address: &str) -> String {
    let a = address.trim();
    let host = match a.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(rest),
        None if a.matches(':').count() == 1 => a.split(':').next().unwrap_or(a),
        None => a,
    };
    host.trim_end_matches('.').to_ascii_lowercase()
}

// Per server: host=Client entries, comma separated; a host covers its subdomains and the longest match wins. The
// client may be named by its start (lunar). Also whether an entry could not be read.
fn per_server(list: &str, host: &str) -> (Option<&'static str>, bool) {
    let norm = |s: &str| s.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase();
    let mut best: Option<(usize, &'static str)> = None;
    let mut unread = false;
    for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((at, name)) = entry.split_once('=') else {
            unread = true;
            continue;
        };
        let name = norm(name);
        let at = self::host(at);
        let profile = PROFILES.iter().copied().find(|p| norm(p) == name)
            .or_else(|| PROFILES.iter().copied().find(|p| !name.is_empty() && norm(p).starts_with(&name)));
        let (Some(profile), false) = (profile, at.is_empty()) else {
            unread = true;
            continue;
        };
        let hit = host == at || host.strip_suffix(at.as_str()).is_some_and(|rest| rest.ends_with('.'));
        if hit && best.is_none_or(|(len, _)| at.len() > len) {
            best = Some((at.len(), profile));
        }
    }
    (best.map(|(_, profile)| profile), unread)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(profile: &str) -> Option<String> {
        brand(profile, "", LUNAR_VERSION, true, &mut Vec::new())
    }

    #[test]
    fn each_profile_sends_its_clients_brand() {
        assert_eq!(sent("Horizon"), None);
        assert_eq!(sent("Vanilla").as_deref(), Some("vanilla"));
        assert_eq!(sent("Fabric").as_deref(), Some("fabric"));
        assert_eq!(sent("Lunar Client").as_deref(), Some("lunarclient:v2.12.0-2349,fabric"));
        assert_eq!(brand("Lunar Client", "", "v2.20.1-2544", false, &mut Vec::new()).as_deref(), Some("lunarclient:v2.20.1-2544"));
        assert_eq!(sent("LabyMod").as_deref(), Some("labymod"));
        assert_eq!(sent("Feather").as_deref(), Some("Feather Fabric"));
        assert_eq!(sent("Forge").as_deref(), Some("forge"));
        assert_eq!(sent("NeoForge").as_deref(), Some("neoforge"));
        let mut notes = Vec::new();
        assert_eq!(brand("Custom", "  My Client ", "", true, &mut notes).as_deref(), Some("My Client"));
        assert!(notes.is_empty());
    }

    #[test]
    fn unusable_custom_brands_and_lunar_versions_fall_back_and_say_so() {
        for bad in ["", "   ", "a\nb", &"x".repeat(129)] {
            let mut notes = Vec::new();
            assert_eq!(brand("Custom", bad, "", true, &mut notes).as_deref(), Some("vanilla"), "{bad:?}");
            assert_eq!(notes.len(), 1);
        }
        let mut notes = Vec::new();
        assert_eq!(brand("Lunar Client", "", "v2 beta", false, &mut notes).as_deref(), Some("lunarclient:v2.12.0-2349"));
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn per_server_matches_the_host_and_its_subdomains_longest_first() {
        let list = "hypixel.net=Lunar, mc.hypixel.net=Vanilla, 127.0.0.1=neoforge, bad entry, x.org=Nope";
        assert_eq!(per_server(list, &host("hypixel.net:25565")), (Some("Lunar Client"), true));
        assert_eq!(per_server(list, &host("EU.Hypixel.net")).0, Some("Lunar Client"));
        assert_eq!(per_server(list, &host("mc.hypixel.net")).0, Some("Vanilla"));
        assert_eq!(per_server(list, &host("nothypixel.net")).0, None);
        assert_eq!(per_server(list, &host("127.0.0.1:41234")).0, Some("NeoForge"));
        assert_eq!(per_server("", "example.net"), (None, false));
        assert_eq!(host("[::1]:25565"), "::1");
        assert_eq!(host("Example.NET."), "example.net");
    }
}
