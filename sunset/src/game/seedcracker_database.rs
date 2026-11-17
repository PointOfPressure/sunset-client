// kaptainwutax.seedcrackerX.util.Database, the part that needs no network: the sheet of server seeds the mod reads at start
// (a CSV: server address in the first column, the seed with an `L` after it in the third, the hash of the seed in the
// seventh). Fetching is the game side's; Sunset never posts a seed.
use std::collections::HashMap;

/// The sheet the mod reads (`DATABASE_URL`).
pub const DATABASE_URL: &str = "https://docs.google.com/spreadsheets/d/1tuQiE-0leW88em9OHbZnH-RFNhVqgoHhIt9WQbeqqWw/export?format=csv&gid=0";

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
