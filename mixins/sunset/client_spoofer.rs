// Client Spoofer (Client): the client brand (minecraft:brand) Horizon sends once per connection, as configuration
// starts, names another client instead, and with Protocol packets on, the packets that client sends once in the world
// go out too where their format is known from the client's server API (Lunar Client, LabyMod). Per server can name a
// profile for a server, its subdomains or one port. The settings are read as a connection starts (a join, a reconnect,
// a transfer), so a change applies on the next one; single player keeps Horizon's own. Diagnostics says what went out
// and what a server can still tell apart.

use super::Game;
use super::sunset::{Category, Module};
use horizon_client::net::sunset_client_brand::{self as wire, Plan};
use horizon_protocol::buf::Writer;
use horizon_protocol::packets::configuration::CustomPayload;
use horizon_protocol::packets::play::Serverbound;

pub(super) const NAME: &str = "Client Spoofer";
const PROFILES: &[&str] =
    &["Horizon", "Vanilla", "Fabric", "Lunar Client", "Badlion", "LabyMod", "Feather", "Forge", "NeoForge", "Custom"];
// Lunar Client's own example of its build string (v<major>.<minor>.<patch>-<year><week>).
const LUNAR_VERSION: &str = "v2.12.0-2349";
const MAX_BRAND: usize = 128;
// Apollo's channels: servers detect Lunar Client by lunar:apollo being registered, and Apollo's JSON servers send on
// apollo:json the moment it is, which Bukkit only delivers to a client already listening on apollo:json.
const LUNAR_CHANNELS: &[&str] = &["apollo:json", "lunar:apollo"];
// LabyMod 4's payload registry, as much of it as its code shows: Minecraft's register and unregister, LabyMod's
// protocol and LabyMod 3's, which it still answers.
const LABYMOD_CHANNELS: &[&str] = &["minecraft:register", "minecraft:unregister", "labymod:neo", "labymod3:main"];

pub(super) fn defaults() -> Vec<Module> {
    vec![
        Module::new(NAME, Category::Client, "Names another client to servers. Applies on the next connection.")
            .pick("Client", PROFILES)
            .bool("Protocol packets", true)
            .text("Custom brand", "vanilla")
            .text("Lunar version", LUNAR_VERSION)
            .bool("Lunar Fabric tag", true)
            .text("LabyMod version", "")
            .text("Per server", "")
            .bool("Diagnostics", true),
    ]
}

// A server being joined: its plan from the settings as they are now (none with the module off or on Horizon).
pub(crate) fn connecting(address: &str) {
    wire::next(super::sunset::with(|s| s.module(NAME).filter(|m| m.on).and_then(|m| plan(m, address))));
}

// After each play Login, the plan's join packets; once in the world, Diagnostics.
pub(super) fn tick(game: &mut Game<'_>, module: Option<&Module>) {
    if let Some(plan) = wire::joins()
        && game.remote.is_some()
    {
        for (channel, data) in &plan.joins {
            game.send(Serverbound::CustomPayload(CustomPayload { channel: (*channel).into(), data: data.clone() }));
        }
    }
    if game.player.is_none() {
        return;
    }
    let Some(plan) = wire::sent() else {
        return;
    };
    if !module.is_some_and(|m| m.b("Diagnostics")) {
        return;
    }
    let after = match plan.joins.as_slice() {
        [] => "brand only".to_owned(),
        joins => format!("then {}", joins.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(", ")),
    };
    super::sunset::say(game, &format!("Client Spoofer: sent \"{}\" ({}, {after}) to {}.", plan.brand, plan.profile, plan.host));
    let mut gaps = vec!["the known packs answer stays empty (vanilla names minecraft:core)"];
    if let Some(gap) = unsent(plan.profile) {
        gaps.push(gap);
    }
    if game.remote.as_ref().is_some_and(|r| r.brand.to_ascii_lowercase().contains("horizon")) {
        gaps.push("Horizon's server checks Horizon's content identity, so it can tell anyway");
    }
    gaps.extend(plan.notes.iter().copied());
    super::sunset::say(game, &format!("Not covered: {}.", gaps.join("; ")));
}

// The settings a profile reads.
struct Choice<'a> {
    custom: &'a str,
    lunar: &'a str,
    lunar_fabric: bool,
    labymod: &'a str,
    packets: bool,
}

