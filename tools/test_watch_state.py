#!/usr/bin/env python3
"""Offline fake-clock tests for the std-only evidence engine; never starts a game/server."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUSTC = os.environ.get("SUNSET_TEST_RUSTC", os.environ.get("RUSTC", "rustc"))
HARNESS = r'''
#[path = "__SOURCE__"]
mod watch_state;
use watch_state::*;

fn names(names: &[&str]) -> Vec<String> { names.iter().map(|s| s.to_string()).collect() }
fn state(probe: bool, names_: &[&str]) -> WatchState {
    let mut state = WatchState::default();
    state.configure(Config { enabled: true, passive_tab: true, completion_probe: probe }, &names(names_));
    state
}
fn player(uuid: u128, name: &str, listed: bool) -> TabPlayer {
    TabPlayer { uuid, name: name.to_owned(), listed, spectator: false }
}
fn baseline(state: &mut WatchState) { state.update_roster(0, &[player(1, "Alice", true)], &[]); }

#[test]
fn default_probe_is_opt_in_and_disabled_is_quiet() {
    assert!(Config::default().passive_tab);
    assert!(!Config::default().completion_probe);
    let mut s = WatchState::default();
    s.configure(Config::default(), &names(&["Alice"]));
    assert!(s.update_roster(0, &[player(1, "Alice", false)], &[]).is_empty());
    assert!(s.probe_candidate(10_000).is_none());
    let mut s = state(false, &["Alice"]);
    s.update_roster(0, &[], &[]);
    assert!(s.probe_candidate(10_000).is_none());
}

#[test]
fn enabling_with_absent_or_unlisted_player_has_no_disappearance() {
    for roster in [vec![], vec![player(1, "Alice", false)]] {
        let mut s = state(false, &["Alice"]);
        assert!(s.update_roster(0, &roster, &[]).is_empty());
        assert!(s.update_roster(99_000, &roster, &[]).is_empty());
        assert_eq!(s.rows(99_000)[0].evidence, None);
    }
}

#[test]
fn hidden_requires_stable_listed_interval_and_is_deduplicated() {
    let mut s = state(false, &["Alice"]);
    baseline(&mut s);
    assert!(s.update_roster(1499, &[player(1, "Alice", false)], &[]).is_empty());
    s.update_roster(2000, &[player(1, "Alice", true)], &[]);
    let alerts = s.update_roster(3500, &[player(1, "Alice", false)], &[]);
    assert_eq!(alerts[0].evidence, Evidence::HiddenFromTab);
    assert_eq!(alerts[0].evidence.label(), "Hidden from TAB");
    assert!(s.update_roster(4000, &[player(1, "Alice", false)], &[]).is_empty());
    assert_eq!(s.rows(63_500)[0].evidence, None);
}

#[test]
fn missing_player_is_unconfirmed_after_grace_and_rejoin_cancels() {
    let mut s = state(false, &["Alice"]);
    baseline(&mut s);
    assert!(s.update_roster(1500, &[], &[]).is_empty());
    assert!(s.update_roster(2499, &[], &[]).is_empty());
    assert!(s.update_roster(2499, &[player(1, "Alice", true)], &[]).is_empty());
    assert!(s.update_roster(2500, &[], &[]).is_empty()); // Rejoin did not re-establish stability.
    assert!(s.update_roster(5000, &[], &[]).is_empty());
    s.update_roster(6000, &[player(1, "Alice", true)], &[]);
    s.update_roster(7500, &[], &[]);
    let alerts = s.update_roster(8500, &[], &[]);
    assert_eq!(alerts[0].evidence.label(), "TAB disappearance; unconfirmed");
    assert!(s.update_roster(10_000, &[], &[]).is_empty());
}

#[test]
fn explicit_departure_suppresses_pending_removal_and_probes() {
    let mut s = state(true, &["Alice"]);
    baseline(&mut s);
    s.update_roster(1500, &[], &[]);
    assert!(s.update_roster(2000, &[], &[1]).is_empty());
    assert!(s.update_roster(3000, &[], &[]).is_empty());
    assert!(s.probe_candidate(3000).is_none());
    assert_eq!(s.rows(3000)[0].evidence, None);
}

#[test]
fn uuid_binding_does_not_follow_name_to_another_account() {
    let mut s = state(false, &["aLiCe"]);
    baseline(&mut s);
    let impostor = player(2, "ALICE", false);
    s.update_roster(1500, &[impostor], &[]);
    assert!(!s.rows(1500)[0].present_in_roster);
    assert_eq!(s.rows(1500)[0].uuid, Some(1));
    s.update_roster(1600, &[player(1, "Alicia", true)], &[]);
    assert_eq!(s.rows(1600)[0].name, "Alicia");
    assert_eq!(s.rows(1600)[0].uuid, Some(1));
}

#[test]
fn completion_cannot_attribute_another_uuid_or_ambiguous_name_to_bound_account() {
    for listed in [false, true] {
        let mut s = state(true, &["Alice"]);
        baseline(&mut s);
        s.update_roster(1500, &[player(2, "Alice", listed)], &[]);
        assert!(s.probe_candidate(1500).is_none());
    }
    let mut s = state(true, &["Alice"]);
    s.update_roster(0, &[player(1, "Alice", false), player(2, "ALICE", false)], &[]);
    assert!(s.probe_candidate(0).is_none());
}

#[test]
fn renamed_account_is_not_bound_twice_and_old_probe_cannot_follow_new_name() {
    let mut s = state(true, &["Alice", "Bob"]);
    baseline(&mut s);
    s.update_roster(1, &[], &[]);
    assert!(s.probe_sent(1, -1, "Alice"));
    s.update_roster(2, &[player(1, "Bob", false)], &[]);
    assert_eq!(s.rows(2)[0].name, "Bob");
    assert_eq!(s.rows(2)[1].uuid, None);
    assert!(s.completion_reply(3, -1, &names(&["Alice"])).is_empty());
    // The unresolved configured Bob cannot issue a second probe for the same account.
    assert!(s.probe_candidate(1001).is_none());
    assert_eq!(s.probe_candidate(10_001).as_deref(), Some("Bob"));
}

#[test]
fn ambiguous_name_and_duplicate_uuid_do_not_bind_or_remove() {
    let mut s = state(false, &["Alice"]);
    s.update_roster(0, &[player(1, "Alice", true), player(2, "ALICE", true)], &[]);
    assert_eq!(s.rows(0)[0].uuid, None);
    s.update_roster(1, &[player(1, "Alice", true)], &[]);
    s.update_roster(2000, &[player(1, "Alice", false), player(1, "Alice", true)], &[]);
    assert!(s.rows(2000)[0].listed);
}

#[test]
fn reset_reconnect_and_config_changes_clear_identity_and_evidence() {
    let mut s = state(false, &["Alice"]);
    baseline(&mut s);
    s.update_roster(1500, &[player(1, "Alice", false)], &[]);
    assert!(s.rows(1500)[0].evidence.is_some());
    assert!(!s.configure(Config { enabled: true, passive_tab: true, completion_probe: false }, &names(&["ALICE"])));
    assert!(s.rows(1500)[0].evidence.is_some());
    s.reset();
    assert_eq!(s.rows(1500)[0].uuid, None);
    assert!(s.update_roster(2000, &[player(2, "Alice", false)], &[]).is_empty());
    assert_eq!(s.rows(2000)[0].uuid, Some(2));
    s.configure(Config { enabled: true, passive_tab: false, completion_probe: false }, &names(&["Bob"]));
    assert_eq!(s.rows(2000)[0].name, "bob");
    assert_eq!(s.rows(2000)[0].uuid, None);
}

#[test]
fn passive_option_and_spectator_status_are_independent() {
    let mut s = state(false, &["Alice"]);
    s.configure(Config { enabled: true, passive_tab: false, completion_probe: false }, &names(&["Alice"]));
    baseline(&mut s);
    let mut p = player(1, "Alice", false);
    p.spectator = true;
    assert!(s.update_roster(1500, &[p], &[]).is_empty());
    assert!(s.rows(1500)[0].spectator);
    assert_eq!(s.rows(1500)[0].evidence, None);
}

#[test]
fn completion_requires_exact_request_and_exact_name_and_is_never_nearby() {
    let mut s = state(true, &["Alice", "Bob"]);
    s.update_roster(0, &[], &[]);
    assert_eq!(s.probe_candidate(0).as_deref(), Some("alice"));
    assert!(s.probe_sent(0, -1, "ALICE"));
    assert!(s.completion_reply(1, 1, &names(&["Alice"])).is_empty());
    assert!(s.completion_reply(2, -2, &names(&["Alice"])).is_empty());
    let alerts = s.completion_reply(3, -1, &names(&["ALICE", "Bob"]));
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].evidence, Evidence::SuggestedOffTab);
    assert_eq!(alerts[0].evidence.label(), "Possibly online: suggested while off TAB");
    assert_eq!(s.rows(3)[1].evidence, None);
    assert!(s.completion_reply(4, -1, &names(&["Alice"])).is_empty());
}

#[test]
fn completion_rejects_suffixes_command_tokens_and_beyond_bound_results() {
    for suggestions in [names(&["Alice2", "/Alice", " Alice", "Alice "]), {
        let mut v = vec!["x".to_owned(); MAX_SUGGESTIONS]; v.push("Alice".to_owned()); v
    }] {
        let mut s = state(true, &["Alice"]);
        s.update_roster(0, &[], &[]);
        assert!(s.probe_sent(0, -1, "alice"));
        assert!(s.completion_reply(1, -1, &suggestions).is_empty());
        assert_eq!(s.rows(1)[0].evidence, None);
    }
}

#[test]
fn completion_timeout_stale_ids_and_reconnect_are_isolated() {
    let mut s = state(true, &["Alice", "Bob"]);
    s.update_roster(0, &[], &[]);
    assert!(!s.probe_sent(0, 0, "Alice"));
    assert!(s.probe_sent(0, -1, "Alice"));
    assert!(s.probe_candidate(5000).is_none());
    assert!(s.completion_reply(5001, -1, &names(&["Alice"])).is_empty());
    assert_eq!(s.probe_candidate(5001).as_deref(), Some("bob"));
    assert!(s.probe_sent(5001, -2, "Bob"));
    assert!(s.completion_reply(5002, -1, &names(&["Bob"])).is_empty());
    s.reset();
    s.update_roster(5003, &[], &[]);
    assert!(s.completion_reply(5004, -2, &names(&["Bob"])).is_empty());
    assert!(s.probe_candidate(5004).is_none()); // Reset cannot bypass global rate limit.
    assert!(!s.probe_sent(6001, -2, "Alice"));
    assert!(s.probe_sent(6001, -3, "Alice"));
    assert!(s.completion_reply(6002, -2, &names(&["Alice"])).is_empty());
    assert_eq!(s.completion_reply(6003, -3, &names(&["Alice"]))[0].evidence, Evidence::SuggestedOffTab);
}

#[test]
fn completion_inflight_global_and_per_player_limits_are_enforced() {
    let mut s = state(true, &["Alice", "Bob"]);
    s.update_roster(0, &[], &[]);
    assert!(s.probe_sent(0, -1, "Alice"));
    assert!(s.probe_candidate(1000).is_none());
    s.completion_reply(1000, -1, &[]);
    assert_eq!(s.probe_candidate(1000).as_deref(), Some("bob"));
    assert!(s.probe_sent(1000, -2, "Bob"));
    s.completion_reply(1001, -2, &[]);
    assert!(s.probe_candidate(9999).is_none());
    assert_eq!(s.probe_candidate(10_000).as_deref(), Some("alice"));
}

#[test]
fn completion_listed_rejoin_cancels_and_config_change_rejects_reply() {
    let mut s = state(true, &["Alice"]);
    s.update_roster(0, &[], &[]);
    assert!(s.probe_sent(0, -1, "Alice"));
    s.update_roster(1, &[player(1, "Alice", true)], &[]);
    assert!(s.completion_reply(2, -1, &names(&["Alice"])).is_empty());
    assert!(s.probe_candidate(10_000).is_none());
    s.update_roster(10_001, &[], &[]);
    assert!(s.probe_sent(10_001, -2, "Alice"));
    s.configure(Config { enabled: true, passive_tab: true, completion_probe: true }, &names(&["Bob"]));
    assert!(s.completion_reply(10_002, -2, &names(&["Alice"])).is_empty());
}

#[test]
fn completion_at_timeout_boundary_is_valid_and_expires() {
    let mut s = state(true, &["Alice"]);
    s.update_roster(0, &[], &[]);
    assert!(s.probe_sent(0, -1, "Alice"));
    assert_eq!(s.completion_reply(5000, -1, &names(&["Alice"]))[0].evidence, Evidence::SuggestedOffTab);
    assert_eq!(s.rows(64_999)[0].evidence, Some(Evidence::SuggestedOffTab));
    assert_eq!(s.rows(65_000)[0].evidence, None);
}

#[test]
fn nearby_requires_resolved_uuid_off_tab_finite_coordinates_and_64m() {
    let mut s = state(true, &["Alice", "Bob"]);
    s.update_roster(0, &[player(1, "Alice", false)], &[]);
    for (uuid, pos) in [(2, [0.0; 3]), (1, [64.001, 0.0, 0.0]), (1, [f64::NAN, 0.0, 0.0]), (1, [f64::INFINITY, 0.0, 0.0])] {
        assert!(s.update_nearby(0, [0.0; 3], &[NearbyPlayer { uuid, position: pos }]).is_empty());
    }
    assert!(s.update_nearby(0, [f64::NAN; 3], &[NearbyPlayer { uuid: 1, position: [0.0; 3] }]).is_empty());
    let alerts = s.update_nearby(0, [0.0; 3], &[NearbyPlayer { uuid: 1, position: [64.0, 0.0, 0.0] }]);
    assert_eq!(alerts[0].evidence, Evidence::NearbyEntity);
    assert_eq!(s.rows(0)[1].evidence, None);
    s.update_nearby(1, [0.0; 3], &[]);
    assert_eq!(s.rows(1)[0].evidence, None);
    s.update_roster(2, &[player(1, "Alice", true)], &[]);
    assert!(s.update_nearby(2, [0.0; 3], &[NearbyPlayer { uuid: 1, position: [0.0; 3] }]).is_empty());
}

#[test]
fn strongest_evidence_escalates_without_combining_players_and_cools_down() {
    let mut s = state(true, &["Alice", "Bob"]);
    s.update_roster(0, &[player(1, "Alice", false)], &[]);
    s.probe_sent(0, -1, "Alice");
    assert_eq!(s.completion_reply(1, -1, &names(&["Alice"]))[0].evidence, Evidence::SuggestedOffTab);
    let entity = NearbyPlayer { uuid: 1, position: [0.0; 3] };
    assert_eq!(s.update_nearby(2, [0.0; 3], &[entity])[0].evidence, Evidence::NearbyEntity);
    assert!(s.update_nearby(60_001, [0.0; 3], &[entity]).is_empty());
    assert_eq!(s.update_nearby(60_002, [0.0; 3], &[entity])[0].evidence, Evidence::NearbyEntity);
    assert_eq!(s.rows(60_002)[1].evidence, None);
}

#[test]
fn backwards_clock_cannot_create_stability_or_accept_reply() {
    let mut s = state(true, &["Alice"]);
    s.update_roster(10_000, &[player(1, "Alice", true)], &[]);
    assert!(s.update_roster(0, &[], &[]).is_empty());
    assert!(s.rows(0)[0].listed);
    s.update_roster(10_001, &[player(1, "Alice", false)], &[]);
    assert_eq!(s.rows(10_001)[0].evidence, None);
    assert!(s.probe_sent(10_001, -1, "Alice"));
    assert!(s.completion_reply(1, -1, &names(&["Alice"])).is_empty());
    assert_eq!(s.completion_reply(10_002, -1, &names(&["Alice"]))[0].evidence, Evidence::SuggestedOffTab);
}

#[test]
fn bounds_and_mass_removals_remain_unconfirmed() {
    let mut all = (0..200).map(|i| format!("P{i}")).collect::<Vec<_>>();
    all[0] = "invalid name".to_owned();
    let mut s = WatchState::default();
    s.configure(Config { enabled: true, passive_tab: true, completion_probe: false }, &all);
    assert!(s.rows(0).len() <= MAX_WATCHED);
    let roster = (0..128).map(|i| player(i, &format!("P{i}"), true)).collect::<Vec<_>>();
    s.update_roster(0, &roster, &[]);
    let oversized = (0..=MAX_ROSTER).map(|i| player(i as u128 + 10_000, "X", true)).collect::<Vec<_>>();
    assert!(s.update_roster(1500, &oversized, &[]).is_empty());
    assert!(s.rows(1500).iter().all(|r| r.listed));
    assert!(s.update_roster(1500, &[], &[]).is_empty());
    let alerts = s.update_roster(2500, &[], &[]);
    assert_eq!(alerts.len(), 127);
    assert!(alerts.iter().all(|a| a.evidence == Evidence::TabDisappearance));
    assert!(s.update_roster(3000, &[], &[]).is_empty());
}
'''

with tempfile.TemporaryDirectory(prefix="sunset-watch-tests-") as temporary:
    directory = Path(temporary)
    source = directory / "watch_tests.rs"
    output = directory / ("watch_tests.exe" if os.name == "nt" else "watch_tests")
    source.write_text(HARNESS.replace("__SOURCE__", (ROOT / "mixins/sunset/watch_state.rs").as_posix()), encoding="utf-8")
    subprocess.run([RUSTC, "--edition=2024", "--test", str(source), "-o", str(output)], check=True)
    subprocess.run([str(output), "--test-threads=1"], check=True)
