"""Renderer-free real item/protocol and extracted production input adapters."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess,tempfile

def function(text,signature):
    a=text.index(signature);b=text.index('{',a);depth=1
    for i in range(b+1,len(text)):
        depth+=(text[i]=='{')-(text[i]=='}')
        if not depth:return text[a:i+1]
    raise AssertionError(signature)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('engine',type=Path);p.add_argument('--dependency-dir',type=Path,required=True);a=p.parse_args()
    root=Path(__file__).resolve().parent;native=a.engine/'crates/horizon-client/src';repo=root.parent/'mixins/sunset'
    assert (a.engine/'.horizon-revision').read_text(encoding='utf-8').strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    melee=(repo/'melee_assist.rs').read_text(encoding='utf-8');bot=(repo/'crystal_bot.rs').read_text(encoding='utf-8')
    remote=(native/'game/remote.rs').read_text(encoding='utf-8');manifest=(repo/'horizon-mixin.toml').read_text(encoding='utf-8')
    assert re.search(r'id\s*=\s*"pretick"\s+kind\s*=\s*"HEAD"\s+target\s*=\s*"horizon_client::game::Game',manifest)
    tick=function(remote,'fn tick_remote_level(');assert tick.index('self.ticks += 1')<tick.index('let keys = Keys')
    flags=remote[remote.index('let input = u8::from(keys.forward)'):];flags=flags[:flags.index(';')+1]
    view=function((native/'game.rs').read_text(encoding='utf-8'),'fn view_vector(')
    methods='\n'.join(function(melee,s)for s in ['pub(super) fn melee_item(','fn spear_hits_at(','pub(super) fn note_attack(','pub(super) fn note_stab_at(','pub(super) fn pre_input(','pub(super) fn post_input('])
    botmethods='\n'.join(function(bot,s)for s in ['fn kit_kind(','fn ordinary_launch_hand(','fn weapon('])
    code=r'''
#![allow(dead_code,unused_imports,private_interfaces)]
use std::{collections::{HashMap,HashSet},sync::Arc,time::Instant};
use horizon_client::{items::ItemStack,item_data::{StackData,Wire}};
use horizon_protocol::{buf::{Codec,Reader,Writer},item::{Stack,Slot,Component},packets::play::{self,Serverbound as S}};
use horizon_core::geometry::{Aabb,Vec3};
#[path="PACKETS"]mod packets;
#[path="TACTICS"]mod tactics;
fn patch(added:Vec<(i32,Component)>,removed:Vec<i32>)->Arc<[u8]>{let mut stack=Stack::new(7,1);stack.added=added;stack.removed=removed;let mut w=Writer::new();Slot::write(&Some(stack),&mut w);let stack=Slot::read(&mut Reader::new(&w.data)).unwrap().unwrap();stack.wire.0.unwrap()}
#[test]fn native_piercing_patch_and_removal_are_resolved_from_real_slot_codec(){let added=patch(vec![(38,Component::Raw(vec![1,0,0,0]))],vec![]);assert_eq!(packets::ordinary_component_hand(false,Some(&added)),Some(false));let removed=patch(vec![],vec![38]);assert_eq!(packets::ordinary_component_hand(true,Some(&removed)),Some(true));}
#[test]fn malformed_duplicate_oversized_or_trailing_patch_is_not_ordinary(){assert_eq!(packets::ordinary_component_hand(false,Some(&[])),None);let mut w=Writer::new();w.varint(2);w.varint(0);for _ in 0..2{w.varint(3);w.byte_array(&[0]);}assert_eq!(packets::ordinary_component_hand(false,Some(&w.data)),None);assert_eq!(packets::ordinary_component_hand(false,Some(&[0,0,1])),None);assert_eq!(packets::ordinary_component_hand(false,Some(&vec![0;8193])),None);}
#[test]fn unlearned_fight_keeps_core_progress_and_closing_without_persistent_rival(){let mut f=tactics::Fight::default();f.start(7,1);let v=tactics::FightView{tick:2,health:20.,absorption:0.,totems:2,offhand_totem:true,enemies:1,risk:0.,target_life:20.,target_totem:true,distance:8.,hole:false,target_y:1.,eat_below:14.};f.observe_unlearned(v);assert_eq!(f.progress_at,2);assert_eq!(f.closing_at,2);let mut v=v;v.tick=10;v.hole=true;v.distance=6.;v.target_life=15.;f.observe_unlearned(v);assert_eq!(f.progress_at,10);assert_eq!(f.closing_at,10);assert_eq!(f.seen_health,Some(15.));}
#[test]fn unrelated_sent_victim_cannot_consume_launch_ticket(){let original=Some((7,100,-90.,true));let mut p=original;assert!(tactics::take_forwarded_attack(&mut p,8,100).is_none());assert_eq!(p,original);assert_eq!(tactics::take_forwarded_attack(&mut p,7,101),original);assert!(p.is_none());}
#[test]fn stale_or_rewound_matching_launch_ticket_cannot_start_a_prediction(){for tick in[99,102]{let mut p=Some((7,100,-90.,true));assert!(tactics::take_forwarded_attack(&mut p,7,tick).is_none());assert!(p.is_none());}}
mod game {
 use super::*;
 #[derive(Clone,Copy,Hash,PartialEq,Eq,Debug)]enum KeyCode{KeyW,KeyS,KeyA,KeyD,ControlLeft,Space,ShiftLeft}
 #[derive(Clone,Copy,PartialEq)]enum Screen{Playing,Other}
 #[derive(Default)]struct Keys{forward:bool,backward:bool,left:bool,right:bool,jump:bool,shift:bool,sprint:bool,analog:Option<()>}
 struct Ground{on_ground:bool}struct Fluids{was_touching_water:bool}
 struct Player{sprinting:bool,ground:Ground,flying:bool,fall_flying:bool,fluids:Fluids,last_climbable:Option<()>}impl Player{fn is_in_lava(&self)->bool{false}}
 struct Mob{id:u32,b:Aabb,removed:bool}impl Mob{fn alive(&self)->bool{!self.removed}fn bounds(&self)->Aabb{self.b}}
 struct Info{disable_blocking_seconds:f32,components:Vec<String>,max_damage:u32,weapon_damage:Option<u32>,attack_damage:f32}
 struct Items{item_tags:HashMap<String,HashSet<String>>,data:HashMap<String,Info>}impl Items{fn get(&self,id:&str)->Option<&Info>{self.data.get(id)}}
 struct Res{items:Items}impl Res{fn items(&self)->&Items{&self.items}}
 struct Game<'a>{ticks:u64,keys:HashSet<KeyCode>,physical_keys:HashSet<KeyCode>,player:Option<Player>,mobs:Vec<Mob>,using:Option<()>,riding:Option<u32>,screen:Screen,res:Res,inventory:Vec<Option<ItemStack>>,selected:usize,yaw:f32,pitch:f32,eye:[f64;3],creative:bool,wall:bool,life:std::marker::PhantomData<&'a()>}
 impl Game<'_>{fn native_playing(&self)->bool{self.screen==Screen::Playing}fn held(&self)->Option<&ItemStack>{self.inventory[self.selected].as_ref()}fn items(&self)->&Items{&self.res.items}fn eye_position(&self)->Option<[f64;3]>{Some(self.eye)}fn known_player_movement(&self)->Vec3{Vec3::ZERO}
 fn input_packet(&self)->S{let held=|k|self.keys.contains(&k);let keys=Keys{forward:held(KeyCode::KeyW),backward:held(KeyCode::KeyS),left:held(KeyCode::KeyA),right:held(KeyCode::KeyD),jump:held(KeyCode::Space),shift:held(KeyCode::ShiftLeft),sprint:held(KeyCode::ControlLeft),analog:None};FLAGS S::PlayerInput(play::PlayerInput{flags:input})}}
 VIEW
 mod sunset_util{pub fn short(value:&str)->&str{value.strip_prefix("minecraft:").unwrap_or(value)}}
 mod sunset_bot_packet_policy{pub use crate::packets::*;}
 mod sunset_combat_wire{pub fn sent_sprinting(_: &super::Game<'_>)->bool{true}}
 mod spear{pub mod sunset_combat_collision{pub fn clip(g:&super::super::Game<'_>,a:horizon_core::geometry::Vec3,b:horizon_core::geometry::Vec3)->Option<horizon_core::geometry::Vec3>{g.wall.then(||horizon_core::geometry::Vec3::new(1.,a.y,a.z)).filter(|p|p.x>=a.x&&p.x<=b.x)}}}
 mod sunset_melee_assist {
 use super::*;use std::cell::RefCell;use horizon_client::spear::{Kinetic,REACH,hits_in_reach};
 #[path="MELEE_POLICY"]mod policy;
 const TAP:&str="W Tap";const JUMP:&str="Jump Reset";const INPUT_KEYS:[KeyCode;4]=[KeyCode::KeyW,KeyCode::KeyS,KeyCode::ControlLeft,KeyCode::Space];
 #[derive(Clone)]struct Module{hold:f64,mode:&'static str}impl Module{fn n(&self,_:&str)->f64{self.hold}fn p(&self,_:&str)->&str{self.mode}}
 struct InputSave{before:[bool;4],physical:[bool;4],changed:[bool;4]}
 #[derive(Default)]struct State{tick:u64,tap_until:u64,jump_pending:bool,jump_owner:Option<&'static str>,input:Option<InputSave>,last_attack:Option<u64>,recent_hit:Option<(u32,Instant)>,broken_shield:Option<(u32,u64)>,config:Option<Module>}
 thread_local!{static STATE:RefCell<State>=RefCell::new(State::default());}
 fn state<R>(f:impl FnOnce(&mut State)->R)->R{STATE.with(|s|f(&mut s.borrow_mut()))}fn module(name:&str)->Option<Module>{if name==TAP{state(|s|s.config.clone())}else{None}}
 fn gui_open()->bool{false}fn alive(g:&Game<'_>)->bool{g.player.is_some()}fn note(_: &str){}fn note_action(_: &Game<'_>){}fn observed_shield(_: &Game<'_>,_:u32)->bool{false}fn slot_strength(_: &Game<'_>,_:usize,_:f32)->Option<f32>{Some(1.)}mod control{pub fn available(_: &str)->bool{true}}
 METHODS
 pub(super)fn configure(hold:f64,mode:&'static str){state(|s|{*s=State::default();s.config=Some(Module{hold,mode});});}
 pub(super)fn input(g:&mut Game<'_>){pre_input(g,&[],g.ticks);}pub(super)fn restore(g:&mut Game<'_>){post_input(g);}pub(super)fn attack(g:&Game<'_>){note_attack(g,7);}pub(super)fn recent()->Option<u32>{state(|s|s.recent_hit.map(|x|x.0))}
 }
 mod bot{use super::*;use super::sunset_util as util;BOT_METHODS
 #[test]fn strict_kit_uses_real_melee_category_including_mace_trident_and_spear(){let g=fixture();for name in["minecraft:diamond_sword","minecraft:diamond_axe","minecraft:mace","minecraft:trident","minecraft:diamond_spear"]{assert_eq!(kit_kind(&g,&ItemStack::new(name,1)),Some(2),"{name}");}assert_eq!(kit_kind(&g,&ItemStack::new("minecraft:banana_sword",1)),None);}
 #[test]fn ordinary_launch_selects_trident_but_never_mace_or_piercing_spear(){let mut g=fixture();g.inventory[0]=Some(ItemStack::new("minecraft:mace",1));g.inventory[1]=Some(ItemStack::new("minecraft:diamond_spear",1));g.inventory[2]=Some(ItemStack::new("minecraft:trident",1));g.inventory[3]=Some(ItemStack::new("minecraft:diamond_sword",1));assert_eq!(weapon(&g,false),Some(2));g.inventory[4]=Some(ItemStack::new("minecraft:diamond_axe",1));assert_eq!(weapon(&g,true),Some(4));}
 #[test]fn patched_piercing_ordinary_item_and_unknown_changed_data_are_refused(){let g=fixture();let mut stack=ItemStack::new("minecraft:diamond_sword",1);let decoded=stack.clone();let mut data=StackData::default();data.wire=Wire::new(decoded,patch(vec![(38,Component::Raw(vec![1,0,0,0]))],vec![]));stack.extra=Some(Box::new(data));assert!(!ordinary_launch_hand(&g,&stack));stack.extra=Some(Box::new(StackData::default()));assert!(!ordinary_launch_hand(&g,&stack));}
 }
 fn fixture()->Game<'static>{let mut data=HashMap::new();for(name,damage)in[("diamond_sword",7.),("diamond_axe",9.),("trident",8.),("mace",12.),("diamond_spear",20.),("banana_sword",100.)]{data.insert(format!("minecraft:{name}"),Info{disable_blocking_seconds:0.,components:vec![],max_damage:1000,weapon_damage:Some(1),attack_damage:damage});}let item_tags=[("minecraft:swords".into(),["minecraft:diamond_sword".into()].into()),("minecraft:axes".into(),["minecraft:diamond_axe".into()].into())].into();Game{ticks:100,keys:[KeyCode::KeyW,KeyCode::ControlLeft].into(),physical_keys:[KeyCode::KeyW,KeyCode::ControlLeft].into(),player:Some(Player{sprinting:true,ground:Ground{on_ground:true},flying:false,fall_flying:false,fluids:Fluids{was_touching_water:false},last_climbable:None}),mobs:vec![Mob{id:7,b:Aabb::new(Vec3::new(3.,0.,-.3),Vec3::new(3.6,1.8,.3)),removed:false}],using:None,riding:None,screen:Screen::Playing,res:Res{items:Items{item_tags,data}},inventory:vec![Some(ItemStack::new("minecraft:diamond_sword",1));36],selected:0,yaw:0.,pitch:0.,eye:[0.,1.62,0.],creative:false,wall:false,life:std::marker::PhantomData}}
 fn flags(packet:S)->u8{let bytes=packet.encode();let mut reader=Reader::new(&bytes);reader.varint().unwrap();reader.u8().unwrap()}
 #[test]fn actual_production_hold_one_two_three_releases_exactly_requested_future_movement_steps(){for hold in 1..=3{let mut g=fixture();sunset_melee_assist::configure(hold as f64,"W Tap");sunset_melee_assist::attack(&g);let mut released=Vec::new();for _ in 0..5{sunset_melee_assist::input(&mut g);let bits=flags(g.input_packet());released.push(bits&1==0&&bits&64==0);g.ticks+=1;sunset_melee_assist::restore(&mut g);}assert_eq!(released.iter().filter(|v|**v).count(),hold as usize);assert!(released[..hold as usize].iter().all(|x|*x));assert!(!released[hold as usize]);}}
 #[test]fn production_s_tap_uses_backward_flags_and_restores_physical_w_after_step(){let mut g=fixture();sunset_melee_assist::configure(1.,"S Tap");sunset_melee_assist::attack(&g);sunset_melee_assist::input(&mut g);let bits=flags(g.input_packet());assert_eq!(bits&3,2);assert_eq!(bits&64,0);g.ticks+=1;sunset_melee_assist::restore(&mut g);assert!(g.keys.contains(&KeyCode::KeyW));assert!(!g.keys.contains(&KeyCode::KeyS));}
 #[test]fn production_stab_uses_sent_look_not_camera_and_sets_recent_contact(){let mut g=fixture();sunset_melee_assist::configure(1.,"W Tap");assert!(sunset_melee_assist::note_stab_at(&g,g.yaw,g.pitch).unwrap().is_empty());assert_eq!(sunset_melee_assist::recent(),None);assert_eq!(sunset_melee_assist::note_stab_at(&g,-90.,0.).unwrap(),[7]);assert_eq!(sunset_melee_assist::recent(),Some(7));sunset_melee_assist::input(&mut g);assert_eq!(flags(g.input_packet())&65,0);}
 #[test]fn native_spear_wall_clip_and_empty_hit_cannot_create_tap_or_recent_hit(){let mut g=fixture();g.wall=true;sunset_melee_assist::configure(1.,"W Tap");assert!(sunset_melee_assist::note_stab_at(&g,-90.,0.).unwrap().is_empty());assert_eq!(sunset_melee_assist::recent(),None);sunset_melee_assist::input(&mut g);assert_eq!(flags(g.input_packet())&65,65);}
 #[test]fn production_restore_respects_physical_release_instead_of_sticking_forward(){let mut g=fixture();sunset_melee_assist::configure(1.,"W Tap");sunset_melee_assist::attack(&g);sunset_melee_assist::input(&mut g);g.physical_keys.clear();g.ticks+=1;sunset_melee_assist::restore(&mut g);assert!(!g.keys.contains(&KeyCode::KeyW));assert!(!g.keys.contains(&KeyCode::ControlLeft));}
}
'''
    code=code.replace('PACKETS',(repo/'bot_packet_policy.rs').as_posix()).replace('TACTICS',(repo/'bot_tactics_policy.rs').as_posix()).replace('MELEE_POLICY',(repo/'melee_policy.rs').as_posix()).replace('FLAGS',flags).replace('VIEW',view).replace('BOT_METHODS',botmethods).replace('METHODS',methods)
    code=re.sub(r'(?<![\w.])\.(\d)',r'0.\1',code)
    deps={}
    for name in ['horizon_client','horizon_protocol','horizon_core']:
        files=sorted(a.dependency_dir.glob('lib'+name+'-*.rlib'),key=lambda p:p.stat().st_mtime,reverse=True);assert files;deps[name]=files[0]
    command=['rustc','--edition=2024','--test','SOURCE','-L','dependency='+str(a.dependency_dir)]
    for name,path in deps.items():command+=['--extern',name+'='+str(path)]
    for library in (Path.home()/'.cargo/registry/src').glob('*/windows_x86_64_msvc-*/lib/windows.0.*.lib'):
        command+=['-L','native='+str(library.parent)]
    with tempfile.TemporaryDirectory(prefix='sunset-r21-bot-')as temp:
        source=Path(temp)/'amend.rs';binary=Path(temp)/'amend.exe';source.write_text(code,encoding='utf-8')
        result=subprocess.run([str(source)if x=='SOURCE'else x for x in command]+['-o',str(binary)],capture_output=True,text=True,encoding='utf-8',errors='replace')
        assert result.returncode==0,result.stdout+result.stderr
        result=subprocess.run([str(binary)],capture_output=True,text=True,encoding='utf-8',errors='replace');print(result.stdout);assert result.returncode==0,result.stdout+result.stderr
    # Pin the unchanged native guard route separately from observer behavior.
    guard=function(melee,'pub(super) fn guard_stab_at(')
    assert 'let hits = spear_hits_at(game, yaw, pitch)' in guard and 'known_friend(game, id' in guard
    assert 'let blocked = hits.is_none() || friend.is_some()' in guard
    wire=(repo/'combat_wire.rs').read_text(encoding='utf-8')
    assert 'note_stab_at(game,yaw,pitch)' in wire and 'sent_look(game)' in wire and 'sunset_crystal_bot::stab' not in wire
    assert 't.fight.observe_unlearned(view)' in bot and 'ordinary_launch_hand(game, s)' in bot
    assert '38 => {' in(a.engine/'crates/horizon-protocol/src/item/skip.rs').read_text(encoding='utf-8')
    print('source-route contract: 3 passed')
if __name__=='__main__':main()
