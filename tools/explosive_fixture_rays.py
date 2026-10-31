"""Offline, exact pinned geometry rays for the explosive-aura fixture.

This uses real Horizon VoxelShape/clip_world traversal and production exposure
samples/raw damage. It supplies explicit cube/snow shapes, not a live client's
registry or state. No game, network, credentials or native build is involved.
"""
from pathlib import Path
import argparse
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("engine", type=Path)
    args = parser.parse_args()
    engine = args.engine.resolve()
    revision = (engine / ".horizon-revision").read_text().strip()
    if revision != "ba6344a59cb9e755b730dcf02285fc0d33b97d3c":
        raise SystemExit("Expected pinned Horizon ba6344a source")
    repo = Path(__file__).resolve().parents[1]
    paths = {
        "geometry": engine / "crates/horizon-core/src/geometry/mod.rs",
        "world_ray": engine / "crates/horizon-world/src/world_ray.rs",
        "combat_rules": engine / "crates/horizon-world/src/combat_rules.rs",
        "policy": repo / "mixins/sunset/explosive_policy.rs",
    }
    code = "#![allow(dead_code, unused_imports)]\nextern crate self as horizon_core;\n"
    code += "\n".join(f'#[path="{str(path).replace(chr(92), "/")}"] mod {name};'
                      for name, path in paths.items())
    position = (engine / "crates/horizon-core/src/position.rs").read_text()
    iterator = str(engine / "crates/horizon-core/src/position/iter.rs").replace("\\", "/")
    position = position.replace("mod iter;", f'#[path="{iterator}"] mod iter;')
    code += "\nmod position {\n" + position.replace("//!", "//") + "\n}\n"
    code += r'''
mod entity_obstruction { #[derive(Clone,Copy)] pub struct EntityIdentity(pub u64); }
use geometry::{Vec3,Aabb,VoxelShape};
use position::BlockPos;
use world_ray::{RayWorld,RayBlockMode,RayFluidMode,clip_world};
use std::collections::HashMap;
struct Fixture { blocks:HashMap<[i32;3],u8>, ignored:Option<[i32;3]> }
impl RayWorld for Fixture {
    type Error=std::convert::Infallible; type Block=u8; type Fluid=();
    fn block_state(&mut self,p:BlockPos)->Result<u8,Self::Error> {
        Ok(if self.ignored==Some([p.x,p.y,p.z]) {0} else {*self.blocks.get(&[p.x,p.y,p.z]).unwrap_or(&0)})
    }
    fn fluid_state(&mut self,_:BlockPos)->Result<(),Self::Error>{Ok(())}
    fn block_shape(&mut self,b:u8,_:BlockPos,_:RayBlockMode)->Result<VoxelShape,Self::Error>{
        Ok(match b {
            1=>VoxelShape::block(),
            2=>VoxelShape::from_box(Aabb::new(Vec3::new(0.0,0.0,0.0),Vec3::new(1.0,0.25,1.0))).unwrap(),
            3=>VoxelShape::from_box(Aabb::new(Vec3::new(0.0,0.0,0.0),Vec3::new(1.0,0.875,1.0))).unwrap(),
            _=>VoxelShape::empty(),
        })
    }
    fn interaction_shape(&mut self,_:u8,_:BlockPos)->Result<VoxelShape,Self::Error>{Ok(VoxelShape::empty())}
    fn fluid_is_source(&mut self,_:())->Result<bool,Self::Error>{Ok(false)}
    fn fluid_is_empty(&mut self,_:())->Result<bool,Self::Error>{Ok(true)}
    fn fluid_is_water(&mut self,_:())->Result<bool,Self::Error>{Ok(false)}
    fn fluid_shape(&mut self,_:(),_:BlockPos)->Result<VoxelShape,Self::Error>{Ok(VoxelShape::empty())}
}
fn vector(p:[f64;3])->Vec3 { Vec3::new(p[0],p[1],p[2]) }
fn ray(world:&mut Fixture,from:[f64;3],to:[f64;3])->bool {
    clip_world(world,vector(from),vector(to),RayBlockMode::Collider,RayFluidMode::None,4096).unwrap().miss
}
fn exposure(world:&mut Fixture,feet:[f64;3],center:[f64;3],height:f64)->(usize,usize,f32) {
    let width=f64::from(0.6f32)/2.0;
    let points=policy::samples([feet[0]-width,feet[1],feet[2]-width],
        [feet[0]+width,feet[1]+height,feet[2]+width]).unwrap();
    let seen=points.iter().filter(|&&p|ray(world,p,center)).count();
    let distance=(0..3).map(|i|(feet[i]-center[i]).powi(2)).sum::<f64>().sqrt();
    (seen,points.len(),policy::raw_damage(distance,6.0,seen as f32/points.len() as f32).unwrap())
}
struct Armor;
impl combat_rules::ArmorWorld for Armor {
    type Error=std::convert::Infallible; type Weapon=(); type ServerLevel=();
    fn weapon(&mut self)->Result<Option<()>,Self::Error>{Ok(None)}
    fn server_level(&mut self)->Result<Option<()>,Self::Error>{Ok(None)}
    fn modify_armor_effectiveness(&mut self,_:(),_:(),v:f32)->Result<f32,Self::Error>{Ok(v)}
}
fn main() {
    for base_y in [120,121,122] {
        for cover in [false,true] {
            for snow in [0,2,3] {
                let base=[4,base_y,0];
                let mut world=Fixture { blocks:HashMap::new(),ignored:None };
                // Continuous floor underneath both actors; replace one floor
                // cell by pedestal when base_y=121.
                for x in -9..=11 {for z in -9..=9 {world.blocks.insert([x,120,z],1);}}
                world.blocks.insert(base,1);
                if cover {for z in -2..=2 {world.blocks.insert([1,121,z],1);}}
                if snow>0 {for z in -2..=2 {world.blocks.insert([1,122,z],snow);}}
                let center=[4.5,f64::from(base_y)+1.0,0.5];
                let own=exposure(&mut world,[0.5,121.0,0.5],center,f64::from(1.8f32));
                let target=exposure(&mut world,[5.5,121.0,0.5],center,f64::from(1.95f32));
                let target_armor=combat_rules::after_armor(&mut Armor,target.2,2.0,0.0).unwrap();
                if base_y==120 && cover && snow==0 {
                    let eye=[0.5,122.62000000476836,0.5];
                    let foot=clip_world(&mut world,vector(eye),vector(center),
                        RayBlockMode::Collider,RayFluidMode::None,4096).unwrap();
                    let aim=[center[0],center[1]+1.0,center[2]];
                    let middle=clip_world(&mut world,vector(eye),vector(aim),
                        RayBlockMode::Collider,RayFluidMode::None,4096).unwrap();
                    // Contrary to the initial hypothesis, this exact endpoint
                    // is a MISS in the pinned kernel. Do not invent a blocked
                    // ray regression to explain an unconfirmed live failure.
                    assert!(foot.miss);
                    assert!(middle.miss);
                    // Pinned crystal bounds are (x-1,y,z-1)..(x+1,y+2,z+1):
                    // y+1 is the box's centre, while the blast stays at feet.
                    assert_eq!(aim,[4.5,122.0,0.5]);
                    println!("BREAK_REGRESSION foot_los={} hit_block={:?} hit_location={:?} box_center_los={} blast_center={center:?} aim={aim:?}",
                        foot.miss,foot.hit.block,foot.hit.location,middle.miss);
                }
                world.ignored=Some(base);
                let los=ray(&mut world,[0.5,122.62000000476836,0.5],center);
                if cover {
                    match (base_y,snow) {
                        (120,0)=>{assert_eq!((own.0,target.0,los),(3,45,true));},
                        (121,0)=>{assert_eq!((own.0,target.0,los),(18,18,true));},
                        (121,2)=>{assert_eq!((own.0,target.0,los),(9,18,true));},
                        (122,3)=>{assert_eq!((own.0,target.0,los),(0,0,false));},
                        _=>{},
                    }
                }
                println!("base_y={base_y} cover={cover} snowkind={snow} own={}/{} normal={:.6} hard_upper={:.6} target={}/{} raw={:.6} armor2={:.6} place_los={los}",
                    own.0,own.1,own.2,own.2*1.5,target.0,target.1,target.2,target_armor);
            }
        }
    }
}
'''
    with tempfile.TemporaryDirectory(prefix="sunset-fixture-rays-") as temp:
        rust = Path(temp) / "fixture_rays.rs"
        binary = Path(temp) / "fixture_rays.exe"
        rust.write_text(code)
        subprocess.run(["rustc", "--edition=2024", str(rust), "-o", str(binary)], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
