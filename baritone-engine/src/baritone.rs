// baritone.Baritone and BaritoneProvider: the instance. One `Baritone` per client, holding the shared
// state (`Core`) and the processes. The hooks in tick.rs, chat.rs, command.rs, list.rs, settings.rs and
// click.rs call the functions at the bottom; each takes the instance out of its cell for the length of
// the call, so nothing is ever borrowed twice (the first version borrowed its state while logging and
// brought the game down on the first command).
use super::baritone_aim::AimProcessor;
use super::baritone_command as commands;
use super::baritone_gworld::WorldCache;
use super::baritone_input::Input;
use super::baritone_p_build::BuilderProcess;
use super::baritone_p_elytra::ElytraProcess;
use super::baritone_p_explore::ExploreProcess;
use super::baritone_p_farm::FarmProcess;
use super::baritone_p_follow::FollowProcess;
use super::baritone_p_getto::GetToBlockProcess;
use super::baritone_p_goal::CustomGoalProcess;
use super::baritone_p_mine::MineProcess;
use super::baritone_pathing::{PathingCommand, Process};
use super::baritone_pathing_behavior::PathingBehavior;
use super::baritone_selection::SelectionManager;
use super::baritone_settings::{self as settings, Settings};
use super::baritone_utils::Pos;
use super::baritone_waypoints::{Tag, Waypoint, Waypoints};
use super::Screen;
use super::Game;
use horizon_client::{
    chat::ChatSource,
    text::{self, Text},
};
use std::cell::{Cell as FlagCell, RefCell};

pub const ID: &str = "baritone";
/// `SettingsUtil.SETTINGS_DEFAULT_NAME`, in the mod's data folder.
pub const SETTINGS_DEFAULT_NAME: &str = "settings.txt";
pub const VERSION: &str = "1.1.52";
pub const DESCRIPTION: &str =
    "A Rust port of Baritone for Horizon: #goto, #mine, #farm, #build, #explore, #follow, #tunnel, #elytra and the rest, with bridging, pillaring, parkour, doors and water bucket landings.";

/// Everything the processes and commands share.
pub struct Core {
    pub settings: Settings,
    /// What could not be read of the settings file, to say once there is a chat to say it in.
    setting_notes: Vec<String>,
    /// The chunks the server dropped that did not fit in the tick's packing time.
    pub unloading: Vec<(i32, i32, std::sync::Arc<horizon_client::chunk::Chunk>)>,
    /// `BaritoneToast`: the one toast, shown again with new text while it is up.
    toast: RefCell<Option<horizon_client::toasts::SystemToast>>,
    pub input: Input,
    pub pathing: PathingBehavior,
    /// LookBehavior's aim processor: the noise and the mouse steps of every aim.
    pub aim: AimProcessor,
    /// LookBehavior: how a rotation a movement asked for is shown, and what the server was last sent.
    pub look: super::baritone_look::LookBehavior,
    /// The click screen of `#click`.
    pub click: super::baritone_gui_click::GuiClick,
    /// Baritone's page in Options > Mods: the category open and how far its lists are scrolled.
    pub ui: super::baritone_screen::SettingsScreen,
    /// InventoryBehavior: the moves between the main inventory and the hotbar, and what a throwaway is.
    pub inventory: super::baritone_inventory::InventoryBehavior,
    /// The cached world of the world and dimension being played in.
    pub world_data: super::baritone_world_data::WorldProvider,
    /// The game's block states as the pathfinder's, kept between ticks.
    pub states: std::rc::Rc<super::baritone_state_factory::StateFactory>,
    /// What the process asking for a path overrides of the calculation context (PathingCommandContext).
    pub path_hooks: Option<std::rc::Rc<dyn super::baritone_calculation_context::CostHooks>>,
    pub cache: WorldCache,
    pub selections: SelectionManager,
    /// The first corner of a selection being made (#sel pos1).
    pub sel_pos1: Option<Pos>,
    /// What #sel copy took, and where the copy's corner lies from the place it was taken at.
    pub clipboard: Option<std::rc::Rc<dyn super::baritone_schematic::Schematic>>,
    pub clipboard_offset: Pos,
    /// Waypoints that were deleted or cleared, for #wp restore.
    pub deleted_waypoints: Vec<Waypoint>,
    pub waypoints: Waypoints,
    /// The world the waypoints in `waypoints` belong to.
    pub waypoints_world: Option<String>,
    pub throwaway: Vec<String>,
    was_dead: bool,
    block_names: Option<Vec<String>>,
}

impl Core {
    fn new(game: &Game<'_>) -> Self {
        let (settings, setting_notes) = Self::stored_settings(game);
        let waypoints = Waypoints::from_text("");
        let mut core = Self {
            settings,
            setting_notes,
            toast: RefCell::new(None),
            unloading: Vec::new(),
            input: Input::default(),
            pathing: PathingBehavior::new(),
            aim: AimProcessor::new(super::baritone_command::now_ms()),
            look: Default::default(),
            click: Default::default(),
            ui: Default::default(),
            inventory: Default::default(),
            world_data: Default::default(),
            states: std::rc::Rc::new(super::baritone_state_factory::StateFactory::new(game)),
            path_hooks: None,
            cache: WorldCache::default(),
            selections: SelectionManager::default(),
            sel_pos1: None,
            clipboard: None,
            clipboard_offset: [0, 0, 0],
            deleted_waypoints: Vec::new(),
            waypoints,
            waypoints_world: None,
            throwaway: Vec::new(),
            was_dead: false,
            block_names: None,
        };
        core.refresh();
        core
    }

