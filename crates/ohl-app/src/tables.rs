// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use eframe::egui;
use ohl_line::{SectionPoint, Stringing, Structure};
use ohl_session::{Command, Kind};

use crate::editors::{SectionForm, StringForm, StructureForm};
use crate::model::Model;
use crate::selection::Selection;

mod changes;
mod columns;
mod library_tables;
mod page;
mod points_circuits;
mod rows;
mod span_objects;
use changes::*;
pub(crate) use columns::*;
pub(crate) use library_tables::*;
pub(crate) use page::*;
use points_circuits::*;
use span_objects::*;

/// A table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TableKind {
    /// Structures.
    Structures,
    /// Sections.
    Sections,
    /// Strings (attachment points).
    Strings,
    /// The string library: the insulator assemblies strings are labelled
    /// with, how many strings use each.
    Insulators,
    /// Jumpers.
    Jumpers,
    /// Electrical systems.
    Systems,
    /// Conductor paths, from the connections (key `circuits`, the name of
    /// before 2026-10-21).
    ConductorPaths,
    /// Circuits: the named groups of conductor paths
    /// ([`ohl_line::circuits`]).
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
    /// Interphase spacers (added with Add string between span points).
    Spacers,
    /// Span points, point loads and spread loads (added with the span
    /// tool).
    SpanObjects,
    /// The point and spread load types (the shipped templates, the
    /// project's own).
    LoadTypes,
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

/// A column a plugin adds to a table (`[[column]]` of its manifest), or
/// one of a plugin not in use whose values are on the objects (shown,
/// kept, not used).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginColumn {
    /// The table ([`TableKind::plugin_table`]).
    pub table: String,
    /// Key of its values on the objects: `plugin.column`.
    pub key: String,
    /// Header.
    pub title: String,
    /// What it holds.
    pub help: String,
    /// The values it is chosen from; none: typed.
    pub values: Vec<String>,
    /// Shown where not set.
    pub default: Option<String>,
    /// The plugin (name and version when in use).
    pub plugin: String,
    /// Its plugin is in use.
    pub in_use: bool,
}

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

/// A jumper end as the table writes it: "T3 2", or "tap S1 P4" (a span
/// point) / "tap J2 K" (a jumper's point).
fn end_text(e: &ohl_line::JumperEnd) -> String {
    match e {
        ohl_line::JumperEnd::Wire(p) => point(p),
        ohl_line::JumperEnd::Tap { .. } => e.to_string(),
    }
}

/// "T3 2" → a wire point; "tap S1 P4" → a tap on span point P4 of
/// section S1, "tap J2 K" on point K of jumper J2.
fn parse_end(text: &str, line: &ohl_line::Line) -> Result<ohl_line::JumperEnd, String> {
    let Some(rest) = text.trim().strip_prefix("tap ") else {
        return parse_point(text).map(ohl_line::JumperEnd::Wire);
    };
    let (host, label) = rest.trim().split_once(' ').ok_or_else(|| {
        format!("\"{text}\": write tap, a section or jumper, a point, e.g. tap S1 P4")
    })?;
    let (host, point) = (host.trim().to_owned(), label.trim().to_owned());
    if line.sections.iter().any(|s| s.id == host) {
        Ok(ohl_line::JumperEnd::tap(ohl_line::Tap::Span(
            ohl_line::SpanPointRef {
                section: host,
                point,
            },
        )))
    } else if line.jumpers.iter().any(|j| j.id == host) {
        Ok(ohl_line::JumperEnd::tap(ohl_line::Tap::Jumper {
            jumper: host,
            point,
        }))
    } else {
        Err(format!("\"{text}\": no section or jumper {host}"))
    }
}

