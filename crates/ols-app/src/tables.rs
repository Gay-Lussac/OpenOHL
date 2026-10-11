// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Table editing: structures, sections, strings, jumpers, electrical
//! systems, alignment points and weather cases, one row per object.
//!
//! Every value shown can be typed in, except counts derived from other
//! rows; **+ Add row** adds objects, 🗑 marks a row for deletion. Changes
//! stay pending (yellow) until **Apply**, which makes one session command
//! for all of them (one undo) through the same code as the editor windows
//! — the weather cases are written to the model's case file instead.
//! Deletions taking other objects with it are listed and confirmed first.
//! Results (a section's tension and largest sag) are typed as targets:
//! the section's condition is found that gives them. Rows can be filtered
//! and sorted; ⤴ selects a row's object and brings the views to it.
//! Colour cells are drawn in their colour and open the colour tool
//! ([`crate::color_tool`]), which writes the colour chosen as a typed edit.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use eframe::egui;
use ols_line::{SectionPoint, Stringing, Structure};
use ols_session::{Command, Kind};

use crate::editors::{SectionForm, StringForm, StructureForm};
use crate::model::Model;
use crate::selection::Selection;

/// A table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TableKind {
    /// Structures.
    Structures,
    /// Sections.
    Sections,
    /// Strings (attachment points).
    Strings,
    /// Jumpers.
    Jumpers,
    /// Electrical systems.
    Systems,
    /// Electrical circuits: the conductor paths, from the connections.
    Circuits,
    /// Alignments: each on one route, in their order.
    Alignments,
    /// Alignment points: one row per point and alignment it is on (its
    /// place there), and one for a point on no alignment.
    AlignmentPoints,
    /// Routes: paths of the line, made of alignments.
    Routes,
    /// Weather cases (of the model's weather case file).
    WeatherCases,
}

/// How a column is typed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// Always.
    Yes,
    /// In new rows only (what an object belongs to).
    New,
    /// Never: derived from other rows (a count).
    Derived,
}

/// A column: name, width (points), how it is edited, help.
pub type Column = (&'static str, f32, Edit, &'static str);

use Edit::{Derived, New, Yes};

const STRUCTURES: &[Column] = &[
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
        "height extension of the type (empty: none)",
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
];

const SECTIONS: &[Column] = &[
    ("id", 90.0, Yes, "unique id"),
    ("cable", 170.0, Yes, "cable id from the wire library"),
    ("circuit", 150.0, Yes, "circuit"),
    ("phase", 50.0, Yes, "phase label"),
    (
        "system",
        140.0,
        Yes,
        "electrical system id (empty: from the jumpers, or earthed)",
    ),
    ("bundle", 55.0, Yes, "subconductors"),
    ("spacing m", 70.0, Yes, "subconductor spacing"),
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
        "horizontal tension of the first span in each display case — type a target (one number: in the first display case): the condition giving it is found",
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
        "cable condition (Initial, after Creep, after Load, Max Sag of the two) and method (RS: ruling span, FE: finite elements); Creep and Load use Criteria › Creep-Stretch",
    ),
    (
        "colour",
        100.0,
        Yes,
        "display colour #rrggbb (empty: its phase's); typed for one section, every section of its circuit takes it",
    ),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

