#!/usr/bin/env python3
"""Standalone upstream-vs-current discovery and picker regression tests."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def picker_source(source):
    source = source.replace("use super::Game;", "").replace("use super::sunset_util::short;", "use crate::short;")
    start = source.index("pub(super) fn open(")
    body = source.index("    let ", start)
    end = source.index("    };", body) + len("    };")
    owned = "Vec<Box<str>>" in source[body:end]
    replacement = "    let mut names: Vec<Box<str>> = ids.into_iter().map(Into::into).collect();" if owned else "    let names = ids;"
    source = source[:start] + "pub(super) fn create(ids: Vec<&str>, kind: ListKind) -> Picker {\n" + replacement + source[end:]
    return source


def world_source(source, current):
    fields = source[source.index("#[derive(Default)]\nstruct Ws"):source.index("pub(super) fn tick(")]
    functions = source[source.index("fn spawner_label("):source.index("#[allow(dead_code)]", source.index("fn spawner_label("))]
    reset_extra = "w.spawner_seen.clear();" if current else ""
    wrappers = """
pub fn run(game: &mut Game<'_>, range: f64) { spawner_alerts(game, &Module(range)); }
pub fn draw(game: &Game<'_>, range: f64) -> Vec<Shape> { let mut out = Vec::new(); spawner_labels(game, &Camera, range, &mut out); out }
pub fn entries() -> Vec<([i32;3],String)> { ws(|w| w.spawners.clone()) }
pub fn reset() { ws(|w| { *w = Ws::default(); }); }
pub fn clear_discoveries() { ws(|w| { w.spawners.clear(); RESET_EXTRA }); }
""".replace("RESET_EXTRA", reset_extra)
    return "use crate::*;\n" + fields + functions + wrappers


HARNESS = r'''
#![allow(dead_code, unused_imports, private_bounds, private_interfaces)]
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};
use std::marker::PhantomData;
fn short(name: &str) -> &str { name.strip_prefix("minecraft:").unwrap_or(name) }
fn dist(a:[f64;3],b:[f64;3])->f64 { ((a[0]-b[0]).powi(2)+(a[1]-b[1]).powi(2)+(a[2]-b[2]).powi(2)).sqrt() }
#[derive(Clone,Hash,Eq,PartialEq)] struct JavaString(String);
impl From<&str> for JavaString { fn from(s:&str)->Self { Self(s.into()) } }
#[derive(Clone)] struct Utf16(Vec<u16>);
#[derive(Clone)] enum Tag { String(Utf16), Other }
#[derive(Clone,Default)] struct SpawnData { entity:HashMap<JavaString,Tag> }
#[derive(Clone)] struct Spawner(Option<SpawnData>);
impl Spawner { fn next_data(&self)->Option<&SpawnData> { self.0.as_ref() } }
#[derive(Clone)] struct Trial { next_spawn_data:Option<SpawnData> }
#[derive(Clone)] enum BlockEntity { Spawner(Spawner),TrialSpawner(Trial),Chest }
struct Position { x:f64,y:f64,z:f64 }
struct Player { position:Position }
#[derive(Default)] struct World { block_entities:HashMap<(i32,i32,i32),BlockEntity> }
struct Game<'a> { player:Option<Player>,ticks:u64,world:World,messages:Vec<String>,_marker:PhantomData<&'a ()> }
impl Game<'_> { fn native_text_width(&self,text:&str)->f32 { text.chars().count() as f32 * 6.0 } }
struct Module(f64);
impl Module { fn n(&self,_:&str)->f64 { self.0 } }
struct Camera;
impl Camera { fn project(&self,p:[f64;3])->Option<[f32;2]> { (p[1] > -5.0).then_some([p[0] as f32*2.0,p[2] as f32*2.0]) } }
#[derive(Debug,PartialEq)] enum Shape { Fill([f32;4],u32),Text(String,[f32;2],u32) }
trait Canvas { fn fill(&mut self,r:[f32;4],color:u32);fn text(&mut self,text:&str,p:[f32;2],color:u32); }
impl Canvas for Vec<Shape> { fn fill(&mut self,r:[f32;4],color:u32) { self.push(Shape::Fill(r,color)); } fn text(&mut self,text:&str,p:[f32;2],color:u32) { self.push(Shape::Text(text.into(),p,color)); } }
fn say(game:&mut Game<'_>,line:&str) { game.messages.push(line.into()); }
mod picker_current { include!(env!("PICKER_CURRENT")); }
mod picker_original { include!(env!("PICKER_ORIGINAL")); }
mod world_current { include!(env!("WORLD_CURRENT")); }
mod world_original { include!(env!("WORLD_ORIGINAL")); }
static LOCK:Mutex<()> = Mutex::new(());

#[test]
fn picker_keeps_upstream_order_search_case_and_unicode_data() {
    let names=vec!["air","coal_ore","dirt","diamond_ore","coal_ore","cave_air","void_air","_diamond__ore_","ß_block","étoile","test:item","@passive"];
    for (old_kind,new_kind) in [(picker_original::ListKind::Blocks,picker_current::ListKind::Blocks),(picker_original::ListKind::Items,picker_current::ListKind::Items),(picker_original::ListKind::Entities,picker_current::ListKind::Entities)] {
        let old=picker_original::create(names.clone(),old_kind);
        let new=picker_current::create(names.clone(),new_kind);
        assert_eq!(old.all,new.all);
        for selected in ["","dirt","dirt, minecraft:diamond_ore","@all,dirt,dirt,missing_id"] {
            for search in ["","ORE"," Diamond O ","ß","SS B","ÉTOILE",":item","@pass","not present"] {
                assert_eq!(picker_original::rows(&old,selected,search),picker_current::rows(&new,selected,search),"selected={selected} search={search}");
            }
        }
    }
    for name in names { assert_eq!(picker_original::readable(name),picker_current::readable(name)); }
}

fn entity(id:Option<&str>,trial:bool)->BlockEntity {
    let data=id.map(|id| SpawnData { entity:HashMap::from([(JavaString::from("id"),Tag::String(Utf16(id.encode_utf16().collect())))]) });
    if trial { BlockEntity::TrialSpawner(Trial { next_spawn_data:data }) } else { BlockEntity::Spawner(Spawner(data)) }
}

#[test]
fn spawner_discoveries_messages_and_label_shapes_match_upstream_sequence() {
    let _lock=LOCK.lock().unwrap();
    world_original::reset();world_current::reset();
    let mut game=Game { player:Some(Player { position:Position { x:0.5,y:64.5,z:0.5 } }),ticks:0,world:World::default(),messages:Vec::new(),_marker:PhantomData };
    for x in 0..500 { game.world.block_entities.insert((x,64,2),BlockEntity::Chest); }
    for (position,kind) in [((1,64,1),entity(Some("minecraft:zombie_piglin"),false)),((3,64,1),entity(Some("custom:trial_mob"),true)),((40,64,0),entity(None,false)),((2,-20,0),entity(None,true))] { game.world.block_entities.insert(position,kind); }
    for (tick,range) in [(1,64.0),(10,8.0),(20,64.0),(30,8.0),(40,256.0)] {
        game.ticks=tick;
        world_original::run(&mut game,range);
        let expected=std::mem::take(&mut game.messages);
        world_current::run(&mut game,range);
        assert_eq!(game.messages,expected,"tick={tick}");game.messages.clear();
        assert_eq!(world_original::entries(),world_current::entries());
        assert_eq!(world_original::draw(&game,range),world_current::draw(&game,range));
    }
    game.world.block_entities.insert((1,64,1),entity(Some("minecraft:skeleton"),false));
    game.ticks=50;
    world_original::run(&mut game,64.0);world_current::run(&mut game,64.0);
    assert!(game.messages.is_empty(),"changing metadata must preserve upstream once-only behavior");
    world_original::clear_discoveries();world_current::clear_discoveries();
    world_original::run(&mut game,64.0);let expected=std::mem::take(&mut game.messages);
    world_current::run(&mut game,64.0);assert_eq!(game.messages,expected);
    assert_eq!(world_original::draw(&game,64.0),world_current::draw(&game,64.0));
    game.player=None;
    assert_eq!(world_original::draw(&game,64.0),world_current::draw(&game,64.0));
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rustc", default="D:/Codex-Meteor-Build/toolchains/1.99.1-x86_64-pc-windows-msvc/bin/rustc.exe")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="sunset-discovery-perf-") as directory:
        directory = Path(directory)
        env = os.environ.copy()
        for name in ["picker", "world"]:
            current = (root / "mixins/sunset" / (name + ".rs")).read_text(encoding="utf-8")
            original = subprocess.check_output(["git", "show", "23bdc11:mixins/sunset/" + name + ".rs"], cwd=root, text=True)
            for suffix, source in [("current", current), ("original", original)]:
                code = picker_source(source) if name == "picker" else world_source(source, suffix == "current")
                path = directory / f"{name}_{suffix}.rs"
                path.write_text(code, encoding="utf-8")
                env[f"{name}_{suffix}".upper()] = str(path)
        source = directory / "tests.rs"
        source.write_text(HARNESS, encoding="utf-8")
        binary = directory / ("tests.exe" if os.name == "nt" else "tests")
        subprocess.run([args.rustc, "--edition=2024", "--test", "-O", str(source), "-o", str(binary)], env=env, check=True)
        subprocess.run([str(binary), "--nocapture"], check=True)


if __name__ == "__main__":
    main()
