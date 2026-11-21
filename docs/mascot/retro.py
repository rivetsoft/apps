#!/usr/bin/env python3
"""Rivet the mascot in the website's retro style: flat colours, thick ink outlines and hard
offset shadows, like the windows on the home page.

Each scene is 640x480 and sits on a card of its own colour (grid or dots), so it reads on the
site's light and dark themes alike; the ink outlines would vanish straight onto the dark one.
Run it to write scenes-retro/NAME.svg, and NAME.png at twice the size when ImageMagick is there.

    python3 docs/mascot/retro.py
"""

import pathlib
import re
import shutil
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / "scenes-retro"

# The site's palette (rivet-web, the home page's windows).
INK = "#24121b"
PAPER = "#e9dee5"
WHITE = "#f7f2f4"
PINK = "#d53f8c"
PINK_SOFT = "#e07aab"
YELLOW = "#fff270"
GREEN = "#3ecf9a"
BLUE = "#0084b8"
BLUE_LINE = "#40a3ca"
MAUVE = "#b09aa2"

W = 5  # outline width

DEFS = f"""
<pattern id="grid" width="24" height="24" patternUnits="userSpaceOnUse"><path d="M24 0H0V24" fill="none" stroke="{INK}" stroke-opacity="0.22" stroke-width="1.4"/></pattern>
<pattern id="gridBlue" width="28" height="28" patternUnits="userSpaceOnUse"><path d="M28 0H0V28" fill="none" stroke="{BLUE_LINE}" stroke-width="1.6"/></pattern>
<pattern id="dots" width="8" height="8" patternUnits="userSpaceOnUse"><circle cx="1.6" cy="1.6" r="1.2" fill="{INK}" fill-opacity="0.5"/></pattern>
<pattern id="dotsFine" width="6" height="6" patternUnits="userSpaceOnUse"><circle cx="1.2" cy="1.2" r="0.9" fill="{INK}" fill-opacity="0.35"/></pattern>
<clipPath id="bodyClip"><rect x="366" y="236" width="108" height="196" rx="16"/></clipPath>
<clipPath id="domeClip"><path d="M296 236 C296 130 544 130 544 236 C544 252 296 252 296 236 Z"/></clipPath>
"""


def hard(el, dx=10, dy=12):
    """`el` with a solid ink shadow behind it, offset down and to the right."""
    inked = re.sub(r'fill="(?!none)[^"]*"', f'fill="{INK}"', el)
    return f'<g transform="translate({dx} {dy})">{inked}</g>{el}'


# ---------------------------------------------------------------- Rivet

BROWS = {
    "knowing": ("M344 222 Q368 190 398 206", "M442 214 Q468 204 496 220"),
    "raised": ("M346 208 Q370 182 398 194", "M442 194 Q468 180 494 202"),
    "puzzled": ("M344 212 Q368 186 398 200", "M442 222 Q468 214 496 212"),
    "focused": ("M346 218 Q372 208 398 218", "M442 218 Q468 208 494 216"),
}


def mouth(kind):
    if kind == "open":
        return (f'<path d="M392 342 Q424 396 458 338 Q424 356 392 342 Z" fill="{INK}" stroke="{INK}" stroke-width="4" stroke-linejoin="round"/>'
                f'<ellipse cx="426" cy="366" rx="11" ry="5" fill="{PINK}"/>')
    if kind == "o":
        return f'<ellipse cx="424" cy="358" rx="10" ry="12" fill="{INK}"/>'
    if kind == "flat":
        return f'<path d="M404 354 Q424 362 446 350" fill="none" stroke="{INK}" stroke-width="7" stroke-linecap="round"/>'
    return f'<path d="M398 352 Q426 378 450 344" fill="none" stroke="{INK}" stroke-width="7" stroke-linecap="round"/>'


