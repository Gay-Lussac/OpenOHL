#!/usr/bin/env python3
"""Write the glossary's drawings (docs/images/glossary/*.svg).

Standard library only; run from the repository root with

    python3 -I tools/glossary_figures.py            # writes docs/images/glossary/
    python3 -I tools/glossary_figures.py OUT --theme light|dark [--fit 760]
                                                     # copies with one palette forced and
                                                     # at most 760 px wide (to look at both,
                                                     # e.g. as PNG with `qlmanage -t`)

The drawings are invented sketches (no real line). Colours are CSS custom
properties switched by `prefers-color-scheme`, every drawing paints its own
background, and nothing external is loaded (system fonts only), so they read
in the book's light and dark themes alike. Labels use the glossary's terms:
after a renaming, change them here and run the script again.
"""

import math
import os
import sys

LIGHT = {"ink": "#1f2328", "muted": "#59636e", "acc": "#0969da", "wire": "#bc4c00",
         "wire2": "#1a7f37", "wire3": "#8250df", "bg": "#ffffff", "fill": "#eef1f4"}
DARK = {"ink": "#e6edf3", "muted": "#9198a1", "acc": "#58a6ff", "wire": "#f0883e",
        "wire2": "#3fb950", "wire3": "#bc8cff", "bg": "#0d1117", "fill": "#1c2128"}


def palette(p):
    return "".join(f"--{k}:{v};" for k, v in p.items())


def style(theme):
    if theme == "light":
        colours = f"svg{{{palette(LIGHT)}}}"
    elif theme == "dark":
        colours = f"svg{{{palette(DARK)}}}"
    else:
        colours = (f"svg{{{palette(LIGHT)}}}"
                   f"@media (prefers-color-scheme: dark){{svg{{{palette(DARK)}}}}}")
    return (
        "<style>" + colours +
        ".bg{fill:var(--bg)}"
        ".st{stroke:var(--ink);stroke-width:2.5;fill:none;stroke-linecap:round;stroke-linejoin:round}"
        ".thin{stroke:var(--muted);stroke-width:1.2;fill:none}"
        ".dash{stroke:var(--muted);stroke-width:1.2;fill:none;stroke-dasharray:5 4}"
        ".str{stroke:var(--acc);stroke-width:3;fill:none;stroke-linecap:round}"
        ".wire{stroke:var(--wire);stroke-width:2.2;fill:none}"
        ".wire2{stroke:var(--wire2);stroke-width:2.2;fill:none}"
        ".wire3{stroke:var(--wire3);stroke-width:2.2;fill:none}"
        ".band{fill:var(--fill);stroke:var(--muted);stroke-width:1;stroke-dasharray:4 3}"
        ".box{fill:var(--fill);stroke:var(--muted);stroke-width:1}"
        ".ap{fill:var(--bg);stroke:var(--ink);stroke-width:2}"
        ".wp{fill:var(--wire);stroke:none}"
        ".dot{fill:var(--ink);stroke:none}"
        ".ldr{stroke:var(--muted);stroke-width:1;fill:none}"
        "text{font:13px system-ui,-apple-system,'Segoe UI',Roboto,sans-serif;fill:var(--ink);"
        "paint-order:stroke;stroke:var(--bg);stroke-width:4px;stroke-linejoin:round}"
        ".lab{fill:var(--muted);font-size:12px}"
        ".t{font-weight:600;font-size:14px}"
        ".c{text-anchor:middle}.e{text-anchor:end}"
        "</style>"
    )