const STRINGS: &[Column] = &[
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
        "insulator",
        110.0,
        Yes,
        "insulator assembly id (empty: none)",
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
        "area the wind blows on, half at each end (empty: its insulator's, if any)",
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

const JUMPERS: &[Column] = &[
    ("id", 110.0, Yes, "unique id"),
    ("from", 150.0, Yes, "structure set:phase"),
    ("to", 150.0, Yes, "structure set:phase"),
    (
        "cable",
        170.0,
        Yes,
        "cable id (new rows, empty: the section's at from)",
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
        "bending stiffness (empty: estimated from the cable with the factor; 0: hinged ends)",
    ),
    (
        "EI factor",
        65.0,
        Yes,
        "estimated EI: this share of the cable's solid-bar stiffness, 0–1 (empty: 0.2)",
    ),
    (
        "angle from °",
        75.0,
        Yes,
        "leaves the clamp at from this far below the conductor (empty: 30)",
    ),
    (
        "angle to °",
        75.0,
        Yes,
        "leaves the clamp at to this far below the conductor (empty: 30)",
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

const SYSTEMS: &[Column] = &[
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
        "display colour #rrggbb when wires are coloured by system (Sections › Display options); empty: by its place",
    ),
    ("sections", 70.0, Derived, "sections given this system"),
    (
        "description",
        220.0,
        Yes,
        "what it is, in words (free text)",
    ),
];

const ALIGNMENTS: &[Column] = &[
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

const POINTS: &[Column] = &[
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

const ROUTES: &[Column] = &[
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

const CIRCUITS: &[Column] = &[
    (
        "circuit",
        70.0,
        Derived,
        "one per conductor path: sections joined by jumpers or a shared attachment (counted from the connections)",
    ),
    (
        "name",
        110.0,
        Yes,
        "circuit name, for all its sections (\"mixed: …\" when they differ)",
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
        "its ends that are starts (power flows in): tick them; at least one start and one end; ⇄ swaps starts and ends",
    ),
    (
        "ends",
        200.0,
        Derived,
        "its other ends (power flows out), along it; a circuit with taps has several",
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
const W_ORDER: usize = 0;
const W_NAME: usize = 1;
const W_DESCRIPTION: usize = 2;
const W_TAGS: usize = 3;
const W_Q: usize = 4;
const W_VELOCITY: usize = 5;
const W_PRESSURE: usize = 6;
const W_ICE: usize = 7;
const W_ICE_DENSITY: usize = 8;
const W_ICE_LOAD: usize = 9;
const W_TEMP: usize = 10;
const W_AMBIENT: usize = 11;

const WEATHER: &[Column] = &[
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
    ("Wire temp\n°C", 80.0, Yes, "cable temperature"),
    (
        "Ambient temp\n°C",
        90.0,
        Yes,
        "air temperature, for ratings (empty: not given)",
    ),
];

/// One row: the object's key, what selecting it selects, the cells.
pub struct Row {
    /// Key of the object.
    pub key: String,
    /// The object, for the views.
    pub select: Selection,
    /// Cell texts.
    pub cells: Vec<String>,
    /// Cells showing a default (not stored on the object; drawn dimmed,
    /// left unset on Apply unless typed in); missing: none.
    pub defaulted: Vec<bool>,
}

fn f(v: f64, decimals: usize) -> String {
    let t = format!("{v:.decimals$}");
    // No "-0.000".
    if t.starts_with('-') && t[1..].chars().all(|c| c == '0' || c == '.') {
        t[1..].to_owned()
    } else {
        t
    }
}

fn point(p: &SectionPoint) -> String {
    format!("{} {}", p.structure, p.set)
}

/// "T3 2" → a section point (files written before: "T3 2:1", the set
/// [`ols_line::assembly::set_from_v1`]).
fn parse_point(text: &str) -> Result<SectionPoint, String> {
    let t = text.trim();
    let (s, sp) = t
        .rsplit_once(' ')
        .ok_or_else(|| format!("\"{t}\": write structure and set, e.g. T3 2"))?;
    let set = match parse_set_phase(sp) {
        Ok((set, phase)) => ols_line::assembly::set_from_v1(set, phase),
        Err(_) => sp
            .trim()
            .parse()
            .map_err(|_| format!("\"{t}\": write structure and set, e.g. T3 2"))?,
    };
    Ok(SectionPoint::new(s.trim(), set))
}

fn parse_set_phase(text: &str) -> Result<(u32, u32), String> {
    text.trim()
        .split_once(':')
        .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)))
        .ok_or_else(|| format!("\"{text}\": write set:phase, e.g. 2:1"))
}

fn num(text: &str, what: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .map_err(|_| format!("{what}: \"{text}\" is not a number"))
}

fn opt_num(text: &str, what: &str) -> Result<Option<f64>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        num(text, what).map(Some)
    }
}

fn opt(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

/// The same put for another id.
fn retarget(command: Command, new_id: &str) -> Command {
    match command {
        Command::Put {
            kind,
            mut value,
            ord,
            ..
        } => {
            value["id"] = serde_json::Value::String(new_id.to_owned());
            Command::Put {
                kind,
                id: new_id.to_owned(),
                value,
                ord,
            }
        }
        other => other,
    }
}

/// The text of each column: the row's, or as typed.
struct Cells<'a> {
    row: &'a [String],
    typed: Option<&'a BTreeMap<usize, String>>,
    /// Cells showing a default: read as empty unless typed in.
    defaulted: &'a [bool],
}

impl Cells<'_> {
    fn get(&self, c: usize) -> String {
        if let Some(t) = self.typed.and_then(|e| e.get(&c)) {
            return t.clone();
        }
        if self.defaulted.get(c).copied().unwrap_or(false) {
            return String::new();
        }
        self.row[c].clone()
    }

    fn changed(&self, c: usize) -> bool {
        self.typed.is_some_and(|e| e.contains_key(&c))
    }
}

/// What a table's changes do.
#[derive(Default)]
pub struct Changes {
    /// Session commands (one log entry).
    pub commands: Vec<(String, Command)>,
    /// A file to write (weather cases): path, text.
    pub file: Option<(PathBuf, String)>,
    /// Sections' display weather cases (none: the case shown).
    pub display: Vec<(String, Vec<usize>)>,
    /// What deletions take with them, to confirm.
    pub takes: Vec<String>,
}

/// Shows the values used where an object leaves a value to its default
/// (dimmed; left unset on Apply unless typed in): a section's electrical
/// system from its conductor path and the weather cases it is drawn in, a
/// strain string's estimated weight, a jumper's bundle (from its section),
/// bending stiffness (estimated from its cable) and clamp exit angles.
fn defaults(kind: TableKind, model: &Model, case: usize, rows: &mut [Row]) {
    let line = &model.line;
    let set = |row: &mut Row, c: usize, text: String| {
        if row.cells[c].is_empty() || row.defaulted.get(c).copied().unwrap_or(false) {
            row.cells[c] = text;
            row.defaulted.resize(row.cells.len(), false);
            row.defaulted[c] = true;
        }
    };
    match kind {
        TableKind::Sections => {
            // The system of a conductor path: that of any of its sections.
            let mut system: std::collections::HashMap<&str, String> =
                std::collections::HashMap::new();
            for path in ols_line::electrical::conductor_paths(line) {
                let given = path.iter().find_map(|id| {
                    line.sections
                        .iter()
                        .find(|s| &s.id == id)
                        .and_then(|s| s.system.clone())
                });
                if let Some(g) = given {
                    for id in &path {
                        if let Some(s) = line.sections.iter().find(|s| &s.id == id) {
                            system.insert(s.id.as_str(), g.clone());
                        }
                    }
                }
            }
            let shown: Vec<String> = model
                .shown(case)
                .iter()
                .filter_map(|k| model.cases.get(*k))
                .map(|c| c.case.name.clone())
                .collect();
            for row in rows.iter_mut() {
                set(
                    row,
                    4,
                    system
                        .get(row.key.as_str())
                        .cloned()
                        .unwrap_or_else(|| "earthed".into()),
                );
                set(row, 12, shown.join("; "));
            }
        }
        TableKind::Strings => {
            // Weights estimated for strain strings given without one.
            let mut weight: std::collections::HashMap<(&str, String), f64> =
                std::collections::HashMap::new();
            if let Some(solved) = model.cases.get(case) {
                for shape in &solved.shapes {
                    let Some(sec) = line.sections.iter().find(|s| s.id == shape.section) else {
                        continue;
                    };
                    for ((p, w), strings) in sec
                        .points
                        .iter()
                        .zip(&shape.estimated_weights)
                        .zip(&shape.strings)
                    {
                        if let (Some(w), [x]) = (w, strings.as_slice()) {
                            weight.insert((p.structure.as_str(), x.label.clone()), *w);
                        }
                    }
                }
            }
            for row in rows.iter_mut() {
                let (structure, label) = row.key.split_once('\t').unwrap_or((&row.key, ""));
                if let Some(w) = weight.get(&(structure, label.to_owned())).copied() {
                    let w = f(w, 1);
                    row.cells[6].clear();
                    set(row, 6, w);
                }
            }
        }
        TableKind::Jumpers => {
            for row in rows.iter_mut() {
                let Some(j) = line.jumpers.iter().find(|j| j.id == row.key) else {
                    continue;
                };
                let b = line.jumper_bundle(j);
                set(row, 7, b.count.to_string());
                if b.count > 1 {
                    set(row, 8, b.spacing.map_or(String::new(), |v| f(v, 3)));
                }
                if let Some(c) = model.library.get(&j.cable) {
                    set(
                        row,
                        9,
                        f(ols_line::jumper::jumper_bending_stiffness(j, c), 1),
                    );
                }
                set(row, 10, f(ols_line::jumper::DEFAULT_STIFFNESS_FACTOR, 2));
                let angle = f(ols_line::jumper::DEFAULT_TERMINAL_ANGLE_DEG, 1);
                set(row, 11, angle.clone());
                set(row, 12, angle);
            }
        }
        TableKind::WeatherCases => {
            let q = f(ols_weather::DEFAULT_AIR_DENSITY_FACTOR, 3);
            for row in rows.iter_mut() {
                set(row, W_Q, q.clone());
            }
        }
        _ => {}
    }
}

impl TableKind {
    /// Every table (the Tables menu).
    pub const ALL: [TableKind; 10] = [
        TableKind::Structures,
        TableKind::Sections,
        TableKind::Strings,
        TableKind::Jumpers,
        TableKind::Systems,
        TableKind::Circuits,
        TableKind::Routes,
        TableKind::Alignments,
        TableKind::AlignmentPoints,
        TableKind::WeatherCases,
    ];

    /// Title of its page.
    pub fn title(self) -> &'static str {
        match self {
            TableKind::Structures => "Structures",
            TableKind::Sections => "Sections",
            TableKind::Strings => "Strings",
            TableKind::Jumpers => "Jumpers",
            TableKind::Systems => "Electrical systems",
            TableKind::Circuits => "Electrical circuits",
            TableKind::Alignments => "Alignments",
            TableKind::AlignmentPoints => "Alignment points",
            TableKind::Routes => "Routes",
            TableKind::WeatherCases => "Weather cases",
        }
    }

    /// Key for saving.
    pub fn key(self) -> &'static str {
        match self {
            TableKind::Structures => "structures",
            TableKind::Sections => "sections",
            TableKind::Strings => "strings",
            TableKind::Jumpers => "jumpers",
            TableKind::Systems => "systems",
            TableKind::Circuits => "circuits",
            TableKind::Alignments => "alignment_list",
            TableKind::AlignmentPoints => "alignments",
            TableKind::Routes => "routes",
            TableKind::WeatherCases => "weather",
        }
    }

    /// Its columns.
    pub fn columns(self) -> &'static [Column] {
        match self {
            TableKind::Structures => STRUCTURES,
            TableKind::Sections => SECTIONS,
            TableKind::Strings => STRINGS,
            TableKind::Jumpers => JUMPERS,
            TableKind::Systems => SYSTEMS,
            TableKind::Circuits => CIRCUITS,
            TableKind::Alignments => ALIGNMENTS,
            TableKind::AlignmentPoints => POINTS,
            TableKind::Routes => ROUTES,
            TableKind::WeatherCases => WEATHER,
        }
    }

    /// The column called `name`.
    pub fn column(self, name: &str) -> usize {
        self.columns()
            .iter()
            .position(|c| c.0 == name)
            .unwrap_or_else(|| panic!("no column {name} in {}", self.title()))
    }

    /// The rows of the model (weather case `case` shown).
    /// The "none" of its multiple choices, if it has one (display cases:
    /// the cases shown; a circuit's starts need one).
    fn none_choice(self, column: usize) -> Option<&'static str> {
        match (self, self.columns()[column].0) {
            (TableKind::Circuits, _) => None,
            (TableKind::Structures, "routes") => Some("none (from the alignments)"),
            (_, "routes") => None,
            _ => Some("none (the cases shown)"),
        }
    }

    /// Its rows follow the model (the circuits, from the connections):
    /// none added or deleted here.
    pub fn fixed_rows(self) -> bool {
        self == TableKind::Circuits
    }

    pub fn rows(self, model: &Model, case: usize) -> Vec<Row> {
        let line = &model.line;
        let mut through: std::collections::HashMap<(&str, u32), usize> =
            std::collections::HashMap::new();
        let mut at_structure: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for s in &line.sections {
            for p in &s.points {
                *through.entry((p.structure.as_str(), p.set)).or_default() += 1;
            }
            let mut ids: Vec<&str> = s.points.iter().map(|p| p.structure.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            for id in ids {
                *at_structure.entry(id).or_default() += 1;
            }
        }
        let on_routes = line.structure_routes();
        let mut rows: Vec<Row> = match self {
            TableKind::Structures => line
                .structures
                .iter()
                .map(|s| Row {
                    // Routes from the alignments: dimmed.
                    defaulted: if s.routes.is_empty() {
                        let mut d = vec![false; STRUCTURES.len()];
                        d[TableKind::Structures.column("routes")] = true;
                        d
                    } else {
                        Vec::new()
                    },
                    key: s.id.clone(),
                    select: Selection::Structure(s.id.clone()),
                    cells: vec![
                        s.id.clone(),
                        s.structure_type.clone().unwrap_or_default(),
                        s.extension.clone().unwrap_or_default(),
                        f(s.orientation_deg, 3),
                        f(s.position[0], 3),
                        f(s.position[1], 3),
                        f(s.position[2], 3),
                        s.sets().count().to_string(),
                        at_structure
                            .get(s.id.as_str())
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                        if s.routes.is_empty() {
                            on_routes.get(&s.id).cloned().unwrap_or_default().join(", ")
                        } else {
                            s.routes.join(", ")
                        },
                        s.description.clone(),
                    ],
                })
                .collect(),
            TableKind::Routes => {
                let views = line.alignment_views();
                let (sections, _) = line.wire_routes();
                line.routes
                    .iter()
                    .map(|r| {
                        let on: Vec<&str> = views
                            .iter()
                            .filter(|a| a.route == r.id)
                            .map(|a| a.id.as_str())
                            .collect();
                        Row {
                            defaulted: Vec::new(),
                            key: r.id.clone(),
                            select: Selection::None,
                            cells: vec![
                                r.id.clone(),
                                r.name.clone(),
                                on.join(", "),
                                on_routes
                                    .values()
                                    .filter(|rs| rs.contains(&r.id))
                                    .count()
                                    .to_string(),
                                sections
                                    .values()
                                    .filter(|rs| rs.contains(&r.id))
                                    .count()
                                    .to_string(),
                                r.description.clone(),
                            ],
                        }
                    })
                    .collect()
            }
            TableKind::Sections => line
                .sections
                .iter()
                .map(|s| {
                    let (condition, value) = match &s.stringing {
                        Stringing::Tension {
                            horizontal_tension, ..
                        } => ("tension".to_owned(), f(horizontal_tension / 1000.0, 2)),
                        Stringing::Sag { sag, span, .. } => (
                            span.map_or("sag, ruling span".into(), |k| {
                                format!("sag, span {}", k + 1)
                            }),
                            f(*sag, 3),
                        ),
                        Stringing::UnstressedLengths { lengths, .. } => {
                            (format!("{} lengths", lengths.len()), String::new())
                        }
                    };
                    let shapes: Vec<&ols_line::solve::SectionShape> = model
                        .section_cases(&s.id, case)
                        .into_iter()
                        .filter_map(|k| {
                            model
                                .cases
                                .get(k)?
                                .shapes
                                .iter()
                                .find(|x| x.section == s.id)
                        })
                        .collect();
                    Row {
                        defaulted: Vec::new(),
                        key: s.id.clone(),
                        select: Selection::Span {
                            section: s.id.clone(),
                            span: 0,
                        },
                        cells: vec![
                            s.id.clone(),
                            s.cable.clone(),
                            s.circuit.clone(),
                            s.phase_label.clone(),
                            s.system.clone().unwrap_or_default(),
                            s.bundle.to_string(),
                            s.bundle_spacing.map_or(String::new(), |v| f(v, 3)),
                            if s.stringing.clipped() {
                                "clipped in".into()
                            } else {
                                "in the blocks".into()
                            },
                            condition,
                            value,
                            f(s.stringing.temperature(), 1),
                            s.points.iter().map(point).collect::<Vec<_>>().join(", "),
                            model.display_cases.get(&s.id).map_or(String::new(), |ks| {
                                ks.iter()
                                    .filter_map(|k| model.cases.get(*k))
                                    .map(|c| c.case.name.clone())
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            }),
                            // One value per display case.
                            shapes
                                .iter()
                                .filter_map(|x| x.spans.first())
                                .map(|sp| f(sp.horizontal_tension / 1000.0, 2))
                                .collect::<Vec<_>>()
                                .join(" / "),
                            shapes
                                .iter()
                                .map(|x| f(x.spans.iter().map(|sp| sp.sag).fold(0.0, f64::max), 3))
                                .collect::<Vec<_>>()
                                .join(" / "),
                            s.calculation.label().to_owned(),
                            s.color.clone().unwrap_or_default(),
                            s.description.clone(),
                        ],
                    }
                })
                .collect(),
            TableKind::Strings => line
                .structures
                .iter()
                .flat_map(|s| {
                    let through = &through;
                    s.strings.iter().map(move |x| {
                        let sets: Vec<u32> = s
                            .component(&x.wire)
                            .wires
                            .iter()
                            .filter_map(|&i| s.wires[i].set)
                            .collect();
                        let dead_end = line.is_dead_end(s, &x.wire);
                        Row {
                            // Not flagged: from where it is.
                            defaulted: (0..12).map(|c| c == 9 && !x.strain).collect(),
                            key: format!("{}\t{}", s.id, x.label),
                            select: match sets.first() {
                                Some(&set) => Selection::Attachment {
                                    structure: s.id.clone(),
                                    set,
                                },
                                None => Selection::Structure(s.id.clone()),
                            },
                            cells: vec![
                                s.id.clone(),
                                x.label.clone(),
                                x.top.clone(),
                                x.wire.clone(),
                                sets.iter()
                                    .map(u32::to_string)
                                    .collect::<Vec<_>>()
                                    .join(" "),
                                s.string_length(x).map_or(String::new(), |l| f(l, 3)),
                                f(x.weight, 1),
                                x.insulator.clone().unwrap_or_default(),
                                x.compression.map_or(String::new(), |c| f(c, 0)),
                                if dead_end { "strain" } else { "suspension" }.into(),
                                if x.wind_area > 0.0 {
                                    f(x.wind_area, 3)
                                } else {
                                    String::new()
                                },
                                sets.iter()
                                    .map(|set| {
                                        through.get(&(s.id.as_str(), *set)).copied().unwrap_or(0)
                                    })
                                    .sum::<usize>()
                                    .to_string(),
                                x.description.clone(),
                            ],
                        }
                    })
                })
                .collect(),
            TableKind::Jumpers => line
                .jumpers
                .iter()
                .map(|j| Row {
                    defaulted: Vec::new(),
                    key: j.id.clone(),
                    select: Selection::Jumper(j.id.clone()),
                    cells: vec![
                        j.id.clone(),
                        point(&j.from),
                        point(&j.to),
                        j.cable.clone(),
                        j.length.map_or(String::new(), |v| f(v, 3)),
                        j.sag.map_or(String::new(), |v| f(v, 3)),
                        f(j.temperature, 1),
                        j.bundle.map_or(String::new(), |v| v.to_string()),
                        j.bundle_spacing.map_or(String::new(), |v| f(v, 3)),
                        j.bending_stiffness.map_or(String::new(), |v| f(v, 1)),
                        j.stiffness_factor.map_or(String::new(), |v| f(v, 2)),
                        j.angle_from_deg.map_or(String::new(), |v| f(v, 1)),
                        j.angle_to_deg.map_or(String::new(), |v| f(v, 1)),
                        if j.rigid { "yes" } else { "no" }.into(),
                        clamps_text(j),
                        j.description.clone(),
                    ],
                })
                .collect(),
            TableKind::Systems => line
                .systems
                .iter()
                .enumerate()
                .map(|(k, s)| Row {
                    defaulted: (0..9)
                        .map(|c| {
                            (c == 6 && s.phases.iter().all(|p| p.code.is_none()))
                                || (c == 7 && s.color.is_none())
                        })
                        .collect(),
                    key: s.id.clone(),
                    select: Selection::None,
                    cells: vec![
                        s.id.clone(),
                        s.name.clone(),
                        match s.kind {
                            ols_line::electrical::SystemKind::Ac => "AC".into(),
                            ols_line::electrical::SystemKind::Dc => "DC".into(),
                        },
                        f(s.voltage_kv, 1),
                        s.frequency_hz.map_or(String::new(), |v| f(v, 1)),
                        s.phases
                            .iter()
                            .map(|p| p.label.clone())
                            .collect::<Vec<_>>()
                            .join(" "),
                        (0..s.phases.len())
                            .map(|p| s.line_code(p))
                            .collect::<Vec<_>>()
                            .join(" "),
                        line.system_color(k),
                        line.sections
                            .iter()
                            .filter(|x| x.system.as_deref() == Some(s.id.as_str()))
                            .count()
                            .to_string(),
                        s.description.clone(),
                    ],
                })
                .collect(),
            TableKind::Circuits => circuit_rows(line),
            TableKind::Alignments => line
                .alignments
                .iter()
                .enumerate()
                .map(|(k, a)| Row {
                    defaulted: Vec::new(),
                    key: a.id.clone(),
                    select: a.points.first().map_or(Selection::None, |p| {
                        Selection::AlignmentPoint { id: p.clone() }
                    }),
                    cells: vec![
                        a.id.clone(),
                        a.name.clone(),
                        a.route.clone(),
                        (k + 1).to_string(),
                        a.points.len().to_string(),
                        a.description.clone(),
                    ],
                })
                .collect(),
            TableKind::AlignmentPoints => point_rows(line),
            TableKind::WeatherCases => model
                .cases
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    let w = &c.case;
                    Row {
                        defaulted: Vec::new(),
                        key: k.to_string(),
                        select: Selection::None,
                        cells: {
                            let mut c = vec![String::new(); WEATHER.len()];
                            c[W_ORDER] = (k + 1).to_string();
                            c[W_NAME] = w.name.clone();
                            c[W_DESCRIPTION] = w.description.clone();
                            c[W_TAGS] = w.tags.join(", ");
                            c[W_Q] = w.air_density_factor.map_or(String::new(), |q| f(q, 3));
                            c[W_VELOCITY] = f(w.wind_velocity(), 2);
                            c[W_PRESSURE] = f(w.wind_pressure.si(), 1);
                            c[W_ICE] = f(w.ice_thickness.si() * 1000.0, 1);
                            c[W_ICE_DENSITY] =
                                w.ice_density.map_or(String::new(), |v| f(v.si(), 0));
                            c[W_ICE_LOAD] = w.ice_load.map_or(String::new(), |v| f(v.si(), 2));
                            c[W_TEMP] = f(w.temperature.celsius(), 1);
                            c[W_AMBIENT] = w
                                .ambient_temperature
                                .map_or(String::new(), |t| f(t.celsius(), 1));
                            c
                        },
                    }
                })
                .collect(),
        };
        defaults(self, model, case, &mut rows);
        rows
    }

    /// A new row's cells, before anything is typed.
    pub fn new_row(self, model: &Model, case: usize, added: &[Vec<String>]) -> Vec<String> {
        let line = &model.line;
        let n = self.columns().len();
        let mut cells = vec![String::new(); n];
        let taken_after = |ids: Vec<&str>, like: &str| {
            let added_ids: Vec<&str> = added.iter().map(|r| r[0].as_str()).collect();
            crate::editors::next_id(ids.into_iter().chain(added_ids), like)
        };
        match self {
            TableKind::Structures => {
                cells[0] = taken_after(
                    line.structures.iter().map(|s| s.id.as_str()).collect(),
                    line.structures.last().map_or("T0", |s| s.id.as_str()),
                );
                cells[3] = "0".into();
            }
            TableKind::Sections => {
                cells[0] = taken_after(
                    line.sections.iter().map(|s| s.id.as_str()).collect(),
                    line.sections.last().map_or("S0", |s| s.id.as_str()),
                );
                let like = SectionForm::new_through(model, case, Vec::new(), None);
                cells[1] = like.cable;
                cells[2] = like.circuit;
                cells[3] = like.phase_label;
                cells[4] = like.system.unwrap_or_default();
                cells[5] = like.bundle.to_string();
                cells[6] = f(like.spacing, 3);
                cells[7] = "in the blocks".into();
                cells[TableKind::Sections.column("calculation")] =
                    ols_line::condition::Calculation::default().label().into();
                cells[8] = "tension".into();
                cells[9] = "20".into();
                cells[10] = "15".into();
            }
            TableKind::Strings => {
                cells[9] = "suspension".into();
            }
            TableKind::Jumpers => {
                cells[0] = taken_after(
                    line.jumpers.iter().map(|j| j.id.as_str()).collect(),
                    line.jumpers.last().map_or("J0", |j| j.id.as_str()),
                );
                cells[6] = "15".into();
                cells[13] = "no".into();
            }
            // Circuits follow the connections: no rows of their own.
            TableKind::Circuits => {}
            TableKind::Routes => {
                let mut taken = line.clone();
                for r in added {
                    taken.routes.push(ols_line::Route::new(&r[0], ""));
                }
                cells[0] = crate::editors::free_route_id(&taken);
            }
            TableKind::Systems => {
                cells[0] = taken_after(
                    line.systems.iter().map(|s| s.id.as_str()).collect(),
                    "system-0",
                );
                cells[2] = "AC".into();
                cells[4] = "50".into();
                cells[5] = "L1 L2 L3".into();
            }
            TableKind::Alignments => {
                let mut taken = line.clone();
                for r in added {
                    taken
                        .alignments
                        .push(ols_line::Alignment::new(&r[0], "", Vec::new()));
                }
                cells[0] = crate::editors::free_alignment_id(&taken);
                // The route worked on, else the only one.
                cells[2] = model
                    .work_route
                    .clone()
                    .filter(|w| line.routes.iter().any(|r| &r.id == w))
                    .or_else(|| match &line.routes[..] {
                        [only] => Some(only.id.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                cells[3] = (line.alignments.len() + added.len() + 1).to_string();
            }
            TableKind::AlignmentPoints => {
                // At the end of the first alignment, a new point id.
                let mut taken = line.clone();
                for r in added {
                    taken
                        .alignment_points
                        .push(ols_line::AlignmentPoint::free(&r[2], [0.0; 3]));
                }
                if let Some(a) = line.alignments.first() {
                    let more = added.iter().filter(|r| r[0] == a.id).count();
                    cells[0] = a.id.clone();
                    cells[1] = (a.points.len() + more + 1).to_string();
                }
                cells[2] = taken.new_point_id();
            }
            TableKind::WeatherCases => {
                cells[W_ORDER] = (model.cases.len() + added.len() + 1).to_string();
                cells[W_NAME] = format!("new case {}", added.len() + 1);
                cells[W_TEMP] = "15".into();
                cells[W_ICE] = "0".into();
                cells[W_PRESSURE] = "0".into();
            }
        }
        cells
    }

    /// What the page's changes do.
    pub fn changes(
        self,
        model: &Model,
        case: usize,
        rows: &[Row],
        page: &TablePage,
    ) -> Result<Changes, String> {
        if self == TableKind::WeatherCases {
            return weather_file(model, rows, page).map(|file| Changes {
                file: Some(file),
                ..Changes::default()
            });
        }
        let stored = model
            .stored_line()
            .ok_or_else(|| "read-only: open a project to edit".to_owned())?;
        let session = &model
            .editing
            .as_ref()
            .map_err(|e| format!("read-only: {e}"))?
            .session;
        let row_of = |key: &str| {
            rows.iter()
                .find(|r| r.key == key)
                .ok_or_else(|| format!("{key} is no longer in the line"))
        };
        let existing = |key: &String| -> Result<Cells<'_>, String> {
            let row = row_of(key)?;
            Ok(Cells {
                row: &row.cells,
                typed: page.edits.get(key),
                defaulted: &row.defaulted,
            })
        };
        let mut out = Changes::default();
        match self {
            TableKind::Structures => {
                for key in &page.deleted {
                    let i = session.structure_deletion(key).map_err(|e| e.to_string())?;
                    if i.has_dependents() {
                        out.takes.push(i.summary());
                    }
                    out.commands.push((
                        format!("deleted structure {key}"),
                        Command::Delete {
                            kind: Kind::Structure,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let mut form = StructureForm::of(model, key)
                        .ok_or_else(|| format!("no structure {key}"))?;
                    // The table changes the structure only (its alignment
                    // point stays tied as it is).
                    form.on_alignment = None;
                    structure_cells(&mut form, &c, model, key)?;
                    let (what, cmd) = form.command(Some(stored))?;
                    let new_id = c.get(0).trim().to_owned();
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed structure {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Structure,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        out.commands.push((what, retarget(cmd, &new_id)));
                    } else {
                        out.commands.push((what, cmd));
                    }
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut form = StructureForm::new_at(model, [0.0; 3]);
                    form.id = id.clone();
                    form.structure_type = None;
                    form.on_ground = false;
                    structure_cells(&mut form, &c, model, &id)?;
                    out.commands.push(form.command(Some(stored))?);
                }
            }
            TableKind::Sections => {
                for key in &page.deleted {
                    let i = session
                        .deletion_impact(Kind::Section, key)
                        .map_err(|e| e.to_string())?;
                    if i.has_dependents() {
                        out.takes.push(i.summary());
                    }
                    out.commands.push((
                        format!("deleted section {key}"),
                        Command::Delete {
                            kind: Kind::Section,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let display = Some(TableKind::Sections.column("display cases"));
                    let only_display = page
                        .edits
                        .get(key)
                        .is_some_and(|e| e.keys().all(|k| Some(*k) == display));
                    let new_id = c.get(0).trim().to_owned();
                    out.display
                        .push((new_id.clone(), display_case(model, &c.get(12))?));
                    if only_display {
                        continue;
                    }
                    let mut form =
                        SectionForm::of(model, key).ok_or_else(|| format!("no section {key}"))?;
                    section_cells(&mut form, &c, model, case, key)?;
                    let (what, cmd) = form.command(Some(stored))?;
                    // Its circuit's other sections take its name, phase and
                    // system (but those changed here by hand): listed to
                    // confirm.
                    let by_hand: Vec<String> = page.edits.keys().cloned().collect();
                    let mates: Vec<_> = form
                        .circuit_mates(stored, key)
                        .into_iter()
                        .filter(|(id, _)| !by_hand.contains(id))
                        .collect();
                    if !mates.is_empty() {
                        out.takes.push(format!(
                            "section {key}: its circuit's {} change too",
                            mates
                                .iter()
                                .map(|(id, what)| format!("{id} ({})", what.join(", ")))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    let mut mate_puts = form.circuit_puts(stored, key, &by_hand)?;
                    // A colour typed: every section of its circuit (by its
                    // name) takes it, listed to confirm.
                    if c.changed(TableKind::Sections.column("colour"))
                        && !form.circuit.trim().is_empty()
                    {
                        let mut taking = Vec::new();
                        for s in stored.sections.iter().filter(|s| {
                            s.id != *key
                                && s.circuit == form.circuit
                                && !by_hand.contains(&s.id)
                                && s.color != form.color
                        }) {
                            let put = mate_puts.iter_mut().find(|(_, cmd)| {
                                matches!(cmd, Command::Put { kind: Kind::Section, id, .. } if *id == s.id)
                            });
                            match put {
                                Some((_, Command::Put { value, .. })) => {
                                    value["color"] = serde_json::to_value(&form.color)
                                        .map_err(|e| e.to_string())?;
                                }
                                _ => {
                                    let mut m = s.clone();
                                    m.color = form.color.clone();
                                    mate_puts.push((
                                        format!("section {}: the colour of its circuit", s.id),
                                        Command::Put {
                                            kind: Kind::Section,
                                            id: s.id.clone(),
                                            value: serde_json::to_value(&m)
                                                .map_err(|e| e.to_string())?,
                                            ord: None,
                                        },
                                    ));
                                }
                            }
                            taking.push(s.id.clone());
                        }
                        if !taking.is_empty() {
                            out.takes.push(format!(
                                "section {key}: its circuit {}'s sections take its colour: {}",
                                form.circuit,
                                taking.join(", ")
                            ));
                        }
                    }
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed section {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Section,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        out.commands.push((what, retarget(cmd, &new_id)));
                    } else {
                        out.commands.push((what, cmd));
                    }
                    out.commands.extend(mate_puts);
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut form = SectionForm::new_through(model, case, Vec::new(), None);
                    form.id = id.clone();
                    section_cells(&mut form, &c, model, case, &id)?;
                    out.display.push((id, display_case(model, &c.get(12))?));
                    out.commands.push(form.command(Some(stored))?);
                }
            }
            TableKind::Strings => {
                // One put per structure with all its strings changed.
                let mut structures: BTreeMap<String, Structure> = BTreeMap::new();
                let take = |structures: &mut BTreeMap<String, Structure>,
                            id: &str|
                 -> Result<(), String> {
                    if !structures.contains_key(id) {
                        let s = stored
                            .structure(id)
                            .ok_or_else(|| format!("no structure {id}"))?
                            .clone();
                        structures.insert(id.to_owned(), s);
                    }
                    Ok(())
                };
                // A string of a typed structure's type becomes its own with
                // the strings and wire points that move with it.
                let own = |s: &mut Structure, label: &str| -> Result<(), String> {
                    if s.string(label).is_some() {
                        return Ok(());
                    }
                    let form = StringForm::of_string(model, &s.id, label)
                        .ok_or_else(|| format!("no string {} {label}", s.id))?;
                    form.apply_to(s, model.line.structure(&s.id));
                    Ok(())
                };
                for key in &page.deleted {
                    let (id, label) = key.split_once('\t').unwrap_or((key, ""));
                    take(&mut structures, id)?;
                    let s = structures.get_mut(id).expect("taken");
                    own(s, label)?;
                    s.strings.retain(|x| x.label != label);
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let (id, label) = key.split_once('\t').unwrap_or((key, ""));
                    take(&mut structures, id)?;
                    let c = existing(key)?;
                    let s = structures.get_mut(id).expect("taken");
                    own(s, label)?;
                    let x = s
                        .strings
                        .iter_mut()
                        .find(|x| x.label == label)
                        .ok_or_else(|| format!("no string {id} {label}"))?;
                    string_cells(x, &c, &format!("{id} {label}"))?;
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    if id.is_empty() {
                        return Err("a new string needs its structure".into());
                    }
                    take(&mut structures, &id)?;
                    let s = structures.get_mut(&id).expect("taken");
                    let mut x = ols_line::InsulatorString::default();
                    string_cells(&mut x, &c, &format!("{id} new string"))?;
                    if x.label.is_empty() {
                        x.label = format!("S{}", s.strings.len() + 1);
                    }
                    s.strings.push(x);
                }
                for (id, s) in structures {
                    out.commands.push((
                        format!("changed strings of {id}"),
                        Command::Put {
                            kind: Kind::Structure,
                            id: id.clone(),
                            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
            }
            TableKind::Jumpers => {
                for key in &page.deleted {
                    out.commands.push((
                        format!("deleted jumper {key}"),
                        Command::Delete {
                            kind: Kind::Jumper,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let mut j = stored
                        .jumpers
                        .iter()
                        .find(|j| &j.id == key)
                        .ok_or_else(|| format!("no jumper {key}"))?
                        .clone();
                    jumper_cells(&mut j, &c, model, key)?;
                    let new_id = c.get(0).trim().to_owned();
                    let put = Command::Put {
                        kind: Kind::Jumper,
                        id: key.clone(),
                        value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                        ord: None,
                    };
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed jumper {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Jumper,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        out.commands
                            .push((format!("changed jumper {new_id}"), retarget(put, &new_id)));
                    } else {
                        out.commands.push((format!("changed jumper {key}"), put));
                    }
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut j: ols_line::Jumper = serde_json::from_value(serde_json::json!({
                        "id": id,
                        "cable": "",
                        "from": {"structure": "", "set": 0, "phase": 0},
                        "to": {"structure": "", "set": 0, "phase": 0},
                    }))
                    .map_err(|e| e.to_string())?;
                    jumper_cells(&mut j, &c, model, &id)?;
                    out.commands.push((
                        format!("added jumper {id}"),
                        Command::Put {
                            kind: Kind::Jumper,
                            id,
                            value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
            }
            TableKind::Systems => {
                let mut systems = stored.systems.clone();
                for key in &page.deleted {
                    let n = stored
                        .sections
                        .iter()
                        .filter(|s| s.system.as_deref() == Some(key.as_str()))
                        .count();
                    if n > 0 {
                        out.takes
                            .push(format!("system {key}: {n} sections lose their system"));
                    }
                    systems.retain(|s| &s.id != key);
                    for sec in stored
                        .sections
                        .iter()
                        .filter(|s| s.system.as_deref() == Some(key.as_str()))
                    {
                        let mut sec = sec.clone();
                        sec.system = None;
                        out.commands.push((
                            format!("section {} without a system", sec.id),
                            Command::Put {
                                kind: Kind::Section,
                                id: sec.id.clone(),
                                value: serde_json::to_value(&sec).map_err(|e| e.to_string())?,
                                ord: None,
                            },
                        ));
                    }
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let new_id = c.get(0).trim().to_owned();
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed system {key} to {new_id}"),
                            Command::RenameSystem {
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                    }
                    let s = systems
                        .iter_mut()
                        .find(|s| &s.id == key)
                        .ok_or_else(|| format!("no system {key}"))?;
                    s.id = new_id;
                    system_cells(s, &c, key)?;
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let id = c.get(0).trim().to_owned();
                    let mut s: ols_line::electrical::System =
                        serde_json::from_value(serde_json::json!({
                            "id": id, "voltage_kv": 0.0, "phases": []
                        }))
                        .map_err(|e| e.to_string())?;
                    system_cells(&mut s, &c, &id)?;
                    systems.push(s);
                }
                // The systems are part of the line settings.
                out.commands.push((
                    "changed electrical systems".into(),
                    ols_session::line_settings_put(stored, Some(systems)),
                ));
            }
            TableKind::Alignments => alignment_list_commands(stored, rows, page, &mut out)?,
            TableKind::AlignmentPoints => {
                point_commands(model, stored, rows, page, &mut out)?;
            }
            TableKind::Circuits => circuit_commands(stored, rows, page, &mut out)?,
            TableKind::Routes => {
                for key in &page.deleted {
                    let i = session
                        .deletion_impact(Kind::Route, key)
                        .map_err(|e| e.to_string())?;
                    if i.has_dependents() {
                        out.takes.push(i.summary());
                    }
                    out.commands.push((
                        format!("deleted route {key}"),
                        Command::Delete {
                            kind: Kind::Route,
                            id: key.clone(),
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let c = existing(key)?;
                    let mut r = stored
                        .routes
                        .iter()
                        .find(|r| &r.id == key)
                        .ok_or_else(|| format!("no route {key}"))?
                        .clone();
                    let new_id = c.get(0).trim().to_owned();
                    if new_id.is_empty() {
                        return Err(format!("route {key}: give it an id"));
                    }
                    r.name = c.get(1).trim().to_owned();
                    r.description = c.get(5).trim().to_owned();
                    if new_id != *key {
                        out.commands.push((
                            format!("renamed route {key} to {new_id}"),
                            Command::Rename {
                                kind: Kind::Route,
                                id: key.clone(),
                                new_id: new_id.clone(),
                            },
                        ));
                        r.id = new_id.clone();
                    }
                    out.commands.push((
                        format!("changed route {}", r.id),
                        Command::Put {
                            kind: Kind::Route,
                            id: r.id.clone(),
                            value: serde_json::to_value(&r).map_err(|e| e.to_string())?,
                            ord: None,
                        },
                    ));
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let (what, cmd) = crate::editors::new_route(stored, &c.get(0), &c.get(1))?;
                    let mut cmd = cmd;
                    if let Command::Put { value, .. } = &mut cmd {
                        value["description"] = c.get(5).trim().into();
                    }
                    out.commands.push((what, cmd));
                }
            }
            TableKind::WeatherCases => unreachable!("written to its file"),
        }
        Ok(out)
    }
}

/// The values a cell is chosen from (a dropdown), and whether several can
/// be ticked (joined with `; `); `None`: typed.
fn choices(
    kind: TableKind,
    column: usize,
    model: &Model,
    cells: &[String],
) -> Option<(Vec<String>, bool)> {
    let name = kind.columns()[column].0;
    let with_none = |v: Vec<String>| {
        let mut out = vec![String::new()];
        out.extend(v);
        out
    };
    let cables = || {
        model
            .library
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    };
    Some(match (kind, name) {
        (TableKind::Structures, "type") => (
            with_none(
                model
                    .types
                    .as_ref()
                    .map(|(s, _)| s.iter().map(|t| t.id.clone()).collect())
                    .unwrap_or_default(),
            ),
            false,
        ),
        (TableKind::Sections | TableKind::Jumpers, "cable") => (cables(), false),
        (TableKind::Sections, "system") => (
            with_none(model.line.systems.iter().map(|s| s.id.clone()).collect()),
            false,
        ),
        (TableKind::Sections, "calculation") => (
            ols_line::condition::Calculation::ALL
                .iter()
                .map(|c| c.label().to_owned())
                .collect(),
            false,
        ),
        (TableKind::Sections, "status") => {
            (vec!["in the blocks".into(), "clipped in".into()], false)
        }
        (TableKind::Sections, "condition") => {
            let spans = cells
                .get(TableKind::Sections.column("points"))
                .map_or(0, |p| p.split(',').filter(|x| !x.trim().is_empty()).count())
                .saturating_sub(1);
            let mut v = vec!["tension".to_owned(), "sag, ruling span".to_owned()];
            v.extend((1..=spans).map(|k| format!("sag, span {k}")));
            (v, false)
        }
        (TableKind::Sections, "display cases") => (
            model.cases.iter().map(|c| c.case.name.clone()).collect(),
            true,
        ),
        (TableKind::Strings, "insulator") => (
            with_none(
                model
                    .types
                    .as_ref()
                    .map(|(_, i)| i.iter().map(|a| a.id.clone()).collect())
                    .unwrap_or_default(),
            ),
            false,
        ),
        (TableKind::Strings, "clamp") => (vec!["suspension".into(), "strain".into()], false),
        (TableKind::Jumpers, "rigid") => (vec!["no".into(), "yes".into()], false),
        (TableKind::Circuits, "starts") => (circuit_ends(cells), true),
        (TableKind::Circuits, "system") => (
            with_none(model.line.systems.iter().map(|x| x.id.clone()).collect()),
            false,
        ),
        (TableKind::Systems, "kind") => (vec!["AC".into(), "DC".into()], false),
        (TableKind::WeatherCases, "Wind from\n ") => (vec!["right".into(), "left".into()], false),
        (TableKind::AlignmentPoints, "structure") => (
            with_none(model.line.structures.iter().map(|s| s.id.clone()).collect()),
            false,
        ),
        (TableKind::AlignmentPoints, "alignment") => (
            with_none(model.line.alignments.iter().map(|a| a.id.clone()).collect()),
            false,
        ),
        (TableKind::Alignments, "route") => (
            model.line.routes.iter().map(|r| r.id.clone()).collect(),
            false,
        ),
        (TableKind::Structures, "routes") => (
            model.line.routes.iter().map(|r| r.id.clone()).collect(),
            true,
        ),
        _ => return None,
    })
}

/// A cell chosen from a list; true when changed.
fn choice_cell(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    text: &mut String,
    (values, multi): (Vec<String>, bool),
    color: Option<egui::Color32>,
    none: Option<&str>,
) -> bool {
    let mut changed = false;
    let mut shown = egui::RichText::new(if text.is_empty() {
        "—"
    } else {
        text.as_str()
    });
    if let Some(c) = color {
        shown = shown.color(c);
    }
    // Exactly the column's width: the combo box's own width leaves out
    // its arrow and padding, and wider cells shifted the columns after it.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cell.set_clip_rect(rect.intersect(ui.clip_rect()));
    let ui = &mut cell;
    let arrow = ui.spacing().icon_width + 2.0 * ui.spacing().button_padding.x + 4.0;
    let mut combo = egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width((width - arrow).max(10.0))
        .height(400.0);
    if multi {
        combo = combo.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    }
    combo.show_ui(ui, |ui| {
        if multi {
            let mut ticked: Vec<String> = text
                .split(';')
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .collect();
            for v in &values {
                let mut on = ticked.contains(v);
                if ui.checkbox(&mut on, v).changed() {
                    if on {
                        ticked.push(v.clone());
                    } else {
                        ticked.retain(|t| t != v);
                    }
                    // In the order of the list.
                    ticked.sort_by_key(|t| values.iter().position(|x| x == t));
                    *text = ticked.join("; ");
                    changed = true;
                }
            }
            // Display cases: none is "the cases shown" (circuits' starts
            // need one: no such button).
            if let Some(label) = none
                && ui.small_button(label).clicked()
            {
                text.clear();
                changed = true;
            }
        } else {
            for v in &values {
                let label = if v.is_empty() { "—" } else { v.as_str() };
                if ui.selectable_label(text == v, label).clicked() && text != v {
                    *text = v.clone();
                    changed = true;
                }
            }
        }
    });
    changed
}

/// The alignment points' rows: on each alignment in order, its points in
/// order (key `alignment\tpoint`); then the points on no alignment (key
/// `\tpoint`).
fn point_rows(line: &ols_line::Line) -> Vec<Row> {
    let row = |a: &str, k: Option<usize>, p: &ols_line::AlignmentPoint| {
        let also: Vec<String> = crate::editors::point_orders(line, &p.id)
            .into_iter()
            .filter(|(x, _)| x != a)
            .map(|(x, n)| format!("{x} #{n}"))
            .collect();
        Row {
            defaulted: Vec::new(),
            key: format!("{a}\t{}", p.id),
            select: Selection::AlignmentPoint { id: p.id.clone() },
            cells: vec![
                a.to_owned(),
                k.map(|k| (k + 1).to_string()).unwrap_or_default(),
                p.id.clone(),
                f(p.position[0], 3),
                f(p.position[1], 3),
                f(p.position[2], 3),
                p.structure.clone().unwrap_or_default(),
                also.join(", "),
                p.description.clone(),
            ],
        }
    };
    let mut out = Vec::new();
    for a in &line.alignments {
        for (k, p) in line.points_of(a).iter().enumerate() {
            out.push(row(&a.id, Some(k), p));
        }
    }
    for p in &line.alignment_points {
        if line.point_alignments(&p.id).is_empty() {
            out.push(row("", None, p));
        }
    }
    out
}

/// Routes as a cell gives them: "a, b".
fn route_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split(',')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
        .collect();
    out.dedup();
    out
}

/// The circuits (conductor paths), one row each: C1, C2… in the order of
/// their first sections; keyed by their first section (stable when turned).
fn circuit_rows(line: &ols_line::Line) -> Vec<Row> {
    ols_line::electrical::circuits(line)
        .into_iter()
        .enumerate()
        .map(|(k, c)| {
            let key = circuit_key(&c);
            let sections: Vec<&ols_line::Section> = c
                .sections
                .iter()
                .filter_map(|id| line.sections.iter().find(|s| &s.id == id))
                .collect();
            // One value for the circuit, or what its sections say.
            let common = |get: &dyn Fn(&ols_line::Section) -> String| -> String {
                let mut v: Vec<String> = sections.iter().map(|s| get(s)).collect();
                v.sort();
                v.dedup();
                match &v[..] {
                    [] => String::new(),
                    [one] => one.clone(),
                    many => format!("mixed: {}", many.join(" / ")),
                }
            };
            let list = |v: &[SectionPoint]| v.iter().map(point).collect::<Vec<_>>().join("; ");
            Row {
                defaulted: Vec::new(),
                key,
                select: c
                    .sections
                    .first()
                    .map_or(Selection::None, |s| Selection::Span {
                        section: s.clone(),
                        span: 0,
                    }),
                cells: vec![
                    format!("C{}", k + 1),
                    common(&|s| s.circuit.clone()),
                    common(&|s| s.phase_label.clone()),
                    common(&|s| s.system.clone().unwrap_or_default()),
                    list(&c.starts),
                    list(&c.ends),
                    c.sections.join(" "),
                    c.jumpers.join(" "),
                    common(&|s| s.color.clone().unwrap_or_default()),
                ],
            }
        })
        .collect()
}

/// The ends of a circuit's row: its starts and its ends cells.
fn circuit_ends(cells: &[String]) -> Vec<String> {
    cells[4]
        .split(';')
        .chain(cells[5].split(';'))
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect()
}

/// A circuit's key: its first section in the line's order.
fn circuit_key(c: &ols_line::electrical::Circuit) -> String {
    c.sections.iter().min().cloned().unwrap_or_default()
}

/// Circuits turned ("reversed" changed): their starts in the line's
/// settings, and their jumpers turned to point along them.
fn circuit_commands(
    stored: &ols_line::Line,
    rows: &[Row],
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    let mut line = stored.clone();
    let along = {
        let mut l = stored.clone();
        l.circuit_starts.clear();
        ols_line::electrical::circuits(&l)
    };
    let mut turned = Vec::new();
    for row in rows {
        let Some(edit) = page.edits.get(&row.key) else {
            continue;
        };
        let Some(v) = edit.get(&4) else { continue };
        let Some(c) = along.iter().find(|c| circuit_key(c) == row.key) else {
            continue;
        };
        let all: Vec<SectionPoint> = c.starts.iter().chain(&c.ends).cloned().collect();
        let mut starts = Vec::new();
        for t in v.split(';').map(str::trim).filter(|t| !t.is_empty()) {
            let p = parse_point(t).map_err(|e| format!("{} starts: {e}", row.cells[0]))?;
            if !all.contains(&p) {
                return Err(format!(
                    "{} starts: {t} is not one of its ends",
                    row.cells[0]
                ));
            }
            starts.push(p);
        }
        if starts.is_empty() {
            return Err(format!("{}: give it at least one start", row.cells[0]));
        }
        if all.len() > 1 && starts.len() == all.len() {
            return Err(format!("{}: leave it at least one end", row.cells[0]));
        }
        // Its ends' roles: none listed when it runs along the line.
        line.circuit_starts.retain(|p| !all.contains(p));
        if starts != c.starts {
            line.circuit_starts.extend(starts);
        }
        turned.push(row.key.clone());
    }
    // Name, phase and system: on all its sections.
    for row in rows {
        let Some(edit) = page.edits.get(&row.key) else {
            continue;
        };
        let Some(c) = along.iter().find(|c| circuit_key(c) == row.key) else {
            continue;
        };
        let set = |k: usize| edit.get(&k).map(|v| v.trim().to_owned());
        let (name, phase, system) = (set(1), set(2), set(3));
        let color = match set(8) {
            Some(v) if v.starts_with("mixed:") => {
                return Err(format!("{}: give one colour for the circuit", row.cells[0]));
            }
            Some(v) => Some(parse_color(&v, &row.cells[0])?),
            None => None,
        };
        if name.is_none() && phase.is_none() && system.is_none() && color.is_none() {
            continue;
        }
        for v in [&name, &phase].into_iter().flatten() {
            if v.starts_with("mixed:") {
                return Err(format!("{}: give one value for the circuit", row.cells[0]));
            }
        }
        let system = match system {
            None => None,
            Some(v) if v.is_empty() => Some(None),
            Some(v) if stored.systems.iter().any(|x| x.id == v) => Some(Some(v)),
            Some(v) => return Err(format!("{}: no electrical system \"{v}\"", row.cells[0])),
        };
        let mut changed = Vec::new();
        for id in &c.sections {
            let Some(mut sec) = stored.sections.iter().find(|s| &s.id == id).cloned() else {
                continue;
            };
            let before = sec.clone();
            if let Some(n) = &name {
                sec.circuit = n.clone();
            }
            if let Some(p) = &phase {
                sec.phase_label = p.clone();
            }
            if let Some(sy) = &system {
                sec.system = sy.clone();
            }
            if let Some(col) = &color {
                sec.color = col.clone();
            }
            if sec != before {
                changed.push(id.clone());
                out.commands.push((
                    format!("section {id}: as its circuit {}", row.cells[0]),
                    Command::Put {
                        kind: Kind::Section,
                        id: id.clone(),
                        value: serde_json::to_value(&sec).map_err(|e| e.to_string())?,
                        ord: None,
                    },
                ));
            }
        }
        // Its jumpers with a colour of their own take the circuit's (the
        // others follow their sections).
        if let Some(col) = &color {
            for id in &c.jumpers {
                let Some(mut j) = stored.jumpers.iter().find(|j| &j.id == id).cloned() else {
                    continue;
                };
                if j.color.is_none() || j.color == *col {
                    continue;
                }
                j.color = col.clone();
                changed.push(format!("jumper {id}"));
                out.commands.push((
                    format!("jumper {id}: the colour of its circuit {}", row.cells[0]),
                    Command::Put {
                        kind: Kind::Jumper,
                        id: id.clone(),
                        value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                        ord: None,
                    },
                ));
            }
        }
        if !changed.is_empty() {
            out.takes.push(format!(
                "circuit {}: sections {}",
                row.cells[0],
                changed.join(", ")
            ));
        }
    }
    if turned.is_empty() {
        return Ok(());
    }
    out.commands.push((
        "changed the starts and ends of electrical circuits".into(),
        ols_session::line_settings_put(&line, None),
    ));
    // Their jumpers along them: `from` the end nearer a start.
    for c in ols_line::electrical::circuits(&line)
        .iter()
        .filter(|c| turned.contains(&circuit_key(c)))
    {
        let at = |p: &SectionPoint| c.order.iter().position(|q| q == p);
        for id in &c.jumpers {
            let Some(j) = line.jumpers.iter().find(|j| &j.id == id) else {
                continue;
            };
            if at(&j.to) < at(&j.from) {
                let mut j = j.clone();
                // Its clamps from the other end: on the same structures,
                // in the other order.
                let first = j.from.structure.clone();
                for c in &mut j.supports {
                    let id = c.structure.take().unwrap_or_else(|| first.clone());
                    c.structure = (id != j.to.structure).then_some(id);
                    c.at = 1.0 - c.at;
                }
                j.supports.reverse();
                std::mem::swap(&mut j.from, &mut j.to);
                std::mem::swap(&mut j.angle_from_deg, &mut j.angle_to_deg);
                if let Some(s) = &mut j.support {
                    s.fraction = 1.0 - s.fraction;
                }
                out.commands.push((
                    format!("jumper {} along its circuit", j.id),
                    Command::Put {
                        kind: Kind::Jumper,
                        id: j.id.clone(),
                        value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                        ord: None,
                    },
                ));
            }
        }
    }
    Ok(())
}

/// "max temperature; glaze ice" → the cases' indices.
fn display_case(model: &Model, names: &str) -> Result<Vec<usize>, String> {
    names
        .split(';')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| {
            model
                .cases
                .iter()
                .position(|c| c.case.name == n)
                .ok_or_else(|| format!("no weather case \"{n}\" (separate cases with ;)"))
        })
        .collect()
}

fn structure_cells(
    form: &mut StructureForm,
    c: &Cells,
    model: &Model,
    name: &str,
) -> Result<(), String> {
    form.structure_type = opt(&c.get(1));
    form.extension = opt(&c.get(2));
    form.orientation_deg = opt_num(&c.get(3), &format!("{name} orientation"))?.unwrap_or(0.0);
    for k in 0..2 {
        form.position[k] = num(&c.get(4 + k), &format!("{name} position"))?;
    }
    form.position[2] = match opt_num(&c.get(6), &format!("{name} z"))? {
        Some(z) => z,
        None => model
            .ground_at(form.position[0], form.position[1])
            .ok_or_else(|| format!("{name}: give z (no ground there)"))?,
    };
    form.description = c
        .get(TableKind::Structures.column("description"))
        .trim()
        .to_owned();
    // Routes typed: its own, unless they are those of the alignments it
    // stands on (then left to them).
    let col = TableKind::Structures.column("routes");
    if c.changed(col) || c.typed.is_none() {
        let list = route_list(&c.get(col));
        if let Some(r) = list
            .iter()
            .find(|r| !model.line.routes.iter().any(|x| &x.id == *r))
        {
            return Err(format!(
                "{name}: no route {r} (make it in the Routes table)"
            ));
        }
        let mut found = model.line.routes_of(name);
        let mut sorted = list.clone();
        sorted.sort();
        found.sort();
        let own = model
            .line
            .structure(name)
            .is_some_and(|s| !s.routes.is_empty());
        form.routes = if sorted == found && !own {
            Vec::new()
        } else {
            list
        };
    }
    Ok(())
}

fn section_cells(
    form: &mut SectionForm,
    c: &Cells,
    model: &Model,
    case: usize,
    name: &str,
) -> Result<(), String> {
    let is_new = c.typed.is_none();
    form.cable = c.get(1).trim().to_owned();
    let calc = c.get(TableKind::Sections.column("calculation"));
    form.calculation = if calc.trim().is_empty() {
        Default::default()
    } else {
        ols_line::condition::Calculation::parse(&calc).ok_or_else(|| {
            format!("{name} calculation: \"{calc}\" — e.g. Initial FE, Creep RS, Max Sag FE")
        })?
    };
    if model.library.get(&form.cable).is_none() {
        return Err(format!(
            "{name}: cable \"{}\" is not in the wire library",
            form.cable
        ));
    }
    form.circuit = c.get(2);
    form.phase_label = c.get(3);
    form.description = c
        .get(TableKind::Sections.column("description"))
        .trim()
        .to_owned();
    let color = c
        .get(TableKind::Sections.column("colour"))
        .trim()
        .to_lowercase();
    form.color = if color.is_empty() {
        None
    } else {
        let hex = color.trim_start_matches('#');
        if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(format!(
                "{name} colour: \"{color}\" — write #rrggbb, e.g. #e4572e"
            ));
        }
        Some(format!("#{hex}"))
    };
    form.system = opt(&c.get(4));
    form.bundle = c
        .get(5)
        .trim()
        .parse()
        .map_err(|_| format!("{name} bundle: \"{}\" is not a number", c.get(5)))?;
    if let Some(s) = opt_num(&c.get(6), &format!("{name} spacing"))? {
        form.spacing = s;
    }
    let clipped = match c.get(7).trim().to_lowercase().as_str() {
        "in the blocks" | "blocks" | "unclipped" | "rolling" => false,
        "clipped in" | "clipped" => true,
        other => {
            return Err(format!(
                "{name} status: \"{other}\" — write in the blocks or clipped in"
            ));
        }
    };
    if form.stringing.clipped() && (!clipped || c.changed(8)) {
        form.stringing = form.unclipped(model, case);
    }
    // The points first: a sag in a span needs the spans.
    if is_new || c.changed(11) {
        let points: Vec<SectionPoint> = c
            .get(11)
            .split([',', '→'])
            .filter(|t| !t.trim().is_empty())
            .map(parse_point)
            .collect::<Result<_, _>>()?;
        if points.len() < 2 {
            return Err(format!("{name}: two points or more"));
        }
        form.set_points(model, case, points);
    }
    let temperature = num(&c.get(10), &format!("{name} temperature"))?;
    if !form.stringing.clipped() {
        let condition = c.get(8).trim().to_lowercase();
        if c.changed(8) && !c.changed(9) && !is_new {
            return Err(format!(
                "{name}: give the value (kN or m) for the new condition"
            ));
        }
        let value = num(&c.get(9), &format!("{name} value"))?;
        form.stringing = if condition == "tension" {
            Stringing::Tension {
                horizontal_tension: value * 1000.0,
                temperature,
            }
        } else if let Some(rest) = condition.strip_prefix("sag") {
            let rest = rest.trim_start_matches([',', ' ']);
            let span = if rest.is_empty() || rest.starts_with("ruling") {
                None
            } else {
                let k: usize = rest
                    .trim_start_matches("span")
                    .trim()
                    .parse()
                    .map_err(|_| format!("{name} condition: \"{condition}\""))?;
                Some(k.max(1) - 1)
            };
            Stringing::Sag {
                sag: value,
                temperature,
                span,
            }
        } else {
            return Err(format!(
                "{name} condition: \"{condition}\" — write tension, sag, ruling span or sag, span N"
            ));
        };
    } else if let Stringing::UnstressedLengths { temperature: t, .. } = &mut form.stringing {
        *t = temperature;
    }
    if clipped && !form.stringing.clipped() {
        form.clip_in(model)?;
    }
    for (col, measure) in [(13, Measure::Tension), (14, Measure::Sag)] {
        if c.changed(col) {
            let target = num(&c.get(col), &format!("{name} {}", SECTIONS[col].0))?;
            let shown = model.section_case(&form.id, case);
            let weather = model
                .cases
                .get(shown)
                .map(|x| x.case.clone())
                .ok_or("no weather case")?;
            reach_target(form, model, &weather, measure, target, name)?;
        }
    }
    Ok(())
}

/// What a target is.
#[derive(Clone, Copy)]
enum Measure {
    /// Horizontal tension of the first span, kN.
    Tension,
    /// Largest sag, m.
    Sag,
}

/// Sets the section's condition (tension or sag) so that `measure` is
/// `target` in `weather`: bracketed bisection on the condition's value
/// (each step one solve of the section); exit clause 60 steps.
fn reach_target(
    form: &mut SectionForm,
    model: &Model,
    weather: &ols_weather::WeatherCase,
    measure: Measure,
    target: f64,
    name: &str,
) -> Result<(), String> {
    if form.stringing.clipped() {
        return Err(format!(
            "{name} is clipped in: a target tension or sag needs it in the blocks"
        ));
    }
    if target.is_nan() || target <= 0.0 {
        return Err(format!("{name}: the target must be above 0"));
    }
    let by_tension = matches!(form.stringing, Stringing::Tension { .. });
    let start = match &form.stringing {
        Stringing::Tension {
            horizontal_tension, ..
        } => *horizontal_tension,
        Stringing::Sag { sag, .. } => *sag,
        Stringing::UnstressedLengths { .. } => unreachable!("checked"),
    };
    let set = |form: &mut SectionForm, v: f64| match &mut form.stringing {
        Stringing::Tension {
            horizontal_tension, ..
        } => *horizontal_tension = v,
        Stringing::Sag { sag, .. } => *sag = v,
        Stringing::UnstressedLengths { .. } => {}
    };
    // Result for a condition value; increasing in it when `rising`.
    // With its loads (marker balls…), as it will be solved.
    let old = model.line.sections.iter().find(|s| s.id == form.id);
    let mut eval = |v: f64| -> Result<f64, String> {
        set(form, v);
        let shape =
            ols_line::solve::solve_one(&model.line, &form.section(old), &model.library, weather)
                .map_err(|e| e.to_string())?;
        Ok(match measure {
            Measure::Tension => shape.spans.first().map_or(0.0, |s| s.horizontal_tension) / 1000.0,
            Measure::Sag => shape.spans.iter().map(|s| s.sag).fold(0.0, f64::max),
        })
    };
    // Tension condition: tension rises, sag falls with it; sag condition:
    // the other way round.
    let rising = matches!(measure, Measure::Tension) == by_tension;
    let (mut lo, mut hi) = (start / 1.5, start * 1.5);
    let below = |r: f64| if rising { r < target } else { r > target };
    let mut steps = 0;
    while !below(eval(lo)?) {
        steps += 1;
        lo /= 2.0;
        if steps > 30 {
            return Err(format!("{name}: no condition gives {target}"));
        }
    }
    while below(eval(hi)?) {
        steps += 1;
        hi *= 2.0;
        if steps > 30 {
            return Err(format!("{name}: no condition gives {target}"));
        }
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if below(eval(mid)?) {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-6 * hi {
            break;
        }
    }
    let v = 0.5 * (lo + hi);
    // Rounded: 1 N, 1 mm.
    set(
        form,
        if by_tension {
            v.round()
        } else {
            (v * 1000.0).round() / 1000.0
        },
    );
    Ok(())
}

fn string_cells(x: &mut ols_line::InsulatorString, c: &Cells, name: &str) -> Result<(), String> {
    x.description = c
        .get(TableKind::Strings.column("description"))
        .trim()
        .to_owned();
    x.label = c.get(1).trim().to_owned();
    x.top = c.get(2).trim().to_owned();
    x.wire = c.get(3).trim().to_owned();
    if x.top.is_empty() || x.wire.is_empty() {
        return Err(format!(
            "{name}: give its top (from) and its wire point (to)"
        ));
    }
    x.weight = opt_num(&c.get(6), &format!("{name} weight"))?.unwrap_or(0.0);
    x.insulator = opt(&c.get(7));
    x.compression = opt_num(&c.get(8), &format!("{name} compression"))?;
    x.strain = match c.get(9).trim().to_lowercase().as_str() {
        "strain" | "yes" | "true" | "1" => true,
        "suspension" | "no" | "false" | "0" | "" => false,
        other => {
            return Err(format!(
                "{name} clamp: \"{other}\" is not suspension or strain"
            ));
        }
    };
    x.wind_area = opt_num(&c.get(10), &format!("{name} wind area"))?.unwrap_or(0.0);
    if x.wind_area.is_nan() || x.wind_area < 0.0 {
        return Err(format!("{name} wind area: must be ≥ 0"));
    }
    Ok(())
}

/// A jumper's support clamps, in order: "T3 JW at 0.50 (0.10 m); …".
fn clamps_text(j: &ols_line::Jumper) -> String {
    j.supports
        .iter()
        .map(|c| {
            format!(
                "{} {} at {} ({} m)",
                c.structure_id(j),
                c.wire,
                f(c.at, 3),
                f(c.clamp_length, 3)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn jumper_cells(
    j: &mut ols_line::Jumper,
    c: &Cells,
    model: &Model,
    name: &str,
) -> Result<(), String> {
    j.description = c
        .get(TableKind::Jumpers.column("description"))
        .trim()
        .to_owned();
    j.from = parse_point(&c.get(1)).map_err(|e| format!("{name} from: {e}"))?;
    j.to = parse_point(&c.get(2)).map_err(|e| format!("{name} to: {e}"))?;
    j.cable = match opt(&c.get(3)) {
        Some(cable) => cable,
        None => model
            .line
            .sections
            .iter()
            .find(|s| s.points.contains(&j.from))
            .map(|s| s.cable.clone())
            .ok_or_else(|| format!("{name}: give its cable"))?,
    };
    j.length = opt_num(&c.get(4), &format!("{name} length"))?;
    j.sag = opt_num(&c.get(5), &format!("{name} sag"))?;
    j.temperature = opt_num(&c.get(6), &format!("{name} temperature"))?.unwrap_or(15.0);
    j.bundle = opt_num(&c.get(7), &format!("{name} bundle"))?.map(|v| v as u32);
    j.bundle_spacing = opt_num(&c.get(8), &format!("{name} spacing"))?;
    j.bending_stiffness = opt_num(&c.get(9), &format!("{name} EI"))?;
    j.stiffness_factor = opt_num(&c.get(10), &format!("{name} EI factor"))?;
    j.angle_from_deg = opt_num(&c.get(11), &format!("{name} angle from"))?;
    j.angle_to_deg = opt_num(&c.get(12), &format!("{name} angle to"))?;
    j.rigid = match c.get(13).trim() {
        "" | "no" => false,
        "yes" => true,
        v => return Err(format!("{name} rigid: yes or no, not \"{v}\"")),
    };
    Ok(())
}

/// A colour typed in a cell: `#rrggbb` (lower case), or none when empty.
fn parse_color(text: &str, name: &str) -> Result<Option<String>, String> {
    let color = text.trim().to_lowercase();
    if color.is_empty() {
        return Ok(None);
    }
    let hex = color.trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} colour: \"{color}\" — write #rrggbb, e.g. #4c9be8"
        ));
    }
    Ok(Some(format!("#{hex}")))
}

fn system_cells(s: &mut ols_line::electrical::System, c: &Cells, name: &str) -> Result<(), String> {
    s.description = c
        .get(TableKind::Systems.column("description"))
        .trim()
        .to_owned();
    s.name = c.get(1);
    s.kind = match c.get(2).trim().to_uppercase().as_str() {
        "AC" => ols_line::electrical::SystemKind::Ac,
        "DC" => ols_line::electrical::SystemKind::Dc,
        other => return Err(format!("{name} kind: \"{other}\" is not AC or DC")),
    };
    s.voltage_kv = num(&c.get(3), &format!("{name} voltage"))?;
    s.frequency_hz = opt_num(&c.get(4), &format!("{name} frequency"))?;
    let labels: Vec<String> = c
        .get(5)
        .split([' ', ','])
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    // A default shown reads empty: the colour by its place.
    s.color = parse_color(&c.get(7), name)?;
    let old = std::mem::take(&mut s.phases);
    s.phases = labels
        .into_iter()
        .enumerate()
        .map(|(k, label)| {
            let mut p = old.get(k).cloned().unwrap_or_else(|| {
                serde_json::from_value(serde_json::json!({ "label": "" })).expect("a phase")
            });
            p.label = label;
            p
        })
        .collect();
    // Line codes, one per phase in order (fewer: the rest by place; a
    // default shown reads empty).
    let codes: Vec<String> = c.get(6).split_whitespace().map(str::to_owned).collect();
    if codes.len() > s.phases.len() {
        return Err(format!(
            "{name} line codes: {} for {} phases",
            codes.len(),
            s.phases.len()
        ));
    }
    for (k, p) in s.phases.iter_mut().enumerate() {
        p.code = match codes.get(k) {
            None => None,
            Some(code) if ols_line::electrical::is_line_code(code) => Some(code.clone()),
            Some(code) => {
                return Err(format!(
                    "{name} line code \"{code}\": dots and dashes, e.g. - .- .--"
                ));
            }
        };
    }
    Ok(())
}

/// The alignments as edited: names, routes, order; new ones (without
/// points) and deletions (their points stay).
fn alignment_list_commands(
    stored: &ols_line::Line,
    rows: &[Row],
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    let known = |r: &str| stored.routes.iter().any(|x| x.id == r);
    // (order, alignment) as the rows give them.
    let mut list: Vec<(f64, ols_line::Alignment)> = Vec::new();
    for (k, row) in rows.iter().enumerate() {
        if page.deleted.contains(&row.key) {
            continue;
        }
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        let mut a = stored
            .alignment_by_id(&row.key)
            .ok_or_else(|| format!("no alignment {}", row.key))?
            .clone();
        let new_id = c.get(0).trim().to_owned();
        if new_id.is_empty() {
            return Err(format!("alignment {}: give it an id", row.key));
        }
        if new_id != row.key {
            out.commands.push((
                format!("renamed alignment {} to {new_id}", row.key),
                Command::Rename {
                    kind: Kind::Alignment,
                    id: row.key.clone(),
                    new_id: new_id.clone(),
                },
            ));
            a.id = new_id;
        }
        a.name = c.get(1).trim().to_owned();
        a.route = c.get(2).trim().to_owned();
        if !known(&a.route) {
            return Err(format!(
                "alignment {}: choose its route (none yet: create one above)",
                a.id
            ));
        }
        a.description = c.get(5).trim().to_owned();
        let mut order = num(&c.get(3), &format!("alignment {} #", a.id))?;
        if c.changed(3) {
            order -= 1e-9;
        }
        let _ = k;
        list.push((order, a));
    }
    for (k, cells) in page.added.iter().enumerate() {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let id = c.get(0).trim().to_owned();
        if id.is_empty() {
            return Err(format!("new alignment {}: give it an id", k + 1));
        }
        if list.iter().any(|(_, a)| a.id == id) {
            return Err(format!("there is an alignment {id} already"));
        }
        let route = c.get(2).trim().to_owned();
        if !known(&route) {
            return Err(format!(
                "new alignment {id}: choose its route (none yet: create one above)"
            ));
        }
        let mut a = ols_line::Alignment::new(&id, &route, Vec::new());
        a.name = c.get(1).trim().to_owned();
        a.description = c.get(5).trim().to_owned();
        let order = opt_num(&c.get(3), &format!("new alignment {id} #"))?.unwrap_or(f64::MAX);
        list.push((order + 1e-9, a));
    }
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
    let alignments: Vec<ols_line::Alignment> = list.into_iter().map(|x| x.1).collect();
    // Renamed ones are put under their new id after the rename.
    let mut renamed = stored.clone();
    for (_, c) in &out.commands {
        if let Command::Rename { id, new_id, .. } = c
            && let Some(a) = renamed.alignments.iter_mut().find(|a| &a.id == id)
        {
            a.id = new_id.clone();
        }
    }
    out.commands.push((
        "changed the alignments".into(),
        crate::editors::alignments_command(&renamed, &alignments)?,
    ));
    Ok(())
}

/// The alignment points as edited: each point's values (the same on every
/// row of it), the alignments' lists of points in the order typed; a row
/// deleted deletes its point (off every alignment it is on).
fn point_commands(
    model: &Model,
    stored: &ols_line::Line,
    rows: &[Row],
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    let point_col = TableKind::AlignmentPoints.column("point");
    let structure_col = TableKind::AlignmentPoints.column("structure");
    let description_col = TableKind::AlignmentPoints.column("description");
    let point_of = |c: &Cells, name: &str| -> Result<ols_line::AlignmentPoint, String> {
        let structure = opt(&c.get(structure_col));
        let position = match &structure {
            Some(s) if c.changed(structure_col) || c.typed.is_none() => {
                model
                    .line
                    .structure(s)
                    .ok_or_else(|| format!("{name}: no structure {s}"))?
                    .position
            }
            Some(s) if model.line.structure(s).is_none() => {
                return Err(format!("{name}: no structure {s}"));
            }
            _ => {
                let e = num(&c.get(3), &format!("{name} E"))?;
                let n = num(&c.get(4), &format!("{name} N"))?;
                let z = match opt_num(&c.get(5), &format!("{name} z"))? {
                    Some(z) => z,
                    None => model.ground_at(e, n).unwrap_or(0.0),
                };
                [e, n, z]
            }
        };
        Ok(ols_line::AlignmentPoint {
            id: c.get(point_col).trim().to_owned(),
            description: c.get(description_col).trim().to_owned(),
            position,
            structure,
        })
    };
    let alignment_of = |c: &Cells, name: &str| -> Result<Option<String>, String> {
        let a = c.get(0).trim().to_owned();
        if a.is_empty() {
            return Ok(None);
        }
        if stored.alignment_by_id(&a).is_none() {
            return Err(format!(
                "{name}: no alignment {a} (make it in the Alignments table)"
            ));
        }
        Ok(Some(a))
    };
    // Points deleted: off every alignment.
    let deleted: BTreeSet<String> = page
        .deleted
        .iter()
        .filter_map(|k| k.split_once('\t').map(|(_, p)| p.to_owned()))
        .collect();
    for p in &deleted {
        let on = stored.point_alignments(p);
        if on.len() > 1 {
            out.takes.push(format!(
                "alignment point {p} comes off {} (the others keep their order)",
                on.join(", ")
            ));
        }
    }
    // The points' values (edited on any of their rows) and the places.
    let mut points: BTreeMap<String, ols_line::AlignmentPoint> = BTreeMap::new();
    let mut edited: BTreeMap<String, String> = BTreeMap::new();
    let mut places: BTreeMap<String, Vec<(f64, String)>> = BTreeMap::new();
    for row in rows {
        let (_, pid) = row.key.split_once('\t').unwrap_or(("", &row.key));
        if deleted.contains(pid) {
            continue;
        }
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        let name = format!("point {pid}");
        let p = point_of(&c, &name)?;
        let typed = page
            .edits
            .get(&row.key)
            .is_some_and(|e| e.keys().any(|k| ![0, 1].contains(k)));
        match points.get(pid) {
            Some(q) if typed && edited.contains_key(pid) && *q != p => {
                return Err(format!(
                    "{name}: changed differently on two of its rows ({} and {})",
                    edited[pid], row.cells[0]
                ));
            }
            Some(_) if !typed => {}
            _ => {
                points.insert(pid.to_owned(), p);
                if typed {
                    edited.insert(pid.to_owned(), row.cells[0].clone());
                }
            }
        }
        let Some(a) = alignment_of(&c, &name)? else {
            continue;
        };
        let mut order = opt_num(&c.get(1), &format!("{name} #"))?.unwrap_or(f64::MAX);
        // Moved to another alignment, or renumbered: before the point that
        // has that number there.
        if a != row.cells[0] || c.changed(1) {
            order -= 1e-9;
        }
        places.entry(a).or_default().push((order, pid.to_owned()));
    }
    let mut new_points = Vec::new();
    for (k, cells) in page.added.iter().enumerate() {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let name = format!("new row {}", k + 1);
        let pid = c.get(point_col).trim().to_owned();
        if pid.is_empty() {
            return Err(format!("{name}: give the point an id"));
        }
        let existing = stored.alignment_point(&pid).is_some() || points.contains_key(&pid);
        if !existing {
            let p = point_of(&c, &name)?;
            points.insert(pid.clone(), p.clone());
            new_points.push(pid.clone());
        }
        if let Some(a) = alignment_of(&c, &name)? {
            let order = opt_num(&c.get(1), &format!("{name} #"))?.unwrap_or(f64::MAX);
            places.entry(a).or_default().push((order + 1e-9, pid));
        }
    }
    // The alignments' new lists.
    let mut alignments = stored.alignments.clone();
    for a in &mut alignments {
        let mut list = places.remove(&a.id).unwrap_or_default();
        list.sort_by(|x, y| x.0.total_cmp(&y.0));
        let ids: Vec<String> = list.into_iter().map(|x| x.1).collect();
        if let Some(d) = ids
            .iter()
            .enumerate()
            .find(|(k, p)| ids[..*k].contains(p))
            .map(|x| x.1)
        {
            return Err(format!("alignment {}: point {d} twice", a.id));
        }
        a.points = ids;
    }
    // Deletions first: they take the points off the stored alignments,
    // the alignments' new lists are put after them.
    let mut commands: Vec<Command> = deleted
        .iter()
        .map(|p| Command::Delete {
            kind: Kind::AlignmentPoint,
            id: p.clone(),
            confirm: true,
        })
        .collect();
    for (pid, p) in &points {
        if stored.alignment_point(pid) != Some(p) {
            commands.push(Command::Put {
                kind: Kind::AlignmentPoint,
                id: pid.clone(),
                value: serde_json::to_value(p).map_err(|e| e.to_string())?,
                ord: None,
            });
        }
    }
    match crate::editors::alignments_command(stored, &alignments)? {
        Command::Batch { commands: c } => commands.extend(c),
        c => commands.push(c),
    }
    if !commands.is_empty() {
        out.commands.push((
            "changed the alignment points".into(),
            if commands.len() == 1 {
                commands.remove(0)
            } else {
                Command::Batch { commands }
            },
        ));
    }
    Ok(())
}

/// Weather cases: the model's case file with the changes written in
/// (comments and layout kept), checked; its path and new text.
/// The weather case file the table writes: the model's, else (a project
/// still on the default case) a new one the project keeps.
pub fn weather_path(model: &Model) -> Option<PathBuf> {
    model.sources.weather.clone().or_else(|| {
        model.can_edit().ok()?;
        Some(
            model
                .library_dir
                .as_ref()?
                .join("weather")
                .join(NEW_WEATHER_FILE),
        )
    })
}

/// The name of a project's first weather case file.
pub const NEW_WEATHER_FILE: &str = "weather-cases.olswc";

fn weather_file(
    model: &Model,
    rows: &[Row],
    page: &TablePage,
) -> Result<(PathBuf, String), String> {
    let path =
        weather_path(model).ok_or_else(|| "the model has no weather case file".to_owned())?;
    // No file yet: the default case's row is written whole into a new one.
    let new = model.sources.weather.is_none();
    let text = if new {
        format!(
            "format = \"{}\"\nversion = {}\nname = \"Weather cases\"\n",
            ols_weather::WEATHER_FILE_FORMAT,
            ols_weather::WEATHER_FILE_VERSION
        )
    } else {
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?
    };
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let old = doc
        .get("case")
        .and_then(toml_edit::Item::as_array_of_tables)
        .cloned()
        .unwrap_or_default();
    let mut cases: Vec<(f64, toml_edit::Table)> = Vec::new();
    let write = |t: &mut toml_edit::Table, c: &Cells, all: bool| -> Result<(), String> {
        let name = c.get(W_NAME);
        let label = |col: usize| WEATHER[col].0.replace('\n', " ").trim().to_owned();
        let number = |col: usize, unit: &str| -> Result<Option<toml_edit::Item>, String> {
            let v = c.get(col);
            let v = v.trim();
            if v.is_empty() {
                return Ok(None);
            }
            let x = num(v, &format!("{name} {}", label(col)))?;
            Ok(Some(toml_edit::value(format!("{x} {unit}"))))
        };
        let mut set = |field: &str, item: Option<toml_edit::Item>| match item {
            Some(i) => {
                t.insert(field, i);
            }
            None => {
                t.remove(field);
            }
        };
        let touched = |col: usize| all || c.changed(col);
        if touched(W_NAME) {
            let v = c.get(W_NAME).trim().to_owned();
            if v.is_empty() {
                return Err("a case needs a name".into());
            }
            set("name", Some(toml_edit::value(v)));
        }
        if touched(W_DESCRIPTION) {
            let v = c.get(W_DESCRIPTION).trim().to_owned();
            set("description", (!v.is_empty()).then(|| toml_edit::value(v)));
        }
        if touched(W_TAGS) {
            let mut a = toml_edit::Array::new();
            for tag in c
                .get(W_TAGS)
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                a.push(tag);
            }
            set("tags", (!a.is_empty()).then(|| toml_edit::value(a)));
        }
        if touched(W_TEMP) {
            if c.get(W_TEMP).trim().is_empty() {
                return Err(format!("{name}: a temperature is needed"));
            }
            set("temperature", number(W_TEMP, "°C")?);
        }
        if touched(W_AMBIENT) {
            set("ambient_temperature", number(W_AMBIENT, "°C")?);
        }
        if touched(W_ICE) {
            set("ice_thickness", number(W_ICE, "mm")?);
        }
        if touched(W_ICE_LOAD) {
            set("ice_load", number(W_ICE_LOAD, "N/m")?);
        }
        if touched(W_ICE_DENSITY) {
            set("ice_density", number(W_ICE_DENSITY, "kg/m³")?);
        }
        // Wind: pressure P = Q·V². The pressure typed wins; else a velocity
        // (or another Q) typed gives the pressure of the velocity shown.
        if touched(W_Q) {
            let q = opt_num(&c.get(W_Q), &format!("{name} air density factor"))?;
            if q.is_some_and(|q| q <= 0.0) {
                return Err(format!("{name}: the air density factor must be > 0"));
            }
            set("air_density_factor", q.map(toml_edit::value));
        }
        let q = opt_num(&c.get(W_Q), &format!("{name} air density factor"))?
            .unwrap_or(ols_weather::DEFAULT_AIR_DENSITY_FACTOR);
        if touched(W_PRESSURE) && (all || !c.changed(W_VELOCITY)) {
            set("wind_pressure", number(W_PRESSURE, "Pa")?);
        } else if c.changed(W_VELOCITY) || c.changed(W_Q) {
            let v = opt_num(&c.get(W_VELOCITY), &format!("{name} wind velocity"))?.unwrap_or(0.0);
            if v < 0.0 {
                return Err(format!("{name}: the wind velocity must be ≥ 0"));
            }
            let p = q * v * v;
            set(
                "wind_pressure",
                (p > 0.0)
                    .then(|| toml_edit::value(format!("{} Pa", (p * 1000.0).round() / 1000.0))),
            );
        }
        Ok(())
    };
    for row in rows {
        if page.deleted.contains(&row.key) {
            continue;
        }
        let k: usize = row.key.parse().map_err(|_| "bad row".to_owned())?;
        let mut t = if new {
            toml_edit::Table::new()
        } else {
            old.get(k)
                .cloned()
                .ok_or_else(|| format!("case {}: not in {}", row.cells[1], path.display()))?
        };
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        write(&mut t, &c, new)?;
        cases.push((
            num(&c.get(W_ORDER), &format!("{} #", row.cells[W_NAME]))?,
            t,
        ));
    }
    for cells in &page.added {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let mut t = toml_edit::Table::new();
        write(&mut t, &c, true)?;
        let order = opt_num(&c.get(W_ORDER), "new case #")?.unwrap_or(f64::MAX);
        cases.push((order + 1e-9, t));
    }
    cases.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut array = toml_edit::ArrayOfTables::new();
    for (_, t) in cases {
        array.push(t);
    }
    doc.insert("case", toml_edit::Item::ArrayOfTables(array));
    let text = doc.to_string();
    ols_weather::WeatherCases::parse(&text)?;
    Ok((path, text))
}

/// An open table page: pending changes, filter, sort.
#[derive(Default)]
pub struct TablePage {
    /// Pending cell texts of existing rows: row key → column → text.
    pub edits: BTreeMap<String, BTreeMap<usize, String>>,
    /// New rows' cells.
    pub added: Vec<Vec<String>>,
    /// Rows marked for deletion (keys).
    pub deleted: BTreeSet<String>,
    /// Rows shown contain this (any cell, any case).
    pub filter: String,
    /// Sorted by this column (ascending when true).
    pub sort: Option<(usize, bool)>,
    /// Why Apply failed, or what it did.
    pub message: Option<(String, bool)>,
    /// What the deletions take with them, waiting for a second Apply.
    pub confirm: Option<Vec<String>>,
    /// A new route's name typed (Alignments table: "+ New route").
    pub new_route: String,
    /// The alignment shown (Alignment points table; empty: all).
    pub alignment: String,
    /// The colour tool, open for a colour cell.
    pub color: Option<(ColorCell, crate::color_tool::ColorTool)>,
}

/// A colour cell the colour tool writes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorCell {
    /// Column `column` of the row `key`, whose value was `original` (empty
    /// for a default shown: what is picked is set even when the same).
    Row {
        /// The row's key.
        key: String,
        /// The column.
        column: usize,
        /// Its value.
        original: String,
    },
    /// Column `column` of new row `index`.
    New {
        /// The new row.
        index: usize,
        /// The column.
        column: usize,
    },
}

/// What a table asks of the application.
pub enum TableAction {
    /// Make these changes.
    Apply(Changes),
    /// Select this object.
    Select(Selection),
    /// Show weather case `k`.
    ShowCase(usize),
}

impl TablePage {
    /// Number of changes pending.
    pub fn pending(&self) -> usize {
        self.edits.values().map(BTreeMap::len).sum::<usize>()
            + self.added.len()
            + self.deleted.len()
    }

    /// Drops every pending change.
    pub fn clear(&mut self) {
        self.edits.clear();
        self.added.clear();
        self.deleted.clear();
        self.confirm = None;
    }

    /// Opens the colour tool for `cell`, showing `text` (its colour, if
    /// it reads) as the old colour.
    pub fn open_color_tool(&mut self, kind: TableKind, cell: ColorCell, text: &str) {
        let what = match &cell {
            ColorCell::Row { key, .. } => key.clone(),
            ColorCell::New { .. } => "new row".into(),
        };
        let old = ols_core::text::parse_rgb(text.trim());
        self.color = Some((
            cell,
            crate::color_tool::ColorTool::new(format!("Colour — {}: {what}", kind.title()), old),
        ));
    }

    /// Writes a colour chosen in the colour tool to its cell (a typed edit,
    /// pending until Apply).
    pub fn write_color(&mut self, cell: &ColorCell, color: [u8; 3]) {
        let text = ols_core::text::format_rgb(color);
        match cell {
            ColorCell::Row {
                key,
                column,
                original,
            } => self.set(key, *column, &text, original),
            ColorCell::New { index, column } => {
                if let Some(c) = self.added.get_mut(*index).and_then(|r| r.get_mut(*column)) {
                    *c = text;
                }
            }
        }
    }

    /// Sets a cell of an existing row (back to its value: not pending).
    pub fn set(&mut self, key: &str, column: usize, text: &str, original: &str) {
        if text == original {
            if let Some(e) = self.edits.get_mut(key) {
                e.remove(&column);
                if e.is_empty() {
                    self.edits.remove(key);
                }
            }
        } else {
            self.edits
                .entry(key.to_owned())
                .or_default()
                .insert(column, text.to_owned());
        }
        self.confirm = None;
    }

    /// What Apply does: the changes, or — when deletions take other objects
    /// and that was not confirmed — `None` (the list is in `confirm`).
    pub fn prepare(
        &mut self,
        kind: TableKind,
        model: &Model,
        case: usize,
        confirmed: bool,
    ) -> Result<Option<Changes>, String> {
        let rows = kind.rows(model, case);
        let changes = kind.changes(model, case, &rows, self)?;
        if !changes.takes.is_empty() && !confirmed {
            self.confirm = Some(changes.takes.clone());
            return Ok(None);
        }
        Ok(Some(changes))
    }

    /// The page.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        kind: TableKind,
        model: &Model,
        case: usize,
        selection: &Selection,
        palette: &mut crate::color_tool::Palette,
    ) -> Option<TableAction> {
        // Weather cases are a file of the model, not part of the project.
        let editable = match kind {
            TableKind::WeatherCases => weather_path(model).is_some(),
            _ => model.can_edit().is_ok(),
        };
        let columns = kind.columns();
        let rows = kind.rows(model, case);
        let mut action = None;
        let mut apply = false;
        let mut confirmed = false;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filter")
                    .desired_width(160.0),
            );
            ui.separator();
            if ui
                .add_enabled(
                    editable && !kind.fixed_rows(),
                    egui::Button::new("+ Add row"),
                )
                .on_hover_text("A new object, typed in the row (pending until Apply)")
                .clicked()
            {
                let cells = kind.new_row(model, case, &self.added);
                self.added.push(cells);
                self.confirm = None;
            }
            let n = self.pending();
            apply = ui
                .add_enabled(
                    editable && n > 0,
                    egui::Button::new(format!("Apply {n} changes")),
                )
                .on_hover_text("All of them in one command (one undo)")
                .clicked();
            if ui.add_enabled(n > 0, egui::Button::new("Revert")).clicked() {
                self.clear();
                self.message = None;
            }
            ui.label(
                egui::RichText::new(format!("{} rows", rows.len()))
                    .small()
                    .weak(),
            );
            if !editable {
                ui.label(egui::RichText::new("read-only").small().weak());
            }
        });
        if let Some(takes) = &self.confirm {
            ui.colored_label(
                egui::Color32::YELLOW,
                "This also changes (one log entry, undo brings everything back):",
            );
            egui::ScrollArea::vertical()
                .id_salt("table-confirm")
                .max_height(160.0)
                .show(ui, |ui| {
                    for t in takes {
                        ui.label(egui::RichText::new(format!("  {t}")).small());
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Apply all that").clicked() {
                    apply = true;
                    confirmed = true;
                }
                if ui.button("Cancel").clicked() {
                    self.confirm = None;
                }
            });
        }
        if kind == TableKind::WeatherCases {
            ui.label(
                egui::RichText::new(match &model.sources.weather {
                    Some(p) => format!(
                        "{} — Apply writes the file (comments kept) and solves the model again",
                        p.display()
                    ),
                    None if weather_path(model).is_some() => "The project uses a default case (15 °C): add cases here (+ Add row), Apply keeps them in the project".into(),
                    None => "The model uses a default case (15 °C): make a case file in Criteria › Weather cases and choose Use for the model".into(),
                })
                .small()
                .weak(),
            );
        }
        if kind == TableKind::Alignments {
            // An alignment needs a route: one is made here when there is none.
            let stored = model.stored_line().unwrap_or(&model.line);
            ui.horizontal(|ui| {
                if stored.routes.is_empty() {
                    crate::menu::message(
                        ui,
                        egui::RichText::new("No route yet: an alignment needs one. Create it:")
                            .small(),
                        Some(egui::Color32::YELLOW),
                    );
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_route)
                        .hint_text("+ New route…")
                        .desired_width(140.0),
                );
                let name = self.new_route.trim().to_owned();
                if ui
                    .add_enabled(
                        editable && !name.is_empty(),
                        egui::Button::new("Create the route"),
                    )
                    .clicked()
                {
                    match crate::editors::new_route(stored, &name, &name) {
                        Ok((what, command)) => {
                            self.new_route.clear();
                            action = Some(TableAction::Apply(Changes {
                                commands: vec![(what, command)],
                                ..Changes::default()
                            }));
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
            });
            ui.label(
                egui::RichText::new("Each alignment belongs to one route. # orders them (3.5 goes between 3 and 4). Their points: Alignment points table.")
                    .small()
                    .weak(),
            );
        }
        if kind == TableKind::AlignmentPoints {
            let stored = model.stored_line().unwrap_or(&model.line);
            ui.horizontal(|ui| {
                ui.label("Alignment");
                let shown = if self.alignment.is_empty() {
                    "all"
                } else if self.alignment == "\t" {
                    "(on no alignment)"
                } else {
                    self.alignment.as_str()
                };
                egui::ComboBox::from_id_salt("points-alignment")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.alignment, String::new(), "all");
                        for a in &stored.alignments {
                            ui.selectable_value(
                                &mut self.alignment,
                                a.id.clone(),
                                format!("{} (route {})", a.id, a.route),
                            );
                        }
                        ui.selectable_value(
                            &mut self.alignment,
                            "\t".to_owned(),
                            "(on no alignment)",
                        );
                    });
            });
            ui.label(
                egui::RichText::new("The profile view follows these points. A point on several alignments has a row on each, with its place (#) there; # orders the points (3.5 goes between 3 and 4). Deleting a row deletes the point: it comes off every alignment, the others keep their order. A point may wait on no alignment.")
                    .small()
                    .weak(),
            );
        }
        if apply {
            match self.prepare(kind, model, case, confirmed) {
                Ok(Some(c)) => action = Some(TableAction::Apply(c)),
                Ok(None) => {}
                Err(e) => self.message = Some((e, true)),
            }
        }
        if let Some((m, error)) = &self.message {
            let t = egui::RichText::new(m).small();
            if *error {
                crate::menu::error_label(ui, t);
            } else if m.contains("warning:") {
                crate::menu::message(ui, t, Some(egui::Color32::YELLOW));
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, t);
            }
        }
        // Filter and sort (on the cells as shown, edits included).
        let shown_cell = |r: &Row, c: usize| -> String {
            self.edits
                .get(&r.key)
                .and_then(|e| e.get(&c))
                .cloned()
                .unwrap_or_else(|| r.cells[c].clone())
        };
        let needle = self.filter.trim().to_lowercase();
        // Alignment points: the alignment chosen ("\t": on none).
        let wanted = |r: &Row| {
            kind != TableKind::AlignmentPoints
                || self.alignment.is_empty()
                || (self.alignment == "\t" && r.cells[0].is_empty())
                || r.cells[0] == self.alignment
        };
        let mut order: Vec<usize> = (0..rows.len())
            .filter(|&i| wanted(&rows[i]))
            .filter(|&i| {
                needle.is_empty()
                    || rows[i]
                        .cells
                        .iter()
                        .any(|c| c.to_lowercase().contains(&needle))
            })
            .collect();
        if let Some((c, up)) = self.sort {
            order.sort_by(|&a, &b| {
                let (x, y) = (shown_cell(&rows[a], c), shown_cell(&rows[b], c));
                let o = match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                    (Ok(p), Ok(q)) => p.total_cmp(&q),
                    // sec2 before sec10.
                    _ => ols_core::text::natural_cmp(&x, &y),
                };
                if up { o } else { o.reverse() }
            });
        }
        let row_height = 20.0;
        let buttons = 52.0;
        // Headers of two lines: the name, then the unit.
        let header_height = if columns.iter().any(|c| c.0.contains('\n')) {
            34.0
        } else {
            row_height
        };
        let added = self.added.len();
        // A colour cell's tool button clicked: the cell and its text.
        let mut open_tool: Option<(ColorCell, String)> = None;
        // The grid: a line at every column edge and under every row, from
        // the columns' widths (each cell `width` wide, 2 apart, after the
        // two row buttons) — every row the same widths whatever its cells.
        let widths: Vec<f32> = columns.iter().map(|c| c.1).collect();
        let grid = move |ui: &egui::Ui, row: egui::Rect| {
            let stroke =
                egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
            let p = ui.painter();
            let mut x = row.left() + buttons - 1.0;
            p.line_segment(
                [egui::pos2(x, row.top()), egui::pos2(x, row.bottom())],
                stroke,
            );
            for w in &widths {
                x += w + 2.0;
                p.line_segment(
                    [egui::pos2(x, row.top()), egui::pos2(x, row.bottom())],
                    stroke,
                );
            }
            p.line_segment(
                [
                    egui::pos2(row.left() + buttons - 1.0, row.bottom() + 1.0),
                    egui::pos2(x, row.bottom() + 1.0),
                ],
                stroke,
            );
        };
        egui::ScrollArea::horizontal().show(ui, |ui| {
            let header = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                // Where the rows have their two buttons.
                ui.add_sized([24.0, header_height], egui::Label::new(""));
                ui.add_sized([24.0, header_height], egui::Label::new(""));
                for (c, col) in columns.iter().enumerate() {
                    let arrow = match self.sort {
                        Some((s, true)) if s == c => " ▲",
                        Some((s, false)) if s == c => " ▼",
                        _ => "",
                    };
                    let text = egui::RichText::new(format!("{}{arrow}", col.0)).strong();
                    if ui
                        .add_sized([col.1, header_height], egui::Button::new(text).frame(false))
                        .on_hover_text(format!("{} — click to sort", col.3))
                        .clicked()
                    {
                        self.sort = match self.sort {
                            Some((s, true)) if s == c => Some((c, false)),
                            Some((s, false)) if s == c => None,
                            _ => Some((c, true)),
                        };
                    }
                }
            });
            grid(ui, header.response.rect);
            ui.add_space(2.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, order.len() + added, |ui, range| {
                    for i in range {
                        if i >= order.len() {
                            // A new row.
                            let k = i - order.len();
                            let cells = ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if ui
                                    .add_sized([buttons - 2.0, row_height], egui::Button::new("🗑"))
                                    .on_hover_text("Remove this new row")
                                    .clicked()
                                {
                                    self.added.remove(k);
                                    self.confirm = None;
                                    return;
                                }
                                for (c, col) in columns.iter().enumerate() {
                                    if col.2 == Derived {
                                        ui.add_sized(
                                            [col.1, row_height],
                                            egui::Label::new(egui::RichText::new("new").weak()),
                                        );
                                        continue;
                                    }
                                    if let Some(list) = choices(kind, c, model, &self.added[k]) {
                                        let mut text = self.added[k][c].clone();
                                        if choice_cell(
                                            ui,
                                            ("new", kind, k, c),
                                            col.1,
                                            &mut text,
                                            list,
                                            Some(egui::Color32::LIGHT_GREEN),
                                            kind.none_choice(c),
                                        ) {
                                            self.added[k][c] = text;
                                        }
                                        continue;
                                    }
                                    if col.0 == "colour" {
                                        let (tool, _) = color_cell(
                                            ui,
                                            [col.1, row_height],
                                            &mut self.added[k][c],
                                            Some(egui::Color32::LIGHT_GREEN),
                                            false,
                                            col.3,
                                        );
                                        if tool {
                                            open_tool = Some((
                                                ColorCell::New { index: k, column: c },
                                                self.added[k][c].clone(),
                                            ));
                                        }
                                        continue;
                                    }
                                    let edit = egui::TextEdit::singleline(&mut self.added[k][c])
                                        .text_color(egui::Color32::LIGHT_GREEN);
                                    ui.add_sized([col.1, row_height], edit);
                                }
                            });
                            grid(ui, cells.response.rect);
                            continue;
                        }
                        let row = &rows[order[i]];
                        let deleted = self.deleted.contains(&row.key);
                        let cells = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let selected = match kind {
                                TableKind::WeatherCases => row.key == case.to_string(),
                                _ => *selection == row.select && row.select != Selection::None,
                            };
                            if ui
                                .add_sized(
                                    [24.0, row_height],
                                    egui::Button::selectable(selected, "⤴"),
                                )
                                .on_hover_text(match kind {
                                    TableKind::WeatherCases => "Show this weather case",
                                    _ => "Select it and bring the views to it",
                                })
                                .clicked()
                            {
                                action = Some(match kind {
                                    TableKind::WeatherCases => {
                                        TableAction::ShowCase(row.key.parse().unwrap_or(0))
                                    }
                                    _ => TableAction::Select(row.select.clone()),
                                });
                            }
                            if kind == TableKind::Circuits {
                                // Reverse: its starts become its ends and
                                // back (pending until Apply).
                                let shown = self
                                    .edits
                                    .get(&row.key)
                                    .and_then(|e| e.get(&4))
                                    .cloned()
                                    .unwrap_or_else(|| row.cells[4].clone());
                                let starts: Vec<&str> =
                                    shown.split(';').map(str::trim).collect();
                                let flipped: Vec<String> = circuit_ends(&row.cells)
                                    .into_iter()
                                    .filter(|p| !starts.contains(&p.as_str()))
                                    .collect();
                                if ui
                                    .add_enabled(
                                        editable && !flipped.is_empty(),
                                        egui::Button::new("⇄")
                                            .min_size(egui::vec2(24.0, row_height)),
                                    )
                                    .on_hover_text("Reverse: its starts become its ends and its ends its starts (on Apply)")
                                    .clicked()
                                {
                                    self.edits
                                        .entry(row.key.clone())
                                        .or_default()
                                        .insert(4, flipped.join("; "));
                                    self.confirm = None;
                                }
                            } else if ui
                                .add_enabled(
                                    editable && !kind.fixed_rows(),
                                    egui::Button::selectable(deleted, "🗑")
                                        .min_size(egui::vec2(24.0, row_height)),
                                )
                                .on_hover_text(if deleted {
                                    "Marked for deletion: click to keep it"
                                } else {
                                    "Delete it (on Apply)"
                                })
                                .clicked()
                            {
                                if deleted {
                                    self.deleted.remove(&row.key);
                                } else {
                                    self.deleted.insert(row.key.clone());
                                }
                                self.confirm = None;
                            }
                            for (c, col) in columns.iter().enumerate() {
                                let original = &row.cells[c];
                                if deleted || col.2 != Yes || !editable {
                                    let mut t = egui::RichText::new(original).weak();
                                    if deleted {
                                        t = t.strikethrough().color(egui::Color32::LIGHT_RED);
                                    }
                                    // A colour read-only: on its colour.
                                    let swatch = (col.0 == "colour" && !deleted)
                                        .then(|| ols_core::text::parse_rgb(original.trim()))
                                        .flatten()
                                        .map(crate::color_tool::color32);
                                    if let Some(fill) = swatch {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(col.1, row_height),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(rect.shrink(1.0), 2.0, fill);
                                        ui.painter().text(
                                            rect.left_center() + egui::vec2(4.0, 0.0),
                                            egui::Align2::LEFT_CENTER,
                                            original,
                                            egui::TextStyle::Body.resolve(ui.style()),
                                            crate::color_tool::text_on(fill),
                                        );
                                        continue;
                                    }
                                    ui.add_sized(
                                        [col.1, row_height],
                                        egui::Label::new(t).truncate(),
                                    )
                                    .on_hover_text(
                                        match col.2 {
                                            Derived => format!("{original} — counted, not typed"),
                                            New => format!("{original} — {}", col.3),
                                            Yes => original.clone(),
                                        },
                                    );
                                    continue;
                                }
                                let edited = self.edits.get(&row.key).and_then(|e| e.get(&c));
                                let mut text = edited.cloned().unwrap_or_else(|| original.clone());
                                // A default shown (not stored): dimmed until typed over.
                                let default = edited.is_none()
                                    && row.defaulted.get(c).copied().unwrap_or(false);
                                let dim = ui.visuals().weak_text_color();
                                if let Some(list) = choices(kind, c, model, &row.cells) {
                                    let color = match (edited, default) {
                                        (Some(_), _) => Some(egui::Color32::YELLOW),
                                        (None, true) => Some(dim),
                                        _ => None,
                                    };
                                    if choice_cell(
                                        ui,
                                        (kind, &row.key, c),
                                        col.1,
                                        &mut text,
                                        list,
                                        color,
                                        kind.none_choice(c),
                                    ) {
                                        self.set(&row.key, c, &text, original);
                                    }
                                    continue;
                                }
                                if col.0 == "colour" {
                                    let hover = if default {
                                        format!(
                                            "default (not set on it; type or pick to set it) — {}",
                                            col.3
                                        )
                                    } else {
                                        col.3.to_owned()
                                    };
                                    let (tool, changed) = color_cell(
                                        ui,
                                        [col.1, row_height],
                                        &mut text,
                                        edited.is_some().then_some(egui::Color32::YELLOW),
                                        default,
                                        &hover,
                                    );
                                    if changed {
                                        self.set(&row.key, c, &text, original);
                                    }
                                    if tool {
                                        open_tool = Some((
                                            ColorCell::Row {
                                                key: row.key.clone(),
                                                column: c,
                                                // A default: set even when the same.
                                                original: if default {
                                                    String::new()
                                                } else {
                                                    original.clone()
                                                },
                                            },
                                            text.clone(),
                                        ));
                                    }
                                    continue;
                                }
                                let mut edit = egui::TextEdit::singleline(&mut text);
                                if edited.is_some() {
                                    edit = edit.text_color(egui::Color32::YELLOW);
                                } else if default {
                                    edit = edit.text_color(dim);
                                }
                                let hover = if default {
                                    format!("default (not set on it; type to set it) — {}", col.3)
                                } else {
                                    col.3.to_owned()
                                };
                                if ui
                                    .add_sized([col.1, row_height], edit)
                                    .on_hover_text(hover)
                                    .changed()
                                {
                                    self.set(&row.key, c, &text, original);
                                }
                            }
                        });
                        grid(ui, cells.response.rect);
                    }
                });
        });
        if let Some((cell, text)) = open_tool {
            self.open_color_tool(kind, cell, &text);
        }
        if let Some((cell, tool)) = &mut self.color {
            let id = egui::Id::new(("colour-tool", kind));
            match tool.show(ui.ctx(), id, palette) {
                Some(crate::color_tool::Outcome::Pick(c)) => {
                    let cell = cell.clone();
                    self.write_color(&cell, c);
                    self.color = None;
                }
                Some(crate::color_tool::Outcome::Cancel) => self.color = None,
                None => {}
            }
        }
        action
    }
}

/// A colour cell: its text typed on its colour (dimmed for a `default`
/// shown, outlined in `mark` when changed), and a button opening the colour
/// tool. Whether that button was clicked, and whether the text changed.
fn color_cell(
    ui: &mut egui::Ui,
    size: [f32; 2],
    text: &mut String,
    mark: Option<egui::Color32>,
    default: bool,
    hover: &str,
) -> (bool, bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size[0], size[1]), egui::Sense::hover());
    let bg = ui.visuals().extreme_bg_color;
    let fill = ols_core::text::parse_rgb(text.trim()).map(|c| {
        let c = crate::color_tool::color32(c);
        if default { bg.lerp_to_gamma(c, 0.4) } else { c }
    });
    let text_color = match (fill, mark) {
        (Some(f), _) => {
            let t = crate::color_tool::text_on(f);
            if default { t.gamma_multiply(0.7) } else { t }
        }
        (None, Some(m)) => m,
        (None, None) if default => ui.visuals().weak_text_color(),
        (None, None) => ui.visuals().text_color(),
    };
    let painter = ui.painter();
    painter.rect_filled(rect.shrink(1.0), 2.0, fill.unwrap_or(bg));
    if let Some(m) = mark {
        painter.rect_stroke(
            rect.shrink(1.0),
            2.0,
            egui::Stroke::new(1.5, m),
            egui::StrokeKind::Inside,
        );
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 2.0;
    let (b, button) = child.allocate_exact_size(egui::vec2(18.0, size[1]), egui::Sense::click());
    button.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "colour tool"));
    // Three stripes of colour: the tool.
    let icon = egui::Rect::from_center_size(b.center(), egui::vec2(12.0, 12.0));
    let stripes = [
        egui::Color32::from_rgb(0xe4, 0x57, 0x2e),
        egui::Color32::from_rgb(0xf2, 0xc1, 0x4e),
        egui::Color32::from_rgb(0x4c, 0x9b, 0xe8),
    ];
    for (k, s) in stripes.into_iter().enumerate() {
        let x = icon.left() + 4.0 * k as f32;
        child.painter().rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, icon.top()), egui::vec2(4.0, 12.0)),
            0.0,
            s,
        );
    }
    let outline = if button.hovered() {
        child.visuals().widgets.hovered.fg_stroke
    } else {
        egui::Stroke::new(1.0, text_color)
    };
    child
        .painter()
        .rect_stroke(icon, 1.0, outline, egui::StrokeKind::Outside);
    let button = button.on_hover_text("The colour tool: a wheel, sliders, your palette");
    let edit = egui::TextEdit::singleline(text)
        .frame(egui::Frame::NONE)
        .text_color(text_color);
    let changed = child
        .add_sized([size[0] - 22.0, size[1]], edit)
        .on_hover_text(hover)
        .changed();
    (button.clicked(), changed)
}
