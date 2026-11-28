// The process of baritone.command.defaults.ExecutionControlCommands: while #pause holds, it is in control and asks for a pause. It
// sits above every ordinary process, so they are not even asked, and being temporary it leaves them as they are for #resume.
use super::baritone::Core;
use super::baritone_pathing::{PathingCommand, Process};
use super::Game;

#[derive(Default)]
pub struct PauseProcess {
    pub paused: bool,
}

impl Process for PauseProcess {
    fn name(&self) -> &'static str {
        "Pause/Resume Commands"
    }

    fn display(&self) -> String {
        self.name().to_owned()
    }

    fn is_active(&self) -> bool {
        self.paused
    }

    fn priority(&self) -> f64 {
        super::baritone_pathing::DEFAULT_PRIORITY + 1.0
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn on_tick(&mut self, core: &mut Core, _game: &mut Game<'_>, _calc_failed: bool, _safe_to_cancel: bool) -> PathingCommand {
        core.input.clear_all_keys();
        PathingCommand::RequestPause
    }

    fn on_lost_control(&mut self, _core: &mut Core, _game: &mut Game<'_>) {}

    fn cancel(&mut self) {}
}
