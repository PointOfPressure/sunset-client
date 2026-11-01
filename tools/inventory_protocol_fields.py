"""Test production unpredicted swaps against the compiled native protocol.

No game or connection starts. The bridge is extracted unchanged; Game is a
read/send adapter. The real horizon_protocol crate supplies typed packets and
their wire encoder/decoder. Menu input ordinals come from pinned engine source.
"""
from pathlib import Path
import argparse,re,subprocess,tempfile

def block(text,start):
    opening=text.index('{',start);depth=1;end=opening+1
    while depth:
        depth+=(text[end]=='{')-(text[end]=='}');end+=1
    return text[start:end]

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('engine',type=Path);p.add_argument('--dependency-dir',type=Path,required=True)
    a=p.parse_args();engine=a.engine.resolve();repo=Path(__file__).resolve().parents[1]
    assert (engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    protocols=list(a.dependency_dir.glob('libhorizon_protocol-*.rlib'))
    assert len(protocols)==1,'Choose one dependency directory containing the exact pinned protocol build.'
    remote=(engine/'crates/horizon-client/src/game/remote.rs').read_text(encoding='utf-8')
    menu=(engine/'crates/horizon-client/src/menu.rs').read_text(encoding='utf-8')
    bridge=(repo/'mixins/sunset/combat_remote.rs').read_text(encoding='utf-8')
    native_input=block(menu,menu.index('pub enum Input'))
    ordinal=block(remote,remote.index('pub(super) fn input_ordinal'))
    swap=block(bridge,bridge.index('pub(crate) fn swap_unpredicted'))
    policy=str(repo/'mixins/sunset/inventory_policy.rs').replace('\\','/')
    code=f'#![allow(dead_code)]\nextern crate self as horizon_client;\n#[path="{policy}"]mod policy;\n'
    code+='mod menu {#[derive(Clone,Copy,PartialEq)]'+native_input+'}\n'
    code+=r'''
mod game {
 use horizon_protocol::packets::play::Serverbound as S;
 pub struct Remote {pub state_id:i32}
 pub struct Game<'a>{pub carried:Option<u8>,pub remote:Option<Remote>,pub menu:(i32,String),pub local:Vec<u32>,pub sent:Vec<S>,pub life:std::marker::PhantomData<&'a ()>}
 impl Game<'_>{
  pub fn remote_menu(&self)->(i32,String){self.menu.clone()}
  pub fn menu_slot_index(_: &str,from:usize)->Option<usize>{match from{0..=8=>Some(36+from),9..=35=>Some(from),300=>Some(0),65536=>Some(65536),_=>None}}
  pub fn send(&mut self,packet:S){self.sent.push(packet)}
 }
 pub mod remote {use crate::menu::Input;
'''+ordinal+'\npub mod sunset_combat_remote {\n'+swap+'\n}}}\n'
    code+=r'''
use game::{Game,Remote};
use game::remote::sunset_combat_remote::swap_unpredicted;
use horizon_protocol::{buf::Reader,packets::play::Serverbound as S};
fn game()->Game<'static>{Game{carried:None,remote:Some(Remote{state_id:912345}),menu:(0,"inventory".into()),local:(0..41).collect(),sent:vec![],life:std::marker::PhantomData}}
#[test]fn actual_protocol_round_trip_has_current_state_and_no_predicted_changes(){
 let mut g=game();let local=g.local.clone();let menu=g.menu.clone();
 assert!(swap_unpredicted(&mut g,&menu,17,504));assert_eq!(g.local,local);
 let packet=&g.sent[0];let S::ContainerClick(p)=packet else{panic!("wrong native operation")};
 assert_eq!((p.container,p.state,p.slot,p.button,p.input),(0,912345,17,40,2));
 assert!(p.changed.is_empty()&&p.carried.is_none());
 let bytes=packet.encode();let mut reader=Reader::new(&bytes);let id=reader.varint().unwrap();
 assert_eq!(&S::decode(id,reader.rest()).unwrap(),packet);
}
#[test]fn hotbar_refill_uses_the_real_number_key_button_and_current_container(){
 let mut g=game();g.menu=(7,"minecraft:generic_9x3".into());let menu=g.menu.clone();
 assert!(swap_unpredicted(&mut g,&menu,300,8));
 let S::ContainerClick(p)=&g.sent[0] else{panic!()};
 assert_eq!((p.container,p.state,p.slot,p.button,p.input),(7,912345,0,8,2));
 assert!(p.changed.is_empty()&&p.carried.is_none());
}
#[test]fn stale_menu_carried_stack_invalid_slot_and_missing_session_send_nothing(){
 let mut g=game();let menu=g.menu.clone();g.carried=Some(1);
 assert!(!swap_unpredicted(&mut g,&menu,17,504));g.carried=None;
 assert!(!swap_unpredicted(&mut g,&(3,"inventory".into()),17,504));
 for (from,to) in [(17,17),(17,9),(17,40),(99,504),(65536,504)]{assert!(!swap_unpredicted(&mut g,&menu,from,to));}
 g.remote=None;assert!(!swap_unpredicted(&mut g,&menu,17,504));assert!(g.sent.is_empty());
}
#[test]fn no_reply_one_reply_or_duplicate_reply_cannot_confirm_the_request(){
 let mut g=game();let menu=g.menu.clone();let mut ack=policy::Swap::new([17,504],[Some(77u64),None],100).unwrap();
 assert!(swap_unpredicted(&mut g,&menu,17,504));assert_eq!(ack.status(101,30),policy::Status::Waiting);
 ack.observe(17,&None);ack.observe(17,&None);assert_eq!(ack.confirmations(),1);
 assert_eq!(ack.status(102,30),policy::Status::Waiting);ack.observe(504,&Some(77));
 assert_eq!(ack.confirmations(),3);assert_eq!(ack.status(103,30),policy::Status::Confirmed);
 ack.observe(504,&Some(78));assert_eq!(ack.status(104,30),policy::Status::Rejected);
}
'''
    with tempfile.TemporaryDirectory(prefix='sunset-native-inventory-protocol-') as temp:
        rust=Path(temp)/'protocol_fields.rs';binary=Path(temp)/'protocol_fields.exe';rust.write_text(code,encoding='utf-8')
        subprocess.run(['rustc','--edition=2024','--test',str(rust),'--extern','horizon_protocol='+str(protocols[0]),'-L','dependency='+str(a.dependency_dir),'-o',str(binary)],check=True)
        subprocess.run([str(binary)],check=True)

if __name__=='__main__':main()
