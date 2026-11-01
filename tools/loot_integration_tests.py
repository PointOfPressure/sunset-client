"""Check critical guard integration order without starting a native client.

These checks pin source wiring, rather than claiming runtime transaction proof.
Policy and real protocol behavior are tested by the separate loot harnesses.
"""
from pathlib import Path
import unittest
import re

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'mixins/sunset'

def read(name):
    return (SOURCE / name).read_text(encoding='utf-8')

def function(text,signature):
    start=text.index(signature);opening=text.index('{',start);depth=1;cursor=opening+1
    while depth:
        depth+=(text[cursor]=='{')-(text[cursor]=='}');cursor+=1
    return text[start:cursor]

class Integration(unittest.TestCase):
    def test_packet_observer_runs_after_delay_before_native_application(self):
        text = read('combat_packet.rs')
        self.assertLess(text.index('sunset_packet_delay::incoming'), text.index('sunset_loot_guard::packet'))
        self.assertEqual(text.count('sunset_loot_guard::packet'), 1)

    def test_protected_existing_anchor_consumes_before_inventory_and_lease_fallbacks(self):
        text = read('combat_macros.rs').split('pub(super) fn intercept_use', 1)[1]
        early = text.index('sunset_loot_guard::allows')
        self.assertLess(early, text.index('if !allowed(game)'))
        self.assertLess(early, text.index('recognized_hand'))
        self.assertLess(early, text.index('sunset_combat_control::claim'))
        self.assertIn('game.native_on(sunset::ID)', text[:early])
        self.assertIn('return true;', text[early:text.index('if !allowed(game)')])
        self.assertLess(early, text.index('!press.is_some_and(|p| p.start_allowed)'))

    def test_active_and_deferred_macro_rejection_use_owned_cleanup(self):
        text = read('combat_macros.rs')
        self.assertEqual(text.count('|| !loot_allowed(game, &run)'), 2)
        for chunk in text.split('|| !loot_allowed(game, &run)')[1:]:
            self.assertIn('reset(game);', chunk[:900])
        safe = function(text,'fn safe_blast(')
        self.assertLess(safe.index('sunset_loot_guard::allows'), safe.index('observed_local_damage'))
        self.assertLess(safe.index('sunset_loot_guard::allows'), safe.index('kind==Kind::Anchor'))

    def test_both_aura_shared_damage_helpers_gate_planned_and_fresh_actions(self):
        for name, helper, deferred, hand in [
            ('crystal_aura.rs','valid_damage','fn queue(','take_weapon(game,'),
            ('anchor_aura.rs','damage_ok','pub(super) fn execute(','take_hand(game,')]:
            text = read(name)
            shared=function(text,'fn damage_pair(')
            planning=function(text,'pub(super) fn prepare(')
            fresh=function(text,deferred)
            self.assertIn('damage_pair(',planning)
            self.assertIn('damage_pair(',function(text,'fn '+helper+'('))
            self.assertLess(shared.index('sunset_loot_guard::allows'),shared.index('local_damage'))
            self.assertLess(fresh.index(helper+'('),fresh.index(hand))
            self.assertRegex(text,r'\.bool\("Protect Loot",\s*true\)')

if __name__ == '__main__':
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Integration))
    if not result.wasSuccessful():
        raise SystemExit(1)
    print(f'test result: ok. {result.testsRun} passed')
