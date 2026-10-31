// Bounded, independently implemented server watchlists. No network or game dependencies.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_BYTES: usize = 128 * 1024;
pub const MAX_PROFILES: usize = 32;
pub const MAX_ALIASES: usize = 8;
pub const MAX_PLAYERS: usize = 128;
pub const MAX_EDITOR_BYTES: usize = 4096;
const HEADER: &str = "sunset-watchlists=1";
const FILE_NAME: &str = "watchlists-v1";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub addresses: Vec<String>,
    pub players: Vec<String>,
    /// A command name only, without slash, arguments, or escapes.
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub profiles: Vec<Profile>,
}

pub fn profile_id(input: &str) -> Result<String, String> {
    if input.is_empty() || input.len() > 32 || !input.as_bytes()[0].is_ascii_alphanumeric()
        || !input.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("profile id must be 1–32 ASCII letters, digits, - or _, starting with a letter or digit".into());
    }
    let id = input.to_ascii_lowercase();
    if id == "auto" {
        return Err("auto is reserved for connected-server selection".into());
    }
    Ok(id)
}

fn port(input: &str) -> Result<u16, String> {
    if input.is_empty() || !input.bytes().all(|b| b.is_ascii_digit()) {
        return Err("server port must be a decimal number from 1 to 65535".into());
    }
    input.parse::<u16>().ok().filter(|p| *p != 0)
        .ok_or_else(|| "server port must be a decimal number from 1 to 65535".into())
}

/// Exact host + effective port, without DNS. Bare IPv6 always means an address
/// at port 25565; a nondefault IPv6 port must use `[address]:port`.
pub fn normalize_address(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty() || value.len() > 300 || !value.is_ascii() || value.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err("server address must be an ASCII hostname or IP with an optional port".into());
    }
    if let Some(rest) = value.strip_prefix('[') {
        let close = rest.find(']').ok_or("IPv6 address is missing ]")?;
        let ip = rest[..close].parse::<Ipv6Addr>().map_err(|_| "invalid IPv6 address")?;
        let suffix = &rest[close + 1..];
        let p = if suffix.is_empty() { 25565 } else { port(suffix.strip_prefix(':').ok_or("unexpected text after IPv6 address")?)? };
        return Ok(format!("[{ip}]:{p}"));
    }
    if value.contains('[') || value.contains(']') {
        return Err("invalid address brackets".into());
    }
    if value.matches(':').count() > 1 {
        let ip = value.parse::<Ipv6Addr>().map_err(|_| "invalid bare IPv6 address; use [address]:port for a port")?;
        return Ok(format!("[{ip}]:25565"));
    }
    let (host, p) = match value.split_once(':') {
        Some((host, p)) => (host, port(p)?),
        None => (value, 25565),
    };
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(format!("{ip}:{p}"));
    }
    if host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err("invalid IPv4 address".into());
    }
    if host.is_empty() || host.len() > 253 || !host.split('.').all(|label| {
        !label.is_empty() && label.len() <= 63 && label.as_bytes()[0].is_ascii_alphanumeric()
            && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return Err("invalid server hostname".into());
    }
    Ok(format!("{host}:{p}"))
}

pub fn valid_player(input: &str) -> bool {
    !input.is_empty() && input.len() <= 16
        && input.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn player_list<'a>(players: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for player in players {
        if !valid_player(player) {
            return Err(format!("invalid Java player name {player:?}; use 1–16 ASCII letters, digits or _"));
        }
        if seen.insert(player.to_ascii_lowercase()) {
            result.push(player.to_owned());
            if result.len() > MAX_PLAYERS {
                return Err(format!("a watchlist allows at most {MAX_PLAYERS} players"));
            }
        }
    }
    Ok(result)
}

/// UI editor input: comma or ASCII whitespace separated Java names. Special
/// Bedrock name formats are deliberately unsupported until separately specified.
pub fn parse_players(input: &str) -> Result<Vec<String>, String> {
    if input.len() > MAX_EDITOR_BYTES {
        return Err(format!("watchlist editor text exceeds {MAX_EDITOR_BYTES} bytes"));
    }
    player_list(input.split(|c: char| c == ',' || c.is_ascii_whitespace()).filter(|s| !s.is_empty()))
}

