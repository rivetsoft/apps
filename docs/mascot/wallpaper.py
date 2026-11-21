#!/usr/bin/env python3
"""A desktop wallpaper in the website's retro style, with Rivet in it.

Writes wallpaper/rivet-dusk.svg and, with ImageMagick, wallpaper/rivet-dusk.png (2880x1620):
a pink dusk with a halftone sky and a striped sun, hills cut like the site's wave bands, a grid
floor, and Rivet standing in his folder at the lower left, where a window in the middle of the
screen leaves him in view.
"""
import math
import pathlib
import random
import shutil
import subprocess

import retro
from retro import BLUE, BLUE_LINE, INK, PAPER, PINK, PINK_SOFT, WHITE, YELLOW

OUT = pathlib.Path(__file__).resolve().parent / "wallpaper"
W, H = 2880, 1620
LINE = 10
PLUM = "#992f64"
DEEP = "#5e1d45"


def sky():
    """Flat pink, with halftone dots that are large at the top and gone by the hills."""
    bands, out, defs = 14, f'<rect width="{W}" height="{H}" fill="{PINK}"/>', ""
    for i in range(bands):
        r = 9.5 * (1 - i / bands) ** 1.25
        defs += (f'<pattern id="sky{i}" width="34" height="34" patternUnits="userSpaceOnUse">'
                 f'<circle cx="17" cy="17" r="{r:.2f}" fill="{DEEP}" fill-opacity="0.55"/></pattern>')
        out += f'<rect x="0" y="{i * 68}" width="{W}" height="69" fill="url(#sky{i})"/>'
    return defs, out


