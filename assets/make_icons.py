import os, sys
root = sys.argv[1]
T = os.path.join(root, "assets/icons/tools")
B = os.path.join(root, "assets/brand")

def tool(name, body, title):
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" '
           f'fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" '
           f'stroke-linejoin="round" role="img" aria-label="{title}"><title>{title}</title>{body}</svg>\n')
    open(os.path.join(T, name + ".svg"), "w").write(svg)

# A small pylon used by several icons: mast + two arms, base at y.
def pylon(x, y, h, w):
    top = y - h
    return (f'<path d="M{x-w*0.35:.2f} {y}L{x:.2f} {top}L{x+w*0.35:.2f} {y}"/>'
            f'<path d="M{x-w/2:.2f} {top+h*0.28:.2f}H{x+w/2:.2f}M{x-w*0.38:.2f} {top+h*0.52:.2f}H{x+w*0.38:.2f}"/>')

tools = {
 "select": ('<path d="M5 3l13 8-5.5 1.5L15 19l-2.2 1-2.6-6.3L6 17z"/>', "Select"),
 "move": ('<path d="M12 3v18M3 12h18"/><path d="M9 6l3-3 3 3M9 18l3 3 3-3M6 9l-3 3 3 3M18 9l3 3-3 3"/>', "Move"),
 "delete": ('<path d="M4 7h16M9 7V4.5h6V7M6.5 7l1 13h9l1-13M10 11v6M14 11v6"/>', "Delete"),
 "goto": ('<path d="M12 21s6.5-6 6.5-11a6.5 6.5 0 0 0-13 0c0 5 6.5 11 6.5 11z"/><circle cx="12" cy="10" r="2.3"/>', "Go to"),
 "edit": ('<path d="M4 20l1-4.5L15.5 5a2.1 2.1 0 0 1 3 3L8 18.5z"/><path d="M13.5 7l3 3"/>', "Edit"),
 "add-structure": (pylon(10, 21, 17, 13) + '<path d="M19.5 3.5v5M17 6h5"/>', "Add structure"),
 "add-string": ('<path d="M2.5 3.5h12"/><path d="M8.5 3.5v3.5"/><path d="M5 7.5h7M5 10.5h7M5 13.5h7" stroke-width="2.2"/><path d="M8.5 7.5v7.5"/><circle cx="8.5" cy="16.5" r="1.5" fill="currentColor"/><path d="M2.5 18.5c4 1.4 8 1.4 12 0" opacity="0.6"/><path d="M19.5 9v5M17 11.5h5"/>', "Add string"),
 "_old_add_string": ('<path d="M10 3v3"/><ellipse cx="10" cy="8" rx="3.6" ry="1.2"/><ellipse cx="10" cy="11.5" rx="3.6" ry="1.2"/><ellipse cx="10" cy="15" rx="3.6" ry="1.2"/><path d="M10 16.2V20"/><circle cx="10" cy="20.6" r="0.9"/><path d="M19 4v5M16.5 6.5h5"/>', "Add string"),
 "string-wire": ('<path d="M3 5v14M21 5v14"/><path d="M3 7c4 7 14 7 18 0"/><path d="M3 11.5c4 6 14 6 18 0" opacity="0.55"/>', "String wire"),
 "add-alignment": ('<path d="M3 18l6-6 5 3 7-9" stroke-dasharray="2.2 2.6"/><circle cx="14" cy="15" r="2.2" fill="currentColor"/><path d="M19 15v5M16.5 17.5h5"/>', "Add alignment point"),
 "new-alignment": ('<path d="M3 19l6-7 5 3 6-8"/><circle cx="3" cy="19" r="1.6" fill="currentColor"/><circle cx="9" cy="12" r="1.6"/><circle cx="14" cy="15" r="1.6"/><path d="M19 17v5M16.5 19.5h5"/>', "New alignment"),
 "edit-alignment": ('<path d="M3 19l6-7 5 3 6-8"/><circle cx="3" cy="19" r="1.4" fill="currentColor"/><circle cx="20" cy="7" r="1.4" fill="currentColor"/><rect x="11.6" y="12.6" width="4.8" height="4.8" rx="0.8" fill="currentColor"/>', "Edit alignment"),
 "_old_edit_alignment": ('<path d="M3 19l6-7 5 3 6-8"/><rect x="11.8" y="12.8" width="4.4" height="4.4" rx="0.6" fill="currentColor"/><path d="M14 9.5v-3M14 20.5v-3" opacity="0.6"/>', "Edit alignment"),
 "add-jumper": ('<path d="M7 7h10" stroke-width="2.4"/><path d="M7 7 3.8 8.4M17 7l3.2 1.4" stroke-width="2.6"/><path d="M3.8 8.4H1M20.2 8.4H23" opacity="0.6"/><path d="M3.8 8.4C3.8 19 20.2 19 20.2 8.4"/><circle cx="3.8" cy="8.4" r="1.2" fill="currentColor"/><circle cx="20.2" cy="8.4" r="1.2" fill="currentColor"/>', "Add jumper"),
 "_old_add_jumper": ('<path d="M4 4v6M20 4v6"/><path d="M4 9c0 9 16 9 16 0"/><circle cx="4" cy="9" r="1.1" fill="currentColor"/><circle cx="20" cy="9" r="1.1" fill="currentColor"/>', "Add jumper"),
 "on-span": ('<path d="M3 5v13M21 5v13"/><path d="M3 7c4 8 14 8 18 0"/><circle cx="12" cy="12.9" r="1.9" fill="currentColor"/><path d="M12 15.5v5M10 18.5l2 2 2-2"/>', "Add on span"),
 "_old_on_span": ('<path d="M3 6v12M21 6v12"/><path d="M3 8c4 8 14 8 18 0"/><circle cx="12" cy="13.9" r="2.4" fill="currentColor"/><path d="M12 16.3V20" />', "Add on span"),
 "link": ('<circle cx="5.5" cy="17.5" r="2.4" fill="currentColor"/><path d="M7.5 15.9 15.1 9.7" stroke-width="2.8"/><circle cx="17" cy="8.2" r="2.3"/><path d="M17 10.5v6" opacity="0.6"/><path d="M15 13.5h4M15 16h4" stroke-width="1.6" opacity="0.6"/>', "Link"),
 "replace": (pylon(8, 20, 15, 11) + '<path d="M15 9h7M19.5 6.5 22 9l-2.5 2.5M22 15h-7M17.5 12.5 15 15l2.5 2.5"/>', "Replace structure"),
 "_old_replace": (pylon(9, 20, 15, 11) + '<path d="M17 6.5a4.5 4.5 0 0 1 4 4.5M21 11l-1.6-1.4M21 11l1.3-1.7"/><path d="M20 17.5a4.5 4.5 0 0 1-4-4.5M16 13l1.6 1.4M16 13l-1.3 1.7"/>', "Replace structure"),
}
for k in [k for k in tools if k.startswith('_old')]:
    del tools[k]