    /// The values worked out from settings, after any of them changes.
    pub fn refresh(&mut self) {
        super::baritone_goals::set_cost_heuristic(self.settings.float("costHeuristic"));
        super::baritone_utils::set_censor(self.settings.flag("censorCoordinates"));
        self.throwaway = self.settings.list("acceptableThrowawayItems");
    }

    /// Set a setting and keep it for the next game.
    pub fn set_setting(&mut self, game: &mut Game<'_>, name: &str, value: &str) -> Result<(&'static settings::Def, String), String> {
        let (def, canonical) = self.settings.set(name, value)?;
        self.save_settings(game);
        self.refresh();
        Ok((def, canonical))
    }

    pub fn reset_setting(&mut self, game: &mut Game<'_>, def: &'static settings::Def) {
        self.settings.reset(def);
        self.save_settings(game);
        self.refresh();
    }

    fn settings_file(game: &Game<'_>) -> std::path::PathBuf {
        game.native_data_dir(ID).join(SETTINGS_DEFAULT_NAME)
    }

    /// The settings on disk, and what could not be read of them. Versions before 1.1.26 kept one file for each
    /// setting: those are read once, moved into `settings.txt` and removed.
    fn stored_settings(game: &Game<'_>) -> (Settings, Vec<String>) {
        match std::fs::read_to_string(Self::settings_file(game)) {
            Ok(text) => {
                let mut settings = Settings::load(|_| None);
                let notes = settings.apply_file_text(&text);
                (settings, notes)
            }
            Err(_) => {
                let settings = Settings::load(|name| game.native_setting(ID, name));
                if Self::write_settings(game, &settings).is_ok() {
                    for d in settings::DEFS {
                        let _ = std::fs::remove_file(game.native_data_dir(ID).join(d.name));
                    }
                }
                (settings, Vec::new())
            }
        }
    }

    fn write_settings(game: &Game<'_>, settings: &Settings) -> std::io::Result<()> {
        std::fs::create_dir_all(game.native_data_dir(ID))?;
        std::fs::write(Self::settings_file(game), settings.to_file_text())
    }

    /// `SettingsUtil.save`.
    pub fn save_settings(&self, game: &mut Game<'_>) {
        if Self::write_settings(game, &self.settings).is_err() {
            self.say_red(game, "Exception thrown while saving Baritone settings!");
        }
    }

    /// `#set load`: everything back to its default, then the file. The lines to say come back.
    pub fn load_settings(&mut self, game: &Game<'_>, file: &str) -> Vec<String> {
        self.settings = Settings::load(|_| None);
        let notes = match std::fs::read_to_string(game.native_data_dir(ID).join(file)) {
            Ok(text) => self.settings.apply_file_text(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec!["Baritone settings file not found, resetting.".to_owned()],
            Err(_) => vec!["Exception while reading Baritone settings, some settings may be reset to default values!".to_owned()],
        };
        self.refresh();
        notes
    }

    /// The waypoints file of the world being played in. The Java keeps them with each world; one global list would show a
    /// bed or a death of another world, and #home or #wp goto would walk to coordinates that mean nothing here.
    fn waypoints_file(game: &Game<'_>) -> std::path::PathBuf {
        game.native_data_dir(ID).join("cache").join(super::baritone_world_data::world_name(game)).join("waypoints.txt")
    }

    pub fn save_waypoints(&self, game: &Game<'_>) {
        let file = Self::waypoints_file(game);
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&file, self.waypoints.to_text());
    }

