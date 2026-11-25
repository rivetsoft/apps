#!/usr/bin/env python3
"""Rivet the mascot, in scenes for the website.

The character is drawn once (`rivet()`), with a few expressions, and each scene places him among
its own props. Every scene is 640x480 on a transparent background, so it sits on the site's light
and dark themes alike. Run it to write scenes/NAME.svg, and NAME.png at twice the size when
ImageMagick is installed.

    python3 docs/mascot/make.py
"""

import pathlib
import shutil
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / "scenes"

PINK = "#d53f8c"
PINK_LIGHT = "#eb65a6"
INK = "#24192c"
TEAL = "#1f9e9a"
AMBER = "#e5a50a"
PURPLE = "#9141ac"

DEFS = """
<linearGradient id="folderFront" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef7ab5"/><stop offset="1" stop-color="#d53f8c"/></linearGradient>
<linearGradient id="folderBack" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#b8487f"/><stop offset="1" stop-color="#8e2f60"/></linearGradient>
<linearGradient id="body" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#c4bdcf"/><stop offset="0.22" stop-color="#ffffff"/><stop offset="0.6" stop-color="#e9e5ee"/><stop offset="1" stop-color="#9d95ad"/></linearGradient>
<linearGradient id="underDome" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4b3f5c" stop-opacity="0.35"/><stop offset="1" stop-color="#4b3f5c" stop-opacity="0"/></linearGradient>
<linearGradient id="pinkBounce" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#eb65a6" stop-opacity="0"/><stop offset="1" stop-color="#eb65a6" stop-opacity="0.4"/></linearGradient>
<radialGradient id="dome" cx="0.35" cy="0.22" r="0.85"><stop offset="0" stop-color="#ffffff"/><stop offset="0.45" stop-color="#f1eef5"/><stop offset="1" stop-color="#ada5bd"/></radialGradient>
<linearGradient id="rim" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#a39bb3"/><stop offset="0.4" stop-color="#ddd8e5"/><stop offset="1" stop-color="#8c839f"/></linearGradient>
<radialGradient id="eye" cx="0.4" cy="0.35" r="0.7"><stop offset="0" stop-color="#ffffff"/><stop offset="0.8" stop-color="#f4f2f6"/><stop offset="1" stop-color="#d6d1dc"/></radialGradient>
<linearGradient id="paper" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#f3ecf1"/></linearGradient>
<linearGradient id="screen" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#7a2a5a"/><stop offset="0.55" stop-color="#c2407f"/><stop offset="1" stop-color="#f08abd"/></linearGradient>
<linearGradient id="metal" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#d9d6df"/><stop offset="0.5" stop-color="#f6f5f8"/><stop offset="1" stop-color="#b9b4c3"/></linearGradient>
<linearGradient id="dark" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#34202c"/><stop offset="1" stop-color="#24141d"/></linearGradient>
<linearGradient id="boxFront" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef7ab5"/><stop offset="1" stop-color="#c43a80"/></linearGradient>
<linearGradient id="boxFlap" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f6a3cb"/><stop offset="1" stop-color="#e46aa8"/></linearGradient>
<clipPath id="bodyClip"><rect x="366" y="236" width="108" height="196" rx="30"/></clipPath>
<filter id="soft" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="9"/></filter>
<filter id="softer" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="14"/></filter>
"""

# Dark props (windows, the monitor, the server) get a faint edge so they stand out on the
# site's dark theme too.
# Expressions. Coordinates are the character's own (he stands about x 296-544, y 130-432).
BROWS = {
    # One raised: the knowing look he has on the home page.
    "knowing": ("M344 222 Q368 190 398 206", "M442 214 Q468 204 496 220"),
    "raised": ("M346 208 Q370 182 398 194", "M442 194 Q468 180 494 202"),
    "puzzled": ("M344 212 Q368 186 398 200", "M442 222 Q468 214 496 212"),
    "focused": ("M346 218 Q372 208 398 218", "M442 218 Q468 208 494 216"),
}