tools.update({
 'add-joint': ('<circle cx="9" cy="14" r="3" fill="currentColor"/><path d="M9 14 3 21M9 14l9-2M9 14V4" opacity="0.55"/><path d="M19 3v5M16.5 5.5h5"/>', 'Add joint'),
 'add-member': ('<circle cx="5" cy="18" r="2.2" fill="currentColor"/><circle cx="16" cy="7" r="2.2" fill="currentColor"/><path d="M6.6 16.4l7.8-7.8" stroke-width="2.6"/><path d="M19.5 13v5M17 15.5h5"/>', 'Member between two joints'),
 "point-on-joint": ('<path d="M4 21 12 6l8 15" opacity="0.55"/><circle cx="12" cy="6" r="2.2" fill="currentColor"/><path d="M12 8.2V15"/><path d="M10 11h4M10 13.5h4" stroke-width="2"/><circle cx="12" cy="16.3" r="1.3"/>', "Point on the joint"),
 'generator': ('<path d="M7 21 10.5 4M17 21 13.5 4M8 16h8M9 11h6"/><path d="M8 16l7-5M16 16 9 11" opacity="0.6"/><path d="M19.5 3l.7 1.8L22 5.5l-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z" fill="currentColor" stroke-width="1"/>', 'Generators'),
 'symmetry': ('<path d="M12 2.5v19" stroke-dasharray="2 2.2"/><path d="M9 6 4 18h5zM15 6l5 12h-5z"/>', 'Symmetry'),
 'measure-distance': ('<path d="M4 7v10M20 7v10M4 12h16"/><path d="M7.5 9.5 4 12l3.5 2.5M16.5 9.5 20 12l-3.5 2.5"/>', 'Distance'),
 'measure-height': ('<path d="M3 20h18"/><path d="M5 23l2.5-3M10 23l2.5-3M15 23l2.5-3" stroke-width="1.4"/><circle cx="12" cy="4" r="1.8" fill="currentColor"/><path d="M12 6.5v12M9.5 9 12 6.5 14.5 9M9.5 16 12 18.5 14.5 16"/>', 'Height above ground'),
 'measure-wires': ('<path d="M2.5 5c5 5 14 5 19 0M2.5 13c5 5 14 5 19 0"/><path d="M12 9.2v6.4M10.2 11 12 9.2 13.8 11M10.2 13.8 12 15.6l1.8-1.8" stroke-width="1.4"/>', 'Wire to wire'),
 'measure-clearance': ('<path d="M2.5 4.5c5 6 14 6 19 0"/><path d="M3 21h18"/><path d="M5 23.5l2-2.5M10 23.5l2-2.5M15 23.5l2-2.5" stroke-width="1.3"/><path d="M12 9.6v9.6M10.2 11.4 12 9.6l1.8 1.8M10.2 17.4 12 19.2l1.8-1.8" stroke-width="1.4"/>', 'Clearance at a point'),
 'measure-object': ('<path d="M2.5 4.5c5 6 14 6 19 0"/><path d="M17 21l2.5-9 2.5 9M18 17h3" stroke-width="1.5"/><path d="M11 9.5l6.2 4.2" stroke-dasharray="1.5 1.6"/>', 'Wire to object'),
 'follow': ('<path d="M2.5 5.5c5 6.5 14 6.5 19 0"/><circle cx="12" cy="10.4" r="1.7" fill="currentColor"/><path d="M12 12.6v4.2" stroke-dasharray="1.4 1.6"/><circle cx="12" cy="19" r="1.5" fill="currentColor"/><circle cx="7" cy="20" r="1.1" fill="currentColor"/><circle cx="17" cy="19.6" r="1.1" fill="currentColor"/>', 'Follow and snap'),
 'note': ('<path d="M4 4h16v11h-9l-5 4.5V15H4z"/><path d="M8 8.5h8M8 11.5h5"/>', 'Note'),
 'measure-stations': ('<path d="M3 17h18M5 14v6M12 15v4M19 14v6"/><path d="M5 8h14M7.5 5.5 5 8l2.5 2.5M16.5 5.5 19 8l-2.5 2.5"/>', 'Stations'),
 'add-point': ('<path d="M3 6h14" opacity="0.55"/><circle cx="10" cy="6" r="2.4" fill="currentColor"/><path d="M10 8.4v7" opacity="0.55"/><path d="M19.5 13v5M17 15.5h5"/>', 'Add point'),
 'two-strings': ('<path d="M2.5 4h14"/><path d="M4.5 4 9.5 15M14.5 4 9.5 15" stroke-width="2.2"/><circle cx="9.5" cy="16.5" r="1.5" fill="currentColor"/><path d="M19.5 9v5M17 11.5h5"/>', 'Two strings to one wire point'),
 'insert-joint': ('<circle cx="4" cy="20" r="1.6"/><circle cx="20" cy="4" r="1.6"/><path d="M5.2 18.8 18.8 5.2" stroke-width="2.4"/><circle cx="12" cy="12" r="2.6" fill="currentColor"/><path d="M19.5 15v5M17 17.5h5"/>', 'Insert a joint on a member'),
 'support': ('<path d="M12 4v8"/><path d="M12 12 7 19h10z"/><path d="M5 21.5h14M6.5 21.5l-1.5 1.5M10 21.5l-1.5 1.5M13.5 21.5 12 23M17 21.5 15.5 23" stroke-width="1.4"/>', 'Support'),
})
for k, (b, t) in tools.items():
    tool(k, b, t)

