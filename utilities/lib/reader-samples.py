#!/usr/bin/env python3
"""Sample documents for Rivet Reader' screenshots and tests, all made here: nothing copied, nothing
personal. Writes, into the folder given:

  Rivet Field Guide.pdf   a dozen pages about the Rivet desktop, with an outline, a linked table
                          of contents, a link to the website, a diagram, a chart and a picture
  Rivet at the Market.cbz six comic pages of Rivet the mascot

The PDF is written by hand (it is mostly text) with the standard fonts every PDF reader has, so
no font files are needed. The pictures need Pillow.

    python3 utilities/lib/reader-samples.py OUT_DIR
"""

import io
import pathlib
import re
import sys
import zipfile
import zlib

from PIL import Image, ImageDraw

HERE = pathlib.Path(__file__).resolve().parent
MASCOT = HERE.parent.parent / "docs" / "mascot" / "pixel" / "rivet-pixel.svg"

PINK = (0xD5, 0x3F, 0x8C)
BLUE = (0x00, 0x84, 0xB8)
YELLOW = (0xFF, 0xF2, 0x70)
INK = (0x24, 0x12, 0x1B)

W, H = 612, 792  # US Letter, in points
LEFT, RIGHT, TOP = 72, 540, 720


def mascot(scale):
    """Rivet in pixels, from the website's pixel art (the eyes open)."""
    svg = MASCOT.read_text()
    svg = svg.split("<g", 1)[0]  # the closed eyes are in a hidden layer after the drawing
    m = re.search(r'viewBox="0 0 (\d+) (\d+)"', svg)
    w, h = int(m.group(1)), int(m.group(2))
    img = Image.new("RGBA", (w * scale, h * scale), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for x, y, rw, rh, fill in re.findall(r'<rect x="(\d+)" y="(\d+)" width="(\d+)" height="(\d+)" fill="(#[0-9a-f]{6})"', svg):
        x, y, rw, rh = int(x), int(y), int(rw), int(rh)
        d.rectangle([x * scale, y * scale, (x + rw) * scale - 1, (y + rh) * scale - 1], fill=fill)
    return img


def landscape(w, h):
    """A made-up view: sky, sun, hills and a river, in the Rivet colors."""
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / h
        d.line([(0, y), (w, y)], fill=(int(255 - 60 * t), int(214 - 90 * t), int(170 + 40 * t)))
    d.ellipse([w * 0.62, h * 0.14, w * 0.78, h * 0.14 + w * 0.16], fill=YELLOW)
    d.polygon([(0, h * 0.62), (w * 0.3, h * 0.4), (w * 0.55, h * 0.6), (w, h * 0.45), (w, h), (0, h)], fill=(0x8E, 0x2A, 0x62))
    d.polygon([(0, h * 0.78), (w * 0.4, h * 0.6), (w * 0.8, h * 0.75), (w, h * 0.7), (w, h), (0, h)], fill=PINK)
    d.polygon([(w * 0.35, h), (w * 0.5, h * 0.72), (w * 0.56, h * 0.72), (w * 0.62, h)], fill=BLUE)
    return img


# ---- a small PDF writer

class Pdf:
    def __init__(self):
        self.objects = []

    def add(self, body):
        self.objects.append(body)
        return len(self.objects)

    def reserve(self):
        return self.add(None)

    def set(self, n, body):
        self.objects[n - 1] = body

    def stream(self, data, extra=""):
        z = zlib.compress(data)
        return self.add(b"<< /Length %d /Filter /FlateDecode %s >>\nstream\n" % (len(z), extra.encode()) + z + b"\nendstream")

    def write(self, path, root, info):
        out = io.BytesIO()
        out.write(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = []
        for i, body in enumerate(self.objects, 1):
            offsets.append(out.tell())
            if isinstance(body, str):
                body = body.encode("latin-1")
            out.write(b"%d 0 obj\n" % i + body + b"\nendobj\n")
        xref = out.tell()
        out.write(b"xref\n0 %d\n0000000000 65535 f \n" % (len(self.objects) + 1))
        for o in offsets:
            out.write(b"%010d 00000 n \n" % o)
        out.write(b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(self.objects) + 1, root, info, xref))
        pathlib.Path(path).write_bytes(out.getvalue())


def esc(s):
    return s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)").replace("\u2019", "\\222").replace("\u2014", "\\227").replace("\u00b7", "\\267").replace("\u2026", "\\205")


