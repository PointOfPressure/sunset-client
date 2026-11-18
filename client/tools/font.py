#!/usr/bin/env python3
# Cuts the menu and HUD fonts (all SIL Open Font License 1.1, licences in assets/licenses/) down to the characters Sunset
# draws, in a Medium and a bolder weight (for headers), with no hinting. Each is kept as assets/<name>.ttf and
# assets/<name>-bold.ttf, and written as Rust byte arrays, with the list of fonts and each one's size, to
# mixins/sunset/font_data.rs. Variable fonts are set to the weight first (fontTools instancer).
# Usage: font.py   (the sources are listed below)
# The size of each font makes a line of text as wide as Inter's at 8.0.
import subprocess, sys, os, tempfile
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

Z = "/usr/share/fonts/truetype"
C = os.path.expanduser("~/.cache/tmp/fonts")
# (resource name, label in HUD > Font, regular file, its weight if variable, bold file, its weight if variable)
FONTS = [
    ("sora", "Sora", f"{C}/Sora.ttf", 500, f"{C}/Sora.ttf", 700),
    ("inter", "Inter", f"{Z}/inter-zorin-os/Inter-Medium.ttf", None, f"{Z}/inter-zorin-os/Inter-SemiBold.ttf", None),
    ("poppins", "Poppins", f"{C}/Poppins-Medium.ttf", None, f"{C}/Poppins-SemiBold.ttf", None),
    ("outfit", "Outfit", f"{C}/Outfit.ttf", 500, f"{C}/Outfit.ttf", 700),
    ("manrope", "Manrope", f"{C}/Manrope.ttf", 500, f"{C}/Manrope.ttf", 700),
    ("geist", "Geist", f"{C}/Geist.ttf", 500, f"{C}/Geist.ttf", 700),
    ("montserrat", "Montserrat", f"{C}/Montserrat.ttf", 500, f"{C}/Montserrat.ttf", 700),
    ("jakarta", "Plus Jakarta Sans", f"{C}/PlusJakartaSans.ttf", 500, f"{C}/PlusJakartaSans.ttf", 700),
    ("lexend", "Lexend", f"{C}/Lexend.ttf", 500, f"{C}/Lexend.ttf", 700),
    ("rubik", "Rubik", f"{C}/Rubik.ttf", 500, f"{C}/Rubik.ttf", 700),
    ("grotesk", "Space Grotesk", f"{Z}/space-grotesk-zorin-os/SpaceGrotesk-Medium.ttf", None, f"{Z}/space-grotesk-zorin-os/SpaceGrotesk-Bold.ttf", None),
    ("redhat", "Red Hat Display", f"{C}/RedHatDisplay.ttf", 500, f"{C}/RedHatDisplay.ttf", 700),
    ("comfortaa", "Comfortaa", f"{C}/Comfortaa.ttf", 500, f"{C}/Comfortaa.ttf", 700),
    ("quicksand", "Quicksand", f"{C}/Quicksand.ttf", 500, f"{C}/Quicksand.ttf", 700),
    ("nunito", "Nunito", f"{C}/Nunito.ttf", 500, f"{C}/Nunito.ttf", 700),
    ("jetbrains", "JetBrains Mono", f"{Z}/jetbrains-mono-zorin-os/JetBrainsMono-Medium.ttf", None,
     f"{Z}/jetbrains-mono-zorin-os/JetBrainsMono-Bold.ttf", None),
]
SAMPLE = "Kill Aura Trigger Bot Aim Assist Criticals Auto Totem Weapon Flight Speed Sprint Scaffold Reverse Step Nametags Storage ESP"
here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
unicodes = "U+0020-007E,U+00A0-00FF,U+2013,U+2014,U+2018,U+2019,U+201C,U+201D,U+2022,U+2026,U+2190-2193,U+2212,U+00D7"


def cut(src, weight, name):
    with tempfile.TemporaryDirectory() as d:
        if weight is not None:
            font = TTFont(src)
            instancer.instantiateVariableFont(font, {"wght": weight}, inplace=True)
            src = os.path.join(d, "instance.ttf")
            font.save(src)
        out = os.path.join(d, f"{name}.ttf")
        subprocess.run(["pyftsubset", src, f"--unicodes={unicodes}", "--layout-features=", "--no-hinting",
                        "--drop-tables+=GSUB,GPOS,GDEF,kern,STAT,fvar,avar,gvar,HVAR,MVAR", f"--output-file={out}"], check=True)
        return open(out, "rb").read(), TTFont(out)


def advance(font):
    cmap, hmtx = font.getBestCmap(), font["hmtx"]
    return sum(hmtx[cmap[ord(c)]][0] for c in SAMPLE) / font["head"].unitsPerEm


os.makedirs(os.path.join(here, "assets"), exist_ok=True)
body, sizes, total = [], {}, 0
for name, label, regular, wr, bold, wb in FONTS:
    for file, src, weight in ((name, regular, wr), (f"{name}-bold", bold, wb)):
        data, font = cut(src, weight, file)
        open(os.path.join(here, "assets", f"{file}.ttf"), "wb").write(data)
        total += len(data)
        if file == name:
            sizes[name] = advance(font)
        body.append(f"pub const UI_{file.upper().replace('-', '_')}: &[u8] = &[")
        for j in range(0, len(data), 24):
            body.append("    " + ", ".join(str(b) for b in data[j:j + 24]) + ",")
        body.append("];")
head = ["// The menu and HUD fonts, Medium and a bolder weight each (all SIL Open Font License 1.1, in assets/licenses/), cut down",
        "// by tools/font.py: the names HUD > Font offers, and every font's bytes and size in GUI pixels.",
        'pub const LABELS: [&str; %d] = [%s, "Minecraft"];' % (len(FONTS) + 1, ", ".join(f'"{f[1]}"' for f in FONTS)),
        "// (resource name, regular, bolder, size)",
        "pub const FONTS: [(&str, &[u8], &[u8], f32); %d] = [" % len(FONTS)]
for name, *_ in FONTS:
    size = round(8 * sizes["inter"] / sizes[name], 1)
    head.append(f'    ("{name}", UI_{name.upper()}, UI_{name.upper()}_BOLD, {size}),')
    print(f"{name}: size {size}")
head.append("];")
open(os.path.join(here, "mixins", "sunset", "font_data.rs"), "w").write("\n".join(head + body) + "\n")
print(total, "bytes of fonts")
