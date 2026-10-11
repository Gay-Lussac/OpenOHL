// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Report tables of a solved line (structures, sections, spans per
//! weather case, stationing), shared by the table formats (CSV, XLSX).

use std::fmt::Write as _;

use ols_core::text::csv_field;
use ols_line::Stringing;

use crate::{ExportModel, Exporter, Options};

/// One cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// Text.
    Text(String),
    /// A number.
    Number(f64),
    /// Nothing.
    Empty,
}

impl From<&str> for Cell {
    fn from(t: &str) -> Self {
        Cell::Text(t.to_owned())
    }
}

impl From<String> for Cell {
    fn from(t: String) -> Self {
        Cell::Text(t)
    }
}

impl From<f64> for Cell {
    fn from(x: f64) -> Self {
        Cell::Number(x)
    }
}

impl From<usize> for Cell {
    fn from(x: usize) -> Self {
        Cell::Number(x as f64)
    }
}

impl From<u32> for Cell {
    fn from(x: u32) -> Self {
        Cell::Number(f64::from(x))
    }
}

impl<T: Into<Cell>> From<Option<T>> for Cell {
    fn from(x: Option<T>) -> Self {
        x.map_or(Cell::Empty, Into::into)
    }
}

/// A table: a name, column headers, rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    /// Name (sheet name).
    pub name: String,
    /// Column headers.
    pub columns: Vec<String>,
    /// Rows (as many cells as columns, or fewer).
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    /// An empty table with these columns.
    pub fn new(name: &str, columns: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
            rows: Vec::new(),
        }
    }

    /// The table as CSV (numbers in full precision).
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        let header: Vec<String> = self.columns.iter().map(|c| csv_field(c)).collect();
        let _ = writeln!(out, "{}", header.join(","));
        for row in &self.rows {
            let cells: Vec<String> = row
                .iter()
                .map(|c| match c {
                    Cell::Text(t) => csv_field(t),
                    Cell::Number(x) => format!("{x}"),
                    Cell::Empty => String::new(),
                })
                .collect();
            let _ = writeln!(out, "{}", cells.join(","));
        }
        out
    }

    /// A table from CSV text (first row: headers); cells that read as
    /// numbers become numbers.
    pub fn from_csv(name: &str, text: &str) -> Self {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let columns = lines
            .next()
            .map(crate::formats::csv_fields)
            .unwrap_or_default();
        let rows = lines
            .map(|l| {
                crate::formats::csv_fields(l)
                    .into_iter()
                    .map(|f| match f.parse::<f64>() {
                        Ok(x) if x.is_finite() => Cell::Number(x),
                        _ if f.is_empty() => Cell::Empty,
                        _ => Cell::Text(f),
                    })
                    .collect()
            })
            .collect();
        Self {
            name: name.to_owned(),
            columns,
            rows,
        }
    }
}

/// Structures: position, orientation, type, attachments, routes.
pub fn structures(model: &ExportModel) -> Table {
    let line = &model.line;
    let mut t = Table::new(
        "Structures",
        &[
            "structure",
            "x",
            "y",
            "z",
            "orientation_deg",
            "type",
            "extension",
            "sets",
            "routes",
        ],
    );
    for s in &line.structures {
        t.rows.push(vec![
            s.id.as_str().into(),
            s.position[0].into(),
            s.position[1].into(),
            s.position[2].into(),
            s.orientation_deg.into(),
            s.structure_type.as_deref().into(),
            s.extension.as_deref().into(),
            s.sets().count().into(),
            line.routes_of(&s.id).join(" ").into(),
        ]);
    }
    t
}