    /// Switch to the waypoints of the world now being played in. The one list older versions kept for every world goes to
    /// the first world opened after the change, once, so nothing is lost.
    pub fn follow_waypoints(&mut self, game: &Game<'_>) {
        let world = super::baritone_world_data::world_name(game);
        if self.waypoints_world.as_deref() == Some(world.as_str()) {
            return;
        }
        self.waypoints_world = Some(world);
        let file = Self::waypoints_file(game);
        let text = match std::fs::read_to_string(&file) {
            Ok(t) => t,
            Err(_) => {
                // A file marks the move as done: a setting that is not one of Baritone's own is not kept between runs.
                let legacy = game.native_setting(ID, "waypoints").unwrap_or_default();
                let marker = game.native_data_dir(ID).join("waypoints_moved");
                if !legacy.trim().is_empty() && !marker.exists() {
                    let _ = std::fs::create_dir_all(game.native_data_dir(ID));
                    let _ = std::fs::write(&marker, "The waypoints older versions kept for every world were moved to the first world opened after that.
");
                    legacy
                } else {
                    String::new()
                }
            }
        };
        self.waypoints = Waypoints::from_text(&text);
        self.deleted_waypoints.clear();
        self.save_waypoints(game);
    }

    /// Every block name the game knows, sorted, for commands to check a name against.
    pub fn known_blocks(&mut self, game: &Game<'_>) -> &Vec<String> {
        self.block_names.get_or_insert_with(|| {
            let mut names: Vec<String> = game.res.facts().states.iter().map(|s| s.name.to_string()).collect();
            names.sort();
            names.dedup();
            names
        })
    }

    /// `stone`, `minecraft:stone` -> `minecraft:stone`, if there is such a block.
    pub fn block_named(&mut self, game: &Game<'_>, word: &str) -> Option<String> {
        let full = if word.contains(':') { word.to_ascii_lowercase() } else { format!("minecraft:{}", word.to_ascii_lowercase()) };
        self.known_blocks(game).binary_search(&full).ok().map(|_| full)
    }

    // -----------------------------------------------------------------------------------
    // Chat: Baritone's look. `[` and `]` dark purple, `Baritone` light purple, the message gray,
    // a failure red.

    fn prefix_text(&self) -> Text {
        let short = self.settings.flag("shortBaritonePrefix");
        let name = if april_first_days() { "Baritoe" } else if short { "B" } else { "Baritone" };
        Text::colored("[", text::DARK_PURPLE)
            .append(Text::colored(name, text::LIGHT_PURPLE))
            .append(Text::colored("]", text::DARK_PURPLE))
            .append(" ")
    }

    /// `Helper.logToast`, `BaritoneToast.addOrUpdate`: a toast with the prefix for a title; one already up takes the new text.
    fn log_toast(&self, body: &Text) {
        let title = self.prefix_text().plain().trim().to_owned();
        let message = body.plain();
        let mut toast = self.toast.borrow_mut();
        match toast.as_mut() {
            Some(up) => up.reset(title, Some(message)),
            None => *toast = Some(horizon_client::toasts::SystemToast::new(title, Some(message))),
        }
    }

    pub fn say_text(&self, game: &mut Game<'_>, body: Text) {
        // logAsToast: the message is a toast and not a line of chat.
        if self.settings.flag("logAsToast") {
            self.log_toast(&body);
            return;
        }
        // useMessageTag: the messages are marked by a tag instead of the prefix (Horizon's chat has no tag to draw, so the
        // lines are plain).
        let line = if self.settings.flag("useMessageTag") { body } else { self.prefix_text().append(body) };
        let focused = game.screen == Screen::Chat;
        game.ui.chat.add(&game.gui.atlas, line, None, ChatSource::Client, game.ticks, focused);
    }

    /// `Helper.logNotification`: a desktop notification, when desktopNotifications is on.
    pub fn notify(&self, message: &str, error: bool) {
        if self.settings.flag("desktopNotifications") {
            super::baritone_notification::notify(message, error);
        }
    }

    pub fn say(&self, game: &mut Game<'_>, message: &str) {
        self.say_text(game, Text::colored(message, text::GRAY));
    }

    pub fn say_red(&self, game: &mut Game<'_>, message: &str) {
        self.say_text(game, Text::colored(message, text::RED));
    }

    /// A line with no prefix, for lists.
    pub fn say_plain(&self, game: &mut Game<'_>, line: Text) {
        let focused = game.screen == Screen::Chat;
        game.ui.chat.add(&game.gui.atlas, line, None, ChatSource::Client, game.ticks, focused);
    }
}

/// Every process the instance has.
pub struct Procs {
    pub goal: CustomGoalProcess,
    pub get_to_block: GetToBlockProcess,
    pub mine: MineProcess,
    pub follow: FollowProcess,
    pub explore: ExploreProcess,
    pub farm: FarmProcess,
    pub build: BuilderProcess,
    pub elytra: ElytraProcess,
    pub backfill: super::baritone_p_backfill::BackfillProcess,
    pub pauser: super::baritone_p_backfill::InventoryPauserProcess,
    pub pause: super::baritone_p_pause::PauseProcess,
}

impl Procs {
    fn new() -> Self {
        Self {
            goal: CustomGoalProcess::new(),
            get_to_block: GetToBlockProcess::new(),
            mine: MineProcess::new(),
            follow: FollowProcess::new(),
            explore: ExploreProcess::new(),
            farm: FarmProcess::new(),
            build: BuilderProcess::new(),
            elytra: ElytraProcess::new(),
            backfill: super::baritone_p_backfill::BackfillProcess::new(),
            pauser: super::baritone_p_backfill::InventoryPauserProcess::new(),
            pause: Default::default(),
        }
    }

    /// Every process, in the order they were registered.
    pub fn ordered(&mut self) -> Vec<&mut dyn Process> {
        vec![
            &mut self.goal,
            &mut self.get_to_block,
            &mut self.follow,
            &mut self.elytra,
            &mut self.farm,
            &mut self.explore,
            &mut self.mine,
            &mut self.build,
            &mut self.backfill,
            &mut self.pauser,
            &mut self.pause,
        ]
    }

    /// The process with this name.
    pub fn by_name(&mut self, name: &str) -> Option<&mut dyn Process> {
        self.ordered().into_iter().find(|p| p.name() == name)
    }

    pub fn cancel_all(&mut self) {
        for p in self.ordered() {
            p.cancel();
        }
    }

    /// Whether a process is running.
    pub fn any_active(&mut self) -> bool {
        self.ordered().iter().any(|p| p.is_active())
    }
}

pub struct Baritone {
    pub core: Core,
    pub procs: Procs,
    pub in_control: Option<&'static str>,
    /// What the process in control last asked for, for #proc.
    pub last_command: Option<String>,
    /// The active processes, the one that became active last first, then by priority (`active` of the control
    /// manager: ties are broken by which was added to the beginning of the list first).
    active_order: Vec<&'static str>,
}

impl Baritone {
    fn new(game: &Game<'_>) -> Self {
        Self { core: Core::new(game), procs: Procs::new(), in_control: None, last_command: None, active_order: Vec::new() }
    }

    /// `PathingBehavior.cancelEverything`: every process stops, and the path too unless that would drop the player mid-move, in which
    /// case the tick cancels it as soon as it is safe (no process is left to ask for anything else).
    pub fn cancel_everything(&mut self, game: &mut Game<'_>) {
        if self.core.pathing_safe_to_cancel() {
            self.core.drive(game, |p, ctx, _| p.secret_internal_segment_cancel(ctx));
        }
        self.cancel_processes();
    }

    /// `forceCancel`: the same whether it is safe or not, and the search as well. Also what leaving the world does.
    pub fn force_cancel(&mut self, game: &mut Game<'_>) {
        self.cancel_processes();
        self.core.pathing_reset(game);
    }

