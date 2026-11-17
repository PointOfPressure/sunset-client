// The player's own fishing hook, for Auto Fish: (bobbing, a fish is biting, hooked something).
use super::State;
use crate::game::Game;

pub(crate) fn state(game: &Game<'_>) -> Option<(bool, bool, bool)> {
    game.hook.as_ref().map(|h| {
        (
            matches!(h.state, State::Bobbing),
            h.nibble > 0,
            matches!(h.state, State::Hooked(_)),
        )
    })
}