/// "T3 2" → a section point (files written before: "T3 2:1", the set
/// [`ohl_line::assembly::set_from_v1`]).
fn parse_point(text: &str) -> Result<SectionPoint, String> {
    let t = text.trim();
    let (s, sp) = t
        .rsplit_once(' ')
        .ok_or_else(|| format!("\"{t}\": write structure and set, e.g. T3 2"))?;
    let set = match parse_set_phase(sp) {
        Ok((set, phase)) => ohl_line::assembly::set_from_v1(set, phase),
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
    /// Insulator library files to write (path, text) or delete (path,
    /// None), before the commands.
    pub library: Vec<(PathBuf, Option<String>)>,
    /// A library file of the project to write (kind, name, text).
    pub project_library: Option<(String, String, String)>,
    /// Sections' display weather cases (none: the case shown).
    pub display: Vec<(String, Vec<usize>)>,
    /// What deletions take with them, to confirm.
    pub takes: Vec<String>,
}

impl TableKind {
    /// Every table (the Tables menu).
    pub const ALL: [TableKind; 15] = [
        TableKind::Structures,
        TableKind::Sections,
        TableKind::Strings,
        TableKind::Insulators,
        TableKind::Jumpers,
        TableKind::Systems,
        TableKind::ConductorPaths,
        TableKind::Circuits,
        TableKind::Routes,
        TableKind::Alignments,
        TableKind::AlignmentPoints,
        TableKind::WeatherCases,
        TableKind::Spacers,
        TableKind::SpanObjects,
        TableKind::LoadTypes,
    ];

    /// Title of its page.
    pub fn title(self) -> &'static str {
        match self {
            TableKind::Structures => "Structures",
            TableKind::Sections => "Sections",
            TableKind::Strings => "Strings",
            TableKind::Insulators => "String library",
            TableKind::Jumpers => "Jumpers",
            TableKind::Systems => "Electrical systems",
            TableKind::ConductorPaths => "Conductor paths",
            TableKind::Circuits => "Circuits",
            TableKind::Alignments => "Alignments",
            TableKind::AlignmentPoints => "Alignment points",
            TableKind::Routes => "Routes",
            TableKind::WeatherCases => "Weather cases",
            TableKind::Spacers => "Spacers",
            TableKind::LoadTypes => "Point and spread load types",
            TableKind::SpanObjects => "Span points and loads",
        }
    }

    /// Key for saving.
    pub fn key(self) -> &'static str {
        match self {
            TableKind::Structures => "structures",
            TableKind::Sections => "sections",
            TableKind::Strings => "strings",
            TableKind::Insulators => "insulators",
            TableKind::Jumpers => "jumpers",
            TableKind::Systems => "systems",
            TableKind::ConductorPaths => "circuits",
            TableKind::Circuits => "circuit_list",
            TableKind::Alignments => "alignment_list",
            TableKind::AlignmentPoints => "alignments",
            TableKind::Routes => "routes",
            TableKind::WeatherCases => "weather",
            TableKind::Spacers => "spacers",
            TableKind::LoadTypes => "load_types",
            TableKind::SpanObjects => "span_objects",
        }
    }

    /// Its columns.
    pub fn columns(self) -> &'static [Column] {
        match self {
            TableKind::Structures => STRUCTURES,
            TableKind::Sections => SECTIONS,
            TableKind::Strings => STRINGS,
            TableKind::Insulators => INSULATORS,
            TableKind::Jumpers => JUMPERS,
            TableKind::Systems => SYSTEMS,
            TableKind::ConductorPaths => CIRCUITS,
            TableKind::Circuits => CIRCUIT_LIST,
            TableKind::Alignments => ALIGNMENTS,
            TableKind::AlignmentPoints => POINTS,
            TableKind::Routes => ROUTES,
            TableKind::WeatherCases => WEATHER,
            TableKind::Spacers => SPACERS,
            TableKind::LoadTypes => LOAD_TYPES,
            TableKind::SpanObjects => SPAN_OBJECTS,
        }
    }

    /// The column called `name`, if it has one.
    pub fn column_of(self, name: &str) -> Option<usize> {
        self.columns().iter().position(|c| c.0 == name)
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
            (TableKind::ConductorPaths | TableKind::Circuits, _) => None,
            (TableKind::Structures, "routes") => Some("none (from the alignments)"),
            (_, "routes") => None,
            _ => Some("none (the cases shown)"),
        }
    }

    /// Its rows follow the model (the circuits, from the connections):
    /// none added or deleted here.
    pub fn fixed_rows(self) -> bool {
        self == TableKind::ConductorPaths
    }

    /// Rows can be added here (not those added with a tool in the views).
    pub fn adds_rows(self) -> bool {
        !self.fixed_rows() && !matches!(self, TableKind::Spacers | TableKind::SpanObjects)
    }

    pub fn rows(self, model: &Model, case: usize) -> Vec<Row> {
        let mut rows = self.own_rows(model, case);
        let columns = self.plugin_columns(model);
        if columns.is_empty() {
            return rows;
        }
        let n = self.columns().len();
        let line = &model.line;
        let circuits =
            (self == TableKind::ConductorPaths).then(|| ohl_line::electrical::circuits(line));
        for r in &mut rows {
            let objects = self.fields_of(line, circuits.as_deref(), &r.key);
            r.defaulted.resize(n, false);
            for c in &columns {
                // Not set: the default (a circuit's sections may differ).
                let fallback = c.default.as_deref().unwrap_or("");
                let mut v: Vec<&str> = objects
                    .iter()
                    .map(|f| f.get(&c.key).map_or(fallback, String::as_str))
                    .collect();
                v.sort_unstable();
                v.dedup();
                let unset = objects.iter().all(|f| !f.contains_key(&c.key));
                let (text, default) = match &v[..] {
                    [] => (fallback.to_owned(), c.default.is_some()),
                    [one] => ((*one).to_owned(), unset && c.default.is_some()),
                    many => (format!("mixed: {}", many.join(" / ")), false),
                };
                r.cells.push(text);
                r.defaulted.push(default);
            }
        }
        rows
    }

    /// The table's name for plugin columns ([`ohl_plugins::COLUMN_TABLES`]);
    /// none: it takes none.
    pub fn plugin_table(self) -> Option<&'static str> {
        Some(match self {
            TableKind::Structures => "structures",
            TableKind::Sections => "sections",
            TableKind::ConductorPaths => "circuits",
            TableKind::Systems => "systems",
            TableKind::Jumpers => "jumpers",
            TableKind::Alignments => "alignments",
            TableKind::Routes => "routes",
            _ => return None,
        })
    }

    /// Its plugin columns, after its own: those the plugins in use add,
    /// then the values found on its objects of plugins not in use (shown,
    /// unused).
    pub fn plugin_columns(self, model: &Model) -> Vec<PluginColumn> {
        let Some(table) = self.plugin_table() else {
            return Vec::new();
        };
        let mut out: Vec<PluginColumn> = model
            .plugin_columns
            .iter()
            .filter(|c| c.table == table)
            .cloned()
            .collect();
        let line = &model.line;
        let mut found = BTreeSet::new();
        let mut take = |f: &ohl_line::Fields| found.extend(f.keys().cloned());
        match self {
            TableKind::Structures => line.structures.iter().for_each(|s| take(&s.fields)),
            TableKind::Sections | TableKind::ConductorPaths => {
                line.sections.iter().for_each(|s| take(&s.fields));
            }
            TableKind::Systems => line.systems.iter().for_each(|s| take(&s.fields)),
            TableKind::Jumpers => line.jumpers.iter().for_each(|j| take(&j.fields)),
            TableKind::Alignments => line.alignments.iter().for_each(|a| take(&a.fields)),
            TableKind::Routes => line.routes.iter().for_each(|r| take(&r.fields)),
            _ => {}
        }
        for key in found {
            if model.plugin_columns.iter().any(|c| c.key == key) {
                // Its plugin shows it (in this table or another).
                continue;
            }
            let plugin = key.split_once('.').map_or(key.as_str(), |x| x.0).to_owned();
            out.push(PluginColumn {
                table: table.to_owned(),
                title: key.clone(),
                help: format!(
                    "added by plugin {plugin}, not in use here: the values are kept, not used"
                ),
                key,
                values: Vec::new(),
                default: None,
                plugin,
                in_use: false,
            });
        }
        out
    }

    /// The plugin values of the object(s) of row `key`: a circuit's are its
    /// sections'.
    fn fields_of<'a>(
        self,
        line: &'a ohl_line::Line,
        circuits: Option<&[ohl_line::electrical::Circuit]>,
        key: &str,
    ) -> Vec<&'a ohl_line::Fields> {
        let one = match self {
            TableKind::Structures => line.structure(key).map(|s| &s.fields),
            TableKind::Sections => line
                .sections
                .iter()
                .find(|s| s.id == key)
                .map(|s| &s.fields),
            TableKind::Systems => line.systems.iter().find(|s| s.id == key).map(|s| &s.fields),
            TableKind::Jumpers => line.jumpers.iter().find(|j| j.id == key).map(|j| &j.fields),
            TableKind::Alignments => line
                .alignments
                .iter()
                .find(|a| a.id == key)
                .map(|a| &a.fields),
            TableKind::Routes => line.routes.iter().find(|r| r.id == key).map(|r| &r.fields),
            TableKind::ConductorPaths => {
                let Some(c) = circuits
                    .unwrap_or_default()
                    .iter()
                    .find(|c| circuit_key(c) == key)
                else {
                    return Vec::new();
                };
                return c
                    .sections
                    .iter()
                    .filter_map(|id| line.sections.iter().find(|s| &s.id == id))
                    .map(|s| &s.fields)
                    .collect();
            }
            _ => None,
        };
        one.into_iter().collect()
    }
}
