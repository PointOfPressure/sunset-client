// baritone.process.CustomGoalProcess, written from the Java: go to a goal the user (or a command) set. A process in
// the state GOAL_SET already has control (and keeps cancelling whatever path there is), PATH_REQUESTED asks for a
// path once, and EXECUTING walks it until the goal is reached or no path is found.
use super::baritone::Core;
use super::baritone_goals::GoalRef;
use super::baritone_pathing::{PathingCommand, Process};
use super::Game;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    None,
    GoalSet,
    PathRequested,
    Executing,
}

pub struct CustomGoalProcess {
    /// A goal set since the last tick, for the flight in progress (`setGoal` calls `ElytraProcess.pathTo`).
    pub elytra_update: Option<Option<GoalRef>>,
    goal: Option<GoalRef>,
    /// The most recent goal. Not invalidated upon onLostControl.
    most_recent_goal: Option<GoalRef>,
    state: State,
}

impl CustomGoalProcess {
    pub fn new() -> Self {
        Self { elytra_update: None, goal: None, most_recent_goal: None, state: State::None }
    }

    pub fn goal(&self) -> Option<&GoalRef> {
        self.goal.as_ref()
    }

    pub fn most_recent_goal(&self) -> Option<&GoalRef> {
        self.most_recent_goal.as_ref()
    }

    /// `setGoal`.
    pub fn set_goal_option(&mut self, goal: Option<GoalRef>) {
        self.goal = goal.clone();
        self.most_recent_goal = goal.clone();
        self.elytra_update = Some(goal);
        if self.state == State::None {
            self.state = State::GoalSet;
        }
        if self.state == State::Executing {
            self.state = State::PathRequested;
        }
    }

    pub fn set_goal(&mut self, goal: GoalRef) {
        self.set_goal_option(Some(goal));
    }

    /// `path`.
    pub fn path(&mut self) {
        self.state = State::PathRequested;
    }

    /// `setGoalAndPath`.
    pub fn set_goal_and_path(&mut self, goal: GoalRef) {
        self.set_goal(goal);
        self.path();
    }

    /// `setGoal(null)`. Whether there was a goal.
    pub fn clear_goal(&mut self) -> bool {
        let had = self.goal.is_some();
        self.set_goal_option(None);
        had
    }

    fn lost(&mut self) {
        self.state = State::None;
        self.goal = None;
    }
}

impl Process for CustomGoalProcess {
    fn name(&self) -> &'static str {
        "custom goal"
    }

    fn display(&self) -> String {
        format!("Custom Goal {}", self.goal.as_ref().map_or("null".to_owned(), |g| g.label()))
    }

    fn is_active(&self) -> bool {
        self.state != State::None
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY
    }

    fn on_tick(&mut self, core: &mut Core, game: &mut Game<'_>, calc_failed: bool, _safe_to_cancel: bool) -> PathingCommand {
        let with_goal = |goal: &Option<GoalRef>, command: fn(GoalRef) -> PathingCommand| match goal {
            Some(g) => command(g.clone()),
            None => PathingCommand::CancelAndClearGoal,
        };
        match self.state {
            State::GoalSet => with_goal(&self.goal, PathingCommand::CancelAndSetGoal),
            State::PathRequested => {
                // Return FORCE_REVALIDATE_GOAL_AND_PATH just once.
                let command = with_goal(&self.goal, PathingCommand::ForceRevalidateGoalAndPath);
                self.state = State::Executing;
                command
            }
            State::Executing => {
                if calc_failed {
                    let command = with_goal(&self.goal, PathingCommand::CancelAndSetGoal);
                    self.lost();
                    return command;
                }
                let arrived = match (&self.goal, core.player_feet(game)) {
                    (None, _) => true,
                    (Some(g), Some(feet)) => g.is_in_goal(feet) && g.is_in_goal(core.pathing.expected_segment_start()),
                    _ => false,
                };
                if arrived {
                    // We're there.
                    let command = with_goal(&self.goal, PathingCommand::CancelAndSetGoal);
                    self.lost();
                    if core.settings.flag("disconnectOnArrival") {
                        let _ = game.close_world();
                    }
                    if core.settings.flag("notificationOnPathComplete") {
                        core.notify("Pathing complete", false);
                    }
                    return command;
                }
                with_goal(&self.goal, PathingCommand::SetGoalAndPath)
            }
            State::None => PathingCommand::Defer,
        }
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {
        self.lost();
    }

    fn cancel(&mut self) {
        self.lost();
    }
}