    /// `PathingControlManager.cancelEverything`.
    fn cancel_processes(&mut self) {
        self.in_control = None;
        self.last_command = None;
        self.active_order.clear();
        self.procs.cancel_all();
    }

    /// The name and description of the process in control.
    pub fn describe(&mut self) -> Option<(&'static str, f64, bool, String)> {
        let in_control = self.in_control?;
        self.procs.ordered().into_iter().find(|p| p.name() == in_control).map(|p| (p.name(), p.priority(), p.is_temporary(), p.display()))
    }

    fn tick(&mut self, game: &mut Game<'_>) {
        if game.player.is_none() || game.title.is_some() {
            self.click_close(game);
            if self.procs.any_active() || self.core.pathing_busy() {
                self.force_cancel(game);
            }
            return;
        }
        for note in std::mem::take(&mut self.core.setting_notes) {
            self.core.say(game, &note);
        }
        self.core.cache.note_chunks(game);
        self.core.cache_tick(game);
        self.click_tick(game);
        self.track_death(game);
        if game.screen == Screen::Dead {
            // The death screen: hold still, keep the plan.
            self.core.input.release(game);
            return;
        }
        self.elytra_goal_update(game);
        self.control_tick(game);
        // Nobody listened to the block changes this tick (the elytra flight takes them in its own tick).
        let _ = take_block_changes();
        for line in self.core.inventory_tick(game) {
            self.core.say(game, &line);
        }
    }

    /// The click screen: opened once the chat that typed `#click` has closed, and the block under the mouse while it is open. Any
    /// other screen takes its place, as a new screen replaces the Java's.
    fn click_tick(&mut self, game: &mut Game<'_>) {
        if game.screen == Screen::Playing && self.core.click.take_open() {
            game.capture(false);
            // A press on the playing screen grabs the mouse again, unless the window has no focus: so it has none as far as the game
            // can tell, until this closes.
            game.focused = false;
        } else if self.core.click.active {
            if game.screen == Screen::Playing && !game.captured {
                self.core.click.over = game.bar_mouse_block();
            } else {
                self.click_close(game);
            }
        }
        CLICK_OPEN.with(|open| open.set(self.core.click.active));
    }

    /// A press (or a drag) on Baritone's page in Options > Mods: 0 nothing came of it, 1 a button was pressed, 2 Done or close.
    fn page_click(&mut self, game: &mut Game<'_>, cursor: (f32, f32), size: (f32, f32), drag: bool) -> u8 {
        use super::baritone_screen::{self as screen, Action};
        let before = self.core.ui.category;
        let shown = self.core.ui.shown(&self.core.settings).clone();
        match self.core.ui.click(cursor, size, drag, &shown) {
            Action::None => u8::from(self.core.ui.category != before),
            Action::Back => 2,
            // A change waits in the screen's draft until Save Changes; nothing is applied or written before.
            Action::Set(name, value) => {
                let _ = self.core.ui.stage(&self.core.settings, name, &value);
                u8::from(!drag)
            }
            Action::Save => {
                for (name, value) in self.core.ui.take_changes() {
                    let _ = self.core.set_setting(game, name, &value);
                }
                self.core.ui.note = None;
                1
            }
            Action::Discard => {
                self.core.ui.discard();
                1
            }
            Action::Open(url) => {
                super::baritone_notification::open_uri(url);
                1
            }
            Action::Reset(category) => {
                for name in screen::reset_names(category) {
                    if let Some(def) = settings::find(name) {
                        self.core.ui.stage_reset(&self.core.settings, def);
                    }
                }
                1
            }
            Action::AddBlock(word) => {
                self.page_add_block(game, &word);
                1
            }
        }
    }

    /// Before each frame of the page: a row for every block added by name, and the blocks the block field could be.
    fn page_sync(&mut self, game: &mut Game<'_>) {
        use super::baritone_screen::{self as screen, Field, THROWAWAY};
        let listed = self.core.ui.shown(&self.core.settings).list(THROWAWAY);
        self.core.ui.set_custom(&listed);
        if self.core.ui.focus != Some(Field::Block) {
            self.core.ui.suggestions.clear();
            return;
        }
        let text = self.core.ui.block.clone();
        let mut found = screen::suggest(self.core.known_blocks(game), &text, 6);
        // A name typed out in full, or picked, needs no offers.
        let full = text.trim().to_lowercase().replace(' ', "_");
        if found.iter().any(|b| *b == full.trim_start_matches("minecraft:")) {
            found.clear();
        }
        self.core.ui.suggestions = found;
    }

    /// A block typed on the page joins the blocks Baritone may place, when the game has such a block. What came of it is told
    /// in the row (and in chat), as a refusal only the chat saw looked like nothing happening.
    fn page_add_block(&mut self, game: &mut Game<'_>, word: &str) {
        use super::baritone_screen::THROWAWAY;
        let Some(full) = self.core.block_named(game, word) else {
            let text = format!("There is no block called {word}.");
            self.core.say_red(game, &text);
            self.core.ui.note = Some((text, true));
            return;
        };
        let name = full.trim_start_matches("minecraft:").to_owned();
        let mut list = self.core.ui.shown(&self.core.settings).list(THROWAWAY);
        if list.iter().any(|b| b.trim_start_matches("minecraft:") == name) {
            let text = format!("{name} is already in the list.");
            self.core.say(game, &text);
            self.core.ui.note = Some((text, true));
            return;
        }
        list.push(name.clone());
        let _ = self.core.ui.stage(&self.core.settings, THROWAWAY, &list.join(","));
        self.core.ui.note = Some((format!("{name} added."), false));
    }

