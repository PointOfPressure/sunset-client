#!/usr/bin/env python3
import re, pathlib, sys, json
base = pathlib.Path.home()/".cache/tmp/meteor-src/meteor-client/src/main/java/meteordevelopment/meteorclient/systems/modules"
sol = pathlib.Path.home()/"dev/horizon-meteor/mixins/solstice"
def norm(s): return re.sub(r'[^a-z0-9]','',s.lower())
# ours
ours = {}
for f in sol.glob("*.rs"):
    src = f.read_text()
    for m in re.finditer(r'Module::new\("([^"]+)"[^;]*?(?=Module::new\(|\n    \]\n)', src, re.S):
        name = m.group(1)
        names = re.findall(r'\.(?:bool|num|pick|text)\(\s*"([^"]+)"', m.group(0))
        ours[norm(name)] = (name, names)
mis = {}
total_m = total_o = 0
for f in sorted(base.rglob("*.java")):
    src = f.read_text()
    t = re.search(r'super\(Categories\.\w+, "([^"]+)"', src)
    if not t: continue
    key = norm(t.group(1))
    sets = re.findall(r'\.name\("([^"]+)"\)', src)
    sets = list(dict.fromkeys(sets))
    total_m += 1
    if key in ours:
        have = {norm(x) for x in ours[key][1]}
        missing = [s for s in sets if norm(s) not in have]
        mis[ours[key][0]] = (len(sets), len(ours[key][1]), missing)
for name,(n,o,missing) in sorted(mis.items()):
    print(f"{name}: meteor {n}, ours {o}; missing: {', '.join(missing) if missing else '-'}")
