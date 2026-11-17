// baritone.process.FollowProcess: walk after entities and stay within followRadius blocks of them. The
// same process picks up items (#pickup): an item is something to walk straight into. It is active only
// while something it follows is there to follow; when the last one is gone it ends.
use super::baritone::Core;
use super::baritone_goals::{rc, GoalBlock, GoalComposite, GoalNear, GoalRef};
use super::baritone_gworld::{bare, dropped_items, player_pos, targets, Target};
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_utils::block_pos_of;
use super::Game;

/// What is followed.
#[derive(Clone, Debug)]
pub enum Filter {
    /// Every living thing but the player (FollowGroup.ENTITIES).
    Entities,
    /// Every other player (FollowGroup.PLAYERS).
    Players,
    /// These kinds of mob (`minecraft:cow`).
    Kinds(Vec<String>),
    /// These players, by name.
    Named(Vec<String>),
    /// Items on the ground: these ones, or all of them when the list is empty.
    Items(Vec<String>),
}

pub struct FollowProcess {
    filter: Option<Filter>,
    /// Where what it follows is now, for #proc and for drawing.
    cache: Vec<[f64; 3]>,
    into: bool,
    /// `isActive`: a filter is set and something is there to follow.
    live: bool,
}

impl FollowProcess {
    pub fn new() -> Self {
        Self { filter: None, cache: Vec::new(), into: false, live: false }
    }

    pub fn follow(&mut self, filter: Filter) {
        self.into = matches!(filter, Filter::Items(_));
        self.filter = Some(filter);
        self.cache.clear();
    }

    pub fn pickup(&mut self, items: Vec<String>) {
        self.follow(Filter::Items(items));
    }

    #[allow(dead_code)]
    pub fn following(&self) -> &[[f64; 3]] {
        &self.cache
    }

    /// Whether a mob or player matches what is followed.
    fn wants(&self, t: &Target) -> bool {
        match &self.filter {
            Some(Filter::Entities) => true,
            Some(Filter::Players) => t.kind == "minecraft:player",
            Some(Filter::Kinds(kinds)) => kinds.iter().any(|k| *k == t.kind),
            Some(Filter::Named(names)) => t.kind == "minecraft:player" && t.name.as_ref().is_some_and(|n| names.iter().any(|w| w.eq_ignore_ascii_case(n))),
            _ => false,
        }
    }

    fn scan(&mut self, core: &Core, game: &Game<'_>) {
        let Some(me) = player_pos(game) else {
            self.cache.clear();
            return;
        };
        let max = f64::from(core.settings.int("followTargetMaxDistance"));
        let near = |p: &[f64; 3]| {
            max <= 0.0 || (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2) + (p[2] - me[2]).powi(2) <= max * max
        };
        self.cache = match &self.filter {
            Some(Filter::Items(items)) => dropped_items(game)
                .into_iter()
                .filter(|(_, name)| items.is_empty() || items.iter().any(|i| i == bare(name)))
                .map(|(p, _)| p)
                .filter(near)
                .collect(),
            Some(_) => targets(game).iter().filter(|t| self.wants(t)).map(|t| t.pos).filter(near).collect(),
            None => Vec::new(),
        };
    }
}

impl Process for FollowProcess {
    fn name(&self) -> &'static str {
        "follow"
    }

    fn display(&self) -> String {
        let at: Vec<String> = self.cache.iter().map(|p| format!("({:.1}, {:.1}, {:.1})", p[0], p[1], p[2])).collect();
        format!("Following [{}]", at.join(", "))
    }

    fn is_active(&self) -> bool {
        self.live
    }

    fn refresh(&mut self, core: &mut Core, game: &mut Game<'_>) {
        // isActive scans the world: with nothing to follow the process is not in control, but the filter stays.
        if self.filter.is_some() {
            self.scan(core, game);
        }
        self.live = self.filter.is_some() && !self.cache.is_empty();
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY
    }

    fn on_tick(&mut self, core: &mut Core, _game: &mut Game<'_>, _calc_failed: bool, _safe: bool) -> PathingCommand {
        if self.cache.is_empty() {
            return PathingCommand::Defer;
        }
        let radius = core.settings.int("followRadius");
        let offset = core.settings.int("followOffsetDistance");
        let goals: Vec<GoalRef> = self
            .cache
            .iter()
            .map(|p| {
                let mut at = *p;
                if offset != 0 && !self.into {
                    let g = super::baritone_goals::GoalXZ::from_direction(*p, core.settings.float("followOffsetDirection") as f32, f64::from(offset));
                    at = [f64::from(g.0), p[1], f64::from(g.1)];
                }
                if self.into { rc(GoalBlock(block_pos_of(at))) } else { rc(GoalNear(block_pos_of(at), radius)) }
            })
            .collect();
        PathingCommand::RevalidateGoalAndPath(rc(GoalComposite(goals)))
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {
        self.filter = None;
        self.cache.clear();
        self.live = false;
    }

    fn cancel(&mut self) {
        self.filter = None;
        self.cache.clear();
        self.live = false;
    }
}
