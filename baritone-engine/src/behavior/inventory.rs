// baritone.behavior.InventoryBehavior: which hotbar slot holds what a move needs, and (allowInventory) moving it there
// from the main inventory. Nine slots are used directly; a throwaway block or a pickaxe in the main inventory is swapped
// into hotbar slot 8 or 0 while a path is walked, and one needed at once goes to slot 7. A swap is a SWAP click on the
// player's own inventory menu, the click a hotbar key makes, so the server sees what a player could have done.
use super::baritone_gworld::{bare, hotbar_slot};
use super::baritone_settings::Settings;
use super::baritone_toolset::{calculate_speed_vs_block, ToolSettings};
use super::Game;
use horizon_client::items::ItemStack;
use horizon_client::menu::Input;
use horizon_world::registry::BlockStateId;

/// The state of InventoryPauserProcess, kept here because the behavior asks it for a still player and the process
/// answers it from its own tick.
#[derive(Default)]
pub struct Pauser {
    pub pause_requested_last_tick: bool,
    pub safe_to_cancel_last_tick: bool,
    pub ticks_of_stationary: i32,
}

/// `BuilderProcess.placeAt(x, y, z, current)`: the state the build wants at a position, or None when it has no say or wants air.
pub type PlaceAt = std::rc::Rc<dyn Fn(i32, i32, i32, &super::baritone_block_state::BlockState) -> Option<std::rc::Rc<super::baritone_block_state::BlockState>>>;

#[derive(Default)]
pub struct InventoryBehavior {
    /// Set by the builder while it has a schematic, for the block it would place at a position.
    pub place_at: Option<PlaceAt>,
    ticks_since_last_move: i32,
    /// Not everything asks every tick, so the request is remembered while the player comes to a halt.
    last_requested: Option<(usize, usize)>,
    pauser: Pauser,
    /// `logDebug` lines, for the caller to print when chatDebug is on.
    debug: Vec<String>,
}

impl InventoryBehavior {
    pub fn pauser(&mut self) -> &mut Pauser {
        &mut self.pauser
    }

    /// A `logDebug` line for the next `take_debug`.
    pub fn log_debug(&mut self, line: &str) {
        self.debug.push(line.to_owned());
    }

    pub fn take_debug(&mut self) -> Vec<String> {
        std::mem::take(&mut self.debug)
    }

    /// `InventoryPauserProcess.stationaryForInventoryMove`: ask for the path to hold still; true once it has.
    pub fn stationary_for_inventory_move(&mut self) -> bool {
        self.pauser.pause_requested_last_tick = true;
        self.pauser.safe_to_cancel_last_tick && self.pauser.ticks_of_stationary > 1
    }

    /// `onTick`: with allowInventory, keep a throwaway block in slot 8 and the best pickaxe in slot 0 while pathing.
    pub fn tick(&mut self, game: &mut Game<'_>, settings: &Settings, acceptable: &[String], pathing: bool, tools: &ToolSettings) {
        if !settings.flag("allowInventory") {
            return;
        }
        if container_open(game) {
            // A crafting table or a chest or something is open.
            return;
        }
        self.ticks_since_last_move += 1;
        if pathing {
            let throwaway = first_valid_throwaway(game, acceptable);
            if throwaway.is_some_and(|i| i >= 9) {
                // There are none on the hotbar, but there are some in the main inventory.
                self.request_swap_with_hotbar(game, settings, throwaway.unwrap_or(9), 8);
            }
            if let Some(pick) = best_pickaxe_against_stone(game, tools).filter(|&i| i >= 9) {
                self.request_swap_with_hotbar(game, settings, pick, 0);
            }
        }
        if let Some((in_inventory, in_hotbar)) = self.last_requested {
            self.debug.push(format!("Remembering to move {in_inventory} {in_hotbar} from a previous tick"));
            self.request_swap_with_hotbar(game, settings, in_inventory, in_hotbar);
        }
    }

