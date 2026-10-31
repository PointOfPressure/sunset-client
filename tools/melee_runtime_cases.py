"""Paired-player outcomes on ALREADY RUNNING disposable native LAN clients.

No launch, network connection, account lookup, build, or screenshot editing.
Both clients must be in the playing screen and the attacker must be an operator.
The caller owns isolation. Commands clear ONLY the two named fixture inventories
and the explicitly bounded arena; nonplayer fixtures have a dedicated tag.

run(ctl, target_ctl, mod, shot, check, attacker='CodexAttacker',
    target='CodexTarget', cases=None, x=0, y=121, z=0)

ctl/target_ctl(list), mod(dot_command), shot(label), check(name,pass,evidence)
match the existing runtime driver. Actual health, use state, positions and items
are evidence. A native screenshot follows every case. The script never declares
WTap or JumpReset proved from a sent packet or its own diagnostic counter;
those timing-sensitive A/B recipes are returned in the report.
"""
from __future__ import annotations

import math
import re

DEFAULT_CASES = ("aura-hit", "aura-native-reach", "trigger-hit",
                 "trigger-only-grounded", "friend-direct", "friend-piercing",
                 "shield-auto", "spear-lunge")
MODULES = ("Kill Aura", "Trigger Bot", "Shield Breaker", "Mace", "Spear Swap",
           "W Tap", "Jump Reset", "Friend Guard")


def target_shield_use(snapshot):
    """Observer metadata for this exact player, never the user's predicted UI."""
    target_id = (snapshot.get("target", {}).get("server") or {}).get("entity_id")
    if target_id is None:
        return None
    equipment = (snapshot.get("attacker", {}).get("server") or {}).get("player_equipment", [])
    matching = [entry for entry in equipment if entry.get("id") == target_id]
    if len(matching) != 1 or matching[0].get("off_hand") != "minecraft:shield":
        return None
    value = matching[0].get("using")
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else None


def shield_proof(before, raised, blocked, lowered, follow_before, after):
    """Require both decoded lowering and an actual charged-sword outcome."""
    health = [float((s.get("target", {}).get("player") or {}).get("health", math.nan))
              for s in (raised, blocked, follow_before, after)]
    attacker = after.get("attacker", {})
    selected = before.get("attacker", {}).get("selected")
    identities = [(s.get("target", {}).get("server") or {}).get("entity_id")
                  for s in (before, raised, blocked, lowered, follow_before, after)]
    return (identities[0] is not None and all(value == identities[0] for value in identities)
            and isinstance(selected, int) and not isinstance(selected, bool) and 1 <= selected <= 9
            and all(math.isfinite(value) and value > 0 for value in health)
            and (target_shield_use(raised) or 0) > 0
            and health[0] == health[1]
            and target_shield_use(lowered) == 0
            and health[3] < health[2]
            and attacker.get("selected") == selected
            and (attacker.get("held") or {}).get("item") == "minecraft:diamond_sword")


