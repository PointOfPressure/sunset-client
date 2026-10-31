// Same-module bridge: native item use retains cooldown and prediction rules.
pub(crate) fn use_main(game: &mut crate::game::Game<'_>) -> bool {
    game.remote_use_item(horizon_client::first_person::Hand::Main)
}