def rivet(x, y, s=1.0, tilt=7, look=(12, 7), brows="knowing", mood="grin", glasses=False, blush=True, shut=False, shades=False, shadow=True):
    """Rivet with the middle of his body at (x, y)."""
    lx, ly = look
    bl, br = BROWS[brows]
    eyes_at = ((382, 290), (460, 288))
    silhouette = (f'<rect x="366" y="236" width="108" height="196" rx="16" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>'
                  f'<ellipse cx="420" cy="240" rx="126" ry="20" fill="{MAUVE}" stroke="{INK}" stroke-width="{W}"/>'
                  f'<path d="M296 236 C296 130 544 130 544 236 C544 252 296 252 296 236 Z" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>'
                  + "".join(f'<ellipse cx="{cx}" cy="{cy}" rx="36" ry="40" fill="{WHITE}" stroke="{INK}" stroke-width="4"/>' for cx, cy in eyes_at))
    silhouette_shadow = re.sub(r'fill="(?!none)[^"]*"', f'fill="{INK}"', silhouette)
    body = (f'<rect x="366" y="236" width="108" height="196" rx="16" fill="{PAPER}"/>'
            f'<g clip-path="url(#bodyClip)"><rect x="446" y="236" width="30" height="196" fill="{MAUVE}"/>'
            f'<rect x="446" y="236" width="30" height="196" fill="url(#dotsFine)"/>'
            f'<rect x="366" y="236" width="108" height="22" fill="{MAUVE}"/>'
            f'<rect x="382" y="270" width="12" height="140" rx="6" fill="{WHITE}"/></g>'
            f'<rect x="366" y="236" width="108" height="196" rx="16" fill="none" stroke="{INK}" stroke-width="{W}"/>'
            f'<ellipse cx="420" cy="240" rx="126" ry="20" fill="{MAUVE}" stroke="{INK}" stroke-width="{W}"/>'
            f'<path d="M296 236 C296 130 544 130 544 236 C544 252 296 252 296 236 Z" fill="{PAPER}"/>'
            f'<g clip-path="url(#domeClip)"><path d="M470 120 Q560 170 548 250 H470 Z" fill="url(#dots)"/></g>'
            f'<path d="M296 236 C296 130 544 130 544 236 C544 252 296 252 296 236 Z" fill="none" stroke="{INK}" stroke-width="{W}"/>'
            f'<path d="M330 200 C342 166 388 154 426 158" fill="none" stroke="{WHITE}" stroke-width="10" stroke-linecap="round"/>')
    # No eyebrows over sunglasses: they crowd the lenses.
    face = "" if shades else (f'<path d="{bl}" fill="none" stroke="{INK}" stroke-width="12" stroke-linecap="round"/>'
                              f'<path d="{br}" fill="none" stroke="{INK}" stroke-width="12" stroke-linecap="round"/>')
    for cx, cy in eyes_at:
        face += f'<ellipse cx="{cx}" cy="{cy}" rx="36" ry="40" fill="{WHITE}" stroke="{INK}" stroke-width="4"/>'
        face += f'<ellipse cx="{cx + lx}" cy="{cy + ly}" rx="16" ry="19" fill="{INK}"/>'
        face += f'<circle cx="{cx + lx + 6}" cy="{cy + ly - 8}" r="5" fill="{WHITE}"/>'
    # Closed eyelids, invisible in the picture unless `shut`: the site shows them for a moment
    # to make him blink.
    face += f'<g class="mascot-lids" opacity="{1 if shut else 0}">' + "".join(
        f'<ellipse cx="{cx}" cy="{cy}" rx="36" ry="40" fill="{PAPER}" stroke="{INK}" stroke-width="4"/>'
        f'<path d="M{cx - 28} {cy + 6} Q{cx} {cy + 22} {cx + 28} {cy + 6}" fill="none" stroke="{INK}" stroke-width="6" stroke-linecap="round"/>'
        for cx, cy in eyes_at) + '</g>'
    if glasses:
        face += (f'<g fill="none" stroke="{INK}" stroke-width="6"><circle cx="382" cy="292" r="44"/><circle cx="460" cy="290" r="44"/>'
                 f'<path d="M424 286 Q421 278 418 286"/></g>')
    if shades:
        # Sunglasses over the lot: two dark lenses, a bridge, and a glint on each.
        face += (f'<path d="M418 284 H426" stroke="{INK}" stroke-width="9"/>'
                 f'<path d="M346 276 L330 266 M498 272 L512 262" stroke="{INK}" stroke-width="7" stroke-linecap="round"/>'
                 + "".join(f'<rect x="{x0}" y="258" width="78" height="58" rx="18" fill="{INK}" stroke="{INK}" stroke-width="4"/>'
                           f'<path d="M{x0 + 14} 292 L{x0 + 34} 270 M{x0 + 30} 298 L{x0 + 44} 282" stroke="{WHITE}" stroke-width="5" stroke-linecap="round"/>'
                           for x0 in (342, 424)))
    face += mouth(mood)
    if blush:
        face += (f'<ellipse cx="388" cy="342" rx="9" ry="5" fill="{PINK_SOFT}"/>'
                 f'<ellipse cx="460" cy="336" rx="9" ry="5" fill="{PINK_SOFT}"/>')
    d = 12 / s
    return (f'<g transform="translate({x} {y}) scale({s}) rotate({tilt}) translate(-420 -330)">'
            + (f'<g transform="rotate({-tilt} 420 330) translate({d} {d * 1.15}) rotate({tilt} 420 330)">{silhouette_shadow}</g>' if shadow else "")
            + f'{body}{face}</g>')


# ---------------------------------------------------------------- props

# Set while writing the bare set: scenes with no card, for placing on the site's blue band.
BARE = False


def card(color, pattern):
    """The scene's backdrop: a flat card with a grid or dots, like the home page's panels."""
    if BARE:
        return ""
    el = (f'<rect x="22" y="24" width="584" height="420" fill="{color}" stroke="{INK}" stroke-width="{W}"/>')
    return hard(el, 12, 14) + f'<rect x="22" y="24" width="584" height="420" fill="url(#{pattern})"/>' + \
        f'<rect x="22" y="24" width="584" height="420" fill="none" stroke="{INK}" stroke-width="{W}"/>'


