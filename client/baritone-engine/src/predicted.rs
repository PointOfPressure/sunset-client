use crate::baritone_utils::Pos;

pub const TARGET_TICKS: u64 = 1200;
pub const REVEAL_TICKS: u64 = 40;
pub const EMPTY_TICKS: u64 = 200;
pub const MISS_LIMIT: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Candidate,
    Approach,
    Expose,
    AwaitReveal,
    Verify,
    Mine,
    AwaitConfirmation,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Mined,
    Skipped,
    Mismatch,
    Unconfirmed,
    Unreachable,
    Rejected,
}

impl Outcome {
    pub fn name(self) -> &'static str {
        match self {
            Self::Mined => "mined",
            Self::Skipped => "skipped",
            Self::Mismatch => "mismatch",
            Self::Unconfirmed => "unconfirmed",
            Self::Unreachable => "unreachable",
            Self::Rejected => "rejected",
        }
    }
}

pub fn matches_kind(kind: u8, name: &str) -> bool {
    let bare = name.strip_prefix("minecraft:").unwrap_or(name);
    match kind {
        0 => matches!(bare, "coal_ore" | "deepslate_coal_ore"),
        1 => matches!(bare, "copper_ore" | "deepslate_copper_ore"),
        2 => matches!(bare, "iron_ore" | "deepslate_iron_ore"),
        3 => matches!(bare, "lapis_ore" | "deepslate_lapis_ore"),
        4 => matches!(bare, "redstone_ore" | "deepslate_redstone_ore"),
        5 => matches!(bare, "gold_ore" | "deepslate_gold_ore" | "nether_gold_ore"),
        6 => matches!(bare, "diamond_ore" | "deepslate_diamond_ore"),
        7 => matches!(bare, "emerald_ore" | "deepslate_emerald_ore"),
        8 => bare == "nether_quartz_ore",
        9 => bare == "ancient_debris",
        _ => false,
    }
}

pub fn plausible_host(kind: u8, dimension: &str, name: &str) -> bool {
    let bare = name.strip_prefix("minecraft:").unwrap_or(name);
    match dimension {
        "minecraft:overworld" if kind < 8 => matches!(
            bare,
            "stone" | "deepslate" | "granite" | "diorite" | "andesite" | "tuff"
        ),
        "minecraft:the_nether" if matches!(kind, 5 | 8 | 9) => {
            matches!(bare, "netherrack" | "basalt" | "blackstone")
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub loaded: bool,
    pub air: bool,
    pub matching: bool,
    pub plausible: bool,
    pub pending: bool,
    pub server_matching: bool,
    pub at_stance: bool,
    pub reachable: bool,
    pub ray_hit: bool,
    pub harvestable: bool,
    pub safe_to_break: bool,
    pub path_failed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Directive {
    Wait,
    Approach { alternate: bool },
    Attack,
    Terminal(Outcome),
    ResourceFailure,
}

pub struct PredictedMineProcess {
    pub pos: Pos,
    pub kind: u8,
    pub phase: Phase,
    pub started: u64,
    pub phase_started: u64,
    pub alternate: bool,
    pub retries: u8,
    attacked: bool,
    outcome: Option<Outcome>,
}

impl PredictedMineProcess {
    pub fn new(pos: Pos, kind: u8, tick: u64) -> Self {
        Self {
            pos,
            kind,
            phase: Phase::Candidate,
            started: tick,
            phase_started: tick,
            alternate: false,
            retries: 0,
            attacked: false,
            outcome: None,
        }
    }
    pub fn protects_target(&self) -> bool {
        self.phase != Phase::Terminal
    }
    fn transition(&mut self, phase: Phase, tick: u64) {
        self.phase = phase;
        self.phase_started = tick;
    }
    fn finish(&mut self, outcome: Outcome) -> Directive {
        self.phase = Phase::Terminal;
        self.outcome = Some(outcome);
        Directive::Terminal(outcome)
    }
    fn alternate_or(&mut self, tick: u64, outcome: Outcome) -> Directive {
        if self.alternate {
            self.finish(outcome)
        } else {
            self.alternate = true;
            self.transition(Phase::Approach, tick);
            Directive::Approach { alternate: true }
        }
    }
    pub fn step(&mut self, tick: u64, o: Observation) -> Directive {
        if let Some(outcome) = self.outcome {
            return Directive::Terminal(outcome);
        }
        if tick.saturating_sub(self.started) >= TARGET_TICKS {
            return self.finish(Outcome::Unconfirmed);
        }
        if !o.loaded {
            return Directive::Wait;
        }
        if matches!(self.phase, Phase::Mine | Phase::AwaitConfirmation) && self.attacked {
            if o.pending {
                self.transition(Phase::AwaitConfirmation, tick);
                return Directive::Wait;
            }
            if !o.matching && !o.server_matching {
                return self.finish(Outcome::Mined);
            }
            if self.phase == Phase::AwaitConfirmation {
                if self.retries >= 1 {
                    return self.finish(Outcome::Rejected);
                }
                self.retries += 1;
                self.transition(Phase::Verify, tick);
            }
        } else if o.air && !o.pending {
            return self.finish(Outcome::Skipped);
        }
        if o.pending {
            return Directive::Wait;
        }
        if !o.matching && !o.plausible && !o.pending {
            return self.finish(Outcome::Mismatch);
        }
        if o.matching && !o.pending && !matches!(self.phase, Phase::Mine | Phase::AwaitConfirmation)
        {
            self.transition(Phase::Verify, tick);
        }
        match self.phase {
            Phase::Candidate => {
                self.transition(Phase::Approach, tick);
                Directive::Approach {
                    alternate: self.alternate,
                }
            }
            Phase::Approach => {
                if o.path_failed {
                    return self.alternate_or(tick, Outcome::Unreachable);
                }
                if o.at_stance {
                    self.transition(Phase::Expose, tick);
                    Directive::Wait
                } else {
                    Directive::Approach {
                        alternate: self.alternate,
                    }
                }
            }
            Phase::Expose => {
                if !o.reachable {
                    return self.alternate_or(tick, Outcome::Unconfirmed);
                }
                self.transition(Phase::AwaitReveal, tick);
                Directive::Wait
            }
            Phase::AwaitReveal => {
                if tick.saturating_sub(self.phase_started) >= REVEAL_TICKS {
                    self.alternate_or(tick, Outcome::Unconfirmed)
                } else {
                    Directive::Wait
                }
            }
            Phase::Verify | Phase::Mine => {
                if !o.matching || o.pending {
                    return Directive::Wait;
                }
                if !o.harvestable {
                    return Directive::ResourceFailure;
                }
                if !o.safe_to_break {
                    return self.finish(Outcome::Rejected);
                }
                if !o.reachable || !o.ray_hit {
                    self.transition(Phase::Approach, tick);
                    return Directive::Approach {
                        alternate: self.alternate,
                    };
                }
                self.transition(Phase::Mine, tick);
                self.attacked = true;
                Directive::Attack
            }
            Phase::AwaitConfirmation => Directive::Wait,
            Phase::Terminal => Directive::Wait,
        }
    }
}
