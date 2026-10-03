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

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use eframe::egui;
use ols_line::{SectionPoint, Stringing};
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
    /// Alignment points of the routes (the profiles follow them).
    Alignments,
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
];

const STRINGS: &[Column] = &[
    ("structure", 110.0, New, "the structure it hangs on"),
    (
        "string",
        55.0,
        Yes,
        "set:phase (renumbering: sections and jumpers follow)",
    ),
    ("label", 100.0, Yes, "label"),
    (
        "top across",
        75.0,
        Yes,
        "insulator point: across the line (right looking ahead), m",
    ),
    (
        "top along",
        75.0,
        Yes,
        "insulator point: along the line (ahead), m",
    ),
    ("top up", 75.0, Yes, "insulator point: up from the base, m"),
    ("wire across", 75.0, Yes, "wire point: across, m"),
    ("wire along", 75.0, Yes, "wire point: along, m"),
    ("wire up", 75.0, Yes, "wire point: up, m"),
    (
        "length m",
        65.0,
        Yes,
        "string length (typed: the wire point straight below)",
    ),
    ("weight N", 75.0, Yes, "insulator weight"),
    (
        "insulator",
        110.0,
        Yes,
        "insulator assembly id (empty: none)",
    ),
    (
        "strain",
        55.0,
        Yes,
        "yes: a strain string (free along its conductor)",
    ),
    ("sections", 60.0, Derived, "sections through it"),
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
        "bending stiffness (empty: estimated from the cable; 0: hinged ends)",
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
    ("sections", 70.0, Derived, "sections given this system"),
];

const ALIGNMENTS: &[Column] = &[
    (
        "route",
        110.0,
        New,
        "route (\"line\": the line's own alignment)",
    ),
    (
        "#",
        45.0,
        Yes,
        "order along the route; 3.5 goes between 3 and 4",
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
];

const WEATHER: &[Column] = &[
    (
        "#",
        40.0,
        Yes,
        "order in the file; 2.5 goes between 2 and 3",
    ),
    ("name", 170.0, Yes, "unique name"),
    ("description", 200.0, Yes, "free text"),
    ("tags", 150.0, Yes, "comma separated, e.g. clearance, loads"),
    ("temp °C", 70.0, Yes, "cable temperature"),
    ("ice mm", 65.0, Yes, "radial ice thickness (0: none)"),
    (
        "ice N/m",
        65.0,
        Yes,
        "or: ice load per length (empty: none)",
    ),
    (
        "ice kg/m³",
        75.0,
        Yes,
        "ice density (empty: glaze ice with a thickness)",
    ),
    (
        "wind Pa",
        70.0,
        Yes,
        "wind pressure on the projected diameter",
    ),
    (
        "wind from",
        70.0,
        Yes,
        "left or right, looking ahead along the line (each span from its back to its ahead point)",
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
    format!("{} {}:{}", p.structure, p.set, p.phase)
}

/// "T3 2:1" → a section point.
fn parse_point(text: &str) -> Result<SectionPoint, String> {
    let t = text.trim();
    let (s, sp) = t
        .rsplit_once(' ')
        .ok_or_else(|| format!("\"{t}\": write structure set:phase, e.g. T3 2:1"))?;
    let (set, phase) = parse_set_phase(sp)?;
    Ok(SectionPoint {
        structure: s.trim().to_owned(),
        set,
        phase,
    })
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
            let mut weight: std::collections::HashMap<(&str, u32, u32), f64> =
                std::collections::HashMap::new();
            if let Some(solved) = model.cases.get(case) {
                for shape in &solved.shapes {
                    let Some(sec) = line.sections.iter().find(|s| s.id == shape.section) else {
                        continue;
                    };
                    for (p, w) in sec.points.iter().zip(&shape.estimated_weights) {
                        if let Some(w) = w {
                            weight.insert((p.structure.as_str(), p.set, p.phase), *w);
                        }
                    }
                }
            }
            for row in rows.iter_mut() {
                let (structure, sp) = row.key.split_once('\t').unwrap_or((&row.key, ""));
                let Some((set_no, phase)) = sp
                    .split_once(':')
                    .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
                else {
                    continue;
                };
                if let Some(w) = weight.get(&(structure, set_no, phase)).copied() {
                    let w = f(w, 1);
                    row.cells[10].clear();
                    set(row, 10, w);
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
                        f(ols_line::jumper::estimated_bending_stiffness(c), 1),
                    );
                }
                let angle = f(ols_line::jumper::DEFAULT_TERMINAL_ANGLE_DEG, 1);
                set(row, 10, angle.clone());
                set(row, 11, angle);
            }
        }
        _ => {}
    }
}

