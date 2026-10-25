#!/usr/bin/env python3
"""Write the theory manual's drawings (docs/images/theory/*.svg).

Standard library only; run from the repository root with

    python3 -I tools/theory_figures.py            # writes docs/images/theory/
    python3 -I tools/theory_figures.py OUT --theme light|dark [--fit 760]
                                                   # copies with one palette forced and
                                                   # at most 760 px wide (to look at both,
                                                   # e.g. as PNG with `qlmanage -t`)

The drawings are invented sketches (no real line), schematic and not to scale
unless a page's own numbers are drawn. Palette, style and helpers are the
glossary's (tools/glossary_figures.py): colours as CSS custom properties
switched by `prefers-color-scheme`, every drawing paints its own background,
system fonts only, nothing external loaded. Labels use the theory pages' own
terms and symbols: after a change of a page, change them here and run the
script again.
"""

import math
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import glossary_figures as gf  # noqa: E402  (the glossary's palette, style and helpers)

EXTRA = (
    "<style>"
    ".fw{fill:var(--wire);fill-opacity:.16;stroke:none}"
    ".fa{fill:var(--acc);fill-opacity:.14;stroke:none}"
    ".f2{fill:var(--wire2);fill-opacity:.16;stroke:none}"
    ".f3{fill:var(--wire3);fill-opacity:.16;stroke:none}"
    ".fi{fill:var(--ink);fill-opacity:.10;stroke:none}"
    ".gnd{stroke:var(--muted);stroke-width:1.6;fill:none}"
    ".acc{stroke:var(--acc);stroke-width:1.8;fill:none}"
    ".w2l{stroke:var(--wire2);stroke-width:1.8;fill:none}"
    ".w3l{stroke:var(--wire3);stroke-width:1.8;fill:none}"
    ".inkl{stroke:var(--ink);stroke-width:1.4;fill:none}"
    ".ds{stroke-dasharray:5 4}"
    ".dt{stroke-dasharray:2 3}"
    ".i{font-style:italic}"
    ".sm{font-size:11px}"
    "</style>"
)

STROKE = {"ink": "--ink", "muted": "--muted", "acc": "--acc", "wire": "--wire",
          "wire2": "--wire2", "wire3": "--wire3"}


def rich(s):
    """Plain text with `_{sub}` and `^{sup}` made into tspans; &, <, > escaped."""
    s = s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    out, i, shift = [], 0, 0
    while i < len(s):
        if s[i] in "_^" and i + 1 < len(s) and s[i + 1] == "{":
            j = s.index("}", i)
            dy = 4 if s[i] == "_" else -6
            out.append(f'<tspan dy="{dy - shift}" style="font-size:78%">{s[i + 2:j]}</tspan>')
            shift = dy
            i = j + 1
            continue
        k = i
        while k < len(s) and not (s[k] in "_^" and k + 1 < len(s) and s[k + 1] == "{"):
            k += 1
        chunk = s[i:k]
        if shift:
            out.append(f'<tspan dx="1.5" dy="{-shift}">{chunk}</tspan>')
            shift = 0
        else:
            out.append(chunk)
        i = k
    return "".join(out)


class Fig(gf.Fig):
    def text(self, x, y, s, cls=""):
        c = f' class="{cls}"' if cls else ""
        self.add(f'<text x="{x:.1f}" y="{y:.1f}"{c}>{rich(s)}</text>')

    def svg(self, theme, fit=None):
        return super().svg(theme, fit).replace("</style>", "</style>" + EXTRA, 1)

    def path(self, d, cls="", style=""):
        c = f' class="{cls}"' if cls else ""
        st = f' style="{style}"' if style else ""
        self.add(f'<path{c}{st} d="{d}"/>')

    def poly(self, pts, cls="", style="", close=False):
        d = "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in pts) + (" Z" if close else "")
        self.path(d, cls, style)

    def arrow(self, x0, y0, x1, y1, col="ink", w=1.6, head=8.0, both=False, dash=False):
        """A straight arrow from (x0, y0) to (x1, y1), its head(s) filled."""
        a = math.atan2(y1 - y0, x1 - x0)
        ca, sa = math.cos(a), math.sin(a)
        sx, sy = (x0 + head * 0.7 * ca, y0 + head * 0.7 * sa) if both else (x0, y0)
        ex, ey = x1 - head * 0.7 * ca, y1 - head * 0.7 * sa
        ds = ";stroke-dasharray:5 4" if dash else ""
        self.add(f'<path style="stroke:var({STROKE[col]});stroke-width:{w};fill:none{ds}" '
                 f'd="M{sx:.1f},{sy:.1f} L{ex:.1f},{ey:.1f}"/>')
        heads = [(x1, y1, a)] + ([(x0, y0, a + math.pi)] if both else [])
        for hx, hy, b in heads:
            cb, sb = math.cos(b), math.sin(b)
            p1 = (hx - head * cb + head * 0.42 * sb, hy - head * sb - head * 0.42 * cb)
            p2 = (hx - head * cb - head * 0.42 * sb, hy - head * sb + head * 0.42 * cb)
            self.add(f'<path style="fill:var({STROKE[col]});stroke:none" d="M{hx:.1f},{hy:.1f} '
                     f'L{p1[0]:.1f},{p1[1]:.1f} L{p2[0]:.1f},{p2[1]:.1f} Z"/>')

    def dot(self, x, y, r=3.5, col="ink"):
        self.add(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r}" style="fill:var({STROKE[col]});stroke:none"/>')

    def ring(self, x, y, r=5, col="wire", w=2.2):
        self.add(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r}" '
                 f'style="fill:var(--bg);stroke:var({STROKE[col]});stroke-width:{w}"/>')

    def arc(self, cx, cy, r, a0, a1, cls="thin", style="", n=40):
        """An arc about (cx, cy), angles in degrees as on screen (0 = +x, clockwise)."""
        self.poly(arc_pts(cx, cy, r, a0, a1, n), cls, style)

    def vdim2(self, x, y0, y1, s, left=False, cls=""):
        """A vertical dimension with arrowheads at both ends and its text beside it."""
        self.arrow(x, y0, x, y1, "muted", 1.1, 7, both=True)
        self.text(x + (-7 if left else 7), (y0 + y1) / 2 + 4, s, ("e " + cls if left else cls).strip())

    def hdim2(self, x0, x1, y, s, above=True, cls=""):
        self.arrow(x0, y, x1, y, "muted", 1.1, 7, both=True)
        self.text((x0 + x1) / 2, y - 6 if above else y + 15, s, ("c " + cls).strip())


def arc_pts(cx, cy, r, a0, a1, n=40):
    return [(cx + r * math.cos(math.radians(a0 + (a1 - a0) * k / n)),
             cy + r * math.sin(math.radians(a0 + (a1 - a0) * k / n))) for k in range(n + 1)]


def pol(cx, cy, rho, phi):
    """A point at radius rho and angle phi (degrees) from the downward vertical, positive to
    the right, about (cx, cy) on screen: the swing angle convention of the theory pages."""
    p = math.radians(phi)
    return cx + rho * math.sin(p), cy + rho * math.cos(p)


def band_pts(cx, cy, r, R, pl, pr, n=48):
    """The outline of the annular sector from radius r to R between angles pl and pr."""
    outer = [pol(cx, cy, R, pl + (pr - pl) * k / n) for k in range(n + 1)]
    inner = [pol(cx, cy, r, pr - (pr - pl) * k / n) for k in range(n + 1)]
    return outer + inner


def parab(x0, y0, x1, y1, sag, n=40):
    return [(x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n + 4 * sag * (k / n) * (1 - k / n))
            for k in range(n + 1)]


def catmull(pts, n=20):
    """A Catmull-Rom curve through the points (the end tangents along the first and last
    chords), `n` samples a piece."""
    ext = [pts[0]] + list(pts) + [pts[-1]]
    out = [pts[0]]
    for i in range(1, len(ext) - 2):
        p0, p1, p2, p3 = ext[i - 1], ext[i], ext[i + 1], ext[i + 2]
        for k in range(1, n + 1):
            t = k / n
            out.append(tuple(0.5 * (2 * p1[j] + (-p0[j] + p2[j]) * t
                                    + (2 * p0[j] - 5 * p1[j] + 4 * p2[j] - p3[j]) * t * t
                                    + (-p0[j] + 3 * p1[j] - 3 * p2[j] + p3[j]) * t ** 3)
                             for j in (0, 1)))
    return out


def resample(pts, n):
    """`n` + 1 points at equal arc lengths along a polyline."""
    d = [0.0]
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        d.append(d[-1] + math.hypot(x1 - x0, y1 - y0))
    out, j = [], 0
    for k in range(n + 1):
        s = d[-1] * k / n
        while j < len(d) - 2 and d[j + 1] < s:
            j += 1
        t = (s - d[j]) / max(d[j + 1] - d[j], 1e-12)
        out.append((pts[j][0] + t * (pts[j + 1][0] - pts[j][0]), pts[j][1] + t * (pts[j + 1][1] - pts[j][1])))
    return out


def ground(f, x0, x1, y, cls="gnd"):
    f.path(f"M{x0},{y} L{x1},{y}", cls)


# =====================================================================================
# Sag-tension
# =====================================================================================
def catenary_span():
    f = Fig("catenary-span", 760, 430, "One span: the catenary and its quantities",
            "An inclined span from the left support at (0, 0) to the right one at (S, h): the "
            "chord, the low point at x_m, the point x* where the wire is parallel to the chord "
            "and the sag there, the horizontal tension H at the low point and the support "
            "tensions T_A and T_B along the wire.")
    f.text(20, 26, "One span: the catenary and its quantities", "t")
    S, h, C = 560.0, 110.0, 420.0
    x0, y0 = 90.0, 300.0
    a = S / (2 * C)
    xm = S / 2 - C * math.asinh(h / (2 * C * math.sinh(a)))

    def y(x):
        return 2 * C * math.sinh((x - 2 * xm) / (2 * C)) * math.sinh(x / (2 * C))

    def P(x):
        return x0 + x, y0 - y(x)

    g = 385
    ground(f, 40, 720, g)
    f.path(f"M{x0},{g} L{x0},{y0} M{x0 + S},{g} L{x0 + S},{y0 - h}", "st")
    f.path(f"M{x0},{y0} L{x0 + S},{y0 - h}", "thin ds")
    f.poly([P(S * k / 80) for k in range(81)], "wire")
    f.add(f'<circle class="ap" cx="{x0}" cy="{y0}" r="4.5"/>')
    f.add(f'<circle class="ap" cx="{x0 + S}" cy="{y0 - h}" r="4.5"/>')
    f.text(x0 - 10, y0 + 18, "(0, 0)", "e")
    f.text(x0 + S - 12, y0 - h - 12, "(S, h)", "e")
    # low point and H
    lx, ly = P(xm)
    f.dot(lx, ly, 4)
    f.arrow(lx, ly, lx - 62, ly, "acc", 2)
    f.arrow(lx, ly, lx + 62, ly, "acc", 2)
    f.text(lx + 68, ly + 4, "H (horizontal tension)")
    f.label(lx, ly, lx - 50, ly - 30, "low point", "lab", end=True)
    # x_m and S
    f.path(f"M{lx},{ly + 6} L{lx},{g - 8}", "thin dt")
    f.hdim2(x0, lx, g - 14, "x_{m}")
    f.hdim2(x0, x0 + S, g + 26, "S")
    # x*: the wire parallel to the chord, the sag there
    xs = xm + C * math.asinh(h / S)
    sx, sy = P(xs)
    cy_ = y0 - h * xs / S
    f.path(f"M{sx},{cy_} L{sx},74", "thin dt")
    f.path(f"M{x0},{y0 - 4} L{x0},74", "thin dt")
    f.hdim2(x0, sx, 80, "x*  (wire parallel to the chord)")
    f.vdim2(sx, cy_, sy, "sag")
    tx = 40 * math.cos(math.atan(h / S))
    f.path(f"M{sx - tx},{sy + 40 * h / S * math.cos(math.atan(h / S))} "
           f"L{sx + tx},{sy - 40 * h / S * math.cos(math.atan(h / S))}", "thin ds")
    # h
    f.path(f"M{x0 + S + 6},{y0} L{x0 + S + 50},{y0}", "thin dt")
    f.vdim2(x0 + S + 40, y0, y0 - h, "h")
    # chord angle
    f.arc(x0, y0, 80, -math.degrees(math.atan(h / S)), 0, "thin")
    f.path(f"M{x0},{y0} L{x0 + 95},{y0}", "thin dt")
    f.text(x0 + 86, y0 - 5, "θ")
    f.text(x0 + 150, y0 - 52, "chord", "lab")
    # support tensions along the wire
    for x, name, sgn in ((0.0, "T_{A}", -1), (S, "T_{B}", 1)):
        sl = math.sinh((x - xm) / C)
        n = math.hypot(1, sl)
        ux, uy = sgn / n, -sgn * sl / n
        px, py = P(x)
        f.arrow(px, py, px + 58 * ux, py + 58 * uy, "acc", 2)
        f.text(px + 64 * ux + (6 if sgn > 0 else -6), py + 58 * uy + 4, name, "" if sgn > 0 else "e")
    # the load per unit wire length
    for t in (0.74, 0.85):
        px, py = P(S * t)
        f.arrow(px, py + 4, px, py + 30, "wire", 1.4, 6)
    px, py = P(S * 0.74)
    f.text(px + 8, py + 30, "w per unit wire length", "lab")
    f.text(20, 424, "C = H/w;  T(x) = H cosh((x − x_{m})/C);  T_{B} − T_{A} = w h", "lab")
    return f


