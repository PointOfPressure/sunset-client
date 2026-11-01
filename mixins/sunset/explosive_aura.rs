// Shared scheduling for the explosive modules. One deferred inventory/action
// owner at a time; neither module sends combat actions from a render hook.
use super::{Game, Screen};
use super::sunset::{self, Module};
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc,Mutex};
struct View {session:Arc<Mutex<Option<Arc<horizon_protocol::connection::Counters>>>>,
    dim:super::Dim,pos:[i32;3],tick:u64}
static VIEWS:Mutex<[Option<View>;2]>=Mutex::new([None,None]);
static CRYSTAL_ON:AtomicBool=AtomicBool::new(false);
pub(super) fn show(game:&Game<'_>,anchor:bool,pos:[i32;3]) {
    if let Some(remote)=game.remote.as_ref() {
        VIEWS.lock().unwrap_or_else(|e|e.into_inner())[usize::from(anchor)]=Some(View{
            session:remote.session.counters.clone(),dim:game.dim,pos,tick:game.ticks});
    }
}
pub(super) fn clear_view(anchor:bool) {VIEWS.lock().unwrap_or_else(|e|e.into_inner())[usize::from(anchor)]=None;}
pub(super) fn draw(game:&Game<'_>,camera:&sunset::Camera,modules:&[Module],out:&mut impl sunset::Canvas,width:f32) {
    let cells=VIEWS.lock().unwrap_or_else(|e|e.into_inner()).each_ref().map(|view|view.as_ref()
        .filter(|view|view.dim==game.dim && game.ticks>=view.tick && game.ticks-view.tick<=4
            && game.remote.as_ref().is_some_and(|r|Arc::ptr_eq(&r.session.counters,&view.session)))
        .map(|view|view.pos));
    for (index,name) in ["Crystal Aura","Anchor Aura"].iter().enumerate() {
        let Some(module)=modules.iter().find(|m|m.name==*name&&m.on&&m.b("Render")) else {continue;};
        let Some(cell)=cells[index] else {continue;};
        let color=module.t("Render Color");
        let color=color.strip_prefix('#').or_else(||color.strip_prefix("0x"))
            .and_then(|text|u32::from_str_radix(text,16).ok()).or_else(||color.parse::<i32>().ok().map(|n|n as u32))
            .map(|argb|if argb<=0x00ff_ffff {argb|0xff00_0000}else{argb}).unwrap_or(0xffff_ffff);
        let lo=cell.map(f64::from);
        super::sunset_visual::draw_box(camera,out,&super::sunset_visual::Look::BOXED,lo,lo.map(|v|v+1.0),color,width);
    }
}
pub(super) use super::sunset_explosive_choice::Rating;
use super::sunset_explosive_choice::prefer_first;

pub(super) fn can_act(game: &Game<'_>) -> bool {
    game.native_window() && game.native_on(sunset::ID)
        && game.native_playing() && game.screen == Screen::Playing
        && game.remote.is_some() && !sunset::gui_open()
        && !super::sunset_melee_assist::acted(game)
        && !game.spectator() && !game.creative && game.selected < 9
        && game.using.is_none() && game.mining.is_none()
        && super::remote::sunset_remote::breaking(game).is_none()
        && !super::sunset_player::gap_eating()
        && !game.left && !game.right
        && super::sunset_util::inventory_idle(game)
        && game.player.as_ref().is_some_and(|p| p.health.health().is_finite() && p.health.health() > 0.0)
        && sunset::CRIT_STAGE.load(Ordering::Relaxed) == 0
        && super::sunset_combat_control::owner().is_none_or(|o| o == "Crystal Aura" || o == "Anchor Aura")
        && !sunset::with(|s| ["Blink", "Nuker", "Scaffold"]
            .iter().any(|name| s.is_on(name)))
}

pub(super) fn tick(game: &mut Game<'_>, modules: &[Module], ticks: u64) {
    super::sunset_prediction::tick(game);
    let crystal = modules.iter().find(|m| m.name == "Crystal Aura" && m.on);
    let anchor = modules.iter().find(|m| m.name == "Anchor Aura" && m.on);
    if CRYSTAL_ON.swap(crystal.is_some(),Ordering::Relaxed) && crystal.is_none() {
        super::sunset_crystal_aura::disabled(game);
    }
    if anchor.is_none() { super::sunset_anchor_aura::reset(); }
    if !can_act(game) {
        if crystal.is_some() {
            if super::sunset_melee_assist::acted(game) {super::sunset_crystal_aura::suspend();}
            else {super::sunset_crystal_aura::paused(game);}
        }
        if anchor.is_some() { super::sunset_anchor_aura::suspend(); }
        return;
    }
    // Prepare each at most once, then claim only the chosen action. A pending
    // server acknowledgement is not a new inventory/rotation owner.
    let c = super::sunset_crystal_aura::prepare(game, crystal, ticks).filter(|r| r.valid());
    let a = anchor.and_then(|m| super::sunset_anchor_aura::prepare(game, m, ticks)).filter(|r| r.valid());
    let crystal_first = match (c, a) {
        (Some(c), Some(a)) => prefer_first(c, a, ticks % 2 == 0),
        (Some(_), None) => true,
        _ => false,
    };
    if crystal_first {
        if let Some(m) = crystal { super::sunset_crystal_aura::execute(game, m, ticks); }
    } else if a.is_some() {
        if let Some(m) = anchor { super::sunset_anchor_aura::execute(game, m, ticks); }
    }
}