fn normalized_profile(profile: &Profile) -> Result<Profile, String> {
    let id = profile_id(&profile.id)?;
    if profile.addresses.len() > MAX_ALIASES {
        return Err(format!("profile {id} allows at most {MAX_ALIASES} server aliases"));
    }
    if profile.players.len() > MAX_PLAYERS {
        return Err(format!("profile {id} allows at most {MAX_PLAYERS} players"));
    }
    if !matches!(profile.command.as_str(), "msg" | "tell" | "w") {
        return Err("completion command must be msg, tell, or w (a name only)".into());
    }
    let mut addresses = Vec::new();
    for address in &profile.addresses {
        let address = normalize_address(address)?;
        if !addresses.contains(&address) {
            addresses.push(address);
        }
    }
    Ok(Profile { id, addresses, players: player_list(profile.players.iter().map(String::as_str))?, command: profile.command.clone() })
}

impl Config {
    pub fn defaults() -> Self {
        Self { profiles: vec![Profile {
            id: "donutsmp".into(), addresses: vec!["donutsmp.net:25565".into()],
            players: Vec::new(), command: "msg".into(),
        }] }
    }

    fn normalized(&self) -> Result<Self, String> {
        if self.profiles.len() > MAX_PROFILES {
            return Err(format!("at most {MAX_PROFILES} watchlist profiles are allowed"));
        }
        let mut profiles = Vec::new();
        let mut ids = HashSet::new();
        let mut aliases = HashSet::new();
        for profile in &self.profiles {
            let profile = normalized_profile(profile)?;
            if !ids.insert(profile.id.clone()) {
                return Err(format!("duplicate profile id {}", profile.id));
            }
            for address in &profile.addresses {
                if !aliases.insert(address.clone()) {
                    return Err(format!("server alias {address} belongs to multiple profiles"));
                }
            }
            profiles.push(profile);
        }
        Ok(Self { profiles })
    }

    pub fn validate(&self) -> Result<(), String> {
        self.serialize().map(|_| ())
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id.eq_ignore_ascii_case(id))
    }

    pub fn list(&self) -> Vec<&str> {
        self.profiles.iter().map(|p| p.id.as_str()).collect()
    }

    /// Explicit ids do not depend on the current server. Auto returns None for
    /// an unknown server or while disconnected, never a guessed brand match.
    pub fn resolve(&self, selection: &str, server: Option<&str>) -> Result<Option<&Profile>, String> {
        self.validate()?;
        if selection.eq_ignore_ascii_case("auto") {
            let Some(server) = server else { return Ok(None); };
            let address = normalize_address(server)?;
            return Ok(self.profiles.iter().find(|p| p.addresses.iter().any(|a| normalize_address(a).as_deref() == Ok(address.as_str()))));
        }
        let id = profile_id(selection)?;
        self.profile(&id).map(Some).ok_or_else(|| format!("unknown watchlist profile {id}"))
    }

    /// Validate a candidate before changing self; errors leave self unchanged.
    pub fn upsert(&mut self, profile: Profile) -> Result<(), String> {
        let profile = normalized_profile(&profile)?;
        let mut next = self.clone();
        if let Some(existing) = next.profiles.iter_mut().find(|p| p.id.eq_ignore_ascii_case(&profile.id)) {
            *existing = profile;
        } else {
            next.profiles.push(profile);
        }
        let next = next.normalized()?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let id = profile_id(id)?;
        let Some(index) = self.profiles.iter().position(|p| p.id.eq_ignore_ascii_case(&id)) else {
            return Err(format!("unknown watchlist profile {id}"));
        };
        self.profiles.remove(index);
        Ok(())
    }

    pub fn serialize(&self) -> Result<String, String> {
        let normalized = self.normalized()?;
        let mut text = format!("{HEADER}\n");
        for profile in normalized.profiles {
            text.push_str(&format!("profile={}\naddresses={}\nplayers={}\ncommand={}\nend\n",
                profile.id, profile.addresses.join(","), profile.players.join(","), profile.command));
        }
        if text.len() > MAX_BYTES {
            return Err(format!("watchlist configuration exceeds {MAX_BYTES} bytes"));
        }
        Ok(text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_BYTES || text.contains('\r') || !text.ends_with('\n') {
            return Err("watchlist file must be bounded LF-terminated UTF-8 text".into());
        }
        let mut lines = text.split_terminator('\n');
        if lines.next() != Some(HEADER) {
            return Err("unsupported watchlist schema; expected sunset-watchlists=1".into());
        }
        let mut profiles = Vec::new();
        while let Some(line) = lines.next() {
            if profiles.len() == MAX_PROFILES {
                return Err(format!("at most {MAX_PROFILES} watchlist profiles are allowed"));
            }
            let id = line.strip_prefix("profile=").ok_or("expected profile field")?.to_owned();
            let addresses = parse_list(lines.next(), "addresses=")?;
            let players = parse_list(lines.next(), "players=")?;
            let command = lines.next().and_then(|s| s.strip_prefix("command=")).ok_or("expected command field")?.to_owned();
            if lines.next() != Some("end") {
                return Err("expected end after watchlist profile".into());
            }
            profiles.push(Profile { id, addresses, players, command });
        }
        let result = Self { profiles }.normalized()?;
        result.validate()?;
        Ok(result)
    }
}