def weather_loads():
    f = Fig("weather-loads", 760, 350, "Ice, wind and the resultant load",
            "Left: the conductor of diameter d with radial ice of thickness t, the wind "
            "pressure q on the iced diameter. Right: the vertical load of the conductor and the "
            "ice, the horizontal wind load, their resultant and the angle the plane swings by.")
    f.text(20, 26, "Weather loads on a wire: ice, wind, the resultant", "t")
    cx, cy = 200, 175
    rd, rt = 38, 64
    f.add(f'<circle cx="{cx}" cy="{cy}" r="{rt}" class="fa" style="stroke:var(--acc);stroke-width:1.4"/>')
    f.add(f'<circle cx="{cx}" cy="{cy}" r="{rd}" style="fill:var(--fill);stroke:var(--wire);stroke-width:2.5"/>')
    f.hdim2(cx - rd, cx + rd, cy + 4, "d")
    f.path(f"M{cx - rt},{cy - rt - 20} L{cx - rt},{cy - 4} M{cx + rt},{cy - rt - 20} L{cx + rt},{cy - 4}",
           "thin dt")
    f.hdim2(cx - rt, cx + rt, cy - rt - 12, "d + 2t")
    f.vdim2(cx + 92, cy + rd, cy + rt, "t", cls="")
    f.path(f"M{cx},{cy + rd} L{cx + 96},{cy + rd} M{cx},{cy + rt} L{cx + 96},{cy + rt}", "thin dt")
    f.label(cx - 40, cy + 48, cx - 30, cy + 112, "radial ice, density ρ_{i}", end=False)
    for k in range(5):
        yy = cy - 48 + 24 * k
        f.arrow(cx - 150, yy, cx - rt - 14, yy, "acc", 1.6, 7)
    f.text(cx - 150, cy + 76, "wind pressure q")
    f.text(20, 336, "w_{i} = ρ_{i} g (π/4)[(d + 2t)² − d²];   w_{h} = q (d + 2t)", "lab")
    # the vector triangle
    ox, oy = 470, 70
    wv, wh = 190, 120
    f.arrow(ox, oy, ox, oy + wv, "wire", 2.2, 9)
    f.arrow(ox, oy + wv, ox + wh, oy + wv, "acc", 2.2, 9)
    f.arrow(ox, oy, ox + wh, oy + wv, "ink", 2.4, 10)
    f.text(ox - 10, oy + wv / 2, "w_{v} = w_{c} + w_{i}", "e")
    f.text(ox - 10, oy + wv / 2 + 17, "(conductor and ice)", "e lab")
    f.text(ox + wh / 2, oy + wv + 22, "w_{h}", "c")
    f.text(ox + wh / 2 + 22, oy + wv / 2 - 4, "√((w_{c} + w_{i})² + w_{h}²)")
    a = math.degrees(math.atan2(wv, wh))
    f.arc(ox, oy, 52, 90, a, "thin")
    f.text(ox - 10, oy + 40, "arctan(w_{h}/w_{v})", "e lab")
    f.text(ox + wh / 2, oy + wv + 46, "the resultant's plane: swung by arctan(w_{h}/w_{v})", "c lab")
    return f


def transit():
    f = Fig("transit", 760, 340, "Sagging by transit",
            "A level span seen from the side: the instrument a below one support, the line of "
            "sight tangent to the wire at x_t, reaching the far support's vertical b below "
            "that support; the sag D at mid-span.")
    f.text(20, 26, "Transit (angle of sight), a level span", "t")
    x0, x1, yt, D = 110.0, 650.0, 80.0, 120.0
    a, b = 0.64 * D, 1.44 * D
    S = x1 - x0
    g = 320
    ground(f, 40, 720, g)
    f.path(f"M{x0},{g} L{x0},{yt} M{x1},{g} L{x1},{yt}", "st")
    f.path(f"M{x0},{yt} L{x1},{yt}", "thin ds")
    f.poly(parab(x0, yt, x1, yt, D, 60), "wire")
    f.vdim2((x0 + x1) / 2, yt, yt + D, "D (sag)")
    xt = x0 + S * math.sqrt(a / (4 * D))
    ytt = yt + 4 * D * ((xt - x0) / S) * (1 - (xt - x0) / S)
    f.path(f"M{x0},{yt + a} L{x1 + 30},{yt + a + (b - a) * (S + 30) / S}", "acc")
    f.add(f'<rect x="{x0 - 7}" y="{yt + a - 7}" width="14" height="14" '
          f'style="fill:var(--bg);stroke:var(--acc);stroke-width:2"/>')
    f.dot(xt, ytt, 4, "acc")
    f.add(f'<path style="fill:var(--acc);stroke:none" d="M{x1},{yt + b - 8} L{x1 + 8},{yt + b} '
          f'L{x1},{yt + b + 8} L{x1 - 8},{yt + b} Z"/>')
    f.vdim2(x0 - 26, yt, yt + a, "a", left=True)
    f.vdim2(x1 + 52, yt, yt + b, "b")
    f.path(f"M{x1 + 6},{yt + b} L{x1 + 58},{yt + b}", "thin dt")
    f.label(x0 + 8, yt + a + 4, x0 + 40, yt + a + 50, "instrument (x_{i}, z_{i})")
    f.label(xt, ytt, xt + 50, ytt + 50, "tangent point x_{t}")
    f.label(x1 - 4, yt + b + 4, x1 - 60, yt + b + 44, "target", end=True)
    f.path(f"M{xt},{ytt} L{xt},{g - 8}", "thin dt")
    f.hdim2(x0, xt, g - 14, "x_{t} = S √(a/4D)")
    f.text(20, 50, "Level span, parabola: √a + √b = 2√D; the angle of sight is arctan y′(x_{t}).", "lab")
    return f


# =====================================================================================
# Finite-element wire systems
# =====================================================================================
def catenary_element():
    f = Fig("catenary-element", 760, 340, "The elastic catenary element",
            "One span as one element from node i to node j, in the plane of the chord and the "
            "load q: the tension at node i has the components H and V_0 along the wire, at node "
            "j H and V_1 = V_0 + W, with W = w L_0 the load of the unstressed length L_0.")
    f.text(20, 26, "One span, one exact elastic catenary element", "t")
    xi, yi, xj, yj, sag = 140.0, 120.0, 610.0, 175.0, 120.0
    pts = parab(xi, yi, xj, yj, sag, 60)
    f.poly(pts, "wire")
    f.add(f'<circle class="ap" cx="{xi}" cy="{yi}" r="5"/><circle class="ap" cx="{xj}" cy="{yj}" r="5"/>')
    f.text(xi - 12, yi + 4, "node i", "e")
    f.text(xj + 12, yj + 4, "node j")

    # the tension's components along the wire, from node i towards node j
    def slope(t):
        return ((yj - yi) + 4 * sag * (1 - 2 * t)) / (xj - xi)
    for (x, y_, t, hn, vn) in ((xi, yi, 0, "H", "V_{0}"), (xj, yj, 1, "H", "V_{1} = V_{0} + W")):
        m = slope(t)
        L = 72
        ex, ey = x + L / math.hypot(1, m), y_ + L * m / math.hypot(1, m)
        bx, by = x, y_ - 46
        ex, ey = bx + (ex - x), by + (ey - y_)
        f.arrow(bx, by, ex, ey, "acc", 2.2)
        f.path(f"M{bx},{by} L{ex},{by} L{ex},{ey}", "thin ds")
        f.text((bx + ex) / 2, by + (16 if ey < by else -7), hn, "c")
        f.text(ex + 8, (by + ey) / 2 + 4, vn)
    f.text(xi + 90, yi - 34, "(H, V_{0}): the tension at node i,", "lab")
    f.text(xi + 90, yi - 19, "along the wire towards node j", "lab")
    f.text(xj - 14, yj - 30, "at node j: (H, V_{1})", "e lab")
    for t in (0.2, 0.4, 0.6, 0.8):
        x, y_ = pts[int(60 * t)]
        f.arrow(x, y_ + 6, x, y_ + 34, "wire", 1.4, 6)
    x, y_ = pts[48]
    f.text(x + 10, y_ + 34, "q per unit unstressed length (w = |q|)", "lab")
    f.text(260, 312, "unstressed length L_{0}, axial stiffness EA;  W = w L_{0}", "lab")
    # the plane's axes
    ox, oy = 60, 300
    f.arrow(ox, oy, ox + 50, oy, "muted", 1.3, 7)
    f.arrow(ox, oy, ox, oy - 50, "muted", 1.3, 7)
    f.text(ox + 54, oy + 4, "x", "lab")
    f.text(ox - 4, oy - 56, "z (opposite to the load)", "lab")
    return f


def fe_section():
    f = Fig("fe-section", 760, 345, "A section as finite elements",
            "Side view of a section: at its ends strain assemblies, a string from a fixed "
            "attachment point to a free clamp; in the middle suspension strings to free clamps; "
            "between the clamps one elastic catenary element per span.")
    f.text(20, 26, "A section in the finite-element model (side view)", "t")
    g = 305
    ground(f, 20, 740, g)
    xs = [70, 260, 470, 690]
    top = 100
    for x in xs:
        f.path(f"M{x},{g} L{x},{top - 20}", "st")
    clamps = [(xs[0] + 40, top + 6), (xs[1], top + 45), (xs[2], top + 45), (xs[3] - 40, top + 6)]
    gf.string(f, xs[0], top, *clamps[0])
    gf.string(f, xs[3], top, *clamps[3])
    gf.string(f, xs[1], top, *clamps[1])
    gf.string(f, xs[2], top, *clamps[2])
    for (x0, y0), (x1, y1) in zip(clamps, clamps[1:]):
        f.poly(parab(x0, y0, x1, y1, 70, 40), "wire")
    for x in xs:
        gf.ap(f, x, top)
    for x, y in clamps:
        gf.wp(f, x, y)
    f.label(xs[1], top, xs[1] + 60, 62, "fixed node: attachment point")
    f.label(xs[1], top + 25, xs[1] - 60, 70, "truss element: string", end=True)
    f.label(xs[2], top + 45, xs[2] + 30, 205, "free node: clamp")
    f.label(xs[0] + 20, top + 3, 110, 240, "strain assembly (free clamp)", end=False)
    f.label(xs[3] - 20, top + 3, 650, 240, "strain assembly", end=True)
    mx, my = parab(*clamps[1], *clamps[2], 70, 40)[20]
    f.text(mx, 128, "one elastic catenary element per span", "c lab")
    f.text(20, 335, "Free nodes found by Newton–Raphson in 3D; strings tension-only "
           "(unless they may take compression).", "lab")
    return f


def span_cases():
    f = Fig("span-cases", 760, 345, "A weather case per span",
            "Two spans either side of a suspension string, the left one iced: the string swings "
            "by the angle φ into the iced span, whose horizontal tension H_0 is larger than the "
            "other's H_1 by T sin φ at the clamp.")
    f.text(20, 26, "A weather case per span: one span iced, its string swings into it", "t")
    g = 305
    ground(f, 20, 740, g)
    xs = [60, 380, 700]
    for x in xs:
        f.path(f"M{x},{g} L{x},{82}", "st")
    L = 110
    phi = 14.0
    cx, cy = xs[1] - L * math.sin(math.radians(phi)), 90 + L * math.cos(math.radians(phi))
    f.path(f"M{xs[1]},90 L{xs[1]},{90 + L}", "thin ds")
    gf.string(f, xs[1], 90, cx, cy)
    f.poly(parab(xs[0], 100, cx, cy, 110, 40), "wire")
    f.poly(parab(cx, cy, xs[2], 100, 60, 40), "wire")
    for p in parab(xs[0], 100, cx, cy, 110, 40)[4:-3:4]:
        f.add(f'<circle cx="{p[0]:.1f}" cy="{p[1]:.1f}" r="3.2" class="fa" '
              f'style="stroke:var(--acc);stroke-width:1"/>')
    gf.ap(f, xs[1], 90)
    gf.wp(f, cx, cy)
    f.arc(xs[1], 90, 60, 90, 90 + phi, "thin")
    f.text(xs[1] - 6, 168, "φ", "e")
    f.arrow(cx, cy, cx - 70, cy, "acc", 2)
    f.arrow(cx, cy, cx + 50, cy, "acc", 2)
    f.text(cx - 76, cy + 4, "H_{0}", "e")
    f.text(cx + 56, cy + 4, "H_{1}")
    f.text(xs[1] + 10, 80, "string tension T")
    f.text(210, 62, "span 0: its own case (iced)", "c")
    f.text(560, 62, "span 1: the section's case", "c")
    f.text(380, 334, "at the clamp:  H_{0} − H_{1} = T sin φ", "c")
    return f


# =====================================================================================
# Jumpers
# =====================================================================================
def jumper_rod():
    f = Fig("jumper-rod", 760, 380, "A clamped jumper as an elastic rod",
            "Two sections end at a structure on tension strings; the jumper leaves each clamp at "
            "the angle alpha from its string's line, turned towards the load, and is solved as an "
            "inextensible rod of short straight pieces between its ends A and B; its sag is "
            "measured below the end-to-end chord.")
    f.text(20, 26, "A clamped jumper: an inextensible rod of short pieces", "t")
    sx, sy = 380.0, 92.0
    A, B = (250.0, 112.0), (510.0, 112.0)
    f.path(f"M{sx},{sy - 40} L{sx},330", "st")
    f.path(f"M{sx - 40},{sy} L{sx + 40},{sy}", "st")
    gf.string(f, sx - 30, sy, *A)
    gf.string(f, sx + 30, sy, *B)
    gf.ap(f, sx - 30, sy)
    gf.ap(f, sx + 30, sy)
    f.poly(parab(20, 150, A[0], A[1], 20, 20), "wire")
    f.poly(parab(B[0], B[1], 740, 150, 20, 20), "wire")
    al = math.radians(30)
    # the rod: a cubic leaving A and B at 30 degrees from the strings' lines, turned down
    dA = (math.cos(math.atan2(sy - A[1], sx - 30 - A[0])), math.sin(math.atan2(sy - A[1], sx - 30 - A[0])))
    angA = math.atan2(dA[1], dA[0]) + al
    angB = math.pi - angA
    k = 45
    ctrl = [A, (A[0] + k * math.cos(angA), A[1] + k * math.sin(angA)), ((A[0] + B[0]) / 2, A[1] + 92),
            (B[0] + k * math.cos(angB), B[1] + k * math.sin(angB)), B]
    pts = resample(catmull(ctrl, 30), 18)
    f.poly(catmull(ctrl, 30), "wire")
    for x, y in pts[1:-1]:
        f.dot(x, y, 2.4, "wire")
    gf.wp(f, *A)
    gf.wp(f, *B)
    f.text(A[0] - 10, A[1] - 8, "A", "e")
    f.text(B[0] + 10, B[1] - 8, "B")
    # the string's line and the exit direction at A
    L = 70
    f.arrow(A[0], A[1] - 14, A[0] + L * dA[0], A[1] - 14 + L * dA[1], "acc", 1.8, 7)
    f.label(A[0] + 40, A[1] - 22, 190, 62, "s: towards the string's top", end=True)
    ex, ey = A[0] + L * math.cos(angA), A[1] + L * math.sin(angA)
    f.arrow(A[0], A[1], ex, ey, "ink", 1.8, 7)
    f.arc(A[0], A[1], 46, math.degrees(math.atan2(dA[1], dA[0])), math.degrees(angA), "thin")
    f.label(A[0] + 44, A[1] + 6, A[0] - 4, A[1] + 48, "α (default 30°)", "lab", end=True)
    f.label(ex, ey, 290, 222, "exit direction s cos α + n sin α", end=True)
    f.arrow(110, 230, 110, 275, "wire", 1.6, 7)
    f.text(118, 262, "load q", "lab")
    f.text(20, 296, "n: the part of q square to s", "lab")
    # chord and sag
    f.path(f"M{A[0]},{A[1]} L{B[0]},{B[1]}", "thin ds")
    lo = max(pts, key=lambda p: p[1])
    f.vdim2(lo[0] + 70, A[1], lo[1], "sag below the chord")
    f.path(f"M{lo[0]},{lo[1]} L{lo[0] + 76},{lo[1]}", "thin dt")
    i = 12
    f.label((pts[i][0] + pts[i + 1][0]) / 2, (pts[i][1] + pts[i + 1][1]) / 2, 560, 300,
            "piece ℓ ≈ 0.12 m, direction d_{i}")
    f.label(pts[6][0], pts[6][1], 230, 330, "node m = A + ℓ Σ_{i<m} d_{i}", end=True)
    f.text(20, 360, "Bending c (1 − d_{i} · d_{i+1}) at every inner node, 2c (1 − d · e) at a "
           "clamped end, c = EI/ℓ; the far end held at B.", "lab")
    return f


