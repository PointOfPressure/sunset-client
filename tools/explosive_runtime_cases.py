"""Native explosive-aura cases for an ALREADY RUNNING disposable client.

This module never launches a game, connects to a server, reads account data, or
edits screenshots. The controller owns isolation and records raw ctl replies.

    run(ctl, mod, shot, check, y=121, x=0, z=0, cases=None)

Callbacks match gui_final_driver.py: ctl(list, required=True, record=True),
mod(dot_command), shot(label), check(name, passed, evidence). `y` is feet Y.
`nether` is opt-in in cases because it changes the disposable world's dimension.
Evidence is actual observed block/entity/item/health state, never packet counts
or a second implementation of the aura's damage/selection algorithm.
"""
from __future__ import annotations

import re
import json


DEFAULT_CASES = (
    "crystal-cycle", "crystal-controls", "crystal-default-target",
    "crystal-no-target", "crystal-no-items", "crystal-self-zero",
    "crystal-hidden-face", "anchor-cycle", "anchor-existing",
    "anchor-offhand-fuel", "anchor-disable", "anchor-self-zero",
    "anchor-no-items", "anchor-default-target", "anchor-place-off", "both-enabled",
    "crystal-anti-suicide", "anchor-anti-suicide",
    "equipment-model",
)


def run(ctl, mod, shot, check, *, y=121, x=0, z=0, cases=None):
    """Run requested cases; return a JSON-serializable evidence summary.

    Every case resets both modules and its arena. Positive cases use a
    one-block obsidian cover wall, custom self cap16, and Anti suicide true.
    Positive crystals stand on a floor-level base, at the target's feet height.
    The deliberately raised hidden-face control adds eight snow layers to cover
    (collision height 0.875) and places its base two blocks higher.
    Temporary resistance255 is a separate fixture fail-safe; self safety still
    uses the module's unmitigated upper damage, so resistance cannot make an
    unsafe default self cap pass. Screenshots follow disabling the modules.
    """
    selected = list(DEFAULT_CASES if cases is None else cases)
    output = {"cases": [], "settings_disclosure": {
        "crystal": "Entities=husk, Place/Break range=4.5, Place/Break delay=20, custom Max self damage=16; Anti suicide true; Damage model=Unmitigated",
        "anchor": "Entities=husk, Action delay=20, Max self damage=16; Anti suicide true; Damage model=Unmitigated",
        "fixture": "survival, normal difficulty, no natural regeneration, resistance255 fail-safe, NoAI husk; requested max health1000 is verified from observation",
        "cover": "one-block obsidian wall; positive crystal base is floor-level; raised hidden-face control uses snow layers8 for 1.875-block cover",
        "limits": "Only this already-running isolated integrated singleplayer. Disable test proves stopping an observed sequence; queued-rotation cancellation needs separate instrumentation.",
    }}
    enabled = set()
    tag = "sunset_explosive_fixture"

    def wait(ticks=2):
        return ctl(["wait", ticks])

    def close_overlay():
        # A modal Sunset overlay intentionally leaves native Screen::Playing.
        # Do not type a new chat command until the pointer is captured again.
        observed = []
        escapes = 0
        for _ in range(8):
            state = ctl(["state"])
            observed.append({key: state.get(key) for key in ("screen", "pointer_grabbed")})
            if state.get("screen") == "playing" and state.get("pointer_grabbed") is True:
                return
            if state.get("screen") == "paused":
                ctl(["resume"])
            elif escapes < 4:
                ctl(["key", "esc", "tap"])
                escapes += 1
            else:
                break
            wait(2)
        raise AssertionError({"ui_did_not_close": observed})

    def command(text, optional=False):
        reply = ctl(["command", text], required=not optional)
        chat = " ".join(str(s) for s in reply.get("chat", []))
        bad = bool(reply.get("error")) or bool(re.search(
            r"unknown (?:command|argument)|incorrect argument|failed to|no entity was found|expected .+ at position", chat, re.I))
        if bad and not optional:
            raise AssertionError({"command": text, "reply": reply})
        return not bad, reply

    def toggle(name, want):
        key = name.replace(" ", "").lower()
        if (key in enabled) != want:
            mod(".t " + key)
            (enabled.add if want else enabled.discard)(key)

    def stop():
        for name in tuple(enabled):
            toggle(name, False)

    def setting(name, label, value):
        mod(f".set {name.replace(' ', '').lower()} {label} {str(value).lower() if isinstance(value, bool) else value}")

    def at(a, b, c):
        return f"{x+a} {y+b} {z+c}"

    def item(slot, name, count=1):
        command(f"item replace entity @s {slot} with minecraft:{name} {count}")

    def block(a=4, b=0, c=0):
        return ctl(["block", x+a, y+b, z+c])

    def snapshot():
        st = ctl(["state"])
        status = ctl(["status"])
        inv = ctl(["inventory"])
        entities = ctl(["entities", 24]).get("entities", [])
        # `entities` omits crystals in the pinned engine. Native status fight
        # and projectile lists expose observed crystal ids/positions instead.
        crystal_list = (status.get("fight") or {}).get("crystal_list")
        if crystal_list is None:
            crystal_list = [e for e in status.get("projectiles", [])
                            if e.get("kind") == "minecraft:end_crystal"]
        return {"ticks": st.get("ticks"), "dimension": st.get("dimension"),
                "screen": st.get("screen"), "player": st.get("player"),
                "selected": inv.get("selected"), "counts": inv.get("counts", {}),
                "crystals": crystal_list,
                "husks": [e for e in entities if e.get("type") == "minecraft:husk"],
                "anchor": block()}

    def counts(s, name):
        return s["counts"].get("minecraft:" + name, 0)

    def health(s):
        p = s.get("player") or {}
        return p.get("health", 0)

    def husk_health(s):
        return sum(e.get("health", 0) for e in s["husks"])

    def poll(predicate, ticks=140):
        trace = []
        for _ in range(ticks // 2):
            s = snapshot()
            trace.append(s)
            if predicate(s):
                return s, trace
            if s["screen"] == "dead" or health(s) <= 0:
                raise AssertionError({"fixture_player_died": s})
            wait(2)
        return trace[-1], trace

    def setup(kind="crystal", *, target=True, supplies=True, hidden=False, offhand=False):
        stop()
        # Panic twice resets unrelated module toggles without leaving GUI hidden.
        mod(".panic")
        mod(".panic")
        mod(".reset crystalaura")
        mod(".reset anchoraura")
        ctl(["release"])
        command("gamemode survival @s")
        command("difficulty normal")
        command("gamerule spawn_mobs false")
        command("gamerule natural_health_regeneration false")
        command("time set 6000")
        command("weather clear")
        command("kill @e[type=!minecraft:player]", optional=True)  # Empty arena is already clean.
        command("clear @s")
        command("effect clear @s")
        command("effect give @s minecraft:instant_health 1 10 true")
        command("effect give @s minecraft:resistance 1000000 255 true")
        command(f"fill {at(-9, 0, -9)} {at(11, 7, 9)} minecraft:air")
        command(f"fill {at(-9, -1, -9)} {at(11, -1, 9)} minecraft:{'stone' if kind == 'crystal' else 'obsidian'}")
        # Stay inside the native integrated server's three-block entity ray
        # reach, rather than exactly on its floating-point boundary.
        command(f"tp @s {at(.6, 0, .5)} -90 5")
        command(f"fill {at(1, 0, -2)} {at(1, 0, 2)} minecraft:obsidian")
        if kind == "crystal":
            if hidden:
                command(f"fill {at(1, 1, -2)} {at(1, 1, 2)} minecraft:snow[layers=8]")
            command(f"setblock {at(4, 1 if hidden else -1, 0)} minecraft:obsidian")
            for label, value in (("Entities", "husk"), ("Place range", 4.5),
                                 ("Break range", 4.5), ("Place delay", 20), ("Break delay", 20),
                                 ("Damage model", "Unmitigated"), ("Max self damage", 16)):
                setting("Crystal Aura", label, value)
            if supplies:
                item("hotbar.0", "end_crystal", 8)
        else:
            # Fill the target neighborhood, leaving one placeable anchor cell
            # and the target's two-block room. This makes all three transitions
            # observable at one canonical block instead of guessing placement.
            command(f"fill {at(4, 0, -1)} {at(6, 2, 1)} minecraft:obsidian")
            for pos in ((4, 0, 0), (5, 0, 0), (5, 1, 0)):
                command(f"setblock {at(*pos)} minecraft:air")
            for label, value in (("Entities", "husk"), ("Action delay", 20),
                                 ("Damage model", "Unmitigated"),
                                 ("Max self damage", 16)):
                setting("Anchor Aura", label, value)
            if supplies:
                item("hotbar.0", "respawn_anchor", 8)
                item("weapon.offhand" if offhand else "hotbar.1", "glowstone", 16)
        item("hotbar.8", "stick")
        ctl(["key", "9", "tap"])
        if target:
            command(f"summon minecraft:husk {at(5.5, 0, .5)} {{NoAI:1b,PersistenceRequired:1b,Silent:1b,Tags:[\"{tag}\"]}}")
            selector = f"@e[type=minecraft:husk,tag={tag},limit=1]"
            # New-version attributes use namespaced ids. These commands are
            # optional only because some engine fixture builds omit NBT writes;
            # observed starting health, not the requested1000, is asserted.
            command(f"attribute {selector} minecraft:max_health base set 1000", optional=True)
            command(f"data merge entity {selector} {{Health:1000.0f}}", optional=True)
        wait(8)
        start = snapshot()
        if health(start) <= 0 or start["screen"] != "playing":
            raise AssertionError({"arena_not_ready": start})
        if target and not start["husks"]:
            raise AssertionError({"missing_NoAI_target": start})
        return start

    def report(name, passed, evidence, screenshot=True):
        stop()
        wait(3)
        mod(".combatdebug")
        evidence["diagnostics"] = ctl(["messages", 8])
        evidence["after_disable"] = snapshot()
        if screenshot:
            evidence["screenshot"] = shot("explosives-" + name)
        check("Explosives: " + name, bool(passed), evidence)
        output["cases"].append({"name": name, "passed": bool(passed), "evidence": evidence})
        print(json.dumps({"explosive_case": name, "passed": bool(passed)}), flush=True)

    def negative(name, *, target=True, supplies=True, overrides=()):
        before = setup(target=target, supplies=supplies)
        for label, value in overrides:
            setting("Crystal Aura", label, value)
        toggle("Crystal Aura", True)
        wait(100)
        after = snapshot()
        passed = not after["crystals"] and counts(before, "end_crystal") == counts(after, "end_crystal")
        passed &= husk_health(before) == husk_health(after) and health(after) > 0
        report(name, passed, {"before": before, "after": after})

    try:
        for name in selected:
            if name == "crystal-cycle":
                before = setup()
                setting("Crystal Aura", "Break", False)
                toggle("Crystal Aura", True)
                placed, trace1 = poll(lambda s: bool(s["crystals"]))
                toggle("Crystal Aura", False)
                ids = {c["id"] for c in placed["crystals"]}
                setting("Crystal Aura", "Place", False)
                setting("Crystal Aura", "Break", True)
                toggle("Crystal Aura", True)
                broken, trace2 = poll(lambda s: bool(ids) and not ids.intersection(c["id"] for c in s["crystals"]))
                toggle("Crystal Aura", False)
                wait(6)  # Entity health and removal can arrive in adjacent packets.
                broken = snapshot()
                passed = bool(ids) and counts(placed, "end_crystal") < counts(before, "end_crystal")
                passed &= not ids.intersection(c["id"] for c in broken["crystals"])
                passed &= husk_health(broken) < husk_health(placed) and health(broken) > 0
                passed &= placed["selected"] == before["selected"] == broken["selected"]
                report(name, passed, {"before": before, "placed": placed, "broken": broken,
                                      "trace_place": trace1, "trace_break": trace2})
            elif name == "crystal-controls":
                before = setup()
                setting("Crystal Aura", "Place", False)
                setting("Crystal Aura", "Break", False)
                toggle("Crystal Aura", True)
                wait(100)
                empty = snapshot()
                toggle("Crystal Aura", False)
                command(f"summon minecraft:end_crystal {at(4.5, 1, .5)}")
                wait(5)
                existing = snapshot()
                toggle("Crystal Aura", True)
                wait(45)
                held = snapshot()
                ids = {c["id"] for c in existing["crystals"]}
                passed = not empty["crystals"] and counts(empty, "end_crystal") == counts(before, "end_crystal")
                passed &= bool(ids) and ids.issubset(c["id"] for c in held["crystals"])
                passed &= husk_health(existing) == husk_health(held)
                report(name, passed, {"before": before, "place_off": empty, "existing": existing, "break_off": held})
            elif name == "crystal-default-target":
                negative(name, overrides=(("Entities", "player"),))
            elif name == "crystal-no-target":
                negative(name, target=False)
            elif name == "crystal-no-items":
                negative(name, supplies=False)
            elif name == "crystal-self-zero":
                negative(name, overrides=(("Max self damage", 0),))
            elif name == "crystal-anti-suicide":
                negative(name, overrides=(("Max self damage", 36), ("Min remaining health", 20)))
            elif name == "crystal-hidden-face":
                before = setup(hidden=True)
                setting("Crystal Aura", "Break", False)
                setting("Crystal Aura", "Min target damage", 0)
                toggle("Crystal Aura", True)
                wait(100)
                hidden = snapshot()
                toggle("Crystal Aura", False)
                setting("Crystal Aura", "Walls range", 4.5)
                toggle("Crystal Aura", True)
                control, trace = poll(lambda s: bool(s["crystals"]))
                passed = not hidden["crystals"] and counts(hidden, "end_crystal") == counts(before, "end_crystal")
                passed &= bool(control["crystals"]) and counts(control, "end_crystal") < counts(hidden, "end_crystal")
                report(name, passed, {"before": before, "walls_zero": hidden,
                                      "walls_control": control, "trace_control": trace})
            elif name in ("anchor-cycle", "anchor-offhand-fuel", "anchor-disable"):
                before = setup("anchor", offhand=name == "anchor-offhand-fuel")
                toggle("Anchor Aura", True)
                placed, first = poll(lambda s: s["anchor"].get("block") == "minecraft:respawn_anchor")
                if name == "anchor-disable":
                    toggle("Anchor Aura", False)
                    baseline = snapshot()
                    wait(70)
                    after = snapshot()
                    passed = placed["anchor"].get("block") == "minecraft:respawn_anchor"
                    passed &= baseline["anchor"].get("block") == "minecraft:respawn_anchor"
                    passed &= baseline["anchor"].get("state") == after["anchor"].get("state")
                    passed &= baseline["counts"] == after["counts"] and husk_health(baseline) == husk_health(after)
                    report(name, passed, {"placed": placed, "disabled": baseline, "after": after,
                                          "scope": "Observed sequence stop; no claim of instrumented queued-rotation cancellation", "trace": first})
                else:
                    charged, second = poll(lambda s: "charges=" in s["anchor"].get("state", "")
                        and "charges=0" not in s["anchor"].get("state", ""))
                    gone, third = poll(lambda s: s["anchor"].get("block") != "minecraft:respawn_anchor")
                    toggle("Anchor Aura", False)
                    wait(6)
                    gone = snapshot()
                    seen = first + second + third
                    stages = [s["anchor"].get("state", "") for s in seen]
                    passed = any("charges=0" in s for s in stages) and any("charges=1" in s for s in stages)
                    if name == "anchor-offhand-fuel":
                        passed &= any("charges=4" in s for s in stages)
                    passed &= gone["anchor"].get("block") != "minecraft:respawn_anchor"
                    passed &= counts(gone, "respawn_anchor") < counts(before, "respawn_anchor")
                    passed &= counts(gone, "glowstone") < counts(before, "glowstone")
                    passed &= husk_health(gone) < husk_health(before) and health(gone) > 0
                    passed &= gone["selected"] == before["selected"]
                    report(name, passed, {"before": before, "placed": placed, "charged": charged,
                                          "gone": gone, "observed_states": stages, "trace": seen})
            elif name == "anchor-existing":
                before = setup("anchor", supplies=False)
                command(f"setblock {at(4, 0, 0)} minecraft:respawn_anchor[charges=1]")
                setting("Anchor Aura", "Place", False)
                wait(4)
                charged = snapshot()
                toggle("Anchor Aura", True)
                gone, trace = poll(lambda s: s["anchor"].get("block") != "minecraft:respawn_anchor")
                toggle("Anchor Aura", False)
                wait(6)
                gone = snapshot()
                passed = "charges=1" in charged["anchor"].get("state", "")
                passed &= gone["anchor"].get("block") != "minecraft:respawn_anchor"
                passed &= husk_health(gone) < husk_health(before) and health(gone) > 0
                report(name, passed, {"before": charged, "after": gone, "trace": trace})
            elif name in ("anchor-self-zero", "anchor-no-items", "anchor-default-target", "anchor-place-off", "anchor-anti-suicide"):
                before = setup("anchor", supplies=name != "anchor-no-items")
                label, value = {
                    "anchor-self-zero": ("Max self damage", 0),
                    "anchor-no-items": ("Entities", "husk"),
                    "anchor-default-target": ("Entities", "player"),
                    "anchor-place-off": ("Place", False),
                    "anchor-anti-suicide": ("Min health remaining", 20),
                }[name]
                setting("Anchor Aura", label, value)
                if name == "anchor-anti-suicide":
                    setting("Anchor Aura", "Max self damage", 36)
                toggle("Anchor Aura", True)
                wait(100)
                after = snapshot()
                passed = before["anchor"] == after["anchor"] and before["counts"] == after["counts"]
                passed &= husk_health(before) == husk_health(after) and health(after) > 0
                report(name, passed, {"before": before, "after": after})
            elif name == "both-enabled":
                before = setup()
                item("hotbar.1", "respawn_anchor", 8)
                item("hotbar.2", "glowstone", 16)
                for label, value in (("Entities", "husk"), ("Action delay", 20), ("Max self damage", 16)):
                    setting("Anchor Aura", label, value)
                before = snapshot()
                toggle("Crystal Aura", True)
                toggle("Anchor Aura", True)
                active, trace = poll(lambda s: bool(s["crystals"])
                    or counts(s, "respawn_anchor") < counts(before, "respawn_anchor"))
                stop()
                wait(6)
                after = snapshot()
                acted = bool(active["crystals"]) or counts(active, "respawn_anchor") < counts(before, "respawn_anchor")
                passed = acted and health(after) > 0 and before["selected"] == after["selected"]
                report(name, passed, {"before": before, "active": active, "after": after, "trace": trace,
                    "scope": "Actual concurrent-enabled action and slot restoration; single-deed-per-tick wire proof is not claimed by this case"})
            elif name == "equipment-model":
                setup()
                command("effect clear @s")
                for slot, name_item in (("head", "helmet"), ("chest", "chestplate"), ("legs", "leggings"), ("feet", "boots")):
                    item("armor." + slot, "netherite_" + name_item)
                wait(10)
                setting("Crystal Aura", "Break", False)
                # R4's accepted canonical blast has an equipment-aware Hard
                # bound of 2.008. The old cap2 fixture correctly rejected it.
                # Cap6 separates equipped eligibility from raw damage without
                # changing the module's safety calculation or performing a blast.
                setting("Crystal Aura", "Max self damage", 6)
                before = snapshot()
                toggle("Crystal Aura", True)
                wait(60)
                strict = snapshot()
                toggle("Crystal Aura", False)
                setting("Crystal Aura", "Damage model", "Observed equipment")
                toggle("Crystal Aura", True)
                equipped, trace = poll(lambda s: bool(s["crystals"]))
                passed = not strict["crystals"] and counts(strict, "end_crystal") == counts(before, "end_crystal")
                passed &= bool(equipped["crystals"]) and counts(equipped, "end_crystal") < counts(strict, "end_crystal")
                passed &= health(equipped) > 0 and before["selected"] == equipped["selected"]
                report(name, passed, {"before": before, "unmitigated": strict, "equipped": equipped, "trace": trace,
                    "scope": "Actual durable netherite equipment, no resistance; model selection changes placement eligibility at self cap6. The earlier cap2 rejection is retained separately. No explosion is performed by this case."})
            elif name == "nether":
                stop()
                ok, reply = command(f"execute in minecraft:the_nether run tp @s {at(.5, 0, .5)}", optional=True)
                wait(40)
                if not ok or ctl(["state"]).get("dimension") != "minecraft:the_nether":
                    output["cases"].append({"name": name, "status": "unsupported", "evidence": reply})
                    continue
                try:
                    setup("anchor")
                    command(f"setblock {at(4, 0, 0)} minecraft:respawn_anchor[charges=1]")
                    wait(4)
                    before = snapshot()
                    toggle("Anchor Aura", True)
                    wait(100)
                    after = snapshot()
                    report(name, before["anchor"] == after["anchor"] and before["counts"] == after["counts"]
                           and husk_health(before) == husk_health(after), {"before": before, "after": after})
                finally:
                    stop()
                    command(f"execute in minecraft:overworld run tp @s {at(.5, 0, .5)}")
                    wait(40)
            else:
                raise ValueError("Unknown explosive runtime case: " + name)
        if cases is None:
            stop()
            mod(".menu crystal aura")
            output["crystal_gui"] = shot("explosives-crystal-gui")
            close_overlay()
            mod(".menu anchor aura")
            output["anchor_gui"] = shot("explosives-anchor-gui")
    finally:
        close_overlay()
        stop()
        ctl(["release"])
    return output
