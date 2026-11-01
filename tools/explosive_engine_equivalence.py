"""Offline checks against pinned native engine source; no client/server launch.

Usage: python tools/explosive_engine_equivalence.py /path/to/pristine-horizon-ba
The temporary rustc harness compiles the *actual* native combat/damage kernels
and the production Defenses adapter. An instrumented clip callback records the
actual engine exposure routine's sample order. It does not simulate ray shapes
or establish live server equivalence; native compilation checks that dispatch.
"""
from pathlib import Path
import argparse
import re
import subprocess
import tempfile


def block(text, start):
    opening = text.index("{", start)
    depth = 1
    cursor = opening + 1
    while depth:
        depth += (text[cursor] == "{") - (text[cursor] == "}")
        cursor += 1
    return text[start:cursor]


def source(path):
    return "\n".join(line for line in path.read_text(encoding="utf-8").splitlines()
                     if not line.startswith("//!"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("engine", type=Path)
    args = parser.parse_args()
    engine = args.engine.resolve()
    revision_file = engine / ".horizon-revision"
    revision = (revision_file.read_text(encoding="utf-8").strip() if revision_file.exists() else
                subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=engine, text=True).strip())
    if revision != "ba6344a59cb9e755b730dcf02285fc0d33b97d3c":
        raise SystemExit(f"Expected pinned Horizon ba6344a, got {revision}")
    repo = Path(__file__).resolve().parents[1]
    production = (repo / "mixins/sunset/explosive_damage.rs").read_text(encoding="utf-8")
    adapter = production[production.index("struct Defenses"):production.index("pub(super) fn local_damage")]
    difficulty = block(production, production.index("fn difficulty("))
    difficulty_id = block(production, production.index("fn difficulty_id("))
    upper_damage = block(production, production.index("fn upper_damage("))
    prediction = block(production, production.index("pub(super) struct Prediction")).replace("pub(super)", "pub")
    safety_damage = block(production, production.index("pub(super) fn safety_damage(")).replace("pub(super)", "pub")
    equipment_patch = block(production, production.index("fn known_equipment_patch("))
    native_buffer = source(engine / "crates/horizon-protocol/src/buf.rs")
    reader = block(native_buffer, native_buffer.index("pub struct Reader<'a>"))
    reader += "\nimpl<'a> Reader<'a> {\n" + "\n".join(
        block(native_buffer, native_buffer.index(signature)) for signature in
        ["pub fn new(data:", "pub fn remaining(", "pub fn bytes(", "fn array<const", "pub fn u8(", "pub fn varint(", "pub fn count("]
    ) + "\n}"
    protocol_item = source(engine / "crates/horizon-protocol/src/item.rs")
    constants = block(protocol_item, protocol_item.index("pub mod component"))
    protocol = ("\nmod horizon_protocol { pub mod buf { use crate::{Error,Result}; " + reader +
                " } pub mod item { " + constants + " } }\n")
    remote_source = source(engine / "crates/horizon-client/src/game/remote.rs")
    difficulty_packet = block(remote_source, remote_source.index("C::ChangeDifficulty(p)"))
    assignment = re.search(r"self\.ui\.difficulty = ([^;]+);", difficulty_packet)
    if assignment is None:
        raise SystemExit("Pinned ChangeDifficulty UI assignment was not found")
    remote_update = "fn native_remote_difficulty(packet:i32)->u8 { " + assignment[1].replace("p.difficulty", "packet") + " }"
    exposure = source(engine / "crates/horizon-client/src/game/explosion.rs")
    exposure = block(exposure, exposure.index("pub(super) fn seen_percent"))
    exposure = exposure.replace("pub(super) fn seen_percent(&self,", "fn native_seen(")
    initializer = block(exposure, exposure.index("let mut rays = ColliderRays")) + ";"
    exposure = exposure.replace(initializer, "let mut rays = ();")
    modules = "\n".join(f"mod {name} {{\n{source(engine / 'crates/horizon-world/src' / (name + '.rs'))}\n}}"
                         for name in ["combat_rules", "living_damage", "damage_reduction"])
    policy_path = str(repo / "mixins/sunset/explosive_policy.rs").replace("\\", "/")
    harness = r'''
#![allow(dead_code)]
use combat_rules::ArmorWorld;
use damage_reduction::{Bypass, DamageReductionWorld};
use std::marker::PhantomData;
#[derive(Debug)] struct Error(String);
impl Error { fn new(message:impl ToString)->Self { Self(message.to_string()) } }
type Result<T,E=Error> = std::result::Result<T,E>;
#[derive(Clone, Copy, PartialEq)] enum Scaling { Never, LivingNonPlayer, Always }
#[derive(Clone, Copy)] enum Difficulty { Peaceful, Easy, Normal, Hard }
struct DamageSource<'a, E> { bypass: [bool;4], marker: PhantomData<&'a E> }
impl<E> DamageSource<'_, E> {
    fn bypasses(&self, b:Bypass)->bool { self.bypass[match b {
        Bypass::Armor=>0,Bypass::Effects=>1,Bypass::Resistance=>2,Bypass::Enchantments=>3 }] }
}
fn defenses(bypass:[bool;4])->Defenses<'static> {
    Defenses { source:DamageSource { bypass,marker:PhantomData }, armor:20.0,
        toughness:8.0,resistance:Some(1),protection:8.0 }
}
#[derive(Clone,Copy,Debug,PartialEq)] struct Vec3([f64;3]);
impl Vec3 { fn new(x:f64,y:f64,z:f64)->Self { Self([x,y,z]) } }
enum RayBlockMode { Collider } enum RayFluidMode { None }
struct Hit { miss:bool }
thread_local! { static POINTS:std::cell::RefCell<Vec<[f64;3]>>=const { std::cell::RefCell::new(Vec::new()) }; }
fn clip_world(_: &mut (),from:Vec3,_:Vec3,_:RayBlockMode,_:RayFluidMode,_:usize)->Result<Hit,()> {
    POINTS.with(|p|p.borrow_mut().push(from.0));
    Ok(Hit { miss:from.0[0]>0.17 })
}
#[test] fn native_grid_matches_all_production_points_and_fraction() {
    for (lo,hi) in [([-0.3,0.0,-0.3],[0.3,1.8,0.3]),([0.0;3],[0.0;3]),
        ([-1.2,2.0,5.0],[0.2,2.3,6.4]),([14.0,-64.0,-5.0],[14.6,-62.5,-4.4])] {
        POINTS.with(|p|p.borrow_mut().clear());
        let fraction=native_seen([0.0;3],lo,hi);
        let ours=policy::samples(lo,hi).unwrap();
        let theirs=POINTS.with(|p|p.borrow().clone());
        assert_eq!(ours,theirs);
        assert_eq!(fraction,ours.iter().filter(|p|p[0]>0.17).count() as f32/ours.len() as f32);
    }
}
#[test] fn production_adapter_uses_native_armor_resistance_then_enchantment_kernels() {
    let actual=reduced(defenses([false;4]),40.0).unwrap();
    let armored=combat_rules::after_armor(&mut defenses([false;4]),40.0,20.0,8.0).unwrap();
    assert_eq!(armored,24.0);
    let expected=combat_rules::after_magic(armored*15.0/25.0,8.0);
    assert_eq!(actual,f64::from(expected));
    assert!((actual-9.792).abs()<0.00001);
    assert_eq!(reduced(defenses([true,true,false,false]),40.0),Some(40.0));
    assert_eq!(reduced(defenses([false,true,false,false]),40.0),Some(24.0));
    assert_eq!(reduced(defenses([true,false,true,true]),40.0),Some(40.0));
    let mut immune=defenses([false;4]); immune.resistance=Some(4);
    assert_eq!(reduced(immune,40.0),Some(0.0));
    let mut invalid=defenses([false;4]); invalid.armor=f64::NAN;
    assert!(reduced(invalid,40.0).is_none());
}
#[test] fn difficulty_scales_only_the_catalogs_always_sources() {
    assert_eq!(difficulty(40.0,Difficulty::Peaceful,Scaling::Always),0.0);
    assert_eq!(difficulty(40.0,Difficulty::Easy,Scaling::Always),21.0);
    assert_eq!(difficulty(40.0,Difficulty::Normal,Scaling::Always),40.0);
    assert_eq!(difficulty(40.0,Difficulty::Hard,Scaling::Always),60.0);
    assert_eq!(difficulty(1.0,Difficulty::Easy,Scaling::Always),1.0);
    for scaling in [Scaling::Never,Scaling::LivingNonPlayer] {
        assert_eq!(difficulty(40.0,Difficulty::Hard,scaling),40.0);
    }
}
#[test] fn remote_difficulty_uses_the_native_packet_ui_mapping_and_hard_safety_bound() {
    for packet in [-100,0,1,2,3,100] {
        let id=native_remote_difficulty(packet);
        let observed=difficulty_id(id);
        let expected=match packet { i32::MIN..=0=>0.0,1=>21.0,2=>40.0,_=>60.0 };
        assert_eq!(difficulty(40.0,observed,Scaling::Always),expected);
        // Regression: a Hard server's world.health_rules can remain Normal.
        assert_eq!(upper_damage(40.0,observed,Scaling::Always,true),60.0);
        assert_eq!(upper_damage(40.0,observed,Scaling::Never,true),40.0);
        assert_eq!(upper_damage(40.0,observed,Scaling::LivingNonPlayer,true),40.0);
    }
    assert_eq!(upper_damage(40.0,Difficulty::Normal,Scaling::Always,false),40.0);
    assert_eq!(upper_damage(40.0,Difficulty::Hard,Scaling::Always,false),60.0);
}
#[test] fn observed_equipment_safety_uses_hard_raw_damage_and_has_a_strict_unknown_mode() {
    let hard=upper_damage(40.0,Difficulty::Normal,Scaling::Always,true);
    assert_eq!(hard,60.0);
    let equipped=reduced(defenses([false;4]),hard).unwrap();
    let estimated=reduced(defenses([false;4]),40.0).unwrap();
    assert!(equipped>estimated && equipped<60.0);
    assert_eq!(safety_damage(Prediction {estimate:estimated,upper:60.0,equipped_upper:equipped},"Observed equipment"),equipped);
    for mode in ["Unmitigated","", "unknown"] {
        assert_eq!(safety_damage(Prediction {estimate:estimated,upper:60.0,equipped_upper:equipped},mode),60.0);
    }
    let survives=policy::armor_survives(hard,85,100,true,true,true);
    assert!(!survives);
    let fallback=if survives {equipped} else {60.0};
    assert_eq!(safety_damage(Prediction {estimate:estimated,upper:60.0,equipped_upper:fallback},"Observed equipment"),60.0);
}
#[test] fn unsupported_or_malformed_wire_components_cannot_receive_equipment_credit() {
    use horizon_protocol::item::component as c;
    assert!(known_equipment_patch(&[2,0,c::DAMAGE as u8,1,4,c::ENCHANTMENTS as u8,1,0]));
    assert!(known_equipment_patch(&[0,0]));
    for patch in [vec![1,0,c::MAX_DAMAGE as u8,1,5],vec![1,0,c::UNBREAKABLE as u8,0],
        vec![1,0,28,0],vec![0,1,c::MAX_DAMAGE as u8],vec![1,0,c::DAMAGE as u8,10,4],
        vec![1,0,c::DAMAGE as u8],vec![0,0,99],vec![]] {
        assert!(!known_equipment_patch(&patch),"{patch:?}");
    }
}
'''
    code = (harness + f'\n#[path="{policy_path}"] mod policy;\n' + modules +
            "\n" + adapter + "\n" + difficulty + "\n" + difficulty_id +
            "\n" + upper_damage + "\n" + remote_update + "\n" + exposure +
            "\n" + prediction + "\n" + safety_damage + "\n" + protocol + "\n" + equipment_patch)
    with tempfile.TemporaryDirectory(prefix="sunset-explosive-engine-") as temp:
        rust_file = Path(temp) / "engine_equivalence.rs"
        binary = Path(temp) / "engine_equivalence.exe"
        rust_file.write_text(code, encoding="utf-8")
        subprocess.run(["rustc", "--edition=2024", "--test", str(rust_file), "-o", str(binary)], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