fn plan(m: &Module, address: &str) -> Option<Plan> {
    let (host, port) = split(address);
    let (listed, unread) = per_server(m.t("Per server"), &host, port);
    let profile = listed.unwrap_or_else(|| m.p("Client"));
    let mut notes = Vec::new();
    if unread {
        notes.push("a Per server entry could not be read (host[:port]=Client, comma separated)");
    }
    let choice = Choice {
        custom: m.t("Custom brand"),
        lunar: m.t("Lunar version"),
        lunar_fabric: m.b("Lunar Fabric tag"),
        labymod: m.t("LabyMod version"),
        packets: m.b("Protocol packets"),
    };
    let brand = brand(profile, &choice, &mut notes)?;
    let joins = if choice.packets { joins(profile, &choice, &mut notes) } else { Vec::new() };
    Some(Plan { brand, profile, host, notes, joins })
}

// The brand each client sends: Fabric Loader's branding ("fabric", and ",fabric" after another client's own), Forge's
// and NeoForge's BrandingControl, LabyMod 4's, Lunar Client's published lunarclient:<version> format; Feather's and
// Badlion's as server-side client detectors match them.
fn brand(profile: &str, c: &Choice<'_>, notes: &mut Vec<&'static str>) -> Option<String> {
    Some(match profile {
        "Vanilla" => "vanilla".into(),
        "Fabric" => "fabric".into(),
        "Lunar Client" => {
            let version = c.lunar.trim();
            let version = if lunar_version(version) {
                version
            } else {
                notes.push("Lunar version unusable (letters, digits and . - _ + only), its example sent");
                LUNAR_VERSION
            };
            let mut brand = format!("lunarclient:{version}");
            if c.lunar_fabric {
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
            let custom = c.custom.trim();
            if usable(custom) {
                custom.into()
            } else {
                notes.push("Custom brand unusable (empty, over 128 characters, or control or invisible characters), vanilla sent");
                "vanilla".into()
            }
        }
        _ => return None,
    })
}

// The packets a profile's client sends after the server's Login packet, as its server API reads them:
// - Lunar Client registers Apollo's channels, in Fabric API's register format (Lunar runs on Fabric).
// - LabyMod 4 registers its payload channels (a zero byte after each), then sends VersionLoginPacket on labymod:neo
//   (packet id 0, its version) and, through its LabyMod 3 translation, INFO {"version": ...} on labymod3:main. Its
//   version is the user's to give; with none set, LabyMod is brand only.
fn joins(profile: &str, c: &Choice<'_>, notes: &mut Vec<&'static str>) -> Vec<(&'static str, Vec<u8>)> {
    match profile {
        "Lunar Client" => vec![("minecraft:register", register(LUNAR_CHANNELS, false))],
        "LabyMod" => {
            let version = c.labymod.trim();
            if !labymod_version(version) {
                notes.push(if version.is_empty() {
                    "LabyMod version not set, so brand only"
                } else {
                    "LabyMod version unusable (like 4.2.60), so brand only"
                });
                return Vec::new();
            }
            let mut login = Writer::new();
            login.varint(0);
            login.string(version);
            let mut info = Writer::new();
            info.string("INFO");
            info.string(&format!("{{\"version\":\"{version}\"}}"));
            vec![
                ("minecraft:register", register(LABYMOD_CHANNELS, true)),
                ("labymod:neo", login.data),
                ("labymod3:main", info.data),
            ]
        }
        _ => Vec::new(),
    }
}

// A minecraft:register payload: the channel names with a zero byte after each, or only between them.
fn register(channels: &[&str], after_each: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, channel) in channels.iter().enumerate() {
        if i > 0 && !after_each {
            out.push(0);
        }
        out.extend_from_slice(channel.as_bytes());
        if after_each {
            out.push(0);
        }
    }
    out
}

// What the profile's own client sends that Horizon does not.
fn unsent(profile: &str) -> Option<&'static str> {
    Some(match profile {
        "Fabric" => "as Fabric Loader without Fabric API (no play minecraft:register of Fabric API's channels)",
        "Lunar Client" => "no Apollo handshake (it lists Lunar's own mods and their settings) and only Apollo's channels registered",
        "LabyMod" => "no Intave client config, and only the channels of LabyMod's own code registered",
        "Feather" => "no feather:client handshake or hello (the hello lists Feather's mods)",
        "Forge" => "no FORGE handshake marker or Forge mod handshake",
        "NeoForge" => "no NeoForge channel negotiation (its configuration and play channel lists)",
        "Badlion" => "Badlion's brand is from client detectors, not its own code",
        _ => return None,
    })
}

