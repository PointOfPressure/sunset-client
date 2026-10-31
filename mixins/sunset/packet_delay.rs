// Original bounded packet timing. Only four ordinary player-movement variants
// can enter the outgoing queue. Native Session::send handles framing/crypto.
use super::{Game,Screen};
use super::sunset::{self,Canvas,Category,Module};
use super::sunset_packet_policy::{self as policy,Queue};
use horizon_protocol::packets::play::{Clientbound as C,Serverbound as S};
use std::{any::Any,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::Instant};

const LAG:&str="Fake Lag";
const KB:&str="Knockback Delay";
const MOVEMENT_BYTES:usize=40;
const MAX_IMPULSE_BYTES:usize=32_768;
static REPLAYING:AtomicBool=AtomicBool::new(false);

pub(super) fn defaults()->Vec<Module> {
    vec![
        Module::new(LAG,Category::Combat,"Delays only ordinary outgoing player movement. Actions flush movement first; chat, keepalive and ping are never held. Dynamic follows your aim and approach.")
            .pick("Mode",&["Dynamic","Always"])
            .num("Delay (ms)",100.0,10.0,1000.0,10.0)
            .num("Range",6.0,3.0,12.0,0.5)
            .bool("Show server position",true)
            .bool("Anti bot",true).bool("Ignore teams",true),
        Module::new(KB,Category::Combat,"Applies selected self velocity and explosion packets after a short bounded delay. Explosion visuals and sound share their packet's delay. Other entities and network control pass immediately.")
            .num("Chance (%)",40.0,0.0,100.0,5.0)
            .num("Air delay min (ms)",50.0,0.0,500.0,10.0)
            .num("Air delay max (ms)",100.0,0.0,500.0,10.0)
            .num("Ground delay min (ms)",200.0,0.0,1000.0,10.0)
            .num("Ground delay max (ms)",250.0,0.0,1000.0,10.0)
            .bool("Not in water",false).bool("Only near enemies",true)
            .bool("Explosion packets",true).bool("Anti bot",true).bool("Ignore teams",true),
    ]
}

#[derive(Clone,Copy,Default)]
struct Config { lag:bool,lag_ms:u64,show:bool,kb:bool,chance:f64,air:[f64;2],ground:[f64;2],water:bool,fights:bool,explosions:bool }
type Session=(i32,super::Dim,Arc<dyn Any+Send+Sync>);
struct State {
    session:Option<Session>,
    epoch:Instant,
    config:Config,
    moves:Queue<S>,
    impulses:Queue<C>,
    server_position:Option<[f64;3]>,
    rng:u64,
    replay_errors:u64,
}
impl Default for State {
    fn default()->Self {Self {session:None,epoch:Instant::now(),config:Config::default(),
        moves:Queue::new(64,4096,1000),impulses:Queue::new(16,262_144,1000),server_position:None,
        rng:0x9e3779b97f4a7c15,replay_errors:0}}
}
static STATE:Mutex<Option<State>>=Mutex::new(None);
fn state<R>(f:impl FnOnce(&mut State)->R)->R {
    let mut g=STATE.lock().unwrap_or_else(|e|e.into_inner());f(g.get_or_insert_with(State::default))
}
fn millis(s:&State)->u64 {s.epoch.elapsed().as_millis().min(u128::from(u64::MAX)) as u64}
fn random(s:&mut State)->u64 {
    s.rng=s.rng.wrapping_add(0x9e3779b97f4a7c15);
    let mut n=s.rng;n=(n^(n>>30)).wrapping_mul(0xbf58476d1ce4e5b9);n=(n^(n>>27)).wrapping_mul(0x94d049bb133111eb);n^(n>>31)
}
fn session(game:&Game<'_>)->Option<Session> {
    let r=game.remote.as_ref()?;Some((r.entity_id,game.dim,r.session.counters.clone()))
}
fn same(game:&Game<'_>,old:&Session)->bool {
    game.remote.as_ref().is_some_and(|r|r.entity_id==old.0&&game.dim==old.1
        && Arc::as_ptr(&r.session.counters) as *const ()==Arc::as_ptr(&old.2) as *const ())
}
fn ensure_session(game:&Game<'_>)->bool {
    state(|s| {
        if !s.session.as_ref().is_some_and(|old|same(game,old)) {
            // Old-coordinate moves and old-world impulses cannot be replayed
            // into a replacement connection or dimension.
            s.moves.clear();s.impulses.clear();s.server_position=None;s.config=Config::default();
            s.session=session(game);
            s.rng^=millis(s).wrapping_add(s.session.as_ref().map_or(0,|old|old.0 as u64));
        }
        s.session.is_some()
    })
}
fn live(game:&Game<'_>)->bool {
    game.native_window()&&game.native_on(sunset::ID)&&game.remote.is_some()
        && !game.spectator()&&game.player.as_ref().is_some_and(|p|p.health.health()>0.0)
        && game.screen==Screen::Playing&&!sunset::gui_open()
}
fn movement(packet:&S)->bool {
    matches!(packet,S::MovePlayerPos(_)|S::MovePlayerPosRot(_)|S::MovePlayerRot(_)|S::MovePlayerStatus(_))
}
fn barrier(packet:&S)->bool {
    !movement(packet)&&!matches!(packet,S::KeepAlive(_)|S::Pong(_)|S::PingRequest(_)
        |S::Chat(_)|S::ChatAck(_)|S::ChatCommand(_)|S::ChatCommandSigned(_)|S::ChatSessionUpdate(_)
        |S::ChunkBatchReceived(_)|S::ClientInformation(_)|S::ClientTickEnd(_)|S::PlayerInput(_)
        |S::ConfigurationAcknowledged(_)|S::ResourcePack(_)|S::CookieResponse(_)|S::PlayerLoaded(_))
}
fn position(packet:&S)->Option<[f64;3]> {
    match packet {S::MovePlayerPos(p)=>Some([p.x,p.y,p.z]),S::MovePlayerPosRot(p)=>Some([p.x,p.y,p.z]),_=>None}
}
fn valid_movement(packet:&S)->bool {
    let pos=position(packet).is_none_or(|p|p.iter().all(|n|n.is_finite()&&n.abs()<=30_000_000.0));
    let look=match packet {S::MovePlayerPosRot(p)=>p.yaw.is_finite()&&p.pitch.is_finite(),
        S::MovePlayerRot(p)=>p.yaw.is_finite()&&p.pitch.is_finite(),_=>true};pos&&look
}
fn track(s:&mut State,packet:&S) {if let Some(p)=position(packet) {s.server_position=Some(p);}}