class Fig:
    def __init__(self, name, w, h, title, desc):
        self.name, self.w, self.h, self.title, self.desc = name, w, h, title, desc
        self.parts = []

    def add(self, s):
        self.parts.append(s)

    def text(self, x, y, s, cls=""):
        c = f' class="{cls}"' if cls else ""
        self.add(f'<text x="{x:.1f}" y="{y:.1f}"{c}>{s}</text>')

    def label(self, x, y, tx, ty, s, cls="", end=None):
        """A leader from (x, y) to the label's anchor (tx, ty); the text runs away from
        the leader unless `end` says on which side it ends."""
        if end is None:
            end = tx < x
        self.add(f'<path class="ldr" d="M{x:.1f},{y:.1f} L{tx:.1f},{ty:.1f}"/>')
        self.text(tx + (-4 if end else 4), ty + 4, s, (cls + " e").strip() if end else cls)

    def dim(self, x0, x1, y, s, cls=""):
        """A horizontal dimension line with end ticks and its text above."""
        self.add(f'<path class="thin" d="M{x0},{y} L{x1},{y} M{x0},{y-5} L{x0},{y+5} '
                 f'M{x1},{y-5} L{x1},{y+5}"/>')
        self.text((x0 + x1) / 2, y - 6, s, ("c " + cls).strip())

    def vdim(self, x, y0, y1, s, left=False):
        self.add(f'<path class="thin" d="M{x},{y0} L{x},{y1} M{x-5},{y0} L{x+5},{y0} '
                 f'M{x-5},{y1} L{x+5},{y1}"/>')
        self.text(x + (-8 if left else 8), (y0 + y1) / 2 + 4, s, "e" if left else "")

    def svg(self, theme, fit=None):
        w, h = self.w, self.h
        if fit and w > fit:
            w, h = fit, round(self.h * fit / self.w)
        return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {self.w} {self.h}" '
                f'width="{w}" height="{h}" role="img" aria-labelledby="t d">'
                f'<title id="t">{self.title}</title><desc id="d">{self.desc}</desc>'
                f'{style(theme)}<rect class="bg" width="{self.w}" height="{self.h}"/>'
                + "".join(self.parts) + "</svg>\n")


def string(f, x0, y0, x1, y1):
    f.add(f'<path class="str" d="M{x0},{y0} L{x1},{y1}"/>')


def ap(f, x, y, r=4.5):
    f.add(f'<circle class="ap" cx="{x}" cy="{y}" r="{r}"/>')


def wp(f, x, y, r=5):
    f.add(f'<circle class="wp" cx="{x}" cy="{y}" r="{r}"/>')


def sag_path(x0, y0, x1, y1, sag):
    """A parabola from (x0, y0) to (x1, y1) sagging `sag` below the chord's middle."""
    pts = []
    for i in range(21):
        t = i / 20
        x = x0 + (x1 - x0) * t
        y = y0 + (y1 - y0) * t + 4 * sag * t * (1 - t)
        pts.append(f"{x:.1f},{y:.1f}")
    return "M" + " L".join(pts)


# 1. A structure and its parts -------------------------------------------------------
def structure_parts():
    f = Fig("structure-parts", 700, 420, "A structure and its parts",
            "A structure seen along the line: attachment points on its crossarms, a string "
            "hanging from each to a wire point with its set number, and the earth wire's "
            "wire point at the peak, fixed without a string.")
    f.text(20, 26, "A structure, looking ahead along the line", "t")
    f.add('<path class="st" d="M300,395 L330,100 L360,395 M315,395 L345,395"/>')
    f.add('<path class="st" d="M210,140 L450,140 M230,215 L430,215 M330,80 L330,100"/>')
    for i, (x, y) in enumerate([(220, 140), (440, 140), (240, 215), (420, 215)], start=1):
        string(f, x, y, x, y + 45)
        wp(f, x, y + 45)
        ap(f, x, y)
        f.text(x + 10, y + 50, f"set {i}", "lab")
    wp(f, 330, 80)
    f.text(342, 85, "set 5", "lab")
    f.label(220, 140, 150, 105, "attachment point")
    f.label(220, 162, 150, 162, "string")
    f.label(240, 260, 170, 300, "wire point")
    f.label(330, 80, 420, 60, "earth wire's wire point (fixed, no string)")
    f.label(347, 320, 430, 340, "structure (here a lattice tower)")
    return f


# 2. Strings combined -------------------------------------------------------------------
def string_combinations():
    f = Fig("string-combinations", 820, 300, "Strings combined",
            "Four ways single strings combine: one string; two strings to one wire point; "
            "a string below the wire point of two strings; two tension strings hanging from a "
            "suspension string's wire point, each ending a section.")
    f.text(20, 26, "Strings and how they combine: each one is a single string", "t")

    def arm(x0, x1, y=80):
        f.add(f'<path class="st" d="M{x0},{y} L{x1},{y}"/>')

    arm(50, 150)
    string(f, 100, 80, 100, 180)
    ap(f, 100, 80)
    wp(f, 100, 180)
    f.text(100, 228, "one string", "c")

    arm(210, 360)
    string(f, 225, 80, 285, 180)
    string(f, 345, 80, 285, 180)
    ap(f, 225, 80)
    ap(f, 345, 80)
    wp(f, 285, 180)
    f.text(285, 228, "two strings,", "c")
    f.text(285, 244, "one wire point", "c")

    arm(410, 560)
    string(f, 425, 80, 485, 150)
    string(f, 545, 80, 485, 150)
    string(f, 485, 150, 485, 195)
    ap(f, 425, 80)
    ap(f, 545, 80)
    ap(f, 485, 150, 4)
    wp(f, 485, 195)
    f.text(485, 228, "a string below", "c")
    f.text(485, 244, "two strings", "c")

    arm(640, 740)
    string(f, 690, 80, 690, 140)
    string(f, 690, 140, 630, 152)
    string(f, 690, 140, 750, 152)
    f.add('<path class="wire" d="M590,170 Q610,158 630,152 M750,152 Q770,158 790,170"/>')
    ap(f, 690, 80)
    ap(f, 690, 140, 4)
    wp(f, 630, 152)
    wp(f, 750, 152)
    f.text(690, 228, "two tension strings", "c")
    f.text(690, 244, "from a string (side view)", "c")

    ap(f, 30, 279)
    f.text(42, 283, "attachment point, or a wire point strings hang from", "lab")
    wp(f, 400, 279)
    f.text(412, 283, "wire point", "lab")
    f.add('<path class="str" d="M510,279 L540,279"/>')
    f.text(548, 283, "string (one straight member)", "lab")
    return f