    /// A key on Baritone's page: true when a text field took it (Escape then does not leave the page).
    fn page_typed(&mut self, game: &mut Game<'_>, key: super::baritone_screen::Key, text: Option<&str>) -> bool {
        use super::baritone_screen::Action;
        // With no field up, every key but Escape is the page's and goes no further: Enter or Tab would press a widget under it.
        let Some(action) = self.core.ui.key(key, text) else {
            // Escape leaves the page as the back button does: not while changes wait, which shakes the bar instead.
            return if key == super::baritone_screen::Key::Escape { !matches!(self.core.ui.leave(), Action::Back) } else { true };
        };
        if let Action::AddBlock(word) = action {
            self.page_add_block(game, &word);
        }
        true
    }

    fn click_close(&mut self, game: &mut Game<'_>) {
        if self.core.click.active {
            self.core.click.close();
            // Under the control socket the window may lack focus and the game plays on regardless.
            game.focused = game.control.is_some() || game.window.as_ref().is_some_and(|w| w.has_focus());
            CLICK_OPEN.with(|open| open.set(false));
        }
    }

    /// GuiClick.mouseReleased.
    fn click_released(&mut self, game: &mut Game<'_>, button: super::baritone_gui_click::Button) {
        use super::baritone_gui_click::Released;
        match self.core.click.released(button) {
            Some(Released::Selection(start, end)) => {
                self.core.selections.remove_all();
                self.core.selections.add(start, end);
                let prefix = self.core.settings.text("prefix");
                let style = text::Style { click: Some(text::ClickEvent::RunCommand(format!("{} help sel", super::baritone_command::CLICK))), ..text::Style::color(text::WHITE) };
                self.core.say_text(game, Text::styled(format!("Selection made! For usage: {prefix}help sel"), style));
            }
            Some(Released::GoalAt(pos)) => self.procs.goal.set_goal_and_path(super::baritone_goals::rc(super::baritone_goals::GoalBlock(pos))),
            None => {}
        }
    }

    fn track_death(&mut self, game: &mut Game<'_>) {
        let dead = game.screen == Screen::Dead;
        if dead && !self.core.was_dead && self.core.settings.flag("doDeathWaypoints") {
            if let Some(pos) = super::baritone_gworld::player_feet(game) {
                let wp = Waypoint {
                    tag: Tag::Death,
                    name: "death".into(),
                    pos,
                    dimension: dimension_of(game),
                    created: super::baritone_command::now_ms(),
                };
                let created = wp.created;
                self.core.waypoints.add(wp);
                self.core.save_waypoints(game);
                // "Death position saved.", a link that walks back to it.
                let click = text::ClickEvent::RunCommand(format!("{} wp goto death @ {created}", super::baritone_command::CLICK));
                let message = super::baritone_command::clickable("Death position saved.", text::WHITE, click, Text::literal("Click to goto death"));
                self.core.say_text(game, message);
            }
        }
        self.core.was_dead = dead;
    }

    /// `CustomGoalProcess.setGoal`, its other half: a flight in progress heads for the new goal.
    fn elytra_goal_update(&mut self, game: &mut Game<'_>) {
        let Some(goal) = self.procs.goal.elytra_update.take() else { return };
        if !self.procs.elytra.is_active() {
            return;
        }
        let result = match &goal {
            Some(goal) => self.procs.elytra.path_to(&mut self.core, game, goal),
            None => Err("The goal must be a GoalXZ or GoalBlock".to_owned()),
        };
        if let Err(why) = result {
            self.core.say_red(game, &format!("Failed to update elytra goal because: {why}"));
        }
    }