# Rough Helvetica widths, in thousandths of the size: enough to wrap lines that never spill.
NARROW = set("iljtfI.,;:!|'()[] ")


def width(s, size):
    total = 0
    for c in s:
        if c in NARROW:
            total += 300
        elif c in "mwMW":
            total += 850
        elif c.isupper() or c in "0123456789":
            total += 660
        else:
            total += 540
    return total * size / 1000


def wrap(text, size, room):
    lines, line = [], ""
    for word in text.split():
        trial = (line + " " + word).strip()
        if width(trial, size) > room and line:
            lines.append(line)
            line = word
        else:
            line = trial
    if line:
        lines.append(line)
    return lines


class Page:
    def __init__(self, number):
        self.number = number
        self.ops = []
        self.y = TOP
        self.links = []  # (rect, target) with target ("page", index, y) or ("url", text)
        self.image = None

    def text(self, s, x, y, size, font="F1", color=INK):
        r, g, b = (c / 255 for c in color)
        self.ops.append(f"BT /{font} {size} Tf {r:.3f} {g:.3f} {b:.3f} rg {x:.1f} {y:.1f} Td ({esc(s)}) Tj ET")

    def rect(self, x, y, w, h, color, stroke=None):
        r, g, b = (c / 255 for c in color)
        self.ops.append(f"{r:.3f} {g:.3f} {b:.3f} rg {x:.1f} {y:.1f} {w:.1f} {h:.1f} re f")
        if stroke:
            r, g, b = (c / 255 for c in stroke)
            self.ops.append(f"{r:.3f} {g:.3f} {b:.3f} RG 1.2 w {x:.1f} {y:.1f} {w:.1f} {h:.1f} re S")

    def line(self, x0, y0, x1, y1, color=INK, w=1.2):
        r, g, b = (c / 255 for c in color)
        self.ops.append(f"{r:.3f} {g:.3f} {b:.3f} RG {w} w {x0:.1f} {y0:.1f} m {x1:.1f} {y1:.1f} l S")

    def heading(self, s, size=22):
        self.y -= size + 6
        self.text(s, LEFT, self.y, size, "F2")
        self.y -= 10
        return self.y + size + 16

    def sub(self, s):
        self.y -= 26
        self.text(s, LEFT, self.y, 14, "F2", BLUE)
        self.y -= 4
        return self.y + 30

    def para(self, s, size=11, font="F1", indent=0):
        for ln in wrap(s, size, RIGHT - LEFT - indent):
            self.y -= size + 4.5
            self.text(ln, LEFT + indent, self.y, size, font)
        self.y -= 8

    def bullet(self, s):
        self.y -= 0
        first = True
        for ln in wrap(s, 11, RIGHT - LEFT - 18):
            self.y -= 15.5
            if first:
                self.rect(LEFT + 4, self.y + 3, 4, 4, PINK)
                first = False
            self.text(ln, LEFT + 18, self.y, 11)
        self.y -= 4

    def footer(self, title):
        self.line(LEFT, 54, RIGHT, 54, (0xD8, 0xCC, 0xD2), 0.8)
        self.text(title, LEFT, 40, 9, "F3", (0x80, 0x70, 0x78))
        n = str(self.number)
        self.text(n, RIGHT - width(n, 9), 40, 9, "F1", (0x80, 0x70, 0x78))


SECTIONS = []  # (title, page index, y, children)


