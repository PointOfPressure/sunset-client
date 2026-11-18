// The click GUI's search: every module scored against what was typed, best first. Each word typed must match
// something of the module's, in any order ("aura kill" is Kill Aura); a word scores by how it matches, best kept:
//
//   the whole name, the name's start, a word's start in it, its initials ("ka" Kill Aura, "bf" Base Finder), inside
//   the name, one of its other names (aliases), a typo or two ("kilaura", "scafold": one wrong letter in four, two
//   from seven), its letters in order ("klaur"), a setting's name (the matching settings show under it, as Geode
//   Finder's Tracers do for "tracers"), its description, its category.
//
// A single letter only matches the start of a word or the initials, and settings, descriptions and typos need three or
// four letters, so short searches stay short. The scores of a search are kept until it changes: the menu asks every
// frame.

use super::sunset::{Module, aliases, norm};
use std::sync::{Arc, Mutex};

// A module a search found: how well, and the settings that matched (a setting named for what was typed).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Found {
    pub(super) score: u32,
    pub(super) settings: Vec<usize>,
}

// The words of a name or text: lowercase letters and digits, split at anything else.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_ascii_lowercase).collect()
}

// Edit distance (insert, delete, change, swap two letters), giving up past `max`.
fn distance(a: &[u8], b: &[u8], max: usize) -> usize {
    if a.len().abs_diff(b.len()) > max {
        return max + 1;
    }
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        rows[0][j] = j;
    }
    for i in 1..=a.len() {
        let mut best = usize::MAX;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut d = (rows[i - 1][j] + 1).min(rows[i][j - 1] + 1).min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = d;
            best = best.min(d);
        }
        if best > max {
            return max + 1;
        }
    }
    rows[a.len()][b.len()]
}

// A word typed with a typo or two of `target` (or of its start: "scafol" for Scaffold).
fn close(typed: &str, target: &str) -> bool {
    let allowed = match typed.len() {
        0..4 => return false,
        4..7 => 1,
        _ => 2,
    };
    let (t, w) = (typed.as_bytes(), target.as_bytes());
    distance(t, w, allowed) <= allowed || (w.len() > t.len() && distance(t, &w[..t.len()], allowed) <= allowed)
}

// The letters of `typed` in `target`, in order, starting with its first.
fn in_order(typed: &str, target: &str) -> bool {
    let mut rest = target.chars();
    typed.chars().next() == target.chars().next() && typed.chars().all(|c| rest.any(|t| t == c))
}

// How well one typed word matches a module, and the settings it matched (by name only, when nothing better did).
fn word_score(word: &str, m: &Module, name: &str, name_words: &[String], initials: &str, alias: &[String], settings: &mut Vec<usize>) -> u32 {
    let long = word.len() >= 3;
    let best = if name == word {
        1000
    } else if name.starts_with(word) {
        900
    } else if name_words.iter().any(|w| w.starts_with(word)) {
        800
    } else if word.len() >= 2 && initials.starts_with(word) {
        750
    } else if word.len() >= 2 && name.contains(word) {
        600
    } else if alias.iter().any(|w| w == word) {
        560
    } else if word.len() >= 2 && alias.iter().any(|w| w.starts_with(word)) {
        520
    } else if long && norm(aliases(m.name)).contains(word) {
        480
    } else if close(word, name) || name_words.iter().any(|w| close(word, w)) {
        420
    } else if long && in_order(word, name) {
        360
    } else {
        0
    };
    if best > 0 {
        return best;
    }
    if long {
        let before = settings.len();
        for (j, s) in m.settings.iter().enumerate() {
            if words(s.name).iter().any(|w| w.starts_with(word)) || (word.len() >= 4 && norm(s.name).contains(word)) {
                settings.push(j);
            }
        }
        if settings.len() > before {
            return 300;
        }
        if words(m.about).iter().any(|w| w.starts_with(word)) {
            return 200;
        }
    }
    if words(m.category.label()).iter().any(|w| w.starts_with(word)) {
        return 150;
    }
    0
}

