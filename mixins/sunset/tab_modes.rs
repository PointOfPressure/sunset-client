// HUD's Tab game modes: the tab list marks players in spectator and creative mode after their names.
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

pub static MARK: AtomicBool = AtomicBool::new(false);
// The setting the cached layout was built with: a change rebuilds it.
static BUILT: AtomicBool = AtomicBool::new(false);
thread_local! {
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

// The tag after a name in game mode `mode`, and its colour: spectators aqua, creative gold.
pub fn tag(mode: i32) -> Option<(&'static str, u32)> {
    match mode {
        super::SPECTATOR => Some((" [S]", 0x55FFFF)),
        1 => Some((" [C]", 0xFFAA00)),
        _ => None,
    }
}

// The marked name: the game's own (team colours, italic spectators), then the tag. None while marking is off, for a
// mode with no tag, or inside the game's own call made here.
pub fn marked(p: &super::TabPlayer, board: &super::Scoreboard) -> Option<super::Text> {
    let (tag, color) = tag(p.game_mode).filter(|_| MARK.load(Relaxed) && !BUSY.with(Cell::get))?;
    BUSY.with(|b| b.set(true));
    let name = super::name_for_display(p, board);
    BUSY.with(|b| b.set(false));
    Some(name.append(super::Text::colored(tag, color)))
}

// Before a layout: true when marking (`running`: Sunset is on) changed since the cached one was built.
pub fn changed(running: bool) -> bool {
    let now = running && MARK.load(Relaxed);
    BUILT.swap(now, Relaxed) != now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectators_and_creative_players_are_tagged() {
        assert_eq!(tag(super::super::SPECTATOR).map(|t| t.0), Some(" [S]"));
        assert_eq!(tag(1).map(|t| t.0), Some(" [C]"));
        assert_eq!(tag(0), None);
        assert_eq!(tag(2), None);
    }
}