# Brand mark: a lattice pylon with a sagging conductor, on the brand blue.
BLUE, YELLOW, INK = "#0a6cdc", "#ffd36b", "#0b1f3a"
mark_inner = (
  '<path d="M32 8 22 56M32 8l10 48" />'
  '<path d="M24.4 44h15.2M26.2 35.5h11.6M28 27h8" />'
  '<path d="M24.4 44 37.8 35.5M39.6 44 26.2 35.5" opacity="0.7"/>'
  '<path d="M11 17h42M15 24h34" />'
  '<path d="M12 17v4.5M52 17v4.5M16 24v4.5M48 24v4.5" stroke="#ffd36b"/>'
  '<path d="M12 21.5C8 29 3 31 -1 30.5M52 21.5C56 29 61 31 65 30.5" stroke="#ffd36b" stroke-width="2.4"/>'
  '<path d="M16 28.5C12 36 6 39 -1 39M48 28.5C52 36 58 39 65 39" stroke="#ffd36b" stroke-width="2.4" opacity="0.8"/>'
)
def mark(size_attr="", bg=True):
    rect = f'<rect width="64" height="64" rx="14" fill="{BLUE}"/>' if bg else ""
    stroke = "#ffffff" if bg else INK
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" {size_attr} role="img" aria-label="OpenOHL">'
            f'<title>OpenOHL</title><clipPath id="r"><rect width="64" height="64" rx="14"/></clipPath>{rect}<g clip-path="url(#r)" fill="none" stroke="{stroke}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">'
            f'{mark_inner}</g></svg>\n')