def build(out):
    pages = []

    def new_page():
        p = Page(len(pages) + 1)
        pages.append(p)
        return p

    def section(p, title, children=()):
        y = p.heading(title)
        SECTIONS.append((title, len(pages) - 1, y, list(children)))

    # 1. Cover
    p = new_page()
    p.rect(0, 0, W, H, (0xFB, 0xF4, 0xF7))
    p.rect(0, 470, W, 150, PINK)
    p.text("Rivet Field Guide", LEFT, 545, 40, "F2", (255, 255, 255))
    p.text("A short tour of the Rivet desktop", LEFT, 505, 16, "F1", (255, 255, 255))
    p.image = ("Mascot", 360, 120, 170, 195)
    p.text("Edition 1 \u00b7 November 2026", LEFT, 150, 11, "F1", INK)
    p.text("Sample document made for Rivet Reader\u2019 screenshots.", LEFT, 132, 9, "F3", (0x80, 0x70, 0x78))

    # 2. Contents (filled in once the sections are known)
    contents = new_page()

    # 3. The panel
    p = new_page()
    section(p, "1  The panel", ["Items", "Groups"])
    p.para("The panel is the bar along the edge of the screen. It holds items: the clock, the workspaces, the tray, the battery and anything a plugin adds. Each item can sit in the left, middle or right zone of the panel, and a panel can run along any edge of any screen.")
    p.para("A second panel is one setting away. Many people keep a thin panel at the top for the clock and the tray, and a dock at the bottom for the apps they use most.")
    sub_items = p.sub("Items")
    p.para("Click an item to open its popup: the calendar under the clock, the volume under the speaker, the networks under the signal bars. Right-click the panel itself to add, move or remove items.")
    p.bullet("Workspaces show where your windows are, with one dot for each window.")
    p.bullet("The tray holds the icons apps put there, such as Rivet Welcome\u2019s.")
    p.bullet("Plugins add their own items, like the notes plugin\u2019s quick note button.")
    sub_groups = p.sub("Groups")
    p.para("Items can be gathered into a group, which shows as one item on the panel and opens the rest in a small sub-panel. Groups keep a busy panel tidy without hiding anything for good.")
    SECTIONS[-1] = (SECTIONS[-1][0], SECTIONS[-1][1], SECTIONS[-1][2], [("Items", 2, sub_items), ("Groups", 2, sub_groups)])

    # 4. Dock and launcher, with the diagram
    p = new_page()
    section(p, "2  The dock and the launcher")
    p.para("The dock keeps the apps you pin, and shows the ones that are open with a dot under them. Drag an app onto the dock to pin it; drag it off to let it go.")
    p.para("The launcher opens with the Super key. Type a few letters of an app, a file or a command and press Enter. Commands start with an at sign: @notes searches your notes and @ftp opens a saved connection.")
    p.sub("How the pieces talk")
    p.para("Apps join the shell through plugins. The shell runs each plugin and shows what it says on the panel and in the launcher; when you choose something, the app opens its own window.")
    # The diagram: three boxes and the arrows between them.
    top = p.y - 30
    boxes = [("Rivet Shell", LEFT + 10, PINK), ("Plugin", LEFT + 175, BLUE), ("App window", LEFT + 340, (0x8E, 0x2A, 0x62))]
    for label, x, col in boxes:
        p.rect(x, top - 60, 130, 60, col, INK)
        p.text(label, x + 65 - width(label, 12) / 2, top - 35, 12, "F2", (255, 255, 255))
    for x in (LEFT + 140, LEFT + 305):
        p.line(x + 4, top - 30, x + 31, top - 30, INK, 2)
        p.ops.append(f"0.141 0.071 0.106 rg {x + 31:.1f} {top - 30:.1f} m {x + 23:.1f} {top - 25:.1f} l {x + 23:.1f} {top - 35:.1f} l f")
    p.text("Panel items and launcher entries go one way; what you chose comes back.", LEFT + 10, top - 85, 9.5, "F3", (0x60, 0x50, 0x58))
    p.y = top - 110
    p.para("Every piece also works alone. An app runs with no shell, and the shell runs with none of the apps: the feature that needs the missing piece is left out quietly.")

    # 5. Quick settings
    p = new_page()
    section(p, "3  Quick settings")
    p.para("Quick settings gather the switches you reach for most: Wi-Fi, Bluetooth, Night Light, Do Not Disturb, the volume and the screen brightness. Open them from the panel, or with Super and S.")
    p.para("Each tile turns its setting on or off with a click. Click the arrow on a tile to see more, such as the list of networks or the speakers you can play to.")
    p.sub("Night Light")
    p.para("Night Light warms the screen in the evening. Install gammastep, hyprsunset or wlsunset to use it; the tile stays hidden until one of them is there.")
    p.sub("Do Not Disturb")
    p.para("While Do Not Disturb is on, notifications wait in the notification list instead of showing on screen. Urgent ones, such as a low battery, still show.")

    # 6. Files
    p = new_page()
    section(p, "4  Files")
    p.para("Rivet Files shows your folders as a list, as icons or as columns. Press Space to peek at a file without opening it: pictures, videos, text and code all show in place.")
    p.para("Rename many files at once with a pattern, filter a folder down to what you need, and keep the places you use in the sidebar. Rivet Files can also be the Open and Save window for every app on the desktop.")
    p.sub("Keys worth knowing")
    rows = [("Ctrl+P", "Quick Open: any file by a few letters of its name"), ("Space", "Peek at the selected file"), ("F2", "Rename"), ("Ctrl+H", "Show hidden files"), ("Ctrl+L", "Type a location")]
    y = p.y - 24
    for i, (k, v) in enumerate(rows):
        if i % 2 == 0:
            p.rect(LEFT, y - 7, RIGHT - LEFT, 22, (0xF3, 0xEA, 0xEE))
        p.text(k, LEFT + 10, y, 11, "F4")
        p.text(v, LEFT + 120, y, 11)
        y -= 22
    p.y = y - 10

    # 7. Notes
    p = new_page()
    section(p, "5  Notes")
    p.para("Rivet Notes keeps notes as plain Markdown files in a folder you choose, called a vault. Nothing is locked away: any editor can open them, and any sync tool can carry them between computers.")
    p.para("Tasks written as check boxes gather on a board, by the day they are due. Tags gather notes about the same thing, and the notes plugin puts a quick note button on the panel.")
    p.sub("A note looks like this")
    for ln in ["---", "tags: [garden, weekend]", "---", "# Seeds to sow", "- [ ] Beans, after the last frost", "- [x] Peas, in early spring"]:
        p.y -= 15
        p.text(ln, LEFT + 14, p.y, 10.5, "F4", (0x40, 0x30, 0x38))
    p.y -= 14
    p.para("Open the vault in Rivet Files and every note is an ordinary file, with its own name and its own folder.")

    # 8. Colors and wallpaper, with the picture
    p = new_page()
    section(p, "6  Colors and wallpaper")
    p.para("Choose a wallpaper and Rivet can make the desktop\u2019s colors from it: the panel, the apps and their accents all follow, with no restart. Turn it off to keep one accent of your own.")
    p.image = ("View", LEFT, p.y - 250, RIGHT - LEFT, 230)
    p.y -= 262
    p.text("A wallpaper picture. In dark pages, pictures keep their own colors.", LEFT, p.y, 9.5, "F3", (0x60, 0x50, 0x58))
    p.y -= 20
    p.para("Every app reads the same colors, so a new wallpaper recolors the open windows as well as the panel.")

    # 9. A chart
    p = new_page()
    section(p, "7  Reading a chart")
    p.para("Charts in a PDF are drawn with lines and shapes, so they stay sharp at any zoom. This one is an example with made-up numbers: notes written on each day of a week.")
    base_y = p.y - 230
    days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    vals = [6, 9, 4, 11, 8, 3, 5]
    p.line(LEFT + 30, base_y, RIGHT - 10, base_y, INK, 1)
    for i, (dname, v) in enumerate(zip(days, vals)):
        x = LEFT + 50 + i * 60
        p.rect(x, base_y, 36, v * 17, [PINK, BLUE][i % 2])
        p.text(dname, x + 18 - width(dname, 10) / 2, base_y - 16, 10)
        p.text(str(v), x + 18 - width(str(v), 10) / 2, base_y + v * 17 + 6, 10, "F2")
    p.y = base_y - 40
    p.para("Zoom in with Ctrl and the wheel, or pinch: the bars and the numbers stay crisp.")

    # 10. Where things are kept
    p = new_page()
    section(p, "8  Where things are kept")
    p.para("Rivet keeps its settings as short text files you can read and edit. Each app has its own folder, and an app never changes another app\u2019s settings.")
    for path, what in [("~/.config/rivet-shell/settings", "the panel, the dock and the launcher"), ("~/.config/rivet-notes/settings", "Rivet Notes"), ("~/.config/rivet-reader/settings", "Rivet Reader"), ("~/.local/state/rivet-reader/places", "the page each document was left at")]:
        p.y -= 18
        p.text(path, LEFT + 10, p.y, 10, "F4", BLUE)
        p.y -= 14
        p.text(what, LEFT + 24, p.y, 10)
    p.y -= 12
    p.para("To start again from the defaults, move a settings file away while the app is closed.")

    # 11. More
    p = new_page()
    section(p, "9  Find out more")
    p.para("The Rivet website has every app, how to install it, and the plugins other people have made for the panel.")
    p.y -= 6
    url = "https://rivetsoft.github.io/"
    p.text(url, LEFT, p.y, 12, "F1", BLUE)
    p.line(LEFT, p.y - 2, LEFT + width(url, 12), p.y - 2, BLUE, 0.8)
    p.links.append(((LEFT, p.y - 4, LEFT + width(url, 12), p.y + 12), ("url", url)))
    p.y -= 24
    p.para("Rivet Welcome installs the apps and keeps them up to date. It sits in the tray and tells you when something new is out.")
    p.para("Thanks for reading. Press Home to go back to the cover.")

    # The contents page, with links.
    y = contents.heading("Contents")
    contents.y -= 8
    for title, index, ty, children in SECTIONS:
        contents.y -= 24
        contents.text(title, LEFT, contents.y, 13)
        n = str(index + 1)
        contents.text(n, RIGHT - width(n, 13), contents.y, 13)
        contents.links.append(((LEFT, contents.y - 5, RIGHT, contents.y + 15), ("page", index, ty)))
        for ctitle, cindex, cy in children:
            contents.y -= 20
            contents.text(ctitle, LEFT + 22, contents.y, 11, "F1", (0x60, 0x50, 0x58))
            contents.links.append(((LEFT + 22, contents.y - 4, RIGHT, contents.y + 13), ("page", cindex, cy)))

    for p in pages[1:]:
        p.footer("Rivet Field Guide")

    # ---- the objects
    pdf = Pdf()
    catalog = pdf.reserve()
    pages_obj = pdf.reserve()
    outlines = pdf.reserve()
    fonts = {
        "F1": pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"),
        "F2": pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"),
        "F3": pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique /Encoding /WinAnsiEncoding >>"),
        "F4": pdf.add("<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>"),
    }
    font_dict = " ".join(f"/{k} {v} 0 R" for k, v in fonts.items())

    def picture(img):
        rgba = img.convert("RGBA")
        rgb = Image.new("RGB", rgba.size, (0xFB, 0xF4, 0xF7))
        rgb.paste(rgba, mask=rgba.split()[3])
        return pdf.stream(rgb.tobytes(), f"/Type /XObject /Subtype /Image /Width {rgb.width} /Height {rgb.height} /ColorSpace /DeviceRGB /BitsPerComponent 8")

    images = {"Mascot": picture(mascot(10)), "View": picture(landscape(720, 330))}
    page_ids = [pdf.reserve() for _ in pages]
    for p, pid in zip(pages, page_ids):
        ops = list(p.ops)
        xobj = ""
        if p.image:
            name, x, y, w, h = p.image
            ops.append(f"q {w} 0 0 {h} {x} {y} cm /{name} Do Q")
            xobj = f"/XObject << /{name} {images[name]} 0 R >>"
        content = pdf.stream("\n".join(ops).encode("latin-1"))
        annots = []
        for (x0, y0, x1, y1), target in p.links:
            if target[0] == "url":
                annots.append(pdf.add(f"<< /Type /Annot /Subtype /Link /Rect [{x0:.1f} {y0:.1f} {x1:.1f} {y1:.1f}] /Border [0 0 0] /A << /S /URI /URI ({target[1]}) >> >>"))
            else:
                annots.append(pdf.add(f"<< /Type /Annot /Subtype /Link /Rect [{x0:.1f} {y0:.1f} {x1:.1f} {y1:.1f}] /Border [0 0 0] /Dest [{page_ids[target[1]]} 0 R /XYZ null {target[2]:.1f} null] >>"))
        annot = f"/Annots [{' '.join(f'{a} 0 R' for a in annots)}]" if annots else ""
        pdf.set(pid, f"<< /Type /Page /Parent {pages_obj} 0 R /MediaBox [0 0 {W} {H}] /Resources << /Font << {font_dict} >> {xobj} >> /Contents {content} 0 R {annot} >>")
    pdf.set(pages_obj, f"<< /Type /Pages /Kids [{' '.join(f'{i} 0 R' for i in page_ids)}] /Count {len(page_ids)} >>")

    # The outline: a top entry per section, with its parts under it.
    def outline_items(items, parent):
        ids = [pdf.reserve() for _ in items]
        for i, (item, oid) in enumerate(zip(items, ids)):
            title, index, y = item[0], item[1], item[2]
            children = item[3] if len(item) > 3 else []
            kids = outline_items(children, oid) if children else []
            links = f"/Parent {parent} 0 R"
            if i > 0:
                links += f" /Prev {ids[i - 1]} 0 R"
            if i + 1 < len(ids):
                links += f" /Next {ids[i + 1]} 0 R"
            if kids:
                links += f" /First {kids[0]} 0 R /Last {kids[-1]} 0 R /Count {len(kids)}"
            pdf.set(oid, f"<< /Title ({esc(title)}) {links} /Dest [{page_ids[index]} 0 R /XYZ null {y:.1f} null] >>")
        return ids

    top = [("Cover", 0, H)] + [("Contents", 1, H)] + SECTIONS
    ids = outline_items(top, outlines)
    pdf.set(outlines, f"<< /Type /Outlines /First {ids[0]} 0 R /Last {ids[-1]} 0 R /Count {len(ids)} >>")
    pdf.set(catalog, f"<< /Type /Catalog /Pages {pages_obj} 0 R /Outlines {outlines} 0 R /PageMode /UseOutlines >>")
    info = pdf.add("<< /Title (Rivet Field Guide) /Author (Rivetsoft) /Subject (A short tour of the Rivet desktop) /Producer (reader-samples.py) >>")
    pdf.write(out / "Rivet Field Guide.pdf", catalog, info)