    /// `attemptToPutOnHotbar`: a hotbar slot not in use for something else gets the stack of the main inventory slot. False
    /// when the swap was asked for and could not be made yet.
    pub fn attempt_to_put_on_hotbar(&mut self, game: &mut Game<'_>, settings: &Settings, in_main_inventory: usize, disallowed_hotbar: &dyn Fn(usize) -> bool) -> bool {
        match temp_hotbar_slot(game, disallowed_hotbar) {
            Some(destination) => self.request_swap_with_hotbar(game, settings, in_main_inventory, destination),
            None => true,
        }
    }

    /// `requestSwapWithHotBar`.
    fn request_swap_with_hotbar(&mut self, game: &mut Game<'_>, settings: &Settings, in_inventory: usize, in_hotbar: usize) -> bool {
        self.last_requested = Some((in_inventory, in_hotbar));
        let between = settings.int("ticksBetweenInventoryMoves") as i32;
        if self.ticks_since_last_move < between {
            self.debug.push(format!("Inventory move requested but delaying {} {between}", self.ticks_since_last_move));
            return false;
        }
        if settings.flag("inventoryMoveOnlyIfStationary") && !self.stationary_for_inventory_move() {
            self.debug.push("Inventory move requested but delaying until stationary".to_owned());
            return false;
        }
        swap_click(game, in_inventory, in_hotbar);
        self.ticks_since_last_move = 0;
        self.last_requested = None;
        true
    }

    /// `hasGenericThrowaway`: whether any acceptable throwaway item is at hand.
    pub fn has_generic_throwaway(&mut self, game: &mut Game<'_>, settings: &Settings, acceptable: &[String]) -> bool {
        let allow = settings.flag("allowInventory");
        acceptable.iter().any(|item| self.throwaway(game, settings, false, &|s| bare(&s.item) == bare(item), allow))
    }

    /// `selectThrowawayForLocation`: a block to place. `placed` is the block the builder wants at the position, if it has a
    /// say: the stack of that block first, then the acceptable items in the order the setting lists them.
    pub fn select_throwaway_for_location(&mut self, game: &mut Game<'_>, settings: &Settings, acceptable: &[String], select: bool, placed: Option<&str>) -> bool {
        let allow = settings.flag("allowInventory");
        if let Some(block) = placed {
            if self.throwaway(game, settings, select, &|s| bare(&s.item) == bare(block), allow) {
                return true;
            }
        }
        acceptable.iter().any(|item| self.throwaway(game, settings, select, &|s| bare(&s.item) == bare(item), allow))
    }

    /// `throwaway`: hold a stack `desired` accepts. The hotbar first; then the off hand (which would win a right click, so
    /// the main hand is given something that does not right click); then, if the inventory may be used, the main inventory,
    /// whose stack is swapped to slot 7.
    pub fn throwaway(&mut self, game: &mut Game<'_>, settings: &Settings, select: bool, desired: &dyn Fn(&ItemStack) -> bool, allow_inventory: bool) -> bool {
        if let Some(slot) = (0..9).find(|&i| stack_at(game, i).is_some_and(|s| desired(&s))) {
            if select {
                game.selected = slot;
            }
            return true;
        }
        if game.offhand.as_ref().is_some_and(|s| s.count > 0 && desired(s)) {
            // Main hand takes precedence over off hand, so something that does not right click goes in the main hand: an
            // empty hand or a pickaxe.
            let slot = (0..9).find(|&i| stack_at(game, i).is_none_or(|s| s.item.ends_with("_pickaxe")));
            if let Some(slot) = slot {
                if select {
                    game.selected = slot;
                }
                return true;
            }
        }
        if allow_inventory {
            if let Some(i) = (9..36).find(|&i| stack_at(game, i).is_some_and(|s| desired(&s))) {
                if select {
                    self.request_swap_with_hotbar(game, settings, i, 7);
                    game.selected = 7;
                }
                return true;
            }
        }
        false
    }
}

fn stack_at(game: &Game<'_>, slot: usize) -> Option<ItemStack> {
    game.inventory.get(slot).and_then(Option::as_ref).filter(|s| s.count > 0).cloned()
}