def jumper_support():
    f = Fig("jumper-support", 760, 360, "Support clamps on a jumper",
            "Left: a jumper passing through a support clamp, one straight piece of the clamp's "
            "length, held at its middle by a string from its top. Right: across the line, a "
            "clamp on two strings in wind; the leg on the lee side goes slack.")
    f.text(20, 26, "Support clamps: a rigid piece held at its middle", "t")
    # left: side view
    A, B = (60.0, 110.0), (420.0, 110.0)
    top = (240.0, 70.0)
    cl = 34
    p = (240.0, 185.0)
    c0 = (p[0] - cl / 2, p[1] + 2)
    c1 = (p[0] + cl / 2, p[1] - 2)
    f.path(f"M{A[0]},{A[1]} C{A[0] + 60},{A[1] + 120} {c0[0] - 70},{c0[1] + 10} {c0[0]},{c0[1]}", "wire")
    f.path(f"M{c1[0]},{c1[1]} C{c1[0] + 70},{c1[1] - 6} {B[0] - 50},{B[1] + 120} {B[0]},{B[1]}", "wire")
    f.path(f"M{c0[0]},{c0[1]} L{c1[0]},{c1[1]}", "", "stroke:var(--ink);stroke-width:7;stroke-linecap:butt")
    gf.string(f, top[0], top[1], *p)
    gf.ap(f, *top)
    f.dot(*p, 3, "acc")
    gf.wp(f, *A)
    gf.wp(f, *B)
    f.text(A[0] - 8, A[1] - 8, "A", "")
    f.text(B[0] - 8, B[1] - 8, "B", "e")
    f.label(top[0], top[1], top[0] + 40, 52, "string's top (attachment point)")
    f.label(p[0] + 6, p[1] - 1, 290, 240, "p: the clamp's middle,")
    f.text(294, 256, "where the string holds")
    f.label(c0[0] + 4, c0[1], 200, 250, "clamp: one straight piece", end=True)
    f.text(196, 266, "(clamp_length, default 0.1 m)", "e lab")
    f.text(20, 330, "The wire runs straight through the clamp: it leaves in the direction it enters.",
           "lab")
    # right: across the line, wind
    ox = 600
    t1, t2 = (ox - 60, 80.0), (ox + 60, 80.0)
    f.path(f"M{t1[0] - 20},80 L{t2[0] + 20},80", "st")
    q = (ox + 40, 170.0)
    gf.string(f, *t1, *q)
    f.path(f"M{t2[0]},{t2[1]} Q{t2[0] + 18},{(t2[1] + q[1]) / 2} {q[0]},{q[1]}", "acc ds")
    gf.ap(f, *t1)
    gf.ap(f, *t2)
    f.add(f'<circle cx="{q[0]}" cy="{q[1]}" r="8" class="wire" style="stroke-width:3"/>')
    for k in range(3):
        f.arrow(ox - 140, 150 + 18 * k, ox - 90, 150 + 18 * k, "acc", 1.6, 7)
    f.text(ox - 140, 138, "wind", "lab")
    f.label(t1[0] + 30, (t1[1] + q[1]) / 2 - 10, 560, 235, "taut (windward)", end=False)
    f.label(t2[0] + 12, 120, 690, 268, "slack (lee side)", end=True)
    f.text(ox, 300, "across the line", "c lab")
    return f


# =====================================================================================
# Thermal rating
# =====================================================================================
def wave(x0, y0, x1, y1, amp=4, waves=3, n=36):
    a = math.atan2(y1 - y0, x1 - x0)
    L = math.hypot(x1 - x0, y1 - y0)
    pts = []
    for k in range(n + 1):
        s = L * k / n
        o = amp * math.sin(2 * math.pi * waves * k / n)
        pts.append((x0 + s * math.cos(a) - o * math.sin(a), y0 + s * math.sin(a) + o * math.cos(a)))
    return pts


def heat_balance():
    f = Fig("heat-balance", 760, 412, "The conductor's heat balance",
            "A conductor cut across: Joule heating inside it, solar heating from the sun, "
            "convective cooling by the wind and radiative cooling outwards; the balance per "
            "unit length and, in plan, the angle between the wind and the conductor's axis.")
    f.text(20, 26, "Heat balance per unit length of conductor", "t")
    cx, cy, r = 200, 195, 52
    f.add(f'<circle cx="{cx}" cy="{cy}" r="{r}" style="fill:var(--fill);stroke:var(--wire);stroke-width:3"/>')
    f.text(cx, cy + 5, "P_{J}", "c t")
    f.label(cx + 26, cy + 30, 262, 300, "Joule heating P_{J} = I² R_{ac}(T)")
    # the sun
    sx, sy = 70, 72
    f.add(f'<circle cx="{sx}" cy="{sy}" r="14" style="fill:var(--wire);fill-opacity:.35;stroke:var(--wire);stroke-width:2"/>')
    for k in range(8):
        a = math.radians(45 * k)
        f.path(f"M{sx + 19 * math.cos(a):.1f},{sy + 19 * math.sin(a):.1f} "
               f"L{sx + 26 * math.cos(a):.1f},{sy + 26 * math.sin(a):.1f}", "", "stroke:var(--wire);stroke-width:2")
    for d in (-14, 0, 14):
        f.arrow(sx + 30 + d, sy + 30 - d, cx - r * 0.72 + d * 0.6, cy - r * 0.72 - d * 0.6, "wire", 1.6, 7)
    f.text(sx + 34, sy - 10, "solar heating P_{S}")
    # wind
    for k in range(3):
        y = cy + 18 + 22 * k
        f.arrow(24, y, cx - r - 12 + (k == 0) * 6, y, "acc", 1.6, 7)
    f.text(24, cy + 100, "convective cooling P_{C}:", "")
    f.text(24, cy + 116, "forced by the wind, or natural", "lab")
    # radiation
    for ang in (-40, -5, 30):
        a = math.radians(ang)
        f.poly(wave(cx + (r + 6) * math.cos(a), cy + (r + 6) * math.sin(a),
                    cx + (r + 52) * math.cos(a), cy + (r + 52) * math.sin(a)), "w3l")
    f.text(cx + r + 30, cy - 62, "radiative cooling P_{R}")
    # the balance
    x = 400
    f.text(x, 80, "m c_{p} dT/dt = P_{J} + P_{S} − P_{C} − P_{R}", "t")
    f.text(x, 108, "steady state: the left side is zero; the ampacity", "lab")
    f.text(x, 124, "for a maximum temperature T_{max}:", "lab")
    f.text(x, 148, "I = √((P_{C} + P_{R} − P_{S}) / R_{ac}(T_{max}))")
    f.text(x, 176, "P_{R} = π D σ ε [(T + 273)^{4} − (T_{a} + 273)^{4}]", "lab")
    # plan: the wind's angle to the axis
    px, py = 520, 318
    f.text(x, 270, "in plan:", "lab")
    f.path(f"M{px - 60},{py} L{px + 150},{py}", "", "stroke:var(--wire);stroke-width:5;stroke-linecap:round")
    f.text(px + 156, py + 4, "conductor axis", "lab")
    f.arrow(px - 80, py + 52, px - 2, py + 1, "acc", 1.8, 8)
    f.text(px - 86, py + 58, "wind", "e lab")
    f.arc(px, py, 40, 180 - math.degrees(math.atan2(52, 80)), 180, "thin")
    f.text(px - 48, py + 18, "φ", "e")
    f.text(x, 404, "K_{angle}: 1 with the wind across the conductor, 0.388 along it", "lab")
    return f


def temperature_limit():
    f = Fig("temperature-limit", 760, 400, "The maximum operating temperature from clearance",
            "The governing clearance falls as the conductor temperature rises. The search starts "
            "at the lowest temperature of the range, steps up by 10 degrees until a check fails, "
            "then bisects between the last pass and the first failure until they are 0.5 degrees "
            "apart; the limit is the last temperature that passed.")
    f.text(20, 26, "Maximum operating temperature from clearance: the search", "t")
    X0, X1, Y0, Y1 = 100, 700, 345, 60

    def X(t):
        return X0 + (X1 - X0) * t / 150

    def Y(t):
        return 90 + 1.0 * t + 0.004 * t * t

    req = Y(87.0)
    f.arrow(X0, Y0, X1 + 20, Y0, "muted", 1.2, 8)
    f.arrow(X0, Y0, X0, Y1 - 10, "muted", 1.2, 8)
    f.text(X1 + 20, Y0 + 34, "conductor temperature", "e lab")
    f.text(X0 + 8, Y1 - 4, "governing clearance (falls as the sag grows)", "lab")
    f.text(X0, Y0 + 18, "range start", "c lab")
    f.text(X(150), Y0 + 18, "range end", "c lab")
    f.path(f"M{X(150)},{Y0 - 4} L{X(150)},{Y0 + 4}", "thin")
    f.path(f"M{X0},{req:.1f} L{X1},{req:.1f}", "acc ds")
    f.text(X1, req - 8, "required clearance", "e")
    f.poly([(X(t), Y(t)) for t in range(0, 151, 3)], "wire")
    for t in range(0, 81, 10):
        f.dot(X(t), Y(t), 4.5, "wire2")
    f.add(f'<circle cx="{X(90):.1f}" cy="{Y(90):.1f}" r="5" style="fill:var(--bg);stroke:var(--wire);stroke-width:2.2"/>')
    f.label(X(0) + 3, Y(0) + 4, X(0) + 24, 192, "1: the lowest temperature of the range")
    f.label(X(40), Y(40), X(40) + 40, Y(40) - 40, "2: up in steps of 10 °C ...")
    f.label(X(90) + 4, Y(90) + 4, X(90) + 40, Y(90) + 50, "... until a check fails")
    # the bisection, enlarged
    bx0, bx1, by = 150, 450, 300
    f.add(f'<rect class="box" x="{bx0 - 16}" y="{by - 58}" width="{bx1 - bx0 + 32}" height="86" rx="6"/>')
    f.text(bx0 - 6, by - 40, "3: bisection between the last pass and the first", "lab")
    f.text(bx0 - 6, by - 25, "failure, until they are 0.5 °C apart", "lab")
    ok = {80: True, 90: False}
    lo, hi = 80.0, 90.0
    while hi - lo > 0.5:
        mid = (lo + hi) / 2
        ok[mid] = Y(mid) <= req
        lo, hi = (mid, hi) if ok[mid] else (lo, mid)
    tt = list(ok)

    def BX(t):
        return bx0 + (bx1 - bx0) * (t - 80) / 10

    f.path(f"M{bx0},{by} L{bx1},{by}", "thin")
    for t in tt:
        if ok[t]:
            f.dot(BX(t), by, 4.5, "wire2")
        else:
            f.add(f'<circle cx="{BX(t):.1f}" cy="{by}" r="5" style="fill:var(--bg);stroke:var(--wire);stroke-width:2.2"/>')
    f.text(BX(80), by + 20, "pass", "c lab")
    f.text(BX(90), by + 20, "fails", "c lab")
    f.arrow(BX(lo) - 30, by - 16, BX(lo) - 4, by - 4, "ink", 1.3, 6)
    f.text(BX(lo) - 34, by - 7, "limit: the last that passed", "e sm")
    f.path(f"M{X(80)},{Y(80) + 6} L{bx0},{by - 58}", "thin dt")
    f.path(f"M{X(90)},{Y(90) + 6} L{bx1},{by - 58}", "thin dt")
    f.text(20, 392, "Conservative: the limit lies up to 0.5 °C below the exact one.", "lab")
    return f


