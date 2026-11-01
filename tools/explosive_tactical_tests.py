"""Test extracted production temporal assessment with pinned native armor.

Raw exposure damage is an explicit instrumented fixture; this tests one-sample
reuse, hurt-window/armor ordering and safety/focus policy, not live geometry.
The exact 16/6 public schemas are also checked without counting extra tests.
"""
from pathlib import Path
import argparse,hashlib,json,re,subprocess,tempfile

def block(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1;cursor=opening+1
    while depth:depth+=(text[cursor]=='{')-(text[cursor]=='}');cursor+=1
    return text[start:cursor]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('engine',type=Path)
    parser.add_argument('--out',type=Path)
    args=parser.parse_args()
    repo=Path(__file__).resolve().parents[1];source=repo/'mixins/sunset';engine=args.engine.resolve()
    revision='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    assert (engine/'.horizon-revision').read_text().strip()==revision
    prediction=(source/'prediction.rs').read_text()
    pair=block(prediction,'pub(super) fn estimate_pair(').replace('pub(super)','')
    tactics=(source/'explosive_tactics.rs').read_text()
    pure='\n'.join(block(tactics,n).replace('pub(super)','') for n in ['pub(super) fn endangered(','pub(super) fn target_score('])
    kernel=engine/'crates/horizon-world/src/combat_rules.rs'
    native=kernel.read_text().replace('//!','//')
    clock=source/'hurt_policy.rs'
    expected={
      'crystal_aura.rs':[
        ('bool','Require Hold',False),('text','Hold Key','mouse4'),
        ('num','Place Delay',0,0,10,1),('num','Break Delay',0,0,10,1),
        ('num','Min Damage',4,0,20,.5),('num','Max Self Damage',8,0,20,.5),
        ('bool','Face Place',True),('text','Force Face Place','None'),('bool','Auto Obsidian',True),
        ('bool','Switch Back',True),('num','Turn Speed',4.5,1,10,.5),('num','View Angle',90,30,180,5),
        ('bool','Human Pace',True),('bool','Protect Loot',True),('bool','Render',True),('text','Render Color','#FFD65CFF')],
      'anchor_aura.rs':[
        ('num','Min Damage',6,0,20,.5),('num','Max Self Damage',6,0,20,.5),
        ('bool','Render',True),('text','Render Color','#FFFF9A3C'),('num','Turn Speed',4.5,1,10,.5),('bool','Protect Loot',True)]}
    schemas={}
    for name,golden in expected.items():
        text=(source/name).read_text();defaults=block(text,'pub(super) fn defaults(')
        schema=[]
        for kind,key,tail in re.findall(r'\.(bool|text|num|pick)\("([^"]+)"\s*,([^)]*)\)',defaults):
            if kind=='bool':value=(tail.strip()=='true',)
            elif kind=='text':value=(json.loads(tail.strip()),)
            elif kind=='num':value=tuple(float(n.strip()) for n in tail.split(','))
            else:raise AssertionError('Unexpected nonreference setting: '+key)
            schema.append((kind,key,*value))
        assert schema==golden,(name,schema,golden)
        names={field[1] for field in schema}
        for read in re.findall(r'(?:m|snapshot|expected)\.(?:n|b|t|p)\("([^"]+)"',text):
            assert read in names,(name,'removed hidden setting read',read)
        schemas[name]=schema
    harness = r'''
#![allow(dead_code)]
use std::sync::{Mutex,atomic::{AtomicUsize,Ordering}};
static SAMPLES:AtomicUsize=AtomicUsize::new(0);
static GATE:Mutex<()>=Mutex::new(());
struct Game<'a>{raw:f32,_life:std::marker::PhantomData<&'a ()>}
struct Target{id:u32}
#[derive(Clone,Copy)]struct Blast;
struct Budget;
struct State{clock:hurt::Clock}
static STATE:Mutex<Option<State>>=Mutex::new(None);
fn state<R>(f:impl FnOnce(&mut State)->R)->R{let mut s=STATE.lock().unwrap();f(s.get_or_insert_with(||State{clock:hurt::Clock::new()}))}
struct Armor;
impl combat_rules::ArmorWorld for Armor {
    type Error=();type Weapon=();type ServerLevel=();
    fn weapon(&mut self)->Result<Option<()>,()>{Ok(None)}
    fn server_level(&mut self)->Result<Option<()>,()>{Ok(None)}
    fn modify_armor_effectiveness(&mut self,_:(),_:(),v:f32)->Result<f32,()>{Ok(v)}
}
mod damage {
    use super::*;
    #[derive(Clone,Copy)]pub struct Sample(f32);
    impl Sample {pub fn raw_before_armor(self)->f32{self.0}}
    pub fn target_hurt_raw_at(game:&Game<'_>,_:&Target,_:Blast,_:&mut Budget)->Option<Sample>{SAMPLES.fetch_add(1,Ordering::SeqCst);game.raw.is_finite().then_some(Sample(game.raw))}
    pub fn reduce_target_hurt_raw(_: &Game<'_>,_:&Target,_:Sample,raw:f32)->Option<f64>{Some(f64::from(combat_rules::after_armor(&mut Armor,raw,20.0,8.0).ok()?))}
}
fn setup()->Game<'static>{SAMPLES.store(0,Ordering::SeqCst);state(|s|{s.clock=hurt::Clock::new();s.clock.tick(1,100,|_|true);s.clock.sent(2,10.0,hurt::ShotSource::Crystal(10),0);});Game{raw:15.0,_life:std::marker::PhantomData}}
#[test]fn one_exposure_sample_produces_current_and_opening_with_native_nonlinear_armor(){let _g=GATE.lock().unwrap();let game=setup();let (pair,raw)=estimate_pair(&game,&Target{id:2},Blast,0,10,0,&mut Budget).unwrap();assert_eq!(raw,15.0);assert_eq!(SAMPLES.load(Ordering::SeqCst),1);let delta=combat_rules::after_armor(&mut Armor,5.0,20.0,8.0).unwrap();let full=combat_rules::after_armor(&mut Armor,15.0,20.0,8.0).unwrap();assert_eq!(pair,[f64::from(delta),f64::from(full),f64::from(full)]);assert_ne!(delta,full-combat_rules::after_armor(&mut Armor,10.0,20.0,8.0).unwrap());}
#[test]fn opening_assessment_does_not_commit_or_mutate_the_current_hurt_window(){let _g=GATE.lock().unwrap();let game=setup();estimate_pair(&game,&Target{id:2},Blast,0,10,0,&mut Budget).unwrap();state(|s|{assert_eq!(s.clock.opens_in(2,0),10);assert_eq!(s.clock.assess(2,15.0,0,0).raw_for_reduction,5.0);});}
#[test]fn zero_current_damage_can_still_stage_a_real_opening(){let _g=GATE.lock().unwrap();let mut game=setup();game.raw=8.0;let (pair,_)=estimate_pair(&game,&Target{id:2},Blast,0,10,0,&mut Budget).unwrap();assert_eq!(pair[0],0.0);assert!(pair[1]>0.0);assert_eq!(SAMPLES.load(Ordering::SeqCst),1);}
#[test]fn failed_raw_sample_never_manufactures_an_opening(){let _g=GATE.lock().unwrap();let mut game=setup();game.raw=f32::NAN;assert!(estimate_pair(&game,&Target{id:2},Blast,0,10,0,&mut Budget).is_none());}
#[test]fn native_reference_endangerment_is_strict_at_the_margin_and_creative_exemption(){assert!(endangered(8.0,11.2,false));assert!(!endangered(8.0,11.21,false));assert!(!endangered(8.0,1.0,true));assert!(endangered(f64::NAN,20.0,true));}
#[test]fn target_focus_weakness_and_bot_bonuses_compete_with_distance(){assert!(target_score(9.0,true,true,true)<target_score(2.0,false,false,false));assert_eq!(target_score(9.0,true,false,true),5.0);}

'''

    harness=harness.replace('#![allow(dead_code)]','')
    code='#![allow(dead_code)]\n#[path="'+clock.as_posix()+'"]mod hurt;\nmod combat_rules {\n'+native+'\n}\n'+harness+'\n'+pair+'\n'+pure
    records=[]
    with tempfile.TemporaryDirectory(prefix='sunset-explosive-tactics-') as temp:
        src=Path(temp)/'cached.rs';exe=Path(temp)/'cached.exe';src.write_text(code)
        for label,command in [('compile',['rustc','--edition=2024','--test',str(src),'-o',str(exe)]),('cached temporal native armor',[str(exe)])]:
            result=subprocess.run(command,capture_output=True,text=True)
            records.append({'name':label,'exit':result.returncode,'stdout':result.stdout,'stderr':result.stderr})
            print(result.stdout,end='');print(result.stderr,end='')
            if result.returncode:raise SystemExit(result.returncode)
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps({'native_revision':revision,'checks':records,'schema':schemas,
          'native_armor_sha256':hashlib.sha256(kernel.read_bytes()).hexdigest(),
          'source_sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [source/'prediction.rs',source/'explosive_tactics.rs',clock]},
          'scope':'6 extracted production tests with actual native armor and HurtClock. Raw sample is an instrumented fixture; no Game build, world or network. Schema assertions are not counted as additional tests.'},indent=2)+'\n')

if __name__=='__main__':main()