impl TableKind {
    /// Every table (the Tables menu).
    pub const ALL: [TableKind; 7] = [
        TableKind::Structures,
        TableKind::Sections,
        TableKind::Strings,
        TableKind::Jumpers,
        TableKind::Systems,
        TableKind::Alignments,
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
            TableKind::Alignments => "Alignment points",
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
            TableKind::Alignments => "alignments",
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
            TableKind::Alignments => ALIGNMENTS,
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
    pub fn rows(self, model: &Model, case: usize) -> Vec<Row> {
        let line = &model.line;
        let mut through: std::collections::HashMap<(&str, u32, u32), usize> =
            std::collections::HashMap::new();
        let mut at_structure: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for s in &line.sections {
            for p in &s.points {
                *through
                    .entry((p.structure.as_str(), p.set, p.phase))
                    .or_default() += 1;
            }
            let mut ids: Vec<&str> = s.points.iter().map(|p| p.structure.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            for id in ids {
                *at_structure.entry(id).or_default() += 1;
            }
        }
        let mut rows: Vec<Row> = match self {
            TableKind::Structures => line
                .structures
                .iter()
                .map(|s| Row {
                    defaulted: Vec::new(),
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
                        s.attachments.len().to_string(),
                        at_structure
                            .get(s.id.as_str())
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                    ],
                })
                .collect(),
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
                        ],
                    }
                })
                .collect(),
            TableKind::Strings => line
                .structures
                .iter()
                .flat_map(|s| {
                    let through = &through;
                    s.attachments.iter().filter_map(move |a| {
                        let form = StringForm::of(model, &s.id, a.set, a.phase)?;
                        Some(Row {
                            defaulted: Vec::new(),
                            key: format!("{}\t{}:{}", s.id, a.set, a.phase),
                            select: Selection::Attachment {
                                structure: s.id.clone(),
                                set: a.set,
                                phase: a.phase,
                            },
                            cells: vec![
                                s.id.clone(),
                                format!("{}:{}", a.set, a.phase),
                                form.label.clone(),
                                f(form.top[0], 3),
                                f(form.top[1], 3),
                                f(form.top[2], 3),
                                f(form.bottom[0], 3),
                                f(form.bottom[1], 3),
                                f(form.bottom[2], 3),
                                f(form.length(), 3),
                                f(form.weight, 1),
                                form.insulator.clone().unwrap_or_default(),
                                if form.strain { "yes" } else { "no" }.into(),
                                through
                                    .get(&(s.id.as_str(), a.set, a.phase))
                                    .copied()
                                    .unwrap_or(0)
                                    .to_string(),
                            ],
                        })
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
                        j.angle_from_deg.map_or(String::new(), |v| f(v, 1)),
                        j.angle_to_deg.map_or(String::new(), |v| f(v, 1)),
                    ],
                })
                .collect(),
            TableKind::Systems => line
                .systems
                .iter()
                .map(|s| Row {
                    defaulted: Vec::new(),
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
                        line.sections
                            .iter()
                            .filter(|x| x.system.as_deref() == Some(s.id.as_str()))
                            .count()
                            .to_string(),
                    ],
                })
                .collect(),
            TableKind::Alignments => alignments(line)
                .into_iter()
                .flat_map(|(route, points)| {
                    points.into_iter().enumerate().map(move |(k, p)| Row {
                        defaulted: Vec::new(),
                        key: format!("{route}\t{k}"),
                        select: Selection::AlignmentPoint {
                            route: if route == "line" {
                                "main".into()
                            } else {
                                route.clone()
                            },
                            index: k,
                        },
                        cells: vec![
                            route.clone(),
                            (k + 1).to_string(),
                            f(p.position[0], 3),
                            f(p.position[1], 3),
                            f(p.position[2], 3),
                            p.structure.clone().unwrap_or_default(),
                        ],
                    })
                })
                .collect(),
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
                        cells: vec![
                            (k + 1).to_string(),
                            w.name.clone(),
                            w.description.clone(),
                            w.tags.join(", "),
                            f(w.temperature.celsius(), 1),
                            f(w.ice_thickness.si() * 1000.0, 1),
                            w.ice_load.map_or(String::new(), |v| f(v.si(), 2)),
                            w.ice_density.map_or(String::new(), |v| f(v.si(), 0)),
                            f(w.wind_pressure.si(), 0),
                            w.wind_from.as_str().into(),
                        ],
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
                cells[8] = "tension".into();
                cells[9] = "20".into();
                cells[10] = "15".into();
            }
            TableKind::Strings => {
                cells[1] = "1:1".into();
                cells[12] = "no".into();
            }
            TableKind::Jumpers => {
                cells[0] = taken_after(
                    line.jumpers.iter().map(|j| j.id.as_str()).collect(),
                    line.jumpers.last().map_or("J0", |j| j.id.as_str()),
                );
                cells[6] = "15".into();
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
                let routes = alignments(line);
                let (route, points) = routes.first().cloned().unwrap_or(("line".into(), vec![]));
                cells[0] = route;
                cells[1] = (points.len() + 1).to_string();
            }
            TableKind::WeatherCases => {
                cells[0] = (model.cases.len() + added.len() + 1).to_string();
                cells[1] = format!("new case {}", added.len() + 1);
                cells[4] = "15".into();
                cells[5] = "0".into();
                cells[8] = "0".into();
                cells[9] = "right".into();
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
                // One put per structure with all its strings, after the
                // deletions (sections through a deleted string go).
                let mut by_structure: BTreeMap<String, Vec<(Option<&String>, Cells)>> =
                    BTreeMap::new();
                let mut deleted: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
                for key in &page.deleted {
                    let (s, sp) = key.split_once('\t').unwrap_or((key, ""));
                    let (set, phase) = parse_set_phase(sp)?;
                    let i = session
                        .attachment_deletion(s, set, phase)
                        .map_err(|e| e.to_string())?;
                    if i.has_dependents() {
                        out.takes.push(i.summary());
                    }
                    deleted.entry(s.to_owned()).or_default().push((set, phase));
                    out.commands.push((
                        format!("deleted string {s} {set}:{phase}"),
                        Command::DeleteAttachment {
                            structure: s.to_owned(),
                            set,
                            phase,
                            confirm: true,
                        },
                    ));
                }
                for key in page.edits.keys().filter(|k| !page.deleted.contains(*k)) {
                    let (s, _) = key.split_once('\t').unwrap_or((key, ""));
                    by_structure
                        .entry(s.to_owned())
                        .or_default()
                        .push((Some(key), existing(key)?));
                }
                for cells in &page.added {
                    let c = Cells {
                        row: cells,
                        typed: None,
                        defaulted: &[],
                    };
                    let s = c.get(0).trim().to_owned();
                    if s.is_empty() {
                        return Err("a new string needs its structure".into());
                    }
                    by_structure.entry(s).or_default().push((None, c));
                }
                for s_id in deleted.keys() {
                    by_structure.entry(s_id.clone()).or_default();
                }
                for (structure, strings) in by_structure {
                    let mut s = stored
                        .structure(&structure)
                        .ok_or_else(|| format!("no structure {structure}"))?
                        .clone();
                    for (set, phase) in deleted.get(&structure).into_iter().flatten() {
                        s.attachments.retain(|a| (a.set, a.phase) != (*set, *phase));
                        if s.structure_type.is_some() && !s.omit.contains(&[*set, *phase]) {
                            s.omit.push([*set, *phase]);
                        }
                    }
                    for (key, c) in strings {
                        let (set, phase) = parse_set_phase(&c.get(1))?;
                        let name = format!("{structure} {set}:{phase}");
                        let mut form = match key {
                            Some(key) => {
                                let (old_set, old_phase) =
                                    parse_set_phase(key.split_once('\t').map_or("", |x| x.1))?;
                                let form = StringForm::of(model, &structure, old_set, old_phase)
                                    .ok_or_else(|| format!("no string {name}"))?;
                                if (set, phase) != (old_set, old_phase) {
                                    out.commands.push((
                                        format!(
                                            "renumbered string {structure} {old_set}:{old_phase} to {set}:{phase}"
                                        ),
                                        Command::RenameAttachment {
                                            structure: structure.clone(),
                                            set: old_set,
                                            phase: old_phase,
                                            new_set: set,
                                            new_phase: phase,
                                        },
                                    ));
                                    s.attachments
                                        .retain(|a| (a.set, a.phase) != (old_set, old_phase));
                                    if s.structure_type.is_some()
                                        && !s.omit.contains(&[old_set, old_phase])
                                    {
                                        s.omit.push([old_set, old_phase]);
                                    }
                                }
                                form
                            }
                            None => StringForm::new_on(model, &structure)
                                .ok_or_else(|| format!("no structure {structure}"))?,
                        };
                        form.set = set;
                        form.phase = phase;
                        string_cells(&mut form, &c, key.is_none(), &name)?;
                        form.apply_to(&mut s);
                    }
                    out.commands.push((
                        format!("changed strings of {structure}"),
                        Command::Put {
                            kind: Kind::Structure,
                            id: structure.clone(),
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
                    Command::Put {
                        kind: Kind::Meta,
                        id: "line".into(),
                        value: serde_json::json!({
                            "name": stored.name,
                            "crs": stored.crs,
                            "alignment": stored.alignment,
                            "display": stored.display,
                            "systems": systems,
                        }),
                        ord: None,
                    },
                ));
            }
            TableKind::Alignments => {
                alignment_commands(model, stored, rows, page, &mut out)?;
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
        (TableKind::Strings, "strain") => (vec!["no".into(), "yes".into()], false),
        (TableKind::Systems, "kind") => (vec!["AC".into(), "DC".into()], false),
        (TableKind::WeatherCases, "wind from") => (vec!["right".into(), "left".into()], false),
        (TableKind::Alignments, "structure") => (
            with_none(model.line.structures.iter().map(|s| s.id.clone()).collect()),
            false,
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
    let mut combo = egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(width - 8.0)
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
            if ui.small_button("none (the cases shown)").clicked() {
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

/// The alignments: per route, its points (`"line"`: the line's own, when
/// it has no routes).
fn alignments(line: &ols_line::Line) -> Vec<(String, Vec<ols_line::AlignmentPoint>)> {
    if line.routes.is_empty() {
        vec![("line".into(), line.alignment.clone())]
    } else {
        line.routes
            .iter()
            .map(|r| (r.id.clone(), r.alignment.clone()))
            .collect()
    }
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
    if model.library.get(&form.cable).is_none() {
        return Err(format!(
            "{name}: cable \"{}\" is not in the wire library",
            form.cable
        ));
    }
    form.circuit = c.get(2);
    form.phase_label = c.get(3);
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

fn string_cells(form: &mut StringForm, c: &Cells, new: bool, name: &str) -> Result<(), String> {
    let n = |i: usize, default: f64| -> Result<f64, String> {
        Ok(opt_num(&c.get(i), &format!("{name} {}", STRINGS[i].0))?.unwrap_or(default))
    };
    form.label = c.get(2);
    for k in 0..3 {
        form.top[k] = n(3 + k, form.top[k])?;
        form.bottom[k] = n(6 + k, form.bottom[k])?;
    }
    if c.changed(9) || (new && !c.get(9).trim().is_empty()) {
        let len = num(&c.get(9), &format!("{name} length"))?;
        form.bottom = [form.top[0], form.top[1], form.top[2] - len.max(0.0)];
    }
    form.weight = n(10, form.weight)?;
    form.insulator = opt(&c.get(11));
    form.strain = match c.get(12).trim().to_lowercase().as_str() {
        "yes" | "true" | "1" => true,
        "no" | "false" | "0" | "" => false,
        other => return Err(format!("{name} strain: \"{other}\" is not yes or no")),
    };
    Ok(())
}

fn jumper_cells(
    j: &mut ols_line::Jumper,
    c: &Cells,
    model: &Model,
    name: &str,
) -> Result<(), String> {
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
    j.angle_from_deg = opt_num(&c.get(10), &format!("{name} angle from"))?;
    j.angle_to_deg = opt_num(&c.get(11), &format!("{name} angle to"))?;
    Ok(())
}

fn system_cells(s: &mut ols_line::electrical::System, c: &Cells, name: &str) -> Result<(), String> {
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
    Ok(())
}

/// The alignment points as edited: per route, the new list, one put each.
fn alignment_commands(
    model: &Model,
    stored: &ols_line::Line,
    rows: &[Row],
    page: &TablePage,
    out: &mut Changes,
) -> Result<(), String> {
    // (route, order, point) of every point kept or added.
    let mut points: BTreeMap<String, Vec<(f64, ols_line::AlignmentPoint)>> = BTreeMap::new();
    let touched: BTreeSet<String> = page
        .edits
        .keys()
        .chain(page.deleted.iter())
        .map(|k| k.split_once('\t').map_or(k.clone(), |x| x.0.to_owned()))
        .chain(page.added.iter().map(|r| r[0].trim().to_owned()))
        .collect();
    let point_of = |c: &Cells, name: &str| -> Result<ols_line::AlignmentPoint, String> {
        let structure = opt(&c.get(5));
        let position = match &structure {
            Some(s) if c.changed(5) || c.typed.is_none() => {
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
                let e = num(&c.get(2), &format!("{name} E"))?;
                let n = num(&c.get(3), &format!("{name} N"))?;
                let z = match opt_num(&c.get(4), &format!("{name} z"))? {
                    Some(z) => z,
                    None => model.ground_at(e, n).unwrap_or(0.0),
                };
                [e, n, z]
            }
        };
        Ok(ols_line::AlignmentPoint {
            position,
            structure,
        })
    };
    for row in rows {
        let (route, _) = row.key.split_once('\t').unwrap_or((&row.key, ""));
        if !touched.contains(route) || page.deleted.contains(&row.key) {
            continue;
        }
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        let name = format!("{route} #{}", row.cells[1]);
        let order = num(&c.get(1), &format!("{name} #"))?;
        points
            .entry(route.to_owned())
            .or_default()
            .push((order, point_of(&c, &name)?));
    }
    for (k, cells) in page.added.iter().enumerate() {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let route = c.get(0).trim().to_owned();
        if !alignments(stored).iter().any(|(r, _)| *r == route) {
            return Err(format!("new point {}: no route \"{route}\"", k + 1));
        }
        let name = format!("new point {}", k + 1);
        let order = opt_num(&c.get(1), &format!("{name} #"))?.unwrap_or(f64::MAX);
        points
            .entry(route.clone())
            .or_default()
            .push((order + 1e-9, point_of(&c, &name)?));
    }
    for route in touched {
        let mut list = points.remove(&route).unwrap_or_default();
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        let alignment: Vec<ols_line::AlignmentPoint> = list.into_iter().map(|x| x.1).collect();
        let command = if route == "line" {
            Command::Put {
                kind: Kind::Meta,
                id: "line".into(),
                value: serde_json::json!({
                    "name": stored.name,
                    "crs": stored.crs,
                    "alignment": alignment,
                    "display": stored.display,
                    "systems": stored.systems,
                }),
                ord: None,
            }
        } else {
            let mut r = stored
                .routes
                .iter()
                .find(|r| r.id == route)
                .ok_or_else(|| format!("no route {route}"))?
                .clone();
            r.alignment = alignment;
            Command::Put {
                kind: Kind::Route,
                id: route.clone(),
                value: serde_json::to_value(&r).map_err(|e| e.to_string())?,
                ord: None,
            }
        };
        out.commands
            .push((format!("changed the alignment of {route}"), command));
    }
    Ok(())
}

/// Weather cases: the model's case file with the changes written in
/// (comments and layout kept), checked; its path and new text.
fn weather_file(
    model: &Model,
    rows: &[Row],
    page: &TablePage,
) -> Result<(PathBuf, String), String> {
    let path = model
        .sources
        .weather
        .clone()
        .ok_or_else(|| "the model has no weather case file".to_owned())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
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
        let name = c.get(1);
        for (col, spec) in WEATHER.iter().enumerate().skip(1) {
            if !(all || c.changed(col)) {
                continue;
            }
            let v = c.get(col);
            let v = v.trim();
            let number = |unit: &str| -> Result<Option<toml_edit::Item>, String> {
                if v.is_empty() {
                    return Ok(None);
                }
                let x = num(v, &format!("{name} {}", spec.0))?;
                Ok(Some(toml_edit::value(format!("{x} {unit}"))))
            };
            let (field, item) = match col {
                1 if v.is_empty() => return Err("a case needs a name".into()),
                1 => ("name", Some(toml_edit::value(v))),
                2 => ("description", (!v.is_empty()).then(|| toml_edit::value(v))),
                3 => {
                    let mut a = toml_edit::Array::new();
                    for tag in v.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                        a.push(tag);
                    }
                    ("tags", (!a.is_empty()).then(|| toml_edit::value(a)))
                }
                4 if v.is_empty() => return Err(format!("{name}: a temperature is needed")),
                4 => ("temperature", number("°C")?),
                5 => ("ice_thickness", number("mm")?),
                6 => ("ice_load", number("N/m")?),
                7 => ("ice_density", number("kg/m³")?),
                8 => ("wind_pressure", number("Pa")?),
                9 => match v.to_lowercase().as_str() {
                    "" | "right" => ("wind_from", None),
                    "left" => ("wind_from", Some(toml_edit::value("left"))),
                    other => {
                        return Err(format!(
                            "{name} wind from: \"{other}\" — write left or right"
                        ));
                    }
                },
                _ => continue,
            };
            match item {
                Some(i) => {
                    t.insert(field, i);
                }
                None => {
                    t.remove(field);
                }
            }
        }
        Ok(())
    };
    for row in rows {
        if page.deleted.contains(&row.key) {
            continue;
        }
        let k: usize = row.key.parse().map_err(|_| "bad row".to_owned())?;
        let mut t = old
            .get(k)
            .cloned()
            .ok_or_else(|| format!("case {}: not in {}", row.cells[1], path.display()))?;
        let c = Cells {
            row: &row.cells,
            typed: page.edits.get(&row.key),
            defaulted: &row.defaulted,
        };
        write(&mut t, &c, false)?;
        cases.push((num(&c.get(0), &format!("{} #", row.cells[1]))?, t));
    }
    for cells in &page.added {
        let c = Cells {
            row: cells,
            typed: None,
            defaulted: &[],
        };
        let mut t = toml_edit::Table::new();
        write(&mut t, &c, true)?;
        let order = opt_num(&c.get(0), "new case #")?.unwrap_or(f64::MAX);
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
    ) -> Option<TableAction> {
        // Weather cases are a file of the model, not part of the project.
        let editable = match kind {
            TableKind::WeatherCases => model.sources.weather.is_some(),
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
                .add_enabled(editable, egui::Button::new("+ Add row"))
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
                "The deletions also take (one log entry, undo brings everything back):",
            );
            for t in takes {
                ui.label(egui::RichText::new(format!("  {t}")).small());
            }
            ui.horizontal(|ui| {
                if ui.button("Apply and delete all that").clicked() {
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
                    None => "The model uses a default case (15 °C): make a case file in Libraries › Weather cases and choose Use for the model".into(),
                })
                .small()
                .weak(),
            );
        }
        if kind == TableKind::Alignments {
            ui.label(
                egui::RichText::new("The profile view follows these points; a route without points follows its structures. # orders the points (3.5 goes between 3 and 4).")
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
                ui.colored_label(egui::Color32::LIGHT_RED, t);
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
        let mut order: Vec<usize> = (0..rows.len())
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
                    _ => x.cmp(&y),
                };
                if up { o } else { o.reverse() }
            });
        }
        let row_height = 20.0;
        let buttons = 52.0;
        let added = self.added.len();
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.add_space(buttons);
                for (c, col) in columns.iter().enumerate() {
                    let arrow = match self.sort {
                        Some((s, true)) if s == c => " ▲",
                        Some((s, false)) if s == c => " ▼",
                        _ => "",
                    };
                    let text = egui::RichText::new(format!("{}{arrow}", col.0)).strong();
                    if ui
                        .add_sized([col.1, row_height], egui::Button::new(text).frame(false))
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
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, order.len() + added, |ui, range| {
                    for i in range {
                        if i >= order.len() {
                            // A new row.
                            let k = i - order.len();
                            ui.horizontal(|ui| {
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
                                        ) {
                                            self.added[k][c] = text;
                                        }
                                        continue;
                                    }
                                    let edit = egui::TextEdit::singleline(&mut self.added[k][c])
                                        .text_color(egui::Color32::LIGHT_GREEN);
                                    ui.add_sized([col.1, row_height], edit);
                                }
                            });
                            continue;
                        }
                        let row = &rows[order[i]];
                        let deleted = self.deleted.contains(&row.key);
                        ui.horizontal(|ui| {
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
                            if ui
                                .add_enabled(
                                    editable,
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
                                    ) {
                                        self.set(&row.key, c, &text, original);
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
                    }
                });
        });
        action
    }
}