fn send_moves(game:&Game<'_>,packets:Vec<S>) {
    for packet in packets {
        if !state(|s|s.session.as_ref().is_some_and(|old|same(game,old))) {break;}
        if let Some(remote)=&game.remote {super::sunset_combat_wire::forwarded(game,&packet);remote.session.send(packet.clone());state(|s|track(s,&packet));}
    }
}
fn flush_moves(game:&Game<'_>) {
    ensure_session(game);
    let packets=state(|s| {let now=millis(s);s.moves.drain(now,true)});
    send_moves(game,packets);
}

struct Replay;
impl Drop for Replay {fn drop(&mut self){REPLAYING.store(false,Ordering::Release);}}
fn apply(game:&mut Game<'_>,packets:Vec<C>) {
    if REPLAYING.swap(true,Ordering::AcqRel) {return;}
    let _guard=Replay;
    for packet in packets {
        if !state(|s|s.session.as_ref().is_some_and(|old|same(game,old))) {break;}
        if super::remote::sunset_packet_replay::apply(game,packet).is_err() {
            state(|s| {s.replay_errors=s.replay_errors.saturating_add(1);s.impulses.clear();});break;
        }
    }
}

// Flush on disable, menu entry and death while the same connection
// and dimension still exist. Call before master-off stops native tick callbacks.
pub(super) fn flush(game:&mut Game<'_>) {
    ensure_session(game);flush_moves(game);
    let packets=state(|s| {let now=millis(s);s.impulses.drain(now,true)});apply(game,packets);
    state(|s|s.config=Config::default());
}
// No Game remains after disconnect: explicitly discard, never replay later.
pub(super) fn reset() {state(|s|*s=State::default());REPLAYING.store(false,Ordering::Release);}
pub(super) fn replaying()->bool {REPLAYING.load(Ordering::Acquire)}

pub(super) fn disabled(game:&mut Game<'_>,owner:&str) {
    ensure_session(game);
    if owner==LAG {flush_moves(game);state(|s|s.config.lag=false);}
    if owner==KB {
        let packets=state(|s|{s.config.kb=false;let now=millis(s);s.impulses.drain(now,true)});apply(game,packets);
    }
}

pub(super) fn correction(game:&mut Game<'_>) {
    ensure_session(game);
    // The server has already teleported: never send pre-teleport coordinates.
    // Earlier incoming impulses apply before native correction overwrites its
    // position/velocity, preserving server receive order without a late push.
    let packets=state(|s| {s.moves.clear();s.server_position=None;s.config.lag=false;
        let now=millis(s);s.impulses.drain(now,true)});
    apply(game,packets);
}