# 3. A section ---------------------------------------------------------------------------
def section():
    f = Fig("section", 860, 330, "A section",
            "A section seen from the side: a wire strung from a tension structure over three "
            "suspension structures to the next tension structure, four spans; a jumper joins it "
            "to the next section around the last tension structure.")
    f.text(20, 26, "A section, seen from the side", "t")
    xs = [70, 240, 410, 580, 750]
    top, ground = 110, 300
    f.add(f'<path class="dash" d="M20,{ground} L840,{ground}"/>')
    for x in xs:
        f.add(f'<path class="st" d="M{x},{top} L{x},{ground}"/>')
    for x in xs[1:-1]:
        string(f, x, top, x, top + 30)
    # tension strings at both ends of the section, and of the next one at the last structure
    string(f, xs[0], top + 20, xs[0] + 28, top + 24)
    string(f, xs[-1], top + 20, xs[-1] - 28, top + 24)
    string(f, xs[-1], top + 20, xs[-1] + 28, top + 24)
    pts = [(xs[0] + 28, top + 24)] + [(x, top + 30) for x in xs[1:-1]] + [(xs[-1] - 28, top + 24)]
    d = " ".join(sag_path(x0, y0, x1, y1, 55) for (x0, y0), (x1, y1) in zip(pts, pts[1:]))
    f.add(f'<path class="wire" d="{d}"/>')
    f.add(f'<path class="wire" d="{sag_path(xs[-1] + 28, top + 24, 840, top + 40, 10)}"/>')
    f.add(f'<path class="wire" stroke-dasharray="5 3" '
          f'd="M{xs[-1]-28},{top+24} Q{xs[-1]},{top+85} {xs[-1]+28},{top+24}"/>')
    for x, y in pts:
        wp(f, x, y)
    wp(f, xs[-1] + 28, top + 24)
    f.dim(xs[0], xs[-1], 52, "section (tension section)")
    f.dim(xs[1], xs[2], 86, "span")
    f.label(xs[0] + 14, top + 22, 120, 205, "tension string")
    f.label(xs[2], top + 15, 450, 100, "suspension string")
    f.label(xs[0], 250, 100, 270, "tension structure")
    f.label(xs[2], 250, 440, 270, "suspension structure")
    f.label(xs[-1], top + 54, 700, 215, "jumper (to the next section)", "")
    return f


# 4. Systems, phases, earth wire ------------------------------------------------------
def systems():
    f = Fig("systems", 700, 470, "Systems, phases and the earth wire",
            "A structure carrying two systems, one on each side, three phases each, and one "
            "earth wire at the peak. Each colour is a conductor path: the sections and jumpers "
            "of one phase joined end to end.")
    f.text(20, 26, "Two systems on one structure, looking ahead", "t")
    f.add('<path class="st" d="M320,410 L350,100 L380,410 M335,410 L365,410"/>')
    f.add('<path class="st" d="M230,140 L470,140 M210,200 L490,200 M240,260 L460,260 M350,80 L350,100"/>')
    rows = [(140, 120, "L1"), (200, 100, "L2"), (260, 90, "L3")]
    cls = ["wire", "wire2", "wire3"]
    for (y, half, ph), c in zip(rows, cls):
        for side in (-1, 1):
            x = 350 + side * half
            string(f, x, y, x, y + 35)
            f.add(f'<circle cx="{x}" cy="{y+35}" r="9" class="{c}" style="stroke-width:3"/>')
            ap(f, x, y)
            f.text(x + side * 16, y + 40, ph, "" if side > 0 else "e")
    f.add('<circle class="dot" cx="350" cy="80" r="5"/>')
    f.label(350, 80, 430, 58, "earth wire (no phase, earthed)")
    f.add('<path class="thin" d="M150,100 L150,320 M550,100 L550,320"/>')
    f.text(150, 340, "system A, 220 kV", "c")
    f.text(150, 356, "phases L1 L2 L3", "c lab")
    f.text(550, 340, "system B, 220 kV", "c")
    f.text(550, 356, "phases L1 L2 L3", "c lab")
    f.text(20, 436, "One colour = one conductor path: one phase's sections and jumpers,", "lab")
    f.text(20, 452, "joined end to end through the line (it carries one phase of one system).",
           "lab")
    return f


