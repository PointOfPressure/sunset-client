"""Outcome oracle regressions; these are not a substitute for native runs."""
from copy import deepcopy
import unittest

from melee_runtime_cases import shield_proof, target_shield_use


def sample(health=1000, use=19, selected=1, held="minecraft:diamond_sword"):
    return {"attacker": {"selected": selected, "held": {"item": held},
                         "server": {"player_equipment": [
                             {"id": 8, "off_hand": "minecraft:shield", "using": use}]}},
            "target": {"server": {"entity_id": 8}, "player": {"health": health},
                       "using": {"slot": 40, "ticks": 544}}}


class ShieldOutcomeTests(unittest.TestCase):
    def proof(self):
        return [sample(), sample(), sample(), sample(use=0), sample(use=0),
                sample(health=993, use=0)]

    def test_stale_defender_use_ui_does_not_hide_real_disable(self):
        # Matches the important historical observation: held-use prediction
        # stays active, observer flag clears, subsequent sword costs real HP.
        self.assertTrue(shield_proof(*self.proof()))

    def test_flag_alone_cannot_pass_without_followup_damage(self):
        proof = self.proof()
        proof[-1]["target"]["player"]["health"] = 1000
        self.assertFalse(shield_proof(*proof))

    def test_unblocked_first_sword_invalidates_fixture(self):
        proof = self.proof()
        proof[2]["target"]["player"]["health"] = 993
        self.assertFalse(shield_proof(*proof))

    def test_wrong_player_or_missing_metadata_is_not_lowering(self):
        for entries in ([], [{"id": 7, "off_hand": "minecraft:shield", "using": 0}]):
            proof = self.proof()
            proof[3]["attacker"]["server"]["player_equipment"] = entries
            self.assertFalse(shield_proof(*proof))

    def test_duplicate_id_unknown_or_wrong_equipment_fails_closed(self):
        for value in (None, False, -1, "0"):
            state = sample(use=value)
            self.assertIsNone(target_shield_use(state))
        state = sample(use=0)
        state["attacker"]["server"]["player_equipment"] *= 2
        self.assertIsNone(target_shield_use(state))
        state = sample(use=0)
        state["attacker"]["server"]["player_equipment"][0]["off_hand"] = None
        self.assertIsNone(target_shield_use(state))

    def test_slot_return_and_sword_followup_are_both_required(self):
        for key, value in (("selected", 2), ("held", {"item": "minecraft:diamond_axe"})):
            proof = self.proof()
            proof[-1]["attacker"][key] = value
            self.assertFalse(shield_proof(*proof))

    def test_reconnected_target_and_nonfinite_health_fail(self):
        for changed in ("identity", "health"):
            proof = self.proof()
            if changed == "identity":
                proof[-1]["target"]["server"]["entity_id"] = 81
            else:
                proof[-1]["target"]["player"]["health"] = float("nan")
            self.assertFalse(shield_proof(*proof))

    def test_input_evidence_is_not_rewritten(self):
        proof = self.proof()
        original = deepcopy(proof)
        shield_proof(*proof)
        self.assertEqual(original, proof)


if __name__ == "__main__":
    unittest.main()