def mouth(kind):
    if kind == "open":
        return (f'<path d="M392 342 Q424 396 458 338 Q424 356 392 342 Z" fill="{INK}"/>'
                f'<ellipse cx="426" cy="366" rx="11" ry="5" fill="{PINK_LIGHT}"/>')
    if kind == "o":
        return f'<ellipse cx="424" cy="358" rx="10" ry="12" fill="{INK}"/>'
    if kind == "flat":
        return f'<path d="M404 354 Q424 362 446 350" fill="none" stroke="{INK}" stroke-width="6" stroke-linecap="round"/>'
    return f'<path d="M398 352 Q426 378 450 344" fill="none" stroke="{INK}" stroke-width="6" stroke-linecap="round"/>'


def rivet(x, y, s=1.0, tilt=7, look=(12, 7), brows="knowing", mood="grin", glasses=False, blush=True):
    """Rivet with the middle of his body at (x, y)."""
    lx, ly = look
    bl, br = BROWS[brows]
    eyes = ""
    for cx, cy in ((382, 290), (460, 288)):
        eyes += f'<ellipse cx="{cx + 3}" cy="{cy + 5}" rx="36" ry="40" fill="#4b3f5c" opacity="0.18"/>'
    for cx, cy in ((382, 290), (460, 288)):
        eyes += f'<ellipse cx="{cx}" cy="{cy}" rx="36" ry="40" fill="url(#eye)" stroke="#8c7d92" stroke-width="2"/>'
        eyes += f'<ellipse cx="{cx + lx}" cy="{cy + ly}" rx="15" ry="18" fill="#1b1220"/>'
        eyes += f'<circle cx="{cx + lx + 6}" cy="{cy + ly - 8}" r="5" fill="#ffffff"/>'
    specs = ""
    if glasses:
        specs = (f'<g fill="none" stroke="{INK}" stroke-width="5">'
                 f'<circle cx="382" cy="292" r="44"/><circle cx="460" cy="290" r="44"/>'
                 f'<path d="M424 286 Q421 280 418 286"/></g>')
    cheeks = ""
    if blush:
        cheeks = (f'<ellipse cx="388" cy="342" rx="10" ry="6" fill="{PINK_LIGHT}" opacity="0.45"/>'
                  f'<ellipse cx="460" cy="336" rx="10" ry="6" fill="{PINK_LIGHT}" opacity="0.45"/>')
    return f"""<g transform="translate({x} {y}) scale({s}) rotate({tilt}) translate(-420 -330)">
<rect x="366" y="236" width="108" height="196" rx="30" fill="url(#body)"/>
<g clip-path="url(#bodyClip)"><rect x="366" y="236" width="108" height="36" fill="url(#underDome)"/><rect x="440" y="236" width="34" height="196" fill="url(#pinkBounce)"/><rect x="380" y="262" width="13" height="152" rx="6.5" fill="#ffffff" opacity="0.75"/></g>
<rect x="366" y="236" width="108" height="196" rx="30" fill="none" stroke="#8a8199" stroke-width="3"/>
<ellipse cx="420" cy="240" rx="126" ry="20" fill="url(#rim)" stroke="#8a8199" stroke-width="2"/>
<path d="M296 236 C296 130 544 130 544 236 C544 252 296 252 296 236 Z" fill="url(#dome)" stroke="#8a8199" stroke-width="3"/>
<path d="M328 196 C340 160 390 148 428 152" fill="none" stroke="#ffffff" stroke-width="10" stroke-linecap="round" opacity="0.9"/>
<path d="{bl}" fill="none" stroke="{INK}" stroke-width="11" stroke-linecap="round"/>
<path d="{br}" fill="none" stroke="{INK}" stroke-width="11" stroke-linecap="round"/>
{eyes}{specs}{mouth(mood)}{cheeks}
</g>"""