# =====================================================================================
# Optimum structure spotting
# =====================================================================================
def spotting_profile():
    f = Fig("spotting-profile", 760, 400, "Spotting on a ground profile",
            "A ground profile g(s) with candidate stations every step, a forbidden zone without "
            "stations and a required station; structures of candidate heights at chosen stations, "
            "the wire between their lowest wire points, the required clearance c and the point "
            "of a ground piece where the wire comes closest.")
    f.text(20, 26, "Spotting: candidate stations, spans and the clearance", "t")
    G = [(30, 300), (120, 295), (200, 275), (270, 262), (340, 214), (400, 250), (450, 300),
         (520, 292), (580, 272), (660, 276), (730, 284)]

    def g(x):
        for (x0, y0), (x1, y1) in zip(G, G[1:]):
            if x0 <= x <= x1:
                return y0 + (y1 - y0) * (x - x0) / (x1 - x0)
        return G[-1][1]

    fz = (405, 470)
    f.add(f'<rect x="{fz[0]}" y="40" width="{fz[1] - fz[0]}" height="310" class="fw"/>')
    f.text((fz[0] + fz[1]) / 2, 54, "forbidden", "c lab")
    f.text((fz[0] + fz[1]) / 2, 68, "zone", "c lab")
    f.poly(G, "gnd")
    f.text(34, 316, "g(s)", "lab")
    clr = 20
    f.poly([(x, y - clr) for x, y in G], "acc ds")
    # candidate stations
    for x in range(40, 721, 20):
        if fz[0] <= x <= fz[1]:
            continue
        f.path(f"M{x},342 L{x},350", "thin")
    f.text(40, 368, "candidate stations s_{0} < s_{1} < … every step", "lab")
    req = 580
    f.add(f'<path style="fill:var(--acc);stroke:none" d="M{req},334 L{req - 6},344 L{req + 6},344 Z"/>')
    f.label(req, 338, 560, 382, "required station (alignment point)", end=True)
    # structures and spans
    hk = {40: 92, 240: 112, 580: 104, 720: 92}
    xs = sorted(hk)
    wp_ = [(x, g(x) - hk[x]) for x in xs]
    for x in xs:
        f.path(f"M{x},{g(x)} L{x},{g(x) - hk[x] - 16}", "st")
        f.dot(x, g(x) - hk[x], 4, "wire")
    best = None
    for (x0, y0), (x1, y1) in zip(wp_, wp_[1:]):
        L = x1 - x0
        sag = L * L / (8 * 400)
        pts = parab(x0, y0, x1, y1, sag, 60)
        f.poly(pts, "wire")
        for (gx0, gy0), (gx1, gy1) in zip(G, G[1:]):
            lo, hi = max(gx0, x0), min(gx1, x1)
            if lo >= hi:
                continue
            m = (gy1 - gy0) / (gx1 - gx0)
            # wire: y = y0 + (y1-y0) t + 4 sag t (1-t); dy/dx = ((y1-y0) + 4 sag (1 - 2t)) / L = m
            t = 0.5 * (1 - (m * L - (y1 - y0)) / (4 * sag))
            xx = min(max(x0 + L * t, lo), hi)
            tt = (xx - x0) / L
            wy = y0 + (y1 - y0) * tt + 4 * sag * tt * (1 - tt)
            gap = g(xx) - wy
            if best is None or gap < best[0]:
                best = (gap, xx, wy, (gx0, gy0, gx1, gy1))
    f.vdim2(xs[0] - 16, g(xs[0]), g(xs[0]) - hk[xs[0]], "h_{k}", left=True)
    f.text(xs[0] - 16, g(xs[0]) - hk[xs[0]] - 26, "lowest wire point", "lab")
    f.path(f"M{xs[1]},{g(xs[1]) - hk[xs[1]] - 22} L{xs[1]},94 M{xs[2]},{g(xs[2]) - hk[xs[2]] - 22} "
           f"L{xs[2]},94", "thin dt")
    f.hdim2(xs[1], xs[2], 100, "span L = s_{j} − s_{i}  (L_{min} ≤ L ≤ L_{max})")
    gap, xx, wy, (gx0, gy0, gx1, gy1) = best
    f.vdim2(xx, wy, g(xx), "")
    f.label(xx + 2, (wy + g(xx)) / 2, xx + 40, 132, "least clearance: on each ground piece where the")
    f.text(xx + 44, 148, "wire's slope equals the piece's slope m (clamped to it)", "")
    f.path(f"M{gx0},{gy0} L{gx1},{gy1}", "", "stroke:var(--ink);stroke-width:2.6")
    f.label(500, g(500) - clr, 524, 232, "required clearance c above the ground")
    return f


def wind_weight_span():
    f = Fig("wind-weight-span", 760, 400, "Wind span and weight span",
            "Structure j between its back span L_b and its span ahead L_a: the wind span runs "
            "from the middle of one to the middle of the other, the weight span from the low "
            "point of one to the low point of the other.")
    f.text(20, 26, "Wind span and weight span of structure j", "t")
    g = 320
    ground(f, 30, 740, g)
    S = [(80, 120.0), (390, 175.0), (700, 112.0)]
    sag = 90
    for x, y in S:
        f.path(f"M{x},{g} L{x},{y - 18}", "st")
        f.dot(x, y, 4, "wire")
    lows = []
    for (x0, y0), (x1, y1) in zip(S, S[1:]):
        f.poly(parab(x0, y0, x1, y1, sag, 60), "wire")
        t = 0.5 + (y1 - y0) / (8 * sag)
        lx = x0 + (x1 - x0) * t
        ly = y0 + (y1 - y0) * t + 4 * sag * t * (1 - t)
        lows.append((lx, ly))
        f.dot(lx, ly, 4)
    f.text(S[1][0], g + 18, "j", "c t")
    f.text(S[0][0], g + 18, "back", "c lab")
    f.text(S[2][0], g + 18, "ahead", "c lab")
    f.hdim2(S[0][0], S[1][0], g + 42, "L_{b}")
    f.hdim2(S[1][0], S[2][0], g + 42, "L_{a}")
    for lx, ly in lows:
        f.path(f"M{lx},{ly + 6} L{lx},{g + 76}", "thin dt")
        f.path(f"M{lx},{ly - 6} L{lx},80", "thin dt")
    f.hdim2(S[0][0], lows[0][0], g + 70, "x_{b}")
    f.hdim2(S[1][0], lows[1][0], g + 70, "x_{a}")
    m0, m1 = (S[0][0] + S[1][0]) / 2, (S[1][0] + S[2][0]) / 2
    f.path(f"M{m0},{g - 4} L{m0},54 M{m1},{g - 4} L{m1},54", "thin dt")
    f.add(f'<rect x="{m0}" y="54" width="{m1 - m0}" height="10" class="fa"/>')
    f.hdim2(m0, m1, 59, "")
    f.text((m0 + m1) / 2, 50, "wind span (L_{b} + L_{a})/2", "c")
    f.add(f'<rect x="{lows[0][0]:.1f}" y="80" width="{lows[1][0] - lows[0][0]:.1f}" height="10" class="fw"/>')
    f.hdim2(lows[0][0], lows[1][0], 85, "")
    f.text((lows[0][0] + lows[1][0]) / 2, 108, "weight span (L_{b} − x_{b}) + x_{a}", "c")
    f.text(lows[0][0] + 8, lows[0][1] + 18, "low point", "lab")
    return f


# =====================================================================================
# Structure analysis
# =====================================================================================
def capped_law():
    f = Fig("capped-law", 760, 360, "A tension-only member: the capped law",
            "Axial force N against the stretch: active, the member follows EA/L in tension and in "
            "compression down to its buckling load; shortened further it is bowed and carries "
            "N_cr; without a second moment it is slack in compression.")
    f.text(20, 26, "A tension-only member: compression up to its buckling load", "t")
    ox, oy = 400, 175
    f.arrow(110, oy, 700, oy, "muted", 1.2, 8)
    f.arrow(ox, 282, ox, 50, "muted", 1.2, 8)
    f.text(700, oy + 20, "stretch δ", "e lab")
    f.text(ox + 8, 56, "N (tension positive)", "lab")
    k = 130 / 240
    ncr = 58
    xcr = ox - ncr / k
    f.path(f"M{xcr:.1f},{oy + ncr} L{ox + 250},{oy - 250 * k:.1f}", "", "stroke:var(--wire);stroke-width:2.8")
    f.path(f"M120,{oy + ncr} L{xcr:.1f},{oy + ncr}", "", "stroke:var(--wire);stroke-width:2.8")
    f.path(f"M120,{oy + 2} L{ox},{oy + 2}", "", "stroke:var(--wire3);stroke-width:2.4;stroke-dasharray:6 4")
    f.path(f"M{ox - 6},{oy + ncr} L{ox + 6},{oy + ncr}", "thin")
    f.text(ox + 10, oy + ncr + 4, "−N_{cr}")
    f.text(ox + 140, oy - 40, "active: N = (EA/L) δ")
    f.text(130, oy + ncr + 22, "bowed: N = −N_{cr}, no stiffness", "")
    f.text(130, oy - 10, "slack (I_{min} = 0, N_{cr} = 0): N = 0", "")
    f.text(20, 300, "N_{cr} = π² E I_{min} / L_{k}²,  L_{k} = lk, else k × length (k = 1: pinned ends)", "lab")
    f.text(20, 318, "A member out of the active set has no stiffness and carries its constant force; "
           "the set is found by repeated solves.", "lab")
    # little members
    f.path(f"M560,250 L690,250", "", "stroke:var(--ink);stroke-width:3")
    f.text(700, 254, "active", "lab")
    f.path(f"M560,275 Q625,262 690,275", "", "stroke:var(--ink);stroke-width:3;fill:none")
    f.text(700, 279, "bowed", "lab")
    f.poly(wave(560, 300, 690, 300, 3, 4), "", "stroke:var(--wire3);stroke-width:2.4;fill:none")
    f.text(700, 304, "slack", "lab")
    return f


def x_panel():
    f = Fig("x-panel", 760, 340, "An X-braced panel with tension-only diagonals",
            "A panel four wide and three high, pinned at its feet, loaded by H across at a top "
            "corner. Left, diagonals without a second moment: one carries 5H/4 in tension, the "
            "other is slack. Right, with a second moment: the compressed one is bowed at N_cr, "
            "the other carries 5H/4 − N_cr.")
    f.text(20, 26, "An X-braced panel under a horizontal load H", "t")
    for k, (x0, title) in enumerate(((100, "diagonals without a second moment"),
                                     (440, "diagonals with a second moment"))):
        w, h, top = 200, 150, 100
        bl, br, tl, tr = (x0, top + h), (x0 + w, top + h), (x0, top), (x0 + w, top)
        f.text(x0 + w / 2, 70, title, "c")
        f.path(f"M{bl[0]},{bl[1]} L{tl[0]},{tl[1]} L{tr[0]},{tr[1]} L{br[0]},{br[1]}", "st")
        f.path(f"M{bl[0]},{bl[1]} L{tr[0]},{tr[1]}", "", "stroke:var(--acc);stroke-width:3.2")
        if k == 0:
            f.path(f"M{tl[0]},{tl[1]} L{br[0]},{br[1]}", "", "stroke:var(--wire3);stroke-width:2;stroke-dasharray:6 4")
            f.text(x0 + w * 0.70, top + h * 0.52, "slack", "")
            f.text(x0 + 78, top + h - 22, "5H/4")
        else:
            mx, my = (tl[0] + br[0]) / 2, (tl[1] + br[1]) / 2
            f.path(f"M{tl[0]},{tl[1]} Q{mx + 16},{my - 20} {br[0]},{br[1]}", "",
                   "stroke:var(--wire);stroke-width:2.6;fill:none")
            f.label(mx + 30, my - 6, x0 + w + 14, top + h / 2 - 30, "bowed: N_{cr}")
            f.text(x0 + 78, top + h - 22, "5H/4 − N_{cr}")
        for x, y in (bl, br):
            f.path(f"M{x},{y} L{x - 9},{y + 15} L{x + 9},{y + 15} Z", "st", "stroke-width:1.8")
        for x, y in (bl, br, tl, tr):
            f.dot(x, y, 3.5)
        f.arrow(x0 - 70, top, x0 - 6, top, "ink", 2.2, 9)
        f.text(x0 - 72, top - 8, "H", "")
        f.text(x0 + w / 2, top + h + 34, "4", "c lab")
        f.text(x0 - 14, top + h / 2 + 4, "3", "e lab")
    f.text(20, 318, "By the method of joints: the stretched diagonal (length 5) takes the load "
           "H across; with the bowed one at N_{cr}, the rest.", "lab")
    return f


def pole_check():
    f = Fig("pole-check", 760, 420, "The pole check",
            "A tapered pole from the ground-line diameter D_0 to the top diameter D_1 over the "
            "height H = l − e above the ground, embedded e: the attachment loads F_i at their "
            "points r_i, the wind on the shaft and the bending moment M(h) at a height h.")
    f.text(20, 26, "Pole check: moment at every height h", "t")
    cx, g, top, bot = 260, 330, 70, 395
    d0, d1 = 34, 16
    f.add(f'<path class="box" d="M{cx - d0 / 2},{bot} L{cx - d1 / 2},{top} L{cx + d1 / 2},{top} '
          f'L{cx + d0 / 2},{bot} Z"/>')
    f.add(f'<rect x="{cx - 120}" y="{g}" width="240" height="{bot - g + 8}" class="fi"/>')
    ground(f, 80, 440, g)
    f.path(f"M{cx - 90},90 L{cx + 90},90", "st")
    for x in (cx - 82, cx + 82):
        gf.string(f, x, 90, x, 108)
        gf.ap(f, x, 108)
        f.arrow(x, 108, x, 150, "wire", 2, 8)
        f.arrow(x, 108, x + 44, 108, "wire", 2, 8)
    f.text(cx + 82 + 50, 112, "F_{i} (factored)")
    f.path(f"M{cx},{g} L{cx + 82},108", "thin ds")
    f.label(cx + 60, 150, cx + 100, 175, "r_{i}", "lab")
    # wind on the shaft
    for y in range(140, 320, 30):
        dw = d1 + (d0 - d1) * (y - top) / (bot - top)
        f.arrow(cx - dw / 2 - 52, y, cx - dw / 2 - 4, y, "acc", 1.4, 6)
    f.text(cx - 60, 166, "p c_{d} D(z)", "e")
    f.text(cx - 60, 182, "across the line", "e lab")
    # the section at h
    hy = 230
    f.path(f"M{cx - 60},{hy} L{cx + 140},{hy}", "thin ds")
    f.arc(cx, hy, 30, -150, 150, "", "stroke:var(--ink);stroke-width:1.8;fill:none")
    f.arrow(cx + 30 * math.cos(math.radians(140)), hy + 30 * math.sin(math.radians(140)),
            cx + 30 * math.cos(math.radians(152)), hy + 30 * math.sin(math.radians(152)), "ink", 1.8, 8)
    f.text(cx + 40, hy - 6, "M(h)")
    f.vdim2(cx + 130, g, hy, "h")
    # dimensions
    f.vdim2(cx + 160, g, top, "H = ℓ − e")
    f.vdim2(cx + 160, bot, g, "e")
    f.path(f"M{cx + d1 / 2 + 4},{top} L{cx + 166},{top} M{cx + d0 / 2 + 4},{bot} L{cx + 166},{bot}", "thin dt")
    f.label(cx - d1 / 2, top + 4, cx - 70, top + 4, "D_{1}", end=True)
    f.label(cx - d0 / 2 + 2, g - 4, cx - 70, g - 20, "D_{0} (ground line)", end=True)
    x = 505
    f.text(x, 210, "M(h): the attachment loads above h", "lab")
    f.text(x, 226, "(horizontal loads × height, vertical", "lab")
    f.text(x, 242, "loads × their offset from the axis)", "lab")
    f.text(x, 258, "and the wind on the shaft above h;", "lab")
    f.text(x, 274, "at 41 heights, ground line to top.", "lab")
    f.text(x, 300, "usage max_{h} M(h) / (φ M_{c}(h))", "")
    f.text(x, 330, "P-Delta: M(h) times the ratio of", "lab")
    f.text(x, 346, "the ground-line moments (second /", "lab")
    f.text(x, 362, "first order), when the pole gives E.", "lab")
    return f


