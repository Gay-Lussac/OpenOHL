#!/usr/bin/env python3
"""Write the sheet template guide's drawings (docs/images/sheet-templates/*.svg).

Standard library only; run from the repository root, after building `ohl`:

    cargo build -p ohl-cli
    python3 -I tools/sheet_template_figures.py        # writes docs/images/sheet-templates/
    python3 -I tools/sheet_template_figures.py OUT --theme light|dark [--fit 1200]
                                                      # copies with one palette forced and
                                                      # at most 1200 px wide, e.g. to look
                                                      # at them as PNG with `qlmanage -t`
    OHL=path/to/ohl python3 -I tools/sheet_template_figures.py   # another program

Two kinds of drawing:

- **Real sheets.** Each is drawn by `ohl drawing` itself, on the invented example
  line (examples/synthetic-line), with a small template written below, then cut to
  the part the guide talks about and marked up. They cannot disagree with the
  program: change the drawing code, run this again, and they follow.
- **Sketches** of what a sheet cannot show by itself (margins and paddings as
  dimensions, the title block's grid, how a plan set is written).

Colours around the real sheets are CSS custom properties switched by
`prefers-color-scheme` (the paper stays white), and nothing external is loaded, so
they read in the book's light and dark themes alike.
"""

import os
import re
import subprocess
import sys
import tempfile

LIGHT = {"ink": "#1f2328", "muted": "#59636e", "acc": "#0969da", "acc2": "#bc4c00",
         "ok": "#1a7f37", "bg": "#ffffff", "fill": "#eef1f4", "line": "#8c959f"}
DARK = {"ink": "#e6edf3", "muted": "#9198a1", "acc": "#58a6ff", "acc2": "#f0883e",
        "ok": "#3fb950", "bg": "#0d1117", "fill": "#1c2128", "line": "#6e7681"}

LINE = "examples/synthetic-line/synthetic-line-typed.ohl.json"
SURVEY = "examples/synthetic-line/synthetic-survey.csv"
RULES = "examples/synthetic-line/clearance-rules.ohlr"
CROSSING = "examples/synthetic-line/crossing-line.ohl.json"
LOGO = "examples/sheet-templates/example-logo.svg"
OHL = os.environ.get("OHL", "target/debug/ohl")
FIT = None


def palette(p):
    return "".join(f"--{k}:{v};" for k, v in p.items())


def style(theme):
    if theme == "light":
        colours = f"svg.f{{{palette(LIGHT)}}}"
    elif theme == "dark":
        colours = f"svg.f{{{palette(DARK)}}}"
    else:
        colours = (f"svg.f{{{palette(LIGHT)}}}"
                   f"@media (prefers-color-scheme: dark){{svg.f{{{palette(DARK)}}}}}")
    return (f"<style>{colours}"
            "svg.f text.t{font-family:system-ui,-apple-system,'Segoe UI',Helvetica,Arial,"
            "sans-serif;fill:var(--ink)}"
            "svg.f text.m{fill:var(--muted)}svg.f text.a{fill:var(--acc)}"
            "svg.f text.b{fill:var(--acc2)}svg.f text.o{fill:var(--ok)}"
            "svg.f text.c{font-family:ui-monospace,Menlo,Consolas,monospace}"
            "svg.f text.k{font-weight:600}"
            "svg.f .l{stroke:var(--ink);fill:none}svg.f .lm{stroke:var(--line);fill:none}"
            "svg.f .la{stroke:var(--acc);fill:none}svg.f .lb{stroke:var(--acc2);fill:none}"
            "svg.f .lo{stroke:var(--ok);fill:none}"
            "svg.f .fi{fill:var(--fill)}svg.f .fa{fill:var(--acc)}svg.f .fb{fill:var(--acc2)}"
            "svg.f .fo{fill:var(--ok)}svg.f .bg{fill:var(--bg)}svg.f .fk{fill:var(--ink)}"
            "</style>")


def esc(s):
    return str(s).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


class Fig:
    """A drawing on a page of `w` × `h` px."""

    def __init__(self, w, h):
        self.w, self.h = w, h
        self.items = [f'<rect class="bg" width="{w}" height="{h}"/>']

    def raw(self, s):
        self.items.append(s)

    def text(self, x, y, s, size=13, cls="", anchor="start"):
        self.items.append(f'<text class="t {cls}" x="{x:.1f}" y="{y:.1f}" font-size="{size}" '
                          f'text-anchor="{anchor}">{esc(s)}</text>')

    def line(self, x1, y1, x2, y2, cls="l", width=1.0, dash=None):
        d = f' stroke-dasharray="{dash}"' if dash else ""
        self.items.append(f'<line class="{cls}" x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" '
                          f'y2="{y2:.1f}" stroke-width="{width}"{d}/>')

    def rect(self, x, y, w, h, cls="l", width=1.0, dash=None, r=0):
        d = f' stroke-dasharray="{dash}"' if dash else ""
        rr = f' rx="{r}"' if r else ""
        self.items.append(f'<rect class="{cls}" x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" '
                          f'height="{h:.1f}" stroke-width="{width}"{d}{rr}/>')

    def fill(self, x, y, w, h, cls="fi", r=0):
        rr = f' rx="{r}"' if r else ""
        self.items.append(f'<rect class="{cls}" x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" '
                          f'height="{h:.1f}"{rr}/>')

    def dim(self, x1, y1, x2, y2, label, cls="la", tcls="a", side=-6, size=12, anchor="middle"):
        """A dimension line with ticks and its label."""
        self.line(x1, y1, x2, y2, cls)
        for (x, y) in ((x1, y1), (x2, y2)):
            if abs(x2 - x1) >= abs(y2 - y1):
                self.line(x, y - 4, x, y + 4, cls)
            else:
                self.line(x - 4, y, x + 4, y, cls)
        if abs(x2 - x1) >= abs(y2 - y1):
            self.text((x1 + x2) / 2, y1 + side, label, size, f"{tcls} c", anchor)
        else:
            self.text(x1 + 7, (y1 + y2) / 2 + 4, label, size, f"{tcls} c", "start")

    def mark(self, x, y, n, r=11):
        """A numbered circle."""
        self.items.append(f'<circle class="fa" cx="{x:.1f}" cy="{y:.1f}" r="{r}"/>')
        self.items.append(f'<text class="t" x="{x:.1f}" y="{y + 4.3:.1f}" font-size="12" '
                          f'font-weight="700" text-anchor="middle" style="fill:#fff">{n}</text>')

    def leader(self, x1, y1, x2, y2, cls="la"):
        self.line(x1, y1, x2, y2, cls, 1.2)
        self.items.append(f'<circle class="{ {"la": "fa", "lb": "fb", "lo": "fo"}[cls] }" '
                          f'cx="{x2:.1f}" cy="{y2:.1f}" r="2.6"/>')

    def paper(self, sheet, box, x, y, w, h=None, outline=True):
        """Part `box` (x, y, w, h in mm) of real sheet `sheet`, drawn at x, y, w px wide."""
        bx, by, bw, bh = box
        if h is None:
            h = w * bh / bw
        self.items.append(f'<svg x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" '
                          f'viewBox="{bx} {by} {bw} {bh}" font-family="Helvetica, Arial, '
                          f'sans-serif"><rect x="{bx}" y="{by}" width="{bw}" height="{bh}" '
                          f'fill="#fff"/>{cut(sheet, box)}</svg>')
        if outline:
            self.rect(x, y, w, h, "lm")
        return h

    def write(self, out, name, theme):
        w, h = self.w, self.h
        if FIT and w > FIT:
            w, h = FIT, int(self.h * FIT / self.w)
        svg = (f'<svg class="f" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {self.w} {self.h}" '
               f'width="{w}" height="{h}">{style(theme)}{"".join(self.items)}</svg>\n')
        with open(os.path.join(out, name + ".svg"), "w", encoding="utf-8") as f:
            f.write(svg)
        print(f"{name}.svg: {len(svg) // 1024} kB")


