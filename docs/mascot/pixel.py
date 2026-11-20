#!/usr/bin/env python3
"""Rivet as pixel art, for the website's grid band: he looks as if the computer swallowed him.

Shrinks the real drawing (scenes-retro/bare/solo.svg, from retro.py) to a few dozen pixels,
snaps every pixel to the site's palette, and writes pixel/rivet-pixel.svg as one square per
pixel, with the pixels of his closed eyes in a hidden layer for blinking. Needs ImageMagick. Run retro.py first.

    python3 docs/mascot/pixel.py
"""

import pathlib
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
SRC = HERE / "scenes-retro" / "bare" / "solo.svg"
OUT = HERE / "pixel"
WIDTH = 34  # pixels across; the height follows the drawing

PALETTE = ["#24121b", "#e9dee5", "#f7f2f4", "#b09aa2", "#e07aab"]


def rgb(h):
    return tuple(int(h[i:i + 2], 16) for i in (1, 3, 5))


def nearest(r, g, b):
    # Ink wins a close call: an outline that covers a third of a pixel should still be drawn,
    # or his outline breaks up.
    def far(h):
        d = sum((a - c) ** 2 for a, c in zip(rgb(h), (r, g, b)))
        return d * 0.3 if h == PALETTE[0] else d
    return min(PALETTE, key=far)


def pixels(src):
    """The drawing as a grid of palette colours (None where it is clear)."""
    # Drawn large, then averaged down, so each pixel is the colour that fills most of it.
    txt = subprocess.run(["magick", "-background", "none", "-density", "288", str(src), "-filter", "box", "-resize", f"{WIDTH}x", "txt:-"],
                         check=True, capture_output=True, text=True).stdout.splitlines()
    w, h = (int(n) for n in txt[0].split(":")[1].split(",")[:2])
    grid = [[None] * w for _ in range(h)]
    for line in txt[1:]:
        pos, rest = line.split(":", 1)
        x, y = (int(n) for n in pos.split(","))
        vals = [float(v) for v in rest.split("(")[1].split(")")[0].split(",")]
        scale = 255 / 65535 if max(vals) > 255 else 1
        r, g, b, a = (v * scale for v in vals[:4])
        if a >= 128:
            grid[y][x] = nearest(r, g, b)
    return grid


def rects(grid, only=None):
    """One rect per run of same-coloured pixels in a row; `only` keeps the pixels that differ from it."""
    out = []
    for y, row in enumerate(grid):
        x = 0
        while x < len(row):
            c = row[x]
            differs = only is None or only[y][x] != c
            n = 1
            while x + n < len(row) and row[x + n] == c and (only is None or (only[y][x + n] != c) == differs):
                n += 1
            if c and differs:
                out.append(f'<rect x="{x}" y="{y}" width="{n}" height="1" fill="{c}"/>')
            x += n
    return out


def main():
    OUT.mkdir(exist_ok=True)
    opened = pixels(SRC)
    shut = pixels(SRC.with_name("solo-shut.svg"))
    w, h = len(opened[0]), len(opened)
    # The blink is only the pixels that change, hidden until the site shows them.
    lids = rects(shut, only=opened)
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w * 8}" height="{h * 8}" shape-rendering="crispEdges">\n'
           f'<title>Rivet the mascot, in pixels</title>\n' + "\n".join(rects(opened))
           + '\n<g class="pixel-lids" opacity="0">\n' + "\n".join(lids) + "\n</g>\n</svg>\n")
    (OUT / "rivet-pixel.svg").write_text(svg)
    subprocess.run(["magick", "-background", "#0084b8", str(OUT / "rivet-pixel.svg"), str(OUT / "rivet-pixel.png")], check=True)
    (OUT / "rivet-pixel-shut.svg").write_text(svg.replace('opacity="0"', 'opacity="1"'))
    subprocess.run(["magick", "-background", "#0084b8", str(OUT / "rivet-pixel-shut.svg"), str(OUT / "rivet-pixel-shut.png")], check=True)
    (OUT / "rivet-pixel-shut.svg").unlink()
    print(f"Wrote {OUT / 'rivet-pixel.svg'}: {w}x{h} pixels, {len(lids)} rects change in the blink")


if __name__ == "__main__":
    main()