def star(x, y, r=18, color=YELLOW):
    k = r * 0.36
    return (f'<path d="M{x} {y - r} L{x + k} {y - k} L{x + r} {y} L{x + k} {y + k} L{x} {y + r} '
            f'L{x - k} {y + k} L{x - r} {y} L{x - k} {y - k} Z" fill="{color}" stroke="{INK}" stroke-width="3.5" stroke-linejoin="round"/>')


def folder(x, y, w, h, color=GREEN, rot=0):
    tw = w * 0.38
    d = f"M{x} {y} h{tw} l14 16 H{x + w} V{y + h} H{x} Z"
    el = (f'<path d="{d}" fill="{color}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
          f'<path d="M{x} {y + 30} H{x + w}" stroke="{INK}" stroke-width="3"/>')
    return f'<g transform="rotate({rot} {x + w / 2} {y + h / 2})">{hard(el)}</g>'


def window(x, y, w, h, bar=PINK, title="", body=PAPER, content="", button=YELLOW):
    el = (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{body}" stroke="{INK}" stroke-width="4"/>'
          f'<rect x="{x}" y="{y}" width="{w}" height="34" fill="{bar}" stroke="{INK}" stroke-width="4"/>'
          f'<rect x="{x + w - 40}" y="{y + 8}" width="26" height="18" fill="{button}" stroke="{INK}" stroke-width="3"/>')
    out = hard(el, 12, 14)
    out += f'<path d="M{x + w - 32} {y + 12}l10 10M{x + w - 22} {y + 12}l-10 10" stroke="{INK}" stroke-width="2.4"/>'
    if title:
        out += f'<text x="{x + 14}" y="{y + 23}" fill="{PAPER}" font-family="Space Grotesk, DejaVu Sans, sans-serif" font-size="16" font-weight="700">{title}</text>'
    return out + f'<g transform="translate({x} {y + 34})">{content}</g>'


def bar(x, y, w, color, h=12):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{color}" stroke="{INK}" stroke-width="2.5"/>'


def paper(x, y, w, h, rot=0, lines=True, margin=True):
    el = f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{WHITE}" stroke="{INK}" stroke-width="4"/>'
    out = hard(el, 9, 10)
    if lines:
        for ly in range(int(y + 44), int(y + h - 14), 24):
            out += f'<line x1="{x + 12}" y1="{ly}" x2="{x + w - 12}" y2="{ly}" stroke="{BLUE_LINE}" stroke-width="2.5"/>'
    if margin:
        out += f'<line x1="{x + 42}" y1="{y + 4}" x2="{x + 42}" y2="{y + h - 4}" stroke="{PINK}" stroke-width="3"/>'
    return f'<g transform="rotate({rot} {x + w / 2} {y + h / 2})">{out}</g>'


def pencil(x, y, length=180, rot=-38):
    return f"""<g transform="translate({x} {y}) rotate({rot})">
<path d="M0 -12 H{length} V12 H0 L-34 0 Z" fill="{INK}" transform="translate(6 8)"/>
<rect x="0" y="-12" width="{length}" height="24" fill="{YELLOW}" stroke="{INK}" stroke-width="4"/>
<line x1="0" y1="0" x2="{length - 30}" y2="0" stroke="{INK}" stroke-width="2"/>
<rect x="{length - 30}" y="-12" width="30" height="24" fill="{PINK}" stroke="{INK}" stroke-width="4"/>
<path d="M0 -12 L-34 0 L0 12 Z" fill="{PAPER}" stroke="{INK}" stroke-width="4" stroke-linejoin="round"/>
<path d="M-22 -4 L-34 0 L-22 4 Z" fill="{INK}"/></g>"""


# ---------------------------------------------------------------- scenes

def scene_hello():
    """Home page: leaning on Rivet's folder."""
    return (card(BLUE, "gridBlue")
            + folder(300, 150, 260, 220, GREEN, -8)
            + rivet(240, 250, 0.84)
            + star(120, 92) + star(84, 140, 11))


def scene_files():
    """Rivet Files: popping up out of an open folder, among its papers."""
    x, y, w, h = 150, 200, 340, 210
    back = hard(f'<path d="M{x} {y} h120 l14 16 H{x + w} V{y + h} H{x} Z" fill="{PINK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    front = hard(f'<path d="M{x - 14} {y + 96} H{x + w + 14} L{x + w} {y + h} H{x} Z" fill="{PINK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
                 f'<path d="M{x - 14} {y + 96} H{x + w + 14} L{x + w} {y + h} H{x} Z" fill="url(#dots)"/>', 8, 10)
    return (card(YELLOW, "grid")
            + back
            + paper(178, 150, 110, 140, -10, margin=False)
            + paper(356, 146, 104, 140, 9, margin=False)
            + rivet(320, 256, 0.72, -3, look=(4, 4), brows="raised")
            + front
            + star(548, 82))


