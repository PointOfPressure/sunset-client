#!/usr/bin/env python3
"""make-icons.py: the Sunset logo, drawn from one geometry, and every file made from it.

    python launcher/assets/make-icons.py      # sunset-launcher.svg/.png/.ico here, logo.png/logo-light.png/icon.png in docs/images

The logo is a striped half sun on its horizon with five round-capped rays around it, in Sunset orange, on a 200 x 100
unit box (y down, the horizon bar along the bottom). ui.rs's 'logo' draws the same geometry (keep DETAILS in step).
Small sizes get their own, bolder variant (fewer bands and rays) and every horizontal edge lands on a pixel row, so
16 and 32 px stay crisp; each raster is drawn analytically (signed distances, exact coverage), not scaled down.
Needs numpy and Pillow."""
import math
import os

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
DOCS = os.path.join(HERE, "..", "..", "docs", "images")
ORANGE = (0xDB, 0x50, 0x00)  # Sunset orange
TILE = (0x0B, 0x0B, 0x0C)  # the launcher's background
CORNER = 0.22  # tile corner radius, of its side

# Per detail level: horizon bar thickness, the gap between the sun's base and the bar, sun radius, bands cut through
# the sun (heights above its base, bottom to top), ray angles (degrees, y down: 270 is straight up), ray inner and
# outer radius and ray width. The level is picked by the logo's width in pixels.
DETAILS = {
    "large": dict(bar=8, inset=0, lift=6, r=54, gaps=[(6, 12.5), (20, 25.5), (34, 38)], rays=[200, 235, 270, 305, 340], ray=(66, 80), ray_w=11),
    "medium": dict(bar=11, inset=0, lift=7, r=50, gaps=[(7, 16), (26, 32)], rays=[200, 235, 270, 305, 340], ray=(62, 74), ray_w=14),
    "compact": dict(bar=11, inset=16, lift=8, r=50, gaps=[(8, 17), (27, 34)], rays=[220, 270, 320], ray=(63, 76), ray_w=15),
    "small": dict(bar=13, inset=22, lift=11, r=50, gaps=[(11, 24)], rays=[225, 270, 315], ray=(64, 76), ray_w=16),
}


def detail(width_px):
    return DETAILS["large" if width_px >= 64 else "medium" if width_px >= 40 else "compact" if width_px >= 26 else "small"]


def logo_alpha(w, h, k, ox, oy, d):
    """Coverage (h x w floats) of the logo drawn k pixels per unit with its box's top left at (ox, oy)."""
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float64) + 0.5
    snap = lambda v: float(round(oy + k * v))  # a horizontal edge, on a pixel row
    bar_t, bar_b = snap(100 - d["bar"]), snap(100)
    bar_t = min(bar_t, bar_b - 1)
    base = min(snap(100 - d["bar"] - d["lift"]), bar_t - 1)
    cx, r = ox + 100 * k, d["r"] * k
    cy = base
    # The sun: the half disc above its base, less the bands.
    dist = np.hypot(xs - cx, ys - cy) - r
    edges = [base]
    for lo, hi in d["gaps"]:
        edges += [min(snap(100 - d["bar"] - d["lift"] - lo), edges[-1] - 1), None]
        edges[-1] = min(snap(100 - d["bar"] - d["lift"] - hi), edges[-2] - 1)
    edges.append(-1e9)
    sdf = np.full((h, w), 1e9)
    for i in range(0, len(edges), 2):  # slices: base..first gap, between gaps, last gap..top
        bot, top = edges[i], edges[i + 1]
        sdf = np.minimum(sdf, np.maximum(dist, np.maximum(top - ys, ys - bot)))
    # The rays, round capped.
    hw = d["ray_w"] * k / 2
    for deg in d["rays"]:
        s, c = math.sin(math.radians(deg)), math.cos(math.radians(deg))
        ax, ay = cx + d["ray"][0] * k * c, cy + d["ray"][0] * k * s
        bx, by = cx + d["ray"][1] * k * c, cy + d["ray"][1] * k * s
        sdf = np.minimum(sdf, capsule(xs, ys, ax, ay, bx, by) - hw)
    # The horizon bar, round capped, across the box.
    t = (bar_b - bar_t) / 2
    yc = (bar_t + bar_b) / 2
    sdf = np.minimum(sdf, capsule(xs, ys, ox + d["inset"] * k + t, yc, ox + (200 - d["inset"]) * k - t, yc) - t)
    return np.clip(0.5 - sdf, 0, 1)


def capsule(xs, ys, ax, ay, bx, by):
    vx, vy = bx - ax, by - ay
    f = np.clip(((xs - ax) * vx + (ys - ay) * vy) / max(vx * vx + vy * vy, 1e-9), 0, 1)
    return np.hypot(xs - ax - f * vx, ys - ay - f * vy)


def rgba(alpha, rgb=ORANGE):
    a = np.dstack([np.full(alpha.shape, c, np.uint8) for c in rgb] + [np.round(alpha * 255).astype(np.uint8)])
    return Image.fromarray(a, "RGBA")


def mark_width(size):
    return 1.08 if size <= 24 else 0.95 if size <= 40 else 0.84 if size <= 48 else 0.76


def top(d):
    """Where the logo's highest point (its upright ray's cap) is, in units: under 0 when it rises out of the box."""
    return 100 - d["bar"] - d["lift"] - d["ray"][1] - d["ray_w"] / 2