open(os.path.join(B, "openohl-mark.svg"), "w").write(mark('width="64" height="64"'))
open(os.path.join(B, "openohl-mark-plain.svg"), "w").write(mark('width="64" height="64"', bg=False))

def wordmark(dark):
    fg = "#ffffff" if dark else INK
    bgc = INK if dark else "#ffffff"
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 72" width="300" height="72" role="img" aria-label="OpenOHL">'
            f'<title>OpenOHL</title><rect width="300" height="72" fill="{bgc}"/>'
            f'<g transform="translate(4 4)">{mark("").split(">",1)[1].rsplit("</svg>",1)[0].replace("<title>OpenOHL</title>","")}</g>'
            f'<text x="80" y="47" font-family="Inter, Helvetica Neue, Arial, sans-serif" font-size="34" font-weight="600" fill="{fg}">Open<tspan fill="{BLUE if not dark else "#5aa7ff"}" font-weight="800">OHL</tspan></text></svg>\n')
open(os.path.join(B, "openohl-wordmark.svg"), "w").write(wordmark(False))
open(os.path.join(B, "openohl-wordmark-dark.svg"), "w").write(wordmark(True))

# App icons: OHL-Design (wires and profile) and OHL-Structure (pylon close-up).
def app_icon(name, label, inner, bg):
    s = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64" role="img" aria-label="{label}">'
         f'<title>{label}</title><rect width="64" height="64" rx="14" fill="{bg}"/>'
         f'<g fill="none" stroke="#ffffff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">{inner}</g></svg>\n')
    open(os.path.join(B, name), "w").write(s)