NUM = re.compile(r"-?\d+\.?\d*")


def extent(el):
    """The bounding box of one element of a sheet's SVG, or None if it is unknown."""
    if el.startswith("<path") or el.startswith("<polyline") or el.startswith("<polygon"):
        m = re.search(r'\b(?:d|points)="([^"]*)"', el)
        if not m:
            return None
        v = [float(n) for n in NUM.findall(m.group(1))]
        if len(v) < 2:
            return None
        return min(v[0::2]), min(v[1::2]), max(v[0::2]), max(v[1::2])
    if el.startswith("<line"):
        g = {k: float(re.search(rf'\b{k}="([^"]*)"', el).group(1))
             for k in ("x1", "y1", "x2", "y2")}
        return (min(g["x1"], g["x2"]), min(g["y1"], g["y2"]),
                max(g["x1"], g["x2"]), max(g["y1"], g["y2"]))
    if el.startswith("<text"):
        m = re.search(r'\bx="([^"]*)" y="([^"]*)"', el)
        if not m:
            return None
        x, y = float(m.group(1)), float(m.group(2))
        # Enough for any label; a text starting outside a cut but reaching in is kept.
        return x - 80, y - 8, x + 80, y + 8
    if el.startswith("<circle"):
        m = re.search(r'\bcx="([^"]*)" cy="([^"]*)" r="([^"]*)"', el)
        if not m:
            return None
        x, y, r = (float(m.group(k)) for k in (1, 2, 3))
        return x - r, y - r, x + r, y + r
    return None


def cut(sheet, box):
    """The elements of a sheet's SVG reaching into `box` (a much smaller file)."""
    x, y, w, h = box
    keep = []
    for el in sheet.splitlines():
        if el.startswith("<svg") or el.startswith("</svg") or el.startswith('<rect width="100%"'):
            continue
        if el.startswith("<g") or el.startswith("</g") or el.startswith("<defs"):
            keep.append(el)
            continue
        e = extent(el)
        if e is None or (e[2] >= x and e[0] <= x + w and e[3] >= y and e[1] <= y + h):
            keep.append(el)
    return "".join(keep)


# --- the real sheets ---------------------------------------------------------------------

HEAD = 'format = "ohl-sheet-template"\nversion = 1\nname = "figure"\n'

MARGINS = '''
[page]
margin = { left = "20 mm", top = "8 mm", right = "8 mm", bottom = "8 mm" }

[frame]
border = [{ inset = "0 mm", width = "0.6 mm" }, { inset = "1.5 mm", width = "0.2 mm" }]
padding = "4 mm"
'''

TITLE = '''
[title_block]
corner = "bottom_right"
width = "150 mm"
columns = ["28 mm", "*", "24 mm", "34 mm"]
rows = ["14 mm", "6 mm", "6 mm", "6 mm"]
line = "0.35 mm"
grid = true

[[title_block.cell]]
at = [0, 0]
image = "logo.svg"
[[title_block.cell]]
at = [0, 1]
span = [1, 3]
text = "{plan_kind}\\n{heading}"
size = "3 mm"
bold = true
[[title_block.cell]]
at = [1, 0]
text = "Route"
size = "2 mm"
[[title_block.cell]]
at = [1, 1]
text = "{route} — {route_name}"
size = "2.4 mm"
[[title_block.cell]]
at = [1, 2]
text = "Sheet"
size = "2 mm"
[[title_block.cell]]
at = [1, 3]
text = "{sheet} / {sheets}"
size = "2.6 mm"
bold = true
align = "right"
[[title_block.cell]]
at = [2, 0]
text = "Stations"
size = "2 mm"
[[title_block.cell]]
at = [2, 1]
text = "{from} – {to}{ends}"
size = "2.4 mm"
[[title_block.cell]]
at = [2, 2]
text = "Scales"
size = "2 mm"
[[title_block.cell]]
at = [2, 3]
text = "{scales}"
size = "2.4 mm"
align = "right"
[[title_block.cell]]
at = [3, 0]
text = "Drawing no."
size = "2 mm"
[[title_block.cell]]
at = [3, 1]
text = "{project_no}-{route}-{sheet:03}"
size = "2.4 mm"
[[title_block.cell]]
at = [3, 2]
text = "Date"
size = "2 mm"
[[title_block.cell]]
at = [3, 3]
text = "{date:%d.%m.%Y}"
size = "2.4 mm"
align = "right"

[[field]]
key = "project_no"
label = "Project number"
default = "EX-0042"

[[image]]
name = "logo.svg"
file = "example-logo.svg"
'''

BANDS = '''
[[band]]
kind = "structures"
height = "7 mm"
label = "Structure"
format = "{name}\\n{type}"
[[band]]
kind = "stations"
height = "6 mm"
label = "Station"
[[band]]
kind = "span_length"
height = "6 mm"
label = "Span (m)"
'''

LAYOUT = '''
[layout]
order = ["plan", "profile", "bands"]
gap = "6 mm"
axis_room = "22 mm"
plan_height = "45 mm"
'''

