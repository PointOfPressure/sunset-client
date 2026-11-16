// kaptainwutax.seedcrackerX.util.Database, the part that needs no network: the sheet of server seeds the mod reads at start
// (a CSV: server address in the first column, the seed with an `L` after it in the third, the hash of the seed in the
// seventh), and the form a cracked seed is sent in. Fetching and sending are the game side's.
use std::collections::HashMap;

/// The sheet the mod reads, and the script it posts to (`DATABASE_URL`, `DATABASE_POST_URL`).
pub const DATABASE_URL: &str = "https://docs.google.com/spreadsheets/d/1tuQiE-0leW88em9OHbZnH-RFNhVqgoHhIt9WQbeqqWw/export?format=csv&gid=0";
pub const DATABASE_POST_URL: &str = "https://script.google.com/macros/s/AKfycbye87L-fEYq2EkgczvhKb_kGecp5wL1oX95vg45TRSwNvpv7K-53zoInGTeI1FZ0kv7DA/exec";
/// The page the `/seedcracker database` command opens.
pub const DATABASE_PAGE_URL: &str = "https://docs.google.com/spreadsheets/d/1tuQiE-0leW88em9OHbZnH-RFNhVqgoHhIt9WQbeqqWw/edit?usp=sharing";
pub const USER_AGENT: &str = "SeedcrackerX mod";
/// The id the mod gives Mojang's session server when it proves who the player is to the database.
pub const AUTH_SERVER_ID: &str = "seedcrackerx";

#[derive(Default, Debug)]
pub struct Database {
    connection_to_seed: HashMap<String, i64>,
    hashed_seed_to_seed: HashMap<i64, i64>,
}

impl Database {
    /// `parseCsv`: the header line is skipped, and a row that is too short or has no number where one is wanted is too.
    pub fn parse_csv(&mut self, csv: &str) {
        for row in csv.split('\n').skip(1) {
            let entry: Vec<&str> = row.split(',').collect();
            if entry.len() < 7 {
                continue;
            }
            let seed_string = entry[2];
            if seed_string.is_empty() {
                continue;
            }
            // The seed is written with an `L` after it, which the Java cuts off whatever it is.
            let digits = &seed_string[..seed_string.len() - seed_string.chars().last().map_or(0, char::len_utf8)];
            let (Ok(seed), Ok(hashed_seed)) = (digits.parse::<i64>(), entry[6].trim().parse::<i64>()) else { continue };
            self.connection_to_seed.insert(entry[0].to_owned(), seed);
            self.hashed_seed_to_seed.insert(hashed_seed, seed);
        }
    }

    /// `getSeed(connection, hashedSeed)`: by the server's address first, then by the hash of the seed.
    pub fn get_seed(&self, connection: &str, hashed_seed: i64) -> Option<i64> {
        self.connection_to_seed.get(connection).or_else(|| self.hashed_seed_to_seed.get(&hashed_seed)).copied()
    }

    /// Look up only the current world's hash; a matching address is not seed verification.
    pub fn get_seed_by_hash(&self, hashed_seed: i64) -> Option<i64> {
        self.hashed_seed_to_seed.get(&hashed_seed).copied()
    }

    pub fn len(&self) -> usize {
        self.connection_to_seed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.connection_to_seed.is_empty()
    }
}

/// `HttpDiscoveryService.buildQuery`: the fields of the post, percent-encoded as a form.
pub fn build_query(fields: &[(&str, String)]) -> String {
    fields.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&")
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// What `handleDatabaseCall` posts for a cracked seed.
pub fn post_body(server_ip: &str, dimension: &str, seed: i64, version: &str, username: &str, anonymous: bool) -> String {
    // The seed goes as a string, because the script on the other end reads a number as a float.
    build_query(&[
        ("serverIp", server_ip.to_owned()),
        ("dimension", dimension.to_owned()),
        ("seed", format!("{seed}L")),
        ("version", version.to_owned()),
        ("username", username.to_owned()),
        ("hash", if anonymous { "1" } else { "0" }.to_owned()),
    ])
}
