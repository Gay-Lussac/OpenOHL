# Glossary

One object, one name: in both applications, on the command line, in the
files and in this manual. Each entry gives the term, what it means, the
names it replaces (older texts, other programs), the German term for the
Swiss users, and the key the files use where it differs from the term.
The file keys stay as they are, so files of every version are read.

The German terms come from common usage, not from a standard's text;
those marked *(to confirm)* are still to be checked. The terms follow the
usual international vocabulary of overhead lines where one exists (per
common IEC usage, to confirm); no clause of a standard is cited here.

Agreed with the project owner on 2026-10-21 ([decision
log](../development/decisions.md#2026-10-21)). Pre-alpha: the terms are
in use, in active development (not approved yet).

## Structures

![A structure seen along the line: attachment points on the crossarms, a
string from each to a wire point with its set, and the earth wire's wire
point at the peak](../images/glossary/structure-parts.svg)

**Structure** — a support of the line at one place: a lattice tower, a
pole or a portal, with the points its strings hang from, its strings and
its wire points. A **suspension structure** carries the wire through; a
**tension structure** ends sections.
*Replaces:* pylon, tower, mast, support. *German:* Mast (Tragmast,
Abspannmast). *Kinds:* **lattice tower** (Gittermast), **pole**
(Stangenmast, to confirm); in files `kind = "lattice"` / `"pole"`.

**Structure type** — a structure described once in a `.ohls` file, used by
any number of structures of the line, each placed, turned and extended on
its own. *Replaces:* pylon type. *German:* Masttyp.

**Structure template** — a parametric face in a `.ohlpt` file from which a
structure type of detail level 1 is built (suspension or tension), its
dimensions given as parameters. *Replaces:* pylon template. *German:*
Mastvorlage (to confirm). *In files:* the format id `ohl-structure-template` and the folders
`libraries/structure-templates/`, `Structure templates` (beside a line) and
`StructureTemplates/` (beside a structure file); the names before 2026-10-21
(`ohl-pylon-template`, `pylon-templates/`, `Pylon templates`,
`PylonTemplates/`) still read.

**Attachment point** — where strings hang on a structure, linked to a
**joint** of the structure (its type's), optionally at an **offset** from
it: a rigid link whose moment the structure analysis applies at the
joint. Several attachment points may share a joint. A point the
structure no longer has a joint for is **unlinked**: it keeps its place,
and the structure's loads are refused until it is linked again. When a
structure is replaced by another type, its attachment points are matched
to the new joints: exactly (a joint at the same position), else the
nearest attach joint is offered with the distance left as the offset;
the user chooses per point. Editable in both applications.
*Replaces:* structure point (retired, 2026-10-21; read as "attachment
point" in older texts). *German:* Aufhängepunkt (to confirm). *In files:*
`points` (line file: `joint`, `offset`, `unlinked`), `[[point]]` (`.ohls`:
`node`, or its own `point`; `offset`; files written before may give
`member` and `at`: read as a joint inserted on that member).
A point cannot be put on a member: it needs a joint (2026-10-21). When
a linked point is moved (dragged, typed, put by a command) it stays on
its joint and its offset follows; a joint moved takes its points along.
In both applications the **Attachment points** table gives each one's
joint, offset and the offset's length; the **Link tool** links a point
to a joint where it is (OHL-Design: Tools › Structures › Link attachment
point; OHL-Structure: Geometry › Link tool); a point's menu unlinks it;
an unlinked point is drawn red in OHL-Design.
*Built (engine, command line and both applications, 2026-10-21; in
active development, not approved yet).*

**Joint** — a node of the structure's model (OHL-Structure). A joint has
a **kind**, or none: **attach** (strings hang here, through the
attachment points linked to it) or **support** (where the structure
stands on its foundation; its fixity is the joint's support). A joint
carrying attachment points is an attach joint, a supported one a support
joint, unless its kind is given. A joint can be **inserted on a member**
(OHL-Structure): the member is split there into two continuous pieces (a
rigid connection, not a hinge; each buckling over the whole member); a
file written before with a point along a member is read so. A stick
figure's point is its own joint. Set in OHL-Structure's Joints table (*auto*, *attach*,
*support*); attach joints are ringed in orange in the 3D views, support
joints marked green. Only attach joints are offered when a new string's
point is added to a structure with a type, and only attach joints are
ringed and take a link in the Link tool (2026-10-21; another joint says
"not an attach joint: set its kind to attach in OHL-Structure"). *German:* Knoten (to confirm). *In
files:* `[[node]]`, its `kind` (`.ohls` version 6). *Built (engine and
both applications, 2026-10-21; in active development, not approved
yet).*

**Rigid link** — the straight, rigid connection from a joint to an
attachment point at an offset: the string's force at the point reaches
the joint as the same force plus the moment r × F (applied where a frame
member meets the joint; where only trusses meet it the moment is left
out, with a note). Drawn as a magenta rod in both applications' 3D views
and OHL-Design's structure screen, on its own layer (OHL-Design: View ›
Layers › Rigid links; OHL-Structure: View › Rigid links), none at zero
offset; a click on it selects its attachment point, which shows the
joint, the offset vector, its length and its moment per load case where
computed. *Replaces:* eccentricity (of an attachment). *German:* starre
Verbindung (to confirm). *In files:* the attachment point's `offset`.
*Built (2026-10-21; in active development, not approved yet).*

![Left: a structure's joints, an attach joint at the crossarm's tip and
support joints at its feet; right, enlarged: an attachment point at an
offset e from its attach joint, joined to it by a rigid link, the
string's force F at the point and the moment M = F · e at the
joint](../images/glossary/rigid-link.svg)

**Body** — the structure type's lattice or shaft above its feet (its
joints and members, or its `[[body]]` polylines). *German:* Mastschaft
(to confirm).

**Extension** — a piece added under a structure type's body to raise it;
a structure names one. **Leg extension** — one leg made longer, alone.
**Height adjustment** — how far a structure's feet stand above the ground
at its position (may be negative).
*Replaces:* height extension, extension body, body extension. *German:*
Mastverlängerung, Eckstielverlängerung, Höhenanpassung (all to confirm).
*In files:* `[[extension]]` (`.ohls`), `extension`, `height_adjust` (line
file).

**Detail level** — how much of a structure its file describes: **level 1**,
a stick figure; **level 2**, sticks of the structure (joints and members
without cross-sections); **level 3**, the full design (every member with
its cross-section, the joints supported). *Replaces:* level of detail (for
structures; the views' *level of detail*, how finely they draw by distance,
is another thing). *German:*
Detaillierungsstufe (to confirm). *In files:* `detail`.

**Cross-section** — the shape and size of a member (an angle, a tube…),
from the **cross-section library** (`.ohlsec`). *Replaces:* section and
member section, Section library. *German:* Querschnitt, Profil. *In
files:* `[[section]]`, `section` (unchanged).

![Left: a structure type's body on an extension, one leg extension and the
height adjustment of its feet; right: detail levels 1, 2 and
3](../images/glossary/extensions.svg)

## Strings

![Four ways single strings combine: one string; two strings to one wire
point; a string below two strings; two tension strings hanging from a
string's wire point](../images/glossary/string-combinations.svg)

**String** — one straight insulating member from its **top** (an
attachment point, or another string's wire point) to its **wire point**, by its
length and direction. It is a **suspension string** (the wire runs
through its wire point) or a **tension string** (the wire ends there).
Its weight, wind area and stiffness are its own or those of its
**insulator assembly**.
*Replaces:* insulator (for a placed string), insulator string, insulator
chain. *German:* Isolatorkette (Tragkette, Abspannkette).

Strings combine; the combination is not a type of its own:

- **two strings to one wire point** (they share the wire point);
- **a string hanging from another string's wire point** (or from the wire
  point two strings share);
- **two tension strings from a string's wire point**, each ending a
  section;
- **two sections sharing one assembly**.

Combinations other programs name separately (I-, V-, V + I strings,
TAKs) need no type of their own here: they are strings combined as shown.

**Insulator assembly** — a library entry (`.ohli`) with a string's length,
weight, wind area, stiffness and the optional limits, from its data sheet;
a string names it by its id. An assembly of two strings to one wire point
gives the distance of their tops from the middle (`half_width`).
*Replaces:* insulator (for a library entry), assembly. *German:*
Isolatorkettentyp (to confirm). *In files:* the string's `insulator`;
in `.ohli`, `legs = 2` with `half_width` (version 3; `kind = "v_string"`
of older files still read).

## Wires and their points

**Wire point** — the bottom of one or more strings, where a wire is
clamped; placed by its strings, or fixed to the structure where no string
reaches it (an earth wire's peak). *Replaces:* clamp (as a point),
conductor position. *German:* Seilbefestigungspunkt (to confirm). *In
files:* `wires` (line file), `[[wire]]` (`.ohls`), a string's `wire`.

**Set** — the number of a wire point, unique on its structure: the place
a wire is connected to. Sections and jumpers end at a structure and a
set. A wire point without a set only joins strings. *Replaces:* slot (in
structure templates). *German:* Phasenplatz (to confirm). *In files:*
`set`; in a structure template (`.ohlpt`), the key `slot`.

**Wire** — any wire strung in a section: a **conductor** (it carries a
phase) or an **earth wire** (earthed, no phase; an **OPGW** is an earth
wire with optical fibres). A conductor may be a **bundle** of
**subconductors**.
*Replaces:* shield wire, ground wire. *German:* Seil; Leiterseil, Erdseil,
LWL-Erdseil; Bündelleiter, Teilleiter.

**Wire type** — a library entry of the **wire library** (`.ohlw`): its
stranding, diameter, mass, modulus, expansion and the data sheet's values.
*Replaces:* cable, cable file. *German:* Seiltyp. *In files:* `cable`
(sections, jumpers), the folder `cables/`; on the command line `--wire-type` (`--cable` still read).

**Wire condition** — the state the wire is solved in: initial, after
creep, after load (or the larger sag of the two). Not a case.
*Replaces:* condition, cable condition. *German:* Seilzustand. *In files:*
`calculation`.

## Sections, spans and jumpers

![A section seen from the side: a wire from a tension structure over
suspension structures to the next tension structure; a jumper around the
last one](../images/glossary/section.svg)

**Section** — a tension section: a wire strung between two tension
structures over one or more spans, with its own wire type, stringing and
phase. *Replaces:* wire section, strain section. *German:*
Abspannabschnitt.

**Span** — the wire between two consecutive structures of a section.
*German:* Spannfeld. **Ruling span** — the one span that stands for all
the spans of a section in the ruling-span method. *German:*
Mittelspannweite (to confirm).

**Jumper** — a wire joining two wire points with sets, usually the ends
of two sections at a tension structure, or ending on a span (**tap**).
A **jumper support** is a clamp on the jumper held up by strings.
*German:* Stromschlaufe, Abzweig, Schlaufenstütze (all to confirm).

![A span with span points: one holds a marker ball, one a spacer's end,
one the clamp of a tap](../images/glossary/span-objects.svg)

**Span point** — a labelled point on a span. **Span object** — what a
span point holds: a **body** (a **marker ball**, a light, a weight: its
weight, wind and ice) or a **clamp** (a hinge for a tap or a spacer's
end). *Replaces:* line object, point load, warning marker. *German:*
Feldpunkt, Feldobjekt (to confirm), Flugwarnkugel. *In files:*
`span_points`, `object`, `kind = "body"` / `"clamp"`.

**Spacer** — an interphase spacer: an insulator from a span point of one
section to a span point of another in the same span. *German:*
Phasenabstandhalter.

## Electrical

![A structure with two systems of three phases and an earth wire; each
colour a conductor path](../images/glossary/systems.svg)

**System** — an electrical system: its kind (AC or DC), voltage,
frequency and phases. *German:* Strang / System (to confirm). *In
files:* `systems`, a section's `system`.

**Phase** — one phase of a system (for DC, a pole). *German:* Phase (DC:
Pol). *In files:* `phase_label`.

**Conductor path** — sections joined end to end by jumpers or a shared
wire point, with its starts (where power flows in) and ends. It carries
one phase of one system. *Replaces:* circuit (one row of the former
*Electrical circuits* table). *German:* Leiterzug (to confirm). *In
files:* `circuit_starts`.

**Circuit** — the conductors that form one circuit (e.g. *North*: its
L1, L2 and L3), an object of the line the user defines: id, name,
optionally its system, a description; its members are conductor paths.
Circuits follow the physical connections: a conductor path is in one
circuit, a circuit holds one conductor path per phase, and two circuits
are never merged — they become one only when connected (a jumper, a wire
strung through), the jumper's window asking which name stays. Checks and
safety volumes take them together ("per circuit", "circuit to circuit").
Decided 2026-10-21. *German:* Strang (to confirm). *In files:*
`circuits`, a section's `circuit_name` (its circuit's name),
`--circuits ID|NAME`.

![A circuit North of two phases in a system; its L1 is two sections
joined by a jumper, one conductor path; a branch of phase L1 not
connected is a circuit of its own](../images/glossary/circuits.svg)

## The line and its route

![A route in plan: two alignments through alignment points meeting at a
junction, structures along them](../images/glossary/route-plan.svg)

**Line** — the overhead line being designed. *German:* Leitung.

**Project line** — one design of the line in a project: the main line, a
**stage** of the project, or an alternative. *German:* Leitungsvariante
(to confirm).

**Stage** — a part of a project done in turn (*stage 1*, *stage 2*),
usually one project line each. *Replaces:* phase (of a project).
*German:* Etappe (to confirm).

**Route** — a path of the line made of alignments. *German:* Trasse (to
confirm).

**Alignment** — a polyline through alignment points along which the
structures are stationed. *German:* Trassenachse (to confirm).

**Alignment point** — a vertex of an alignment; a structure may be tied
to it, and one point may be on several alignments (a junction).
*Replaces:* angle point, PI. *German:* Winkelpunkt (to confirm).

## Cases and loads

![Weather case, load case and structure load case, one feeding the
next](../images/glossary/cases.svg)

**Weather case** — temperature, wind and ice (`.ohlwc`): it gives the
wires' sag and tension. *German:* Wetterfall (to confirm).

**Load case** — a weather case with load factors (`.ohllc`): the loads the
line sends to its structures. *German:* Lastfall.

**Structure load case** — OHL-Structure's own case (`.ohlslc`): load cases
from the line with the structure's own loads, for its analysis.
*German:* Mastlastfall (to confirm).

## Clearances

![Sag, vertical clearance; a span cut across with the swing band and the
radial clearance to an object](../images/glossary/clearances.svg)

**Sag** — how far the wire hangs below the chord between its two wire
points. *German:* Durchhang.

**Clearance** — the distance from a wire to the ground, an object or
another wire, in a case: **vertical** (straight down) or **radial** (the
shortest, the wire swung by the wind). *German:* Abstand.

**Required clearance** — the distance a rule asks for. *Replaces:*
requirement. *German:* Mindestabstand.

**Clearance rule** — a row of a `.ohlr` file: which objects, which
voltage, which required clearance. **Rule preset** — a named group of
clearance rules from a design-code plugin. *Replaces:* rule, preset
(alone). *German:* Abstandsregel, Regelvorgabe (to confirm).

**Swing band** — the space a wire sweeps between still air and its full
swing in the wind. *German:* Ausschwingbereich (to confirm).

## Terrain and survey

**Terrain surface** — a triangulated ground model (from survey points, a
point cloud, LandXML…). *Replaces:* ground surface, surface (alone), TIN
(as a noun). *German:* Geländemodell.

**Point cloud** — LAS/LAZ points read from a file or kept in a project;
*LiDAR* names the way they were measured and goes with what comes of
it (LiDAR classes, the ground built from LiDAR). *German:* Punktwolke.
*On the command line:*
`--lidar FILE`, `--pointcloud PROJECT#name` (unchanged).

**Survey points** — measured ground points (CSV / XYZ). *German:*
Vermessungspunkte.

## Drawings on this page

The drawings are generated by `tools/glossary_figures.py` (`python3 -I
tools/glossary_figures.py` writes `docs/images/glossary/`): after a
change of a term, change its label there and run it again.