/// Whether a crafting table, a chest or any other menu than the player's own is open.
fn container_open(game: &Game<'_>) -> bool {
    game.container.is_some() || game.station.is_some() || game.remote.as_ref().is_some_and(|_| game.remote_menu().0 != 0)
}

/// `firstValidThrowaway`: the first inventory slot, hotbar first, holding an acceptable item.
fn first_valid_throwaway(game: &Game<'_>, acceptable: &[String]) -> Option<usize> {
    (0..36).find(|&i| stack_at(game, i).is_some_and(|s| acceptable.iter().any(|a| bare(a) == bare(&s.item))))
}

/// `bestToolAgainst(Blocks.STONE, PickaxeItem.class)`: the pickaxe that breaks stone fastest, skipping the ones itemSaver
/// protects.
fn best_pickaxe_against_stone(game: &Game<'_>, tools: &ToolSettings) -> Option<usize> {
    let stone = game.res.facts().states.iter().position(|s| &*s.name == "minecraft:stone")?;
    let mut best: Option<usize> = None;
    let mut best_speed = -1.0;
    for i in 0..36 {
        let Some(stack) = stack_at(game, i) else { continue };
        if let Some(threshold) = tools.item_saver {
            let max = game.res.items().get(&stack.item).map_or(0, |info| info.max_damage as i32);
            if stack.damage as i32 + threshold >= max && max > 1 {
                continue;
            }
        }
        if stack.item.ends_with("_pickaxe") {
            // Takes the enchantments into account.
            let speed = calculate_speed_vs_block(game, Some(&stack), BlockStateId(stone as u32));
            if speed > best_speed {
                best_speed = speed;
                best = Some(i);
            }
        }
    }
    best
}

/// `getTempHotbarSlot`: slots 0 and 8 are for the pickaxe and the throwaway, so one of 1 to 7 that is empty and not
/// disallowed, or any that is not disallowed, at random.
fn temp_hotbar_slot(game: &Game<'_>, disallowed: &dyn Fn(usize) -> bool) -> Option<usize> {
    let mut candidates: Vec<usize> = (1..8).filter(|&i| stack_at(game, i).is_none() && !disallowed(i)).collect();
    if candidates.is_empty() {
        candidates = (1..8).filter(|&i| !disallowed(i)).collect();
    }
    if candidates.is_empty() {
        return None;
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize);
    Some(candidates[nanos % candidates.len()])
}

/// `windowClick(containerId, slot, hotbar, ClickType.SWAP)` on the player's inventory menu: the main inventory slot and the
/// hotbar slot trade stacks. The slot numbers are the game's own (hotbar 0 to 8, main inventory 9 to 35).
fn swap_click(game: &mut Game<'_>, in_inventory: usize, in_hotbar: usize) -> bool {
    let ops = [(Some(in_inventory), in_hotbar as i32, Input::Swap)];
    if game.remote.is_some() {
        game.remote_menu_ops(&ops)
    } else {
        game.menu_ops(&ops)
    }
}

/// `windowClick(containerId, slot, 0, ClickType.PICKUP)` on the player's inventory menu: the stack of the slot is picked up,
/// or the carried one put down (the game's own slot numbers: hotbar 0 to 8, main inventory 9 to 35, armor 500 to 503).
pub fn pickup_click(game: &mut Game<'_>, slot: usize) -> bool {
    let ops = [(Some(slot), 0, Input::Pickup)];
    if game.remote.is_some() {
        game.remote_menu_ops(&ops)
    } else {
        game.menu_ops(&ops)
    }
}

pub fn bucket_slot(game: &Game<'_>) -> Option<usize> {
    hotbar_slot(game, |n| n == "minecraft:water_bucket")
}

pub fn empty_bucket_slot(game: &Game<'_>) -> Option<usize> {
    hotbar_slot(game, |n| n == "minecraft:bucket")
}

/// Hold the stack in `slot`.
pub fn select(game: &mut Game<'_>, slot: usize) {
    game.selected = slot;
}