# =====================================================================================
# Wind on lattice structures
# =====================================================================================
def member_wind():
    f = Fig("member-wind", 760, 350, "The wind on a member",
            "A member at an angle to the wind: the wind's unit vector w, the member's axis x, the "
            "angle theta between them, the wind's part normal to the member w_n of length sin "
            "theta, and the force F along w_n, half at each joint.")
    f.text(20, 26, "The cross-flow force on one member", "t")
    a = math.radians(-58)
    ox, oy = 290, 190
    L = 125
    p0 = (ox - L * math.cos(a), oy - L * math.sin(a))
    p1 = (ox + L * math.cos(a), oy + L * math.sin(a))
    f.path(f"M{p0[0]:.1f},{p0[1]:.1f} L{p1[0]:.1f},{p1[1]:.1f}", "", "stroke:var(--ink);stroke-width:7;stroke-linecap:round")
    for p in (p0, p1):
        f.add(f'<circle cx="{p[0]:.1f}" cy="{p[1]:.1f}" r="6" style="fill:var(--bg);stroke:var(--ink);stroke-width:2"/>')
    ux, uy = math.cos(a), math.sin(a)
    W = 130
    f.arrow(ox - W, oy, ox, oy, "acc", 2.2, 9)
    f.text(ox - W, oy - 10, "wind w (unit)")
    dp = 1.0 * ux
    nx, ny = 1 - dp * ux, 0 - dp * uy
    qx, qy = ox + 16 * uy, oy - 16 * ux
    f.arrow(qx, qy, qx + 90 * ux, qy + 90 * uy, "muted", 1.6, 8)
    f.text(qx + 90 * ux - 8, qy + 90 * uy + 4, "member axis x", "e")
    # w_n from the origin, scaled as w
    f.path(f"M{ox + W * dp * ux:.1f},{oy + W * dp * uy:.1f} L{ox + W:.1f},{oy}", "thin ds")
    f.path(f"M{ox},{oy} L{ox + W},{oy}", "thin ds")
    f.arrow(ox, oy, ox + W * nx, oy + W * ny, "wire", 2.4, 9)
    f.text(ox + W * nx + 10, oy + W * ny + 18, "w_{n} = w − (w · x) x,  length sin θ")
    f.arc(ox, oy, 40, math.degrees(a), 0, "thin")
    f.text(ox + 46, oy - 12, "θ")
    x = 470
    f.text(x, 90, "F = q c η d L sin θ  w_{n}", "t")
    f.text(x, 112, "|F| = q c η d L sin²θ", "")
    f.text(x, 140, "q: the wind pressure; c: the drag factor;", "lab")
    f.text(x, 156, "η: the shielding factor (1 unless", "lab")
    f.text(x, 172, "behind a face); d: the profile's width", "lab")
    f.text(x, 188, "across w_{n}; L: the length.", "lab")
    f.text(x, 214, "Half of F at each joint.", "lab")
    for p in (p0, p1):
        f.arrow(p[0], p[1], p[0] + 40 * nx, p[1] + 40 * ny, "wire", 1.8, 7)
        f.text(p[0] + 46 * nx + 6, p[1] + 46 * ny + 4, "F/2", "lab")
    return f


def face_solidity():
    f = Fig("face-solidity", 760, 400, "A face, its outline and its solidity ratio",
            "Left: one face of a lattice body, its outline the convex hull of its joints with the "
            "area A_o, its extent h along z and its breadth b = A_o/h; the solidity ratio sums "
            "the members' silhouettes over A_o. Right: a waisted face, whose convex hull is "
            "larger than the face.")
    f.text(20, 26, "A face of the lattice: outline, breadth and solidity ratio", "t")
    bl, br, tl, tr = (90, 340), (370, 340), (170, 80), (290, 80)
    levels = [0, 0.32, 0.6, 0.82, 1.0]

    def at(t, side):
        p, q = (bl, tl) if side == 0 else (br, tr)
        return p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t

    f.add(f'<path class="fa" d="M{bl[0]},{bl[1]} L{tl[0]},{tl[1]} L{tr[0]},{tr[1]} L{br[0]},{br[1]} Z"/>')
    f.add(f'<path class="dash" d="M{bl[0] - 6},{bl[1] + 5} L{tl[0] - 5},{tl[1] - 5} L{tr[0] + 5},{tr[1] - 5} '
          f'L{br[0] + 6},{br[1] + 5} Z"/>')
    f.path(f"M{bl[0]},{bl[1]} L{tl[0]},{tl[1]} M{br[0]},{br[1]} L{tr[0]},{tr[1]}", "st")
    for t0, t1 in zip(levels, levels[1:]):
        a0, b0, a1, b1 = at(t0, 0), at(t0, 1), at(t1, 0), at(t1, 1)
        f.path(f"M{a0[0]:.1f},{a0[1]:.1f} L{b0[0]:.1f},{b0[1]:.1f} M{a0[0]:.1f},{a0[1]:.1f} "
               f"L{b1[0]:.1f},{b1[1]:.1f} M{b0[0]:.1f},{b0[1]:.1f} L{a1[0]:.1f},{a1[1]:.1f}",
               "", "stroke:var(--ink);stroke-width:1.6")
    f.path(f"M{tl[0]:.1f},{tl[1]} L{tr[0]},{tr[1]}", "", "stroke:var(--ink);stroke-width:1.6")
    a0, b1 = at(0.32, 0), at(0.6, 1)
    f.path(f"M{a0[0]:.1f},{a0[1]:.1f} L{b1[0]:.1f},{b1[1]:.1f}", "", "stroke:var(--wire);stroke-width:4")
    mx, my = (a0[0] + b1[0]) / 2, (a0[1] + b1[1]) / 2
    f.label(a0[0] + 0.75 * (b1[0] - a0[0]), a0[1] + 0.75 * (b1[1] - a0[1]), 410, 150, "member m: its silhouette")
    f.text(414, 168, "d_{m} L_{m} sin θ_{m}")
    f.label(tl[0] - 2, tl[1] - 4, 120, 56, "outline: convex hull of the joints, area A_{o}", end=False)
    f.vdim2(60, bl[1], tl[1], "h", left=True)
    f.text(410, 228, "φ = Σ d_{m} L_{m} sin θ_{m} / A_{o}", "t")
    f.text(410, 254, "breadth b = A_{o} / h", "")
    f.text(410, 272, "(h: the extent along z, in the plane)", "lab")
    # a waisted face
    ox = 670
    W = [(ox - 60, 340), (ox - 25, 210), (ox - 48, 80), (ox + 48, 80), (ox + 25, 210), (ox + 60, 340)]
    f.add(f'<path class="dash" d="M{W[0][0] - 6},{W[0][1] + 5} L{W[2][0] - 5},{W[2][1] - 5} '
          f'L{W[3][0] + 5},{W[3][1] - 5} L{W[5][0] + 6},{W[5][1] + 5} Z"/>')
    f.poly(W[:3], "st")
    f.poly(W[3:], "st")
    f.path(f"M{W[2][0]},{W[2][1]} L{W[3][0]},{W[3][1]} M{W[1][0]},{W[1][1]} L{W[4][0]},{W[4][1]} "
           f"M{W[0][0]},{W[0][1]} L{W[4][0]},{W[4][1]} M{W[5][0]},{W[5][1]} L{W[1][0]},{W[1][1]} "
           f"M{W[1][0]},{W[1][1]} L{W[3][0]},{W[3][1]} M{W[4][0]},{W[4][1]} L{W[2][0]},{W[2][1]}",
           "", "stroke:var(--ink);stroke-width:1.4")
    f.text(ox, 368, "a waisted face: the hull", "c lab")
    f.text(ox, 384, "is larger, φ lower (Conservative)", "c lab")
    return f


def shielding_plan():
    f = Fig("shielding-plan", 760, 400, "Which members are shielded",
            "Plan of a square lattice body. Left, the wind normal to two faces: the far face is "
            "shielded by the near one at the spacing e, the two side faces are edge-on. Right, "
            "the wind at 45 degrees: the two far faces and the far corner leg are shielded, the "
            "other legs not.")
    f.text(20, 26, "Shielding seen in plan", "t")

    def face(x0, y0, x1, y1, kind):
        st = {"near": "stroke:var(--ink);stroke-width:3",
              "far": "stroke:var(--wire3);stroke-width:3;stroke-dasharray:7 4",
              "edge": "stroke:var(--muted);stroke-width:1.6"}[kind]
        f.path(f"M{x0},{y0} L{x1},{y1}", "", st)

    # left: wind along +x
    x0, y0, s = 150, 110, 160
    face(x0, y0, x0, y0 + s, "near")
    face(x0 + s, y0, x0 + s, y0 + s, "far")
    face(x0, y0, x0 + s, y0, "edge")
    face(x0, y0 + s, x0 + s, y0 + s, "edge")
    for x, y in ((x0, y0), (x0 + s, y0), (x0, y0 + s), (x0 + s, y0 + s)):
        f.dot(x, y, 5)
    for k in range(3):
        f.arrow(40, y0 + 30 + 50 * k, 100, y0 + 30 + 50 * k, "acc", 1.8, 8)
    f.text(40, y0 - 8, "wind", "lab")
    my = y0 + s / 2 + 22
    f.arrow(x0 + s, my, x0 + 4, my, "muted", 1.2, 7, dash=True)
    f.dot(x0 + s, my, 3.5, "wire3")
    f.text(x0 + s / 2, my - 8, "ray back against the wind", "c lab")
    f.hdim2(x0, x0 + s, y0 + s + 30, "e (spacing)", above=False)
    f.vdim2(x0 - 22, y0, y0 + s, "b", left=True)
    f.text(x0 + s / 2, y0 - 14, "edge-on: neither", "c lab")
    f.text(x0 + s + 12, y0 + 40, "far face:", "lab")
    f.text(x0 + s + 12, y0 + 56, "shielded", "lab")
    f.text(x0 + s / 2, y0 + s + 70, "η(φ_{front}, e/b), φ_{front}: the near face's", "c lab")
    # right: wind at 45 degrees from the lower left
    x0 = 520
    face(x0, y0, x0, y0 + s, "near")
    face(x0, y0 + s, x0 + s, y0 + s, "near")
    face(x0 + s, y0, x0 + s, y0 + s, "far")
    face(x0, y0, x0 + s, y0, "far")
    for x, y in ((x0, y0), (x0, y0 + s), (x0 + s, y0 + s)):
        f.dot(x, y, 5)
    f.add(f'<circle cx="{x0 + s}" cy="{y0}" r="6" style="fill:var(--wire3);stroke:none"/>')
    for k in range(3):
        bx, by = 420 + 28 * k, 262 + 28 * k
        f.arrow(bx, by, bx + 40, by - 40, "acc", 1.8, 8)
    f.text(420, 352, "wind at 45°", "lab")
    f.label(x0 + s - 3, y0 - 4, x0 + s - 24, y0 - 30, "far corner leg: shielded", end=True)
    f.text(x0 + s + 12, y0 + s / 2, "far faces:", "lab")
    f.text(x0 + s + 12, y0 + s / 2 + 16, "shielded", "lab")
    f.text(x0 - 12, y0 + s / 2 - 8, "near faces:", "e lab")
    f.text(x0 - 12, y0 + s / 2 + 8, "not shielded", "e lab")
    # legend
    f.path("M30,385 L60,385", "", "stroke:var(--ink);stroke-width:3")
    f.text(66, 389, "faces the wind, in front", "lab")
    f.path("M230,385 L260,385", "", "stroke:var(--wire3);stroke-width:3;stroke-dasharray:7 4")
    f.text(266, 389, "shielded", "lab")
    f.path("M350,385 L380,385", "", "stroke:var(--muted);stroke-width:1.6")
    f.text(386, 389, "edge-on", "lab")
    return f


# =====================================================================================
# Galloping
# =====================================================================================
def ellipse_pts(cx, cy, A, B, theta, n=72):
    """An ellipse of full axes A (tilted theta degrees from the vertical, top to the right)
    and B about (cx, cy)."""
    t = math.radians(theta)
    ax, ay = math.sin(t), -math.cos(t)
    bx, by = math.cos(t), math.sin(t)
    return [(cx + A / 2 * math.cos(2 * math.pi * k / n) * ax + B / 2 * math.sin(2 * math.pi * k / n) * bx,
             cy + A / 2 * math.cos(2 * math.pi * k / n) * ay + B / 2 * math.sin(2 * math.pi * k / n) * by)
            for k in range(n)]