app_icon("ohl-design-icon.svg", "OHL-Design",
  '<path d="M10 50 14 16M14 16 18 50M8 22h12" />'
  '<path d="M46 50 50 16M50 16 54 50M44 22h12" />'
  f'<path d="M8 22c12 18 36 18 48 0" stroke="{YELLOW}"/>'
  '<path d="M6 56c10-4 20-2 28-5s18-3 24-1" stroke-width="2.4" opacity="0.7"/>', BLUE)
app_icon("ohl-structure-icon.svg", "OHL-Structure",
  '<path d="M32 6 20 58M32 6l12 52" />'
  '<path d="M23.5 44h17M25.4 34h13.2M27.6 24h8.8" />'
  '<path d="M23.5 44 38.6 34M40.5 44 25.4 34M25.4 34 36.4 24M38.6 34 27.6 24" opacity="0.8"/>'
  '<path d="M10 14h44M14 22h36" />'
  f'<path d="M10 14v6M54 14v6M14 22v6M50 22v6" stroke="{YELLOW}"/>', "#0d4f9e")

# Contact sheet for review.
names = list(tools.keys())
cell = 72
cols = 10
rows = (len(names) + cols - 1) // cols
w = cols * cell + 40
h = 220 + rows * (cell + 30) + 30
parts = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" font-family="Helvetica Neue, Arial, sans-serif">',
         f'<rect width="{w}" height="{h}" fill="#f4f6f9"/>',
         '<text x="20" y="30" font-size="16" font-weight="700" fill="#0b1f3a">OpenOHL — logo and tool icons (draft)</text>']
for i, f in enumerate(["openohl-mark.svg", "ohl-design-icon.svg", "ohl-structure-icon.svg"]):
    inner = open(os.path.join(B, f)).read()
    parts.append(f'<g transform="translate({20 + i*90} 50) scale(1.1)">{inner.split(">",1)[1].rsplit("</svg>",1)[0]}</g>')
labels = ["OpenOHL", "OHL-Design", "OHL-Structure"]
for i, l in enumerate(labels):
    parts.append(f'<text x="{20 + i*90 + 35}" y="135" font-size="11" text-anchor="middle" fill="#0b1f3a">{l}</text>')
wm = open(os.path.join(B, "openohl-wordmark.svg")).read()
parts.append(f'<g transform="translate(300 45)">{wm.split(">",1)[1].rsplit("</svg>",1)[0]}</g>')
for k, n in enumerate(names):
    r, c = divmod(k, cols)
    x, y = 20 + c*cell, 170 + r*(cell+30)
    inner = open(os.path.join(T, n + ".svg")).read().split(">",1)[1].rsplit("</svg>",1)[0]
    parts.append(f'<rect x="{x}" y="{y}" width="56" height="56" rx="10" fill="#ffffff" stroke="#d6dbe3"/>')
    parts.append(f'<g transform="translate({x+10} {y+10}) scale(1.5)" fill="none" stroke="#0b1f3a" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" color="#0b1f3a">{inner}</g>')
    parts.append(f'<rect x="{x}" y="{y+60}" width="56" height="0" />')
    parts.append(f'<text x="{x+28}" y="{y+74}" font-size="9" text-anchor="middle" fill="#3a4a60">{n}</text>')
# dark row: same icons in light ink on dark background
yd = 170 + rows*(cell+30)
parts.append(f'<rect x="0" y="{yd-6}" width="{w}" height="{h-yd+6}" fill="#1b2230"/>')
for k, n in enumerate(names):
    x = 20 + k*((w-40)//len(names))
    inner = open(os.path.join(T, n + ".svg")).read().split(">",1)[1].rsplit("</svg>",1)[0]
    parts.append(f'<g transform="translate({x} {yd+2})" fill="none" stroke="#e6edf6" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" color="#e6edf6">{inner}</g>')
parts.append('</svg>')
open(os.path.join(B, "review-sheet.svg"), "w").write("\n".join(parts) + "\n")
print("ok", len(names), "tool icons")