def comic(out):
    """Six pages: Rivet walks to the market, buys a tomato and goes home."""
    m = mascot(9)
    pages = []
    scenes = [
        ((0xFF, 0xE9, 0xA8), "sun"), ((0xC9, 0xE8, 0xF5), "road"), ((0xF8, 0xD5, 0xE6), "stall"),
        ((0xFF, 0xF2, 0x70), "tomato"), ((0xC9, 0xE8, 0xF5), "road"), ((0x2B, 0x1B, 0x3A), "night"),
    ]
    for n, (sky, what) in enumerate(scenes):
        img = Image.new("RGB", (900, 1350), (0xFB, 0xF4, 0xF7))
        d = ImageDraw.Draw(img)
        # Two panels a page: a wide one on top, a tall one under it.
        for (x0, y0, x1, y1) in [(40, 40, 860, 560), (40, 600, 860, 1310)]:
            d.rectangle([x0, y0, x1, y1], fill=sky, outline=INK, width=8)
            d.rectangle([x0 + 4, y1 - (y1 - y0) // 4, x1 - 4, y1 - 4], fill=(0x8E, 0x2A, 0x62) if what != "night" else (0x1A, 0x10, 0x22))
        if what in ("sun", "road", "tomato"):
            d.ellipse([660, 90, 800, 230], fill=YELLOW, outline=INK, width=6)
        if what == "night":
            for x, y in [(120, 120), (300, 200), (520, 110), (700, 240), (780, 90)]:
                d.rectangle([x, y, x + 10, y + 10], fill=YELLOW)
            d.ellipse([640, 80, 760, 200], fill=(0xF7, 0xF2, 0xF4))
        if what == "stall":
            d.rectangle([450, 180, 820, 480], fill=PINK, outline=INK, width=6)
            for i in range(6):
                d.rectangle([450 + i * 62, 140, 450 + i * 62 + 31, 190], fill=(255, 255, 255) if i % 2 else BLUE, outline=INK, width=3)
        if what == "tomato":
            d.ellipse([360, 700, 560, 900], fill=(0xE0, 0x3A, 0x2E), outline=INK, width=8)
            d.rectangle([450, 670, 470, 715], fill=(0x2E, 0x8B, 0x57))
        # Rivet in each panel, walking from left to right through the book.
        x = 80 + n * 90
        img.paste(m, (x, 560 - m.height - 30), m)
        img.paste(m.resize((m.width * 3 // 2, m.height * 3 // 2), Image.NEAREST), (200 if what != "tomato" else 80, 1310 - m.height * 3 // 2 - 30), m.resize((m.width * 3 // 2, m.height * 3 // 2), Image.NEAREST))
        # A speech balloon with no words, and the page number.
        d.ellipse([560, 640, 840, 780], fill=(255, 255, 255), outline=INK, width=6)
        d.polygon([(600, 760), (560, 840), (650, 770)], fill=(255, 255, 255), outline=INK)
        for i in range(3):
            d.rectangle([610, 680 + i * 28, 790 - i * 40, 692 + i * 28], fill=(0xB0, 0x9A, 0xA2))
        buf = io.BytesIO()
        img.save(buf, "PNG", optimize=True)
        pages.append((f"Rivet at the Market/page {n + 1:02}.png", buf.getvalue()))
    with zipfile.ZipFile(out / "Rivet at the Market.cbz", "w", zipfile.ZIP_STORED) as z:
        for name, data in pages:
            z.writestr(name, data)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    build(out)
    comic(out)
    print(f"Wrote the samples in {out}")