def stand(x, y, s=1.0, **kw):
    """Rivet standing on the ground, his shadow under his feet."""
    feet = y + 102 * s
    return shadow(x, feet + 2, 118 * s, 13 * s, 0, 0.32) + rivet(x, y, s, **kw)


# ---------------------------------------------------------------- props

def shadow(cx, cy, rx, ry, rot=0, op=0.28, color="#3d1830"):
    return f'<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" fill="{color}" opacity="{op}" filter="url(#soft)" transform="rotate({rot} {cx} {cy})"/>'


def sparkle(x, y, r=18, op=1.0, color="#ffffff"):
    k = r * 0.39
    return (f'<path d="M{x} {y - r} L{x + k} {y - k} L{x + r} {y} L{x + k} {y + k} L{x} {y + r} '
            f'L{x - k} {y + k} L{x - r} {y} L{x - k} {y - k} Z" fill="{color}" opacity="{op}"/>')


def glints(x, y, color="#ffffff"):
    """Two sparkles, the bigger one at (x, y). White on light pages; pass the accent on others."""
    return sparkle(x, y, 18, 1.0, color) + sparkle(x - 38, y + 48, 10, 0.85, color)


def folder_back(x, y, w, h):
    return (f'<rect x="{x + w * 0.54}" y="{y - 26}" width="{w * 0.39}" height="70" rx="18" fill="url(#folderBack)"/>'
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="26" fill="url(#folderBack)"/>')


def folder_front(x, y, w, h):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="26" fill="url(#folderFront)"/>'
            f'<rect x="{x}" y="{y}" width="{w}" height="16" rx="8" fill="#ffffff" opacity="0.25"/>')


def folder(x, y, w, h, rot=0):
    c = (x + w / 2, y + h / 2)
    return f'<g transform="rotate({rot} {c[0]} {c[1]})">{folder_back(x, y, w, h)}{folder_front(x, y + 30, w, h - 30)}</g>'


def window(x, y, w, h, fill="url(#dark)", bar="#3f2734", content=""):
    """A window with a title bar; `content` is drawn in its own coordinates (0,0 = below the bar)."""
    return f"""<rect x="{x + 6}" y="{y + 14}" width="{w}" height="{h}" rx="16" fill="#3d1830" opacity="0.3" filter="url(#softer)"/>
<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="16" fill="{fill}" stroke="#6a4a5c" stroke-width="2"/>
<path d="M{x} {y + 34} V{y + 16} Q{x} {y} {x + 16} {y} H{x + w - 16} Q{x + w} {y} {x + w} {y + 16} V{y + 34} Z" fill="{bar}"/>
<circle cx="{x + 20}" cy="{y + 17}" r="5.5" fill="{PINK_LIGHT}"/><circle cx="{x + 38}" cy="{y + 17}" r="5.5" fill="{AMBER}"/><circle cx="{x + 56}" cy="{y + 17}" r="5.5" fill="{TEAL}"/>
<g transform="translate({x} {y + 34})">{content}</g>"""


def pill(x, y, w, color, h=10, op=1.0):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{h / 2}" fill="{color}" opacity="{op}"/>'


def paper(x, y, w, h, rot=0, lines=True, margin=True):
    out = f'<g transform="rotate({rot} {x + w / 2} {y + h / 2})">'
    out += f'<rect x="{x + 4}" y="{y + 10}" width="{w}" height="{h}" rx="10" fill="#3d1830" opacity="0.22" filter="url(#soft)"/>'
    out += f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="url(#paper)" stroke="#e2cfdb" stroke-width="2"/>'
    if lines:
        for ly in range(int(y + 46), int(y + h - 16), 24):
            out += f'<line x1="{x + 14}" y1="{ly}" x2="{x + w - 14}" y2="{ly}" stroke="#f2bfd8" stroke-width="2"/>'
    if margin:
        out += f'<line x1="{x + 44}" y1="{y + 10}" x2="{x + 44}" y2="{y + h - 10}" stroke="{PINK_LIGHT}" stroke-width="2.5" opacity="0.7"/>'
    return out + "</g>"


