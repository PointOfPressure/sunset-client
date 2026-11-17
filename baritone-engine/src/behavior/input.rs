// baritone.utils.InputOverrideHandler: the inputs Baritone holds down (`setInputForceState`), applied to the
// game once a tick, so that letting go of all of them is one call and nothing is left pressed when it
// stops. The movement keys go into the game's key set; the two clicks hold the attack button and use the
// held item. With a menu or the chat open the game ignores its key set, so the keys are also published for
// the hook that lets Baritone keep walking with a screen open (see `through`).
use super::baritone_movement_state::Input as In;
use super::{Game, Screen};
use horizon_client::chat::ChatScreen;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use winit::keyboard::KeyCode;

thread_local! {
    /// What Baritone holds down: forward, back, left, right, sprint, jump, sneak.
    static HELD: Cell<[bool; 7]> = const { Cell::new([false; 7]) };
    /// Baritone is clicking: BlockBreakHelper and BlockPlaceHelper do not care what screen is open.
    static CLICKING: Cell<bool> = const { Cell::new(false) };
    /// The path, as the motion of this tick has to know it.
    static PATH: Cell<PathState> = const { Cell::new(PathState { has_path: false, sprint: None, allow_swimming: false }) };
    /// The player's auto jump option was on and is off for this tick's motion.
    static AUTO_JUMP: Cell<bool> = const { Cell::new(false) };
    /// `InputOverrideHandler.inControl`: the player's motion reads Baritone's inputs and not the keyboard.
    static IN_CONTROL: Cell<bool> = const { Cell::new(false) };
    /// Which of the keys the player was holding, while Baritone's stand in for them.
    static PLAYER_KEYS: Cell<Option<[bool; 7]>> = const { Cell::new(None) };
    /// The screen the tick turned into the playing one, to give back.
    static BORROWED: Cell<Option<Screen>> = const { Cell::new(None) };
    /// The chat box, kept aside while the chat is the borrowed screen: the game drops the box of a chat screen when the screen is
    /// not the chat (`tick_in_bed_chat`), and it is not the chat while the tick plays it.
    static CHAT_BOX: RefCell<Option<ChatScreen>> = const { RefCell::new(None) };
}

/// What the Java's redirects in the player's `aiStep` read from `PathingBehavior`.
#[derive(Clone, Copy, Default)]
pub struct PathState {
    /// `current != null`: the option `autoJump` is off for the motion (`PathingBehavior.onPlayerUpdate`).
    pub has_path: bool,
    /// `onPlayerSprintState`: `Some` while a path is walked, when the keyboard is not the player's, and then
    /// whether that path sprints.
    pub sprint: Option<bool>,
    /// The setting `allowSwimming`.
    pub allow_swimming: bool,
}

const KEYS: [(In, KeyCode); 7] = [
    (In::MoveForward, KeyCode::KeyW),
    (In::MoveBack, KeyCode::KeyS),
    (In::MoveLeft, KeyCode::KeyA),
    (In::MoveRight, KeyCode::KeyD),
    (In::Sprint, KeyCode::ControlLeft),
    (In::Jump, KeyCode::Space),
    (In::Sneak, KeyCode::ShiftLeft),
];

/// Just before the motion reads the keys: `player.input = new PlayerMovementInput(this)`. While Baritone is in control the motion sees
/// its inputs and not the keyboard, on any screen, as the Java Baritone walks with any GUI open. The keys are the player's again
/// right after, so what the player holds is never lost.
pub fn through(game: &mut Game<'_>) {
    let path = PATH.with(Cell::get);
    if path.has_path && game.settings.auto_jump {
        game.settings.auto_jump = false;
        AUTO_JUMP.with(|a| a.set(true));
    }
    let playing = game.screen == Screen::Playing;
    let wanted = IN_CONTROL.with(Cell::get) || (!playing && CLICKING.with(Cell::get));
    if PLAYER_KEYS.with(Cell::get).is_some() || game.screen == Screen::Dead || !wanted {
        return;
    }
    let held = HELD.with(Cell::get);
    let mut player = [false; 7];
    for (i, (input, key)) in KEYS.iter().enumerate() {
        player[i] = game.keys.remove(key);
        // MixinClientPlayerEntity.isKeyDown: the one key `PlayerMovementInput` does not cover.
        let down = if *input == In::Sprint { path.sprint.unwrap_or(player[i] && playing) } else { held[i] };
        if down {
            game.keys.insert(*key);
        }
    }
    if path.sprint.is_some() {
        if let Some(body) = game.player.as_mut() {
            // MixinClientPlayerEntity.isAllowFlying and tryToStartFallFlying: a jump of a path neither toggles
            // creative flight nor opens an elytra. The motion asks for a new press of jump for both.
            body.was_jumping = true;
            // MixinEntity.allowSwimStart: the swim state latches at the surface, not once the eye is under.
            if path.allow_swimming && body.sprinting && body.fluids.was_touching_water {
                body.swimming = true;
            }
        }
    }
    PLAYER_KEYS.with(|p| p.set(Some(player)));
    if !playing {
        if game.screen == Screen::Chat {
            CHAT_BOX.with(|b| *b.borrow_mut() = game.ui.chat_screen.take());
        }
        BORROWED.with(|b| b.set(Some(game.screen)));
        game.screen = Screen::Playing;
    }
}

