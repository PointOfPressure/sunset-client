#!/usr/bin/env python3
"""Test and measure production module snapshots without a game or Horizon build.

Extracts the exact pure Rust types/helpers from runtime.rs, includes the actual
bind_state.rs, then uses rustc --test. No running game is accessed.
"""
from pathlib import Path
import argparse
import os
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--rustc', default=os.environ.get('RUSTC', 'rustc'))
args = parser.parse_args()

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / 'mixins/sunset/runtime.rs').read_text()
start = source.index('#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub(super) enum Category')
end = source.index('// Every module with its defaults, in menu order.')
production = source[start:end]
scratch_start = source.index('#[derive(Default)]\nstruct EntityScratch')
scratch_end = source.index('// ESP, Tracers and Nametags in one pass over the mobs:', scratch_start)
production += source[scratch_start:scratch_end]


def function(signature):
    start = source.index(signature)
    opening = source.index('{', start)
    depth = 1
    cursor = opening + 1
    while depth:
        depth += (source[cursor] == '{') - (source[cursor] == '}')
        cursor += 1
    return source[start:cursor]


helpers = '\n'.join(function(s) for s in [
    'pub(super) fn on_off(', 'pub(super) fn num(v: f64)',
    'pub(super) fn round_to(', 'pub(super) fn norm(', 'pub(super) fn is_friend(',
])
binds = (ROOT / 'mixins/sunset/bind_state.rs').as_posix()
util = (ROOT / 'mixins/sunset/util.rs').read_text()
legacy_list = util[util.index('pub(super) fn short('):util.index('pub(super) fn toggle_off(')]
tests = r'''
#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::hint::black_box;
    use std::time::Instant;

    thread_local! { static ALLOCS: Cell<usize> = const { Cell::new(0) }; }
    struct Allocator;
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout); }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
            unsafe { System.realloc(ptr, layout, size) }
        }
    }
    #[global_allocator] static ALLOCATOR: Allocator = Allocator;

    fn fixture() -> Vec<Module> {
        (0..128).map(|i| {
            let name = Box::leak(format!("Module {i}").into_boxed_str());
            let mut module = Module::new(name, Category::Render, "benchmark module")
                .bool("Players", true)
                .num("Range", 64.0, 8.0, 256.0, 8.0)
                .pick("Mode", &["Default", "Potato"])
                .text("List", "minecraft:stone,minecraft:dirt");
            module.on = i % 8 == 0;
            module
        }).collect()
    }

    #[test]
    fn equal_values_reuse_the_snapshot_without_allocation() {
        let live = fixture();
        let mut cached = None;
        let old = shared_modules(&live, &mut cached);
        ALLOCS.with(|n| n.set(0));
        let next = shared_modules(&live, &mut cached);
        let allocations = ALLOCS.with(|n| n.get());
        assert!(Arc::ptr_eq(&old, &next));
        assert_eq!(allocations, 0);
    }

    #[test]
    fn every_setting_kind_keys_and_on_state_refresh_without_changing_old_readers() {
        let mut live = fixture();
        let mut cached = None;
        let old = shared_modules(&live, &mut cached);
        live[0].on = false;
        let next = shared_modules(&live, &mut cached);
        assert!(old.enabled("Module 0").is_some());
        assert!(next.enabled("Module 0").is_none());
        assert!(!Arc::ptr_eq(&old, &next));
        live[0].key = Some("key.keyboard.r".into());
        let next_key = shared_modules(&live, &mut cached);
        assert_eq!(next_key.module("Module 0").unwrap().key.as_deref(), Some("key.keyboard.r"));
        for (setting, text) in [(0, "false"), (1, "96"), (2, "Potato"), (3, "minecraft:sand")] {
            assert!(live[0].settings[setting].parse(text));
            let next = shared_modules(&live, &mut cached);
            assert_eq!(next[0], live[0]);
            assert_ne!(next[0], old[0]);
        }
    }

    #[test]
    fn registration_removal_and_reordering_refresh_the_lookup() {
        let mut live = fixture();
        let mut cached = None;
        shared_modules(&live, &mut cached);
        live.push(Module::new("Plugin", Category::World, "plugin").on());
        assert!(shared_modules(&live, &mut cached).enabled("Plugin").is_some());
        live.swap(0, 128);
        assert_eq!(shared_modules(&live, &mut cached).module("Plugin").unwrap().name, "Plugin");
        live.remove(0);
        assert!(shared_modules(&live, &mut cached).module("Plugin").is_none());
    }

    #[test]
    fn lookup_matches_the_existing_linear_dispatch() {
        let live = fixture();
        let shared = SharedModules::new(&live);
        for name in live.iter().map(|m| m.name).chain(["Unknown"]) {
            assert_eq!(shared.module(name), live.iter().find(|m| m.name == name));
            assert_eq!(shared.enabled(name), live.iter().find(|m| m.name == name && m.on));
        }
        fn accepts_slice(_: &[Module]) {}
        accepts_slice(&Arc::new(shared));
    }

    #[test]
    fn repeated_key_down_does_not_allocate() {
        let mut inputs = crate::binds::Inputs::default();
        assert!(inputs.press("key.keyboard.r"));
        ALLOCS.with(|n| n.set(0));
        assert!(!inputs.press("key.keyboard.r"));
        assert_eq!(ALLOCS.with(|n| n.get()), 0);
    }

    static SCRATCH_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn friend_edits_refresh_while_readers_keep_their_own_view() {
        let mut live = vec!["ALICE".to_owned(), "Bob".to_owned()];
        let mut cached = None;
        let old = shared_friends(&live, &mut cached);
        let names = vec![(1, "Alice".to_owned()), (2, "bob".to_owned()), (3, "Eve".to_owned())];
        for id in 0..5 {
            let name = names.iter().find(|(candidate, _)| *candidate == id).map(|(_, name)| name.as_str());
            assert_eq!(friend_named(&old, name), is_friend(&old, &names, id));
        }
        ALLOCS.with(|n| n.set(0));
        let same = shared_friends(&live, &mut cached);
        assert_eq!(ALLOCS.with(|n| n.get()), 0);
        assert!(Arc::ptr_eq(&old, &same));
        live.remove(0);
        live.push("Eve".to_owned());
        let next = shared_friends(&live, &mut cached);
        assert!(friend_named(&old, Some("alice")));
        assert!(!friend_named(&next, Some("alice")));
        assert!(friend_named(&next, Some("EVE")));
        assert!(!friend_named(&next, None));
        live.clear();
        assert!(shared_friends(&live, &mut cached).is_empty());
    }

    #[test]
    fn scratch_reuses_capacity_and_nested_reads_never_share_live_buffers() {
        let _guard = SCRATCH_TEST_LOCK.lock().unwrap();
        *ENTITY_SCRATCH.lock().unwrap() = None;
        let mut outer = EntityScratch::take();
        outer.near.reserve(100);
        outer.near.push((4.0, 1, [1.0; 3], [2.0; 3]));
        outer.name.push_str("Outer");
        let mut nested = EntityScratch::take();
        assert!(nested.near.is_empty());
        nested.near.reserve(10);
        nested.name.push_str("Inner");
        assert_eq!(outer.name, "Outer");
        assert_eq!(outer.near.len(), 1);
        nested.recycle();
        let capacity = outer.near.capacity();
        outer.recycle();
        ALLOCS.with(|n| n.set(0));
        let mut next = EntityScratch::take();
        next.near.reserve(100);
        next.name.push_str("Label");
        assert_eq!(ALLOCS.with(|n| n.get()), 0);
        assert_eq!(next.near.capacity(), capacity);
        assert!(next.near.is_empty());
        assert_eq!(next.name, "Label");
        next.recycle();
    }

    #[test]
    fn entity_scratch_and_friend_snapshot_microbenchmark() {
        let _guard = SCRATCH_TEST_LOCK.lock().unwrap();
        let candidates: Vec<_> = (0..512).map(|i| (i as f64, i, [1.0; 3], [2.0; 3])).collect();
        let names: Vec<_> = (0..128).map(|i| (i, format!("Player{i}"))).collect();
        let friends: Vec<_> = (0..8).map(|i| format!("Player{i}")).collect();
        let mut cached = None;
        shared_friends(&friends, &mut cached);
        let mut warm = EntityScratch::take();
        warm.near.extend_from_slice(&candidates);
        warm.name.push_str("Player127");
        warm.hp.push_str("20");
        warm.tail.push_str(" 64m");
        warm.recycle();
        const RUNS: usize = 20_000;
        ALLOCS.with(|n| n.set(0));
        let start = Instant::now();
        for _ in 0..RUNS {
            let near = black_box(&candidates).clone();
            let copied_names = black_box(&names).clone();
            let copied_friends = black_box(&friends).clone();
            let buffers = (String::from("Player127"), String::from("20"), String::from(" 64m"));
            black_box((near, copied_names, copied_friends, buffers));
        }
        let cloned = start.elapsed();
        let clone_allocs = ALLOCS.with(|n| n.get());
        ALLOCS.with(|n| n.set(0));
        let start = Instant::now();
        for _ in 0..RUNS {
            let mut scratch = EntityScratch::take();
            scratch.near.extend_from_slice(black_box(&candidates));
            scratch.name.push_str("Player127");
            scratch.hp.push_str("20");
            scratch.tail.push_str(" 64m");
            let friends = shared_friends(black_box(&friends), &mut cached);
            black_box((&scratch, friends));
            scratch.recycle();
        }
        let shared = start.elapsed();
        let shared_allocs = ALLOCS.with(|n| n.get());
        println!("512 overlay candidates/128 profiles/8 friends, {RUNS} frames: fresh {cloned:?} ({clone_allocs} allocations), scratch+shared {shared:?} ({shared_allocs} allocations)");
        assert_eq!(shared_allocs, 0);
        assert!(clone_allocs > RUNS * 128);
    }

    #[test]
    fn parsed_item_lists_match_namespace_case_empty_and_filter_rules() {
        for source in ["", ",", " stone, minecraft:dirt ,,stone", "mod:stone, minecraft:STONE,Stone", "minecraft:minecraft:stone,\tstone\n"] {
            let parsed = ItemList::new(source);
            for item in ["", "stone", "minecraft:stone", "STONE", "Stone", "minecraft:STONE", "mod:stone", "minecraft:minecraft:stone", "dirt", "minecraft:dirt", "sand"] {
                for mode in ["All", "Whitelist", "Blacklist", "Unknown"] {
                    let old = mode == "All" || crate::legacy_list::listed(source, item) == (mode == "Whitelist");
                    assert_eq!(parsed.allows(mode, item), old, "source={source:?}, item={item:?}, mode={mode}");
                }
            }
        }
    }

    #[test]
    fn item_list_edits_refresh_immutable_readers_and_stable_text_allocates_nothing() {
        let mut cached = None;
        let old = shared_item_list("stone", &mut cached);
        ALLOCS.with(|n| n.set(0));
        let same = shared_item_list("stone", &mut cached);
        assert!(same.contains("minecraft:stone"));
        assert_eq!(ALLOCS.with(|n| n.get()), 0);
        assert!(Arc::ptr_eq(&old, &same));
        let next = shared_item_list("minecraft:dirt", &mut cached);
        assert!(old.contains("stone"));
        assert!(!next.contains("stone"));
        assert!(next.contains("dirt"));
        assert!(!Arc::ptr_eq(&old, &next));
    }

    #[test]
    fn item_labels_keep_custom_names_counts_and_translation_fallback() {
        let mut text = String::with_capacity(64);
        for count in [0, 1, 2, 64] {
            ALLOCS.with(|n| n.set(0));
            item_name_into(&mut text, Some("Custom \u{03a9}"), count, || panic!("custom names must bypass fallback"));
            assert_eq!(ALLOCS.with(|n| n.get()), 0);
            let expected = if count > 1 { format!("Custom \u{03a9} x{count}") } else { "Custom \u{03a9}".to_owned() };
            assert_eq!(text, expected);
        }
        item_name_into(&mut text, None, 3, || "Translated Item".to_owned());
        assert_eq!(text, "Translated Item x3");
        item_name_into(&mut text, Some(""), 1, || panic!("empty custom names still override fallback"));
        assert!(text.is_empty());
    }

    #[test]
    fn item_filter_microbenchmark() {
        let source = (0..256).map(|i| format!("minecraft:item_{i}")).collect::<Vec<_>>().join(", ");
        let items: Vec<_> = (0..512).map(|i| format!("minecraft:item_{i}")).collect();
        let mut cached = None;
        let parsed = shared_item_list(&source, &mut cached);
        const RUNS: usize = 400;
        let start = Instant::now();
        let mut old_matches = 0;
        for _ in 0..RUNS {
            for item in &items { old_matches += usize::from(crate::legacy_list::listed(black_box(&source), black_box(item))); }
        }
        let old = start.elapsed();
        ALLOCS.with(|n| n.set(0));
        let start = Instant::now();
        let mut new_matches = 0;
        for _ in 0..RUNS {
            let parsed = shared_item_list(black_box(&source), &mut cached);
            for item in &items { new_matches += usize::from(parsed.contains(black_box(item))); }
        }
        let new = start.elapsed();
        let allocations = ALLOCS.with(|n| n.get());
        assert_eq!(new_matches, old_matches);
        assert_eq!(allocations, 0);
        black_box(parsed);
        println!("256-entry ItemESP list, {} lookups: split/trim {old:?}, cached membership {new:?} ({allocations} warm allocations)", RUNS * items.len());
    }

    #[test]
    fn snapshot_microbenchmark() {
        let live = fixture();
        let mut cached = None;
        shared_modules(&live, &mut cached);
        const RUNS: usize = 20_000;
        ALLOCS.with(|n| n.set(0));
        let start = Instant::now();
        for _ in 0..RUNS { black_box(black_box(&live).clone()); }
        let cloned = start.elapsed();
        let clone_allocs = ALLOCS.with(|n| n.get());
        ALLOCS.with(|n| n.set(0));
        let start = Instant::now();
        for _ in 0..RUNS { black_box(shared_modules(black_box(&live), &mut cached)); }
        let shared = start.elapsed();
        let shared_allocs = ALLOCS.with(|n| n.get());
        println!("128 modules, 4 settings each, {RUNS} views: clone {cloned:?} ({clone_allocs} allocations), shared {shared:?} ({shared_allocs} allocations)");
        assert_eq!(shared_allocs, 0);
        assert!(clone_allocs > RUNS * 128);
    }
}
'''
with tempfile.TemporaryDirectory(prefix='sunset-runtime-perf-') as temporary:
    temporary = Path(temporary)
    rust = temporary / 'runtime_perf.rs'
    rust.write_text(f'#![allow(dead_code)]\n#[path = "{binds}"] mod binds;\nmod runtime {{\nuse std::sync::{{Arc, Mutex}};\nuse std::fmt::Write as _;\n' + production + helpers + tests + '\n}\nmod legacy_list {\n' + legacy_list + '\n}\n')
    binary = temporary / 'runtime_perf_tests.exe'
    subprocess.run([args.rustc, '--edition=2024', '--test', '-O', str(rust), '-o', str(binary)], check=True)
    subprocess.run([str(binary), '--nocapture'], check=True)