def pencil(x, y, length=170, rot=-35):
    return f"""<g transform="translate({x} {y}) rotate({rot})">
<rect x="0" y="-11" width="{length}" height="22" rx="3" fill="{AMBER}"/>
<rect x="0" y="-11" width="{length}" height="7" fill="#ffffff" opacity="0.35"/>
<rect x="{length - 24}" y="-11" width="24" height="22" rx="5" fill="{PINK_LIGHT}"/>
<rect x="{length - 32}" y="-11" width="10" height="22" fill="#c9c3d1"/>
<path d="M0 -11 L-30 0 L0 11 Z" fill="#f3d3a8"/><path d="M-20 -4 L-30 0 L-20 4 Z" fill="{INK}"/>
</g>"""


# ---------------------------------------------------------------- scenes

def scene_hello():
    """Home page: the mascot as he first appeared, leaning on Rivet's folder."""
    return (folder(290, 150, 320, 250, -9)
            + shadow(450, 330, 90, 20, -10, 0.32)
            + rivet(230, 250, 0.9)
            + glints(140, 90, PINK_LIGHT))


def scene_files():
    """Rivet Files: popping up out of an open folder, among its papers."""
    x, y, w, h = 130, 170, 380, 250
    return (shadow(320, 440, 200, 16, 0, 0.25)
            + folder_back(x, y, w, h)
            + paper(170, 140, 120, 150, -10, lines=True, margin=False)
            + paper(368, 132, 112, 150, 9, lines=True, margin=False)
            + rivet(320, 236, 0.78, -3, look=(4, 4), brows="raised", mood="grin")
            + f'<g transform="skewX(-4) translate(22 0)">{folder_front(x - 10, y + 110, w + 4, h - 110)}</g>'
            + glints(560, 90, PINK_LIGHT))


def scene_shell():
    """Rivet Shell: beside a desktop with its panel and dock."""
    sx, sy, sw, sh = 50, 50, 420, 280
    dock = "".join(f'<rect x="{sx + 140 + i * 38}" y="{sy + sh - 44}" width="28" height="28" rx="8" fill="{c}"/>'
                   for i, c in enumerate([PINK_LIGHT, TEAL, AMBER, PURPLE]))
    return (f'<rect x="{sx + 8}" y="{sy + 18}" width="{sw}" height="{sh}" rx="18" fill="#3d1830" opacity="0.3" filter="url(#softer)"/>'
            f'<rect x="{sx - 10}" y="{sy - 10}" width="{sw + 20}" height="{sh + 20}" rx="22" fill="#2a1922" stroke="#6a4a5c" stroke-width="2"/>'
            f'<rect x="{sx}" y="{sy}" width="{sw}" height="{sh}" rx="12" fill="url(#screen)"/>'
            f'<rect x="{sx + 8}" y="{sy + 8}" width="{sw - 16}" height="22" rx="11" fill="#1d1018" opacity="0.75"/>'
            + pill(sx + 20, sy + 15, 34, "#ffffff", 8, 0.8) + pill(sx + sw - 70, sy + 15, 50, "#ffffff", 8, 0.8)
            + f'<rect x="{sx + 128}" y="{sy + sh - 52}" width="{4 * 38 + 14}" height="44" rx="14" fill="#1d1018" opacity="0.75"/>'
            + dock
            + f'<rect x="{sx + sw / 2 - 30}" y="{sy + sh + 10}" width="60" height="44" fill="#2a1922" stroke="#6a4a5c" stroke-width="2"/>'
            f'<rect x="{sx + sw / 2 - 90}" y="{sy + sh + 50}" width="180" height="14" rx="7" fill="#2a1922" stroke="#6a4a5c" stroke-width="2"/>'
            + stand(520, 330, 0.62, tilt=-6, look=(-14, 4), brows="knowing", mood="grin"))