/// After the motion has read them: the keys are the player's again, and the screen is the screen.
pub fn back(game: &mut Game<'_>) {
    if AUTO_JUMP.with(Cell::take) {
        game.settings.auto_jump = true;
    }
    let Some(player) = PLAYER_KEYS.with(Cell::take) else {
        return;
    };
    for (i, (_, key)) in KEYS.iter().enumerate() {
        if player[i] {
            game.keys.insert(*key);
        } else {
            game.keys.remove(key);
        }
    }
    // The chat box goes back first: the screen comes back only with it, as a chat with no box is a screen no key can type in or close.
    if let Some(chat) = CHAT_BOX.with(|b| b.borrow_mut().take()) {
        if game.ui.chat_screen.is_none() {
            game.ui.chat_screen = Some(chat);
        }
    }
    if let Some(screen) = BORROWED.with(Cell::take) {
        if game.screen == Screen::Playing && (screen != Screen::Chat || game.ui.chat_screen.is_some()) {
            game.screen = screen;
        }
    }
}

/// Once a tick: the chat screen is only ever opened together with its box, so a chat screen without one is a screen that
/// swallows every key (even F5) and cannot be closed. Whatever left the game like that, it is the player's again.
pub fn mend(game: &mut Game<'_>) {
    if game.screen == Screen::Chat && game.ui.chat_screen.is_none() {
        game.screen = Screen::Playing;
        game.capture(true);
        eprintln!("[Baritone] the chat screen was open without its box: closed it");
    }
}

#[derive(Default)]
pub struct Input {
    forced: HashMap<In, bool>,
    /// Set from the behavior each tick.
    pub path: PathState,
    /// Whether Baritone itself is holding the left mouse button. The game's own left button is the player's too, so it is
    /// only ever let go of when this says Baritone pressed it.
    attacking: bool,
    /// `rightClickSpeed`: the ticks between two right clicks, set from the settings each tick.
    pub right_click_speed: i32,
    /// `blockBreakSpeed`: the ticks from one broken block to the next attack, set from the settings each tick.
    pub block_break_speed: i32,
    /// BlockBreakHelper.breakDelayTimer.
    break_delay: i32,
    /// The block under the crosshair while attacking: when it changes, the block was broken.
    last_target: Option<[i32; 3]>,
    place_cooldown: i32,
}

impl Input {
    // ---- InputOverrideHandler

    pub fn is_input_forced_down(&self, input: In) -> bool {
        self.forced.get(&input).copied().unwrap_or(false)
    }

    pub fn set_input_force_state(&mut self, input: In, forced: bool) {
        self.forced.insert(input, forced);
    }

    /// `clearAllKeys`: nothing forced any more (applied at the next `apply`).
    pub fn clear_all_keys(&mut self) {
        self.forced.clear();
    }

    /// Put what is forced into the game: once a tick, after everything has set its inputs.
    pub fn apply(&mut self, game: &mut Game<'_>) {
        if self.is_input_forced_down(In::ClickLeft) {
            self.set_input_force_state(In::ClickRight, false);
        }
        let mut bits = [false; 7];
        for (i, (input, _)) in KEYS.iter().enumerate() {
            bits[i] = self.is_input_forced_down(*input);
        }
        HELD.with(|h| h.set(bits));
        CLICKING.with(|c| c.set(self.is_input_forced_down(In::ClickLeft) || self.is_input_forced_down(In::ClickRight)));
        PATH.with(|p| p.set(self.path));
        // inControl: the sprint alone does not count.
        let moving = [In::MoveForward, In::MoveBack, In::MoveLeft, In::MoveRight, In::Sneak, In::Jump].iter().any(|i| self.is_input_forced_down(*i));
        IN_CONTROL.with(|c| c.set(moving || self.path.sprint.is_some()));
        let mut attack = self.is_input_forced_down(In::ClickLeft);
        if self.break_delay > 0 {
            self.break_delay -= 1;
            attack = false;
        }
        if self.attacking {
            // The block in front changed while attacking: it broke, so the next break waits (BASE_BREAK_DELAY is 1 tick).
            let target = game.bar_look_target();
            if self.last_target.is_some() && target != self.last_target {
                self.break_delay = (self.block_break_speed - 1).max(0);
                // The game's own delay would otherwise slow every setting under the vanilla speed.
                game.destroy_delay = 0;
            }
            self.last_target = target;
        } else {
            self.last_target = None;
        }
        // A screen that opens lets go of the game's button; Baritone's click does not go through the screen.
        if attack != self.attacking || (attack && !game.left) {
            game.bar_attack(attack);
            self.attacking = attack;
        }
        if self.place_cooldown > 0 {
            self.place_cooldown -= 1;
        }
        if self.is_input_forced_down(In::ClickRight) && self.place_cooldown <= 0 {
            game.bar_use();
            self.place_cooldown = self.right_click_speed;
        }
    }

    /// Let go of everything held, at once.
    pub fn release(&mut self, game: &mut Game<'_>) {
        self.path = PathState::default();
        self.clear_all_keys();
        self.apply(game);
    }
}