fn parse_list(line: Option<&str>, field: &str) -> Result<Vec<String>, String> {
    let value = line.and_then(|s| s.strip_prefix(field)).ok_or_else(|| format!("expected {field} field"))?;
    if value.is_empty() { return Ok(Vec::new()); }
    if value.split(',').any(str::is_empty) {
        return Err(format!("empty item in {field} field"));
    }
    Ok(value.split(',').map(str::to_owned).collect())
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    { metadata.file_type().is_symlink() }
}

fn plain(path: &Path, directory: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("can't inspect {}: {e}", path.display()))?;
    if linked(&metadata) || (if directory { !metadata.is_dir() } else { !metadata.is_file() }) {
        return Err(format!("{} must be a plain {} without links", path.display(), if directory { "directory" } else { "file" }));
    }
    Ok(())
}

fn plain_parents(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => plain(ancestor, true)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(format!("can't inspect {}: {error}", ancestor.display())),
        }
    }
    Ok(())
}

/// Caller supplies only the trusted mod data directory; file names are fixed.
pub struct Store { data: PathBuf }

impl Store {
    pub fn new(data: PathBuf) -> Result<Self, String> {
        plain_parents(&data)?;
        fs::create_dir_all(&data).map_err(|e| format!("can't create watchlist data directory: {e}"))?;
        plain_parents(&data)?;
        Ok(Self { data })
    }

    pub fn load(&self) -> Result<Config, String> {
        plain_parents(&self.data)?;
        let path = self.data.join(FILE_NAME);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::defaults()),
            Err(error) => return Err(format!("can't inspect watchlists: {error}")),
            Ok(_) => plain(&path, false)?,
        }
        let mut bytes = Vec::new();
        File::open(&path).and_then(|f| f.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes))
            .map_err(|e| format!("can't read watchlists: {e}"))?;
        if bytes.len() > MAX_BYTES { return Err("watchlist file exceeds 128 KiB".into()); }
        let text = String::from_utf8(bytes).map_err(|_| "watchlist file is not UTF-8")?;
        Config::parse(&text)
    }

    pub fn save(&self, config: &Config) -> Result<(), String> {
        let text = config.serialize()?;
        // An unsuccessful load must never turn into an overwrite of the user's
        // malformed file, even if a caller retained an in-memory fallback.
        self.load().map_err(|e| format!("existing watchlists cannot be loaded; refusing to overwrite: {e}"))?;
        plain_parents(&self.data)?;
        let path = self.data.join(FILE_NAME);
        match fs::symlink_metadata(&path) {
            Ok(_) => plain(&path, false)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(format!("can't inspect watchlists: {error}")),
        }
        let temp = self.data.join(format!(".watchlists-v1-{}-{}.tmp", std::process::id(), TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)));
        // Never remove a pre-existing temporary file if create_new fails.
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)
            .map_err(|e| format!("can't create watchlist temporary file: {e}"))?;
        let result = file.write_all(text.as_bytes()).and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| replace(&temp, &path));
        if result.is_err() { let _ = fs::remove_file(&temp); }
        result.map_err(|e| format!("can't save watchlists; previous file preserved: {e}"))
    }
}

// The standard library replaces an existing destination on Windows and Unix.
// Both paths are siblings; never remove the destination before replacement.
fn replace(from: &Path, to: &Path) -> std::io::Result<()> { fs::rename(from, to) }
