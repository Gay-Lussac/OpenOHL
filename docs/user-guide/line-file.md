# Line files (`.ols.json`)

A **line file** describes a line in 3D: structures with the strings hanging on them and their wire points, and the wire sections strung through them. It is JSON (the open exchange format of ADR-0004). Example: [`examples/synthetic-line/synthetic-line.ols.json`](https://github.com/Gay-Lussac/OLS-CAD/blob/main/examples/synthetic-line/synthetic-line.ols.json).

```bash
cargo run -p ols-cli -- line examples/synthetic-line/synthetic-line.ols.json \
    --temperature 15 --wind-pressure 600 --ice-thickness 0 \
    --html line.html --spans-csv spans.csv --csv wires.csv
```

`--html` writes a standalone 3D viewer (rotate, pan, zoom, vertical exaggeration); `--spans-csv` one row per span (tensions, sag, mid-point, low point); `--csv` every wire point. More formats (KML/KMZ, CSV of every weather case, plugin formats) with `--export`: see [Import and export](import-export.md).

## Conventions

- Numbers are **SI** (metres, newtons, °C for temperatures).
- Coordinates are in the line's coordinate reference system (`crs`, e.g. `"EPSG:2056"`): x east, y north, z up (elevation).
- Wire properties come from the [wire libraries](wire-library.md) (`cable` = a cable id).

## Top level

| Field | Required | Meaning |
|---|---|---|
| `format` | no (`"ols-line"`) | File identifier. |
| `version` | no (`1`) | Format version; newer versions are rejected. |
| `name` | yes | Line name. |
| `crs` | no | Coordinate system, e.g. `"EPSG:2056"` (see [coordinate systems](coordinates.md)); used by `ols crs line` and exports. |
| `routes` | no | Routes, the paths of the line: `{ "id", "name", "description" }`. A route is made of the alignments on it. |
| `alignments` | no | Alignments, in their order along their routes: `{ "id", "name", "description", "route": "route id", "points": ["P1", "P2", …] }` — exactly one route (required, existing); its points by id, in order (none while it is being laid out; a profile needs two). Structures are on the route of the alignments they stand on (tied to one of its points, or within 5 m); a structure on none gives its `routes` itself (see Structures). |
| `alignment_points` | no | Alignment points (PIs): `{ "id", "position": [x, y, z], "structure": "T12", "description" }`. A point is on any number of alignments, with its own place in each (where alignments meet they share the point: a junction, on all their routes); on none, it waits to be used. `structure`: tied under that structure — it follows it, and must lie under it in plan within 1 mm. |
| `structures` | yes | List of structures. |
| `sections` | yes | List of wire sections. |
| `jumpers` | no | Jumpers between wire points (below). |
| `systems` | no | Electrical systems: phases or poles with their voltage ([below](#electrical-systems)). |
| `circuit_starts` | no | The ends of electrical circuits that are starts: `[{ "structure", "set" }, …]` ([below](#electrical-systems)). A circuit none of whose ends is listed runs along the line. |
| `display` | no | Display settings: `{ "phase_colors": { "L1": "#8b4513", … }, "ahead_deg": 270 }` — wire colours (`#rrggbb`) by section `phase_label`; `ahead_deg`, the angle that means ahead along the line for angles in plan around a structure, which turn clockwise seen from above (default 0: right 90°; 270: right 0°, ahead 270°). |

## Structures

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id, referenced by sections. |
| `position` | yes | Structure origin `[x, y, z]` (centre at ground). |
| `orientation_deg` | no | Direction of the local x axis (right of the line), degrees CCW from east; used to place a structure type. |
| `type` | no | Structure type id from the [structure library](structure-library.md); its points, strings and wire points are then generated. |
| `extension` | no | Height extension of the type. |
| `description` | no | What it is, in words (free text). |
| `routes` | no | The routes it is on, given: needed when it stands on no alignment (a crossing line); empty: those of the alignments it stands on. |
| `height_adjust` | no | With a `type`: height of the type's feet above the ground at `position` (foundations), metres; default 0. |
| `points` | no | Points on the structure where strings hang: `{ "label", "position": [x, y, z] }` (below). |
| `wires` | no | Wire points: `{ "label", "position": [x, y, z], "set": n }` (below). |
| `strings` | no | Strings between them (below). |
| `omit` | no | With a `type`: labels of the type's points, strings or wire points left out. |
| `omit_sets` | no | With a `type`: the type's wire points with these sets left out, with what moves with them. |
| `members` | no | Stick drawing: polylines of `[x, y, z]` points, for views and drawings only; from the structure type when it is placed. |

With a `type`, the structure's own points, strings and wire points replace the type's with the same label, and a wire point with a set replaces the type's wire point with that set and what moves with it.

### Points, strings and wire points

What hangs on a structure is given explicitly (each point, wire point and string may also have a `description`, free text):

- **Points** (`points`): labelled points on the structure where strings hang.
- **Wire points** (`wires`): where strings meet and where wires are clamped. A wire point with a **`set`** number is where a section or jumper ends (sections and jumpers name it by structure and set, unique on the structure). A wire point no string reaches is **fixed** to the structure (a wire clamped directly). Its `position` is where it is in still air: the strings' lengths are the distances from their tops to it. A **`weight`** (N) hangs at it: a counterweight under the clamp (a string's own `weight` is spread along the string, half to each end).
- **Strings** (`strings`): straight members from a **top** — a point, *or another string's wire point* — to a wire point. Strings hang from each other, so any assembly is built from them:
  - an **I-string**: one string from a point to a wire point with a set;
  - a **V-string**: two strings from two points to one wire point;
  - a **V with a string below**: a V to a wire point without a set, and a string from it down to the clamp (the V swings along the line about its tops, the string below swings across);
  - a **TAK**: a string from a point down to a wire point without a set, and two strain strings from it to two wire points with sets, where one section ends and the next begins. Sections sharing an assembly are solved together ([theory](../theory/wirefe.md#strings-hanging-from-strings)).

| String field | Required | Meaning |
|---|---|---|
| `label` | yes | Unique among the structure's strings. |
| `description` | no | What it is, in words (free text). |
| `top` | yes | A point's or a wire point's label. |
| `wire` | yes | A wire point's label. |
| `length` | yes (files written before: no) | From its top to its wire point, m. Without it (files written before), the distance to its wire point as given — taken when read, so nothing moves. |
| `angle_deg` | no (0) | Its angle from straight down, degrees: 0 hanging, 90 level, 180 up. |
| `direction_deg` | no (0) | Its direction in plan, in the structure's axes, degrees: 0 ahead along the line, 90 to the right, 180 back, 270 to the left. |
| `insulator` | no | Insulator assembly id from the [insulator library](structure-library.md) (its weight, stiffness and wind area when resolved, where the string gives none). |
| `weight` | no (0) | Weight, N, half at each end. |
| `wind_area` | no (0) | Area the wind blows on, m², half at each end (0: its insulator's, if it has one). Every string takes it: suspension, V legs, strain strings and a jumper's helping strings. |
| `stiffness` | no | Axial stiffness EA, N (default 5·10⁷ N). |
| `compression` | no | Compression it may take, N: it then acts both ways (a post, a V leg designed to push), and the solve **flags** it beyond that. Without it: tension only, slack when pushed. |
| `strain` | no (false) | A strain (dead-end) string, else a suspension string: a set on its wire point is a strain point (the conductor ends there). Solved, it lines up with its conductor by equilibrium. |

**A wire point's position follows from its strings**: its top, plus its length in its direction. Where two strings end at one wire point (a V), it is where their lengths meet — of the circle both reach, the point nearest where the first string's own direction points (a hanging V: the lowest) — and their directions follow. A wire point no string reaches is fixed to the structure, where it is given. So a structure point moved (in structure editing) takes its strings' wire points along, and a string made longer moves its wire point down; the `position` written for a wire point on strings is the result, not an input. There is no "ahead" or "back" for a string: its angles alone give where it points.

How the wire is held at each point of a section follows from these. The usual cases keep the behaviour they have always had:

- in the middle of a section, one string hanging below its top (swung less than 45°) is a **suspension string** that swings freely when solved;
- one string at a section's end (or not hanging) is a **strain assembly**: a free member lined up with its conductor by equilibrium, so it moves with every weather case and its pull is in the calculation. One given only as geometry, without a weight or insulator and not marked `strain` (imported lines), gets the weight that holds it where its wire point puts it in the section's stringing state (still air at the stringing temperature): the moment of the wire's pull at the clamp balances that of the weight at the member's middle; its unstressed length is shortened by its stretch under that pull. A string whose data droops less than the wire pulls it down cannot be held there by any weight: its weight is 0 and it lines up with the wire. On a 930-span real line, mid-spans match the source's reported points as with clamps held at their data positions (median 1.5 cm, p90 3.3 cm);
- a V holds its clamp across the line while both strings are in tension; when the wind load leans further from vertical than the windward string, the leeward one goes **slack** and the clamp swings about the windward one's top (`ols line` lists slack strings);
- anything else — strings hanging from strings — is solved as given: every wire point a free node, every string a bar.

```json
{ "id": "T2", "position": [310.0, 0.0, 505.0],
  "points": [ { "label": "VL", "position": [310.0, -9.0, 535.0] },
              { "label": "VR", "position": [310.0, -3.0, 535.0] } ],
  "wires": [ { "label": "V1", "position": [310.0, -6.0, 532.0] },
             { "label": "W1", "position": [310.0, -6.0, 531.5], "set": 1 } ],
  "strings": [ { "label": "L", "top": "VL", "wire": "V1", "weight": 400 },
               { "label": "R", "top": "VR", "wire": "V1", "weight": 400, "compression": 2000 },
               { "label": "S", "top": "V1", "wire": "W1", "weight": 50 } ] }
```

**Files written before** (line file version 1) gave each string as an `attachment` (`set`, `phase`, `insulator_point`, `wire_point`, optional `legs`, …) and section points with a phase. They are read as points, strings and wire points: an attachment's point labelled by its `top_label`, else `A<set>-<phase>`; its wire point by its `bottom_label`, else its `label`, else `W<set>-<phase>`; V legs as two strings; a wire point within 1 cm of its insulator point fixed. A (set, phase) becomes the set itself for phase 1 — the usual case, one wire per set — and `set + 1000 · (phase + 1)` for other phases, everywhere (structures, sections, jumpers, project history). They are written as the current version.

**Files of version 2** had `routes` that were their own alignment (`alignment` points, `structures` listed) or followed their structures, and the line's own `alignment` without routes. They are read as routes and alignments of the same ids: a route that followed its structures gets points tied under them; the line's own alignment becomes the route and alignment `main`; a structure the alignments would not put on its routes gives them itself.

**Files of version 3** had alignments holding their points (`[x, y, z]` or `{ "position", "structure", "routes" }`) on one or more `routes`. Their points become alignment points of their own (`P1`, `P2`…); points of different alignments at the same place (within 1 mm, tied to the same structure or one of them free) become one shared point; an alignment on several routes becomes one alignment per route through the same points (`a`, `a (route)`…); a point's own routes go to the structure tied to it (a free point's are dropped). Projects are converted the same way when they open.

**Files of version 4** gave a jumper's tap as `from_tap` / `to_tap` beside a placeholder `from` / `to`, its supports as `support` (a string from a point) and `supports` (clamps on a structure's strings, `at` a fraction), a section's circuit name as `circuit`, and a place could carry a fraction beside a distance or a station (the station, else the distance, was used). They are read as a tap end, support points (`S1`; `C1`, `C2`… — labels its points do not use) and a place of one kind. Files are written as version 5.

`cargo run -p ols-cli -- line FILE --stations` prints the stationing, offsets and line angles along every alignment (with its route), and for each structure the other routes it is also on.

## Sections

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id. |
| `description` | no | What it is, in words (free text). |
| `cable` | yes | Cable id from the wire libraries. |
| `circuit_name`, `phase_label` | no | Labels: the name of its circuit (the same on all its sections; files before version 5: `circuit`) and its phase. |
| `system` | no | Electrical system (id in `systems`) whose phase `phase_label` the section carries. Give it once per conductor path: sections joined by jumpers (or sharing a wire point) follow. None on the whole path: earthed (earth wire, OPGW). Independent of the cable. |
| `bundle` | no (1) | Subconductors per phase. Tension, sag and loads are per subconductor; the solve follows the bundle's centre line with each subconductor's share of the insulator weights (string forces are reported for the whole bundle). |
| `bundle_spacing` | no | Distance between neighbouring subconductors, m (e.g. `0.4` for 400 mm). They sit evenly on a circle around the centre line, across the wire — radius `s / (2·sin(π/n))` — so clearances are measured to the nearest subconductor. Without it a bundle is checked as one wire. |
| `bundle_rotation_deg` | no (0) | Turns the arrangement (counter-clockwise looking ahead). 0: the two highest subconductors level — two side by side, three with two side by side above one, four in a square. |
| `points` | yes | `{ "structure": id, "set": n }` (a wire point) in line order; spans are between consecutive points. |
| `stringing` | yes | See below. |
| `span_points` | no | Labelled points on spans: `{ "label": "P1", "span": index, "at": place, "object": … }`; a place is `{ "fraction": 0–1 }` (horizontal, from the back point), `{ "distance": m }` or `{ "station": m }` (the line's station, along the alignment the span's structures stand on). One object per point: `{ "kind": "body" | "clamp", "type": library id, "weight": N, "shape": "point" | "sphere" | "cylinder" | "box" | "areas", its sizes, "drag": … }` — a marker ball is a body sphere; a point without an object is bare (a spread's end). A spacer's ends hold a clamp; files written before with bare spacer ends get a clamp of no weight there when read. |
| `spreads` | no | Objects spread between two points of one span: `{ "label": "…", "from": point label, "to": point label, "load": N/m (downward, whole bundle) }`. Solved by finite elements on the wire they cover (the ruling span leaves loads out). Older files' `loads` and `spread_loads` are read into points and spreads. |
| `color` | no | Display colour `#rrggbb`, overriding the phase colour. |
| `calculation` | no (`"initial-fe"`) | How the section is solved: a cable condition and a method — `initial-rs`, `creep-rs`, `load-rs`, `max-sag-rs`, `initial-fe`, `creep-fe`, `load-fe`, `max-sag-fe`. See below. |

### Calculations: condition and method

| Condition | The cable |
|---|---|
| Initial | as strung |
| Creep | after its permanent stretch due to creep (years at the creep weather case) |
| Load | after its permanent stretch due to the heaviest of the load weather cases (the one stretching it most) |
| Max Sag | after creep or after load, whichever sags more in the weather case solved |

The weather cases come from the line's `creep_stretch` (Criteria › Creep-Stretch): `{ "creep_case": "everyday", "load_cases": ["glaze ice", "rime ice + wind"] }` (by name, from the weather case file used); how a cable stretches, from its model ([wire library](wire-library.md#cable-models)). A section after creep or load without its weather case fails, saying so.

The line's **bimetallic conductor model** (`bimetallic`, optional): what the outer strands of its bimetallic cables do above the transition temperature, for the cables whose wire library entry does not say — `{ "behavior": "bird_cage" }` or `{ "behavior": "compression", "max_virtual_compression": 68947400000.0 }` (Pa; default compression with 68 947.4 MPa). Set in the app with Criteria › Bimetallic conductor model; see [Wire library](wire-library.md).

| Method | |
|---|---|
| FE | the section's finite-element model: suspension strings swing, each span its own tension; after creep or load the unstressed lengths are longer by the permanent stretch |
| RS | ruling span: one horizontal tension for the whole section (the sag-tension section with the non-linear cable model and the permanent set), every span a catenary between its clamps held where they hang in still air at stringing, in the plane of its load (blown out by the wind); point loads are left out |

RS and FE agree to about 1 % (the FE strings swing to even out the spans' tensions: on the synthetic example 1.2 % of sag at 60 °C).

### Colours

Wires are drawn in the section's `color`, else the line's `display.phase_colors` entry for the section's `phase_label`, else a default by the place of its `phase_label` among its system's phases (or the number its phase label ends with, `L2`): first red `#e4572e`, second yellow `#f2c14e`, third blue `#4c9be8` (then repeating); without either (e.g. shield wires) grey `#b0b8c0`. Utilities' conventions differ, so set them per project:

```json
"display": { "phase_colors": { "L1": "#8b4513", "L2": "#202020", "L3": "#808080" } }
```

### Stringing and status

A section is **in the blocks** (unclipped, the default for new sections) or **clipped in**. The `kind` says which.

In the blocks, the wire rolls over the suspension clamps (running blocks): in every weather case its unstressed length is shared out so that all its spans carry the **same horizontal tension** — ruling-span behaviour, computed exactly in 3D (point loads are left out of the sharing). Moving or changing a structure sags the section again to its condition. Two conditions:

```json
{ "kind": "tension", "horizontal_tension": 25000.0, "temperature": 15.0 }
```
Sagged to this horizontal tension (per subconductor) at this temperature, still air.

```json
{ "kind": "sag", "sag": 7.5, "temperature": 15.0 }
{ "kind": "sag", "sag": 9.2, "temperature": 15.0, "span": 2 }
```
Sagged to a sag (largest vertical distance below the chord, m) at a temperature, still air: in the ruling span \\( \sqrt{\sum S^3 / \sum S} \\) (level) without `span`, or in span `span` (0 = first). The tension giving that sag is used in every span.

In both, suspension clamps sit at their equilibrium position: swung across the line at line angles, plumb along it.

Clipped in, each span keeps its own unstressed length (per subconductor) at a temperature — the state after clipping in, or lengths from a previous design or another program:

```json
{ "kind": "unstressed_lengths", "lengths": [306.07, 330.73], "temperature": 15.0,
  "sagged": { "kind": "sag", "sag": 7.5, "temperature": 15.0 } }
```
In each weather case the suspension strings swing along the line to balance the spans, so tensions differ a little between spans. A structure moved changes the spans next to it; the strings beyond take up part of it, less and less along the section. `sagged` (optional) keeps the condition it was sagged to, to go back to the blocks. In the app, a section's window switches between *in the blocks* and *clipped in* (clipping in computes the lengths of its condition on the current geometry).

## Jumpers

A jumper is a slack cable joining two wire points: the ends of two sections at a dead-end structure, or a tap from a branch to a through wire (the example joins the branch's first point at the Y pylon T3 to the main L1 wire).

```json
"jumpers": [
  { "id": "J-T3-branch", "cable": "acsr-drake-26-7",
    "from": { "structure": "T3", "set": 2 },
    "to":   { "structure": "T3", "set": 1 },
    "sag": 1.5, "temperature": 15.0 }
]
```

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id. |
| `description` | no | What it is, in words (free text). |
| `cable` | yes | Cable id from the wire libraries. |
| `from`, `to` | yes | The ends: wire points `{ "structure", "set" }`, or a **tap** — a clamp on a span point, `{ "tap": { "span": { "section", "point" } } }`, or on another jumper's point, `{ "tap": { "jumper": { "jumper", "point" } } }`; the point holds a clamp object. A jumper on a tap is given by its `length` (or `rigid`); its section is solved by finite elements. |
| `length` | one of | Unstressed (cut) length, m, at `temperature`. |
| `rigid` | one of | `true`: a rigid jumper (a tube or bar), the straight member between its solved clamps; it takes no `length` or `sag`, does not change with the temperature, and is never taut. |
| `sag` | one of | Or the largest vertical distance below the end-to-end chord, m, in still air at `temperature`; the cut length is found from it (jumpers without support). |
| `temperature` | no (15) | Temperature of `length` / `sag`, °C. |
| supports | no | Points of `points` holding a clamp object with a `hold`, placed by a `fraction` of its length from `from`: `"hold": { "strings": { "structure": id, "wire": label } }` — the clamp hangs on the suspension strings of a structure to wire point `wire`, which has **no set** (a wire point with a set is a wire's end; structure default: the one at `from`) (below) — or `"hold": { "string": { "point": [x,y,z], "length": m, "weight": N } }`, a tension-only string from `point`. Needs `length`; strings from a fixed point and clamps on a structure's strings not both. Files of version 4 gave them as `support` and `supports` of their own. |
| `bending_stiffness` | no | Bending stiffness EI of the cable, N·m²: where an end is a section's end clamp, the jumper leaves it in a set direction and bends away from it (below). Default: estimated from the cable, `stiffness_factor` × its stiffness as a solid bar \\( E\\,\\pi d^4/64 \\) (composite modulus E, diameter d), not below its strands slipping (when the wire library gives the strand counts); Drake 26/7: ~455 N·m². `0`: hinged ends. |
| `stiffness_factor` | no (0.2) | The estimated EI as a share of the solid bar's, above 0 and at most 1: the jumper's uniform effective stiffness (0.2–0.35 is the usual range; the lower, the further it swings). Unused when `bending_stiffness` is given. |
| `angle_from_deg`, `angle_to_deg` | no (30) | Angle at which the jumper leaves the clamp at `from` / `to`, degrees, from its tension string's line (from the clamp towards the string's top; two strings: the middle of their tops; none: the conductor's axis at the clamp), turned within the conductor's plane towards the weather case's load on it (straight down in still air, tilted with the wind as the conductor swings) (0: along the string; negative: the other way). Only where that end is a section's end clamp. |
| `color` | no | Display colour; default: the colour of the section through `from`. |

**Clamped ends.** Where an end is the first or last point of a section, the clamp holds the jumper: it leaves at `angle_from_deg` / `angle_to_deg` from its tension string's line (from the clamp towards the string's top; two strings: the middle of their tops; none: the conductor's axis at the clamp), turned within the conductor's plane towards the weather case's load on it (straight down in still air, tilted with the wind as the conductor swings) — in each weather case, as the string and the conductor move — and the cable's bending stiffness carries that direction into the jumper. The jumper then does not swing in the wind as if hinged at its ends, and follows its clamps as they move with the sections. It is solved as an inextensible elastic rod in pieces of ~0.12 m (16 to 40; its stretch, ~10⁻⁶ at a jumper's tension, is left out); a support string is then rigid, and a string whose top is out of reach of the point it holds is an error. A `sag` is the clamped jumper's sag in still air. A jumper shorter than the distance its clamps swing apart to (free strain strings in the wind) fails in that weather case, said so ("taut"): its pull holding the clamps back is not modelled yet. An end that is not a section's end (a tap on a through wire) stays hinged; with both ends hinged, or `bending_stiffness` 0, the jumper is the catenary described next.

How much the clamps hold a jumper back depends on their angles: a clamp along the chord between the ends (a straight dead-end with terminals straight on) does not resist a swing about that chord at all, and a jumper leaving its clamps upwards (terminals straight on along spans rising to the structure) swings further than a hinged one, since the bend at the clamps eases as it swings out. Terminals angled down (the default 30°) hold it back.

Each end is the **solved clamp** of the section through that wire point (so the jumper follows strain assemblies and swung suspension clamps), or the wire point's position if no section uses it. The jumper is one exact catenary element (two with a support string) carrying the weather case's weight, ice and wind (horizontal, perpendicular to the jumper's chord); its length scales with \\( e^{\alpha \Delta t} \\) from `temperature`. Sections are solved first: a jumper's tension (a few hundred N) does not change them. A jumper given by its `sag` is cut once and keeps that length until it, the sections at its ends or their structures change; the computation, its costs and the expected ranges are in the theory manual ([Jumpers](../theory/jumpers.md)). `ols line` prints each jumper's length, sag and tension, includes jumpers in the ground clearance check (as `jumper:<id>`) and draws them in the viewer. Clearance from jumpers to the structure body comes with structure outlines.


**Helping strings.** A jumper between two dead-end clamps can be held by helping strings between them — insulator strings of the structure, as suspension strings hold the conductor on the line — each holding it in a clamp. A clamp's `wire` is a wire point of `structure` (default: the structure at `from`) without a set, held by one string (an I) or two strings (a V), each from a structure point or **from another string's wire point** (a string below a string): hanging from the wire point where a section is clamped (below a conductor's suspension string), its top is where that clamp is solved in each weather case and the section carries the string — its weight and its share of the jumper (half the stretches either side of the clamp; with the jumper's own pull, the string's solved force) —; from a wire point no section is clamped at, its top is where that point is in still air. Its point's `fraction` is where the clamp's middle is, as a share of the jumper's length from `from`, between 0 and 1; the clamps are taken in order along the jumper, with room for their lengths. `clamp_length` is the clamp's length, m (default 0.1, above 0 and at most 1 m). The clamp swings and turns freely; the jumper runs straight through it (the direction it leaves in is the one it enters in), also with `bending_stiffness` 0 or hinged ends. Each string is rigid and tension-only — a V leg goes slack when the wind swings the clamp towards it — unless it has a `compression`: then it acts both ways and may push (a V reaching sideways to the jumper); its force is then negative. Half of each string's weight and the wire point's `weight` hang on the clamp, and half of the wind on each string (its `wind_area`) pushes it; the clamp itself takes no wind. The jumper is solved as the rod above ([Jumpers](../theory/jumpers.md)); `ols line` prints each string's force; the structure loads include the strings' pull at their tops.

```json
"points": [
  { "label": "C1", "at": { "fraction": 0.5 }, "object": { "kind": "clamp", "clamp_length": 0.1, "hold": { "strings": { "wire": "JW" } } } },
  { "label": "C2", "at": { "fraction": 0.8 }, "object": { "kind": "clamp", "clamp_length": 0.08, "hold": { "strings": { "structure": "T3", "wire": "JV" } } } }
]
```

A jumper is drawn and checked as a bundle like its section: `bundle`, `bundle_spacing` and `bundle_rotation_deg` default to those of the section at `from` (the first section through that point) and can be set on the jumper, each on its own (e.g. only a different spacing). A support string's weight is shared by the subconductors.

## Electrical systems

The electrical model is kept apart from the physical wires: a **system** is a group of phases (AC) or poles (DC) with its voltage, and sections point to it.

```json
"systems": [
  { "id": "380-1", "name": "380 kV Alpha–Beta 1", "kind": "ac",
    "voltage_kv": 380, "frequency_hz": 50,
    "phases": [{ "label": "L1", "angle_deg": 0 }, { "label": "L2", "angle_deg": -120 },
               { "label": "L3", "angle_deg": 120 }] },
  { "id": "sbb", "name": "132 kV railway", "voltage_kv": 132, "frequency_hz": 16.7,
    "phases": [{ "label": "UU", "angle_deg": 0 }, { "label": "VV", "angle_deg": 180 }] },
  { "id": "hvdc", "kind": "dc", "voltage_kv": 500,
    "phases": [{ "label": "+", "polarity": 1 }, { "label": "-", "polarity": -1 },
               { "label": "MR", "polarity": 0 }] }
]
```

| Field | Required | Meaning |
|---|---|---|
| `id`, `name` | id | Sections refer to the id. |
| `kind` | no (`ac`) | `ac` or `dc`. |
| `voltage_kv` | yes | AC: nominal voltage between phases (rms). DC: pole voltage to earth. |
| `frequency_hz` | no | AC frequency (50, 60, 16.7…). Systems of different frequencies are not synchronous. |
| `color` | no | Display colour `#rrggbb` when wires are coloured by system (default: by the system's place in the line). |
| `phases` | yes | `label` (matched with the sections' `phase_label`); AC `angle_deg` (default evenly spread: 0°, −120°, +120°; two phases 0°, 180°); DC `polarity` +1, −1 or 0 (default in that order); `code`, the phase's line code when wires are drawn by system (`-` solid, else its dots and dashes repeated, e.g. `.-`; default `-`, `.-`, `.--` by place). |

**Conductor paths.** Sections joined by jumpers, or sharing a wire point, form one conductor path. Setting `system` on one section of the path assigns the whole path to that system's phase `phase_label`; two sections of a path naming different phases is an error that names them. A path without a system is earthed.

**Electrical circuits.** Each conductor path is an electrical circuit, so the line has as many circuits as it has paths, counted from its connections. Each end of a circuit is a **start** (power flows in) or an **end** (power flows out), with at least one of each; a circuit with taps, or fed from several sides, can have several of either. The starts are those listed in `circuit_starts`. A circuit none of whose ends is listed runs along the line: its end that comes first along the line (its routes) is its start, the others are its ends. Reversing a circuit swaps every role. A system's circuits should run the same way — from the same start structures, to the same end structures (a branch adding an end is the same way); the desktop app warns when they do not, after a change in the tables, without refusing it. The roles order its sections and jumpers from its starts outwards, and the desktop app turns its jumpers to point along it (`from` nearer a start). Nothing physical depends on them: the wind's side is the line's (see [Conventions](../theory/conventions.md)).

**Voltages.** With `n` phases evenly spread, a phase is at `U / (2·sin(π/n))` to earth (`U/√3` for three phases, `U/2` for two). Between two conductors: the phasor difference for AC of the same frequency (`U` between two phases of a system, 0 between the same phase of two parallel systems), the difference of pole voltages for DC, and the sum of the voltages to earth otherwise (different frequencies, AC with DC). Clearance rules can use the nominal voltage or this voltage between conductors ([clearances by voltage](terrain.md#clearances-by-voltage)).

Electrical properties of the cables (resistance, ampacity data) belong to the [wire library](wire-library.md), linked through each section's `cable`.

## How a section is solved

Every span is one exact elastic catenary element (split at point loads), suspension strings and V-string legs are tension-only truss elements, wind acts horizontally and perpendicular to each span, and the free clamps and load points are found by Newton–Raphson in 3D (see [finite-element wire systems](../theory/wirefe.md)). Each span's unstressed length scales with \\( e^{\alpha \Delta t} \\) from the stringing temperature.

## Checked against PLS-CADD

On a real 78-structure, 930-span line exported from PLS-CADD (private data, compared locally and not part of the repository), OLS CAD reproduced PLS-CADD's mid-span heights to a median of 1.1 cm (90 % within 3.4 cm) for unloaded spans and 1.7 cm (90 % within 3.9 cm) for spans with marker balls, and mid-span positions to 1 mm, solving all 554 sections in under 20 ms. The largest differences (up to 21 cm) are nearly vertical, nearly taut down-leads, where PLS-CADD's 1 cm rounding of unstressed lengths moves the sag by that much.
