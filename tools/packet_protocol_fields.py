"""Compile production packet predicates with the actual pinned native enum/codecs.

No socket, client or native project build. Checks protocol scope, ordering and
payload preservation using already-built pinned protocol artifacts.
"""
from pathlib import Path
import argparse,subprocess,tempfile

def function(source,signature):
    a=source.index(signature);b=source.index('{',a);depth=1
    for i in range(b+1,len(source)):
        depth+=(source[i]=='{')-(source[i]=='}')
        if not depth:return source[a:i+1]
    raise AssertionError(signature)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('engine',type=Path)
    p.add_argument('--dependency-dir',type=Path,required=True);a=p.parse_args()
    assert (a.engine/'.horizon-revision').read_text().strip()=='ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    root=Path(__file__).resolve().parents[1];source=(root/'mixins/sunset/packet_delay.rs').read_text(encoding='utf-8')
    deps=list(a.dependency_dir.glob('libhorizon_protocol-*.rlib'));assert len(deps)==1
    helpers='\n'.join(function(source,'fn '+name+'(') for name in ['movement','barrier','stale_motion','position','valid_movement'])
    code=r'''
#![allow(dead_code)]
use horizon_protocol::{buf::Reader,packets::{Empty,play::{self,Serverbound as S},configuration}};
#[path="POLICY"]mod policy;
fn roundtrip(packet:&S){let bytes=packet.encode();let mut r=Reader::new(&bytes);
let id=r.varint().unwrap();assert_eq!(S::decode(id,r.rest()).unwrap(),*packet);}
#[test]fn real_keepalive_input_rotation_and_original_tick_end_share_fifo_unchanged(){
 let packets=[S::KeepAlive(configuration::KeepAlive{id:0x0102030405060708}),
 S::PlayerInput(play::PlayerInput{flags:0b01100101}),
 S::MovePlayerRot(play::MovePlayerRot{yaw:179.5,pitch:-89.25,flags:3}),
 S::ClientTickEnd(Empty),S::PingRequest(horizon_protocol::packets::status::Ping{time:993})];
 let mut queue=policy::Queue::new(64,65536,1000);
 for packet in &packets{assert!(!barrier(packet));roundtrip(packet);queue.push(packet.clone(),10,100,packet.encode().len()).unwrap();}
 assert_eq!(queue.drain(110,false),packets);
}
#[test]fn exact_actions_flush_and_carried_slot_change_is_a_non_action(){
 for packet in [S::Attack(play::Attack{id:27}),S::Punch(Empty),
 S::PlayerAction(play::PlayerAction{action:7,pos:[0;3],direction:0,sequence:0}),
 S::ContainerClose(play::ContainerClose{container:0}),
 S::Unknown(horizon_protocol::ids::play::serverbound::COMMAND_SUGGESTION,vec![1,0])]{assert!(barrier(&packet));}
 let packet=S::SetCarriedItem(play::SetCarriedItem{slot:8});assert!(!barrier(&packet));roundtrip(&packet);
}
#[test]fn correction_preserves_non_motion_control_and_original_boundaries(){
 let keepalive=S::KeepAlive(configuration::KeepAlive{id:77});let boundary=S::ClientTickEnd(Empty);
 let old=S::MovePlayerPos(play::MovePlayerPos{x:1.,y:120.,z:3.,flags:1});
 let mut queue=policy::Queue::new(64,65536,1000);
 for packet in [old.clone(),keepalive.clone(),boundary.clone()]{queue.push(packet,0,900,40).unwrap();}
 assert_eq!(queue.discard_except(|packet|!stale_motion(packet)),[keepalive,boundary]);
 assert_eq!((queue.dropped,queue.released),(1,2));
}
#[test]fn due_prefix_and_capacity_do_not_let_actions_or_new_controls_overtake(){
 let first=S::KeepAlive(configuration::KeepAlive{id:1});let second=S::PlayerInput(play::PlayerInput{flags:4});
 let mut queue=policy::Queue::new(2,100,1000);queue.push(first.clone(),0,500,9).unwrap();
 queue.push(second.clone(),1,1,2).unwrap();assert!(queue.drain(499,false).is_empty());
 assert!(queue.needs_flush(499,1));assert_eq!(queue.drain(499,true),[first,second]);
}
'''.replace('POLICY',(root/'mixins/sunset/packet_policy.rs').as_posix())+'\n'+helpers
    with tempfile.TemporaryDirectory(prefix='sunset-native-packet-parity-') as temp:
        rust=Path(temp)/'packet.rs';binary=Path(temp)/'packet.exe';rust.write_text(code,encoding='utf-8')
        subprocess.run(['rustc','--edition=2024','--test',str(rust),'--extern','horizon_protocol='+str(deps[0]),
                        '-L','dependency='+str(a.dependency_dir),'-o',str(binary)],check=True)
        subprocess.run([str(binary)],check=True)
if __name__=='__main__':main()
