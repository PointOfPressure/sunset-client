// Bounded, game-independent watchlist evidence. Times are caller-supplied monotonic milliseconds.
// Observations describe evidence, never prove that a player enabled vanish.

use std::collections::{HashMap, HashSet};

pub const MAX_WATCHED: usize = 128;
pub const MAX_ROSTER: usize = 4096;
pub const MAX_SUGGESTIONS: usize = 128;
pub const STABLE_LISTED_MS: u64 = 1_500;
pub const REMOVAL_GRACE_MS: u64 = 1_000;
pub const PROBE_INTERVAL_MS: u64 = 1_000;
pub const PROBE_PERSON_MS: u64 = 10_000;
pub const PROBE_TIMEOUT_MS: u64 = 5_000;
pub const EVIDENCE_TTL_MS: u64 = 60_000;
pub const ALERT_COOLDOWN_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub passive_tab: bool,
    pub completion_probe: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            passive_tab: true,
            completion_probe: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TabPlayer {
    pub uuid: u128,
    pub name: String,
    pub listed: bool,
    pub spectator: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct NearbyPlayer {
    pub uuid: u128,
    pub position: [f64; 3],
}

/// Increasing order is presentation priority, not a combined confidence score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Evidence {
    SuggestedOffTab,
    TabDisappearance,
    HiddenFromTab,
    NearbyEntity,
}

impl Evidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::SuggestedOffTab => "Possibly online: suggested while off TAB",
            Self::TabDisappearance => "TAB disappearance; unconfirmed",
            Self::HiddenFromTab => "Hidden from TAB",
            Self::NearbyEntity => "Nearby entity while off TAB",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub name: String,
    pub uuid: Option<u128>,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchRow {
    pub name: String,
    pub uuid: Option<u128>,
    pub present_in_roster: bool,
    pub listed: bool,
    pub spectator: bool,
    pub evidence: Option<Evidence>,
}

#[derive(Clone, Debug)]
struct Entry {
    configured: String,
    name: String,
    uuid: Option<u128>,
    present: bool,
    listed: bool,
    spectator: bool,
    probe_blocked: bool,
    departed: bool,
    listed_since: Option<u64>,
    pending_removal: Option<u64>,
    evidence: [Option<u64>; 4],
    last_alert: Option<(Evidence, u64)>,
    last_probe: Option<u64>,
}

impl Entry {
    fn new(name: String) -> Self {
        Self {
            configured: name.clone(),
            name,
            uuid: None,
            present: false,
            listed: false,
            spectator: false,
            probe_blocked: false,
            departed: false,
            listed_since: None,
            pending_removal: None,
            evidence: [None; 4],
            last_alert: None,
            last_probe: None,
        }
    }

    fn clear_observations(&mut self) {
        self.present = false;
        self.listed = false;
        self.spectator = false;
        self.probe_blocked = false;
        self.listed_since = None;
        self.pending_removal = None;
        self.evidence = [None; 4];
    }

    fn strongest(&self, now: u64) -> Option<Evidence> {
        [
            Evidence::NearbyEntity,
            Evidence::HiddenFromTab,
            Evidence::TabDisappearance,
            Evidence::SuggestedOffTab,
        ]
        .into_iter()
        .find(|kind| {
            self.evidence[*kind as usize].is_some_and(|at| now.saturating_sub(at) < EVIDENCE_TTL_MS)
        })
    }

    fn off_tab(&self) -> bool {
        !self.listed && !self.departed
    }
    fn probe_eligible(&self) -> bool {
        self.off_tab() && !self.probe_blocked
    }
}

#[derive(Clone, Debug)]
struct InFlight {
    id: i32,
    configured: String,
    name: String,
    sent_at: u64,
}

#[derive(Debug, Default)]
pub struct WatchState {
    config: Config,
    entries: Vec<Entry>,
    now: Option<u64>,
    has_roster: bool,
    last_probe: Option<u64>,
    in_flight: Option<InFlight>,
    next_entry: usize,
    // Retained across resets; IDs from a previous connection cannot be reused.
    lowest_probe_id: Option<i32>,
}

fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

impl WatchState {
    /// Names are bounded, validated and deduplicated case-insensitively. Effective changes reset identity/evidence.
    pub fn configure(&mut self, config: Config, names: &[String]) -> bool {
        let mut names: Vec<String> = names
            .iter()
            .take(MAX_WATCHED)
            .map(|n| n.trim().to_ascii_lowercase())
            .filter(|n| valid_name(n))
            .collect();
        names.sort_unstable();
        names.dedup();
        if self.config == config && self.entries.iter().map(|e| &e.configured).eq(names.iter()) {
            return false;
        }
        self.config = config;
        self.entries = names.into_iter().map(Entry::new).collect();
        self.reset();
        true
    }

