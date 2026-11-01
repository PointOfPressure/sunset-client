"""Original Bot damage adapter checks; instrumented kernels, no world/RPC."""
from pathlib import Path
import argparse,subprocess,tempfile,json

def function(text,start):
 a=text.index(start);b=text.index('{',a);depth=1
 for i in range(b+1,len(text)):
  depth+=(text[i]=='{')-(text[i]=='}')
  if not depth:return text[a:i+1]
 raise AssertionError(start)
def main():
 root=Path(__file__).resolve().parent;production=root.parent/'mixins/sunset';s=(production/'crystal_bot.rs').read_text(encoding='utf-8')
 safe=function(s,'fn safe_blast(')
 whole=function(s,'fn sampled_threat(');tail=whole[whole.index('    let complete ='):];threat='fn sampled_adapter(game:&mut Game,sources:Vec<Blast>,at:[f64;3],signature:u64)->(Option<f64>,bool){'+tail
 endangered=function((production/'explosive_tactics.rs').read_text(encoding='utf-8'),'pub(super) fn endangered(')
 bound=next(line for line in(production/'crystal_bot_policy.rs').read_text(encoding='utf-8').splitlines()if line.startswith('pub const THREAT_PREDICTIONS:'))
 code=r"""
#![allow(dead_code)]
use std::{cell::RefCell,collections::HashMap};
#[derive(Clone,Copy)]struct Prediction{estimate:f64,upper:f64}
#[derive(Clone,Copy)]struct Blast{id:usize}
struct Budget;impl Budget{fn new(_:usize)->Self{Self}}
struct Health{life:f32,absorb:f32}impl Health{fn health(&self)->f32{self.life}fn absorption(&self)->f32{self.absorb}}
struct Player{health:Health}
struct Kind{id:&'static str}struct Mob{id:u32,kind:Kind,removed:bool,alive:bool}impl Mob{fn alive(&self)->bool{self.alive}}
struct Game<'a>{ticks:u64,creative:bool,player:Option<Player>,mobs:Vec<Mob>,names:HashMap<u32,String>,local:Vec<Option<Prediction>>,target:HashMap<u32,Option<Prediction>>,budget:bool,loot:bool,calls:usize,_life:std::marker::PhantomData<&'a()>}
impl Game<'_>{fn remote_player_name(&self,id:u32)->Option<&str>{self.names.get(&id).map(String::as_str)}}
struct Config{friends:Vec<String>}thread_local!{static CFG:RefCell<Config>=RefCell::new(Config{friends:vec![]});static STATE:RefCell<State>=RefCell::new(State::default());}
fn with<R>(f:impl FnOnce(&Config)->R)->R{CFG.with(|s|f(&s.borrow()))}fn state<R>(f:impl FnOnce(&mut State)->R)->R{STATE.with(|s|f(&mut s.borrow_mut()))}
#[derive(Default)]struct State{scan_complete:bool,threat:Option<f64>,threat_complete:bool,threat_origin:Option<[f64;3]>,threat_signature:u64,threat_at:u64}
fn local_damage(game:&mut Game,blast:Blast,_:&mut Budget)->Option<Prediction>{game.calls+=1;game.local.get(blast.id).copied().flatten()}
fn prediction_budget(g:&mut Game,_:u8)->bool{g.budget}
fn enemy(_: &Game,m:&Mob)->bool{m.id==7&&m.alive&&!m.removed}
mod policy{ACTUAL_BOUND}
mod util{pub fn mob_target(m:&super::Mob)->u32{m.id}}
mod sunset_loot_guard{pub fn allows(g:&super::Game,_:super::Blast,_:bool)->bool{g.loot}}
mod sunset_explosive_damage{pub fn target_damage(g:&super::Game,id:&u32,_:super::Blast,_:&mut super::Budget)->Option<super::Prediction>{g.target.get(id).copied().flatten()}}
mod sunset_explosive_tactics{ENDANGERED}
mod bot{use super::*;ADAPTER_SAFE ADAPTER_THREAT
fn fixture()->Game<'static>{CFG.with(|s|s.borrow_mut().friends.clear());state(|s|*s=State{scan_complete:true,..Default::default()});Game{ticks:100,creative:false,player:Some(Player{health:Health{life:20.,absorb:0.}}),mobs:vec![Mob{id:7,kind:Kind{id:"minecraft:player"},removed:false,alive:true}],names:[(7,"enemy".into())].into(),local:vec![Some(Prediction{estimate:1.,upper:80.})],target:[(7,Some(Prediction{estimate:4.,upper:80.}))].into(),budget:true,loot:true,calls:0,_life:std::marker::PhantomData}}
fn safe(g:&mut Game)->bool{safe_blast(g,Blast{id:0},7,2.,8.)}
#[test]fn real_adapter_uses_observed_local_estimate_instead_of_removed_ui_raw_mode(){let mut g=fixture();assert!(safe(&mut g));g.local[0].as_mut().unwrap().estimate=9.;assert!(!safe(&mut g));}
#[test]fn exact_shared_endangered_margin_and_actual_absorption_apply(){let mut g=fixture();g.local[0].as_mut().unwrap().estimate=15.66;assert!(!safe_blast(&mut g,Blast{id:0},7,2.,20.));g.player.as_mut().unwrap().health.absorb=4.;assert!(safe_blast(&mut g,Blast{id:0},7,2.,20.));g.local[0].as_mut().unwrap().estimate=15.65;g.player.as_mut().unwrap().health.absorb=0.;assert!(safe_blast(&mut g,Blast{id:0},7,2.,20.));}
#[test]fn invalid_local_or_requested_thresholds_fail_closed(){for value in[f64::NAN,f64::INFINITY,-1.]{let mut g=fixture();g.local[0].as_mut().unwrap().estimate=value;assert!(!safe(&mut g));}let mut g=fixture();assert!(!safe_blast(&mut g,Blast{id:0},7,f64::NAN,8.));assert!(!safe_blast(&mut g,Blast{id:0},7,2.,-1.));}
#[test]fn friend_raw_upper_zero_veto_and_unknown_identity_remain_conservative(){let mut g=fixture();CFG.with(|s|s.borrow_mut().friends.push("friend".into()));g.mobs.push(Mob{id:8,kind:Kind{id:"minecraft:player"},removed:false,alive:true});g.names.insert(8,"friend".into());g.target.insert(8,Some(Prediction{estimate:0.,upper:0.01}));assert!(!safe(&mut g));g.target.get_mut(&8).unwrap().as_mut().unwrap().upper=0.;assert!(safe(&mut g));g.names.remove(&8);assert!(!safe(&mut g));}
#[test]fn real_target_minimum_and_live_enemy_are_still_required(){let mut g=fixture();g.target.get_mut(&7).unwrap().as_mut().unwrap().estimate=1.99;assert!(!safe(&mut g));g.target.get_mut(&7).unwrap().as_mut().unwrap().estimate=2.;assert!(safe(&mut g));g.mobs[0].alive=false;assert!(!safe(&mut g));}
#[test]fn loot_prediction_budget_and_unknown_local_kernel_deny(){let mut g=fixture();g.loot=false;assert!(!safe(&mut g));g.loot=true;g.budget=false;assert!(!safe(&mut g));g.budget=true;g.local[0]=None;assert!(!safe(&mut g));}
#[test]fn actual_threat_loop_uses_observed_max_and_keeps_coverage_state(){let mut g=fixture();g.local=vec![Some(Prediction{estimate:2.,upper:100.}),Some(Prediction{estimate:5.,upper:70.})];assert_eq!(sampled_adapter(&mut g,vec![Blast{id:0},Blast{id:1}],[1.,2.,3.],9),(Some(5.),true));state(|s|{assert_eq!(s.threat,Some(5.));assert_eq!(s.threat_signature,9);assert_eq!(s.threat_at,100);assert_eq!(s.threat_origin,Some([1.,2.,3.]));});}
#[test]fn threat_loop_is_bounded_and_unknown_or_nonfinite_stays_unknown(){let mut g=fixture();let n=policy::THREAT_PREDICTIONS;g.local=vec![Some(Prediction{estimate:1.,upper:20.});n+1];assert_eq!(sampled_adapter(&mut g,(0..n+1).map(|id|Blast{id}).collect(),[0.;3],0),(Some(1.),false));assert_eq!(g.calls,n);g.local[0]=None;assert_eq!(sampled_adapter(&mut g,vec![Blast{id:0}],[0.;3],0),(None,true));g.local[0]=Some(Prediction{estimate:f64::NAN,upper:20.});assert_eq!(sampled_adapter(&mut g,vec![Blast{id:0}],[0.;3],0),(None,true));}
}
"""
 code=code.replace('ACTUAL_BOUND',bound).replace('ENDANGERED',endangered).replace('ADAPTER_SAFE',safe).replace('ADAPTER_THREAT',threat)
 with tempfile.TemporaryDirectory(prefix='bot-observed-damage-')as tmp:
  f=Path(tmp)/'test.rs';binary=Path(tmp)/'test.exe';f.write_text(code,encoding='utf-8');r=subprocess.run(['rustc','--edition=2024','--test',str(f),'-o',str(binary)],capture_output=True,text=True,encoding='utf-8');assert r.returncode==0,r.stderr
  r=subprocess.run([str(binary)],capture_output=True,text=True,encoding='utf-8');print(r.stdout);assert r.returncode==0,r.stdout+r.stderr
 assert 'if !d.upper.is_finite() || d.upper > 0.' in safe and 'if !friends.is_empty() && name.is_none()' in safe and 'if checked > 8' in safe
 assert 'Damage model'not in s and 'own + 1.'not in safe
 print('source-route contract: 2 passed')

if __name__=='__main__':main()
