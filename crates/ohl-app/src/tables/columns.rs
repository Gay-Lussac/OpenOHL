// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tables' columns: name, width, how each is edited, help.

use super::*;

pub(super) const STRUCTURES: &[Column] = &[
    (
        "id",
        120.0,
        Yes,
        "unique id (renaming: sections, jumpers and routes follow)",
    ),
    (
        "type",
        110.0,
        Yes,
        "structure type id (empty: strings given)",
    ),
    (
        "extension",
        80.0,
        Yes,
        "extension of the type (empty: none)",
    ),
    (
        "orientation °",
        90.0,
        Yes,
        "cross-arm direction, degrees counter-clockwise from east",
    ),
    ("E", 110.0, Yes, "base easting, m"),
    ("N", 110.0, Yes, "base northing, m"),
    (
        "z",
        80.0,
        Yes,
        "base height, m (new rows, empty: on the ground)",
    ),
    (
        "strings",
        60.0,
        Derived,
        "attachment points (Strings table)",
    ),
    ("sections", 60.0, Derived, "sections through it"),
    (
        "routes",
        140.0,
        Yes,
        "the routes it is on: from the alignments it stands on (dimmed), or ticked — needed when it stands on no alignment",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
    (
        "weather groups",
        150.0,
        Yes,
        "its weather case groups (Line › Criteria › Weather case groups…), ticked: a check with the structure override ticked takes here only its cases in these groups (e.g. a wind study's lower winds); empty: the check's cases",
    ),
    (
        "soil ρ Ω·m",
        80.0,
        Yes,
        "the soil resistivity measured at it, Ω·m; the line parameters take on each span the geometric mean of its two structures' (one measured: that one); empty: not measured (the line's)",
    ),
];

pub(super) const SECTIONS: &[Column] = &[
    ("id", 90.0, Yes, "unique id"),
    ("cable", 170.0, Yes, "wire type id from the wire library"),
    (
        "circuit",
        150.0,
        Yes,
        "circuit name (the same on the sections of its conductor path)",
    ),
    ("phase", 50.0, Yes, "phase label"),
    (
        "system",
        140.0,
        Yes,
        "electrical system id (empty: from the jumpers, or earthed)",
    ),
    ("bundle", 55.0, Yes, "subconductors"),
    ("spacing m", 70.0, Yes, "subconductor spacing"),
    (
        "bundle angle °",
        90.0,
        Yes,
        "rotation of the bundle, degrees counter-clockwise looking ahead; 0: the two highest subconductors level (a triple bundle: two side by side above one)",
    ),
    ("status", 95.0, Yes, "in the blocks (rolling) or clipped in"),
    (
        "condition",
        120.0,
        Yes,
        "tension · sag, ruling span · sag, span N (clipped: N lengths); give the value too",
    ),
    (
        "value",
        70.0,
        Yes,
        "tension, kN, or sag, m (clipped: empty)",
    ),
    (
        "temp °C",
        65.0,
        Yes,
        "temperature of the condition (or of the lengths)",
    ),
    (
        "points",
        320.0,
        Yes,
        "structure set:phase, in order, separated by commas",
    ),
    (
        "display cases",
        160.0,
        Yes,
        "weather cases it is drawn in, separated by ; (the first is reported; empty: the case shown)",
    ),
    (
        "H kN",
        65.0,
        Yes,
        "horizontal tension of the first span in each display case (\"above RTS\" where a span's tension at its most tensioned support exceeds the wire's rated tensile strength) — type a target (one number: in the first display case): the condition giving it is found",
    ),
    (
        "max sag m",
        75.0,
        Yes,
        "largest sag of its spans in each display case — type a target (one number: in the first display case): the condition giving it is found",
    ),
    (
        "calculation",
        100.0,
        Yes,
        "wire condition (Initial, after Creep, after Load, Max Sag of the two) and method (RS: ruling span, FE: finite elements); Creep and Load use Line › Criteria › Creep-Stretch",
    ),
    (
        "colour",
        100.0,
        Yes,
        "display colour #rrggbb (empty: its phase's); typed for one section, every section of its conductor path takes it",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
    (
        "weather groups",
        150.0,
        Yes,
        "its weather case groups (Line › Criteria › Weather case groups…), ticked: a check with the section override ticked takes on it only its cases in these groups (e.g. the snow load along the line); empty: the check's cases",
    ),
    (
        "local winds",
        130.0,
        Yes,
        "local winds: spans whose wind is the weather case's times a factor, in every case with wind (e.g. 2–4: 1.2; 6: 0.7 — spans counted from 1, a run a–b); empty: the case's wind everywhere. Typed over many rows, each section takes it",
    ),
];

pub(super) const INSULATORS: &[Column] = &[
    (
        "label",
        150.0,
        New,
        "the library label: strings carry it (their weight, wind area and stiffness come from here)",
    ),
    ("name", 180.0, Yes, "what it is, in words"),
    (
        "kind",
        90.0,
        Yes,
        "suspension (one string), strain (a dead end) or two strings (to one wire point: legs = 2 in the file; v_string still read)",
    ),
    (
        "length",
        70.0,
        Yes,
        "m, top to wire point: the strings with this label take it (their wire points move; two strings to one wire point: where both meet)",
    ),
    (
        "weight",
        70.0,
        Yes,
        "N, the whole assembly (two strings: shared by them)",
    ),
    ("wind area", 70.0, Yes, "m², the area the wind blows on"),
    ("stiffness", 90.0, Yes, "N, axial stiffness EA"),
    (
        "half width",
        70.0,
        Yes,
        "m, two strings to one wire point: across from the centre to each one's top",
    ),
    (
        "max load",
        80.0,
        Yes,
        "N, the tension it is rated for (two strings: each one's) — strings with this label take it unless they give their own; not checked by OpenOHL (for a plugin or a report); empty: not given",
    ),
    (
        "max swing",
        70.0,
        Yes,
        "°, the swing from straight down it may take, any direction (taken by its strings; not checked); empty: not given",
    ),
    (
        "max across",
        75.0,
        Yes,
        "°, the swing across the line it may take (taken by its strings; not checked); empty: not given",
    ),
    (
        "max along",
        70.0,
        Yes,
        "°, the swing along the line it may take (taken by its strings; not checked); empty: not given",
    ),
    ("source", 160.0, Yes, "where its values come from"),
    (
        "used",
        50.0,
        Derived,
        "strings with this label in the model (the structures' own and their types')",
    ),
    (
        "swing across",
        85.0,
        Yes,
        "free or blocked: whether its shapes (String shapes) turn with the string's swing across the line; blocked: they keep their still-air angle (the internal clearances)",
    ),
    (
        "swing along",
        85.0,
        Yes,
        "free or blocked: whether its shapes turn with the string's swing along the line",
    ),
    (
        "twist",
        70.0,
        Yes,
        "free or blocked: about the string's axis; free: its shapes' across axis is across the conductor at the wire point, blocked: the structure's",
    ),
    (
        "strings",
        60.0,
        Yes,
        "several strings in one assembly (1 to 4): the line's string stands for all of them, on their middle; each string carries a copy of its shapes (a shared one: once)",
    ),
    (
        "arrangement",
        90.0,
        Yes,
        "several strings: across (side by side across the line), along (one behind the other) or triangle (three)",
    ),
    (
        "spacing",
        70.0,
        Yes,
        "m, between the strings' axes (several strings); empty: one string",
    ),
    (
        "shapes",
        55.0,
        Derived,
        "its volumes: edited in Line › Structures › String shapes",
    ),
];

pub(super) const STRING_SHAPES: &[Column] = &[
    (
        "library label",
        150.0,
        New,
        "the insulator assembly carrying it (String library): every string with that label has it; a line's string may override it by name",
    ),
    ("shape", 130.0, Yes, "its name, unique on the assembly"),
    ("kind", 80.0, Yes, "sphere, cylinder, box or cone"),
    (
        "state",
        90.0,
        Yes,
        "grounded (at the structure's potential: top fittings, a tower-side horn), electrified (at the line's: the clamp, a line-side horn, a corona ring, the yoke) or insulated (the insulator itself: drawn, not a clearance target)",
    ),
    (
        "at",
        70.0,
        Yes,
        "its centre along the string from its top: a fraction of the length (0 the top, 1 the wire point), or metres with m (0.3 m)",
    ),
    (
        "diameter",
        70.0,
        Yes,
        "m: sphere, cylinder; a cone's at its end nearer the top",
    ),
    (
        "end diameter",
        80.0,
        Yes,
        "m: a cone's at its end nearer the wire point (0: a point)",
    ),
    (
        "length",
        65.0,
        Yes,
        "m, on the string's axis: cylinder, cone, box",
    ),
    ("width", 60.0, Yes, "m, a box's across"),
    ("depth", 60.0, Yes, "m, a box's along"),
    (
        "offset across",
        85.0,
        Yes,
        "m, its centre off the string's axis, across (an arcing horn beside it)",
    ),
    (
        "offset along",
        85.0,
        Yes,
        "m, its centre off the string's axis, along",
    ),
    (
        "shared",
        60.0,
        Yes,
        "yes: one for an assembly of several strings, on its axis (a yoke plate); else one on each string",
    ),
];

pub(super) const STRINGS: &[Column] = &[
    ("structure", 110.0, New, "the structure it hangs on"),
    ("string", 90.0, Yes, "its label on the structure"),
    (
        "from",
        90.0,
        Yes,
        "its top: a point of the structure, or another string's wire point",
    ),
    ("to", 90.0, Yes, "its wire point"),
    (
        "sets",
        60.0,
        Derived,
        "the sets (wire ends) hanging on it and the strings joined to it",
    ),
    ("length m", 65.0, Derived, "from its top to its wire point"),
    ("weight N", 75.0, Yes, "its weight"),
    (
        "library label",
        110.0,
        Yes,
        "its entry in the string library (Line › Structures › String library: its weight, wind area and stiffness come from there); empty: its own values — a label is given when applied",
    ),
    (
        "compression N",
        90.0,
        Yes,
        "compression it may take (empty: tension only; beyond it: flagged)",
    ),
    (
        "clamp",
        80.0,
        Yes,
        "suspension: the wire runs through, the string hangs; strain: a tension string at a dead end, lined up with its conductor. Shown dimmed: from where it is (a section ends at its set), not set on the string",
    ),
    (
        "wind area m²",
        80.0,
        Yes,
        "area the wind blows on, half at each end (empty: its assembly's, if any)",
    ),
    (
        "max load N",
        85.0,
        Yes,
        "the tension it is rated for — kept, not checked by OpenOHL (for a plugin or a report); empty: not given. Shown dimmed: its assembly's",
    ),
    (
        "max swing °",
        80.0,
        Yes,
        "the swing from straight down it may take, any direction — kept, not checked; empty: not given. Shown dimmed: its assembly's",
    ),
    (
        "max across °",
        85.0,
        Yes,
        "the swing across the line it may take, either side — kept, not checked; empty: not given. Shown dimmed: its assembly's",
    ),
    (
        "max along °",
        80.0,
        Yes,
        "the swing along the line it may take, either way — kept, not checked; empty: not given. Shown dimmed: its assembly's",
    ),
    (
        "sections",
        60.0,
        Derived,
        "sections through its wire points",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

pub(super) const SPACERS: &[Column] = &[
    ("id", 90.0, Derived, "unique id"),
    ("a", 150.0, Derived, "one end: section, span point"),
    ("b", 150.0, Derived, "the other end: section, span point"),
    (
        "insulator",
        150.0,
        Yes,
        "insulator assembly id (empty: its own values)",
    ),
    (
        "length m",
        75.0,
        Yes,
        "its length (empty: the distance between its ends where the wires hang at stringing)",
    ),
    ("weight N", 70.0, Yes, "weight (half at each end)"),
    ("wind area m²", 85.0, Yes, "area the wind blows on"),
    ("EA N", 90.0, Yes, "axial stiffness (empty: the default)"),
    (
        "compression N",
        95.0,
        Yes,
        "compression it may take, flagged beyond (empty: tension only)",
    ),
    (
        "force N",
        75.0,
        Derived,
        "axial force in the case shown (tension positive)",
    ),
    (
        "state",
        110.0,
        Derived,
        "in the case shown: ok, slack, or over its compression limit",
    ),
    ("description", 200.0, Yes, "what it is"),
];

pub(super) const SPAN_OBJECTS: &[Column] = &[
    ("on", 100.0, Derived, "its section, or its jumper"),
    (
        "span",
        45.0,
        Derived,
        "span of the section (1 = first; — on a jumper)",
    ),
    (
        "kind",
        110.0,
        Derived,
        "a bare point, a point with its object (body or clamp, its library type), or a spread between two points",
    ),
    (
        "label",
        90.0,
        Yes,
        "label (a point's: relabelled, its spreads, spacers and taps follow)",
    ),
    (
        "at",
        110.0,
        Yes,
        "a point on a span: a fraction (0.25), a distance from the back structure (120 m) or a station (st. 1520 m); on a jumper: a fraction, a length along it (3 m along) or a distance from its from clamp (2.5 m); a spread: the point it starts at",
    ),
    ("end", 80.0, Yes, "a spread: the point it ends at"),
    (
        "load",
        70.0,
        Yes,
        "an object's weight, N; a spread's load, N/m",
    ),
    (
        "shape",
        70.0,
        Yes,
        "an object's shape: point, sphere, cylinder, box or areas",
    ),
    (
        "size 1",
        60.0,
        Yes,
        "sphere and cylinder: diameter, m; box: along, m; areas: wind area, m²",
    ),
    (
        "size 2",
        60.0,
        Yes,
        "cylinder: length, m; box: across, m; areas: ice surface, m²",
    ),
    ("size 3", 60.0, Yes, "box: height, m"),
    (
        "axis",
        60.0,
        Yes,
        "a cylinder's axis: along (the wire), across (the line) or vertical",
    ),
    ("drag", 50.0, Yes, "drag coefficient (empty: 1)"),
    (
        "load on",
        70.0,
        Yes,
        "a spread's load: on the whole bundle (bundle, empty) or on each subconductor (each)",
    ),
];

pub(super) const LOAD_TYPES: &[Column] = &[
    ("id", 120.0, New, "unique id"),
    (
        "source",
        70.0,
        Derived,
        "shipped template, or the project's own (a template changed here becomes the project's)",
    ),
    ("name", 150.0, Yes, "name shown"),
    ("kind", 60.0, Yes, "body, clamp or spread"),
    ("weight N", 70.0, Yes, "a body's or a clamp's weight"),
    ("shape", 70.0, Yes, "point, sphere, cylinder, box or areas"),
    (
        "size 1",
        60.0,
        Yes,
        "sphere and cylinder: diameter, m; box: along, m; areas: wind area, m²",
    ),
    (
        "size 2",
        60.0,
        Yes,
        "cylinder: length, m; box: across, m; areas: ice surface, m²",
    ),
    ("size 3", 60.0, Yes, "box: height, m"),
    (
        "axis",
        60.0,
        Yes,
        "a cylinder's axis: along, across or vertical",
    ),
    ("drag", 50.0, Yes, "drag coefficient (empty: 1)"),
    (
        "clamp length m",
        90.0,
        Yes,
        "a clamp's length along the wire",
    ),
    ("load N/m", 70.0, Yes, "a spread type's load"),
    ("description", 200.0, Yes, "what it is"),
    (
        "load on",
        70.0,
        Yes,
        "a spread type's load: on the whole bundle (bundle, empty) or on each subconductor (each)",
    ),
];

pub(super) const JUMPERS: &[Column] = &[
    ("id", 110.0, Yes, "unique id"),
    ("from", 150.0, Yes, "structure set:phase"),
    ("to", 150.0, Yes, "structure set:phase"),
    (
        "cable",
        170.0,
        Yes,
        "wire type id (new rows, empty: the section's at from)",
    ),
    ("length m", 75.0, Yes, "length (or give a sag)"),
    ("sag m", 65.0, Yes, "sag (or give a length)"),
    ("temp °C", 65.0, Yes, "temperature of the length or sag"),
    ("bundle", 60.0, Yes, "subconductors (empty: its section's)"),
    (
        "spacing m",
        70.0,
        Yes,
        "subconductor spacing (empty: its section's)",
    ),
    (
        "EI N·m²",
        70.0,
        Yes,
        "bending stiffness (empty: estimated from the wire type with the factor; 0: hinged ends)",
    ),
    (
        "EI factor",
        65.0,
        Yes,
        "estimated EI: this share of the wire's solid-bar stiffness, 0–1 (empty: 0.2)",
    ),
    (
        "angle from °",
        75.0,
        Yes,
        "leaves the clamp at from at this angle from its tension string, turned towards the load (empty: 30)",
    ),
    (
        "angle to °",
        75.0,
        Yes,
        "leaves the clamp at to at this angle from its tension string, turned towards the load (empty: 30)",
    ),
    (
        "rigid",
        50.0,
        Yes,
        "yes: a rigid jumper (tube or bar), straight between its clamps — no length or sag",
    ),
    (
        "helping strings",
        200.0,
        Derived,
        "helping strings holding it, in order: structure, wire point, where along (share of the length), clamp length (edit them in the jumper's Edit form)",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

pub(super) const SYSTEMS: &[Column] = &[
    (
        "id",
        160.0,
        Yes,
        "unique id (renaming: the sections follow)",
    ),
    ("name", 200.0, Yes, "name"),
    ("kind", 50.0, Yes, "AC or DC"),
    ("kV", 70.0, Yes, "nominal voltage"),
    ("Hz", 60.0, Yes, "frequency (AC; empty: none)"),
    (
        "phases",
        160.0,
        Yes,
        "phase labels, e.g. L1 L2 L3 (angles kept by position)",
    ),
    (
        "line codes",
        110.0,
        Yes,
        "how each phase is drawn when wires are coloured by system, in the order of the phases, separated by spaces: - solid, else its dots and dashes repeated (e.g. - .- .--); empty: by place",
    ),
    (
        "colour",
        100.0,
        Yes,
        "display colour #rrggbb when wires are coloured by system (Line › Sections › Display options); empty: by its place",
    ),
    ("sections", 70.0, Derived, "sections given this system"),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
    (
        "owner",
        120.0,
        Yes,
        "who owns it (free text; the phase plan's table of circuits)",
    ),
    (
        "number",
        90.0,
        Yes,
        "its number in the owner's register (free text; the phase plan's table of circuits)",
    ),
];

pub(super) const ALIGNMENTS: &[Column] = &[
    (
        "alignment",
        120.0,
        Yes,
        "unique id (renaming: nothing else changes)",
    ),
    ("name", 140.0, Yes, "display name"),
    (
        "route",
        120.0,
        Yes,
        "the route it belongs to (one; required — none yet: create one above)",
    ),
    (
        "#",
        45.0,
        Yes,
        "order of the alignments (along their routes); 3.5 goes between 3 and 4",
    ),
    (
        "points",
        60.0,
        Derived,
        "its points (Alignment points table)",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

pub(super) const POINTS: &[Column] = &[
    (
        "alignment",
        120.0,
        Yes,
        "the alignment of this row (a point on several has a row on each); another moves it there, none takes it off (it stays)",
    ),
    (
        "#",
        45.0,
        Yes,
        "its place on that alignment; 3.5 goes between 3 and 4",
    ),
    (
        "point",
        70.0,
        New,
        "its id; a new row with an existing id puts that point on the alignment too (shared: a junction)",
    ),
    ("E", 110.0, Yes, "easting, m"),
    ("N", 110.0, Yes, "northing, m"),
    ("z", 80.0, Yes, "height, m"),
    (
        "structure",
        120.0,
        Yes,
        "tied under this structure (it follows it); empty: free",
    ),
    (
        "also on",
        140.0,
        Derived,
        "the other alignments it is on, with its place there",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

pub(super) const ROUTES: &[Column] = &[
    (
        "route",
        120.0,
        Yes,
        "unique id (renaming: its alignments and structures follow)",
    ),
    ("name", 160.0, Yes, "display name"),
    (
        "alignments",
        200.0,
        Derived,
        "its alignments, in order (Alignments table: their route)",
    ),
    ("structures", 80.0, Derived, "structures on it"),
    ("sections", 70.0, Derived, "sections on it"),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

pub(super) const CIRCUIT_LIST: &[Column] = &[
    (
        "id",
        110.0,
        Yes,
        "unique id (--circuits takes it or the name)",
    ),
    (
        "name",
        140.0,
        Yes,
        "unique name, what its sections give (renaming: they follow). Circuits are not merged: they become one only when connected (a jumper, a wire strung through)",
    ),
    (
        "system",
        110.0,
        Yes,
        "the electrical system its conductor paths are of (empty: not said); a path of another system is refused",
    ),
    (
        "members",
        200.0,
        Yes,
        "its conductor paths, each by its first section (as the Conductor paths table); a section typed takes its whole path. A path in another circuit: take it out there in the same Apply, or use Assign to circuit in Properties. Two paths of one phase that are not connected are refused",
    ),
    ("phases", 90.0, Derived, "its conductor paths' phases"),
    ("sections", 200.0, Derived, "its sections"),
    (
        "notes",
        260.0,
        Derived,
        "no conductor path (allowed), or what contradicts the connections (files written before)",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

pub(super) const CIRCUITS: &[Column] = &[
    (
        "path",
        70.0,
        Derived,
        "one per conductor path: sections joined by jumpers or a shared attachment (counted from the connections)",
    ),
    (
        "circuit",
        110.0,
        Yes,
        "the name of the circuit it is in, for all its sections (\"mixed: …\" when they differ; empty: none). A new name makes a circuit (Circuits table); a circuit's other path of the same phase, not connected, is refused",
    ),
    (
        "phase",
        70.0,
        Yes,
        "phase label, for all its sections (\"mixed: …\" when they differ)",
    ),
    (
        "system",
        110.0,
        Yes,
        "electrical system, for all its sections (empty: earthed)",
    ),
    (
        "starts",
        150.0,
        Yes,
        "its ends that are starts (power flows in): tick them; at least one start and one end; ⬌ swaps starts and ends",
    ),
    (
        "ends",
        200.0,
        Derived,
        "its other ends (power flows out), along it; a conductor path with taps has several",
    ),
    ("sections", 240.0, Derived, "its sections, from its starts"),
    ("jumpers", 200.0, Derived, "its jumpers, from its starts"),
    (
        "colour",
        100.0,
        Yes,
        "display colour #rrggbb for all its sections (and its jumpers with their own colour); empty: their phases' (\"mixed: …\" when they differ)",
    ),
];

// Columns of the weather case table (as in the criteria tables of other
// line design programs: name, then the unit; every row the same widths).
pub(super) const W_ORDER: usize = 0;
pub(super) const W_NAME: usize = 1;
pub(super) const W_DESCRIPTION: usize = 2;
pub(super) const W_TAGS: usize = 3;
pub(super) const W_Q: usize = 4;
pub(crate) const W_VELOCITY: usize = 5;
pub(super) const W_PRESSURE: usize = 6;
pub(super) const W_ICE: usize = 7;
pub(super) const W_ICE_DENSITY: usize = 8;
pub(super) const W_ICE_LOAD: usize = 9;
pub(super) const W_TEMP: usize = 10;
pub(super) const W_AMBIENT: usize = 11;

pub(super) const WEATHER: &[Column] = &[
    (
        "#\n ",
        40.0,
        Yes,
        "order in the file; 2.5 goes between 2 and 3",
    ),
    ("Name\n ", 170.0, Yes, "unique name"),
    ("Description\n ", 190.0, Yes, "free text"),
    (
        "Tags\n ",
        130.0,
        Yes,
        "comma separated, e.g. clearance, loads",
    ),
    (
        "Air density factor\nPa/(m/s)²",
        120.0,
        Yes,
        "Q: the wind pressure of a velocity V is Q·V² (empty: 0.613, air at 15 °C at sea level)",
    ),
    (
        "Wind velocity\nm/s",
        95.0,
        Yes,
        "V = √(P / Q); typed in, the pressure becomes Q·V²",
    ),
    (
        "Wind pressure\nPa",
        95.0,
        Yes,
        "on the projected (iced) diameter; what the loads use",
    ),
    (
        "Ice thickness\nmm",
        95.0,
        Yes,
        "radial ice thickness (0: none)",
    ),
    (
        "Ice density\nkg/m³",
        90.0,
        Yes,
        "empty: glaze ice (913) with a thickness; with a load: the iced diameter for the wind",
    ),
    (
        "Ice load\nN/m",
        80.0,
        Yes,
        "or: ice load per length (instead of a thickness; empty: none)",
    ),
    ("Wire temp\n°C", 80.0, Yes, "wire temperature"),
    (
        "Ambient temp\n°C",
        90.0,
        Yes,
        "air temperature, for ratings (empty: not given)",
    ),
];
