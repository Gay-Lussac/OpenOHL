#!/usr/bin/env python3
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""Make what the Windows installer needs besides the programs.

Standard library only; runs on any system:

    python3 -I packaging/windows/make_assets.py OUTDIR

Writes into OUTDIR (nothing of it is kept in the repository):

    ohl-design.ico      from assets/brand/ohl-design-icon.svg
    ohl-structure.ico   from assets/brand/ohl-structure-icon.svg
    License.rtf         the LICENSE file as the installer's licence page

Each .ico holds the icon at 16, 20, 24, 32, 40, 48, 64 and 256 pixels
(the sizes Windows asks for), drawn at each size from the SVG, not
scaled down from one picture.

The SVG reader knows only what the two application icons use: one
`<svg>` with a `viewBox`, `<g>` groups that hand down their presentation
attributes, `<rect>` (with `rx`) filled with a plain colour, and `<path>`
drawn as lines (M L H V C S Q T Z, absolute or relative) with round caps
and round joins, colours as `#rrggbb`, and `opacity` on a path or rect.
Anything else (a filled path, an arc, a gradient, a transform, text)
stops the script with a message that names it, rather than drawing a
wrong icon: after redrawing an icon with such a feature, extend this
reader or make the .ico with another tool.
"""

import math
import os
import re
import struct
import sys
import xml.etree.ElementTree as ET
import zlib

SIZES = (16, 20, 24, 32, 40, 48, 64, 256)
ICONS = (
    ("assets/brand/ohl-design-icon.svg", "ohl-design.ico"),
    ("assets/brand/ohl-structure-icon.svg", "ohl-structure.ico"),
)
SVG = "{http://www.w3.org/2000/svg}"
# Presentation attributes a group hands down to what it holds.
INHERITED = ("fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin")
# Pieces a curve is cut into.
CURVE_STEPS = 24


class Unsupported(Exception):
    """Something in the SVG this reader does not draw."""


def colour(text, what):
    """`#rrggbb` (or `#rgb`) as three numbers 0..1; None for `none`."""
    if text == "none":
        return None
    m = re.fullmatch(r"#([0-9a-fA-F]{6}|[0-9a-fA-F]{3})", text.strip())
    if not m:
        raise Unsupported(f"{what}: colour {text!r} (only #rrggbb and none)")
    h = m.group(1)
    if len(h) == 3:
        h = "".join(c * 2 for c in h)
    return tuple(int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4))


def number(text, what):
    try:
        return float(text)
    except ValueError:
        raise Unsupported(f"{what}: {text!r} is not a plain number") from None


def polylines(d):
    """The path data as polylines (lists of points), curves cut in pieces."""
    tokens = re.findall(r"[A-Za-z]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?", d)
    leftover = re.sub(r"[A-Za-z]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?|[\s,]", "", d)
    if leftover:
        raise Unsupported(f"path data: cannot read {leftover!r}")
    out = []
    cur = None  # the polyline being drawn
    x = y = 0.0
    start = (0.0, 0.0)
    last_ctrl = None  # the last control point, for S and T
    last_cmd = ""
    i = 0
    cmd = ""

    def take(n):
        nonlocal i
        vals = tokens[i : i + n]
        if len(vals) < n or any(v.isalpha() for v in vals):
            raise Unsupported(f"path data: '{cmd}' needs {n} numbers")
        i += n
        return [float(v) for v in vals]

    def curve(points):
        # A Bézier curve of any degree, by de Casteljau.
        for k in range(1, CURVE_STEPS + 1):
            t = k / CURVE_STEPS
            pts = points
            while len(pts) > 1:
                pts = [
                    (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)
                    for a, b in zip(pts, pts[1:])
                ]
            cur.append(pts[0])

    while i < len(tokens):
        if tokens[i].isalpha():
            cmd = tokens[i]
            i += 1
        elif cmd in ("M", "m"):
            # More pairs after a move are lines.
            cmd = "L" if cmd == "M" else "l"
        elif not cmd:
            raise Unsupported("path data: does not begin with a command")
        rel = cmd.islower()
        c = cmd.upper()
        ox, oy = (x, y) if rel else (0.0, 0.0)
        if c == "Z":
            if cur:
                cur.append(start)
            x, y = start
            cur = None
        elif c == "M":
            px, py = take(2)
            x, y = ox + px, oy + py
            start = (x, y)
            cur = [(x, y)]
            out.append(cur)
        else:
            if cur is None:
                cur = [(x, y)]
                out.append(cur)
            if c == "L":
                px, py = take(2)
                x, y = ox + px, oy + py
                cur.append((x, y))
            elif c == "H":
                x = ox + take(1)[0]
                cur.append((x, y))
            elif c == "V":
                y = oy + take(1)[0]
                cur.append((x, y))
            elif c in ("C", "S", "Q", "T"):
                if c == "C":
                    v = take(6)
                    ctrl = [(ox + v[0], oy + v[1]), (ox + v[2], oy + v[3])]
                    end = (ox + v[4], oy + v[5])
                elif c == "S":
                    v = take(4)
                    first = (x, y)
                    if last_cmd in ("C", "S") and last_ctrl:
                        first = (2 * x - last_ctrl[0], 2 * y - last_ctrl[1])
                    ctrl = [first, (ox + v[0], oy + v[1])]
                    end = (ox + v[2], oy + v[3])
                elif c == "Q":
                    v = take(4)
                    ctrl = [(ox + v[0], oy + v[1])]
                    end = (ox + v[2], oy + v[3])
                else:
                    v = take(2)
                    first = (x, y)
                    if last_cmd in ("Q", "T") and last_ctrl:
                        first = (2 * x - last_ctrl[0], 2 * y - last_ctrl[1])
                    ctrl = [first]
                    end = (ox + v[0], oy + v[1])
                curve([(x, y)] + ctrl + [end])
                last_ctrl = ctrl[-1]
                x, y = end
            else:
                raise Unsupported(f"path data: command '{cmd}'")
        last_cmd = c
    return [p for p in out if p]


def shapes(svg_path):
    """The drawing as (view box, list of shapes), in drawing order.

    A shape is ("rect", colour, opacity, x, y, w, h, r) or
    ("stroke", colour, opacity, half width, polylines).
    """
    root = ET.parse(svg_path).getroot()
    if root.tag != SVG + "svg":
        raise Unsupported("the file is not an SVG")
    for a in root.attrib:
        if a not in ("viewBox", "width", "height", "role", "aria-label"):
            raise Unsupported(f"<svg>: attribute {a}")
    box = [number(v, "viewBox") for v in root.get("viewBox", "").replace(",", " ").split()]
    if len(box) != 4 or box[2] <= 0 or box[2] != box[3]:
        raise Unsupported("<svg>: needs a square viewBox")
    found = []

    def walk(node, inherited):
        for el in node:
            tag = el.tag.replace(SVG, "")
            if tag in ("title", "desc"):
                continue
            style = dict(inherited)
            own = dict(el.attrib)
            for a in INHERITED:
                if a in own:
                    style[a] = own.pop(a)
            opacity = number(own.pop("opacity", "1"), f"<{tag}> opacity") if tag != "g" else 1.0
            if tag == "g":
                if own:
                    raise Unsupported(f"<g>: attribute {sorted(own)[0]}")
                walk(el, style)
            elif tag == "rect":
                geo = {k: number(own.pop(k, "0"), f"<rect> {k}") for k in ("x", "y", "width", "height", "rx")}
                if "ry" in own and number(own.pop("ry"), "<rect> ry") != geo["rx"]:
                    raise Unsupported("<rect>: ry different from rx")
                if own:
                    raise Unsupported(f"<rect>: attribute {sorted(own)[0]}")
                if colour(style.get("stroke", "none"), "<rect> stroke") is not None:
                    raise Unsupported("<rect>: an outline (only filled rectangles)")
                fill = colour(style.get("fill", "#000000"), "<rect> fill")
                if fill is not None:
                    r = min(geo["rx"], geo["width"] / 2, geo["height"] / 2)
                    found.append(("rect", fill, opacity, geo["x"], geo["y"], geo["width"], geo["height"], r))
            elif tag == "path":
                d = own.pop("d", "")
                if own:
                    raise Unsupported(f"<path>: attribute {sorted(own)[0]}")
                if colour(style.get("fill", "#000000"), "<path> fill") is not None:
                    raise Unsupported('<path>: a filled path (only lines, fill="none")')
                for a in ("stroke-linecap", "stroke-linejoin"):
                    if style.get(a) != "round":
                        raise Unsupported(f"<path>: {a} must be round")
                stroke = colour(style.get("stroke", "none"), "<path> stroke")
                width = number(style.get("stroke-width", "1"), "<path> stroke-width")
                if stroke is not None and width > 0:
                    found.append(("stroke", stroke, opacity, width / 2, polylines(d)))
            else:
                raise Unsupported(f"<{tag}>")

    walk(root, {})
    if not found:
        raise Unsupported("nothing to draw")
    return box, found


def render(box, drawing, size):
    """The drawing at size x size pixels: bytes R G B A, rows from the top."""
    ss = 4 if size >= 128 else 8  # samples per pixel, each way
    n = size * ss
    scale = n / box[2]
    # Premultiplied colour per sample.
    red, green, blue, alpha = ([0.0] * (n * n) for _ in range(4))

    def to_samples(px, py):
        # Drawing units to sample coordinates (sample k covers k..k+1).
        return (px - box[0]) * scale, (py - box[1]) * scale

    for shape in drawing:
        mask = bytearray(n * n)
        rgb, opacity = shape[1], shape[2]
        if shape[0] == "rect":
            x0, y0 = to_samples(shape[3], shape[4])
            x1, y1 = to_samples(shape[3] + shape[5], shape[4] + shape[6])
            r = shape[7] * scale
            for row in range(max(0, int(y0)), min(n, int(math.ceil(y1)))):
                cy = row + 0.5
                if not y0 <= cy <= y1:
                    continue
                inset = 0.0
                dy = max(y0 + r - cy, cy - (y1 - r))
                if dy > 0:
                    inset = r - math.sqrt(max(r * r - dy * dy, 0.0))
                a = max(0, int(math.ceil(x0 + inset - 0.5)))
                b = min(n, int(math.floor(x1 - inset - 0.5)) + 1)
                if b > a:
                    mask[row * n + a : row * n + b] = b"\x01" * (b - a)
        else:
            r = shape[3] * scale
            rr = r * r
            for line in shape[4]:
                pts = [to_samples(*p) for p in line]
                if len(pts) == 1:
                    pts = pts * 2
                for (ax, ay), (bx, by) in zip(pts, pts[1:]):
                    dx, dy = bx - ax, by - ay
                    length2 = dx * dx + dy * dy
                    for row in range(max(0, int(min(ay, by) - r)), min(n, int(max(ay, by) + r) + 1)):
                        cy = row + 0.5
                        base = row * n
                        for col in range(max(0, int(min(ax, bx) - r)), min(n, int(max(ax, bx) + r) + 1)):
                            if mask[base + col]:
                                continue
                            cx = col + 0.5
                            t = 0.0
                            if length2 > 0:
                                t = ((cx - ax) * dx + (cy - ay) * dy) / length2
                                t = 0.0 if t < 0 else 1.0 if t > 1 else t
                            ex, ey = cx - (ax + t * dx), cy - (ay + t * dy)
                            if ex * ex + ey * ey <= rr:
                                mask[base + col] = 1
        sr, sg, sb = (c * opacity for c in rgb)
        keep = 1.0 - opacity
        k = mask.find(1)
        while k >= 0:
            red[k] = sr + red[k] * keep
            green[k] = sg + green[k] * keep
            blue[k] = sb + blue[k] * keep
            alpha[k] = opacity + alpha[k] * keep
            k = mask.find(1, k + 1)

    out = bytearray(size * size * 4)
    per = ss * ss
    for py in range(size):
        for px in range(size):
            r = g = b = a = 0.0
            for sy in range(py * ss, py * ss + ss):
                o = sy * n + px * ss
                r += sum(red[o : o + ss])
                g += sum(green[o : o + ss])
                b += sum(blue[o : o + ss])
                a += sum(alpha[o : o + ss])
            o = (py * size + px) * 4
            if a > 0:
                out[o] = min(255, int(r / a * 255 + 0.5))
                out[o + 1] = min(255, int(g / a * 255 + 0.5))
                out[o + 2] = min(255, int(b / a * 255 + 0.5))
                out[o + 3] = min(255, int(a / per * 255 + 0.5))
    return bytes(out)


def png(size, rgba):
    """A PNG file (8-bit RGBA) of the picture."""

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    rows = b"".join(b"\x00" + rgba[y * size * 4 : (y + 1) * size * 4] for y in range(size))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )


def dib(size, rgba):
    """The picture as an icon bitmap: 32-bit BGRA from the bottom row up,
    then the 1-bit mask (transparent where the picture is fully clear)."""
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    colour_rows = bytearray()
    mask_rows = bytearray()
    stride = ((size + 31) // 32) * 4
    for y in range(size - 1, -1, -1):
        bits = bytearray(stride)
        for x in range(size):
            r, g, b, a = rgba[(y * size + x) * 4 : (y * size + x) * 4 + 4]
            colour_rows += bytes((b, g, r, a))
            if a == 0:
                bits[x // 8] |= 0x80 >> (x % 8)
        mask_rows += bits
    return header + bytes(colour_rows) + bytes(mask_rows)


def ico(pictures):
    """An .ico file of the pictures [(size, rgba)]: the 256-pixel one as
    PNG, the smaller ones as bitmaps (what every Windows reads)."""
    images = [(s, png(s, p) if s >= 256 else dib(s, p)) for s, p in pictures]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for s, data in images:
        out += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return out + b"".join(data for _, data in images)


def rtf(text):
    """The text as a plain RTF document (the installer's licence page)."""
    out = []
    for ch in text.replace("\r\n", "\n"):
        if ch in "\\{}":
            out.append("\\" + ch)
        elif ch == "\n":
            out.append("\\par\n")
        elif ch == "\f":
            continue
        elif ord(ch) < 128:
            out.append(ch)
        else:
            code = ord(ch)
            if code > 0xFFFF:
                raise ValueError(f"character {ch!r} outside what the RTF writer handles")
            out.append(f"\\u{code if code < 32768 else code - 65536}?")
    return "{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0\\fmodern Consolas;}}\\f0\\fs16 " + "".join(out) + "}\n"


def main():
    if len(sys.argv) != 2 or sys.argv[1].startswith("-"):
        print(__doc__.strip(), file=sys.stderr)
        return 2
    out_dir = sys.argv[1]
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    os.makedirs(out_dir, exist_ok=True)
    for source, name in ICONS:
        try:
            box, drawing = shapes(os.path.join(root, source))
        except Unsupported as e:
            print(f"{source}: this script does not draw {e}; see its first lines", file=sys.stderr)
            return 1
        data = ico([(s, render(box, drawing, s)) for s in SIZES])
        with open(os.path.join(out_dir, name), "wb") as f:
            f.write(data)
        print(f"{name}: {len(SIZES)} sizes, {len(data)} bytes")
    with open(os.path.join(root, "LICENSE"), encoding="utf-8") as f:
        licence = rtf(f.read())
    with open(os.path.join(out_dir, "License.rtf"), "w", encoding="ascii", newline="\r\n") as f:
        f.write(licence)
    print(f"License.rtf: {len(licence)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
