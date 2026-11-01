"""Compile production accepted inventory boundary checks, without duplicate counts.

Real pinned C packet codec, production Swap/Lease and exact production packet/discard
function bodies. Native Game effects/stack catalog remain explicit adapters.
"""
from pathlib import Path
import ast,argparse,hashlib,json,re,subprocess,tempfile
REPO=Path(__file__).resolve().parents[1]
SOURCE=REPO/'mixins/sunset'
def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('engine',type=Path);parser.add_argument('--dependency-dir',required=True,type=Path);parser.add_argument('--out',type=Path);args=parser.parse_args()
    assert (args.engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    text=(SOURCE/'combat_inventory.rs').read_text(encoding='utf-8')
    adapter=(REPO/'tools/inventory_retirement_tests.py').read_text(encoding='utf-8')
    tree=ast.parse(adapter)
    # The code assignment is a replace-chain over one raw literal.
    assignment=next(n for n in ast.walk(tree)if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='code'for t in n.targets))
    code=next(n.value for n in ast.walk(assignment.value)if isinstance(n,ast.Constant)and isinstance(n.value,str)and n.value.startswith('\n#!'))
    for match in list(re.finditer(r'#\[test\]fn \w+\(',code)):
        signature=match.group();code=code.replace(function(code,signature),'')
    functions='\n'.join(function(text,s)for s in ['fn retire_job(','fn poll_retiring(','fn retire_matching(','fn discard_scene(','pub(super) fn packet('])
    code=code.replace('HELPERS',functions)
    code=code.replace('use policy::{Status,Swap,Failure};','use policy::{Status,Swap,Failure};\nuse horizon_protocol::packets::play::{self,Clientbound as C};')
    code=code.replace('holds:Vec<usize>,closes:usize,','holds:Vec<usize>,closes:usize,')
    code=code.replace('fn hold(game:', '''impl Game<'_>{fn to_stack(&self,stack:&Option<horizon_protocol::item::Stack>)->Option<inventory::Stack>{
     stack.as_ref().map(|s|inventory::Stack(if s.item==1{"totem"}else{"other"}))}}
mod sunset{pub static CANCELS:std::sync::Mutex<Vec<&'static str>>=std::sync::Mutex::new(vec![]);pub fn cancel_turns(owner:&'static str){CANCELS.lock().unwrap().push(owner);}}
fn hold(game:''')
    code=code.replace('struct Stack(&\'static str);','pub(super) struct Stack(pub(super) &\'static str);')
    code=code.replace('phase:Phase,swap:Option<Swap<Stack>>,','phase:Phase,swap:Option<Swap<Stack>>,menu:(i32,String),wire:[Option<usize>;2],revision:Option<i32>,')
    code=code.replace('failures:Vec<(&\'static str,Failure<Stack>)>,next:u64}', 'failures:Vec<(&\'static str,Failure<Stack>)>,next:u64,queued_use:Option<(&\'static str,Instant)>}')
    code=code.replace('static SWAP_CONFIRMED:AtomicU64=', 'static SWAP_LAST_ACK:AtomicU64=AtomicU64::new(0);static SWAP_CONFIRMED:AtomicU64=')
    code=code.replace('phase,swap,slots:[9,504],before,began:Instant::now()', 'phase,swap,menu:(0,"inventory".into()),wire:[Some(9),Some(45)],revision:Some(5),slots:[9,504],before,began:Instant::now()')
    tests=r'''
 fn wire(packet:C)->C{let body=packet.encode();let mut r=horizon_protocol::buf::Reader::new(&body);let id=r.varint().unwrap();C::decode(id,r.rest()).unwrap()}
 fn spawn()->play::SpawnInfo{play::SpawnInfo{dimension_type:0,dimension:"minecraft:overworld".into(),seed:1,game_type:0,previous_game_type:Some(0),debug:false,flat:true,last_death:None,portal_cooldown:0,sea_level:63}}
 fn respawn()->C{wire(C::Respawn(play::Respawn{spawn:spawn(),keep:3}))}
 fn login()->C{wire(C::Login(play::Login{player_id:7,hardcore:false,levels:vec!["minecraft:overworld".into()],max_players:2,chunk_radius:2,simulation_distance:2,reduced_debug_info:false,show_death_screen:true,limited_crafting:false,spawn:spawn(),online_mode:false,enforces_secure_chat:false}))}
 fn disconnect()->C{wire(C::Disconnect(horizon_protocol::packets::configuration::Disconnect{reason:horizon_protocol::text::literal("test boundary")}))}
 fn accept(game:&Game<'_>,packet_value:C){packet(game,&wire(packet_value));}
 fn slot(game:&Game<'_>,at:i16,item:Option<horizon_protocol::item::Stack>){accept(game,C::ContainerSetSlot(play::ContainerSetSlot{container:0,state:6,slot:at,item}));}
 #[test]fn same_identity_respawn_drops_retirement_without_old_hand_return(){let mut g=setup(Phase::Await);retire_matching(&mut g,OWNER);g.selected=4;g.holds.clear();packet(&g,&respawn());state(|s|{assert!(s.retiring.is_none());assert!(s.job.is_none());assert!(s.connection.is_none());});assert_eq!(g.selected,4);assert!(g.holds.is_empty());assert_eq!(g.closes,0);assert!(!poll_retiring(&g));}
 #[test]fn accepted_login_drops_live_unsent_job_without_scene_restore(){let mut g=setup(Phase::Reaction);g.holds.clear();packet(&g,&login());state(|s|assert!(s.job.is_none()));assert!(g.holds.is_empty());assert_eq!(g.selected,8);assert_eq!(g.closes,0);}
 #[test]fn disconnect_drops_held_old_ticket_and_queued_use(){let mut g=setup(Phase::Await);retire_matching(&mut g,OWNER);state(|s|s.queued_use=Some(("Key Throw",Instant::now())));sunset::CANCELS.lock().unwrap().clear();packet(&g,&disconnect());assert_eq!(control::owner(),None);assert_eq!(*sunset::CANCELS.lock().unwrap(),["Key Throw"]);state(|s|assert!(s.queued_use.is_none()));}
 #[test]fn boundary_cannot_release_replacement_owner_ticket(){let mut g=setup(Phase::Await);retire_matching(&mut g,OWNER);control::replace("Kill Aura");let ticket=control::ticket("Kill Aura");packet(&g,&respawn());assert_eq!(control::ticket("Kill Aura"),ticket);}
 #[test]fn boundary_cannot_release_new_ticket_of_same_owner(){let mut g=setup(Phase::Await);retire_matching(&mut g,OWNER);control::replace(OWNER);let ticket=control::ticket(OWNER);packet(&g,&respawn());assert_eq!(control::ticket(OWNER),ticket);}
 #[test]fn late_old_slot_payload_cannot_credit_abandoned_receipt(){let mut g=setup(Phase::Await);retire_matching(&mut g,OWNER);slot(&g,9,None);state(|s|assert_eq!(s.retiring.as_ref().unwrap().swap.as_ref().unwrap().confirmations(),1));packet(&g,&respawn());slot(&g,45,Some(horizon_protocol::item::Stack::new(1,1)));assert_eq!(SWAP_CONFIRMED.load(Ordering::Relaxed),0);state(|s|assert!(s.retiring.is_none()));}
 #[test]fn normal_packets_keep_real_two_slot_receipt_until_complete(){let mut g=setup(Phase::Await);retire_matching(&mut g,OWNER);slot(&g,9,None);assert!(poll_retiring(&g));state(|s|assert!(s.retiring.is_some()));slot(&g,45,Some(horizon_protocol::item::Stack::new(1,1)));poll_retiring(&g);assert_eq!(SWAP_CONFIRMED.load(Ordering::Relaxed),1);}
 #[test]fn observer_accepts_boundary_before_old_session_match(){let mut g=setup(Phase::Await);retire_matching(&mut g,OWNER);g.scene=2;packet(&g,&respawn());state(|s|{assert!(s.retiring.is_none());assert!(s.connection.is_none());});}
'''
    index=code.rfind('}');code=code[:index]+tests+code[index:]
    code=code.replace('POLICY',(REPO/'mixins/sunset/inventory_policy.rs').as_posix()).replace('LEASE',(REPO/'mixins/sunset/combat_lease.rs').as_posix())
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-inventory-scene-') as directory:
        rust=Path(directory)/'scene.rs';exe=Path(directory)/'scene.exe';rust.write_text(code,encoding='utf-8')
        protocol=next(args.dependency_dir.glob('libhorizon_protocol-*.rlib'))
        for stage,command in [('compile',['rustc','--edition=2024','--test',str(rust),'-L','dependency='+str(args.dependency_dir),'--extern','horizon_protocol='+str(protocol),'-o',str(exe)]),('run',[str(exe),'--test-threads=1'])]:
            result=subprocess.run(command,capture_output=True,text=True);records.append({'stage':stage,'exit':result.returncode,'command':command,'stdout':result.stdout,'stderr':result.stderr});print(result.stdout+result.stderr)
            if result.returncode:raise SystemExit(result.returncode)
    incoming=(REPO/'mixins/sunset/combat_packet.rs').read_text(encoding='utf-8')
    assert incoming.index('sunset_packet_delay::incoming')<incoming.index('sunset_combat_inventory::packet')
    assert incoming.index('sunset_crystal_aura::packet')<incoming.index('sunset_combat_inventory::packet')
    packet_body=function(text,'pub(super) fn packet(')
    assert packet_body.index('C::Respawn')<packet_body.index('same_session(game, old)')
    discard=function(text,'fn discard_scene(')
    for bad in ['control::release','hold(','restore(','close_container(']:assert bad not in discard
    print('source-route contract: 1 passed (accepted packet boundary before identity match, exact ticket discard; not compiled)')
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':1,
            'source_sha256':{p.relative_to(REPO).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()for p in [SOURCE/name for name in ['combat_inventory.rs','combat_packet.rs','inventory_policy.rs','combat_lease.rs']]},
            'scope':'Eight production boundary/receipt adapters with native C codec roundtrips and actual Swap/Lease. Eleven old retirement tests removed from reused declaration harness; no duplicate count. No Game/client/network/liveACK.'},indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