def scene_shell():
    """Rivet Shell: beside a desktop with its panel and dock."""
    sx, sy, sw, sh = 64, 70, 380, 250
    mon = (f'<rect x="{sx}" y="{sy}" width="{sw}" height="{sh}" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>')
    screen = (f'<rect x="{sx + 16}" y="{sy + 16}" width="{sw - 32}" height="{sh - 32}" fill="{PINK}" stroke="{INK}" stroke-width="4"/>'
              f'<rect x="{sx + 16}" y="{sy + 16}" width="{sw - 32}" height="{sh - 32}" fill="url(#dots)"/>'
              f'<rect x="{sx + 16}" y="{sy + 16}" width="{sw - 32}" height="24" fill="{INK}"/>'
              f'<rect x="{sx + 28}" y="{sy + 24}" width="30" height="8" fill="{PAPER}"/><rect x="{sx + sw - 80}" y="{sy + 24}" width="50" height="8" fill="{PAPER}"/>'
              + hard(f'<rect x="{sx + 120}" y="{sy + sh - 72}" width="{4 * 36 + 10}" height="42" fill="{INK}" stroke="{INK}" stroke-width="3"/>', 0, 0)
              + "".join(f'<rect x="{sx + 128 + i * 36}" y="{sy + sh - 64}" width="26" height="26" fill="{c}" stroke="{PAPER}" stroke-width="2.5"/>'
                        for i, c in enumerate([YELLOW, GREEN, BLUE, PINK_SOFT])))
    stand = (f'<rect x="{sx + sw / 2 - 26}" y="{sy + sh}" width="52" height="40" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>'
             f'<rect x="{sx + sw / 2 - 90}" y="{sy + sh + 38}" width="180" height="18" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>')
    return (card(GREEN, "dots") + hard(stand) + hard(mon) + screen
            + rivet(510, 300, 0.62, -6, look=(-14, 4)))


def scene_edit():
    """Rivet Edit: studying the code in an editor window."""
    rows = [(0, [(PINK, 60), (GREEN, 90)]), (1, [(YELLOW, 70), (PAPER, 110)]), (1, [(BLUE, 50), (PAPER, 80), (GREEN, 40)]),
            (2, [(PAPER, 120)]), (1, [(PINK, 40), (YELLOW, 60)]), (0, [(PAPER, 30)]), (0, []), (0, [(GREEN, 70), (PAPER, 100)])]
    code = f'<rect x="4" y="4" width="34" height="{8 * 28 + 18}" fill="{MAUVE}"/>'
    for i, (indent, parts) in enumerate(rows):
        yy = 20 + i * 28
        code += f'<text x="12" y="{yy + 11}" font-family="DejaVu Sans Mono, monospace" font-size="12" font-weight="700" fill="{INK}">{i + 1}</text>'
        xx = 52 + indent * 24
        for color, w in parts:
            code += bar(xx, yy, w, color)
            xx += w + 10
    code += f'<rect x="{52 + 24 + 50 + 10 + 80 + 10 + 40 + 8}" y="{20 + 2 * 28 - 4}" width="4" height="20" fill="{INK}"/>'
    return (card(BLUE, "gridBlue")
            + window(56, 64, 360, 290, PINK, "main.rs", WHITE, code)
            + rivet(510, 296, 0.66, -6, look=(-14, 6), brows="focused", mood="flat"))


def scene_terminal():
    """Rivet Terminal: at the prompt."""
    mono = 'font-family="DejaVu Sans Mono, monospace" font-size="17" font-weight="700"'
    content = (f'<text x="18" y="40" {mono} fill="{GREEN}">~ $</text><text x="62" y="40" {mono} fill="{PAPER}">cargo build</text>'
               f'<text x="18" y="70" {mono} fill="{MAUVE}">  Compiling rivet</text>'
               f'<text x="18" y="100" {mono} fill="{YELLOW}">  Finished</text><text x="130" y="100" {mono} fill="{MAUVE}">in 4.2 s</text>'
               f'<text x="18" y="140" {mono} fill="{GREEN}">~ $</text><rect x="62" y="125" width="12" height="20" fill="{PINK}"/>')
    return (card(PINK, "dots")
            + window(226, 70, 350, 240, BLUE, "Terminal", INK, content)
            + rivet(140, 290, 0.66, 6, look=(14, 4)))


def scene_notes():
    """Rivet Notes: leaning on a page of notes, pencil at the ready."""
    return (card(YELLOW, "grid")
            + paper(290, 70, 250, 330, 7)
            + pencil(352, 368, 170, -40)
            + rivet(214, 258, 0.82, 7, look=(12, 6), brows="raised", mood="open")
            + star(96, 86))