def galloping_ellipse():
    f = Fig("galloping-ellipse", 760, 380, "The galloping ellipse",
            "Across the span, looking from its first structure to its last: the wire at rest, "
            "the ellipse's major axis A tilted by theta from the vertical, its minor axis B and "
            "its centre c A above the wire at rest along the major axis.")
    f.text(20, 26, "The galloping ellipse (across the span)", "t")
    rx, ry = 240, 300
    A, B, th, c = 230, 110, 18, 0.4
    t = math.radians(th)
    ux, uy = math.sin(t), -math.cos(t)
    cx, cy = rx + c * A * ux, ry + c * A * uy
    f.poly(ellipse_pts(cx, cy, A, B, th), "acc", "fill:var(--acc);fill-opacity:.10", close=True)
    f.path(f"M{cx - A / 2 * ux:.1f},{cy - A / 2 * uy:.1f} L{cx + A / 2 * ux:.1f},{cy + A / 2 * uy:.1f}", "thin ds")
    f.path(f"M{cx - B / 2 * -uy:.1f},{cy - B / 2 * ux:.1f} L{cx + B / 2 * -uy:.1f},{cy + B / 2 * ux:.1f}", "thin ds")
    f.path(f"M{rx},{ry + 20} L{rx},{ry - A * 0.95}", "thin dt")
    f.add(f'<circle cx="{rx}" cy="{ry}" r="7" class="wire" style="stroke-width:3;fill:var(--bg)"/>')
    f.dot(cx, cy, 3.5, "acc")
    f.arc(rx, ry, 150, -90, -90 + th, "thin")
    f.text(rx + 26, ry - 156, "θ", "")
    # dimensions along the axes, offset to the side
    off = B / 2 + 26
    p0 = (cx - A / 2 * ux - off * -uy, cy - A / 2 * uy - off * ux)
    p1 = (cx + A / 2 * ux - off * -uy, cy + A / 2 * uy - off * ux)
    f.arrow(*p0, *p1, "muted", 1.1, 7, both=True)
    f.text((p0[0] + p1[0]) / 2 - 10, (p0[1] + p1[1]) / 2, "A", "e")
    f.label(rx - 6, ry + 4, rx - 60, ry + 40, "wire at rest", end=True)
    f.label(cx, cy, cx + 90, cy + 70, "centre: c A above the wire at rest")
    f.label(cx + B / 2 * -uy * 0.6, cy + B / 2 * ux * 0.6, cx + 110, cy + 20, "B (minor axis)")
    f.text(rx + 34, 62, "θ positive: the top to the right", "lab")
    f.text(20, 362, "Looking from the span's first structure to its last; A, B full lengths.", "lab")
    x = 430
    f.text(x, 80, "A = k_{D} D + k_{L} L + k_{√D} √D + k_{d} d", "")
    f.text(x + 20, 100, "+ k_{ln} n d ln(s D / n² d) + k_{0}", "")
    f.text(x, 126, "B = (the same terms) + k_{A} A", "")
    f.text(x, 156, "D: the sag; L: the span; d: the conductor's", "lab")
    f.text(x, 172, "diameter; n: the number of loops.", "lab")
    f.text(x, 198, "Every coefficient from the user's", "lab")
    f.text(x, 214, "parameter file; none built in.", "lab")
    return f


def galloping_loops():
    f = Fig("galloping-loops", 760, 330, "The ellipse along the span",
            "A span from the side with two loops: the ellipse's size scaled by |sin(n pi t)| at "
            "the fraction t of the span, largest in the middle of each loop and nothing at the "
            "supports; the ellipses compared at the inner stations.")
    f.text(20, 26, "The ellipse along the span: two loops (n = 2)", "t")
    x0, x1, y0, sag = 70.0, 690.0, 90.0, 80.0
    g = 300
    ground(f, 30, 730, g)
    f.path(f"M{x0},{g} L{x0},{y0 - 14} M{x1},{g} L{x1},{y0 - 14}", "st")
    A, c, n = 90.0, 0.4, 2
    up, lo = [], []
    for k in range(121):
        t = k / 120
        x = x0 + (x1 - x0) * t
        y = y0 + 4 * sag * t * (1 - t)
        sc = abs(math.sin(n * math.pi * t))
        up.append((x, y - (c + 0.5) * A * sc))
        lo.append((x, y - (c - 0.5) * A * sc))
    f.poly(up + lo[::-1], "", "fill:var(--acc);fill-opacity:.12;stroke:var(--acc);stroke-width:1.2", close=True)
    f.poly(parab(x0, y0, x1, y0, sag, 80), "wire")
    for k in range(1, 20):
        t = k / 20
        x = x0 + (x1 - x0) * t
        y = y0 + 4 * sag * t * (1 - t)
        sc = abs(math.sin(n * math.pi * t))
        f.path(f"M{x:.1f},{y - (c + 0.5) * A * sc:.1f} L{x:.1f},{y - (c - 0.5) * A * sc:.1f}", "",
               "stroke:var(--acc);stroke-width:1.6")
        f.path(f"M{x:.1f},{g - 4} L{x:.1f},{g + 4}", "thin")
    f.text(x0, g + 20, "t = 0", "c lab")
    f.text(x1, g + 20, "t = 1", "c lab")
    f.text((x0 * 3 + x1) / 4, 58, "loop 1", "c")
    f.text((x0 + 3 * x1) / 4, 58, "loop 2", "c")
    f.text((x0 + x1) / 2, g + 20, "19 inner stations (default): the ellipses compared there", "c lab")
    f.label(x0 + (x1 - x0) * 0.5, y0 + sag, x0 + (x1 - x0) * 0.5 + 40, y0 + sag + 50,
            "nothing between the loops and at the supports", "lab")
    f.text(90, g - 40, "size × |sin(nπt)| at the fraction t of the span", "")
    f.text(90, g - 24, "(or kept the same: along = \"constant\")", "lab")
    return f


def poly_dist(P, Q):
    """The shortest vertex-to-edge distance between two polygons, and its two points."""
    best = None
    for A_, B_ in ((P, Q), (Q, P)):
        for (px, py) in A_:
            for i in range(len(B_)):
                (ax, ay), (bx, by) = B_[i], B_[(i + 1) % len(B_)]
                dx, dy = bx - ax, by - ay
                t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
                qx, qy = ax + t * dx, ay + t * dy
                d = math.hypot(px - qx, py - qy)
                if best is None or d < best[0]:
                    best = (d, (px, py), (qx, qy))
    return best


def galloping_gap():
    f = Fig("galloping-gap", 760, 390, "Clearance between two ellipses",
            "Two wires of one span side by side with their galloping ellipses, each drawn as a "
            "circumscribed polygon; the clearance is the distance between the polygons less "
            "each wire's outer radius.")
    f.text(20, 26, "Clearance between the ellipses of two wires", "t")
    A, B, th = 210, 110, 14
    polys = []
    for rx in (210, 480):
        ry = 270
        cx, cy = rx + 0.4 * A * math.sin(math.radians(th)), ry - 0.4 * A * math.cos(math.radians(th))
        f.poly(ellipse_pts(cx, cy, A, B, th, 96), "thin")
        # a circumscribed polygon: the regular n-gon about the unit circle, stretched
        n = 12
        k = 1 / math.cos(math.pi / n)
        pts = []
        for i in range(n):
            a = 2 * math.pi * (i + 0.5) / n
            t = math.radians(th)
            ax, ay = math.sin(t), -math.cos(t)
            bx, by = math.cos(t), math.sin(t)
            pts.append((cx + k * A / 2 * math.cos(a) * ax + k * B / 2 * math.sin(a) * bx,
                        cy + k * A / 2 * math.cos(a) * ay + k * B / 2 * math.sin(a) * by))
        f.poly(pts, "acc", close=True)
        polys.append(pts)
        f.add(f'<circle cx="{rx}" cy="{ry}" r="9" class="wire" style="stroke-width:3;fill:var(--bg)"/>')
    d, p, q = poly_dist(*polys)
    f.path(f"M{p[0]:.1f},{p[1]:.1f} L{q[0]:.1f},{q[1]:.1f}", "", "stroke:var(--ink);stroke-width:2")
    f.dot(*p, 3)
    f.dot(*q, 3)
    f.label((p[0] + q[0]) / 2, (p[1] + q[1]) / 2, 360, 60, "distance between the polygons", "")
    f.label(210 + 9, 274, 300, 318, "wire at rest: its outer radius is taken off", "")
    f.label(polys[1][1][0], polys[1][1][1], 690, 60, "circumscribed polygon", end=True)
    f.text(686, 78, "(72 sides; 12 drawn)", "e lab")
    f.text(20, 348, "Clearance: the distance less each wire's outer radius (bundle radius, conductor "
           "radius, iced in a case with", "lab")
    f.text(20, 364, "ice, the polyline's bound); 0 when they overlap. Conservative: the polygons hold "
           "the ellipses, so the", "lab")
    f.text(20, 380, "gap found is never more than between the true ellipses.", "lab")
    return f


# =====================================================================================
# LiDAR analyses
# =====================================================================================
def lidar_classes():
    f = Fig("lidar-classes", 760, 420, "Ground filter and height classes",
            "Left: the lowest point of each grid cell, the surface opened with a window wider "
            "than a building, and the cells standing above it by more than the threshold holding "
            "no ground. Right: the classes by height above the ground's TIN.")
    f.text(20, 26, "Classification: ground, then heights above it", "t")
    # left: the morphological filter
    gy = lambda x: 300 - 0.12 * (x - 30)  # noqa: E731
    f.poly([(30, gy(30)), (350, gy(350))], "gnd")
    f.add(f'<rect x="150" y="{gy(150) - 70:.1f}" width="70" height="70" class="box"/>')
    f.text(185, gy(150) - 78, "building", "c lab")
    for k in range(17):
        x0 = 30 + 20 * k
        f.path(f"M{x0},{gy(x0) + 6:.1f} L{x0},{gy(x0) + 22:.1f}", "thin")
        xm = x0 + 10
        if 150 <= xm <= 220:
            y = gy(150) - 70
            f.add(f'<circle cx="{xm}" cy="{y:.1f}" r="4" style="fill:var(--bg);stroke:var(--wire);stroke-width:2"/>')
        elif xm < 350:
            f.dot(xm, gy(xm) - 2, 3.5, "wire2")
    f.poly([(30, gy(30) - 3), (350, gy(350) - 3)], "acc ds")
    f.label(300, gy(300) - 3, 300, 160, "opened surface (window w_{k} c)", end=True)
    f.vdim2(232, gy(232) - 3, gy(150) - 70, "> Δh_{k}")
    f.label(160, gy(150) - 70, 100, 196, "no ground", end=True)
    f.text(30, 340, "lowest point of each cell (size c);", "lab")
    f.text(30, 356, "w_{k} = 2r_{k} + 1 cells, r_{k} = 1, 2, 4, 8, …", "lab")
    f.text(30, 372, "Δh_{k} = s (w_{k} − w_{k−1}) c + Δh_{0}, at most Δh_{max}", "lab")
    # right: height bands above the TIN (heights not to scale)
    x0, x1, gl = 400, 560, 270
    bands = [(gl + 16, gl + 50, "7  low noise (below the ground)", "f3"),
             (gl - 8, gl + 8, "2  ground (within ± 0.3 m)", "f2"),
             (gl - 30, gl - 8, "3  low vegetation (up to 0.5 m)", "fw"),
             (gl - 66, gl - 30, "4  medium (up to 2 m)", "fa"),
             (gl - 186, gl - 66, "5  high (up to 60 m)", "f2"),
             (50, gl - 186, "18  high noise (above the last band)", "f3")]
    for ya, yb, name, cls in bands:
        f.add(f'<rect x="{x0}" y="{min(ya, yb)}" width="{x1 - x0}" height="{abs(yb - ya)}" class="{cls}"/>')
        f.text(x1 + 10, (ya + yb) / 2 + 4, name, "sm")
    f.path(f"M{x0 - 10},{gl} L{x1 + 4},{gl}", "gnd")
    f.text(x0 - 12, gl + 4, "TIN", "e lab")
    # a tree and a building as points
    for k in range(40):
        a = 2 * math.pi * k / 40
        f.dot(450 + 20 * math.cos(a) * (0.4 + 0.6 * (k % 3) / 2), 140 + 30 * math.sin(a) * (0.4 + 0.6 * (k % 2)), 1.8, "wire2")
    f.path(f"M450,{gl} L450,170", "", "stroke:var(--wire2);stroke-width:2")
    f.add(f'<rect x="500" y="{gl - 60}" width="40" height="60" style="fill:none;stroke:var(--ink);stroke-width:1.2;stroke-dasharray:3 2"/>')
    f.text(520, gl - 66, "building", "c sm")
    f.text(20, 412, "Heights not to scale. A building, pole or wire above the ground gets a "
           "vegetation class too.", "lab")
    return f


