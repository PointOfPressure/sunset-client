// The click queues Sunset adds its own clicks to. The remote game's queue (InteractionInput) keeps its counts private
// on this game build, so Sunset can't tell its clicks from the player's there: counts reads none and cancel drops the
// whole queue (a click the player made in the same tick goes too). The local game's queues (Game::attack_clicks and
// use_clicks) are read and trimmed exactly.
use horizon_client::input_queue::ClickQueue;
use horizon_client::interaction_input::{Button, InteractionInput, InteractionWorld};

pub fn counts(_input: &InteractionInput) -> [u32; 3] {
    [0; 3]
}

// The pinned SDK has no count accessor for the remote queue. Inspect the next batch without a
// game world, requeue the visited clicks, then restore the unvisited backlog and refusal state.
pub fn pending_buttons(input: &mut InteractionInput) -> [bool; 2] {
    let mut queued = input.take_queued();
    let mut inspection = PendingButtons { input, seen: [false; 2] };
    queued.tick(&mut inspection, false, false);
    let seen = inspection.seen;
    input.merge_queued(queued);
    seen
}

struct PendingButtons<'a> {
    input: &'a mut InteractionInput,
    seen: [bool; 2],
}

impl InteractionWorld for PendingButtons<'_> {
    fn pick_hit_result(&mut self) {}
    fn release_hit_result(&mut self) {}
    fn using_item(&self) -> bool { false }
    fn release_using(&mut self) {}
    fn start_attack(&mut self) -> bool {
        self.input.press(Button::Attack);
        self.seen[0] = true;
        false
    }
    fn start_use(&mut self, _: bool) {
        self.input.press(Button::Use);
        self.seen[1] = true;
    }
    fn pick(&mut self) { self.input.press(Button::Pick); }
    fn use_delay(&self) -> u32 { 0 }
    fn continue_attack(&mut self, _: bool) {}
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