def scene_ftp():
    """Rivet FTP: a file on its way from this computer to a server."""
    laptop = (f'<rect x="56" y="250" width="160" height="108" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>'
              f'<path d="M36 358 H236 L222 380 H50 Z" fill="{PAPER}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    screen = (f'<rect x="70" y="264" width="132" height="80" fill="{PINK}" stroke="{INK}" stroke-width="3"/>'
              f'<rect x="70" y="264" width="132" height="80" fill="url(#dots)"/>')
    server = f'<rect x="444" y="170" width="132" height="210" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>'
    slots = "".join(f'<rect x="458" y="{186 + i * 46}" width="104" height="32" fill="{INK}"/>'
                    f'<rect x="470" y="{196 + i * 46}" width="12" height="12" fill="{GREEN if i != 1 else YELLOW}"/>'
                    f'<rect x="492" y="{198 + i * 46}" width="56" height="8" fill="{MAUVE}"/>' for i in range(4))
    arc = (f'<path d="M150 232 Q320 30 508 150" fill="none" stroke="{INK}" stroke-width="6" stroke-linecap="square" stroke-dasharray="10 12"/>'
           f'<path d="M488 132 L512 156 L478 162" fill="none" stroke="{INK}" stroke-width="6" stroke-linejoin="miter"/>')
    sheet = (f'<g transform="rotate(14 330 100)">' + hard(f'<rect x="306" y="70" width="48" height="62" fill="{WHITE}" stroke="{INK}" stroke-width="4"/>', 6, 7)
             + bar(314, 86, 30, PINK, 6) + bar(314, 98, 22, PINK, 6) + bar(314, 110, 28, PINK, 6) + '</g>')
    return (card(BLUE, "gridBlue") + hard(laptop) + screen + hard(server) + slots + arc + sheet
            + rivet(320, 320, 0.58, 0, look=(12, -10), brows="raised", mood="open"))