def scene_edit():
    """Rivet Edit: studying the code in an editor window."""
    rows = [(0, [(PINK_LIGHT, 60), (TEAL, 90)]), (1, [(AMBER, 70), ("#d6c9d2", 110)]), (1, [(PURPLE, 50), ("#d6c9d2", 80), (TEAL, 40)]),
            (2, [("#d6c9d2", 120)]), (1, [(PINK_LIGHT, 40), (AMBER, 60)]), (0, [("#d6c9d2", 30)]), (0, []), (0, [(TEAL, 70), ("#d6c9d2", 100)])]
    code = ""
    for i, (indent, parts) in enumerate(rows):
        yy = 24 + i * 28
        code += f'<text x="18" y="{yy + 9}" font-family="monospace" font-size="12" fill="#8c7084">{i + 1}</text>'
        xx = 52 + indent * 24
        for color, w in parts:
            code += pill(xx, yy, w, color)
            xx += w + 10
    code += f'<rect x="{52 + 24 + 50 + 10 + 80 + 10 + 40 + 6}" y="{24 + 2 * 28 - 4}" width="3" height="18" fill="#ffffff"/>'
    return (window(40, 70, 380, 300, content=code)
            + stand(500, 320, 0.7, tilt=-6, look=(-14, 6), brows="focused", mood="flat"))


def scene_terminal():
    """Rivet Terminal: at the prompt."""
    mono = 'font-family="DejaVu Sans Mono, monospace" font-size="17"'
    content = (f'<text x="20" y="40" {mono} fill="{TEAL}">~</text><text x="38" y="40" {mono} fill="#e8dde4">$ cargo build</text>'
               f'<text x="20" y="70" {mono} fill="#9d8a96">  Compiling rivet</text>'
               f'<text x="20" y="100" {mono} fill="{AMBER}">  Finished</text><text x="120" y="100" {mono} fill="#9d8a96">in 4.2 s</text>'
               f'<text x="20" y="140" {mono} fill="{TEAL}">~</text><text x="38" y="140" {mono} fill="#e8dde4">$</text>'
               f'<rect x="58" y="126" width="11" height="19" fill="{PINK_LIGHT}"/>')
    return (window(220, 80, 380, 250, content=content)
            + stand(150, 320, 0.7, tilt=6, look=(14, 4), brows="knowing", mood="grin"))


def scene_notes():
    """Rivet Notes: leaning on a page of notes, pencil at the ready."""
    return (paper(270, 110, 290, 330, 8)
            + pencil(330, 400, 180, -40)
            + shadow(400, 340, 80, 18, -8, 0.3)
            + rivet(220, 250, 0.88, 7, look=(12, 6), brows="raised", mood="open")
            + glints(110, 80, PINK_LIGHT))