// Invoke after every existing outgoing transform, on the actual final packet.
// true means this packet was queued; its normal send must be cancelled.
pub(super) fn outgoing(game:&Game<'_>,packet:&S)->bool {
    if !ensure_session(game) {return false;}
    if !movement(packet) {
        // Gameplay actions are an ordering barrier. Network control and chat
        // bypass both the queue and the barrier; ClientTickEnd is sent every
        // tick and must not silently collapse all configured movement delay.
        if barrier(packet) {flush_moves(game);}return false;
    }
    let capture=live(game)&&valid_movement(packet)
        && super::sunset_combat_control::owner().is_none()
        && !sunset::with(|s|s.is_on("Blink"));
    let config=state(|s|s.config);
    if !capture||!config.lag {flush_moves(game);state(|s|track(s,packet));return false;}
    let overflow=state(|s|s.moves.needs_flush(millis(s),MOVEMENT_BYTES));
    if overflow {flush_moves(game);}
    state(|s| {let now=millis(s);s.moves.push(packet.clone(),now,config.lag_ms,MOVEMENT_BYTES).is_ok()})
}

fn pushes_self(game:&Game<'_>,packet:&C,explosions:bool)->bool {
    match packet {
        C::SetEntityMotion(p)=>game.remote.as_ref().is_some_and(|r|r.entity_id==p.id)
            &&p.velocity.iter().all(|n|n.is_finite()),
        C::Explode(p)=>explosions&&p.knockback.is_some_and(|v|v.iter().all(|n|n.is_finite())),
        _=>false,
    }
}
fn self_motion_event(game:&Game<'_>,packet:&C)->bool {
    match packet {
        C::SetEntityMotion(p)=>game.remote.as_ref().is_some_and(|r|r.entity_id==p.id),
        C::Explode(p)=>p.knockback.is_some(),
        _=>false,
    }
}

// Invoke at HEAD of handle_packet. true cancels the normal native handling;
// replay goes through the same handler under a guard, never through networking.
pub(super) fn incoming(game:&mut Game<'_>,packet:&C)->bool {
    if REPLAYING.load(Ordering::Acquire) {return false;}
    if !ensure_session(game) {return false;}
    match packet {
        C::Login(_)|C::Respawn(_)=>{reset();return false;}
        C::PlayerPosition(_)=>{correction(game);return false;}
        _=>{},
    }
    if !live(game) {flush(game);return false;}
    let config=state(|s|s.config);
    if !config.kb||!pushes_self(game,packet,config.explosions) {
        // A newer self-motion update that is not eligible must not overtake
        // an older held one (for example after an explosion-option change).
        if self_motion_event(game,packet) {
            let packets=state(|s|{let now=millis(s);s.impulses.drain(now,true)});apply(game,packets);
        }
        return false;
    }
    // Encoding checks the exact complete native payload. Reject large explosion
    // payloads before cloning them into a bounded queue.
    let bytes=packet.encode().len();
    let waiting=state(|s|!s.impulses.is_empty());
    let should_delay=waiting||(config.fights&&(!config.water||game.player.as_ref().is_some_and(|p|!p.fluids.was_touching_water))
        &&state(|s|policy::probability(config.chance,random(s))));
    if bytes>MAX_IMPULSE_BYTES||!should_delay {
        if waiting {let packets=state(|s|{let now=millis(s);s.impulses.drain(now,true)});apply(game,packets);}
        return false;
    }
    if state(|s|s.impulses.needs_flush(millis(s),bytes)) {
        let packets=state(|s|{let now=millis(s);s.impulses.drain(now,true)});apply(game,packets);
    }
    let ground=game.player.as_ref().is_some_and(|p|p.ground.on_ground);
    state(|s| {
        let wait=if !s.impulses.is_empty() {0} else {
            let bounds=if ground {config.ground}else{config.air};policy::uniform_between(bounds[0],bounds[1],random(s))
        };
        if wait==0&&s.impulses.is_empty() {return false;}
        let now=millis(s);s.impulses.push(packet.clone(),now,wait,bytes).is_ok()
    })
}