fn usable(brand: &str) -> bool {
    !brand.is_empty() && brand.chars().count() <= MAX_BRAND && !brand.chars().any(|c| c.is_control() || invisible(c))
}

// Format characters and line separators: invisible in a brand, and what makes two brands look alike.
fn invisible(c: char) -> bool {
    matches!(c, '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}'
        | '\u{2060}'..='\u{206f}' | '\u{feff}' | '\u{fff9}'..='\u{fffb}')
}

fn lunar_version(v: &str) -> bool {
    !v.is_empty() && v.len() <= 32 && v.chars().all(|c| c.is_ascii_alphanumeric() || ".-_+".contains(c))
}

// LabyMod's SemanticVersion: major.minor[.patch][-extension].
fn labymod_version(v: &str) -> bool {
    let (numbers, extension) = v.split_once('-').map_or((v, None), |(n, e)| (n, Some(e)));
    let parts = numbers.split('.').collect::<Vec<_>>();
    v.len() <= 32
        && (2..=3).contains(&parts.len())
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && extension.is_none_or(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'))
}

// An address's host (lower case, without a trailing dot) and port: the default 25565 when it names none.
fn split(address: &str) -> (String, u16) {
    let (host, port) = parts(address);
    (host, port.and_then(|p| p.parse().ok()).unwrap_or(25565))
}

fn parts(address: &str) -> (String, Option<&str>) {
    let a = address.trim();
    let (host, port) = match a.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((host, tail)) => (host, tail.strip_prefix(':')),
            None => (rest, None),
        },
        None => match a.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => (host, Some(port)),
            _ => (a, None),
        },
    };
    (host.trim_end_matches('.').to_ascii_lowercase(), port)
}