def scene_welcome():
    """Rivet Welcome: popping out of a parcel, freshly installed."""
    flaps = (f'<path d="M180 250 L120 192 L262 168 L320 222 Z" fill="{PAPER}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
             f'<path d="M460 250 L520 192 L378 168 L320 222 Z" fill="{PAPER}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    inside = f'<path d="M180 250 L320 222 L460 250 L320 280 Z" fill="{INK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
    front = (f'<path d="M180 250 L320 280 L320 430 L180 402 Z" fill="{YELLOW}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
             f'<path d="M460 250 L320 280 L320 430 L460 402 Z" fill="{YELLOW}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    shade = f'<path d="M460 250 L320 280 L320 430 L460 402 Z" fill="url(#dots)"/>'
    arrow = f'<path d="M250 318 V364 M230 344 L250 366 L270 344" fill="none" stroke="{INK}" stroke-width="9" stroke-linecap="square"/>'
    return (card(PINK, "dots") + hard(flaps) + inside
            + rivet(320, 200, 0.7, 4, look=(6, -4), brows="raised", mood="open")
            + hard(front) + shade + arrow
            + star(556, 90) + star(84, 130, 12))


def scene_plugins():
    """Plugins: fitting a new piece into the panel."""
    panel = f'<rect x="60" y="340" width="508" height="58" fill="{INK}" stroke="{INK}" stroke-width="4"/>'
    slots = ("".join(f'<rect x="{80 + i * 60}" y="354" width="44" height="30" fill="{c}" stroke="{PAPER}" stroke-width="2.5"/>' for i, c in enumerate([GREEN, BLUE, PINK_SOFT]))
             + f'<rect x="260" y="354" width="44" height="30" fill="none" stroke="{YELLOW}" stroke-width="3" stroke-dasharray="6 5"/>'
             + f'<rect x="460" y="364" width="90" height="10" fill="{MAUVE}"/>')
    piece = ('<g transform="translate(250 190) rotate(-12)">'
             + hard(f'<path d="M0 0 H22 a10 10 0 1 1 20 0 H64 V22 a10 10 0 1 1 0 20 V64 H42 a10 10 0 1 0 -20 0 H0 V42 a10 10 0 1 0 0 -20 Z" fill="{YELLOW}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>', 8, 9)
             + '</g>')
    arrow = f'<path d="M282 290 V326 M268 312 L282 328 L296 312" fill="none" stroke="{INK}" stroke-width="6" stroke-linecap="square"/>'
    return (card(GREEN, "dots") + hard(panel, 10, 12) + slots + piece + arrow
            + rivet(480, 200, 0.56, -6, look=(-14, -2)))


def scene_lost():
    """Not found: he tells the visitor he can't help, next to a mushroom he hasn't noticed."""
    sans = 'font-family="Space Grotesk, DejaVu Sans, sans-serif" font-weight="700"'
    # A plain mushroom, no face: the one thing here that is what it looks like.
    mushroom = hard(f'<rect x="482" y="366" width="36" height="52" rx="8" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>'
                    f'<path d="M442 372 C442 310 558 310 558 372 C558 382 442 382 442 372 Z" fill="{PINK}" stroke="{INK}" stroke-width="{W}"/>', 8, 9)
    spots = "".join(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{WHITE}" stroke="{INK}" stroke-width="2.5"/>' for cx, cy, r in [(476, 348, 8), (510, 338, 6), (532, 356, 7)])
    # He speaks to the visitor, Clippy's way: the offer, then the let-down.
    bubble = hard(f'<path d="M236 30 H612 V150 H372 L318 200 L330 150 H236 Z" fill="{WHITE}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>', 8, 9)
    words = (f'<text x="424" y="78" text-anchor="middle" {sans} font-size="26" fill="{INK}">It looks like you&#8217;re lost.</text>'
             f'<text x="424" y="116" text-anchor="middle" {sans} font-size="17" fill="{INK}">I can&#8217;t help with that. I&#8217;m a rivet.</text>')
    # Two layers, named so the site can move them with the pointer as the home page's pictures do.
    return (card(BLUE, "gridBlue")
            + f'<g class="mascot-back">{mushroom}{spots}</g>'
            + '<g class="mascot-front">' + rivet(210, 330, 0.74, -5, look=(5, 4), brows="knowing", mood="grin")
            + bubble + words + '</g>')


def scene_docs():
    """Docs: reading up, glasses on."""
    pages = (f'<path d="M320 326 L170 306 L170 424 L320 444 Z" fill="{WHITE}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
             f'<path d="M320 326 L470 306 L470 424 L320 444 Z" fill="{WHITE}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    cover = f'<path d="M160 312 L160 434 L320 456 L480 434 L480 312 L320 334 Z" fill="{PINK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
    lines = "".join(f'<path d="M188 {334 + i * 20} L300 {348 + i * 20}" stroke="{BLUE_LINE}" stroke-width="4"/>'
                    f'<path d="M340 {348 + i * 20} L452 {334 + i * 20}" stroke="{BLUE_LINE}" stroke-width="4"/>' for i in range(4))
    return (card(YELLOW, "grid")
            + rivet(320, 222, 0.74, -2, look=(-2, 16), brows="focused", mood="flat", glasses=True)
            + hard(cover) + pages + lines)


def scene_blog():
    """Blog: with something to tell."""
    bubble = (f'<path d="M318 64 H586 V226 H426 L384 270 L392 226 H318 Z" fill="{WHITE}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    text = bar(342, 92, 170, PINK, 16) + bar(342, 126, 218, PAPER) + bar(342, 150, 190, PAPER) + bar(342, 174, 120, PAPER)
    return (card(PINK, "dots") + hard(bubble) + text
            + rivet(200, 300, 0.68, 6, look=(14, -6), brows="raised", mood="open"))


def scene_named():
    """Beside the home page's Apps heading: he comes up out of his folder to say so."""
    sans = 'font-family="Space Grotesk, DejaVu Sans, sans-serif" font-weight="700"'
    # His folder, as Clippy had his sheet of paper: the back, then him, then the front over
    # his lower half, so he is standing in it.
    x, y, w, h = 572, 118, 176, 104
    back = hard(f'<path d="M{x} {y} h66 l12 14 H{x + w} V{y + h} H{x} Z" fill="#992f64" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>', 8, 9)
    front = (f'<path d="M{x - 8} {y + 46} H{x + w + 8} L{x + w} {y + h} H{x} Z" fill="{PINK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
             f'<path d="M{x - 8} {y + 46} H{x + w + 8} L{x + w} {y + h} H{x} Z" fill="url(#dots)"/>'
             f'<path d="M{x - 8} {y + 46} H{x + w + 8} L{x + w} {y + h} H{x} Z" fill="none" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    bubble = hard(f'<path d="M18 34 H508 V92 L548 108 L508 124 V162 H18 Z" fill="{WHITE}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>', 8, 9)
    words = (f'<text x="263" y="90" text-anchor="middle" {sans} font-size="25" fill="{INK}">They named all of these after me.</text>'
             f'<text x="263" y="130" text-anchor="middle" {sans} font-size="19" fill="{INK}">I don&#8217;t know what any of them do.</text>')
    # He, his folder and his bubble are separate layers, in case the site wants to move them.
    return (card(BLUE, "gridBlue")
            + f'<g class="mascot-folder-back">{back}</g>'
            + '<g class="mascot-rivet">' + rivet(660, 128, 0.52, 5, look=(-8, 12), brows="raised", mood="flat") + '</g>'
            + f'<g class="mascot-folder-front">{front}</g>'
            + f'<g class="mascot-bubble">{bubble}{words}</g>')


def crt(x, y, w, h, screen, rot=0):
    """A beige 90s monitor, `w` by `h`, with `screen` drawn on its glass (in a 100 by 70 box)."""
    k = (w - 24) / 100
    el = (f'<path d="M{x + w * 0.3} {y + h} h{w * 0.4} l8 14 h-{w * 0.4 + 16} Z" fill="{MAUVE}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
          f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="{PAPER}" stroke="{INK}" stroke-width="{W}"/>')
    glass = (f'<rect x="{x + 12}" y="{y + 10}" width="{w - 24}" height="{70 * k:.1f}" rx="5" fill="{INK}" stroke="{INK}" stroke-width="3"/>'
             f'<g transform="translate({x + 12} {y + 10}) scale({k:.3f})">{screen}</g>'
             f'<circle cx="{x + w - 16}" cy="{y + h - 9}" r="3.2" fill="{GREEN}"/>'
             f'<path d="M{x + 14} {y + h - 9} h{w * 0.3}" stroke="{INK}" stroke-width="3" stroke-linecap="round"/>')
    return f'<g transform="rotate({rot} {x + w / 2} {y + h / 2})">{hard(el, 7, 8)}{glass}</g>'


def scene_vibes():
    """The website's footer: he owns up to how all of this was made, beside the computers that did it."""
    sans = 'font-family="Space Grotesk, DejaVu Sans, sans-serif" font-weight="700"'
    bubble = hard(f'<path d="M14 22 H390 V84 L430 100 L390 116 V178 H14 Z" fill="{WHITE}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>', 8, 9)
    lines = (("An idiot told a bunch of computers", 21, 62), ("what to do, and they did it.", 21, 90),
             ("He probably didn&#8217;t read the code.", 21, 118), ("I certainly didn&#8217;t, I&#8217;m a rivet.", 17, 152))
    words = "".join(f'<text x="202" y="{ty}" text-anchor="middle" {sans} font-size="{size}" fill="{INK}">{t}</text>' for t, size, ty in lines)
    code = "".join(f'<path d="M{8 + ind} {12 + n * 11} h{length}" stroke="{color}" stroke-width="4.5" stroke-linecap="round"/>'
                   for n, (ind, length, color) in enumerate(((0, 46, GREEN), (10, 62, GREEN), (10, 30, YELLOW), (20, 50, GREEN), (0, 22, PINK_SOFT))))
    grin = (f'<circle cx="36" cy="26" r="5" fill="{YELLOW}"/><circle cx="64" cy="26" r="5" fill="{YELLOW}"/>'
            f'<path d="M28 42 Q50 62 72 42" fill="none" stroke="{YELLOW}" stroke-width="6" stroke-linecap="round"/>')
    okay = f'<text x="50" y="50" text-anchor="middle" font-family="DejaVu Sans Mono, monospace" font-weight="700" font-size="38" fill="{GREEN}">OK</text>'
    machines = crt(590, 124, 124, 100, code) + crt(700, 138, 104, 86, okay, 3) + crt(626, 30, 112, 92, grin, -6)
    return ('<g class="mascot-back">' + machines + '</g>'
            + '<g class="mascot-rivet">' + rivet(506, 172, 0.52, -5, look=(-12, -6), brows="raised", mood="o") + '</g>'
            + f'<g class="mascot-bubble">{bubble}{words}</g>')


def scene_island():
    """Rivet Welcome: it installs everything, so he is on a small island with a coconut."""
    dark = "#6a3050"
    cx, cy = 450, 322
    # A wide sea with rings of ripples spreading from the island, and a few wave marks.
    sea = f'<ellipse cx="{cx}" cy="{cy}" rx="436" ry="92" fill="{BLUE}" stroke="{INK}" stroke-width="{W}"/>'
    rings = "".join(f'<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" fill="none" stroke="{BLUE_LINE}" stroke-width="3.5" stroke-linecap="round" stroke-dasharray="{dash}" stroke-dashoffset="{off}"/>'
                    for rx, ry, dash, off in [(262, 38, "54 30", 0), (322, 54, "70 44", 30), (376, 70, "48 60", 12), (414, 82, "30 80", 50)])
    waves = "".join(f'<path d="M{x} {y}q9 -9 18 0t18 0" fill="none" stroke="{WHITE}" stroke-opacity="0.55" stroke-width="3.5" stroke-linecap="round"/>'
                    for x, y in [(96, 318), (190, 372), (420, 388), (640, 376), (764, 320), (300, 262), (590, 258)])
    # Shark fins. Each is placed where it stands in the picture; the site sends them round the
    # island on a loop. They are drawn under the sand, so the island hides one that is behind it.
    fin = (f'<path d="M-15 0 Q-6 -30 12 -36 Q9 -15 17 0 Z" fill="{MAUVE}" stroke="{INK}" stroke-width="4" stroke-linejoin="round"/>'
           f'<path d="M-24 2 q9 -7 18 0 M10 2 q9 -7 18 0" fill="none" stroke="{WHITE}" stroke-width="3" stroke-linecap="round"/>')
    sharks = "".join(f'<g class="mascot-shark" transform="translate({x} {y})">{fin}</g>' for x, y in [(150, 340), (700, 372), (770, 300)])
    sand_d = "M226 322 Q450 196 674 322 Q450 352 226 322 Z"
    sand = (f'<path d="{sand_d}" fill="{YELLOW}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
            f'<path d="{sand_d}" fill="url(#dotsFine)"/>')
    # The palm: a leaning trunk with rings, five fronds and two coconuts.
    trunk_d = "M582 292 Q566 192 608 118"
    trunk = (f'<path d="{trunk_d}" fill="none" stroke="{INK}" stroke-width="29" stroke-linecap="round"/>'
             f'<path d="{trunk_d}" fill="none" stroke="{MAUVE}" stroke-width="19" stroke-linecap="round"/>'
             + "".join(f'<path d="M{x - 10} {y} q10 6 20 0" fill="none" stroke="{INK}" stroke-width="3.5"/>' for x, y in [(580, 262), (577, 228), (579, 194), (586, 162), (596, 136)]))
    frond = "M0 0 Q46 -44 104 -10 Q52 -14 0 0 Z"
    fronds = "".join(f'<path d="{frond}" fill="{GREEN}" stroke="{INK}" stroke-width="4.5" stroke-linejoin="round" transform="translate(608 116) rotate({r}) scale({k})"/>'
                     for r, k in [(20, 1.0), (-28, 1.05), (-150, 1.0), (158, 0.95), (-92, 0.9)])
    nuts = "".join(f'<circle cx="{x}" cy="{y}" r="11" fill="{dark}" stroke="{INK}" stroke-width="4"/>' for x, y in [(598, 130), (618, 134)])
    # The lounge chair, side on: a striped backrest that runs up behind his back and past his
    # head, a seat, and a leg at each end. He leans into the corner of it.
    back_d = "M156 131 L241 252 L228 262 L143 141 Z"
    seat_d = "M234 252 H352 V268 H226 Z"
    legs = f'<path d="M238 268 L232 288 M342 268 L348 288" fill="none" stroke="{INK}" stroke-width="9" stroke-linecap="round"/>'
    cushions = (f'<path d="{back_d}" fill="{PINK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
                f'<path d="{seat_d}" fill="{PINK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    stripes = ("".join(f'<path d="M{156 + 85 * t:.1f} {131 + 121 * t:.1f} l-13 10" stroke="{WHITE}" stroke-width="7"/>' for t in (0.14, 0.34, 0.54, 0.74))
               + "".join(f'<path d="M{x} 254 v12" stroke="{WHITE}" stroke-width="7"/>' for x in (262, 290, 318)))
    # A coconut on the seat in front of him, with a short bent straw to his mouth.
    coconut = (f'<circle cx="304" cy="232" r="19" fill="{dark}" stroke="{INK}" stroke-width="{W}"/>'
               f'<ellipse cx="304" cy="216" rx="13" ry="5" fill="{WHITE}" stroke="{INK}" stroke-width="3.5"/>')
    straw_d = "M304 217 L295 196 L251 207"
    straw = (f'<path d="{straw_d}" fill="none" stroke="{INK}" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>'
             f'<path d="{straw_d}" fill="none" stroke="{WHITE}" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>'
             f'<path d="{straw_d}" fill="none" stroke="{PINK}" stroke-width="5" stroke-dasharray="6 7"/>')
    # Small things carry no hard shadows here: at this size they only muddy it.
    lounger = (legs + cushions + stripes + rivet(232, 200, 0.42, -35, mood="o", shades=True, shadow=False) + coconut + straw)
    sun = f'<circle cx="150" cy="84" r="34" fill="{YELLOW}" stroke="{INK}" stroke-width="{W}"/>'
    return (card(BLUE, "gridBlue") + sun + sea + rings + waves + sharks + sand + trunk + fronds + nuts
            + f'<g transform="translate(150 -12)">{lounger}</g>')


def scene_solo():
    """Rivet on his own, startled: the drawing pixel.py turns into pixel art."""
    return rivet(142, 172, 1.0, 4, look=(4, 2), brows="raised", mood="o")


def scene_solo_shut():
    """The same, mid-blink: pixel.py's second frame."""
    return rivet(142, 172, 1.0, 4, look=(4, 2), brows="raised", mood="o", shut=True)


# Scenes that are not the usual 640x480.
SIZES = {"named": (770, 236), "vibes": (830, 246), "solo": (300, 340), "solo-shut": (300, 340), "island": (900, 424)}

SCENES = {
    "hello": scene_hello,
    "files": scene_files,
    "shell": scene_shell,
    "edit": scene_edit,
    "terminal": scene_terminal,
    "notes": scene_notes,
    "ftp": scene_ftp,
    "welcome": scene_welcome,
    "plugins": scene_plugins,
    "not-found": scene_lost,
    "docs": scene_docs,
    "blog": scene_blog,
    "named": scene_named,
    "vibes": scene_vibes,
    "island": scene_island,
    "solo": scene_solo,
    "solo-shut": scene_solo_shut,
}


def main():
    global BARE
    magick = shutil.which("magick")
    # Each scene twice: on its card, and bare (bare/NAME.svg) for a coloured band of the site.
    for bare, out in ((False, OUT), (True, OUT / "bare")):
        BARE = bare
        out.mkdir(parents=True, exist_ok=True)
        for name, draw in SCENES.items():
            title = f"Rivet the mascot: {draw.__doc__.strip()}"
            w, h = SIZES.get(name, (640, 480))
            svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">\n'
                   f'<title>{title}</title>\n<defs>{DEFS}</defs>\n{draw()}\n</svg>\n')
            path = out / f"{name}.svg"
            path.write_text(svg)
            if magick:
                subprocess.run([magick, "-background", "none", "-density", "192", str(path), str(out / f"{name}.png")], check=True)
    print(f"Wrote {len(SCENES)} scenes to {OUT}, and bare ones to {OUT / 'bare'}")


if __name__ == "__main__":
    main()