# 4b. System, circuit, conductor path, phase ----------------------------------------------
def circuits():
    f = Fig("circuits", 760, 420, "System, circuit, conductor path and phase",
            "Plan, schematic. In system A, circuit North holds two conductor paths: its phase L1, "
            "two sections joined by a jumper at a tension structure (one path), and its phase "
            "L2, one section. A branch of phase L1 that is not connected to North's L1 is a "
            "second circuit; a jumper joining them would make one path, and one circuit.")
    f.text(20, 26, "System, circuit, conductor path, phase (plan, schematic)", "t")
    f.add('<rect class="band" x="40" y="50" width="560" height="300" rx="8"/>')
    f.text(52, 340, "system A, 220 kV — phases L1 L2 L3", "lab")
    xs = [90, 330, 560]
    for x in xs:
        f.add(f'<rect class="box" x="{x-8}" y="92" width="16" height="96"/>')
    # North L1: two sections joined by a jumper at the middle structure.
    f.add(f'<path class="wire" d="M{xs[0]},110 L{xs[1]-14},110"/>')
    f.add(f'<path class="wire" d="M{xs[1]+14},110 L{xs[2]},110"/>')
    f.add(f'<path class="wire" stroke-dasharray="5 3" '
          f'd="M{xs[1]-14},110 Q{xs[1]},132 {xs[1]+14},110"/>')
    for x in (xs[0], xs[1] - 14, xs[1] + 14, xs[2]):
        wp(f, x, 110, 4)
    # North L2: one section.
    f.add(f'<path class="wire2" d="M{xs[0]},170 L{xs[2]},170"/>')
    for x in (xs[0], xs[2]):
        f.add(f'<circle cx="{x}" cy="170" r="4" style="fill:var(--wire2)"/>')
    f.text(xs[0] - 14, 114, "L1", "e")
    f.text(xs[0] - 14, 174, "L2", "e")
    f.text(210, 102, "section", "c lab")
    f.text(450, 102, "section", "c lab")
    f.label(xs[1], 124, 380, 145, "jumper", "")
    # The branch: L1 too, not connected.
    for x in (200, 470):
        f.add(f'<rect class="box" x="{x-8}" y="262" width="16" height="40"/>')
    f.add('<path class="wire3" d="M200,282 L470,282"/>')
    for x in (200, 470):
        f.add(f'<circle cx="{x}" cy="282" r="4" style="fill:var(--wire3)"/>')
    f.text(186, 286, "L1", "e")
    # Brackets naming the circuits.
    f.add('<path class="thin" d="M615,100 L625,100 L625,180 L615,180"/>')
    f.text(635, 136, "circuit North", "")
    f.text(635, 152, "2 conductor paths", "lab")
    f.add('<path class="thin" d="M615,272 L625,272 L625,292 L615,292"/>')
    f.text(635, 280, "circuit Branch", "")
    f.text(635, 296, "1 conductor path", "lab")
    f.label(140, 110, 250, 74, "one conductor path (L1): section, jumper, section", "", end=False)
    f.label(330, 282, 230, 316, "the same phase L1, not connected: another circuit", "", end=False)
    f.text(20, 380, "A conductor path is in one circuit; a circuit has one path per phase. "
           "Circuits are not merged:", "lab")
    f.text(20, 396, "a jumper joining the two L1 paths would make them one path, "
           "and one circuit (its name chosen then).", "lab")
    return f