    /// Required on disconnect, transfer, and profile change, even when configuration is unchanged.
    pub fn reset(&mut self) {
        self.entries = self
            .entries
            .iter()
            .map(|e| Entry::new(e.configured.clone()))
            .collect();
        self.has_roster = false;
        // Keep the global rate limit and request-ID watermark across resets.
        self.in_flight = None;
        self.next_entry = 0;
    }

    fn advance(&mut self, now: u64) -> bool {
        if self.now.is_some_and(|last| now < last) {
            return false;
        }
        self.now = Some(now);
        if self
            .in_flight
            .as_ref()
            .is_some_and(|p| now.saturating_sub(p.sent_at) > PROBE_TIMEOUT_MS)
        {
            self.in_flight = None;
        }
        true
    }

    /// `players` must be a complete roster, including unlisted entries. Oversized or duplicate-UUID
    /// snapshots are ignored rather than treating a truncated roster as a mass departure.
    pub fn update_roster(
        &mut self,
        now: u64,
        players: &[TabPlayer],
        known_leaves: &[u128],
    ) -> Vec<Alert> {
        if !self.config.enabled || players.len() > MAX_ROSTER || !self.advance(now) {
            return Vec::new();
        }
        let mut by_uuid = HashMap::with_capacity(players.len());
        let mut by_name = HashMap::with_capacity(players.len());
        let mut listed_names = HashSet::with_capacity(players.len());
        for player in players {
            if by_uuid.insert(player.uuid, player).is_some() {
                return Vec::new();
            }
            if valid_name(&player.name) {
                let key = player.name.to_ascii_lowercase();
                if player.listed {
                    listed_names.insert(key.clone());
                }
                by_name
                    .entry(key)
                    .and_modify(|entry| *entry = None)
                    .or_insert(Some(player));
            }
        }
        let known_leaves: HashSet<u128> = known_leaves.iter().take(MAX_ROSTER).copied().collect();
        let mut claimed: HashSet<u128> = self.entries.iter().filter_map(|e| e.uuid).collect();
        for entry in &mut self.entries {
            let player = if let Some(uuid) = entry.uuid {
                by_uuid.get(&uuid).copied()
            } else {
                by_name
                    .get(&entry.configured)
                    .copied()
                    .flatten()
                    .filter(|p| !claimed.contains(&p.uuid))
            };
            let was_listed = entry.listed;
            let was_stable = entry
                .listed_since
                .is_some_and(|at| now.saturating_sub(at) >= STABLE_LISTED_MS);
            if let Some(player) = player {
                entry.uuid = Some(player.uuid);
                claimed.insert(player.uuid);
                // A new name is authoritative only when attached to the same bound UUID.
                if valid_name(&player.name) {
                    entry.name.clone_from(&player.name);
                }
                entry.present = true;
                entry.listed = player.listed;
                entry.spectator = player.spectator;
                entry.departed = false;
                entry.pending_removal = None;
                if player.listed {
                    if !was_listed {
                        entry.listed_since = Some(now);
                    }
                    entry.evidence = [None; 4];
                } else {
                    if self.has_roster && was_listed && was_stable && self.config.passive_tab {
                        entry.evidence[Evidence::HiddenFromTab as usize] = Some(now);
                    }
                    entry.listed_since = None;
                }
            } else if entry.uuid.is_some_and(|id| known_leaves.contains(&id)) {
                entry.clear_observations();
                entry.departed = true;
            } else {
                entry.present = false;
                entry.listed = false;
                entry.spectator = false;
                if self.has_roster && was_listed && was_stable && self.config.passive_tab {
                    entry.pending_removal = Some(now);
                }
                entry.listed_since = None;
                if entry
                    .pending_removal
                    .is_some_and(|at| now.saturating_sub(at) >= REMOVAL_GRACE_MS)
                {
                    entry.evidence[Evidence::TabDisappearance as usize] = Some(now);
                    entry.pending_removal = None;
                }
            }
            // A same-name account or ambiguous roster name cannot substantiate completion for
            // the bound account. Never probe a name visibly listed under any UUID.
            let name = entry.name.to_ascii_lowercase();
            entry.probe_blocked = listed_names.contains(&name)
                || by_name.get(&name).is_some_and(|candidate| {
                    candidate.is_none_or(|player| match entry.uuid {
                        Some(uuid) => player.uuid != uuid,
                        None => claimed.contains(&player.uuid),
                    })
                });
        }
        self.has_roster = true;
        if self.in_flight.as_ref().is_some_and(|p| {
            !self
                .entries
                .iter()
                .any(|e| e.configured == p.configured && e.probe_eligible())
        }) {
            self.in_flight = None;
        }
        self.alerts(now)
    }