// Per server: host[:port]=Client entries, comma separated. A host covers its subdomains, an entry with a port only that
// port; the longest host wins, then the entry with a port. The client may be named by a start only it has (lunar).
// Also whether an entry could not be read.
fn per_server(list: &str, host: &str, port: u16) -> (Option<&'static str>, bool) {
    let norm = |s: &str| s.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase();
    let mut best: Option<((usize, bool), &'static str)> = None;
    let mut unread = false;
    for entry in list.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((at, name)) = entry.split_once('=') else {
            unread = true;
            continue;
        };
        let name = norm(name);
        let starts = PROFILES.iter().copied().filter(|p| !name.is_empty() && norm(p).starts_with(&name)).collect::<Vec<_>>();
        let profile = PROFILES.iter().copied().find(|p| norm(p) == name).or(match starts.as_slice() {
            [only] => Some(*only),
            _ => None,
        });
        let (at, at_port) = parts(at);
        let at_port = match at_port.map(str::parse::<u16>) {
            None => None,
            Some(Ok(p)) => Some(p),
            Some(Err(_)) => {
                unread = true;
                continue;
            }
        };
        let (Some(profile), false) = (profile, at.is_empty()) else {
            unread = true;
            continue;
        };
        let hit = (host == at || host.strip_suffix(at.as_str()).is_some_and(|rest| rest.ends_with('.')))
            && at_port.is_none_or(|p| p == port);
        let rank = (at.len(), at_port.is_some());
        if hit && best.is_none_or(|(r, _)| rank > r) {
            best = Some((rank, profile));
        }
    }
    (best.map(|(_, profile)| profile), unread)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(labymod: &str) -> Choice<'_> {
        Choice { custom: "", lunar: LUNAR_VERSION, lunar_fabric: true, labymod, packets: true }
    }

    fn sent(profile: &str) -> Option<String> {
        brand(profile, &choice(""), &mut Vec::new())
    }

    #[test]
    fn each_profile_sends_its_clients_brand() {
        assert_eq!(sent("Horizon"), None);
        assert_eq!(sent("Vanilla").as_deref(), Some("vanilla"));
        assert_eq!(sent("Fabric").as_deref(), Some("fabric"));
        assert_eq!(sent("Lunar Client").as_deref(), Some("lunarclient:v2.12.0-2349,fabric"));
        let lunar = Choice { lunar: "v2.20.1-2544", lunar_fabric: false, ..choice("") };
        assert_eq!(brand("Lunar Client", &lunar, &mut Vec::new()).as_deref(), Some("lunarclient:v2.20.1-2544"));
        assert_eq!(sent("LabyMod").as_deref(), Some("labymod"));
        assert_eq!(sent("Feather").as_deref(), Some("Feather Fabric"));
        assert_eq!(sent("Forge").as_deref(), Some("forge"));
        assert_eq!(sent("NeoForge").as_deref(), Some("neoforge"));
        let mut notes = Vec::new();
        let custom = Choice { custom: "  My Client ", ..choice("") };
        assert_eq!(brand("Custom", &custom, &mut notes).as_deref(), Some("My Client"));
        assert!(notes.is_empty());
    }

    #[test]
    fn unusable_custom_brands_and_versions_fall_back_and_say_so() {
        for bad in ["", "   ", "a\nb", "a\u{200b}b", "a\u{202e}b", &"x".repeat(129)] {
            let mut notes = Vec::new();
            let custom = Choice { custom: bad, ..choice("") };
            assert_eq!(brand("Custom", &custom, &mut notes).as_deref(), Some("vanilla"), "{bad:?}");
            assert_eq!(notes.len(), 1);
        }
        let wide = "é".repeat(128);
        assert_eq!(brand("Custom", &Choice { custom: &wide, ..choice("") }, &mut Vec::new()), Some(wide.clone()));
        let mut notes = Vec::new();
        let lunar = Choice { lunar: "v2 beta", lunar_fabric: false, ..choice("") };
        assert_eq!(brand("Lunar Client", &lunar, &mut notes).as_deref(), Some("lunarclient:v2.12.0-2349"));
        assert_eq!(notes.len(), 1);
        for (v, ok) in [("4.2.60", true), ("4.3", true), ("4.3.27-beta.1", true), ("", false), ("4", false), ("4.x.1", false), ("4.2.", false), ("4.2-", false)] {
            assert_eq!(labymod_version(v), ok, "{v}");
        }
    }

    #[test]
    fn join_packets_follow_the_clients_formats() {
        let mut notes = Vec::new();
        assert_eq!(joins("Lunar Client", &choice(""), &mut notes), vec![("minecraft:register", b"apollo:json\0lunar:apollo".to_vec())]);
        let laby = joins("LabyMod", &choice("4.2.60"), &mut notes);
        assert_eq!(laby[0], ("minecraft:register", b"minecraft:register\0minecraft:unregister\0labymod:neo\0labymod3:main\0".to_vec()));
        assert_eq!(laby[1], ("labymod:neo", b"\x00\x064.2.60".to_vec()));
        assert_eq!(laby[2], ("labymod3:main", b"\x04INFO\x14{\"version\":\"4.2.60\"}".to_vec()));
        assert!(notes.is_empty());
        assert!(joins("LabyMod", &choice(""), &mut notes).is_empty());
        assert_eq!(notes, ["LabyMod version not set, so brand only"]);
        for profile in ["Vanilla", "Fabric", "Badlion", "Feather", "Forge", "NeoForge", "Custom"] {
            assert!(joins(profile, &choice("4.2.60"), &mut notes).is_empty(), "{profile}");
        }
    }

    #[test]
    fn per_server_matches_host_subdomains_and_ports() {
        let list = "hypixel.net=Lunar, mc.hypixel.net=Vanilla, 127.0.0.1=neoforge, 127.0.0.1:25599=Fabric, bad entry, x.org=Nope, y.org=f, z.org:http=Vanilla";
        let at = |address: &str| {
            let (host, port) = split(address);
            per_server(list, &host, port)
        };
        assert_eq!(at("hypixel.net:25565"), (Some("Lunar Client"), true));
        assert_eq!(at("EU.Hypixel.net").0, Some("Lunar Client"));
        assert_eq!(at("mc.hypixel.net").0, Some("Vanilla"));
        assert_eq!(at("nothypixel.net").0, None);
        assert_eq!(at("127.0.0.1:41234").0, Some("NeoForge"));
        assert_eq!(at("127.0.0.1:25599").0, Some("Fabric"));
        assert_eq!(at("y.org").0, None, "f could be Fabric, Feather or Forge");
        assert_eq!(per_server("example.net:25565=Forge", "example.net", split("example.net").1).0, Some("Forge"));
        assert_eq!(per_server("", "example.net", 25565), (None, false));
        assert_eq!(split("[::1]:25566"), ("::1".into(), 25566));
        assert_eq!(split("::1"), ("::1".into(), 25565));
        assert_eq!(split("Example.NET."), ("example.net".into(), 25565));
    }
}