# 5. Route, alignments, alignment points (plan) ------------------------------------------
def route_plan():
    f = Fig("route-plan", 760, 360, "Route, alignment and alignment points, in plan",
            "Plan view: a route made of two alignments meeting at a junction; each alignment "
            "runs through alignment points, and structures stand along it, some tied to an "
            "alignment point.")
    f.text(20, 26, "A route in plan: alignments through alignment points", "t")
    a1 = [(40, 260), (230, 210), (430, 230), (620, 120)]
    a2 = [(430, 230), (560, 310), (730, 320)]
    for pts, cls in ((a1, "st"), (a2, "st")):
        d = "M" + " L".join(f"{x},{y}" for x, y in pts)
        f.add(f'<path class="{cls}" d="{d}" style="stroke-width:2"/>')
    for x, y in a1 + a2[1:]:
        f.add(f'<rect class="ap" x="{x-5}" y="{y-5}" width="10" height="10"/>')
    # structures along the alignments (dots), some on the alignment points
    def along(pts, step):
        out = []
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            n = max(1, int(math.hypot(x1 - x0, y1 - y0) // step))
            for i in range(1, n):
                out.append((x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n))
        return out
    for x, y in along(a1, 70) + along(a2, 70) + [a1[1], a1[2]]:
        f.add(f'<circle class="dot" cx="{x:.1f}" cy="{y:.1f}" r="4"/>')
    f.label(230, 210, 240, 150, "alignment point (a structure tied to it)")
    f.label(620, 120, 600, 70, "alignment point")
    f.label(330, 220, 330, 290, "structure on the alignment")
    f.label(430, 230, 570, 250, "junction (on both alignments)")
    f.text(90, 280, "alignment 1", "lab")
    f.text(640, 296, "alignment 2", "lab")
    f.text(20, 50, "route: both alignments together (the path of the line)", "lab")
    f.add('<rect class="ap" x="20" y="331" width="10" height="10"/>')
    f.text(36, 340, "alignment point", "lab")
    f.add('<circle class="dot" cx="160" cy="336" r="4"/>')
    f.text(170, 340, "structure", "lab")
    return f


# 6. Sag, clearances, swing band ------------------------------------------------------
def clearances():
    f = Fig("clearances", 900, 360, "Sag, clearances and the swing band",
            "Left: a span from the side, with the sag below the chord and the vertical "
            "clearance from the lowest point of the wire to the ground. Right: the same span "
            "cut across, with the wire swung by the wind inside its swing band and the radial "
            "clearance to an object beside the line.")
    f.text(20, 26, "Sag and clearances (left: side view; right: across the span)", "t")
    g = 320
    f.add(f'<path class="dash" d="M20,{g} L480,{g}"/>')
    f.add(f'<path class="st" d="M50,{g} L50,80 M450,{g} L450,80"/>')
    f.add('<path class="thin" d="M50,90 L450,90"/>')
    f.add(f'<path class="wire" d="{sag_path(50, 90, 450, 90, 120)}"/>')
    f.vdim(250, 90, 210, "sag")
    f.add(f'<path class="thin" stroke-dasharray="3 3" d="M250,210 L250,{g}"/>')
    f.vdim(300, 210, g, "vertical clearance")
    f.text(60, 82, "chord", "lab")
    # the span cut across: the wire swings about the chord's point inside its band
    cx, cy, r0 = 650, 90, 125            # the point it swings about, the swing radius
    f.add(f'<path class="dash" d="M500,{g} L890,{g}"/>')

    def at(deg, r):
        a = math.radians(deg)
        return cx + r * math.sin(a), cy + r * math.cos(a)

    (ax, ay), (bx, by) = at(-45, r0 + 14), at(45, r0 + 14)
    (cx2, cy2), (dx, dy) = at(45, r0 - 14), at(-45, r0 - 14)
    f.add(f'<path class="band" d="M{ax:.1f},{ay:.1f} A{r0+14},{r0+14} 0 0 0 {bx:.1f},{by:.1f} '
          f'L{cx2:.1f},{cy2:.1f} A{r0-14},{r0-14} 0 0 1 {dx:.1f},{dy:.1f} Z"/>')
    sx, sy = at(0, r0)
    wx, wy = at(35, r0)
    f.add(f'<path class="thin" d="M{cx},{cy} L{sx:.1f},{sy:.1f} M{cx},{cy} L{wx:.1f},{wy:.1f}"/>')
    f.add(f'<circle cx="{sx:.1f}" cy="{sy:.1f}" r="6" class="wire" style="stroke-width:3"/>')
    f.add(f'<circle cx="{wx:.1f}" cy="{wy:.1f}" r="6" class="wire" style="stroke-width:3"/>')
    ap(f, cx, cy, 4)
    f.add(f'<rect class="box" x="830" y="130" width="40" height="{g-130}"/>')
    f.text(850, 122, "object", "c lab")
    f.add(f'<path class="thin" d="M{wx+7:.1f},{wy:.1f} L830,{wy:.1f} '
          f'M830,{wy-5:.1f} L830,{wy+5:.1f}"/>')
    f.text((wx + 830) / 2, wy + 22, "radial clearance", "c")
    f.label(sx, sy, 600, 262, "wire, still air")
    f.label(wx, wy, 720, 292, "wire in the wind", end=True)
    mx, my = at(-40, r0)
    f.label(mx, my, 540, 130, "swing band")
    f.label(cx, cy, 600, 64, "point the wire swings about", end=True)
    return f


# 7. Weather case vs load case --------------------------------------------------------
def cases():
    f = Fig("cases", 820, 300, "Weather case and load case",
            "A weather case (temperature, wind, ice) gives the wires' shape and tension in "
            "OHL-Design. A load case takes a weather case with factors and gives the loads "
            "sent to the structures; OHL-Structure's structure load cases combine them for the "
            "analysis.")
    f.text(20, 26, "Weather case, load case, structure load case", "t")

    def box(x, y, w, h, head, lines):
        f.add(f'<rect class="box" x="{x}" y="{y}" width="{w}" height="{h}" rx="6"/>')
        f.text(x + 12, y + 22, head, "t")
        for i, s in enumerate(lines):
            f.text(x + 12, y + 44 + 17 * i, s, "lab")

    box(20, 60, 230, 120, "weather case (.ohlwc)",
        ["temperature", "wind pressure", "ice", "→ the wires' sag and tension"])
    box(300, 60, 230, 120, "load case (.ohllc)",
        ["a weather case", "+ load factors", "+ the structures' own wind", "→ loads on structures"])
    box(580, 60, 220, 120, "structure load case",
        ["(.ohlslc, OHL-Structure)", "load cases from the line", "+ the structure's own loads",
         "→ member forces"])
    f.add('<path class="ldr" d="M250,120 L300,120 M292,115 L300,120 L292,125"/>')
    f.add('<path class="ldr" d="M530,120 L580,120 M572,115 L580,120 L572,125"/>')
    f.text(20, 220, "Wire condition (not a case): initial, after creep, after load: "
           "the state of the wire the weather case is solved in.", "lab")
    f.text(20, 245, "A weather case alone may also be sent to the structures "
           "(every factor 1).", "lab")
    return f


# 8. Extension, leg extension, height adjustment, detail levels --------------------------
def extensions():
    f = Fig("extensions", 860, 380, "Extensions and detail levels",
            "Left: a structure type's body on an extension, one leg with a leg extension, "
            "and the height adjustment of its feet above the ground. Right: the three detail "
            "levels of a structure type: a stick figure, sticks in 3D, and the full design.")
    f.text(20, 26, "Extension, leg extension, height adjustment; detail levels", "t")
    g = 350
    f.add(f'<path class="dash" d="M20,{g} L330,{g}"/>')
    f.add('<path class="st" d="M140,80 L120,230 L200,230 L180,80 Z M110,110 L210,110"/>')
    f.add('<path class="st" stroke-dasharray="6 3" d="M120,230 L105,300 M200,230 L215,300 '
          'M105,300 L215,300"/>')
    f.add('<path class="st" d="M215,300 L222,335"/>')
    f.add(f'<path class="thin" d="M60,300 L60,{g} M55,300 L103,300 M55,{g} L65,{g}"/>')
    f.label(160, 160, 230, 150, "body of the structure type")
    f.label(113, 265, 100, 250, "extension")
    f.label(219, 318, 260, 300, "leg extension")
    f.text(66, 324, "height", "lab")
    f.text(66, 338, "adjustment", "lab")
    # detail levels
    x0 = 380
    for i, (title, sub) in enumerate((("level 1", "stick figure"),
                                      ("level 2", "sticks of the structure"),
                                      ("level 3", "full design"))):
        x = x0 + 165 * i
        f.text(x + 60, 330, title, "c t")
        f.text(x + 60, 348, sub, "c lab")
        if i == 0:
            f.add(f'<path class="st" d="M{x+60},300 L{x+60},80 M{x+20},120 L{x+100},120 '
                  f'M{x+30},170 L{x+90},170"/>')
        else:
            w = 3 if i == 2 else 1.6
            f.add(f'<path class="st" style="stroke-width:{w}" d="M{x+30},300 L{x+50},90 '
                  f'L{x+70},90 L{x+90},300 M{x+15},120 L{x+105},120 M{x+20},170 L{x+100},170"/>')
            braces = "".join(f"M{x+30+ (20*k/6):.1f},{300-210*k/6:.1f} L{x+90-(20*(k+1)/6):.1f},"
                             f"{300-210*(k+1)/6:.1f} " for k in range(6))
            f.add(f'<path class="st" style="stroke-width:{w*0.6}" d="{braces}"/>')
            if i == 2:
                for k in range(7):
                    y = 300 - 210 * k / 6
                    xa, xb = x + 30 + 20 * k / 6, x + 90 - 20 * k / 6
                    f.add(f'<circle class="dot" cx="{xa:.1f}" cy="{y:.1f}" r="2.5"/>'
                          f'<circle class="dot" cx="{xb:.1f}" cy="{y:.1f}" r="2.5"/>')
    f.text(x0 + 225, 375, "joints and members with cross-sections, supported (level 3)",
           "c lab")
    return f


# 9. Span points and their objects ----------------------------------------------------
def span_objects():
    f = Fig("span-objects", 860, 360, "Span points and span objects",
            "A span from the side with labelled span points: one holding a marker ball, one "
            "holding a spacer's clamp joining it to the wire of another section below, and "
            "one holding the clamp of a tap, a jumper ending on the span.")
    f.text(20, 26, "Span points and what they hold (side view of one span)", "t")
    f.add('<path class="st" d="M60,60 L60,300 M800,60 L800,300"/>')
    f.add('<path class="dash" d="M20,300 L840,300"/>')
    upper = sag_path(60, 90, 800, 90, 90)
    lower = sag_path(60, 150, 800, 150, 90)
    f.add(f'<path class="wire" d="{upper}"/><path class="wire2" d="{lower}"/>')

    def on(t, y0):
        return 60 + 740 * t, y0 + 4 * 90 * t * (1 - t)

    bx, by = on(0.25, 90)
    f.add(f'<circle class="ap" cx="{bx:.1f}" cy="{by+12:.1f}" r="11" style="fill:var(--wire)"/>')
    f.label(bx, by + 12, 170, 230, "marker ball (a body)")
    sx, sy = on(0.5, 90)
    sx2, sy2 = on(0.5, 150)
    f.add(f'<path class="str" d="M{sx:.1f},{sy:.1f} L{sx2:.1f},{sy2:.1f}"/>')
    for x, y in ((sx, sy), (sx2, sy2)):
        f.add(f'<rect class="ap" x="{x-4:.1f}" y="{y-4:.1f}" width="8" height="8"/>')
    f.label(sx, (sy + sy2) / 2, 520, 250, "spacer (joins span points of two sections)")
    tx, ty = on(0.78, 90)
    f.add(f'<rect class="ap" x="{tx-4:.1f}" y="{ty-4:.1f}" width="8" height="8"/>')
    f.add(f'<path class="wire" stroke-dasharray="5 3" d="M{tx:.1f},{ty:.1f} '
          f'Q{tx+40:.1f},{ty+60:.1f} {tx+60:.1f},{ty+130:.1f}"/>')
    f.label(tx + 45, ty + 85, 760, 325, "tap: a jumper ending on the span", end=True)
    f.label(tx, ty, 640, 70, "span point holding a clamp")
    f.label(bx, by, 300, 60, "span point holding a body")
    f.text(20, 350, "■ clamp (a hinge: forces only)    ● body (weight, wind, ice)", "lab")
    return f


# 10. Joints and the rigid link ------------------------------------------------------
def rigid_link():
    f = Fig("rigid-link", 860, 400, "Joints, attachment points and the rigid link",
            "Left: a structure's joints, an attach joint at the crossarm's tip and support "
            "joints at its feet. Right, enlarged: an attachment point at an offset e from its "
            "attach joint, joined to it by a rigid link; the string's force F at the point "
            "reaches the joint as the same force and the moment M = F times e.")
    f.text(20, 26, "Joints, attachment points and the rigid link", "t")
    # Left: the structure and its joints.
    f.add('<path class="st" d="M120,360 L160,90 L200,360 M140,360 L180,360"/>')
    f.add('<path class="st" d="M160,90 L290,110 M160,140 L290,110"/>')
    for x, y in ((160, 90), (160, 140), (148, 220), (172, 220)):
        f.add(f'<circle class="dot" cx="{x}" cy="{y}" r="3"/>')
    # Attach joint: ringed; support joints: triangles.
    f.add('<circle class="dot" cx="290" cy="110" r="3.5"/>')
    f.add('<circle cx="290" cy="110" r="9" style="fill:none;stroke:var(--wire);stroke-width:2"/>')
    for x in (120, 200):
        f.add(f'<path class="st" style="stroke-width:1.8" d="M{x},360 L{x-9},376 L{x+9},376 Z"/>')
    string(f, 290, 110, 290, 175)
    wp(f, 290, 175)
    f.label(286, 102, 250, 55, "attach joint (strings hang here)", end=False)
    f.label(200, 368, 260, 385, "support joint (the structure stands here)")
    f.label(160, 140, 60, 140, "joint", end=True)
    f.label(290, 150, 330, 160, "string")
    # The enlarged box: the crossarm's tip, its point at an offset.
    f.add('<rect class="box" x="470" y="50" width="370" height="300" rx="6"/>')
    f.text(480, 70, "enlarged: a point at an offset from its joint", "lab")
    jx, jy = 560, 150
    px, py = 700, 175
    f.add(f'<path class="st" d="M480,{jy-30} L{jx},{jy} M480,{jy+40} L{jx},{jy}"/>')
    f.add(f'<circle class="dot" cx="{jx}" cy="{jy}" r="4"/>')
    f.add(f'<circle cx="{jx}" cy="{jy}" r="11" style="fill:none;stroke:var(--wire);stroke-width:2"/>')
    # The rigid link: a solid rod from the joint to the point.
    f.add(f'<path style="stroke:var(--wire3);stroke-width:6;stroke-linecap:round" '
          f'd="M{jx},{jy} L{px},{py}"/>')
    ap(f, px, py, 5.5)
    string(f, px, py, px, py + 90)
    wp(f, px, py + 90)
    # The force at the point.
    f.add(f'<path class="wire" style="stroke-width:2.6" d="M{px+28},{py+20} L{px+28},{py+70} '
          f'M{px+22},{py+60} L{px+28},{py+72} L{px+34},{py+60}"/>')
    f.text(px + 36, py + 52, "F (the string's force)")
    # The offset e and the moment at the joint.
    f.dim(jx, px, 300, "e (offset)")
    f.add(f'<path class="dash" d="M{jx},{jy+15} L{jx},293 M{px},{py+95} L{px},293"/>')
    # M: a quarter-and-more arc around the joint, clockwise, its arrowhead.
    r = 26
    a0, a1 = math.radians(200), math.radians(-30)
    x0, y0 = jx + r * math.cos(a0), jy - r * math.sin(a0)
    x1, y1 = jx + r * math.cos(a1), jy - r * math.sin(a1)
    f.add(f'<path class="wire" style="stroke-width:2" d="M{x0:.1f},{y0:.1f} '
          f'A{r},{r} 0 1 1 {x1:.1f},{y1:.1f}"/>')
    f.add(f'<path class="wire" style="stroke-width:2" d="M{x1-9:.1f},{y1-1:.1f} '
          f'L{x1:.1f},{y1:.1f} L{x1-2:.1f},{y1-9:.1f}"/>')
    f.text(jx - 40, jy - 40, "M = F · e at the joint", "")
    f.label(jx - 4, jy + 10, 515, 250, "attach joint", end=True)
    f.label((jx + px) / 2, (jy + py) / 2, 650, 110, "rigid link")
    f.label(px, py, 712, 138, "attachment point", end=False)
    f.text(480, 325, "The point keeps its place. The joint carries F and M = F · e", "lab")
    f.text(480, 341, "where a frame member meets it (a truss-only joint cannot).", "lab")
    return f


FIGURES = [structure_parts, string_combinations, section, systems, circuits, route_plan, clearances,
           cases, extensions, span_objects, rigid_link]


def main(argv):
    out = "docs/images/glossary"
    theme = None
    args = list(argv)
    fit = None
    if "--fit" in args:
        i = args.index("--fit")
        fit = int(args[i + 1])
        del args[i:i + 2]
    if "--theme" in args:
        i = args.index("--theme")
        theme = args[i + 1]
        del args[i:i + 2]
    if args:
        out = args[0]
    os.makedirs(out, exist_ok=True)
    for make in FIGURES:
        fig = make()
        with open(os.path.join(out, fig.name + ".svg"), "w", encoding="utf-8") as fh:
            fh.write(fig.svg(theme, fit))
    print(f"{len(FIGURES)} drawings written to {out}")


if __name__ == "__main__":
    main(sys.argv[1:])