FURNITURE = '''
[north_arrow]
place = "plan"
size = "7 mm"

[scale_bar]
place = "bottom"
lengths = ["100 m", "10 m"]

[legend]
place = "above_title_block"
height = "26 mm"
column_width = "70 mm"
size = "2 mm"

[key_map]
place = "bottom"
width = "70 mm"
height = "38 mm"
structures = true
highlight = { width = "0.7 mm", colour = "#cc2222" }

[corridor]
width = "25 m"
label = "corridor ±{width} m"
pen = { width = "0.25 mm" }
near = { colour = "#2a7f3f", dash = "6 1.5 1 1.5" }
far = { colour = "#7a3fa0", dash = "2 1.5" }

[revisions]
place = "above_title_block"
rows = 2
row_height = "5 mm"
columns = [
  { title = "Rev", width = "12 mm", value = "{rev.index}" },
  { title = "Date", width = "24 mm", value = "{rev.date}" },
  { title = "Description", width = "90 mm", value = "{rev.text}" },
  { title = "By", width = "24 mm", value = "{rev.by}" },
]

[[note]]
at = ["3 mm", "-34 mm"]
width = "110 mm"
size = "2 mm"
text = "Invented line, drawn for this guide. Heights: {height_datum}."
'''

ANATOMY = HEAD + MARGINS + LAYOUT + BANDS + TITLE + FURNITURE + '''
[elevations]
place = "above"
offset = "6 mm"
rotation = 90
scale = 1000
labels = ["number", "height"]
size = "1.8 mm"
pen = { width = "0.15 mm", colour = "#444444" }
'''


class Renderer:
    """Runs `ohl drawing` and keeps its SVG sheets."""

    def __init__(self, work):
        self.work = work
        self.count = 0
        with open(LOGO, "rb") as src, open(os.path.join(work, "example-logo.svg"), "wb") as dst:
            dst.write(src.read())

    def run(self, template, args=(), page="A2"):
        self.count += 1
        tag = f"t{self.count}"
        out = os.path.join(self.work, tag)
        tpl = os.path.join(self.work, tag + ".ohlsheet")
        with open(tpl, "w", encoding="utf-8") as f:
            f.write(template)
        cmd = [OHL, "drawing", LINE, "--terrain", SURVEY, "--sheet-template", tpl,
               "--format", "svg", "--page", page, "--out", out, *args]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
        if r.returncode != 0:
            sys.exit(f"ohl drawing failed:\n{' '.join(cmd)}\n{r.stderr[-1500:]}")
        return out

    @staticmethod
    def sheet(folder, name):
        with open(os.path.join(folder, name + ".svg"), encoding="utf-8") as f:
            return f.read()


WEATHER = ["--weather", "examples/synthetic-line/weather-cases.ohlwc",
           "--case", "max temperature", "--case", "everyday"]
ONE_CASE = ["--weather", "examples/synthetic-line/weather-cases.ohlwc", "--case", "max temperature"]
SCALES = ["--hscale", "2500", "--vscale", "250"]
REVISIONS = ["--revision", "A;2026-10-20;first issue;AB",
             "--revision", "B;2026-11-02;structure T3 moved;CD"]


def plain(*tables):
    """A template with the title block and the tables given."""
    return HEAD + MARGINS + "".join(tables) + TITLE


def text_at(sheet, words):
    """Where the first text saying `words` starts on a sheet, mm."""
    m = re.search(rf'<text x="([-\d.]+)" y="([-\d.]+)"[^>]*>{re.escape(esc(words))}</text>', sheet)
    if not m:
        sys.exit(f"no text {words!r} on the sheet")
    return float(m.group(1)), float(m.group(2))


def caption(f, x, y, title, code=None):
    f.text(x, y, title, 14, "k")
    if code:
        f.text(x, y + 17, code, 12, "c m")


def fig_anatomy(r, out, theme):
    d = r.run(ANATOMY.replace('at = ["3 mm", "-34 mm"]', 'at = ["165 mm", "-42 mm"]'),
              [*SCALES, *REVISIONS, *ONE_CASE])
    s = r.sheet(d, "main-sheet-01")
    k = 1.75
    f = Fig(int(594 * k) + 60, int(420 * k) + 20)
    x0, y0 = 30, 10
    f.paper(s, (0, 0, 594, 420), x0, y0, 594 * k)

    def at(x, y):
        return x0 + x * k, y0 + y * k

    marks = [
        (1, (9, 210), None),                   # margin (binding edge)
        (2, (25, 130), (20, 130)),             # frame
        (3, (60, 36), (46, 50)),               # plan panel
        (4, (60, 96), (46, 110)),              # profile panel
        (5, (26, 300), (38, 306)),             # bands
        (6, (195, 42), (171, 40)),             # elevations
        (7, (250, 44), (238, 50)),             # corridor
        (8, (205, 20), (177, 20)),             # north arrow
        (9, (45, 354), (55, 372)),             # key map
        (10, (130, 354), (125, 397)),          # scale bar
        (11, (415, 345), (432, 345)),          # legend
        (12, (415, 368), (432, 368)),          # revisions
        (13, (415, 392), (432, 392)),          # title block
        (14, (215, 361), (195, 372)),          # note
        (15, (262, 192), (243, 213)),          # wires, layers
        (16, (136, 332), (130, 327)),          # station labels
    ]
    for n, p, target in marks:
        x, y = at(*p)
        if target:
            tx, ty = at(*target)
            f.leader(x, y, tx, ty)
        f.mark(x, y, n)
    # The margin as a dimension, outside the frame.
    x1, _ = at(0, 0)
    x2, _ = at(20, 0)
    y = at(0, 225)[1]
    f.line(x1, y, x2, y, "la", 1.2)
    # Marks read from the frame.
    f.write(out, "sheet-anatomy", theme)


def fig_page_frame(out, theme):
    f = Fig(860, 500)
    k = 0.95
    px, py, pw, ph = 40, 30, 780, 430
    # paper
    f.fill(px, py, pw, ph, "fi")
    f.rect(px, py, pw, ph, "l", 1.6)
    f.text(px + pw / 2, py - 10, "the paper: [page] format, orientation, or width + height", 13, "m", "middle")
    ml, mt, mr, mb = 70, 22, 22, 22
    fx, fy, fw, fh = px + ml, py + mt, pw - ml - mr, ph - mt - mb
    f.fill(fx, fy, fw, fh, "bg")
    f.rect(fx, fy, fw, fh, "l", 2.2)
    f.rect(fx + 7, fy + 7, fw - 14, fh - 14, "l", 0.7)
    pad = 20
    ix, iy, iw, ih = fx + 7 + pad, fy + 7 + pad, fw - 14 - 2 * pad, fh - 14 - 2 * pad
    f.rect(ix, iy, iw, ih, "lm", 1, "4 3")
    # margin dimensions
    f.dim(px, py + 100, fx, py + 100, "left", side=-6)
    f.text(px + 4, py + 122, "margin", 11, "a c")
    f.dim(fx + fw, py + 200, px + pw, py + 200, "", side=-6)
    f.text(fx + fw - 4, py + 204, "right", 12, "a c", "end")
    f.dim(px + 640, py, px + 640, fy, "top")
    f.dim(px + 640, fy + fh, px + 640, py + ph, "bottom")
    # border insets
    f.leader(fx + 150, fy - 12, fx + 150, fy + 0.5, "lb")
    f.text(fx + 156, fy - 2, "border[0]: inset 0, width 0.6 mm", 12, "b c")
    f.leader(fx + 150, fy + 44, fx + 150, fy + 7.5, "lb")
    f.text(fx + 156, fy + 40, "border[1]: inset 1.5 mm, width 0.2 mm", 12, "b c")
    # padding
    f.dim(fx + 7, fy + 90, ix, fy + 90, "padding", side=-8, anchor="start")
    # inside
    f.text(ix + 34, iy + 90, "Everything of the sheet stays inside the dashed line:", 14, "k")
    f.text(ix + 34, iy + 112, "the panels, title block, legend, key map, scale bars and notes.", 13)
    f.text(ix + 34, iy + 148, "A wide left margin is the binding edge.", 13, "m")
    f.text(ix + 34, iy + 170, "Lengths on paper carry their unit: \"10 mm\", \"1 cm\" (a bare number is mm).", 13, "m")
    f.write(out, "page-and-frame", theme)


