use sunset_baritone_engine::predicted::*;

fn masked() -> Observation {
    Observation {
        loaded: true,
        air: false,
        matching: false,
        plausible: true,
        pending: false,
        server_matching: false,
        at_stance: false,
        reachable: false,
        ray_hit: false,
        harvestable: true,
        safe_to_break: true,
        path_failed: false,
    }
}
fn ore() -> Observation {
    Observation {
        matching: true,
        plausible: false,
        server_matching: true,
        reachable: true,
        ray_hit: true,
        ..masked()
    }
}

#[test]
fn masked_ore_approaches_exposes_reveals_then_waits_for_ack() {
    let mut p = PredictedMineProcess::new([1, 12, 3], 6, 1);
    assert_eq!(
        p.step(1, masked()),
        Directive::Approach { alternate: false }
    );
    assert!(p.protects_target());
    let stance = Observation {
        at_stance: true,
        reachable: true,
        ..masked()
    };
    assert_eq!(p.step(2, stance), Directive::Wait);
    assert_eq!(p.phase, Phase::Expose);
    assert_eq!(p.step(3, stance), Directive::Wait);
    assert_eq!(p.phase, Phase::AwaitReveal);
    assert_eq!(p.step(4, stance), Directive::Wait);
    assert_eq!(p.step(5, ore()), Directive::Attack);
    assert_eq!(p.phase, Phase::Mine);
    let predicted_air = Observation {
        matching: false,
        air: true,
        pending: true,
        server_matching: true,
        ..ore()
    };
    assert_eq!(p.step(6, predicted_air), Directive::Wait);
    assert_eq!(p.phase, Phase::AwaitConfirmation);
    assert_eq!(p.step(7, predicted_air), Directive::Wait);
    let accepted = Observation {
        pending: false,
        server_matching: false,
        ..predicted_air
    };
    assert_eq!(p.step(8, accepted), Directive::Terminal(Outcome::Mined));
    assert_eq!(p.step(9, accepted), Directive::Terminal(Outcome::Mined));
}

#[test]
fn never_breaks_masked_final_target_even_if_reachable() {
    let mut p = PredictedMineProcess::new([1, 12, 3], 6, 0);
    for tick in 0..200 {
        let directive = p.step(
            tick,
            Observation {
                at_stance: true,
                reachable: true,
                ray_hit: true,
                ..masked()
            },
        );
        assert_ne!(directive, Directive::Attack);
        if matches!(directive, Directive::Terminal(_)) {
            break;
        }
        assert!(p.protects_target());
    }
}

#[test]
fn final_tool_ray_and_live_kind_are_all_required() {
    let mut p = PredictedMineProcess::new([0, 0, 0], 6, 0);
    assert_eq!(
        p.step(
            1,
            Observation {
                ray_hit: false,
                ..ore()
            }
        ),
        Directive::Approach { alternate: false }
    );
    assert_eq!(
        p.step(
            2,
            Observation {
                harvestable: false,
                ..ore()
            }
        ),
        Directive::ResourceFailure
    );
    assert_eq!(
        p.step(
            3,
            Observation {
                matching: false,
                plausible: false,
                server_matching: false,
                ..ore()
            }
        ),
        Directive::Terminal(Outcome::Mismatch)
    );
}

#[test]
fn gone_before_attack_is_skipped_not_mined() {
    let mut p = PredictedMineProcess::new([0, 0, 0], 6, 0);
    assert_eq!(
        p.step(
            1,
            Observation {
                air: true,
                plausible: false,
                ..masked()
            }
        ),
        Directive::Terminal(Outcome::Skipped)
    );
}

#[test]
fn unsafe_verified_ore_is_rejected_without_holding_attack() {
    let mut p = PredictedMineProcess::new([1, 12, 3], 6, 0);
    assert_eq!(
        p.step(
            1,
            Observation {
                safe_to_break: false,
                ..ore()
            }
        ),
        Directive::Terminal(Outcome::Rejected)
    );
}

#[test]
fn pending_air_at_intake_waits_without_claiming_mine() {
    let mut p = PredictedMineProcess::new([0, 0, 0], 6, 0);
    assert_eq!(
        p.step(
            1,
            Observation {
                air: true,
                pending: true,
                ..masked()
            }
        ),
        Directive::Wait
    );
    assert_eq!(p.step(2, ore()), Directive::Attack);
}

#[test]
fn rollback_has_exactly_one_recovery_retry() {
    let mut p = PredictedMineProcess::new([0, 0, 0], 6, 0);
    assert_eq!(p.step(1, ore()), Directive::Attack);
    let pending = Observation {
        pending: true,
        matching: false,
        air: true,
        ..ore()
    };
    assert_eq!(p.step(2, pending), Directive::Wait);
    assert_eq!(p.step(3, ore()), Directive::Attack);
    assert_eq!(p.retries, 1);
    assert_eq!(p.step(4, pending), Directive::Wait);
    assert_eq!(p.step(5, ore()), Directive::Terminal(Outcome::Rejected));
}

#[test]
fn unrevealed_host_tries_second_stance_then_times_out() {
    let mut p = PredictedMineProcess::new([0, 0, 0], 6, 0);
    let stance = Observation {
        at_stance: true,
        reachable: true,
        ..masked()
    };
    p.step(0, masked());
    p.step(1, stance);
    p.step(2, stance);
    assert_eq!(p.step(41, stance), Directive::Wait);
    assert_eq!(p.step(42, stance), Directive::Approach { alternate: true });
    p.step(43, stance);
    p.step(44, stance);
    assert_eq!(
        p.step(84, stance),
        Directive::Terminal(Outcome::Unconfirmed)
    );
}

#[test]
fn unreachable_and_unloaded_are_bounded() {
    let mut p = PredictedMineProcess::new([0, 0, 0], 6, 10);
    p.step(10, masked());
    let failed = Observation {
        path_failed: true,
        ..masked()
    };
    assert_eq!(p.step(11, failed), Directive::Approach { alternate: true });
    assert_eq!(
        p.step(12, failed),
        Directive::Terminal(Outcome::Unreachable)
    );
    let mut p = PredictedMineProcess::new([0, 0, 0], 6, 10);
    let unloaded = Observation {
        loaded: false,
        air: true,
        ..masked()
    };
    assert_eq!(p.step(11, unloaded), Directive::Wait);
    assert_eq!(
        p.step(1210, unloaded),
        Directive::Terminal(Outcome::Unconfirmed)
    );
}

#[test]
fn exact_ore_families_and_geological_hosts_only() {
    assert!(matches_kind(6, "minecraft:deepslate_diamond_ore"));
    assert!(!matches_kind(6, "minecraft:emerald_ore"));
    assert!(!matches_kind(6, "mod:diamond_ore"));
    assert!(plausible_host(
        6,
        "minecraft:overworld",
        "minecraft:deepslate"
    ));
    assert!(!plausible_host(
        6,
        "minecraft:overworld",
        "minecraft:bedrock"
    ));
    assert!(!plausible_host(
        6,
        "minecraft:the_nether",
        "minecraft:netherrack"
    ));
    assert!(plausible_host(
        9,
        "minecraft:the_nether",
        "minecraft:basalt"
    ));
    assert!(!plausible_host(9, "minecraft:overworld", "minecraft:stone"));
}
