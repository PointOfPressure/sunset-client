"""Compile real macro component adapters with pinned native item/protocol APIs.

The production functions are extracted unchanged. Game is a read-only catalog
adapter; native ItemStack/Wire and Reader are real. No client or connection starts.
Startup/deferred route assertions are separately reported source contracts.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re
import subprocess
import tempfile


def function(text, signature):
    start = text.index(signature)
    opening = text.index('{', start)
    depth = 1
    for index in range(opening + 1, len(text)):
        depth += (text[index] == '{') - (text[index] == '}')
        if not depth:
            return text[start:index + 1]
    raise AssertionError(signature)


def source_contract(text):
    intercept = function(text, 'pub(super) fn intercept_use(')
    for later in ['sunset_combat_control::claim(owner)', 's.anchor_latched = true', 's.run = Some(Run']:
        assert intercept.index('policy::anchor_supplied') < intercept.index(later)
        assert intercept.index('guard_slot_at(game,target') < intercept.index(later)
    # Existing protected-anchor fallback suppression must precede supply failure.
    assert intercept.index('Blast::anchor(p)') < intercept.index('policy::anchor_supplied')
    queue = function(text, 'fn queue(')
    assert queue.index('&& select == selected') < queue.index('game.send(S::UseItemOn')
    assert queue.index('if let Some(selected)=select') < queue.index('game.send(S::UseItemOn')
    assert queue.index('game.selected != run.owned') < queue.index('if let Some(selected)=select')
    # Guard open-ray classification propagates unknown, rather than calling the
    # boolean visibility predicate and mistaking an error for protective cover.
    opened = function(text, 'fn guard_open_points(')
    assert 'known_line(game,point,blast,Some(anchor),budget)?' in opened


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('engine', type=Path)
    parser.add_argument('--dependency-dir', required=True, type=Path)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    revision = 'ba6344a59cb9e755b730dcf02285fc0d33b97d3c'
    assert (args.engine / '.horizon-revision').read_text().strip() == revision
    root = Path(__file__).resolve().parents[1]
    production = root / 'mixins/sunset/combat_macros.rs'
    text = production.read_text(encoding='utf-8')
    source_contract(text)
    print('source-route contract: 1 passed (macro supply preflight, fresh selected-hand route and unknown guard ray; not compiled)', flush=True)
    helpers = '\n'.join(function(text, signature) for signature in
                        ['fn stack(', 'fn known_click_patch(', 'fn click_facts(', 'fn crystal_route('])
    utility = (root / 'mixins/sunset/util.rs').read_text(encoding='utf-8')
    offhand = re.search(r'const OFFHAND: usize = (\d+);', utility)[1]
    code = r'''
#![allow(dead_code)]
extern crate horizon_items as horizon_client;
#[path="POLICY"] mod policy;
const OFFHAND:usize=OFFHAND_SLOT;
struct Game<'a>{selected:usize,inventory:Vec<Option<horizon_client::items::ItemStack>>,
 offhand:Option<horizon_client::items::ItemStack>,catalog:&'a horizon_client::items::Items}
impl Game<'_>{fn items(&self)->&horizon_client::items::Items{self.catalog}}
mod util{pub fn short(name:&str)->&str{name.strip_prefix("minecraft:").unwrap_or(name)}}
HELPERS
fn item()->horizon_client::items::ItemStack {
 horizon_client::items::ItemStack{item:"minecraft:diamond_sword".into(),count:1,damage:0,extra:None}
}
fn wire(bytes:Vec<u8>)->horizon_client::items::ItemStack {
 let mut stack=item();let original=stack.clone();
 stack.data_mut().wire=horizon_client::item_data::Wire::new(original,bytes.into());stack
}
#[test]fn native_plain_stack_is_known(){assert!(known_click_patch(&item()));}
#[test]fn unmodelled_component_addition_or_removal_cannot_borrow_prototype_use_facts(){
 assert!(!known_click_patch(&wire(vec![1,0,22,0])));
 assert!(!known_click_patch(&wire(vec![0,1,22])));
}
#[test]fn native_wire_parser_rejects_truncation_and_trailing_bytes(){
 for bytes in [vec![1,0,3,4,0],vec![0,0,1],vec![255]]{assert!(!known_click_patch(&wire(bytes)));}
}
#[test]fn damage_only_wire_patch_preserves_known_use_prototype(){
 assert!(known_click_patch(&wire(vec![1,0,3,1,0])));
}
'''.replace('POLICY', (root / 'mixins/sunset/macro_policy.rs').as_posix()).replace('OFFHAND_SLOT', offhand).replace('HELPERS', helpers)
    deps = args.dependency_dir.resolve()
    protocol, = deps.glob('libhorizon_protocol-*.rlib')
    candidates = sorted(deps.glob('libhorizon_items-*.rlib'))
    assert candidates, 'Choose a dependency directory with pinned native item libraries.'
    checks = []
    try:
        with tempfile.TemporaryDirectory(prefix='sunset-macro-protocol-') as scratch:
            source = Path(scratch) / 'macro.rs'
            executable = Path(scratch) / 'macro.exe'
            source.write_text(code, encoding='utf-8')
            for items in candidates:
                command = ['rustc', '--edition=2024', '--test', str(source), '-L', f'dependency={deps}',
                           '--extern', f'horizon_items={items}', '--extern', f'horizon_protocol={protocol}',
                           '-o', str(executable)]
                result = subprocess.run(command, capture_output=True, text=True)
                checks.append({'command': command, 'exit': result.returncode,
                               'stdout': result.stdout, 'stderr': result.stderr})
                if result.returncode == 0:
                    break
            if result.returncode == 0:
                result = subprocess.run([str(executable)], capture_output=True, text=True)
                checks.append({'command': [str(executable)], 'exit': result.returncode,
                               'stdout': result.stdout, 'stderr': result.stderr})
            print(result.stdout + result.stderr)
            if result.returncode:
                raise SystemExit(result.returncode)
    finally:
        if args.out:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(json.dumps({
                'host_revision': revision, 'checks': checks, 'separate_source_route_contracts': 1,
                'source_sha256': {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                                  for p in [production, root / 'mixins/sunset/macro_policy.rs']},
                'scope': 'Four compiled real ItemStack/Wire/Reader adapter checks; one separate source contract; no Game/client/network.'
            }, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