def fig_layout(out, theme):
    f = Fig(860, 470)
    x, y, w = 150, 30, 620
    f.rect(x - 110, y - 12, w + 150, 430, "l", 1.5)
    f.text(x - 100, y + 2, "[layout]", 13, "c k")
    plan_h, gap, prof_h, band_h = 90, 24, 170, 54
    f.text(x, y + 14, "Plan", 13, "k")
    py = y + 20
    f.fill(x, py, w, plan_h, "fi")
    f.rect(x, py, w, plan_h)
    f.text(x + w / 2, py + plan_h / 2 + 5, "plan panel", 13, "m", "middle")
    f.text(x + 80, py - 6, 'plan_title = "Plan"', 12, "a c")
    f.dim(x + w + 12, py, x + w + 12, py + plan_h, "plan_height")
    q = py + plan_h + gap
    f.dim(x + w + 12, py + plan_h, x + w + 12, q, "gap")
    f.text(x, q - 6, "Profile", 13, "k")
    f.text(x + 80, q - 6, 'profile_title = "Profile"', 12, "a c")
    f.fill(x, q, w, prof_h, "fi")
    f.rect(x, q, w, prof_h)
    f.text(x + w / 2, q + prof_h / 2 + 5, "profile panel: takes the height that is left", 13, "m", "middle")
    b = q + prof_h
    for k, name in enumerate(["structures", "stations", "span_length"]):
        f.rect(x, b + k * 18, w, 18)
        f.text(x - 6, b + k * 18 + 13, name, 11, "m c", "end")
    f.text(x + w / 2, b + 31, "[[band]] rows, top to bottom", 12, "m", "middle")
    # axis room
    f.dim(x - 100, q + 60, x, q + 60, "axis_room", side=-7)
    for k, h in enumerate(["540", "530", "520"]):
        f.text(x - 6, q + 90 + k * 28, h, 11, "m", "end")
    # station labels
    sy = b + band_h + 16
    for k in range(5):
        f.text(x + k * 150, sy, f"0+{k}00.0", 11, "m", "middle")
    f.leader(x + w - 60, sy + 22, x + w - 22, sy + 4)
    f.text(x + w - 66, sy + 26, "station_labels = true", 12, "a c", "end")
    # the match line
    f.line(x + w, py - 4, x + w, b + band_h, "lb", 1.6, "6 3")
    f.text(x + w - 8, py + 12, "match line", 11, "b", "end")
    f.write(out, "layout-keys", theme)


def fig_places(out, theme):
    f = Fig(860, 340)
    x, y, w, h = 40, 30, 780, 280
    f.rect(x, y, w, h, "l", 1.6)
    # panels
    f.fill(x + 60, y + 12, w - 80, 50, "fi")
    f.text(x + 66, y + 30, "plan", 12, "m")
    f.fill(x + 60, y + 68, w - 80, 86, "fi")
    f.text(x + 66, y + 84, "profile and bands", 12, "m")
    # plan and profile places
    f.text(x + 300, y + 30, 'north_arrow place = "plan": one per straight stretch, top edge', 12, "a c")
    f.text(x + 66, y + 54, 'scale_bar place = "plan": lower left corner', 12, "a c")
    f.text(x + 66, y + 104, 'scale_bar place = "profile": upper left corner', 12, "a c")
    # the bottom row
    by = y + 168
    f.rect(x + 12, by, 480, h - 180, "la", 1.2, "4 3")
    f.text(x + 18, by + 16, 'the bottom row, left to right: place = "bottom"', 12, "a k")
    names = [("key_map", 90), ("stringing_table", 120), ("legend", 90), ("scale_bar", 90)]
    cx = x + 20
    for n, cw in names:
        f.rect(cx, by + 26, cw, 58, "l", 1)
        f.text(cx + cw / 2, by + 58, n, 12, "c", "middle")
        cx += cw + 8
    # above the title block
    tx = x + w - 262
    f.rect(tx, y + 158, 250, 52, "lo", 1.2, "4 3")
    f.text(tx + 6, y + 174, 'place = "above_title_block"', 12, "o k")
    f.text(tx + 6, y + 192, "legend, revisions, stringing_table", 12, "o c")
    f.rect(tx, y + 214, 250, 54, "l", 1.6)
    f.text(tx + 125, y + 245, "[title_block]", 13, "c k", "middle")
    f.write(out, "where-things-go", theme)


def fig_order(r, out, theme):
    first = r.run(plain(LAYOUT, BANDS), [*SCALES, *ONE_CASE], "A3")
    last = r.run(plain(LAYOUT.replace('["plan", "profile", "bands"]', '["profile", "bands", "plan"]'), BANDS),
                 [*SCALES, *ONE_CASE], "A3")
    a = r.sheet(first, "main-sheet-01")
    b = r.sheet(last, "main-sheet-01")
    box = (0, 0, 420, 297)
    f = Fig(1000, 400)
    caption(f, 20, 22, "The plan above the profile", 'order = ["plan", "profile", "bands"]')
    f.paper(a, box, 20, 50, 470)
    caption(f, 510, 22, "The plan below the profile", 'order = ["profile", "bands", "plan"]')
    f.paper(b, box, 510, 50, 470)
    f.write(out, "panel-order", theme)