// A module against a search: None when a typed word matches nothing of it.
pub(super) fn score(m: &Module, query: &str) -> Option<Found> {
    let typed = words(query);
    if typed.is_empty() {
        return None;
    }
    let name = norm(m.name);
    let name_words = words(m.name);
    let initials: String = name_words.iter().filter_map(|w| w.chars().next()).collect();
    let alias = words(aliases(m.name));
    let mut settings = Vec::new();
    let mut total = 0;
    for word in &typed {
        let s = word_score(word, m, &name, &name_words, &initials, &alias, &mut settings);
        if s == 0 {
            return None;
        }
        total += s;
    }
    // The whole search, spaces left out, as the name or its start: "kill aura" and "killaura" alike.
    let joined: String = typed.concat();
    if typed.len() > 1 && name == joined {
        total += 1000;
    } else if typed.len() > 1 && name.starts_with(&joined) {
        total += 500;
    }
    // A shorter name is the closer match when the rest is even.
    total = (total / typed.len() as u32) * 4 + 40u32.saturating_sub(name.len() as u32);
    settings.sort_unstable();
    settings.dedup();
    Some(Found { score: total, settings })
}

// Every module's result for `query` (by index), worked out once per search.
pub(super) fn results(modules: &[Module], query: &str) -> Arc<Vec<Option<Found>>> {
    static CACHE: Mutex<Option<(String, usize, Arc<Vec<Option<Found>>>)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((q, n, found)) = cache.as_ref()
        && q == query
        && *n == modules.len()
    {
        return found.clone();
    }
    let found = Arc::new(modules.iter().map(|m| score(m, query)).collect::<Vec<_>>());
    *cache = Some((query.to_owned(), modules.len(), found.clone()));
    found
}

// The indices of `modules` a search finds among `candidates`, best first (the menu's order between equals).
pub(super) fn ranked(found: &[Option<Found>], candidates: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut hits: Vec<(u32, usize)> = candidates.filter_map(|i| found.get(i)?.as_ref().map(|f| (f.score, i))).collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::sunset::State;

    fn best(state: &State, query: &str) -> Option<&'static str> {
        let found = results(&state.modules, query);
        ranked(&found, 0..state.modules.len()).first().map(|&i| state.modules[i].name)
    }

    #[test]
    fn names_initials_typos_and_word_order_find_the_module() {
        let state = State::new();
        assert_eq!(best(&state, "kill aura"), Some("Kill Aura"));
        assert_eq!(best(&state, "aura kill"), Some("Kill Aura"));
        assert_eq!(best(&state, "ka"), Some("Kill Aura"));
        assert_eq!(best(&state, "kilaura"), Some("Kill Aura"));
        assert_eq!(best(&state, "scafold"), Some("Scaffold"));
        assert_eq!(best(&state, "base finder"), Some("Base Finder"));
        assert_eq!(best(&state, "esp"), Some("ESP"));
    }

    #[test]
    fn settings_and_descriptions_are_searched_and_rank_below_names() {
        let state = State::new();
        let found = results(&state.modules, "tracers");
        let order = ranked(&found, 0..state.modules.len());
        assert_eq!(state.modules[order[0]].name, "Tracers");
        let geode = state.modules.iter().position(|m| m.name == "Geode Finder").unwrap();
        let hit = found[geode].as_ref().expect("Geode Finder has a Tracers setting");
        assert_eq!(state.modules[geode].settings[hit.settings[0]].name, "Tracers");
        assert_eq!(best(&state, "vanish"), Some("Anti Vanish"));
        // Words of a description: Sound Detector "Tells what players nearby are doing".
        let sound = state.modules.iter().position(|m| m.name == "Sound Detector").unwrap();
        assert!(results(&state.modules, "nearby")[sound].is_some());
    }

    #[test]
    fn short_and_unmatched_searches_stay_short() {
        let state = State::new();
        assert!(best(&state, "zzqx").is_none());
        let found = results(&state.modules, "k");
        for (i, f) in found.iter().enumerate() {
            if f.is_some() {
                let m = &state.modules[i];
                assert!(words(m.name).iter().any(|w| w.starts_with('k')) || m.category.label().to_ascii_lowercase().starts_with('k'), "{}", m.name);
            }
        }
    }

    #[test]
    fn typos_are_counted_with_swaps() {
        assert_eq!(distance(b"scafold", b"scaffold", 2), 1);
        assert_eq!(distance(b"kilaura", b"killaura", 2), 1);
        assert_eq!(distance(b"auar", b"aura", 1), 1);
        assert!(!close("abc", "abd"));
    }
}
