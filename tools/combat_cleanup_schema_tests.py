"""Native API and cancellation wiring audit; no build, launch or fixture I/O."""
from pathlib import Path
import re

ROOT=Path(__file__).resolve().parents[1]
import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('engine',type=Path)
ENGINE=parser.parse_args().engine.resolve()
control=(ROOT/'mixins/sunset/combat_control.rs').read_text(encoding="utf-8")
melee=(ROOT/'mixins/sunset/melee_assist.rs').read_text(encoding="utf-8")
runtime=(ROOT/'mixins/sunset/runtime.rs').read_text(encoding="utf-8")
net=(ENGINE/'crates/horizon-client/src/net.rs').read_text(encoding="utf-8")
remote=(ENGINE/'crates/horizon-client/src/game/remote.rs').read_text(encoding="utf-8")

def body(source,name):
    start=source.index('{',source.index('fn '+name+'('));depth=0
    for end in range(start,len(source)):
        if source[end]=='{':depth+=1
        elif source[end]=='}':
            depth-=1
            if depth==0:return source[start+1:end]
    raise AssertionError(name)

assert 'pubcounters:Arc<Mutex<Option<Arc<Counters>>>>' in re.sub(r'\s+','',net)
assert re.search(r'pub\(super\)\s+entity_id:\s*i32',remote)
assert re.search(r'fn begin_tick\([^)]*\)\s*->\s*bool',control)
cleanup=body(control,'prepare_cleanup')
assert 'native_on' not in cleanup and 'alive(game)' in cleanup and 'same(game, s)' in cleanup
cancel=re.sub(r'\s+','',body(melee,'cancel'))
assert cancel.index('post_input(game)')<cancel.index('release_lease(game,true)')<cancel.index('cancel_turns(owner)')
assert '[SHIELD,MACE,SPEAR,"TriggerBot"]' in cancel
assert 'control::release' not in cancel
assert 'let sent=s.sent;'.replace(' ','') in cancel and 'let blocked=s.blocked;'.replace(' ','') in cancel
assert '..State::default()' in cancel
release=body(melee,'release_lease')
assert 'end.current()' in release and 'lease.restore(' in release
normal=re.sub(r'\s+','',runtime)
assert '"AnchorAura"=>super::sunset_anchor_aura::disabled(game)' in normal
assert 'sunset_anchor_aura::reset()' not in runtime, 'Runtime off/cleanup requires Game-aware owned hand return'
pre_tick=body(runtime,'pre_tick')
master=pre_tick[pre_tick.index('if !game.native_on(ID)'):]
panic=body(runtime,'panic_off')
for scope in [master,panic]:
    assert scope.index('sunset_combat_control::prepare_cleanup(game)')<scope.index('sunset_melee_assist::cancel(game)')<scope.index('sunset_combat_control::reset()')
    assert scope.index('sunset_crystal_aura::disabled(game)')<scope.index('sunset_crystal_aura::cleanup_shared(game)')
    assert scope.index('sunset_anchor_aura::disabled(game)')<scope.index('sunset_crystal_aura::cleanup_shared(game)')
assert panic.index('sunset_anchor_aura::suspend()')<panic.index('sunset_combat_control::reset()')
assert panic.index('sunset_crystal_aura::suspend()')<panic.index('sunset_combat_control::reset()')
print('combat cleanup native schema/cancellation audit passed')