def tile(size):
    """The app icon: the logo on a rounded dark tile."""
    ss = 8 if size <= 64 else 2  # only the tile's round corners are supersampled; the logo is exact
    s = size * ss
    yy, xx = np.mgrid[0:s, 0:s].astype(np.float64) + 0.5
    rad = CORNER * s
    qx, qy = np.maximum(np.abs(xx - s / 2) - (s / 2 - rad), 0), np.maximum(np.abs(yy - s / 2) - (s / 2 - rad), 0)
    cov = np.clip(rad - np.hypot(qx, qy) + 0.5, 0, 1)
    t = Image.fromarray(np.round(cov * 255).astype(np.uint8), "L").resize((size, size), Image.BOX)
    img = Image.new("RGBA", (size, size), TILE + (0,))
    img.putalpha(t)
    w = mark_width(size) * size
    k = w / 200
    d = detail(w)
    ox, oy = round((size - w) / 2), round((size - (100 - top(d)) * k) / 2 - top(d) * k)
    img.alpha_composite(rgba(logo_alpha(size, size, k, ox, oy, d)))
    return img


def svg(side=256):
    d = DETAILS["large"]
    w = mark_width(256) * side
    k = w / 200
    ox, oy = (side - w) / 2, (side - 100 * k) / 2
    f = lambda v: f"{v:.3f}".rstrip("0").rstrip(".")
    X = lambda x: f(ox + k * x)
    Y = lambda y: f(oy + k * y)
    cx, base, r = 100, 100 - d["bar"] - d["lift"], d["r"]
    # Slices of the half disc between heights a and b above its base (b = r for the cap).
    cuts = [0] + [v for g in d["gaps"] for v in g] + [r]
    parts = []
    for a, b in zip(cuts[0::2], cuts[1::2]):
        xa, xb = math.sqrt(r * r - a * a), math.sqrt(max(r * r - b * b, 0))
        arc = lambda x, y: f"A{f(k * r)} {f(k * r)} 0 0 0 {X(x)} {Y(y)}"
        if b >= r:
            parts.append(f"M{X(cx - xa)} {Y(base - a)}L{X(cx + xa)} {Y(base - a)}{arc(cx - xa, base - a)}Z")
        else:
            parts.append(f"M{X(cx - xa)} {Y(base - a)}L{X(cx + xa)} {Y(base - a)}{arc(cx + xb, base - b)}L{X(cx - xb)} {Y(base - b)}{arc(cx - xa, base - a)}Z")
    rays = []
    for deg in d["rays"]:
        s, c = math.sin(math.radians(deg)), math.cos(math.radians(deg))
        rays.append(f"M{X(cx + d['ray'][0] * c)} {Y(base + d['ray'][0] * s)}L{X(cx + d['ray'][1] * c)} {Y(base + d['ray'][1] * s)}")
    t = d["bar"] / 2
    rays.append(f"M{X(t)} {Y(100 - t)}L{X(200 - t)} {Y(100 - t)}")
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {side} {side}">\n'
        f"  <title>Sunset</title>\n"
        f'  <rect width="{side}" height="{side}" rx="{f(CORNER * side)}" fill="#0b0b0c"/>\n'
        f'  <path fill="#db5000" d="{"".join(parts)}"/>\n'
        f'  <path fill="none" stroke="#db5000" stroke-linecap="round" stroke-width="{f(k * d["ray_w"])}" d="{"".join(rays[:-1])}"/>\n'
        f'  <path fill="none" stroke="#db5000" stroke-linecap="round" stroke-width="{f(k * d["bar"])}" d="{rays[-1]}"/>\n'
        f"</svg>\n"
    )


def docs_logo(path, width, out=None):
    """Swaps the mark above the wordmark in a docs logo (the wordmark itself is kept, pixel for pixel)."""
    img = Image.open(path)
    mode = img.mode
    a = np.asarray(img.convert("RGBA")).astype(np.int32)
    orange = (a[..., 0] - a[..., 2] > 40) & (a[..., 3] > 0)
    rows, cols = np.nonzero(orange)
    if len(rows):  # the old mark: its box, then gone
        top, bot, left, right = rows.min(), rows.max() + 1, cols.min(), cols.max() + 1
        bg = a[0, 0].copy()
        a[orange] = bg
    else:
        raise SystemExit(f"{path}: no orange mark found")
    dest = out
    out = Image.fromarray(a.astype(np.uint8), "RGBA")
    w = (right - left) * width
    k = w / 200
    ox, oy = round((left + right) / 2 - w / 2), round(bot - 100 * k - 0.07 * w)  # clear of the wordmark's ascenders
    out.alpha_composite(rgba(logo_alpha(out.width, out.height, k, ox, oy, DETAILS["large"])))
    (out if mode == "RGBA" else out.convert(mode)).save(dest or path, optimize=True)


if __name__ == "__main__":
    import sys
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    imgs = [tile(n) for n in sizes]
    imgs[-1].save(os.path.join(HERE, "sunset-launcher.ico"), sizes=[(n, n) for n in sizes], append_images=imgs[:-1])
    imgs[-1].save(os.path.join(HERE, "sunset-launcher.png"))
    open(os.path.join(HERE, "sunset-launcher.svg"), "w", newline="\n").write(svg())
    if "--docs" in sys.argv:  # once: the docs logos are edited in place
        big = tile(512)
        Image.alpha_composite(Image.new("RGBA", big.size, (0, 0, 0, 255)), big).convert("RGB").save(os.path.join(DOCS, "icon.png"), optimize=True)
        for name in ("logo.png", "logo-light.png"):
            docs_logo(os.path.join(DOCS, name), 1.15)
    print("icons:", ", ".join(map(str, sizes)))
