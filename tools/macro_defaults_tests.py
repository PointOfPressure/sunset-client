"""Compile current manual macro defaults and actual-action damage gates.

Thirteen production-function adapter checks with instrumented geometry and
equipment confidence, plus one separate route contract. No native game/network.
"""
from pathlib import Path
import argparse,hashlib,json,subprocess,tempfile


def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--out',type=Path);args=parser.parse_args()
    repo=Path(__file__).resolve().parents[1];source=repo/'mixins/sunset'
    macro=(source/'combat_macros.rs').read_text(encoding='utf-8');damage=(source/'explosive_damage.rs').read_text(encoding='utf-8')
    tactics=(source/'explosive_tactics.rs').read_text(encoding='utf-8')
    defaults=function(macro,'pub(super) fn defaults(');safe=function(macro,'fn safe_blast(');stack=function(macro,'fn stack(')
    estimate=function(damage,'pub(super) fn observed_local_damage(')
    endangered=function(tactics,'pub(super) fn endangered(')
    code=r'''
#![allow(dead_code)]
#[path="POLICY"]mod policy;
use policy::Kind;
#[derive(Clone)]enum Value{Bool(bool),Num(f64,f64,f64,f64),Pick(&'static [&'static str])}
#[derive(Clone)]struct Module{name:&'static str,fields:Vec<(&'static str,Value)>}
enum Category{Combat}
impl Module{
 fn new(name:&'static str,_:Category,_:&str)->Self{Self{name,fields:vec![]}}
 fn bool(mut self,name:&'static str,value:bool)->Self{self.fields.push((name,Value::Bool(value)));self}
 fn num(mut self,name:&'static str,value:f64,lo:f64,hi:f64,step:f64)->Self{self.fields.push((name,Value::Num(value,lo,hi,step)));self}
 fn pick(mut self,name:&'static str,values:&'static [&'static str])->Self{self.fields.push((name,Value::Pick(values)));self}
 fn b(&self,name:&str)->bool{match &self.fields.iter().find(|(key,_)|*key==name).unwrap().1{Value::Bool(value)=>*value,_=>panic!("wrong field")}}
 fn n(&self,name:&str)->f64{match &self.fields.iter().find(|(key,_)|*key==name).unwrap().1{Value::Num(value,..)=>*value,_=>panic!("wrong field")}}
}
struct Health{hp:f32,abs:f32}impl Health{fn health(&self)->f32{self.hp}fn absorption(&self)->f32{self.abs}}
struct Player{health:Health}
struct Stack{item:String,count:u32}
struct Game<'a>{player:Option<Player>,inventory:Vec<Option<Stack>>,offhand:Option<Stack>,creative:bool,
 known:bool,estimate:Option<f64>,loot:bool,life:std::marker::PhantomData<&'a ()>}
#[derive(Clone,Copy)]struct Blast;
#[derive(Default)]struct Budget{samples:usize}
mod util{pub fn short(name:&str)->&str{name.strip_prefix("minecraft:").unwrap_or(name)}}
mod sunset_loot_guard{pub fn allows(game:&super::Game<'_>,_:super::Blast,enabled:bool)->bool{!enabled||game.loot}}
mod sunset_explosive_tactics{ENDANGERED}
mod damage{
 use super::*;
 struct EquipmentProof{known:bool}
 fn equipment_proof(game:&Game<'_>,_:f32)->EquipmentProof{EquipmentProof{known:game.known}}
 struct Prediction{estimate:f64}
 fn local_damage(game:&mut Game<'_>,_:Blast,budget:&mut Budget)->Option<Prediction>{budget.samples+=1;game.estimate.map(|estimate|Prediction{estimate})}
 ESTIMATE
}
mod macros{
 use super::*;use damage::observed_local_damage;
 const CRYSTAL:&str="Crystal Macro";const ANCHOR:&str="Anchor Macro";const OFFHAND:usize=40;
 DEFAULTS
 STACK
 SAFE
 fn fixture(estimate:Option<f64>,hp:f32)->Game<'static>{Game{player:Some(Player{health:Health{hp,abs:0.0}}),
  inventory:(0..9).map(|_|None).collect(),offhand:None,creative:false,known:true,estimate,loot:true,life:std::marker::PhantomData}}
 fn crystal(game:&mut Game<'_>,budget:&mut Budget)->bool{safe_blast(game,&defaults()[0],Kind::Crystal,Blast,None,budget)}
 fn anchor(game:&mut Game<'_>,slot:usize,budget:&mut Budget)->bool{safe_blast(game,&defaults()[1],Kind::Anchor,Blast,Some(slot),budget)}
 fn totem()->Stack{Stack{item:"minecraft:totem_of_undying".into(),count:1}}
 #[test]fn crystal_specific_schema_is_reference_seven_plus_native_loot(){
  let modules=defaults();let m=&modules[0];let names:Vec<_>=m.fields.iter().map(|(key,_)|*key).collect();
  assert_eq!(names,["Break delay","Place delay","Auto obsidian","Obsidian delay","Switch back","Max self damage","Human pace","Protect loot"]);
  assert_eq!(m.n("Max self damage"),8.0);assert_eq!(m.n("Obsidian delay"),2.0);assert!(m.b("Switch back"));assert!(m.b("Protect loot"));
 }
 #[test]fn anchor_specific_schema_is_reference_five_plus_loot(){let modules=defaults();let m=&modules[1];
  assert_eq!(m.fields.iter().map(|(key,_)|*key).collect::<Vec<_>>(),["Mode","Delay","Safe anchor","If unguarded","Switch back","Protect loot"]);
  assert!(!m.b("Switch back"));assert!(m.b("Safe anchor"));assert!(m.b("Protect loot"));assert_eq!(m.n("Delay"),0.0);
 }
 #[test]fn crystal_default_cap_is_inclusive_and_does_not_credit_totem(){let mut budget=Budget::default();
  let mut game=fixture(Some(8.0),20.0);assert!(crystal(&mut game,&mut budget));
  game.estimate=Some(8.001);game.offhand=Some(totem());assert!(!crystal(&mut game,&mut budget));
 }
 #[test]fn reference_margin_allows_damage_four_at_health_seven_without_fixed_four_hp_rule(){
  let mut game=fixture(Some(4.0),7.0);assert!(crystal(&mut game,&mut Budget::default()));
  game.player.as_mut().unwrap().health.hp=6.6;assert!(!crystal(&mut game,&mut Budget::default()));
 }
 #[test]fn raised_crystal_cap_still_obeys_scaled_reference_margin(){let mut game=fixture(Some(16.0),20.0);let mut module=defaults().remove(0);
  for(key,value)in &mut module.fields{if *key=="Max self damage"{*value=Value::Num(20.0,0.0,20.0,0.5)}}
  assert!(!safe_blast(&mut game,&module,Kind::Crystal,Blast,None,&mut Budget::default()));
 }
 #[test]fn anchor_has_no_hidden_eight_damage_cap(){let mut game=fixture(Some(10.0),20.0);assert!(anchor(&mut game,0,&mut Budget::default()));}
 #[test]fn anchor_actual_detonation_totem_bypasses_only_self_margin(){let mut game=fixture(Some(100.0),1.0);game.inventory[2]=Some(totem());let mut budget=Budget::default();
  assert!(anchor(&mut game,2,&mut budget));assert_eq!(budget.samples,0);game.inventory[2]=None;game.offhand=Some(totem());
  assert!(anchor(&mut game,0,&mut budget));assert_eq!(budget.samples,0);
 }
 #[test]fn double_replacement_anchor_does_not_borrow_a_different_current_main_totem(){let mut game=fixture(Some(100.0),20.0);game.inventory[0]=Some(totem());
  game.inventory[2]=Some(Stack{item:"minecraft:respawn_anchor".into(),count:1});assert!(!anchor(&mut game,2,&mut Budget::default()));
 }
 #[test]fn protected_loot_is_checked_before_totem_exemption_or_damage(){let mut game=fixture(Some(100.0),20.0);game.offhand=Some(totem());game.loot=false;let mut budget=Budget::default();
  assert!(!anchor(&mut game,0,&mut budget));assert_eq!(budget.samples,0);
 }
 #[test]fn invalid_health_does_not_turn_totem_into_unbounded_unknown_permission(){let mut game=fixture(Some(100.0),f32::NAN);game.offhand=Some(totem());assert!(!anchor(&mut game,0,&mut Budget::default()));}
 #[test]fn unknown_ordinary_self_estimate_fails_closed(){let mut game=fixture(None,20.0);assert!(!anchor(&mut game,0,&mut Budget::default()));
  game.estimate=Some(f64::NAN);assert!(!crystal(&mut game,&mut Budget::default()));game.estimate=Some(-1.0);assert!(!anchor(&mut game,0,&mut Budget::default()));}
 #[test]fn unmodelled_equipment_cannot_borrow_prototype_estimate(){let mut game=fixture(Some(0.0),20.0);game.known=false;let mut budget=Budget::default();
  assert!(!crystal(&mut game,&mut budget));assert_eq!(budget.samples,0);game.offhand=Some(totem());assert!(anchor(&mut game,0,&mut budget));
 }
 #[test]fn creative_anchor_margin_is_exempt_but_crystal_retains_its_own_cap(){let mut game=fixture(Some(100.0),20.0);game.creative=true;
  assert!(anchor(&mut game,0,&mut Budget::default()));assert!(!crystal(&mut game,&mut Budget::default()));}
}
'''.replace('POLICY',(source/'macro_policy.rs').as_posix()).replace('ENDANGERED',endangered).replace('ESTIMATE',estimate).replace('DEFAULTS',defaults).replace('STACK',stack).replace('SAFE',safe)
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-macro-defaults-') as scratch:
        rust=Path(scratch)/'defaults.rs';exe=Path(scratch)/'defaults.exe';rust.write_text(code,encoding='utf-8')
        for stage,command in [('compile',['rustc','--edition=2024','--test',str(rust),'-o',str(exe)]),('run',[str(exe)])]:
            result=subprocess.run(command,capture_output=True,text=True)
            records.append({'stage':stage,'command':command,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
            print(result.stdout+result.stderr)
            if result.returncode:raise SystemExit(result.returncode)
    next_action=function(macro,'fn next_action(')
    place=next_action.split('Step::CrystalPlace =>',1)[1].split('Step::CrystalBreak =>',1)[0]
    breaking=next_action.split('Step::CrystalBreak =>',1)[1].split('Step::AnchorPlace =>',1)[0]
    detonate=next_action.split('Step::Detonate =>',1)[1]
    assert 'safe_blast(' not in place and 'visible(game' in place and 'crystal_room(game' in place
    assert 'safe_blast(' in breaking and 'Kind::Crystal' in breaking
    assert detonate.index('let slot=')<detonate.index('safe_blast(') and 'Some(slot)' in detonate
    assert 'planned_blast' in function(macro,'fn loot_allowed(')
    assert '|| !loot_allowed(game, &run)' in macro
    for removed in ['Damage model','Max friend damage','Min remaining health']:assert '"'+removed+'"' not in macro
    print('source-route contract: 1 passed (place is not detonation, actual selected hand, removed hidden settings; not compiled)')
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'checks':records,'separate_source_route_contracts':1,
            'source_sha256':{p.relative_to(repo).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in
                             [source/name for name in ['combat_macros.rs','explosive_damage.rs','explosive_tactics.rs']]},
            'scope':'Thirteen exact production defaults/safety wrappers with instrumented damage confidence; one separate source contract. No actual exposure/native client/network.'},indent=2)+'\n',encoding='utf-8')


if __name__=='__main__':main()
