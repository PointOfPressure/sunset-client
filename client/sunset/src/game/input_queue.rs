// The click queues Sunset adds its own clicks to. The remote game's queue (InteractionInput) keeps its counts private
// on this game build, so Sunset can't tell its clicks from the player's there: counts reads none and cancel drops the
// whole queue (a click the player made in the same tick goes too). The local game's queues (Game::attack_clicks and
// use_clicks) are read and trimmed exactly.
use horizon_client::input_queue::ClickQueue;
use horizon_client::interaction_input::InteractionInput;

pub fn counts(_input: &InteractionInput) -> [u32; 3] {
    [0; 3]
}

pub fn cancel(input: &mut InteractionInput, owned: [u32; 3]) {
    if owned != [0; 3] {
        let _ = input.take_queued();
    }
}

// Takes `n` clicks off the end of a queue.
pub fn trim(queue: &mut ClickQueue, n: u32) {
    let left = queue.count().saturating_sub(n);
    queue.clear();
    for _ in 0..left {
        queue.press();
    }
}