    /// baritone.utils.PathingControlManager.executeProcesses and preTick/postTick: ask the active
    /// processes, highest priority first; the first with something to say is in control and the rest are
    /// told they lost it. The pathing behavior then does what that process asked.
    fn control_tick(&mut self, game: &mut Game<'_>) {
        let Baritone { core, procs, in_control, last_command, active_order } = self;
        let last_tick = *in_control;
        let mut priorities: Vec<(&'static str, f64, bool, bool)> = Vec::new();
        for p in procs.ordered() {
            p.refresh(core, game);
            priorities.push((p.name(), p.priority(), p.is_active(), p.is_temporary()));
        }
        // executeProcesses: a newly active process goes to the very front of the queue, a process that is not
        // active leaves it; the queue is then sorted by priority (a stable sort).
        for (name, _, active, _) in &priorities {
            if *active {
                if !active_order.contains(name) {
                    active_order.insert(0, name);
                }
            } else {
                active_order.retain(|n| n != name);
            }
        }
        let priority_of = |name: &str| priorities.iter().find(|p| p.0 == name).map_or(0.0, |p| p.1);
        active_order.sort_by(|a, b| priority_of(b).total_cmp(&priority_of(a)));
        let temporary_of = |name: &str| priorities.iter().find(|p| p.0 == name).is_some_and(|p| p.3);

        let calc_failed = core.pathing.calc_failed_last_tick();
        let safe = core.pathing_safe_to_cancel();
        let mut command: Option<PathingCommand> = None;
        let mut now: Option<&'static str> = None;
        let queue = active_order.clone();
        for (i, name) in queue.iter().enumerate() {
            let Some(p) = procs.by_name(name) else { continue };
            core.path_hooks = None;
            let cmd = p.on_tick(core, game, last_tick == Some(*name) && calc_failed, safe);
            if matches!(cmd, PathingCommand::Defer) {
                core.path_hooks = None;
                continue;
            }
            now = Some(*name);
            command = Some(cmd);
            // A temporary process leaves the others as they are; any other takes control from them.
            if !temporary_of(name) {
                for rest in &queue[i + 1..] {
                    if let Some(q) = procs.by_name(rest) {
                        q.on_lost_control(core, game);
                    }
                }
            }
            break;
        }
        *in_control = now;
        *last_command = command.as_ref().map(PathingCommand::describe);
        // Control changed from a real process to another one that wants to do something.
        let changed = now != last_tick && last_tick.is_some_and(|l| !temporary_of(l)) && command.as_ref().is_some_and(|c| !matches!(c, PathingCommand::RequestPause));
        let lines = core.path_tick(game, command.as_ref(), changed);
        for line in lines {
            core.say(game, &line);
        }
    }
}

/// `Helper.getPrefix`'s April fools: the first three days of April, by the clock of the machine.
fn april_first_days() -> bool {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let _ = era;
    month == 4 && day <= 3
}

fn dimension_of(game: &Game<'_>) -> String {
    game.native_player().map_or("overworld", |p| p.dimension).to_owned()
}

// ---------------------------------------------------------------------------------------
// The instance, and the functions the hooks call.

thread_local! {
    static INSTANCE: RefCell<Option<Box<Baritone>>> = const { RefCell::new(None) };
    static BUSY: FlagCell<bool> = const { FlagCell::new(false) };
}

/// Run `f` with the instance, taking it out of its cell for the length of the call. None when it is
/// already in use (the game called back into a hook from inside one).
pub fn with<R>(game: &mut Game<'_>, f: impl FnOnce(&mut Baritone, &mut Game<'_>) -> R) -> Option<R> {
    if BUSY.with(FlagCell::get) {
        return None;
    }
    BUSY.with(|b| b.set(true));
    let mut b = INSTANCE.with_borrow_mut(Option::take).unwrap_or_else(|| Box::new(Baritone::new(game)));
    let r = f(&mut b, game);
    INSTANCE.with_borrow_mut(|slot| *slot = Some(b));
    BUSY.with(|b| b.set(false));
    Some(r)
}

thread_local! {
    /// The blocks set since the last tick, as (x, y, z, solid): `BlockChangeEvent`. A thread-local of its own, because the game
    /// sets blocks from inside calls Baritone makes.
    static BLOCK_CHANGES: RefCell<Vec<(i32, i32, i32, bool)>> = const { RefCell::new(Vec::new()) };
    /// The chunks where a block the cached world keeps track of was set, for repackOnAnyBlockChange.
    static TRACKED_CHANGES: RefCell<Vec<(i32, i32)>> = const { RefCell::new(Vec::new()) };
}

/// `BlockChangeEvent`: a block was set.
pub fn block_changed(game: &Game<'_>, pos: [i32; 3], state: i32) {
    let id = horizon_world::registry::BlockStateId(state.max(0) as u32);
    let solid = !game.res.facts().is_air(id);
    let name = game.res.facts().get(id).name.clone();
    if super::baritone_cached_world::BLOCKS_TO_KEEP_TRACK_OF.contains(&name.strip_prefix("minecraft:").unwrap_or(&name)) {
        TRACKED_CHANGES.with_borrow_mut(|tracked| {
            if tracked.len() < 1 << 12 {
                tracked.push((pos[0] >> 4, pos[2] >> 4));
            }
        });
    }
    BLOCK_CHANGES.with_borrow_mut(|changes| {
        if changes.len() < 1 << 16 {
            changes.push((pos[0], pos[1], pos[2], solid));
        }
    });
}

/// The chunks with a tracked block set since the last call.
pub fn take_tracked_block_changes() -> Vec<(i32, i32)> {
    TRACKED_CHANGES.with_borrow_mut(std::mem::take)
}

/// The block changes since the last call.
pub fn take_block_changes() -> Vec<(i32, i32, i32, bool)> {
    BLOCK_CHANGES.with_borrow_mut(std::mem::take)
}

/// `BlockInteractEvent` USE, which WaypointBehavior listens to: a bed that is right-clicked becomes a bed waypoint, at its head.
pub fn block_interact_use(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    with(game, |b, game| {
        if !b.core.settings.flag("doBedWaypoints") {
            return;
        }
        let Some(pos) = game.bar_look_target() else { return };
        let state = b.core.states.state(game.world.block(pos[0], pos[1], pos[2]));
        if !state.bare_name().ends_with("_bed") {
            return;
        }
        let mut at = pos;
        if state.prop("part") == Some("foot") {
            let step = match state.prop("facing") {
                Some("north") => [0, 0, -1],
                Some("south") => [0, 0, 1],
                Some("west") => [-1, 0, 0],
                _ => [1, 0, 0],
            };
            at = [pos[0] + step[0], pos[1], pos[2] + step[2]];
        }
        let dimension = dimension_of(game);
        let exists = b.core.waypoints.all.iter().any(|w| w.tag == Tag::Bed && w.pos == at && w.dimension == dimension);
        if !exists {
            b.core.waypoints.add(Waypoint { tag: Tag::Bed, name: "bed".into(), pos: at, dimension, created: super::baritone_command::now_ms() });
            b.core.save_waypoints(game);
        }
    });
}

thread_local! {
    /// Whether the click screen is open: the hooks below run for every key and every grab of the mouse.
    static CLICK_OPEN: FlagCell<bool> = const { FlagCell::new(false) };
}

/// Escape on the click screen: it closes, and the mouse goes back to the game. True when it was open.
pub fn click_escape(game: &mut Game<'_>) -> bool {
    if !CLICK_OPEN.with(FlagCell::get) || game.screen != Screen::Playing {
        return false;
    }
    let closed = with(game, |b, game| b.click_close(game)).is_some();
    if closed {
        game.capture(true);
    }
    closed
}

/// GuiClick.mouseClicked: a button pressed, any button.
pub fn click_press(game: &mut Game<'_>) {
    if CLICK_OPEN.with(FlagCell::get) {
        with(game, |b, _| b.core.click.pressed());
    }
}

/// A key or a mouse button while playing. A press through the game's control socket comes this way too.
pub fn click_key(game: &mut Game<'_>, key: &str, pressed: bool) {
    use super::baritone_gui_click::Button;
    use winit::event::MouseButton;
    if !CLICK_OPEN.with(FlagCell::get) {
        return;
    }
    let button = if key == horizon_client::keybinds::mouse_name(MouseButton::Left) {
        Button::Left
    } else if key == horizon_client::keybinds::mouse_name(MouseButton::Right) {
        Button::Right
    } else {
        return;
    };
    with(game, |b, game| {
        if pressed {
            b.core.click.pressed();
        } else {
            b.click_released(game, button);
        }
    });
}

/// Whether the page open in Options is Baritone's own (Options > Mods > Baritone).
pub fn page_open(game: &Game<'_>) -> bool {
    matches!(game.ui.options.page(), horizon_client::options_ui::Page::NativeMod(i)
        if game.settings.mods.installed.get(i).is_some_and(|m| m.id == ID))
}

pub fn page_draw(game: &mut Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    with(game, |b, game| {
        b.page_sync(game);
        super::baritone_settings_ui::draw(b, game, batch);
    });
}

/// Characters typed on the page: a field takes them, and with none up they are dropped.
pub fn page_text(game: &mut Game<'_>, text: &str) {
    with(game, |b, game| b.page_typed(game, super::baritone_screen::Key::Other, Some(text)));
}

/// A key on the page: true when the page took it. Escape in a field only leaves the field.
pub fn page_key(game: &mut Game<'_>, key: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
    use winit::keyboard::KeyCode;
    use super::baritone_screen::Key;
    let key = match key {
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        _ => Key::Other,
    };
    with(game, |b, game| b.page_typed(game, key, text)).unwrap_or(false)
}

/// A press at the cursor (`drag`: a movement with the button held): see `Baritone::page_click`. A right click does nothing.
pub fn page_press(game: &mut Game<'_>, drag: bool, right: bool) -> u8 {
    if right {
        return 0;
    }
    let (cursor, size) = (game.cursor, game.gui_size());
    with(game, |b, game| b.page_click(game, cursor, size, drag)).unwrap_or(0)
}

/// The wheel over the page scrolls the rows.
pub fn page_wheel(game: &mut Game<'_>, amount: f32) {
    let size = game.gui_size();
    with(game, |b, _| {
        let l = super::baritone_screen::layout(size.0, size.1);
        b.core.ui.wheel(amount, &l);
    });
}

/// `onPlayerUpdate` PRE, just before the player's motion reads its rotation.
pub fn look_pre(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    with(game, |b, game| {
        let Core { look, aim, settings, .. } = &mut b.core;
        let aim_settings = super::baritone_aim::AimSettings::of(settings, f64::from(game.settings.sensitivity));
        let current = super::baritone_rotation::Rotation::new(game.yaw, game.pitch);
        if let Some(actual) = look.pre(current, aim, &aim_settings, settings) {
            game.yaw = actual.yaw;
            game.pitch = actual.pitch.clamp(-90.0, 90.0);
        }
    });
}

/// `onPlayerUpdate` POST, once the tick has been sent to the server.
pub fn look_post(game: &mut Game<'_>) {
    with(game, |b, game| {
        let fall_flying = game.player.as_ref().is_some_and(|p| p.fall_flying);
        let current = super::baritone_rotation::Rotation::new(game.yaw, game.pitch);
        if let Some(shown) = b.core.look.post(current, fall_flying, &b.core.settings) {
            game.yaw = shown.yaw;
            game.pitch = shown.pitch.clamp(-90.0, 90.0);
        }
    });
}

pub fn tick(game: &mut Game<'_>) {
    look_post(game);
    super::baritone_input::back(game);
    if !game.native_window() {
        return;
    }
    with(game, |b, game| {
        b.tick(game);
        publish(b, game);
        publish_tab(b, game);
    });
}

/// Every block's properties and the values each can take, from the block states the game knows.
fn block_properties(game: &Game<'_>) -> super::baritone_tab::BlockProperties {
    let registry = game.pack.assets.registry;
    let mut found: std::collections::HashMap<String, std::collections::BTreeMap<String, Vec<String>>> = std::collections::HashMap::new();
    for id in 0..registry.len() {
        let Some(state) = registry.state(horizon_world::registry::BlockStateId(id as u32)) else {
            continue;
        };
        let block = found.entry(state.name.clone()).or_default();
        for (name, value) in &state.properties {
            let values = block.entry(name.clone()).or_default();
            if !values.contains(value) {
                values.push(value.clone());
            }
        }
    }
    found.into_iter().map(|(name, properties)| (name, properties.into_iter().collect())).collect()
}

/// The toast hook, once a frame from the game's mod HUD: `ToastManager.extractRenderState` for the Baritone toast, in a free
/// 32 pixel slot, with the sound of a toast coming and going.
pub fn draw_toast(game: &mut Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    with(game, |b, game| {
        let mut cell = b.core.toast.borrow_mut();
        let Some(toast) = cell.as_mut() else { return };
        if toast.slot.is_none() {
            // The game's toasts take the lowest free slot; this one the highest, so that they rarely meet.
            let used = game.toast_slots_used();
            let Some(slot) = (0..Game::TOAST_SLOTS).rev().find(|s| used & (1 << s) == 0) else { return };
            toast.slot = Some(slot);
        }
        let (sound, finished) = toast.update(f64::from(game.settings.notification_display_time));
        if let Some(show) = sound {
            let event = if show { "ui.toast.in" } else { "ui.toast.out" };
            game.audio.play(&format!("minecraft:{event}"), None, 1.0, 1.0);
        }
        if finished {
            *cell = None;
            return;
        }
        if let Some(slot) = toast.slot {
            toast.draw(&game.gui.atlas, batch, 32.0 * slot as f32);
        }
    });
}

/// The beacon hook: the beam of a GoalXZ, drawn with the beacons.
pub fn goal_beams(game: &mut Game<'_>, camera: glam::DVec3, alpha: f64, out: &mut Vec<horizon_client::mesher::Vertex>) {
    if !game.native_window() {
        return;
    }
    with(game, |b, game| super::baritone_render::goal_beams(b, game, camera, alpha, out));
}

/// What TAB completes: the prefix, the waypoints, the block and mob names, and the function the chat
/// screen asks (registered once).
fn publish_tab(b: &mut Baritone, game: &mut Game<'_>) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| horizon_client::chat::baritone_suggest::set(super::baritone_tab::suggest));
    if super::baritone_tab::needs_names() {
        let blocks = b.core.known_blocks(game).clone();
        let mobs = game.res.mob_types().keys().cloned().collect();
        let items = game.res.items().order.iter().map(|id| id.to_string()).collect();
        let properties = block_properties(game);
        let settings_dir = game.native_data_dir(ID);
        let schematics_dir = super::baritone_cmd_build::schematics_dir(game);
        super::baritone_tab::publish(|d| {
            d.blocks = blocks;
            d.mobs = mobs;
            d.items = items;
            d.properties = properties;
            d.settings_dir = settings_dir;
            d.schematics_dir = schematics_dir;
        });
    }
    // Only while typing: nothing else reads these, and they are a lot to copy every tick.
    if game.screen == Screen::Chat {
        let mut waypoints: Vec<&Waypoint> = b.core.waypoints.all.iter().filter(|w| !w.name.is_empty()).collect();
        waypoints.sort_by_key(|w| std::cmp::Reverse(w.created));
        let waypoints = waypoints.into_iter().map(|w| w.name.clone()).collect();
        let players = super::baritone_gworld::targets(game).into_iter().filter(|t| t.kind == "minecraft:player").filter_map(|t| t.name).collect();
        let settings = b.core.settings.clone();
        super::baritone_tab::publish(|d| {
            d.prefix = settings.text("prefix");
            d.waypoints = waypoints;
            d.players = players;
            d.settings = settings;
        });
    }
}

/// The mod was turned off: stop and draw nothing.
pub fn stop(game: &mut Game<'_>) {
    look_post(game);
    if !game.native_window() {
        return;
    }
    with(game, |b, game| {
        b.click_close(game);
        if b.procs.any_active() || b.core.pathing_busy() {
            b.force_cancel(game);
        }
        // The cached world goes to disk when the game stops.
        let Baritone { core, .. } = b;
        core.world_data.save(game, &core.settings);
    });
    horizon_client::gizmos::baritone_lines::set(Vec::new(), Vec::new());
}

fn publish(b: &mut Baritone, game: &Game<'_>) {
    let mut out = super::baritone_render::Canvas::default();
    super::baritone_render::lines(b, game, &mut out);
    horizon_client::gizmos::baritone_lines::set(out.depth, out.top);
}

/// A line typed in chat: true when it was a Baritone command, so the server never sees it.
pub fn chat(game: &mut Game<'_>, message: &str) -> bool {
    with(game, |b, game| {
        let s = &b.core.settings;
        let prefix = s.text("prefix");
        let on = s.flag("chatControl") || (s.flag("chatControlAnyway") && s.flag("prefixControl"));
        if !on || !s.flag("prefixControl") || prefix.is_empty() {
            return false;
        }
        let Some(line) = message.strip_prefix(prefix.as_str()) else {
            return false;
        };
        // A doubled prefix is a way to say the character itself.
        if line.starts_with(prefix.as_str()) {
            return false;
        }
        game.ui.chat.add_recent(message);
        commands::run(b, game, line.trim());
        true
    })
    .unwrap_or(false)
}

/// `@baritone <line>` from a clicked chat line, and `/baritone <line>` typed with a slash.
pub fn command(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    let mut words = command.splitn(2, char::is_whitespace);
    let first = words.next().unwrap_or("");
    if first != commands::CLICK && !first.eq_ignore_ascii_case("baritone") && !first.eq_ignore_ascii_case("b") {
        return;
    }
    *ran = true;
    let line = words.next().unwrap_or("").trim().to_owned();
    with(game, |b, game| commands::run(b, game, &line));
}

// ---------------------------------------------------------------------------------------
// Options > Mods.

const PAGE_TOGGLES: &[(&str, &str)] = &[
    ("Place blocks (bridge, pillar)", "allowPlace"),
    ("Parkour jumps", "allowParkour"),
    ("Water bucket landings", "allowWaterBucketFall"),
    ("Draw the path", "renderPath"),
];

fn toggle_on(game: &Game<'_>, name: &str) -> bool {
    Core::stored_settings(game).0.flag(name)
}

pub fn page_labels(game: &Game<'_>) -> Vec<String> {
    PAGE_TOGGLES
        .iter()
        .map(|(label, name)| format!("{label}: {}", if toggle_on(game, name) { "ON" } else { "OFF" }))
        .collect()
}

pub fn page_click(game: &mut Game<'_>, button: usize) {
    let Some((_, name)) = PAGE_TOGGLES.get(button) else {
        return;
    };
    let now = if toggle_on(game, name) { "false" } else { "true" };
    // The running instance reads its own copy.
    with(game, |b, game| {
        let _ = b.core.set_setting(game, name, now);
    });
}