def swing_band():
    f = Fig("swing-band", 760, 470, "The swing arc as a band",
            "The plane square to the span's axis at a place along it: the wire blown left, in "
            "still air and blown right at their angles and radii; the band from the smallest to "
            "the largest radius between the outermost angles, widened by its margin m; a point q "
            "at radius rho_q and angle phi_q and its distance to the band.")
    f.text(20, 26, "The swing arc: a band about the span's axis", "t")
    cx, cy = 300, 66
    rl, r0, rr = 248, 222, 262
    pl, p0, pr = -38, 5, 42
    r, R = min(rl, r0, rr), max(rl, r0, rr)
    m = 14
    f.poly(band_pts(cx, cy, r - m, R + m, pl - 3.2, pr + 3.2), "dash", close=True)
    f.poly(band_pts(cx, cy, r, R, pl, pr), "", "fill:var(--acc);fill-opacity:.15;stroke:var(--acc);stroke-width:1.4", close=True)
    f.path(f"M{cx},{cy} L{cx},{cy + R + 40}", "thin dt")
    for rho, phi, col in ((rl, pl, "L"), (r0, p0, "0"), (rr, pr, "R")):
        x, y = pol(cx, cy, rho, phi)
        f.path(f"M{cx},{cy} L{x:.1f},{y:.1f}", "thin")
        f.add(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="7" class="wire" style="stroke-width:3;fill:var(--bg)"/>')
    f.add(f'<circle cx="{cx}" cy="{cy}" r="5" style="fill:var(--bg);stroke:var(--ink);stroke-width:2"/>')
    f.text(cx + 12, cy - 6, "C(s): on the axis A_{1}A_{2}, seen along u")
    x, y = pol(cx, cy, rl, pl)
    f.text(x - 14, y + 4, "blown left: φ_{L}, R_{L}", "e")
    x, y = pol(cx, cy, r0, p0)
    f.label(x - 4, y + 6, x - 40, y + 66, "still air: φ_{0}, R_{0}", end=True)
    x, y = pol(cx, cy, rr, pr)
    f.text(x + 14, y + 4, "blown right: φ_{R}, R_{R}")
    f.arc(cx, cy, 70, 90, 90 - pl, "thin")
    f.text(cx - 34, cy + 92, "φ_{L}", "e")
    f.arc(cx, cy, 82, 90 - pr, 90, "thin")
    f.text(cx + 40, cy + 98, "φ_{R}")
    # r and R
    f.label(*pol(cx, cy, r, -14), 150, 205, "inner radius r", end=True)
    f.text(146, 221, "= min(R_{L}, R_{0}, R_{R})", "e")
    f.label(*pol(cx, cy, R, -24), 110, 376, "outer radius R = max(R_{L}, R_{0}, R_{R})", end=False)
    f.label(*pol(cx, cy, R + m, 30), 560, 356, "band margin m (dashed)", end=False)
    f.text(20, 462, "Angles from the downward vertical, positive to the right.", "lab")
    # a point and its distance
    qx, qy = pol(cx, cy, R + 66, 12)
    bx, by = pol(cx, cy, R, 12)
    f.path(f"M{bx:.1f},{by:.1f} L{qx:.1f},{qy:.1f}", "", "stroke:var(--ink);stroke-width:2")
    f.dot(qx, qy, 4.5)
    f.text(qx + 10, qy + 4, "q at ρ_{q}, φ_{q}")
    f.text(qx + 10, qy + 20, "d_{arc} = ρ_{q} − R (outside)", "lab")
    ex, ey = pol(cx, cy, (r + R) / 2, pr)
    f.label(ex + 4, ey - 4, 610, 150, "end edge E_{R}", end=False)
    ex, ey = pol(cx, cy, (r + R) / 2, pl)
    f.label(ex - 4, ey - 4, 90, 150, "end edge E_{L}", end=True)
    f.text(545, 414, "clearance: d_{arc} − r_{c} − m", "")
    f.text(545, 430, "(d_{arc} 0 inside the band; beyond its", "lab")
    f.text(545, 446, "angles, to the nearer end edge)", "lab")
    return f


def fall_in():
    f = Fig("fall-in", 760, 420, "A tree falling about its root ball",
            "In the vertical plane through the tree's base B and the wire point q: the grown "
            "tree of height h' on a straight trunk, the hinge H at the root-ball radius from the "
            "trunk, the trunk's length L from the hinge, the sector it sweeps from its start "
            "angle down to its resting angle on the ground, and the distance from q to the "
            "swept trunk.")
    f.text(20, 26, "Fall-in: a tree pivoting about its root ball", "t")
    g = 340
    bx = 170
    hp = 230.0
    br = 26.0
    Hx = bx + br
    L = math.hypot(hp, br)
    th0 = math.degrees(math.atan2(hp, -br))
    thmin = -6.0
    # ground: level, then falling away beyond the reach
    G = [(30, g), (Hx, g), (740, g - (740 - Hx) * math.tan(math.radians(thmin)))]
    f.poly(G, "gnd")
    pts = [(Hx, g)] + [(Hx + L * math.cos(math.radians(a)), g - L * math.sin(math.radians(a)))
                       for a in [thmin + (th0 - thmin) * k / 60 for k in range(61)]]
    f.poly(pts, "", "fill:var(--wire2);fill-opacity:.13;stroke:var(--wire2);stroke-width:1.2", close=True)
    # the trunk, standing and at rest
    f.path(f"M{bx},{g} L{bx},{g - hp}", "", "stroke:var(--wire2);stroke-width:4")
    rx, ry = Hx + L * math.cos(math.radians(thmin)), g - L * math.sin(math.radians(thmin))
    f.path(f"M{Hx},{g} L{rx:.1f},{ry:.1f}", "", "stroke:var(--wire2);stroke-width:2;stroke-dasharray:6 4")
    f.dot(bx, g, 4)
    f.dot(Hx, g, 4, "wire3")
    f.text(bx - 8, g + 18, "B (base)", "e")
    f.label(Hx, g, Hx + 30, g + 36, "H: hinge, b_{r} = k h′ from the trunk", "")
    f.dot(bx, g - hp + 22, 4, "acc")
    f.label(bx, g - hp + 22, bx - 40, g - hp + 50, "the point:", end=True)
    f.text(bx - 44, g - hp + 70, "h above the ground", "e")
    f.vdim2(bx - 26, g - hp + 22, g - hp, "g", left=True)
    f.text(bx + 8, g - hp - 6, "T: top of the grown tree, h′ = h + g")
    f.path(f"M{Hx},{g} L{bx},{g - hp}", "thin ds")
    f.text(Hx + 10, g - hp / 2 + 30, "L = √(h′² + b_{r}²)", "")
    f.arc(Hx, g, 44, -th0, 0, "thin")
    f.text(Hx + 34, g - 44, "θ_{0}", "")
    f.label(rx - 40, ry - 3, rx + 20, g - 70, "at rest on the ground: θ_{min}", "")
    # the wire point
    qx, qy = 520, 150
    f.add(f'<circle cx="{qx}" cy="{qy}" r="8" class="wire" style="stroke-width:3;fill:var(--bg)"/>')
    f.text(qx + 14, qy - 8, "q: a wire point")
    d = math.hypot(qx - Hx, g - qy)
    ux, uy = (qx - Hx) / d, (qy - g) / d
    ax, ay = Hx + L * ux, g + L * uy
    f.path(f"M{Hx},{g} L{ax:.1f},{ay:.1f}", "thin dt")
    f.path(f"M{ax:.1f},{ay:.1f} L{qx - 8 * ux:.1f},{qy - 8 * uy:.1f}", "", "stroke:var(--ink);stroke-width:2.2")
    f.label((ax + qx) / 2, (ay + qy) / 2, 600, 220, "d_{fall} = |q − H| − L")
    f.text(604, 238, "(within the sector's angles)", "lab")
    f.text(420, 60, "A danger tree when d_{fall} − r < A", "t")
    f.text(420, 78, "(r: the conductor's radius and the polyline's bound;", "lab")
    f.text(420, 94, " A: the clearance allowance)", "lab")
    f.text(20, 412, "x towards q, z up, from B. With k = 0 on level ground: a sphere of radius h′ about the base.",
           "lab")
    return f


# =====================================================================================
# Safety volumes
# =====================================================================================
def safety_section():
    f = Fig("safety-section", 760, 340, "A safety volume cut at mid-span",
            "The section of a safety volume at mid-span about a level axis: the band from r = 8 "
            "m to R = 10 m between −40° and +40°, and the volume within D = 2.15 m of it, "
            "reaching 8.578 m to each side, 12.15 m below the axis and up to 3.978 m below "
            "it at the band's inner corners.")
    f.text(20, 26, "A safety volume at mid-span (the page's check by hand, to scale)", "t")
    s = 20.0
    cx, cy = 290, 62
    r, R, D, a = 8 * s, 10 * s, 2.15 * s, 40.0
    out = []
    out += [pol(cx, cy, R + D, -a + 2 * a * k / 60) for k in range(61)]
    cr = pol(cx, cy, R, a)
    out += [pol(cr[0], cr[1], D, a + 90 * k / 12) for k in range(13)]
    ci = pol(cx, cy, r, a)
    out += [pol(ci[0], ci[1], D, a + 90 + 90 * k / 12) for k in range(13)]
    out += [pol(cx, cy, r - D, a - 2 * a * k / 60) for k in range(61)]
    ci = pol(cx, cy, r, -a)
    out += [pol(ci[0], ci[1], D, -a - 180 + 90 * k / 12) for k in range(13)]
    cr = pol(cx, cy, R, -a)
    out += [pol(cr[0], cr[1], D, -a - 90 + 90 * k / 12) for k in range(13)]
    f.poly(out, "", "fill:var(--wire2);fill-opacity:.12;stroke:var(--wire2);stroke-width:1.8", close=True)
    f.poly(band_pts(cx, cy, r, R, -a, a), "", "fill:var(--acc);fill-opacity:.25;stroke:var(--acc);stroke-width:1.2", close=True)
    for rho, phi in ((R, -a), (r, 0), (R, a)):
        x, y = pol(cx, cy, rho, phi)
        f.path(f"M{cx},{cy} L{x:.1f},{y:.1f}", "thin")
        f.add(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="5" class="wire" style="stroke-width:2.5;fill:var(--bg)"/>')
    f.add(f'<circle cx="{cx}" cy="{cy}" r="4.5" style="fill:var(--bg);stroke:var(--ink);stroke-width:2"/>')
    f.text(cx - 10, cy - 8, "axis", "e")
    # the page's numbers
    ry_ = cy + R * math.cos(math.radians(a))
    f.path(f"M{cx},{cy} L{cx},{cy + R + D + 12}", "thin dt")
    f.hdim2(cx, cx + (R * math.sin(math.radians(a)) + D), ry_, "")
    f.text(cx + (R * math.sin(math.radians(a)) + D) + 8, ry_ + 4, "8.578 m")
    f.vdim2(cx - 200, cy, cy + R + D, "12.15 m", left=True)
    f.path(f"M{cx - 206},{cy} L{cx - 6},{cy} M{cx - 206},{cy + R + D} L{cx},{cy + R + D}", "thin dt")
    tx = cx + r * math.sin(math.radians(a))
    f.vdim2(tx + 28, cy, cy + r * math.cos(math.radians(a)) - D, "3.978 m")
    f.path(f"M{tx},{cy + r * math.cos(math.radians(a)) - D:.1f} L{tx + 34},{cy + r * math.cos(math.radians(a)) - D:.1f}", "thin dt")
    x = 548
    f.text(x, 110, "band: r = 8 m (still air)", "")
    f.text(x, 128, "to R = 10 m (blown), −40° to +40°", "")
    f.text(x, 156, "D = 2.15 m: d = 2 m and", "")
    f.text(x, 174, "the band's margin", "")
    f.add(f'<rect x="{x}" y="200" width="22" height="12" style="fill:var(--acc);fill-opacity:.25;stroke:var(--acc)"/>')
    f.text(x + 30, 210, "the band", "lab")
    f.add(f'<rect x="{x}" y="222" width="22" height="12" style="fill:var(--wire2);fill-opacity:.12;stroke:var(--wire2)"/>')
    f.text(x + 30, 232, "the volume: within D of it", "lab")
    return f


def jumper_gap():
    f = Fig("safety-jumper", 760, 340, "The jumper closes the gap",
            "Two sections end at dead-end clamps 6 m apart either side of a structure, d = 1 m: "
            "alone their tubes leave a gap of 4 m; with a jumper sagging 1.5 m between the "
            "clamps the volume is one closed piece.")
    f.text(20, 26, "The safety volume at a dead end (side view, to scale)", "t")
    s = 30.0
    for k, (y, with_j) in enumerate(((110, False), (250, True))):
        cl, cr = 380 - 3 * s, 380 + 3 * s
        pieces = [[(30, y), (cl, y)], [(cr, y), (730, y)]]
        if with_j:
            pieces.append(parab(cl, y, cr, y, 1.5 * s, 30))
        f.path(f"M380,{y - 40} L380,{y + 74}", "st")
        for col, wd in (("--muted", 2 * s + 2), ("--fill", 2 * s - 1.5)):
            for pts in pieces:
                d = "M" + " L".join(f"{x:.1f},{yy:.1f}" for x, yy in pts)
                f.add(f'<path d="{d}" style="fill:none;stroke:var({col});stroke-width:{wd};'
                      f'stroke-linecap:round;stroke-linejoin:round"/>')
        for pts in pieces:
            f.poly(pts, "wire")
        for x in (cl, cr):
            f.dot(x, y, 4, "wire")
        if not with_j:
            f.hdim2(cl + s, cr - s, y + 4, "")
            f.text(cl + s + 10, y + 22, "gap 6 − 2 = 4 m", "")
            f.hdim2(cl, cr, y - 44, "clamps 6 m apart")
            f.vdim2(120, y, y - s, "d = 1 m")
            f.text(30, y - 46, "without the jumper: two pieces", "lab")
        else:
            f.text(30, y - 46, "with a jumper (sag 1.5 m): one closed piece", "lab")
    return f


# =====================================================================================
# Drawings
# =====================================================================================
def sheet_ranges():
    f = Fig("sheet-ranges", 760, 330, "The profile's height range on a sheet",
            "Three profile panels of the same vertical scale: everything fits; only the "
            "essentials fit, the structures' tops cut with break marks; not even the essentials "
            "fit, the datum stepped at a structure with a double line and its own height labels "
            "each side.")
    f.text(20, 26, "The profile's height range (the vertical scale never changes)", "t")
    W, H, top = 220, 200, 60
    for k in range(3):
        x0 = 20 + k * 250
        f.add(f'<rect x="{x0}" y="{top}" width="{W}" height="{H}" style="fill:none;stroke:var(--muted);stroke-width:1.2"/>')
        if k == 0:
            G = [(x0, 210), (x0 + 110, 200), (x0 + W, 214)]
            S = [(x0 + 30, 205, 120), (x0 + 190, 211, 120)]
            cut = False
        elif k == 1:
            G = [(x0, 220), (x0 + 110, 232), (x0 + W, 214)]
            S = [(x0 + 30, 222, 190), (x0 + 190, 216, 190)]
            cut = True
        else:
            G = []
            S = None
            cut = False
        if S is not None:
            f.poly(G, "gnd")
            f.poly([(x, y - 18) for x, y in G], "acc ds")
            for x, gy_, h in S:
                yt = max(top, gy_ - h)
                f.path(f"M{x},{gy_ - 2} L{x},{yt}", "st")
                if cut and gy_ - h < top:
                    f.path(f"M{x - 8},{top + 8} L{x - 3},{top + 4} L{x + 3},{top + 12} L{x + 8},{top + 8}", "",
                           "stroke:var(--ink);stroke-width:1.6;fill:none")
            (xa, ga, ha), (xb, gb, hb) = S
            ya, yb = ga - ha + 14, gb - hb + 14
            pts = parab(xa, ya, xb, yb, 60, 40)
            pts = [(x, max(y, top)) for x, y in pts]
            f.poly(pts, "wire")
        else:
            xs = x0 + 110
            # each part its own height range: the ground at about the same place on the paper
            for (xa, ga, xb, gb, ha, hb) in ((x0, 236, xs - 3, 222, 120, 126), (xs + 3, 238, x0 + W, 226, 126, 118)):
                f.poly([(xa, ga), (xb, gb)], "gnd")
                f.poly([(xa, ga - 18), (xb, gb - 18)], "acc ds")
                pa = xa + 22 if xa == x0 else xs + 8
                pb = xb - 8 if xb == xs - 3 else x0 + W - 22
                gpa = ga + (gb - ga) * (pa - xa) / (xb - xa)
                gpb = ga + (gb - ga) * (pb - xa) / (xb - xa)
                f.path(f"M{pa:.1f},{gpa:.1f} L{pa:.1f},{gpa - ha:.1f} M{pb:.1f},{gpb:.1f} L{pb:.1f},{gpb - hb:.1f}", "st")
                f.poly(parab(pa, gpa - ha + 12, pb, gpb - hb + 12, 46, 20), "wire")
                for yy in (top + 30, top + 80, top + 130):
                    f.path(f"M{xa},{yy} L{xa + 6},{yy}", "thin")
            f.path(f"M{xs - 3},{top} L{xs - 3},{top + H} M{xs + 3},{top} L{xs + 3},{top + H}", "",
                   "stroke:var(--ink);stroke-width:1.2")
        f.text(x0, top + H + 22, ["1  everything fits", "2  only the essentials fit", "3  the datum stepped"][k], "t")
        for i, line in enumerate([["ground, clearance line, wires,", "structures and their names"],
                                  ["ground, clearance line, lowest", "points; tops cut, break marks"],
                                  ["split at a structure: a double", "line, own height labels"]][k]):
            f.text(x0, top + H + 40 + 16 * i, line, "lab")
    return f


def lowest_line():
    f = Fig("lowest-line", 760, 470, "The line below the lowest conductor",
            "Above, the plan: the corridor of half width w about the centreline, its near side "
            "on the right looking along increasing stations, and the LiDAR points kept within "
            "it. Below, the profile: the circuit's wires, their lower envelope, and the line d "
            "below it.")
    f.text(20, 26, "Plan: the corridor; profile: the line below the lowest conductor", "t")
    # plan
    f.text(20, 50, "plan", "lab")
    f.path("M30,95 L640,95", "thin")
    f.path("M30,62 L640,62 M30,128 L640,128", "dash")
    f.text(648, 66, "− w (left)", "lab")
    f.text(648, 99, "centreline", "lab")
    f.text(648, 132, "+ w (right)", "lab")
    f.text(30, 156, "near side: the right, looking along increasing stations (the plan's lower edge); "
           "points kept within w", "lab")
    rnd = random.Random(7)
    for _ in range(64):
        x, y = rnd.uniform(40, 630), rnd.uniform(48, 142)
        if 62 <= y <= 128:
            f.dot(x, y, 2.2, "wire2")
        else:
            f.add(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="2.2" style="fill:none;stroke:var(--muted);stroke-width:1"/>')
    # profile
    f.text(20, 190, "profile", "lab")
    G = [(30, 430), (150, 422), (260, 402), (380, 412), (480, 386), (600, 402), (730, 416)]
    f.poly(G, "gnd")
    xs = [60, 380, 700]

    def gat(x):
        for (a, b), (c, d) in zip(G, G[1:]):
            if a <= x <= c:
                return b + (d - b) * (x - a) / (c - a)
        return G[-1][1]

    up = [(xs[0], 214), (xs[1], 220), (xs[2], 210)]
    lo = [(xs[0], 246), (xs[1], 252), (xs[2], 242)]
    for x in xs:
        f.path(f"M{x},{gat(x)} L{x},200", "st")
    env = []
    for (p, q), (r_, t_) in zip(zip(up, up[1:]), zip(lo, lo[1:])):
        a = parab(*p, *q, 92, 40)
        b = parab(*r_, *t_, 44, 40)
        f.poly(a, "wire")
        f.poly(b, "wire")
        env += [(x, max(y1, y2)) for (x, y1), (_, y2) in zip(a, b)]
    d = 46
    f.poly([(x, y + d) for x, y in env], "", "stroke:var(--wire3);stroke-width:2.4;fill:none")
    f.poly(env, "", "stroke:var(--ink);stroke-width:2;fill:none;stroke-dasharray:2 3")
    xm, ym = env[20]
    f.vdim2(xm, ym, ym + d, "d")
    f.label(env[52][0], env[52][1] + d, 470, 362, "z(s) = min z_{wire}(s) − d", "")
    f.label(env[8][0], env[8][1], 170, 200, "lower envelope of the circuit's wires", end=False)
    f.text(30, 460, "d: the ground rule's vertical clearance at the circuit's voltage; jumpers left out", "lab")
    return f


def vertical_radial():
    f = Fig("vertical-radial", 760, 380, "Vertical and radial",
            "Left, across the line: a vertical rule checks the targets within the zone's half "
            "width w of the wire in plan and measures straight up to the wire; a target farther "
            "out is left out, though a radial rule measures to it. Right, along the line: over "
            "the slice of depth d the wire rising by m per metre is taken m d/2 lower.")
    f.text(20, 26, "Vertical and radial (left: across the line; right: along it)", "t")
    s = 80
    wx, wy = 190, 90
    g = 340
    ground(f, 30, 380, g)
    f.add(f'<rect x="{wx - s}" y="{wy + 10}" width="{2 * s}" height="{g - wy - 10}" class="fa"/>')
    f.path(f"M{wx - s},{wy + 10} L{wx - s},{g} M{wx + s},{wy + 10} L{wx + s},{g}", "acc ds")
    f.add(f'<circle cx="{wx}" cy="{wy}" r="8" class="wire" style="stroke-width:3;fill:var(--bg)"/>')
    f.hdim2(wx, wx + s, wy + 30, "w")
    p1 = (wx + 0.9 * s, 300)
    p2 = (wx + 1.1 * s, 210)
    f.path(f"M{p1[0]},{p1[1]} L{p1[0]},{wy}", "", "stroke:var(--ink);stroke-width:2")
    f.path(f"M{wx + 8},{wy} L{p1[0] + 6},{wy}", "thin dt")
    f.dot(*p1, 4.5)
    f.add(f'<circle cx="{p2[0]}" cy="{p2[1]}" r="4.5" style="fill:var(--bg);stroke:var(--ink);stroke-width:1.6"/>')
    dx, dy = p2[0] - wx, p2[1] - wy
    n = math.hypot(dx, dy)
    f.path(f"M{wx + 9 * dx / n:.1f},{wy + 9 * dy / n:.1f} L{p2[0]:.1f},{p2[1]:.1f}", "", "stroke:var(--wire3);stroke-width:1.8;stroke-dasharray:6 4")
    f.label(p1[0], p1[1], 300, 300, "0.9 m beside:")
    f.text(304, 316, "checked, drawn vertical")
    f.label(p2[0], p2[1], 300, 196, "1.1 m beside:")
    f.text(304, 212, "left out of the vertical")
    f.text(304, 228, "(a radial rule: the shortest", "lab")
    f.text(304, 244, "3D distance)", "lab")
    f.text(30, 362, "the zone of a vertical rule (rule 10 m, w = 1 m)", "lab")
    # right: along the line
    x0, x1 = 430, 730
    ground(f, x0, x1, g)
    m = 0.22
    xa, xb = 520, 660
    xt = (xa + xb) / 2
    y = lambda x: 160 - m * (x - x0)  # noqa: E731
    f.path(f"M{x0},{y(x0):.1f} L{x1},{y(x1):.1f}", "wire")
    f.add(f'<rect x="{xa}" y="70" width="{xb - xa}" height="{g - 70}" class="fa"/>')
    f.path(f"M{xa},70 L{xa},{g} M{xb},70 L{xb},{g}", "acc ds")
    f.hdim2(xa, xb, 82, "depth d")
    f.dot(xt, 300, 4.5)
    f.text(xt + 8, 318, "target")
    f.path(f"M{xa},{y(xa):.1f} L{xt + 30},{y(xa):.1f}", "thin dt")
    f.path(f"M{xt},{y(xt):.1f} L{xt + 30},{y(xt):.1f}", "thin dt")
    f.vdim2(xt + 26, y(xt), y(xa), "")
    f.text(xt + 34, (y(xt) + y(xa)) / 2 + 2, "m d/2", "sm")
    f.path(f"M{xt},300 L{xt},{y(xa):.1f}", "", "stroke:var(--ink);stroke-width:2")
    f.text(x0, 112, "wire rising m per metre", "lab")
    f.label(xa, y(xa), 456, 236, "the wire's lowest point", "lab", end=False)
    f.text(460, 252, "over the slice", "lab")
    return f


def vertical_arc():
    f = Fig("vertical-arc", 760, 420, "The vertical distance to the arc",
            "The page's check by hand at mid-span, to scale: the axis 20 m up, the band from r = "
            "4 m to R = 5 m between −40° and +40°; for a point 4 m right at 10 m, the band's "
            "lowest point within w = 1 m across is at 36.87° and 16 m high, 6 m above it; for "
            "a point 2 m below the axis, the band's highest point within the strip is on the "
            "inner circle at 16.127 m, 1.873 m below it.")
    f.text(20, 26, "The vertical distance to the band (the page's check by hand, to scale)", "t")
    s = 26.0
    cx, cy = 330, 66

    def P(xm, zm):
        return cx + xm * s, cy + (20 - zm) * s

    for (xl, xr) in ((3, 5), (-1, 1)):
        f.add(f'<rect x="{P(xl, 0)[0]:.1f}" y="{cy}" width="{(xr - xl) * s:.1f}" height="{11 * s:.1f}" class="fa"/>')
    f.poly(band_pts(cx, cy, 4 * s, 5 * s, -40, 40), "", "fill:var(--wire);fill-opacity:.18;stroke:var(--wire);stroke-width:1.6", close=True)
    f.add(f'<circle cx="{cx}" cy="{cy}" r="4.5" style="fill:var(--bg);stroke:var(--ink);stroke-width:2"/>')
    f.text(cx - 10, cy + 4, "axis, 20 m", "e")
    # q1
    q1 = P(4, 10)
    low = P(3, 16)
    f.dot(*q1, 4.5)
    f.text(q1[0] - 10, q1[1] + 4, "q: 4 m right, at 10 m", "e")
    f.path(f"M{low[0]:.1f},{low[1]:.1f} L{q1[0] + 20:.1f},{low[1]:.1f}", "thin dt")
    f.dot(*low, 3.5, "ink")
    f.path(f"M{q1[0]},{q1[1]} L{q1[0]},{low[1]}", "", "stroke:var(--ink);stroke-width:2")
    f.text(q1[0] + 8, (q1[1] + low[1]) / 2, "6 m")
    f.label(low[0] - 2, low[1] + 2, 200, 250, "lowest point in the strip:", end=True)
    f.text(196, 266, "36.87°, 16 m", "e")
    # q2
    q2 = P(0, 18)
    hi = P(1, 20 - 4 * math.cos(math.asin(0.25)))
    f.dot(*q2, 4.5)
    f.label(q2[0] - 3, q2[1], 220, 116, "q: 2 m below the axis, at 18 m", end=True)
    f.path(f"M{hi[0]:.1f},{hi[1]:.1f} L{q2[0]:.1f},{hi[1]:.1f}", "thin dt")
    f.dot(*hi, 3.5, "ink")
    f.path(f"M{q2[0]},{q2[1]} L{q2[0]},{hi[1]:.1f}", "", "stroke:var(--ink);stroke-width:2")
    f.text(q2[0] - 8, (q2[1] + hi[1]) / 2 + 8, "1.873 m", "e sm")
    f.hdim2(*[P(x, 0)[0] for x in (3, 5)], cy + 11 * s + 14, "2w", above=False)
    f.hdim2(*[P(x, 0)[0] for x in (-1, 1)], cy + 11 * s + 14, "2w", above=False)
    x = 490
    f.text(x, 220, "band: r = 4 m (still air) to R = 5 m", "")
    f.text(x, 238, "(blown ±40°); w = 1 m", "")
    f.text(x, 268, "q at 10 m: sin φ in [0.6, 1], the lowest", "lab")
    f.text(x, 284, "point at 36.87°: vertical 6 m", "lab")
    f.text(x, 300, "(radially 5.770 m)", "lab")
    f.text(x, 328, "q at 18 m: above the band; its highest", "lab")
    f.text(x, 344, "point within 1 m across on the inner", "lab")
    f.text(x, 360, "circle at sin φ = 1/4, 16.127 m:", "lab")
    f.text(x, 376, "vertical 1.873 m (radially 2.0 m)", "lab")
    return f


# =====================================================================================
# Wire tensions
# =====================================================================================
def max_tension():
    f = Fig("max-tension", 760, 370, "The most tensioned support",
            "An inclined span: the horizontal tension H at the low point z_0, and the tension at "
            "the higher support z_s, T = H + w (z_s − z_0), the larger of the span's two end "
            "tensions, checked against f R.")
    f.text(20, 26, "The maximum tension of a span: at its most tensioned support", "t")
    A, B = (90.0, 210.0), (640.0, 90.0)
    g = 300
    ground(f, 40, 720, g)
    f.path(f"M{A[0]},{g} L{A[0]},{A[1]} M{B[0]},{g} L{B[0]},{B[1]}", "st")
    sag = 80
    pts = parab(*A, *B, sag, 80)
    f.poly(pts, "wire")
    t = 0.5 + (B[1] - A[1]) / (8 * sag)
    lx = A[0] + (B[0] - A[0]) * t
    ly = A[1] + (B[1] - A[1]) * t + 4 * sag * t * (1 - t)
    f.dot(lx, ly, 4)
    f.arrow(lx, ly, lx - 60, ly, "acc", 2)
    f.arrow(lx, ly, lx + 60, ly, "acc", 2)
    f.text(lx + 66, ly + 4, "H")
    f.text(lx - 6, ly + 22, "low point z_{0}", "c lab")
    f.path(f"M{lx + 84},{ly} L{B[0] + 60},{ly}", "thin dt")
    f.path(f"M{B[0]},{B[1]} L{B[0] + 60},{B[1]}", "thin dt")
    f.vdim2(B[0] + 50, ly, B[1], "z_{s} − z_{0}")
    f.add(f'<circle cx="{B[0]}" cy="{B[1]}" r="11" style="fill:none;stroke:var(--wire3);stroke-width:2.5"/>')
    gf.ap(f, *B)
    gf.ap(f, *A)
    f.text(B[0] - 16, B[1] - 16, "most tensioned support (the higher end)", "e")
    f.text(20, 336, "T = H + w (z_{s} − z_{0})   (by finite elements: the largest element end tension)", "")
    f.text(20, 356, "checked: T ≤ f R, usage T / (f R); R: the wire type's RTS, f: the rule's factor", "lab")
    return f


FIGURES = [catenary_span, weather_loads, transit, catenary_element, fe_section, span_cases,
           jumper_rod, jumper_support, heat_balance, temperature_limit, spotting_profile,
           wind_weight_span, capped_law, x_panel, pole_check, member_wind, face_solidity,
           shielding_plan, galloping_ellipse, galloping_loops, galloping_gap, lidar_classes,
           swing_band, fall_in, safety_section, jumper_gap, sheet_ranges, lowest_line,
           vertical_radial, vertical_arc, max_tension]


def main(argv):
    out = "docs/images/theory"
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