/// Sections: cable, circuit, phase, system, bundle, points, stringing.
pub fn sections(model: &ExportModel) -> Table {
    let mut t = Table::new(
        "Sections",
        &[
            "section",
            "cable",
            "circuit",
            "phase",
            "system",
            "bundle",
            "spans",
            "from",
            "to",
            "stringing",
            "horizontal_tension_n",
            "sag_m",
            "temperature_c",
            "calculation",
        ],
    );
    for s in &model.line.sections {
        let (kind, tension, sag) = match &s.stringing {
            Stringing::Tension {
                horizontal_tension, ..
            } => ("tension", Some(*horizontal_tension), None),
            Stringing::Sag { sag, .. } => ("sag", None, Some(*sag)),
            Stringing::UnstressedLengths { .. } => ("clipped in", None, None),
        };
        t.rows.push(vec![
            s.id.as_str().into(),
            s.cable.as_str().into(),
            s.circuit.as_str().into(),
            s.phase_label.as_str().into(),
            s.system.as_deref().into(),
            s.bundle.into(),
            s.points.len().saturating_sub(1).into(),
            s.points.first().map(|p| p.structure.as_str()).into(),
            s.points.last().map(|p| p.structure.as_str()).into(),
            kind.into(),
            tension.into(),
            sag.into(),
            s.stringing.temperature().into(),
            s.calculation.label().into(),
        ]);
    }
    t
}

/// Spans (and jumpers) of every weather case: tensions and sag.
pub fn spans(model: &ExportModel) -> Table {
    let mut t = Table::new(
        "Spans",
        &[
            "case",
            "section",
            "span",
            "back",
            "ahead",
            "circuit",
            "phase",
            "horizontal_span_m",
            "horizontal_tension_n",
            "max_tension_n",
            "sag_m",
        ],
    );
    for case in &model.cases {
        for w in &case.wires {
            let points = model
                .line
                .sections
                .iter()
                .find(|s| s.id == w.section)
                .map(|s| &s.points);
            let at = |k: usize| points.and_then(|p| p.get(k)).map(|p| p.structure.as_str());
            let (a, b) = (w.points.first(), w.points.last());
            let span = a
                .zip(b)
                .map_or(0.0, |(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]));
            t.rows.push(vec![
                case.name.as_str().into(),
                w.section.as_str().into(),
                (w.span + 1).into(),
                at(w.span).into(),
                at(w.span + 1).into(),
                w.circuit.as_str().into(),
                w.phase.as_str().into(),
                span.into(),
                w.horizontal_tension.into(),
                w.max_tension.into(),
                w.sag.into(),
            ]);
        }
    }
    t
}

/// Stationing of every route: station, offset, line angle, span ahead.
pub fn stations(model: &ExportModel) -> Table {
    let line = &model.line;
    let mut t = Table::new(
        "Stations",
        &[
            "route",
            "structure",
            "station_m",
            "offset_m",
            "line_angle_deg",
            "ahead_span_m",
        ],
    );
    for route in line.alignment_views() {
        for s in route.stations(line).unwrap_or_default() {
            t.rows.push(vec![
                route.id.as_str().into(),
                s.structure.into(),
                s.station.into(),
                s.offset.into(),
                s.line_angle_deg.into(),
                s.ahead_span.into(),
            ]);
        }
    }
    t
}

/// Every table of a model: structures, sections, spans, stations and the
/// model's further tables.
pub fn all(model: &ExportModel) -> Vec<Table> {
    let mut v = vec![
        structures(model),
        sections(model),
        spans(model),
        stations(model),
    ];
    v.extend(model.tables.iter().cloned());
    v
}

/// Spans of every weather case as CSV ([`spans`]).
pub struct SpansCsv;

impl Exporter for SpansCsv {
    fn id(&self) -> &str {
        "spans-csv"
    }
    fn description(&self) -> &str {
        "Spans and jumpers of every weather case: tensions, sag (CSV)"
    }
    fn extension(&self) -> &str {
        "csv"
    }
    fn export(&mut self, model: &ExportModel, _: &Options) -> Result<Vec<u8>, String> {
        Ok(spans(model).to_csv().into_bytes())
    }
}