    /// A complete current snapshot of actually loaded player entities. Generic sound/packet activity
    /// is not an entity observation. Only a resolved UUID and finite 3D distance <=64m qualifies.
    pub fn update_nearby(
        &mut self,
        now: u64,
        local: [f64; 3],
        entities: &[NearbyPlayer],
    ) -> Vec<Alert> {
        if !self.config.enabled
            || !self.has_roster
            || entities.len() > MAX_ROSTER
            || !self.advance(now)
        {
            return Vec::new();
        }
        let near: HashSet<u128> = if local.iter().all(|v| v.is_finite()) {
            entities
                .iter()
                .filter(|entity| {
                    entity.position.iter().all(|v| v.is_finite())
                        && entity
                            .position
                            .iter()
                            .zip(local)
                            .map(|(a, b)| (a - b).powi(2))
                            .sum::<f64>()
                            <= 64.0 * 64.0
                })
                .map(|entity| entity.uuid)
                .collect()
        } else {
            HashSet::new()
        };
        for entry in &mut self.entries {
            entry.evidence[Evidence::NearbyEntity as usize] = None;
            if entry.off_tab() && entry.uuid.is_some_and(|uuid| near.contains(&uuid)) {
                entry.evidence[Evidence::NearbyEntity as usize] = Some(now);
            }
        }
        self.alerts(now)
    }

    /// Suggests a request; does not reserve a slot. Call `probe_sent` only after allocating a unique
    /// reserved negative ID (-1, -2, ...) and successfully sending the request for this exact name.
    pub fn probe_candidate(&mut self, now: u64) -> Option<String> {
        if !self.config.enabled
            || !self.config.completion_probe
            || !self.has_roster
            || !self.advance(now)
            || self.in_flight.is_some()
            || self
                .last_probe
                .is_some_and(|at| now.saturating_sub(at) < PROBE_INTERVAL_MS)
        {
            return None;
        }
        (0..self.entries.len())
            .map(|offset| (self.next_entry + offset) % self.entries.len())
            .find_map(|i| {
                let entry = &self.entries[i];
                (entry.probe_eligible()
                    && entry
                        .last_probe
                        .is_none_or(|at| now.saturating_sub(at) >= PROBE_PERSON_MS))
                .then(|| entry.name.clone())
            })
    }

    pub fn probe_sent(&mut self, now: u64, id: i32, name: &str) -> bool {
        if id >= 0
            || self.lowest_probe_id.is_some_and(|lowest| id >= lowest)
            || self
                .probe_candidate(now)
                .as_deref()
                .is_none_or(|candidate| !candidate.eq_ignore_ascii_case(name))
        {
            return false;
        }
        let Some(index) = self
            .entries
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name) && e.probe_eligible())
        else {
            return false;
        };
        let entry = &mut self.entries[index];
        entry.last_probe = Some(now);
        self.in_flight = Some(InFlight {
            id,
            configured: entry.configured.clone(),
            name: entry.name.clone(),
            sent_at: now,
        });
        self.last_probe = Some(now);
        self.lowest_probe_id = Some(id);
        self.next_entry = (index + 1) % self.entries.len();
        true
    }

    pub fn completion_reply(&mut self, now: u64, id: i32, suggestions: &[String]) -> Vec<Alert> {
        if !self.config.enabled || !self.config.completion_probe || !self.advance(now) {
            return Vec::new();
        }
        let Some(probe) = self.in_flight.as_ref() else {
            return Vec::new();
        };
        if probe.id != id {
            return Vec::new();
        } // Normal chat or stale/out-of-order replies do not consume the request.
        let probe = self.in_flight.take().unwrap();
        if now.saturating_sub(probe.sent_at) > PROBE_TIMEOUT_MS {
            return Vec::new();
        }
        if suggestions
            .iter()
            .take(MAX_SUGGESTIONS)
            .any(|s| valid_name(s) && s.eq_ignore_ascii_case(&probe.name))
        {
            if let Some(entry) = self.entries.iter_mut().find(|e| {
                e.configured == probe.configured
                    && e.probe_eligible()
                    && e.name.eq_ignore_ascii_case(&probe.name)
            }) {
                entry.evidence[Evidence::SuggestedOffTab as usize] = Some(now);
            }
        }
        self.alerts(now)
    }

    fn alerts(&mut self, now: u64) -> Vec<Alert> {
        let mut alerts = Vec::new();
        for entry in &mut self.entries {
            let Some(evidence) = entry.strongest(now) else {
                continue;
            };
            if entry.last_alert.is_none_or(|(previous, at)| {
                evidence > previous || now.saturating_sub(at) >= ALERT_COOLDOWN_MS
            }) {
                entry.last_alert = Some((evidence, now));
                alerts.push(Alert {
                    name: entry.name.clone(),
                    uuid: entry.uuid,
                    evidence,
                });
            }
        }
        alerts
    }

    pub fn rows(&self, now: u64) -> Vec<WatchRow> {
        let now = now.max(self.now.unwrap_or(now));
        self.entries
            .iter()
            .map(|e| WatchRow {
                name: e.name.clone(),
                uuid: e.uuid,
                present_in_roster: e.present,
                listed: e.listed,
                spectator: e.spectator,
                evidence: self.config.enabled.then(|| e.strongest(now)).flatten(),
            })
            .collect()
    }
}
