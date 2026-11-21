#!/usr/bin/env python3
"""make-icons.py: the launcher's icons from the Sunset mark's geometry (the same two half rings ui.rs's `mark` draws).

    python launcher/assets/make-icons.py      # writes sunset-launcher.ico, sunset-launcher.png and .svg beside it

Each size is drawn on its own (supersampled, then reduced), not scaled from one picture, so 16-48 px stay sharp; the
small sizes give the mark more of the tile. Needs Pillow."""
import math
import os
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
TILE = (0x0B, 0x0B, 0x0C, 255)  # the launcher's background
MARK = (0xDB, 0x50, 0x00, 255)  # Sunset orange
# The mark in its own 432 x 216 units: centre x, baseline y, outer and inner radius, foot below the baseline.
RINGS = [(216, 216, 216, 167, 0), (216, 211, 121, 72, 5)]
CORNER = 0.22  # tile corner radius, of its side


def mark_width(size):
    return 0.86 if size <= 24 else 0.8 if size <= 48 else 0.74


def ring_points(cx, cy, ro, ri, foot, k, ox, oy, steps=512):
    at = lambda x, y: (ox + k * x, oy + k * y)
    arc = lambda r: [(cx + r * math.cos(math.pi * (1 + i / steps)), cy + r * math.sin(math.pi * (1 + i / steps))) for i in range(steps + 1)]
    pts = [at(cx - ro, cy + foot)] + [at(*p) for p in arc(ro)] + [at(cx + ro, cy + foot), at(cx + ri, cy + foot)]
    pts += [at(*p) for p in reversed(arc(ri))] + [at(cx - ri, cy + foot)]
    return pts


def render(size):
    ss = 16 if size <= 64 else 4
    s = size * ss
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, s - 1, s - 1), radius=CORNER * s, fill=TILE)
    w = mark_width(size) * s
    k = w / 432
    ox, oy = (s - w) / 2, (s - 216 * k) / 2  # the rings' box centred in the tile
    for ring in RINGS:
        d.polygon(ring_points(*ring, k, ox, oy), fill=MARK)
    return img.resize((size, size), Image.LANCZOS)


def svg(side=256):
    w = mark_width(256) * side
    k = w / 432
    ox, oy = (side - w) / 2, (side - 216 * k) / 2
    f = lambda v: f"{v:.3f}".rstrip("0").rstrip(".")
    paths = []
    for cx, cy, ro, ri, foot in RINGS:
        X = lambda x: f(ox + k * x)
        Y = lambda y: f(oy + k * y)
        paths.append(
            f"M{X(cx - ro)} {Y(cy + foot)}L{X(cx - ro)} {Y(cy)}A{f(k * ro)} {f(k * ro)} 0 0 1 {X(cx + ro)} {Y(cy)}"
            f"L{X(cx + ro)} {Y(cy + foot)}L{X(cx + ri)} {Y(cy + foot)}L{X(cx + ri)} {Y(cy)}"
            f"A{f(k * ri)} {f(k * ri)} 0 0 0 {X(cx - ri)} {Y(cy)}L{X(cx - ri)} {Y(cy + foot)}Z"
        )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {side} {side}">\n'
        f"  <title>Sunset</title>\n"
        f'  <rect width="{side}" height="{side}" rx="{f(CORNER * side)}" fill="#0b0b0c"/>\n'
        f'  <path fill="#db5000" d="{"".join(paths)}"/>\n'
        f"</svg>\n"
    )


if __name__ == "__main__":
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    imgs = [render(n) for n in sizes]
    imgs[-1].save(os.path.join(HERE, "sunset-launcher.ico"), sizes=[(n, n) for n in sizes], append_images=imgs[:-1])
    imgs[-1].save(os.path.join(HERE, "sunset-launcher.png"))
    open(os.path.join(HERE, "sunset-launcher.svg"), "w", newline="\n").write(svg())
    print("icons:", ", ".join(map(str, sizes)))