def scene_ftp():
    """Rivet FTP: a file on its way from this computer to a server."""
    laptop = (f'<rect x="40" y="250" width="170" height="112" rx="10" fill="#2a1922" stroke="#6a4a5c" stroke-width="2"/>'
              f'<rect x="50" y="260" width="150" height="92" rx="4" fill="url(#screen)"/>'
              f'<path d="M20 362 H230 L218 378 H32 Z" fill="url(#metal)" stroke="#a39bb3" stroke-width="2"/>')
    server = (f'<rect x="452" y="180" width="140" height="198" rx="14" fill="url(#dark)" stroke="#6a4a5c" stroke-width="2"/>'
              + "".join(f'<rect x="466" y="{196 + i * 44}" width="112" height="32" rx="6" fill="#3f2734"/>'
                        f'<circle cx="482" cy="{212 + i * 44}" r="4.5" fill="{TEAL if i != 1 else AMBER}"/>'
                        + pill(496, 208 + i * 44, 60, "#5c4251", 8) for i in range(4)))
    arc = (f'<path d="M140 236 Q320 40 520 160" fill="none" stroke="{PINK_LIGHT}" stroke-width="6" stroke-linecap="round" stroke-dasharray="2 16"/>'
           f'<path d="M500 140 L526 164 L492 172" fill="none" stroke="{PINK_LIGHT}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>')
    sheet = (f'<g transform="rotate(14 330 108)"><rect x="306" y="78" width="48" height="62" rx="6" fill="url(#paper)" stroke="#e2cfdb" stroke-width="2"/>'
             + pill(316, 96, 28, "#f2bfd8", 5) + pill(316, 108, 20, "#f2bfd8", 5) + pill(316, 120, 26, "#f2bfd8", 5) + '</g>')
    return (shadow(125, 388, 110, 10, 0, 0.25) + shadow(522, 388, 80, 10, 0, 0.25) + laptop + server + arc + sheet
            + stand(320, 330, 0.6, tilt=0, look=(12, -10), brows="raised", mood="open"))


def scene_welcome():
    """Rivet Welcome: popping out of a parcel, freshly installed."""
    return (shadow(320, 440, 170, 16, 0, 0.28)
            + f'<path d="M180 250 L120 196 L262 170 L320 220 Z" fill="url(#boxFlap)"/>'
            f'<path d="M460 250 L520 196 L378 170 L320 220 Z" fill="url(#boxFlap)"/>'
            f'<path d="M180 250 L320 220 L460 250 L320 280 Z" fill="#9a2f66"/>'
            + rivet(320, 196, 0.74, 4, look=(6, -4), brows="raised", mood="open")
            + f'<path d="M180 250 L320 280 L320 440 L180 410 Z" fill="url(#boxFront)"/>'
            f'<path d="M460 250 L320 280 L320 440 L460 410 Z" fill="#b8346f"/>'
            f'<path d="M180 250 L320 280 L460 250" fill="none" stroke="#ffffff" stroke-width="3" opacity="0.4"/>'
            f'<path d="M226 320 L226 356 M210 342 L226 358 L242 342" fill="none" stroke="#ffffff" stroke-width="7" stroke-linecap="round" stroke-linejoin="round" transform="translate(24 6)"/>'
            + glints(560, 110, PINK_LIGHT) + sparkle(96, 150, 12, 0.9, PINK_LIGHT))


def scene_plugins():
    """Plugins: fitting a new piece into the panel."""
    bar = (f'<rect x="56" y="350" width="528" height="56" rx="18" fill="#2a1922" stroke="#6a4a5c" stroke-width="2"/>'
           + "".join(f'<rect x="{78 + i * 60}" y="364" width="44" height="28" rx="9" fill="{c}"/>' for i, c in enumerate([TEAL, AMBER, PURPLE]))
           + f'<rect x="258" y="364" width="44" height="28" rx="9" fill="none" stroke="{PINK_LIGHT}" stroke-width="3" stroke-dasharray="6 5"/>'
           + pill(470, 373, 90, "#5c4251", 10))
    piece = (f'<g transform="translate(250 196) rotate(-12)">'
             f'<path d="M0 0 H22 a10 10 0 1 1 20 0 H64 V22 a10 10 0 1 1 0 20 V64 H42 a10 10 0 1 0 -20 0 H0 V42 a10 10 0 1 0 0 -20 Z" fill="{PINK_LIGHT}" stroke="#b8346f" stroke-width="2"/>'
             f'<rect x="6" y="6" width="12" height="6" rx="3" fill="#ffffff" opacity="0.5"/></g>'
             f'<path d="M282 300 V334 M270 322 L282 336 L294 322" fill="none" stroke="{PINK_LIGHT}" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" opacity="0.7"/>')
    return (shadow(320, 420, 270, 14, 0, 0.25) + bar + piece
            + stand(470, 220, 0.6, tilt=-6, look=(-14, -2), brows="knowing", mood="grin"))