def run(ctl, target_ctl, mod, shot, check, *, attacker="CodexAttacker",
        target="CodexTarget", cases=None, x=0, y=121, z=0):
    if not all(re.fullmatch(r"[A-Za-z0-9_]{1,16}", n) for n in (attacker, target)):
        raise ValueError("Fixture player names must be literal Java player names")
    selected = list(DEFAULT_CASES if cases is None else cases)
    enabled = set()
    result = {"cases": [], "settings": {
        "isolation": "Two already-running disposable native LAN players; operator commands",
        "safety": "Attacker resistance255 is a fixture death fail-safe; target has no resistance/armor",
        "health": "Both players max_health1000 and instant_health; every comparison uses observed HP",
        "combat": "Aura/Trigger Crits Off positive; Only on grounded target is a negative control",
    }}
    tag = "sunset_melee_fixture"

    def wait(ticks=2):
        return ctl(["wait", ticks])

    def command(text):
        reply = ctl(["command", text])
        chat = " ".join(map(str, reply.get("chat", [])))
        if reply.get("error") or re.search(r"unknown (?:command|argument)|incorrect argument|expected .+ at position", chat, re.I):
            raise AssertionError({"command": text, "reply": reply})
        return reply

    def at(a, b, c):
        return f"{x+a} {y+b} {z+c}"

    def setting(name, label, value):
        mod(f".set {name.replace(' ', '').lower()} {label} {str(value).lower() if isinstance(value, bool) else value}")

    def toggle(name, want=True):
        if (name in enabled) != want:
            mod(".t " + name.replace(" ", "").lower())
            (enabled.add if want else enabled.discard)(name)

    def stop():
        for name in tuple(enabled):
            toggle(name, False)
        ctl(["release"])
        target_ctl(["release"])

    def snapshot():
        return {"attacker": ctl(["status"]), "target": target_ctl(["status"]),
                "entities": ctl(["entities", 16]).get("entities", [])}

    def hp(s, who="target"):
        return float((s[who].get("player") or {}).get("health", 0))

    def alive(s):
        if hp(s, "attacker") <= 0 or hp(s) <= 0:
            raise AssertionError({"fixture_player_died": s})

    def poll(predicate, ticks=60):
        trace = []
        for _ in range(max(1, ticks // 2)):
            s = snapshot()
            alive(s)
            trace.append(s)
            if predicate(s):
                break
            wait(2)
        return trace[-1], trace

    def setup(distance=2.5, weapon="diamond_sword"):
        stop()
        mod(".panic")
        mod(".panic")
        for name in MODULES:
            mod(".reset " + name.replace(" ", "").lower())
        mod(".friend remove " + target)
        command("gamerule natural_health_regeneration false")
        command("difficulty normal")
        command(f"kill @e[tag={tag}]")
        command(f"fill {at(-6, 0, -6)} {at(12, 9, 6)} minecraft:air")
        command(f"fill {at(-6, -1, -6)} {at(12, -1, 6)} minecraft:stone")
        for name in (attacker, target):
            command(f"gamemode survival {name}")
            command(f"clear {name}")
            command(f"effect clear {name}")
            command(f"attribute {name} minecraft:max_health base set 1000")
            command(f"effect give {name} minecraft:instant_health 1 10 true")
        command(f"effect give {attacker} minecraft:resistance 1000000 255 true")
        command(f"tp {attacker} {at(.5, 0, .5)} -90 0")
        command(f"tp {target} {at(.5+distance, 0, .5)} 90 0")
        command(f"item replace entity {attacker} hotbar.0 with minecraft:{weapon} 1")
        ctl(["key", "1", "tap"])
        target_ctl(["key", "1", "tap"])
        wait(30)
        start = snapshot()
        alive(start)
        if start["attacker"].get("screen") != "playing" or start["target"].get("screen") != "playing":
            raise AssertionError({"fixture_not_playing": start})
        return start

    def report(name, passed, evidence):
        stop()
        mod(".combatdebug")
        evidence["diagnostics"] = ctl(["messages", 10])
        evidence["screenshot"] = shot("melee-" + name)
        check(name, bool(passed), evidence)
        result["cases"].append({"name": name, "passed": bool(passed), "evidence": evidence})

    try:
        for case in selected:
            if case == "aura-hit":
                before = setup()
                setting("Kill Aura", "Crits", "Off")
                toggle("Kill Aura")
                after, trace = poll(lambda s: hp(s) < hp(before))
                report(case, hp(after) < hp(before), {"before": before, "after": after, "trace": trace})
            elif case == "aura-native-reach":
                before = setup(distance=3.6)
                setting("Kill Aura", "Crits", "Off")
                setting("Kill Aura", "Range", 6.5)
                toggle("Kill Aura")
                after, trace = poll(lambda s: hp(s) < hp(before), ticks=50)
                report(case, hp(after) == hp(before), {"before": before, "after": after, "trace": trace,
                       "geometry": "Player boxes remain beyond native survival3 entity reach despite configured6.5"})
            elif case in ("trigger-hit", "trigger-only-grounded"):
                before = setup()
                setting("Trigger Bot", "Reaction", 3)
                setting("Trigger Bot", "Crits", "Only" if case.endswith("grounded") else "Off")
                toggle("Trigger Bot")
                after, trace = poll(lambda s: hp(s) < hp(before), ticks=50)
                expected = hp(after) == hp(before) if case.endswith("grounded") else hp(after) < hp(before)
                report(case, expected, {"before": before, "after": after, "trace": trace})
            elif case == "friend-direct":
                before = setup()
                mod(".friend add " + target)
                toggle("Friend Guard")
                ctl(["mouse", "left", "click"])
                wait(12)
                after = snapshot()
                report(case, hp(after) == hp(before), {"before": before, "after": after,
                       "action": "Real native manual left click at known friend"})
            elif case == "friend-piercing":
                before = setup(distance=3.8, weapon="diamond_spear")
                command(f'summon minecraft:husk {at(3.0,0,.5)} {{NoAI:1b,PersistenceRequired:1b,Tags:["{tag}"]}}')
                wait(10)
                before = snapshot()
                husk_before = sum(float(e.get("health", 0)) for e in before["entities"] if e.get("type") == "minecraft:husk")
                if husk_before <= 0:
                    raise AssertionError({"missing_piercing_fixture": before})
                mod(".friend add " + target)
                toggle("Friend Guard")
                ctl(["mouse", "left", "click"])
                wait(12)
                after = snapshot()
                husk_after = sum(float(e.get("health", 0)) for e in after["entities"] if e.get("type") == "minecraft:husk")
                report(case, hp(after) == hp(before) and husk_after == husk_before,
                       {"before": before, "after": after, "geometry": "Known friend behind first nonfriend husk on the native piercing ray"})
            elif case == "shield-auto":
                before = setup()
                command(f"item replace entity {attacker} hotbar.1 with minecraft:diamond_axe 1")
                command(f"item replace entity {target} weapon.offhand with minecraft:shield 1")
                target_ctl(["mouse", "right", "down"])
                wait(10)
                raised = snapshot()
                if not (target_shield_use(raised) or 0) > 0:
                    raise AssertionError({"shield_did_not_raise": raised})
                # A real blocked sword hit validates the shield geometry. The
                # defender keeps use held through the axe and follow-up hit.
                ctl(["mouse", "left", "click"])
                wait(8)
                blocked = snapshot()
                wait(30)
                setting("Shield Breaker", "Mode", "Auto")
                toggle("Shield Breaker")
                lowered, trace = poll(lambda s: target_shield_use(s) == 0, ticks=50)
                toggle("Shield Breaker", False)
                wait(24)  # Full native sword cooldown; shield cooldown remains active.
                follow_before = snapshot()
                ctl(["mouse", "left", "click"])
                wait(8)
                after = snapshot()
                report(case, shield_proof(before, raised, blocked, lowered, follow_before, after),
                       {"before": before, "raised": raised, "blocked_sword": blocked,
                        "lowered": lowered, "follow_before": follow_before, "after": after,
                        "trace": trace,
                        "action": "Held shield first blocks a real sword hit; observer metadata clears after axe; charged restored sword then damages while use remains held",
                        "oracle": "Defender own using UI is retained as evidence and may stay stale; it does not establish authoritative shield blocking"})
            elif case == "spear-lunge":
                setup(distance=8.0)
                command(f"item replace entity {attacker} hotbar.1 with minecraft:diamond_spear 1")
                ctl(["key", "2", "tap"])
                wait(2)
                command(f"enchant {attacker} minecraft:lunge 3")
                wait(30)
                ctl(["key", "1", "tap"])
                wait(30)
                setting("Spear Swap", "Lunge key", "g")
                toggle("Spear Swap")
                before = snapshot()
                ctl(["key", "g", "tap"])
                def moved(s):
                    p = s["attacker"]["player"]["position"]
                    q = before["attacker"]["player"]["position"]
                    return math.hypot(p[0]-q[0], p[2]-q[2]) > .4
                after, trace = poll(moved, ticks=40)
                wait(4)
                restored = snapshot()
                report(case, moved(after) and restored["attacker"].get("selected") == before["attacker"].get("selected"),
                       {"before": before, "after": after, "restored": restored, "trace": trace,
                        "action": "Actual enchanted-spear native lunge displacement from one key press; no injected velocity"})
            else:
                raise ValueError("Unknown melee case " + case)
    finally:
        stop()
        mod(".friend remove " + target)
        command(f"kill @e[tag={tag}]")
    result["timing_sensitive_recipes"] = [
        "W Tap: paired A/B on same attacker/target, full cooldown, physical W+Ctrl held, Hold3. Record actual attacker horizontal velocity every native tick across one real hit; compare post-hit forward taper and subsequent sprint hit knockback against module-off baseline. Physical W must remain held and release cleanly on disable.",
        "Jump Reset: paired A/B of identical target-to-attacker non-sprint full-charge hits while attacker is grounded and holds W+Ctrl. Record actual own Y/velocity over native ticks after the applied knockback packet. Require greater native jump excursion than the off baseline, plus no jump in water/riding controls. Diagnostic requested-jump counts alone are not proof.",
        "Mace: attacker real fall fromY+5 beside target, sword selected and mace hotbar1, Auto swap on. Observe actual falling velocity, target HP loss exceeding grounded sword baseline, and selected-slot return. Repeat without a mace, while gliding, and Min fall above observed fall as negative controls.",
        "Aim Assist: use true physical mouse-motion event injection, not ctl look (which directly sets rotation). Recent matching-sign input may assist; no movement for>150ms and opposing-axis movement must stop the extra pull. Mining/use pause controls require the native input path.",
    ]
    return result
