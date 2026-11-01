"""Production macro eligibility with real native KeyBinds and ToggleKeys.

Nine new Game/owner adapters; no client/server/RPC. Old input tests not recounted.
"""
from pathlib import Path
import ast,argparse,hashlib,importlib.util,json,re,subprocess,tempfile
REPO=Path(__file__).resolve().parents[1]
SOURCE=REPO/'mixins/sunset'
def function(t,s):
 a=t.index(s);b=t.index('{',a);depth=1
 for i in range(b+1,len(t)):
  depth+=(t[i]=='{')-(t[i]=='}')
  if not depth:return t[a:i+1]
 raise AssertionError(s)
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('engine',type=Path);p.add_argument('--dependency-dir',type=Path,required=True);p.add_argument('--out',type=Path);a=p.parse_args()
 assert(a.engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
 script=REPO/'tools/macro_input_tests.py';spec=importlib.util.spec_from_file_location('native_macro_input_helpers',script);adapter=importlib.util.module_from_spec(spec);spec.loader.exec_module(adapter)
 tree=ast.parse(script.read_text());code=next(n.value for n in ast.walk(tree)if isinstance(n,ast.Constant)and isinstance(n.value,str)and n.value.startswith('\n#!'))
 old=list(re.finditer(r'#\[test\]fn \w+\(',code));assert len(old)==16
 for match in old:code=code.replace(function(code,match.group()),'')
 macro=(SOURCE/'combat_macros.rs').read_text(encoding='utf-8');remote=(REPO/'mixins/sunset/combat_remote.rs').read_text()
 helpers='\n'.join(function(macro,s)for s in ['pub(super) fn user_mouse(','pub(super) fn focus_lost(','fn physical_use(','fn down(','fn anchor_can_click('])
 code=code.replace('HELPERS',helpers).replace('SPRINT',function(remote,'pub(crate) fn sprint_synced('))
 code=code.replace('struct Player{sprinting:bool}', 'struct Health(f32);impl Health{fn health(&self)->f32{self.0}}struct Player{sprinting:bool,health:Health}')
 code=code.replace('right:bool,ticks:u64,','toggles:horizon_client::keymapping::ToggleKeys,selected:usize,using:Option<()>,mining:Option<()>,left:bool,riding:Option<u32>,mobs:Vec<()>,crystals:Vec<()>,right:bool,ticks:u64,')
 code=code.replace('right:false,ticks:4,','toggles:Default::default(),selected:0,using:None,mining:None,left:false,riding:None,mobs:vec![],crystals:vec![],right:false,ticks:4,')
 code=code.replace('Player{sprinting:false}', 'Player{sprinting:false,health:Health(20.)}')
 code=code.replace('fn native_playing(&self)->bool{self.playing}', 'fn native_window(&self)->bool{self.playing}fn spectator(&self)->bool{false}fn native_playing(&self)->bool{self.playing}')
 sync=function((a.engine/'crates/horizon-client/src/game.rs').read_text(),'fn sync_toggle(')
 code=code.replace('mod game{', 'impl Game<\'_>{'+sync+'}\nmod util{pub fn in_world(_: &super::Game<\'_>)->bool{true}pub fn inventory_idle(_: &super::Game<\'_>)->bool{true}}\nmod sunset_player{pub fn gap_eating()->bool{false}}\nmod game{')
 code=code.replace('fn allowed(g:&Game<\'_>)->bool{g.allowed}', 'struct Enabled;impl Enabled{fn is_on(&self,_:&str)->bool{false}}fn with<R>(f:impl FnOnce(&Enabled)->R)->R{f(&Enabled)}\n'+function(macro,'fn allowed('))
 tests=r'''
 #[test]fn crouching_crystal_retains_common_eligibility(){let mut g=fixture();g.keys.insert(KeyCode::ShiftLeft);assert!(allowed(&g));}
 #[test]fn canonical_secondary_use_rejects_anchor_only(){let mut g=fixture();g.keys.insert(KeyCode::ShiftLeft);assert!(!anchor_can_click(&g));assert!(allowed(&g));}
 #[test]fn unrelated_raw_right_shift_is_not_secondary_use(){let mut g=fixture();g.keys.insert(KeyCode::ShiftRight);g.physical_keys.insert(KeyCode::ShiftRight);assert!(anchor_can_click(&g));assert!(allowed(&g));}
 #[test]fn rebound_native_sneak_maps_to_actual_canonical_gate(){let mut g=fixture();g.settings.keys.set("key.sneak","key.keyboard.r");for target in g.settings.keys.translate_all(KeyCode::KeyR){if let Ok(key)=target{g.keys.insert(key);}}assert!(g.keys.contains(&KeyCode::ShiftLeft));assert!(!anchor_can_click(&g));assert!(allowed(&g));}
 #[test]fn native_toggled_sneak_survives_release_then_second_press_clears(){use horizon_client::keymapping::KeyAction;let mut g=fixture();g.toggles.key(0,KeyAction::Press,false,true);g.sync_toggle(0);assert!(!anchor_can_click(&g));assert!(g.physical_keys.is_empty());g.toggles.key(0,KeyAction::Release,false,true);g.sync_toggle(0);assert!(!anchor_can_click(&g));g.toggles.key(0,KeyAction::Press,false,true);g.sync_toggle(0);assert!(anchor_can_click(&g));}
 #[test]fn mounted_player_is_ineligible_for_both_macros(){let mut g=fixture();g.riding=Some(7);assert!(!allowed(&g));g.riding=None;assert!(allowed(&g));}
 #[test]fn crouched_quick_edge_guards_but_cannot_replay_anchor_start(){let mut g=fixture();g.keys.insert(KeyCode::ShiftLeft);user_mouse(&g,MouseButton::Right,true);user_mouse(&g,MouseButton::Right,false);state(|s|assert!(!s.anchor_press.as_ref().unwrap().start_allowed));g.keys.clear();state(|s|assert!(!s.anchor_press.take().unwrap().start_allowed));}
 #[test]fn ordinary_quick_edge_still_starts_anchor(){let g=fixture();user_mouse(&g,MouseButton::Right,true);user_mouse(&g,MouseButton::Right,false);state(|s|assert!(s.anchor_press.take().unwrap().start_allowed));}
 #[test]fn dead_and_overfull_scenes_remain_common_rejections(){let mut g=fixture();g.player.as_mut().unwrap().health.0=0.;assert!(!allowed(&g));g.player.as_mut().unwrap().health.0=20.;g.mobs=vec![();257];assert!(!allowed(&g));}
'''
 index=code.rfind('}');code=code[:index]+tests+code[index:]
 native,=a.dependency_dir.glob('libhorizon_client_input-*.rlib');winit=adapter.paired_winit(a.dependency_dir,native);directories=adapter.native_search(a.dependency_dir,[])
 records=[]
 with tempfile.TemporaryDirectory(prefix='sunset-macro-eligibility-')as directory:
  src=Path(directory)/'eligibility.rs';exe=Path(directory)/'eligibility.exe';src.write_text(code,encoding='utf-8')
  command=['rustc','--edition=2024','--test',str(src),'-L','dependency='+str(a.dependency_dir),'--extern','horizon_client_input='+str(native),'--extern','winit='+str(winit),'-o',str(exe)]
  for d in directories:command+=['-L','native='+str(d)]
  for stage,cmd in [('compile',command),('run',[str(exe),'--test-threads=1'])]:
   result=subprocess.run(cmd,capture_output=True,text=True);records.append({'stage':stage,'command':cmd,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr});print(result.stdout+result.stderr)
   if result.returncode:raise SystemExit(result.returncode)
 intercept=function(macro,'pub(super) fn intercept_use(');tick=function(macro,'pub(super) fn tick(');queue=function(macro,'fn queue(')
 assert intercept.index('sunset_loot_guard::allows')<intercept.index('if !allowed(game)')
 assert 'game.anchor_works() || !anchor_can_click(game)'in intercept
 assert '(run.sequence.kind==Kind::Anchor && !anchor_can_click(game))'in tick and '(run.sequence.kind==Kind::Anchor && !anchor_can_click(game))'in queue
 print('source-route contract: 1 passed (owner-specific secondary-use at intent/active/deferred and mounted common gate; not compiled)')
 if a.out:
  a.out.parent.mkdir(parents=True,exist_ok=True)
  a.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':1,'paired_native_winit':str(winit),
   'source_sha256':{p.relative_to(REPO).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()for p in [SOURCE/'combat_macros.rs']},
   'scope':'Nine new production eligibility/observer adapters with actual native KeyBinds/ToggleKeys/sync_toggle. Old sixteen input test bodies excluded; one source contract. No Game/client/RPC or live mounted result.'},indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