def sun(cx, cy, r):
    """The sun every 80s sunset had: a disc with bars cut out of its lower half."""
    disc = f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{YELLOW}" stroke="{INK}" stroke-width="{LINE}"/>'
    bars = "".join(f'<rect x="{cx - r - 20}" y="{cy + r * t:.0f}" width="{2 * r + 40}" height="{h}" fill="{PINK}"/>'
                   f'<path d="M{cx - r} {cy + r * t:.0f}h{2 * r}M{cx - r} {cy + r * t + h:.0f}h{2 * r}" stroke="{INK}" stroke-width="5"/>'
                   for t, h in ((0.18, 16), (0.38, 24), (0.58, 34), (0.8, 46)))
    return (f'<clipPath id="sunClip"><circle cx="{cx}" cy="{cy}" r="{r}"/></clipPath>'
            f'<circle cx="{cx + 26}" cy="{cy + 30}" r="{r}" fill="{INK}"/>{disc}'
            f'<g clip-path="url(#sunClip)">{bars}</g>'
            f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{INK}" stroke-width="{LINE}"/>')


def hill(y, amp, length, phase, fill, pattern=None):
    """A band of hills: a wave across the picture, filled down to the bottom."""
    pts = [(x, y + amp * math.sin((x + phase) * 2 * math.pi / length)) for x in range(-40, W + 80, 40)]
    d = "M" + " L".join(f"{x} {py:.1f}" for x, py in pts) + f" V{H + 20} H-40 Z"
    out = f'<path d="{d}" fill="{fill}"/>'
    if pattern:
        out += f'<path d="{d}" fill="url(#{pattern})"/>'
    return out + f'<path d="{d}" fill="none" stroke="{INK}" stroke-width="{LINE}" stroke-linejoin="round"/>'


def floor(top):
    """The grid floor from the site's last band, running off to a horizon at `top`."""
    out = f'<rect x="-20" y="{top}" width="{W + 40}" height="{H - top + 20}" fill="{INK}"/>'
    vx = W * 0.5
    for i in range(-22, 23):
        out += f'<path d="M{vx + i * 34:.0f} {top} L{vx + i * 330:.0f} {H}" stroke="{BLUE_LINE}" stroke-width="3.5" opacity="0.8"/>'
    y, step = top, 10.0
    while y < H:
        out += f'<path d="M0 {y:.0f}H{W}" stroke="{BLUE_LINE}" stroke-width="3.5" opacity="0.8"/>'
        step *= 1.34
        y += step
    return out + f'<path d="M-20 {top}H{W + 20}" stroke="{INK}" stroke-width="{LINE}"/>'


def mushroom(x, y, s, cap=PINK_SOFT):
    return (f'<g transform="translate({x} {y}) scale({s})" stroke="{INK}" stroke-width="{5 / s + 4:.1f}" stroke-linejoin="round">'
            f'<path d="M-13 0 L-10 -44 H10 L13 0 Z" fill="{PAPER}"/>'
            f'<path d="M-46 -40 C-46 -98 46 -98 46 -40 C46 -30 -46 -30 -46 -40 Z" fill="{cap}"/>'
            f'<circle cx="-18" cy="-62" r="7" fill="{WHITE}" stroke="none"/><circle cx="14" cy="-70" r="5" fill="{WHITE}" stroke="none"/>'
            f'<circle cx="22" cy="-50" r="4" fill="{WHITE}" stroke="none"/></g>')


def rivet_in_folder(x, y, s):
    """Rivet standing in his folder, as beside the Apps heading, looking up at the screen."""
    w, h = 176 * s / 0.52, 104 * s / 0.52
    fx, fy = x - w / 2, y - 10 * s / 0.52
    tab = 66 * s / 0.52
    back = retro.hard(f'<path d="M{fx} {fy} h{tab} l{24 * s} {28 * s} H{fx + w} V{fy + h} H{fx} Z" fill="{PLUM}" stroke="{INK}" stroke-width="{LINE}" stroke-linejoin="round"/>', 20, 24)
    lip, drop = 16 * s, 46 * s / 0.52
    d = f"M{fx - lip} {fy + drop} H{fx + w + lip} L{fx + w} {fy + h} H{fx} Z"
    front = (f'<path d="{d}" fill="{PINK}"/><path d="{d}" fill="url(#dotsBig)"/>'
             f'<path d="{d}" fill="none" stroke="{INK}" stroke-width="{LINE}" stroke-linejoin="round"/>')
    him = retro.rivet(x, y, s, 6, look=(12, -8), brows="knowing", mood="grin")
    return back + him + front


def main():
    rng = random.Random(7)
    defs, picture = sky()
    defs += (f'<pattern id="dotsBig" width="20" height="20" patternUnits="userSpaceOnUse"><circle cx="5" cy="5" r="3.2" fill="{INK}" fill-opacity="0.45"/></pattern>'
             f'<pattern id="dotsHill" width="26" height="26" patternUnits="userSpaceOnUse"><circle cx="6" cy="6" r="3.6" fill="{INK}" fill-opacity="0.28"/></pattern>')
    picture += "".join(retro.star(x, y, r) for x, y, r in ((560, 760, 34), (980, 150, 26), (1500, 260, 40), (2010, 120, 24), (2700, 250, 34), (1180, 560, 22), (2600, 820, 26)))
    picture += sun(2230, 700, 400)
    picture += hill(980, 70, 1700, 300, PLUM, "dotsHill")
    picture += hill(1120, 55, 1250, 900, BLUE)
    picture += floor(1290)
    for _ in range(9):
        x = rng.uniform(900, 2800)
        y = rng.uniform(1330, 1580)
        picture += mushroom(x, y, 0.5 + (y - 1290) / 300, rng.choice((PINK_SOFT, YELLOW, "#3ecf9a")))
    picture += rivet_in_folder(430, 1240, 1.9)
    OUT.mkdir(exist_ok=True)
    path = OUT / "rivet-dusk.svg"
    path.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">\n'
                    f'<title>Rivet at dusk: a wallpaper</title>\n<defs>{retro.DEFS}{defs}</defs>\n{picture}\n</svg>\n')
    if magick := shutil.which("magick"):
        subprocess.run([magick, "-background", "none", str(path), str(OUT / "rivet-dusk.png")], check=True)


if __name__ == "__main__":
    main()
