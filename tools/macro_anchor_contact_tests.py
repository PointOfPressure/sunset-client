"""Compile production sent-contact helpers against pinned Game::clip_ray.

Unit-cube outlines and explicit visibility-budget adapter; no Game/client/RPC.
Old AuraBase tests are removed from the reused declarations, not recounted.
"""
from pathlib import Path
import ast,argparse,hashlib,json,re,subprocess,tempfile
REPO=Path(__file__).resolve().parents[1]
SOURCE=REPO/'mixins/sunset'
def function(t,s):
 a=t.index(s);b=t.index('{',a);depth=1
 for i in range(b+1,len(t)):
  depth+=(t[i]=='{')-(t[i]=='}')
  if not depth:return t[a:i+1]
 raise AssertionError(s)
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('engine',type=Path);p.add_argument('--out',type=Path);a=p.parse_args();assert(a.engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
 text=(SOURCE/'combat_macros.rs').read_text(encoding='utf-8');adapter=(REPO/'tools/aura_base_engine_tests.py').read_text();tree=ast.parse(adapter)
 code=next(n.value for n in ast.walk(tree)if isinstance(n,ast.Constant)and isinstance(n.value,str)and n.value.startswith('\n#!'))
 oldtests=list(re.finditer(r'#\[test\]fn \w+\(',code));assert len(oldtests)==6
 for match in oldtests:code=code.replace(function(code,match.group()),'')
 code=code.replace('previews:Vec<u64>,accept:bool,','sent:Option<(f32,f32)>,ranges:Option<(f64,f64)>,previews:Vec<u64>,accept:bool,')
 code=code.replace('previews:vec![],accept:true,','sent:None,ranges:Some((4.5,3.)),previews:vec![],accept:true,')
 host=(a.engine/'crates/horizon-client/src/game.rs').read_text();remote=(a.engine/'crates/horizon-client/src/game/remote.rs').read_text()
 code=code.replace('FACE',function(remote,'fn face_direction(')).replace('CLIP',function(host,'fn clip_ray(&self, origin: [f64; 3], dir: [f64; 3], reach: f64)')).replace('BRIDGE',(REPO/'mixins/sunset/aura_base.rs').as_posix())
 code=code.replace('fn fixture()->Game', '''#[path="POLICY"]mod policy;
mod sunset_combat_wire{pub fn sent_look(g:&super::Game<'_>)->Option<(f32,f32)>{g.sent}}
mod sunset_explosive_tactics{pub fn interaction_ranges(g:&super::Game<'_>)->Option<(f64,f64)>{g.ranges}}
IN_RANGE
LOOK
struct Budget{known:bool}
fn clear_line(g:&Game<'_>,eye:[f64;3],end:[f64;3],_:Option<[i32;3]>,b:&mut Budget)->bool{
 if !b.known{return false;}let delta=std::array::from_fn::<_,3,_>(|i|end[i]-eye[i]);let d=delta.iter().map(|v|v*v).sum::<f64>().sqrt();
 g.clip_ray(eye,delta.map(|n|n/d),d).is_none()
}
#[derive(Clone,Copy,Debug)]
CLICK
mod macros{use super::*;
HELPERS
#[cfg(test)]mod tests{use super::*;
 fn scene()->Game<'static>{let mut g=super::super::fixture();g.eye=[0.6,122.62,0.5];g.world.blocks.clear();g.world.blocks.insert([2,121,0]);g}
 fn budget()->Budget{Budget{known:true}}
 #[test]fn actual_top_contact_preserves_double_where_old_axis_chose_side(){let mut g=scene();g.sent=Some(look_at(g.eye,[2.5,122.,0.5]));
  let old=fallback_anchor_click(&g,[2,121,0],&mut budget()).unwrap();assert_eq!(old.face,4);
  let hit=sent_anchor_click(&g,[2,121,0],&mut budget()).unwrap();assert_eq!(hit.face,1);
  assert!(policy::double_eligible(true,hit.face,121,121,true,true));assert_eq!(anchor_click(&g,[2,121,0],&mut budget()).unwrap().face,1);}
 #[test]fn actual_side_contact_stays_single(){let mut g=scene();g.sent=Some(look_at(g.eye,[2.,121.5,0.5]));let hit=sent_anchor_click(&g,[2,121,0],&mut budget()).unwrap();assert_eq!(hit.face,4);assert!(!policy::double_eligible(true,hit.face,121,121,true,true));}
 #[test]fn absent_sent_contact_allows_only_planning_fallback(){let g=scene();assert!(anchor_click(&g,[2,121,0],&mut budget()).is_some());assert!(anchor_deed_click(&g,[2,121,0],&mut budget(),false).is_some());assert!(anchor_deed_click(&g,[2,121,0],&mut budget(),true).is_none());}
 #[test]fn fallback_up_never_establishes_actual_up_eligibility(){let mut g=scene();g.eye=[2.5,124.,0.5];assert_eq!(fallback_anchor_click(&g,[2,121,0],&mut budget()).unwrap().face,1);assert!(sent_anchor_click(&g,[2,121,0],&mut budget()).is_none());}
 #[test]fn native_outline_block_before_anchor_refuses_real_deed(){let mut g=scene();g.sent=Some(look_at(g.eye,[2.5,122.,0.5]));g.world.blocks.insert([1,122,0]);assert!(anchor_deed_click(&g,[2,121,0],&mut budget(),true).is_none());}
 #[test]fn current_attribute_range_and_wrong_block_are_not_bypassed(){let mut g=scene();g.sent=Some(look_at(g.eye,[2.5,122.,0.5]));g.ranges=Some((1.,3.));assert!(sent_anchor_click(&g,[2,121,0],&mut budget()).is_none());g.ranges=Some((4.5,3.));assert!(sent_anchor_click(&g,[3,121,0],&mut budget()).is_none());}
 #[test]fn unknown_ranges_visibility_or_nonfinite_sent_look_refuse(){let mut g=scene();g.sent=Some(look_at(g.eye,[2.5,122.,0.5]));assert!(sent_anchor_click(&g,[2,121,0],&mut Budget{known:false}).is_none());g.ranges=None;assert!(sent_anchor_click(&g,[2,121,0],&mut budget()).is_none());g.ranges=Some((4.5,3.));g.sent=Some((f32::NAN,0.));assert!(sent_anchor_click(&g,[2,121,0],&mut budget()).is_none());}
 #[test]fn f32_rotation_roundoff_is_tiny_but_not_bit_identical(){let mut g=scene();let point=[2.5,122.,0.5];g.sent=Some(look_at(g.eye,point));let hit=sent_anchor_click(&g,[2,121,0],&mut budget()).unwrap();assert_ne!(point,hit.hit);assert!(in_range(point,hit.hit,1.0e-4));}
 #[test]fn changed_eye_reclips_contact_and_old_large_drift_stays_rejected(){let mut g=scene();g.sent=Some(look_at(g.eye,[2.5,122.,0.5]));let old=sent_anchor_click(&g,[2,121,0],&mut budget()).unwrap();g.eye[0]+=0.00001;let fresh=anchor_deed_click(&g,[2,121,0],&mut budget(),true).unwrap();assert_ne!(old.hit,fresh.hit);assert_eq!(old.face,fresh.face);assert!(in_range(old.hit,fresh.hit,1.0e-4));g.eye[0]+=0.01;let changed=sent_anchor_click(&g,[2,121,0],&mut budget()).unwrap();assert!(!in_range(old.hit,changed.hit,1.0e-4));}
 #[test]fn actual_top_still_requires_supplies_height_and_double_setting(){let mut g=scene();g.sent=Some(look_at(g.eye,[2.5,122.,0.5]));let face=sent_anchor_click(&g,[2,121,0],&mut budget()).unwrap().face;
  assert!(!policy::double_eligible(false,face,121,121,true,true));assert!(!policy::double_eligible(true,face,120,121,true,true));assert!(!policy::double_eligible(true,face,121,121,false,true));assert!(!policy::double_eligible(true,face,121,121,true,false));}
}}
fn fixture()->Game''')
 helpers='\n'.join(function(text,s)for s in ['fn center(','fn visible(','fn sent_anchor_click(','fn fallback_anchor_click(','fn anchor_click(','fn anchor_deed_click('])
 code=code.replace('POLICY',(REPO/'mixins/sunset/macro_policy.rs').as_posix()).replace('IN_RANGE',function((REPO/'mixins/sunset/explosive_policy.rs').read_text(),'pub(super) fn in_range(')).replace('LOOK',function((REPO/'mixins/sunset/runtime.rs').read_text(),'pub(super) fn look_at(')).replace('CLICK',function(text,'struct Click')).replace('HELPERS',helpers)
 records=[]
 with tempfile.TemporaryDirectory(prefix='sunset-macro-anchor-')as directory:
  src=Path(directory)/'contact.rs';exe=Path(directory)/'contact.exe';src.write_text(code,encoding='utf-8')
  for stage,cmd in [('compile',['rustc','--edition=2024','--test',str(src),'-o',str(exe)]),('run',[str(exe)])]:
   result=subprocess.run(cmd,capture_output=True,text=True);records.append({'stage':stage,'command':cmd,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr});print(result.stdout+result.stderr)
   if result.returncode:raise SystemExit(result.returncode)
 queue=function(text,'fn queue(');tick=function(text,'pub(super) fn tick(')
 assert 'next_action(game, &run, &mut budget,true)'in queue and 'next_action(game, &run, &mut budget,false)'in tick
 assert 'let eligible=sent_anchor_click' in tick and '&& in_range(click.hit,c.hit,1.0e-4)'in queue
 assert queue.index('let action=fresh;')<queue.index('game.send(S::UseItemOn')
 print('source-route contract: 1 passed (actual Double face, strict deferred contact and fresh cursor; not compiled)')
 if a.out:
  a.out.parent.mkdir(parents=True,exist_ok=True)
  a.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':1,
   'source_sha256':{p.relative_to(REPO).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()for p in [SOURCE/name for name in ['combat_macros.rs','macro_policy.rs','explosive_policy.rs','aura_base.rs','runtime.rs']]},
   'scope':'Ten production sent-contact helpers with exact native Game::clip_ray and unit-cube outlines/visibility-budget adapter. Six old AuraBase test bodies excluded. One source contract; no live Double/client/server.'},indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