def fig_beyond(r, out, theme):
    args = ["--hscale", "1500", "--vscale", "250", *ONE_CASE]
    t = plain(LAYOUT, BANDS)
    a = r.sheet(r.run(t, args), "main-sheet-01")
    b = r.sheet(r.run(t.replace('axis_room = "22 mm"', 'axis_room = "22 mm"\nbeyond_match = "faded"'), args),
                "main-sheet-01")
    # The sheet's right end: where it stops, and the paper beyond it.
    x = float(re.search(r'0\+\d+\.\d', a) is not None) and 0
    box = (330, 10, 256, 330)
    f = Fig(1000, 690)
    caption(f, 20, 22, "Nothing beyond the match line", 'beyond_match = "none" (default)')
    f.paper(a, box, 20, 50, 470)
    caption(f, 510, 22, "The next sheet shown faded", 'beyond_match = "faded"')
    f.paper(b, box, 510, 50, 470)
    f.write(out, "beyond-match", theme)


def fig_bands(r, out, theme):
    kinds = [("structures", 'format = "{name}\\n{type}"\nheight = "7 mm"'), ("stations", ""),
             ("ground_height", 'every = "100 m"'), ("span_length", ""), ("structure_height", ""),
             ("line_angle", ""), ("lowest_wire_height", ""), ("ground_clearance", ""),
             ("sag", ""), ("stringing", ""), ("text", 'field = "remarks"')]
    bands = "".join(f'[[band]]\nkind = "{k}"\nlabel = "{k}"\n{o}\n' for k, o in kinds)
    t = plain(LAYOUT.replace('axis_room = "22 mm"', 'axis_room = "34 mm"'), bands,
              '[cases]\nstringing = "everyday"\nconductors = "max temperature"\n'
              '[[field]]\nkey = "remarks"\nper_sheet = true\ndefault = "crossing of road R12"\n')
    d = r.run(t, [*SCALES, "--rules", RULES, *WEATHER])
    s = r.sheet(d, "main-sheet-01")
    _, y0 = text_at(s, "structures")
    _, y1 = text_at(s, "remarks") if False else text_at(s, "text")
    box = (32, y0 - 3, 400, y1 - y0 + 13)
    f = Fig(1040, int(1000 * box[3] / box[2]) + 30)
    f.paper(s, box, 20, 15, 1000)
    f.write(out, "band-rows", theme)


def fig_title_block(r, out, theme):
    d = r.run(plain(LAYOUT, BANDS), [*SCALES, *ONE_CASE])
    s = r.sheet(d, "main-sheet-01")
    # Grid: columns 28, * (the room left of 150 mm), 24, 34; rows 14, 6, 6, 6 mm.
    cols, rows = [28, 64, 24, 34], [14, 6, 6, 6]
    k = 4.8
    f = Fig(900, 590)
    gx, gy = 150, 76
    caption(f, 20, 24, "1. The grid: [title_block] columns and rows; each cell says where it sits")
    xs = [gx]
    for c in cols:
        xs.append(xs[-1] + c * k)
    ys = [gy]
    for c in rows:
        ys.append(ys[-1] + c * k)
    f.rect(xs[0], ys[0], xs[-1] - xs[0], ys[-1] - ys[0], "l", 1.6)
    for x in xs[1:-1]:
        f.line(x, ys[0], x, ys[-1], "lm")
    for y in ys[1:-1]:
        f.line(xs[0], y, xs[-1], y, "lm")
    labels = ['"28 mm"', '"*" (the room left)', '"24 mm"', '"34 mm"']
    for c, label in enumerate(labels):
        f.text((xs[c] + xs[c + 1]) / 2, gy - 22, f"column {c}", 12, "m", "middle")
        f.text((xs[c] + xs[c + 1]) / 2, gy - 7, label, 12, "a c", "middle")
    for q, label in enumerate(['"14 mm"', '"6 mm"', '"6 mm"', '"6 mm"']):
        f.text(gx - 66, (ys[q] + ys[q + 1]) / 2 + 4, f"row {q}", 12, "m", "end")
        f.text(gx - 8, (ys[q] + ys[q + 1]) / 2 + 4, label, 12, "a c", "end")
    # A spanning cell, and some cells.
    f.fill(xs[1] + 2, ys[0] + 2, xs[4] - xs[1] - 4, ys[1] - ys[0] - 4, "fi")
    f.text((xs[1] + xs[4]) / 2, ys[0] + 22, "at = [0, 1], span = [1, 3]", 13, "c k", "middle")
    f.text((xs[1] + xs[4]) / 2, ys[0] + 40, 'text = "{plan_kind}\\n{heading}", bold = true', 12, "c", "middle")
    f.text((xs[0] + xs[1]) / 2, ys[0] + 22, "at = [0, 0]", 12, "c", "middle")
    f.text((xs[0] + xs[1]) / 2, ys[0] + 38, 'image = "logo.svg"', 11, "c", "middle")
    for q in range(1, 4):
        for c in range(4):
            f.text((xs[c] + xs[c + 1]) / 2, (ys[q] + ys[q + 1]) / 2 + 4, f"[{q}, {c}]", 11, "c m", "middle")
    caption(f, 20, ys[-1] + 44, "2. What is drawn: the fields filled in, per sheet")
    # The title block on the sheet: bottom right, 150 mm wide, inside padding.
    tx, ty = text_at(s, "Route")
    box = (tx - 3.5, ty - 18, 152, 34)
    h = f.paper(s, box, gx, ys[-1] + 60, 150 * k)
    f.h = int(ys[-1] + 60 + h + 16)
    f.write(out, "title-block", theme)


def fig_corridor(r, out, theme):
    t = plain(LAYOUT.replace('plan_height = "45 mm"', 'plan_height = "50 mm"'), BANDS, '''
[corridor]
width = "25 m"
label = "corridor ±{width} m"
pen = { width = "0.3 mm" }
near_side = "right"
near = { colour = "#2a7f3f", dash = "6 1.5 1 1.5" }
far = { colour = "#7a3fa0", dash = "2 1.5" }

[centreline]
pen = { width = "0.2 mm", colour = "#000000", dash = "4 1 1 1" }
''')
    s = r.sheet(r.run(t, [*SCALES, *ONE_CASE]), "main-sheet-01")
    box = (45, 12, 275, 292)
    f = Fig(1040, 1100)
    h = f.paper(s, box, 20, 15, 1000)
    f.h = int(h + 30)
    # The same pens in plan and profile.
    k = 1000 / box[2]

    def at(x, y):
        return 20 + (x - box[0]) * k, 15 + (y - box[1]) * k

    x, y = at(150, 24)
    f.text(x, y, "far edge, far pen: the plan's upper edge", 13, "b k")
    x, y = at(150, 59)
    f.text(x, y, "near edge, near pen: the plan's lower edge (the right, looking along the stations)", 13, "o k")
    x, y = at(60, 108)
    f.text(x, y, "In the profile: the ground along each edge, in that edge's pen,", 13, "k")
    f.text(x, y + 18, "and the ground on the axis, always solid.", 13, "k")
    f.write(out, "corridor", theme)