fn enemy(game:&Game<'_>,m:&Module,dynamic:bool,range:f64)->bool {
    if game.mobs.len()>512 {return false;}
    let Some(eye)=game.eye_position() else{return false;};
    let Some(p)=game.player.as_ref() else{return false;};
    let friends=sunset::with(|s|s.friends.clone());
    let yaw=f64::from(game.yaw).to_radians();let pitch=f64::from(game.pitch).to_radians();
    let look=[-yaw.sin()*pitch.cos(),-pitch.sin(),yaw.cos()*pitch.cos()];
    let feet=[p.position.x,p.position.y,p.position.z];
    let velocity=[p.velocity.x,p.velocity.y,p.velocity.z];
    for target in &game.mobs {
        if target.removed||!target.alive()||target.kind.id!="minecraft:player"
            ||game.remote.as_ref().is_some_and(|r|r.entity_id as u32==target.id)
            ||super::sunset_util::spared(game,m,target.id,true) {continue;}
        let Some(name)=game.remote_player_name(target.id) else{continue;};
        if friends.iter().any(|f|f.eq_ignore_ascii_case(name)) {continue;}
        let at=[target.position.x,target.position.y,target.position.z];
        let distance=(0..3).map(|i|(feet[i]-at[i]).powi(2)).sum::<f64>().sqrt();
        if !distance.is_finite()||distance>range {continue;}
        if !dynamic {return true;}
        let bounds=target.bounds();let(lo,hi)=(bounds.min_position(),bounds.max_position());
        let center=[(lo.x+hi.x)/2.0,(lo.y+hi.y)/2.0,(lo.z+hi.z)/2.0];
        if policy::approaching(eye,look,velocity,center,distance,range) {return true;}
    }
    false
}

pub(super) fn tick(game:&mut Game<'_>,modules:&[Module]) {
    ensure_session(game);
    if !live(game) {flush(game);return;}
    let lag=modules.iter().find(|m|m.name==LAG&&m.on);
    let kb=modules.iter().find(|m|m.name==KB&&m.on);
    let blocked=sunset::with(|s|s.is_on("Blink"))||super::sunset_combat_control::owner().is_some();
    let lag_active=!blocked&&lag.is_some_and(|m|m.p("Mode")=="Always"||enemy(game,m,true,m.n("Range")));
    let config=Config {
        lag:lag_active,lag_ms:lag.map_or(100,|m|policy::bounded(m.n("Delay (ms)"),10,1000)),
        show:lag.is_some_and(|m|m.b("Show server position")),kb:kb.is_some(),
        chance:kb.map_or(0.0,|m|m.n("Chance (%)")),
        air:kb.map_or([0.0;2],|m|[m.n("Air delay min (ms)"),m.n("Air delay max (ms)")]),
        ground:kb.map_or([0.0;2],|m|[m.n("Ground delay min (ms)"),m.n("Ground delay max (ms)")]),
        water:kb.is_some_and(|m|m.b("Not in water")),
        fights:kb.is_some_and(|m|!m.b("Only near enemies")||enemy(game,m,false,6.0)),
        explosions:kb.is_some_and(|m|m.b("Explosion packets")),
    };
    state(|s|s.config=config);
    let moves=state(|s|{let now=millis(s);s.moves.drain(now,!config.lag)});send_moves(game,moves);
    let packets=state(|s|{let now=millis(s);s.impulses.drain(now,!config.kb)});apply(game,packets);
}

pub(super) fn draw(game:&Game<'_>,camera:&sunset::Camera,out:&mut impl Canvas,line_width:f32) {
    let position=state(|s|(s.config.show&&!s.moves.is_empty()).then_some(s.server_position).flatten());
    let (Some(position),Some(player))=(position,game.player.as_ref()) else{return;};
    let half=f64::from(player.dimensions.width)/2.0;
    let lo=[position[0]-half,position[1],position[2]-half];
    let hi=[position[0]+half,position[1]+f64::from(player.dimensions.height),position[2]+half];
    super::sunset_visual::draw_box(camera,out,&super::sunset_visual::Look::BOXED,lo,hi,0xFFFF_5555,line_width);
}

pub(super) fn debug()->serde_json::Value {
    state(|s|{let now=millis(s);serde_json::json!({"replaying":replaying(),"movement_only":true,"movement_queued":s.moves.len(),
        "movement_bytes":s.moves.bytes(),"movement_oldest_ms":s.moves.oldest_age(now),
        "impulses_queued":s.impulses.len(),"impulse_bytes":s.impulses.bytes(),"impulse_oldest_ms":s.impulses.oldest_age(now),
        "movement_held":s.moves.accepted,"movement_released":s.moves.released,"movement_discarded":s.moves.dropped,
        "impulses_held":s.impulses.accepted,"impulses_released":s.impulses.released,"impulses_discarded":s.impulses.dropped,
        "replay_errors":s.replay_errors,"server_position":s.server_position})})
}