def scene_lost():
    """Not found: puzzled over an empty folder."""
    q = (f'<text x="470" y="150" font-family="DejaVu Sans, sans-serif" font-weight="bold" font-size="120" fill="{PINK_LIGHT}" transform="rotate(12 500 110)">?</text>')
    return (shadow(320, 440, 220, 14, 0, 0.25)
            + folder(330, 300, 250, 130, 0)
            + rivet(220, 300, 0.74, 9, look=(14, 12), brows="puzzled", mood="o", blush=False)
            + q
            + "".join(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="#b8487f" opacity="0.35"/>' for cx, cy, r in [(430, 270, 4), (470, 250, 3), (520, 272, 5)]))


def scene_docs():
    """Docs: reading up, glasses on."""
    book = (f'<path d="M320 330 Q250 300 160 318 L160 440 Q250 420 320 446 Z" fill="url(#paper)" stroke="#e2cfdb" stroke-width="2"/>'
            f'<path d="M320 330 Q390 300 480 318 L480 440 Q390 420 320 446 Z" fill="url(#paper)" stroke="#e2cfdb" stroke-width="2"/>'
            f'<path d="M156 324 L156 446 Q250 426 320 452 Q390 426 484 446 L484 324" fill="none" stroke="{PINK}" stroke-width="8" stroke-linejoin="round"/>'
            + "".join(f'<path d="M180 {346 + i * 18} Q240 {334 + i * 18} 300 {350 + i * 18}" fill="none" stroke="#e7c3d6" stroke-width="4" stroke-linecap="round"/>'
                      f'<path d="M340 {350 + i * 18} Q400 {334 + i * 18} 460 {346 + i * 18}" fill="none" stroke="#e7c3d6" stroke-width="4" stroke-linecap="round"/>' for i in range(4))
            + f'<line x1="320" y1="332" x2="320" y2="448" stroke="#d9c3d0" stroke-width="2"/>')
    return (shadow(320, 452, 190, 14, 0, 0.28)
            + rivet(320, 240, 0.78, -2, look=(-2, 16), brows="focused", mood="flat", glasses=True)
            + book)


def scene_blog():
    """Blog: with something to tell."""
    bubble = (f'<path d="M330 60 H570 Q600 60 600 90 V200 Q600 230 570 230 H420 L380 272 L388 230 H330 Q300 230 300 200 V90 Q300 60 330 60 Z" fill="url(#paper)" stroke="#e2cfdb" stroke-width="2"/>'
              + pill(330, 96, 170, PINK_LIGHT, 14) + pill(330, 130, 230, "#e7c3d6", 10) + pill(330, 154, 200, "#e7c3d6", 10) + pill(330, 178, 120, "#e7c3d6", 10))
    return (f'<path d="M336 76 H576 Q606 76 606 106 V216 Q606 246 576 246 H336 Q306 246 306 216 V106 Q306 76 336 76 Z" fill="#3d1830" opacity="0.2" filter="url(#soft)"/>'
            + bubble
            + stand(210, 320, 0.72, tilt=6, look=(14, -6), brows="raised", mood="open"))


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
}


def main():
    OUT.mkdir(exist_ok=True)
    magick = shutil.which("magick")
    for name, draw in SCENES.items():
        title = f"Rivet the mascot: {draw.__doc__.strip()}"
        svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 480" width="640" height="480">\n'
               f'<title>{title}</title>\n<defs>{DEFS}</defs>\n{draw()}\n</svg>\n')
        path = OUT / f"{name}.svg"
        path.write_text(svg)
        if magick:
            subprocess.run([magick, "-background", "none", "-density", "192", str(path), str(OUT / f"{name}.png")], check=True)
    print(f"Wrote {len(SCENES)} scenes to {OUT}")


if __name__ == "__main__":
    main()