def fig_elevations(r, out, theme):
    def sheet(e):
        t = plain(LAYOUT.replace('plan_height = "45 mm"', 'plan_height = "62 mm"'), BANDS, e)
        return r.sheet(r.run(t, [*SCALES, *ONE_CASE]), "main-sheet-01")

    above = sheet('[elevations]\nplace = "above"\noffset = "6 mm"\nrotation = 90\nscale = 1000\n'
                  'labels = ["number", "type", "height"]\nsize = "2 mm"\n')
    below = sheet('[elevations]\nplace = "below"\noffset = "6 mm"\nrotation = 90\nscale = 1000\n'
                  'labels = ["number", "string_length", "dimensions", "arm_heights"]\nsize = "2 mm"\n')
    turned = sheet('[elevations]\nplace = "above"\noffset = "6 mm"\nrotation = 60\nscale = 1000\n'
                   'labels = ["number"]\nsize = "2 mm"\n')
    stick = sheet('[elevations]\nplace = "above"\noffset = "6 mm"\nrotation = 90\nscale = 1000\nstick = true\n'
                  'labels = ["number", "height"]\nsize = "2 mm"\n')
    box = (150, 12, 130, 76)
    w = 490
    h = w * box[3] / box[2]
    f = Fig(1020, int(2 * h + 150))
    caption(f, 20, 22, "Above the line, its own scale", 'place = "above", rotation = 90, scale = 1000')
    f.paper(above, box, 20, 50, w)
    caption(f, 530, 22, "Below the line, more labels", 'place = "below", labels = […, "dimensions", "arm_heights"]')
    f.paper(below, box, 530, 50, w)
    y = 50 + h + 28
    caption(f, 20, y, "Turned", "rotation = 60 (degrees between the line and the height)")
    f.paper(turned, box, 20, y + 28, w)
    caption(f, 530, y, "Stick figures", "stick = true (even where members exist)")
    f.paper(stick, box, 530, y + 28, w)
    f.write(out, "elevations", theme)


def fig_clearance(r, out, theme):
    args = [*SCALES, "--rules", RULES, *ONE_CASE]
    per = plain(LAYOUT, BANDS, '''
[conductor_clearance]
show = true
pen = { width = "0.3 mm", colour = "#a01818", dash = "5 1 1 1 1 1" }

[sag_labels]
show = true
size = "2.2 mm"
''')
    env = plain(LAYOUT, BANDS, '''
[conductor_clearance]
show = true
envelope = true
mode = "instead"
pen = { width = "0.3 mm", colour = "#a01818", dash = "5 1 1 1 1 1" }
''')
    a = r.sheet(r.run(per, args), "main-sheet-01")
    b = r.sheet(r.run(env, args), "main-sheet-01")
    box = (45, 118, 330, 186)
    w = 1000
    h = w * box[3] / box[2]
    f = Fig(1040, int(2 * h + 150))
    caption(f, 20, 22, "A line per circuit, and the sag written in each span",
            '[conductor_clearance] show = true, mode = "also"; [sag_labels] show = true')
    f.paper(a, box, 20, 50, w)
    y = 50 + h + 28
    caption(f, 20, y, "One line for the whole line, instead of the line above the ground",
            '[conductor_clearance] envelope = true, mode = "instead"')
    f.paper(b, box, 20, y + 28, w)
    f.write(out, "conductor-clearance", theme)


def fig_wires(r, out, theme):
    args = [*SCALES, *ONE_CASE]
    base = r.sheet(r.run(plain(LAYOUT, BANDS), args), "main-sheet-01")
    kinds = plain(LAYOUT, BANDS, '''
[line_styles]
by_kind = true
[line_styles.kinds.hv]
colour = "#c00000"
width = "0.4 mm"
label = "high voltage"
[line_styles.kinds.earth]
colour = "#606060"
width = "0.25 mm"
label = "earth wire"
''')
    by = r.sheet(r.run(kinds, args), "main-sheet-01")
    roles = plain(LAYOUT, BANDS, '''
[roles.project]
label = "new"
weight = 1.8
[roles.other]
label = "other line"
colour = "#8c8c8c"
weight = 0.7
names = true
dash = "4 1.5"
''')
    ro = r.sheet(r.run(roles, [*args, "--role", "project", "--version", f"{CROSSING}=other"]), "main-sheet-01")
    w = 1000
    x0, y0 = 45, 12
    bw, bh = 330, 57
    h = w * bh / bw
    plan = (x0, y0, bw, bh)
    f = Fig(1040, int(3 * (h + 62)) + 10)
    y = 22
    for title, code, sheet in (("Phase colours (the default)", " ", base),
                               ("By kind", "[line_styles] by_kind = true, kinds.hv: red, 0.4 mm", by),
                               ("By role, with another line nearby", "[roles.project] weight = 1.8; [roles.other] grey, dashed", ro)):
        caption(f, 20, y, title, code)
        f.paper(sheet, plan, 20, y + 28, w)
        y += h + 62
    f.write(out, "wire-styles", theme)


def fig_key_map(r, out, theme):
    args = ["--hscale", "1000", "--vscale", "250", *ONE_CASE]
    kmap = '''
[scale_bar]
place = "bottom"

[key_map]
place = "bottom"
width = "110 mm"
height = "62 mm"
structures = true
highlight = { width = "0.7 mm", colour = "#cc2222" }
'''
    a = r.sheet(r.run(plain(LAYOUT, BANDS, kmap), args), "main-sheet-02")
    b = r.sheet(r.run(plain(LAYOUT, BANDS, kmap.replace('height = "62 mm"', 'height = "62 mm"\nextent = "sheet"\nmargin = "100 m"')), args),
                "main-sheet-02")
    box = (22, 345, 190, 67)
    w = 490
    f = Fig(1020, int(w * box[3] / box[2] + 90))
    caption(f, 20, 22, "The whole route, this sheet marked", 'extent = "route" (default)')
    f.paper(a, box, 20, 52, w)
    caption(f, 530, 22, "A window around this sheet", 'extent = "sheet", margin = "100 m"')
    f.paper(b, box, 530, 52, w)
    f.write(out, "key-map", theme)


