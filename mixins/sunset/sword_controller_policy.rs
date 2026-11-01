// Original combat-intent controller. All observations and accepted deeds
// are supplied by the native adapter; this module never changes game state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attack {
    pub minimum_charge: f32,
    pub falling_critical: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    pub forward: i8,
    pub strafe: f64,
    pub sprint: bool,
    pub jump: bool,
    pub drop_sprint: bool,
    pub attack: Option<Attack>,
    pub gap: f64,
    pub flip_side: bool,
}
impl Drive {
    fn moving(forward: i8, strafe: f64, sprint: bool) -> Self {
        Self {
            forward,
            strafe,
            sprint,
            ..Self::default()
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub tick: u64,
    pub target: u32,
    pub reach: f64,
    pub their_reach: f64,
    pub height_difference: f64,
    pub ready_in: i32,
    pub charge: f32,
    pub facing: f64,
    pub fleeing: f64,
    pub latency_ms: i32,
    pub own_hurt: i32,
    pub immune: bool,
    pub shield: bool,
    pub grounded: bool,
    pub water: bool,
    pub vertical_speed: f64,
    pub aggressive: bool,
    pub strafe: bool,
    pub jump_crits: bool,
    pub jump_reset: bool,
}
#[derive(Clone, Copy, Debug, Default)]
enum Phase {
    #[default]
    Spacing,
    Air {
        landed: u8,
    },
    Recover {
        left: u8,
        back: bool,
    },
}
#[derive(Default, Debug)]
pub struct Brain {
    target: Option<u32>,
    previous_reach: Option<f64>,
    closing: f64,
    jitter: f64,
    phase: Phase,
    last_hurt: i32,
    reset_until: u64,
}
pub fn ground_safe(distance: Option<f64>, hazard: bool) -> bool {
    !hazard && distance.is_some_and(|d| d.is_finite() && (-0.11..=3.1).contains(&d))
}
impl Brain {
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn set_fight(&mut self, target: u32, jitter: f64) {
        if self.target != Some(target) {
            *self = Self {
                target: Some(target),
                jitter: jitter.clamp(-0.06, 0.06),
                ..Self::default()
            };
        }
    }
    pub fn accepted(&mut self, reach: f64, jitter: f64) {
        self.jitter = jitter.clamp(-0.06, 0.06);
        let back = reach < 2.3;
        self.phase = Phase::Recover {
            left: if back { 2 } else { 1 },
            back,
        };
    }
    pub fn is_air_sequence(&self) -> bool {
        matches!(self.phase, Phase::Air { .. })
    }
    fn gap(&self, v: View) -> (f64, f64) {
        let latency = (f64::from(v.latency_ms.max(0)) / 50.).round().clamp(0., 3.);
        let danger = 3.35 + 0.12 * latency + 2.5 * self.closing.clamp(0., 0.35)
            - if v.aggressive { 0.2 } else { 0. };
        let desired = match v.ready_in {
            i32::MIN..=0 => 2.9,
            1..=2 => 3.08 + 0.1 * f64::from(v.ready_in - 1) + self.jitter,
            _ => (danger + self.jitter).max(2.9 + 0.26 * f64::from(v.ready_in)),
        };
        (desired, danger)
    }
    fn jump_window(v: View) -> bool {
        ((7..=10).contains(&v.ready_in) && (2.3..=5.).contains(&v.reach))
            || ((3..=10).contains(&v.ready_in)
                && (2.3..=4.2).contains(&v.reach)
                && ((v.facing >= 100. && v.fleeing > 0.05) || (v.aggressive && v.facing > 60.)))
    }
    pub fn plan(&mut self, v: View) -> Option<Drive> {
        if self.target != Some(v.target)
            || ![
                v.reach,
                v.their_reach,
                v.height_difference,
                v.facing,
                v.fleeing,
                v.vertical_speed,
                self.jitter,
            ]
            .iter()
            .all(|x| x.is_finite())
            || !v.charge.is_finite()
            || v.reach < 0.
            || v.their_reach < 0.
        {
            return None;
        }
        self.closing = self
            .previous_reach
            .map_or(0., |old| self.closing * 0.5 + (old - v.their_reach) * 0.5);
        self.previous_reach = Some(v.their_reach);
        if v.own_hurt > self.last_hurt && v.own_hurt >= 8 {
            self.reset_until = v.tick.saturating_add(3);
        }
        self.last_hurt = v.own_hurt;
        let (gap, danger) = self.gap(v);
        let side = if v.strafe { 1. } else { 0. };
        let close = v.reach <= 3.;
        let threatened = v.their_reach <= 3.15 && v.facing <= 40.;
        let mut drive = match self.phase {
            Phase::Recover { left, back } if left > 0 => {
                self.phase = if left == 1 {
                    Phase::Spacing
                } else {
                    Phase::Recover {
                        left: left - 1,
                        back,
                    }
                };
                Drive::moving(
                    if back { -1 } else { 0 },
                    side * if back { 0.8 } else { 1. },
                    false,
                )
            }
            _ if (v.reach > 6. && v.reach > gap + 1.) || v.height_difference.abs() > 2.5 => {
                let mut d = Drive::moving(1, 0., true);
                if close && v.ready_in <= 0 && !v.immune && !v.shield {
                    d.attack = Some(Attack {
                        minimum_charge: 0.95,
                        falling_critical: false,
                    });
                }
                d
            }
            _ => self.combat_drive(v, side, close, threatened, gap, danger),
        };
        drive.gap = gap;
        drive.flip_side = v.facing < 8. && v.their_reach < 4.2;
        Some(drive)
    }
    fn combat_drive(
        &mut self,
        v: View,
        side: f64,
        close: bool,
        threatened: bool,
        mut gap: f64,
        danger: f64,
    ) -> Drive {
        if let Phase::Air { landed } = self.phase {
            let landed = if v.grounded {
                landed.saturating_add(1)
            } else {
                0
            };
            self.phase = if landed >= 2 {
                Phase::Spacing
            } else {
                Phase::Air { landed }
            };
            if landed < 2 {
                let falling = v.ready_in <= 1 && v.vertical_speed < 0.1;
                let mut d = Drive::moving(
                    if falling { 0 } else { 1 },
                    if falling { side * 0.35 } else { 0. },
                    !falling,
                );
                if v.ready_in <= 0 && close && !v.immune {
                    d.drop_sprint = true;
                    d.attack = Some(Attack {
                        minimum_charge: 0.95,
                        falling_critical: true,
                    });
                }
                return d;
            }
        }
        if v.jump_crits && v.grounded && !v.water && !threatened && Self::jump_window(v) {
            self.phase = Phase::Air { landed: 0 };
            return Drive {
                jump: true,
                ..Drive::moving(1, 0., true)
            };
        }
        if v.ready_in <= 0 && !v.shield {
            if v.immune {
                let forward = if v.reach > 3.12 {
                    1
                } else if v.reach < 2.88 {
                    -1
                } else {
                    0
                };
                return Drive::moving(forward, side, false);
            }
            let mut d = Drive::moving(
                if v.reach < 1.6 { -1 } else { 1 },
                if v.reach <= 3.5 { side * 0.35 } else { 0. },
                v.reach >= 1.6,
            );
            if close {
                d.attack = Some(Attack {
                    minimum_charge: 0.95,
                    falling_critical: false,
                });
            }
            return d;
        }
        if v.shield {
            gap = gap.max(danger);
        }
        let mut band = if v.their_reach > gap + 0.12 {
            1
        } else if v.their_reach < gap - 0.12 {
            -1
        } else {
            0
        };
        if band < 0 && v.fleeing > 0.15 {
            band = 0;
        }
        let counter = threatened && close && v.charge >= 0.6 && !v.immune && !v.shield;
        let mut d = if v.jump_reset && v.tick < self.reset_until && v.grounded && !v.water {
            Drive {
                jump: true,
                ..Drive::moving(1, 0., true)
            }
        } else if v.ready_in <= 2 && !counter && v.strafe && !v.shield {
            Drive::moving(1, 1., true)
        } else {
            let strafe = if !v.strafe {
                0.
            } else {
                match band {
                    0 => 1.,
                    -1 => 0.8,
                    _ => 0.5,
                }
            };
            Drive::moving(band, strafe, band > 0)
        };
        if counter {
            d.attack = Some(Attack {
                minimum_charge: 0.6,
                falling_critical: false,
            });
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_terrain_support_matches_three_block_scan_and_rejects_unknown_or_hazard() {
        assert!(ground_safe(Some(0.), false));
        assert!(ground_safe(Some(3.), false));
        assert!(!ground_safe(Some(3.2), false));
        assert!(!ground_safe(None, false));
        assert!(!ground_safe(Some(0.), true));
        assert!(!ground_safe(Some(f64::NAN), false));
    }
    fn view() -> View {
        View {
            tick: 10,
            target: 1,
            reach: 3.4,
            their_reach: 3.5,
            height_difference: 0.,
            ready_in: 6,
            charge: 0.5,
            facing: 70.,
            fleeing: 0.,
            latency_ms: 0,
            own_hurt: 0,
            immune: false,
            shield: false,
            grounded: true,
            water: false,
            vertical_speed: 0.,
            aggressive: false,
            strafe: true,
            jump_crits: false,
            jump_reset: true,
        }
    }
    fn brain() -> Brain {
        let mut b = Brain::default();
        b.set_fight(1, 0.);
        b
    }
    #[test]
    fn patient_and_aggressive_use_distinct_enemy_envelope() {
        let mut a = brain();
        let mut b = brain();
        let mut v = view();
        v.ready_in = 3;
        a.plan(v);
        b.plan(v);
        v.their_reach -= 0.7;
        let patient = a.plan(v).unwrap();
        v.aggressive = true;
        let eager = b.plan(v).unwrap();
        assert!((patient.gap - eager.gap - 0.2).abs() < 1e-9);
    }
    #[test]
    fn latency_and_closing_expand_spacing_with_native_ready_ticks() {
        let mut a = brain();
        let mut b = brain();
        let mut v = view();
        v.ready_in = 3;
        let baseline = a.plan(v).unwrap().gap;
        v.latency_ms = 150;
        assert!(b.plan(v).unwrap().gap > baseline);
        v.their_reach -= 0.4;
        assert!(b.plan(v).unwrap().gap > baseline + 0.36);
    }
    #[test]
    fn threatened_counter_is_real_partial_charge_intent() {
        let mut b = brain();
        let mut v = view();
        v.reach = 2.8;
        v.their_reach = 3.;
        v.facing = 0.;
        v.charge = 0.6;
        assert_eq!(b.plan(v).unwrap().attack.unwrap().minimum_charge, 0.6);
        v.charge = 0.59;
        assert!(b.plan(v).unwrap().attack.is_none());
    }
    #[test]
    fn shield_and_immunity_block_counter() {
        for shield in [false, true] {
            let mut b = brain();
            let mut v = view();
            v.reach = 2.8;
            v.their_reach = 3.;
            v.facing = 0.;
            v.charge = 0.8;
            v.immune = !shield;
            v.shield = shield;
            assert!(b.plan(v).unwrap().attack.is_none());
        }
    }
    #[test]
    fn only_accepted_hit_arms_one_or_two_recovery_frames() {
        let mut b = brain();
        let mut v = view();
        v.ready_in = 0;
        v.reach = 2.;
        assert!(b.plan(v).unwrap().attack.is_some());
        assert!(b.plan(v).unwrap().attack.is_some());
        b.accepted(2., 0.);
        assert_eq!(b.plan(v).unwrap().forward, -1);
        assert_eq!(b.plan(v).unwrap().forward, -1);
        assert!(b.plan(v).unwrap().attack.is_some());
    }
    #[test]
    fn normal_critical_window_and_landing_settle_are_stateful() {
        let mut b = brain();
        let mut v = view();
        v.ready_in = 8;
        v.jump_crits = true;
        assert!(b.plan(v).unwrap().jump);
        assert!(b.is_air_sequence());
        v.grounded = false;
        v.ready_in = 0;
        v.reach = 2.9;
        v.vertical_speed = -0.2;
        let d = b.plan(v).unwrap();
        assert!(d.drop_sprint);
        assert!(d.attack.unwrap().falling_critical);
        v.grounded = true;
        v.ready_in = 4;
        v.jump_crits = false;
        b.plan(v);
        assert!(b.is_air_sequence());
        b.plan(v);
        assert!(!b.is_air_sequence());
    }
    #[test]
    fn aggressive_alternate_jump_depends_on_facing_and_flee() {
        let mut b = brain();
        let mut v = view();
        v.ready_in = 4;
        v.jump_crits = true;
        assert!(!b.plan(v).unwrap().jump);
        v.aggressive = true;
        assert!(b.plan(v).unwrap().jump);
    }
    #[test]
    fn new_actual_hurt_opens_three_tick_jump_reset_without_velocity_guess() {
        let mut b = brain();
        let mut v = view();
        v.own_hurt = 10;
        assert!(b.plan(v).unwrap().jump);
        v.tick = 13;
        v.own_hurt = 7;
        assert!(!b.plan(v).unwrap().jump);
    }
    #[test]
    fn fleeing_does_not_trigger_backpedal_and_immune_full_charge_uses_band() {
        let mut b = brain();
        let mut v = view();
        v.their_reach = 2.;
        v.fleeing = 0.2;
        assert_eq!(b.plan(v).unwrap().forward, 0);
        v.ready_in = 0;
        v.immune = true;
        v.reach = 2.9;
        assert_eq!(b.plan(v).unwrap().forward, 0);
    }
    #[test]
    fn new_fight_and_cancel_clear_recovery_and_unknown_facts_fail_closed() {
        let mut b = brain();
        let mut v = view();
        b.accepted(2., 0.);
        b.set_fight(2, 0.);
        assert!(b.plan(v).is_none());
        v.target = 2;
        assert!(b.plan(v).is_some());
        v.reach = f64::NAN;
        assert!(b.plan(v).is_none());
        b.cancel();
        v.reach = 3.;
        assert!(b.plan(v).is_none());
    }
}
