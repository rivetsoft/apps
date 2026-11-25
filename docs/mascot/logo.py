#!/usr/bin/env python3
"""Rivetsoft's logo, from the retro Rivet in retro.py.

Writes logo/rivetsoft-mark.svg (the square badge: Rivet looking up over the bottom edge),
logo/rivetsoft-logo.svg (the badge beside the name) and logo/rivetsoft-logo-dark.svg (the same
for dark backgrounds). The name is drawn as outlines from Space Grotesk Bold, so the files look
the same anywhere; pass the font file as the first argument:

    python3 logo.py path/to/space-grotesk-latin-700-normal.woff
"""
import pathlib
import shutil
import subprocess
import sys

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont

import retro
from retro import INK, PAPER, PINK, YELLOW, W

OUT = pathlib.Path(__file__).resolve().parent / "logo"


def badge(size=512, color=YELLOW):
    """The mark: a square card with its hard shadow, and Rivet coming up over the bottom edge."""
    pad, sh = 16, 22
    side = size - pad * 2 - sh
    box = f'<rect x="{pad}" y="{pad}" width="{side}" height="{side}" rx="34"'
    k = side / 474
    him = retro.rivet(pad + side * 0.5, pad + side * 0.86, 1.5 * k, 5, look=(8, -4), brows="knowing", mood="grin", shadow=False)
    return (f'<defs><clipPath id="badgeClip">{box}/></clipPath></defs>'
            f'{box} fill="{INK}" transform="translate({sh} {sh})"/>'
            f'{box} fill="{color}"/>'
            f'<g clip-path="url(#badgeClip)">{box} fill="url(#dots)" opacity="0.5"/>{him}</g>'
            f'{box} fill="none" stroke="{INK}" stroke-width="{W * 2}"/>')


def word(font, text, x, y, size, fill):
    """`text` as outlines, with its baseline at (x, y)."""
    glyphs, cmap, k = font.getGlyphSet(), font.getBestCmap(), size / font["head"].unitsPerEm
    out, at = "", 0
    for ch in text:
        name = cmap[ord(ch)]
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(pen)
        out += f'<path transform="translate({x + at * k:.2f} {y}) scale({k:.5f} {-k:.5f})" d="{pen.getCommands()}"/>'
        at += glyphs[name].width - 14  # set a little tight, as a logo is
    return f'<g fill="{fill}">{out}</g>', at * k


def lockup(font, ink, soft):
    mark = f'<g transform="scale(0.5)">{badge()}</g>'
    a, wa = word(font, "Rivet", 290, 170, 132, ink)
    b, wb = word(font, "soft", 290 + wa, 170, 132, soft)
    return mark + a + b, int(290 + wa + wb + 24)


def write(name, w, h, body, title):
    OUT.mkdir(exist_ok=True)
    path = OUT / f"{name}.svg"
    path.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">\n'
                    f'<title>{title}</title>\n<defs>{retro.DEFS}</defs>\n{body}\n</svg>\n')
    if magick := shutil.which("magick"):
        subprocess.run([magick, "-background", "none", "-density", "192", str(path), str(OUT / f"{name}.png")], check=True)


def main():
    font = TTFont(sys.argv[1])
    write("rivetsoft-mark", 512, 512, badge(), "Rivetsoft")
    body, w = lockup(font, INK, PINK)
    write("rivetsoft-logo", w, 256, body, "Rivetsoft")
    body, w = lockup(font, PAPER, "#ef7ab5")
    write("rivetsoft-logo-dark", w, 256, body, "Rivetsoft")


if __name__ == "__main__":
    main()