def fig_tables(r, out, theme):
    t = plain(LAYOUT, BANDS, '''
[cases]
stringing = "everyday"
conductors = "max temperature"

[revisions]
place = "above_title_block"
rows = 3
columns = [
  { title = "Rev", width = "12 mm", value = "{rev.index}" },
  { title = "Date", width = "24 mm", value = "{rev.date}" },
  { title = "Description", width = "66 mm", value = "{rev.text}" },
  { title = "By", width = "24 mm", value = "{rev.by}" },
  { title = "Checked", width = "24 mm", value = "{rev.checked}" },
]

[stringing_table]
place = "above_title_block"
rows = 5
columns = [
  { title = "From", width = "18 mm", value = "{st.from}" },
  { title = "To", width = "18 mm", value = "{st.to}" },
  { title = "Circuit", width = "36 mm", value = "{st.circuit}" },
  { title = "Phase", width = "18 mm", value = "{st.phase}" },
  { title = "Tension (kN)", width = "30 mm", value = "{st.tension}" },
  { title = "Case", width = "30 mm", value = "{st.case}" },
]
''')
    d = r.run(t, [*SCALES, *WEATHER, *REVISIONS])
    s = r.sheet(d, "main-sheet-01")
    tx, ty = text_at(s, "Route")
    box = (tx - 5, ty - 72, 156, 90)
    w = 760
    f = Fig(800, int(w * box[3] / box[2] + 30))
    f.paper(s, box, 20, 15, w)
    f.write(out, "tables", theme)


def fig_notes(out, theme):
    f = Fig(860, 370)
    x, y, w, h = 40, 24, 780, 320
    f.rect(x, y, w, h, "l", 1.6)
    f.text(x + 8, y + 18, "the frame", 12, "m")
    f.fill(x + 70, y + 30, w - 90, 60, "fi")
    f.text(x + 110, y + 50, "plan", 12, "m")
    f.fill(x + 70, y + 96, w - 90, 80, "fi")
    f.text(x + 110, y + 116, "profile and bands", 12, "m")
    f.line(x + 70, y + 176, x + w - 20, y + 176, "lb", 1.4, "6 3")
    f.text(x + w - 26, y + 170, "the drawing's lower edge", 11, "b", "end")
    # 1 from the frame's top left
    f.dim(x, y + 262, x + 90, y + 262, '"90 mm"', side=-6)
    f.dim(x + 90, y, x + 90, y + 262, '"210 mm"')
    f.fill(x + 90, y + 262, 190, 28, "bg")
    f.rect(x + 90, y + 262, 190, 28, "la")
    f.text(x + 96, y + 281, 'at = ["90 mm", "210 mm"]', 11, "a c")
    # 2 negative: from the bottom right
    f.fill(x + w - 268, y + h - 96, 190, 28, "bg")
    f.rect(x + w - 268, y + h - 96, 190, 28, "lo")
    f.text(x + w - 262, y + h - 77, 'at = ["-80 mm", "-30 mm"]', 11, "o c")
    f.dim(x + w - 268, y + h - 40, x + w, y + h - 40, "negative: from the right and bottom edges", tcls="o", cls="lo", side=16)
    f.dim(x + w - 40, y + h - 96, x + w - 40, y + h, "", cls="lo")
    # 3 from the drawing
    f.dim(x + 240, y + 176, x + 240, y + 204, '"6 mm"', cls="lb", tcls="b")
    f.fill(x + 240, y + 204, 250, 28, "bg")
    f.rect(x + 240, y + 204, 250, 28, "lb")
    f.text(x + 246, y + 223, 'from = "drawing", at = [x, "6 mm"]', 11, "b c")
    f.text(x + 240, y + 248, "follows the drawing on every sheet", 11, "b")
    f.write(out, "notes", theme)


