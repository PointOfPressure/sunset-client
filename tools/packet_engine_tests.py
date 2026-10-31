"""Compile production timing predicates with packet shapes from pinned source.

No client/connection starts. Packet enum shapes are mechanically read from the
native protocol source; tests cover whitelist/barrier/replay selection and typed
payload preservation, not live network framing or encryption.
"""
from pathlib import Path
import argparse
import re
import subprocess
import tempfile


def block(text, start):
    opening = text.index("{", start)
    depth, cursor = 1, opening + 1
    while depth:
        depth += (text[cursor] == "{") - (text[cursor] == "}")
        cursor += 1
    return text[start:cursor]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("engine", type=Path)
    args = p.parse_args()
    engine = args.engine.resolve()
    if (engine / ".horizon-revision").read_text(encoding="utf-8").strip() != "ba6344a59cb9e755b730dcf02285fc0d33b97d3c":
        raise SystemExit("Expected pinned native source ba6344a")
    here = Path(__file__).resolve().parents[1] / "mixins/sunset"
    production = (here / "packet_delay.rs").read_text(encoding="utf-8")
    native = (engine / "crates/horizon-protocol/src/packets/play.rs").read_text(encoding="utf-8")
    primitives = []
    for name in ["MovePlayerPos", "MovePlayerPosRot", "MovePlayerRot", "MovePlayerStatus", "SetEntityMotion"]:
        match = re.search(r"\b" + name + r"\s*\{(.*?)\}\s*\);", native, re.S)
        if not match:
            raise SystemExit(f"Missing native schema {name}")
        fields = re.sub(r"\s*=>\s*[A-Za-z0-9_:<>]+", "", match[1])
        primitives.append(f"#[derive(Clone,Debug,PartialEq)] struct {name} {{{fields}}}")
    serverbound = block(native, native.index("packet_enum!(Serverbound"))
    variants = re.findall(r"^\s*(\w+)\([^\n]*?\)\s*=", serverbound, re.M)
    movements = {"MovePlayerPos", "MovePlayerPosRot", "MovePlayerRot", "MovePlayerStatus"}
    enum = "#[derive(Clone,Debug,PartialEq)] enum S {" + ",".join(
        f"{v}({v if v in movements else '()'})" for v in variants) + ",Unknown(i32,Vec<u8>)}"
    explode = block(native, native.index("pub struct Explode"))
    explode = explode.replace("pub struct", "struct")
    particle = "#[derive(Clone,Debug,PartialEq)] struct Particle { payload:Vec<u8> }"
    explosion_particle = block(native, native.index("pub struct ExplosionParticle")).replace("pub struct", "struct")
    helpers = "\n".join(block(production, production.index("fn " + name + "("))
                        for name in ["movement", "barrier", "position", "valid_movement", "pushes_self", "self_motion_event"])
    policy = str(here / "packet_policy.rs").replace("\\", "/")
    code = f'#![allow(dead_code)]\n#[path="{policy}"]mod policy;\n'
    code += "\n".join(primitives) + "\n" + enum + "\n" + particle
    code += "\n#[derive(Clone,Debug,PartialEq)]\n" + explosion_particle
    code += "\n#[derive(Clone,Debug,PartialEq)]\n" + explode
    code += r'''
#[derive(Clone,Debug,PartialEq)] enum C {SetEntityMotion(SetEntityMotion),Explode(Explode),Other}
struct Remote {entity_id:i32}
struct Game<'a> {remote:Option<Remote>,_life:std::marker::PhantomData<&'a ()>}
#[test]fn only_the_four_native_player_movement_variants_can_be_held(){
    let packets=[S::MovePlayerPos(MovePlayerPos{x:1.25,y:63.75,z:-4.5,flags:3}),
        S::MovePlayerPosRot(MovePlayerPosRot{x:1.25,y:63.75,z:-4.5,yaw:179.0,pitch:-89.0,flags:3}),
        S::MovePlayerRot(MovePlayerRot{yaw:179.0,pitch:-89.0,flags:3}),
        S::MovePlayerStatus(MovePlayerStatus{flags:3})];
    let mut queue=policy::Queue::new(64,4096,1000);
    for (i,p) in packets.iter().enumerate(){assert!(movement(p)&&valid_movement(p));queue.push(p.clone(),i as u64,100,40).unwrap();}
    assert_eq!(queue.drain(1000,true),packets);
    for p in [S::MoveVehicle(()),S::Attack(()),S::UseItem(()),S::Unknown(999,vec![1,2,3])]{assert!(!movement(&p));}
}
#[test]fn gameplay_actions_flush_but_tickend_ping_keepalive_and_chat_do_not_collapse_delay(){
    for p in [S::Attack(()),S::UseItem(()),S::UseItemOn(()),S::PlayerAction(()),S::SetCarriedItem(()),S::ContainerClick(()),S::Unknown(999,vec![])] {
        assert!(barrier(&p));assert!(!movement(&p));
    }
    for p in [S::KeepAlive(()),S::Pong(()),S::PingRequest(()),S::ClientTickEnd(()),S::PlayerInput(()),S::Chat(()),S::ChatCommand(()),S::CookieResponse(()),S::ConfigurationAcknowledged(())] {
        assert!(!barrier(&p));assert!(!movement(&p));
    }
}
#[test]fn nonfinite_movement_is_never_held_and_flags_are_not_rewritten(){
    assert!(!valid_movement(&S::MovePlayerPos(MovePlayerPos{x:f64::NAN,y:0.0,z:0.0,flags:0})));
    assert!(!valid_movement(&S::MovePlayerRot(MovePlayerRot{yaw:f32::INFINITY,pitch:0.0,flags:0})));
    assert!(!valid_movement(&S::MovePlayerPos(MovePlayerPos{x:30_000_001.0,y:0.0,z:0.0,flags:0})));
    for flags in 0..=255 {assert!(valid_movement(&S::MovePlayerStatus(MovePlayerStatus{flags})));}
}
#[test]fn only_self_motion_and_optional_explosion_knockback_enter_incoming_queue(){
    let game=Game{remote:Some(Remote{entity_id:7}),_life:std::marker::PhantomData};
    assert!(pushes_self(&game,&C::SetEntityMotion(SetEntityMotion{id:7,velocity:[0.0,0.4,-0.6]}),true));
    assert!(!pushes_self(&game,&C::SetEntityMotion(SetEntityMotion{id:8,velocity:[0.0,0.4,-0.6]}),true));
    assert!(!pushes_self(&game,&C::SetEntityMotion(SetEntityMotion{id:7,velocity:[f64::NAN,0.4,-0.6]}),true));
    assert!(self_motion_event(&game,&C::SetEntityMotion(SetEntityMotion{id:7,velocity:[f64::NAN,0.4,-0.6]})),"ineligible latest self update is still a pending-order barrier");
    assert!(!self_motion_event(&game,&C::SetEntityMotion(SetEntityMotion{id:8,velocity:[0.0,0.4,-0.6]})));
    let blast=Explode{center:[1.0,2.0,3.0],radius:5.0,block_count:17,knockback:Some([0.1,0.3,0.2]),
        particle:Particle{payload:vec![4,5]},sound:(3,Some(("custom:blast".into(),Some(2.5)))),
        block_particles:vec![ExplosionParticle{particle:Particle{payload:vec![9]},scaling:1.5,speed:0.2,weight:4}],play_sound:true};
    let original=C::Explode(blast.clone());assert!(pushes_self(&game,&original,true));assert!(!pushes_self(&game,&original,false));
    assert!(self_motion_event(&game,&original),"disabled explosion option still flushes an older queued self update");
    let mut queue=policy::Queue::new(16,262144,1000);queue.push(original.clone(),0,250,128).unwrap();
    assert_eq!(queue.drain(250,false),[original],"native replay must preserve sound/particle/knockback metadata");
    let mut none=blast;none.knockback=None;assert!(!pushes_self(&game,&C::Explode(none),true));
    assert!(!pushes_self(&game,&C::Other,true));
}
'''
    code += "\n" + helpers
    with tempfile.TemporaryDirectory(prefix="sunset-packet-source-") as temp:
        rust = Path(temp) / "packet_source.rs"
        binary = Path(temp) / "packet_source.exe"
        rust.write_text(code)
        subprocess.run(["rustc", "--edition=2024", "--test", str(rust), "-o", str(binary)], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