def fig_backgrounds(out, theme):
    f = Fig(860, 500)
    caption(f, 20, 24, "[plan_background]: the ground is straightened along the line, like everything on the plan")
    # Ground, with a bend.
    ax = [(50, 150), (190, 150), (360, 80)]
    f.text(50, 62, "on the ground", 12, "m")
    for (a, b) in zip(ax, ax[1:]):
        f.line(a[0], a[1], b[0], b[1], "l", 2)
    # Parcel lines
    for x, y, x2, y2 in [(95, 72, 110, 215), (163, 72, 170, 215), (240, 72, 280, 190), (305, 72, 345, 175)]:
        f.line(x, y, x2, y2, "lm", 1)
    f.text(50, 175, "axis", 12, "k")
    f.text(232, 172, "bend", 12, "b")
    f.line(240, 160, 196, 152, "lb", 1)
    # Sheet
    sx = 460
    f.text(sx, 62, "on the sheet: station along, offset across", 12, "m")
    f.rect(sx, 74, 360, 150, "l", 1.2)
    f.line(sx, 149, sx + 360, 149, "l", 2)
    for x, xo in [(sx + 45, 3), (sx + 115, 2), (sx + 195, -2), (sx + 270, -3)]:
        f.line(x - xo * 10, 74, x + xo * 2, 224, "lm", 1)
    f.line(sx + 172, 74, sx + 172, 224, "lb", 1, "4 3")
    f.text(sx + 178, 90, "the bend: a wedge not shown outside,", 11, "b")
    f.text(sx + 178, 104, "shown twice inside", 11, "b")
    f.line(420, 149, 450, 149, "la", 2)
    f.line(442, 143, 450, 149, "la", 2)
    f.line(442, 155, 450, 149, "la", 2)
    # The veil.
    caption(f, 20, 270, "veil: white mixed into the orthophoto, so that the line stays readable", "veil = 0 … 1 (default 0.6)")
    vs = [0.0, 0.3, 0.6, 0.9]
    for k, v in enumerate(vs):
        x = 20 + k * 205
        f.fill(x, 305, 190, 60, "fi")
        f.raw(f'<defs><linearGradient id="g{k}"><stop offset="0" stop-color="#4d6b3c"/>'
              f'<stop offset=".5" stop-color="#8a7a4a"/><stop offset="1" stop-color="#3f5d7a"/>'
              f'</linearGradient></defs>')
        f.raw(f'<rect x="{x}" y="305" width="190" height="60" fill="url(#g{k})"/>')
        f.raw(f'<rect x="{x}" y="305" width="190" height="60" fill="#fff" fill-opacity="{v}"/>')
        f.raw(f'<line x1="{x}" y1="335" x2="{x + 190}" y2="335" stroke="#d00" stroke-width="2"/>')
        f.text(x + 95, 383, f"veil = {v:g}", 12, "c", "middle")
    # The LiDAR cell.
    caption(f, 20, 420, "[lidar] cell: one dot per class in each square of the paper", 'size = "0.25 mm", cell = "0.2 mm". Only what is drawn is thinned')
    gx, gy = 640, 410
    s = 16
    pts = [(1, 1), (5, 3), (9, 2), (14, 5), (18, 1), (22, 4), (27, 2), (31, 5), (3, 7), (12, 8), (20, 9), (29, 8)]
    for row in range(3):
        for col in range(5):
            f.rect(gx + col * s, gy + row * s, s, s, "lm", 0.5)
    for (px, py) in pts:
        f.raw(f'<circle class="fk" cx="{gx + px * 2}" cy="{gy + py * 4}" r="1.5" style="fill:var(--muted)"/>')
    for (cx, cy) in {(int(px * 2 // s), int(py * 4 // s)) for (px, py) in pts}:
        f.raw(f'<circle class="fa" cx="{gx + (cx + .5) * s}" cy="{gy + (cy + .5) * s}" r="3"/>')
    f.text(gx + 92, gy + 16, "grey: the points", 12, "m")
    f.text(gx + 92, gy + 34, "blue: drawn", 12, "a")
    f.write(out, "backgrounds-and-lidar", theme)


def fig_plan_set(out, theme):
    f = Fig(860, 300)
    f.fill(20, 30, 270, 240, "fi", 6)
    f.text(32, 54, "submission.ohlset", 13, "c k")
    rows = [('kind = "sheets"', 'page = "A1", scale = 2000'), ('kind = "situation"', 'page = "A1", scale = 1000'),
            ('kind = "situation"', 'page = "A3", scale = 500'), ('kind = "phase"', 'page = "A3"')]
    out_rows = [["main-sheet-01.pdf", "main-sheet-02.pdf …"], ["main-situation-plan.pdf"], ["main-situation-plan-2.pdf"], ["main-phase-plan.pdf"]]
    for k, (a, b) in enumerate(rows):
        y = 70 + k * 49
        f.rect(32, y, 246, 42, "lm")
        f.text(38, y + 17, a, 12, "c a")
        f.text(38, y + 34, b, 11, "c m")
        f.line(290, y + 21, 340, y + 21, "la", 1.5)
        f.line(333, y + 16, 340, y + 21, "la", 1.5)
        f.line(333, y + 26, 340, y + 21, "la", 1.5)
        for q, n in enumerate(out_rows[k]):
            f.text(350, y + 17 + q * 16, n, 12, "c")
    f.text(350, 54, "for each route (here main)", 12, "m")
    f.rect(610, 90, 220, 110, "lo", 1.3, r=6)
    f.text(622, 114, "Submission.pdf", 13, "c k")
    f.text(622, 136, "--set-pdf / also as one PDF:", 12, "o")
    f.text(622, 154, "every sheet of the set,", 12)
    f.text(622, 172, "in the set's order", 12)
    f.line(560, 145, 610, 145, "lo", 1.5)
    f.line(603, 140, 610, 145, "lo", 1.5)
    f.line(603, 150, 610, 145, "lo", 1.5)
    f.write(out, "plan-set", theme)


def fig_plans(r, out, theme):
    plan_tpl = plain("[corridor]\nwidth = \"25 m\"\n")
    args = ["--plans", "route,situation,phase", "--route-plan-scale", "5000", "--situation-scale", "1000",
            "--line-width", "60", *ONE_CASE]
    d = r.run(plan_tpl, args, "A3")
    route = r.sheet(d, "main-route-plan-01")
    situation = r.sheet(d, "main-situation-plan-01")
    phase = r.sheet(d, "main-phase-plan-01")
    w = 620
    k = w / 420
    box = (0, 0, 420, 297)
    h = w * 297 / 420

    def sheet(name, s, marks, side):
        f = Fig(w + 400, int(h + 24))
        f.paper(s, box, 12, 12, w)
        # The frame and title block from the template; the rest from the program.
        y = 34
        f.text(w + 34, y, "Taken from the template", 14, "o k")
        y += 22
        for t in ["[page] margin", "[frame] border, padding", "[title_block] and its cells",
                  "[[field]], [if_empty], [[image]]", "[text] colour", "[layers] OHL_FRAME", "the paper's format"]:
            f.text(w + 34, y, t, 12, "c")
            y += 18
        for t in side[0]:
            f.text(w + 34, y, t, 12, "c")
            y += 18
        y += 18
        f.text(w + 34, y, "Always drawn by the program", 14, "b k")
        y += 22
        for t in side[1]:
            f.text(w + 34, y, t, 12)
            y += 18
        for (mx, my, tx, ty, cls) in marks:
            f.leader(12 + mx * k, 12 + my * k, 12 + tx * k, 12 + ty * k, cls)
        f.write(out, name, theme)

    sheet("plan-route", route, [], (['[legend.labels] axis,', '  corridor_near, plan_route'],
                                    ["north up, north arrow, scale bar", "legend beside the scale bar",
                                     "the line's axis, the wires,", "  the structures numbered",
                                     "the corridor's pen; its width is", "  the drawings' line width",
                                     "the scale (1:5000 to 1:25 000)"]))
    sheet("plan-situation", situation, [], (['[legend.labels] axis,', '  corridor_near, plan_situation'],
                                            ["as the route plan, at 1:500 to 1:2000", "the sheets overlap by a tenth",
                                             "over the cadastral survey (a", "  drawing) and the orthophotos,",
                                             "  under a white veil of 0.4"]))
    sheet("plan-phase", phase, [], (['[legend.labels] phase_circuits,', '  phase_line_type, phase_circuit,',
                                     '  phase_conductor, phase_owner,', '  phase_number, plan_phase'],
                                    ["a track per wire, in its", "  circuit's colour and dashes",
                                     "a column per run of structures", "  with the same arrangement",
                                     "the table of the circuits", "  under the tracks"]))


def main():
    args = sys.argv[1:]
    theme = None
    if "--theme" in args:
        i = args.index("--theme")
        theme = args[i + 1]
        del args[i:i + 2]
    if "--fit" in args:
        global FIT
        i = args.index("--fit")
        FIT = int(args[i + 1])
        del args[i:i + 2]
    out = args[0] if args else "docs/images/sheet-templates"
    os.makedirs(out, exist_ok=True)
    if not os.path.exists(OHL):
        sys.exit(f"{OHL}: build it first (cargo build -p ohl-cli), or set OHL")
    with tempfile.TemporaryDirectory() as work:
        r = Renderer(work)
        fig_anatomy(r, out, theme)
        fig_page_frame(out, theme)
        fig_layout(out, theme)
        fig_places(out, theme)
        fig_order(r, out, theme)
        fig_beyond(r, out, theme)
        fig_bands(r, out, theme)
        fig_title_block(r, out, theme)
        fig_corridor(r, out, theme)
        fig_elevations(r, out, theme)
        fig_clearance(r, out, theme)
        fig_wires(r, out, theme)
        fig_key_map(r, out, theme)
        fig_tables(r, out, theme)
        fig_notes(out, theme)
        fig_backgrounds(out, theme)
        fig_plan_set(out, theme)
        fig_plans(r, out, theme)


if __name__ == "__main__":
    main()
