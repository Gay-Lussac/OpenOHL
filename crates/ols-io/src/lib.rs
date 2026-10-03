// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Import and export (DESIGN_GOALS §8, ADR-0003).
//!
//! Every file format is an [`Exporter`] (a solved model → a file) or an
//! [`Importer`] (a file → a line), listed in a [`Registry`]. Built-in
//! formats implement the traits in Rust ([`formats`]); WebAssembly plugins
//! implement the `io-plugin` WIT world and are wrapped by [`plugin`] — to
//! the rest of OLS CAD both look the same.
//!
//! An exporter receives an [`ExportModel`]: the line, its coordinate system
//! and, for each weather case, the solved wires. Plugins receive it as JSON
//! ([`ExportModel::to_json`], format `ols-export-model` version 1).

pub mod formats;
pub mod plugin;

use std::collections::BTreeMap;

use ols_line::Line;
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;

/// Options given to a format, as key/value pairs (`--option key=value`).
pub type Options = BTreeMap<String, String>;

/// One wire of a solved model: a span of a section, or a jumper.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportWire {
    /// Section id, or `jumper:<id>`.
    pub section: String,
    /// Span index (0 for jumpers).
    pub span: usize,
    /// Circuit.
    pub circuit: String,
    /// Phase label.
    pub phase: String,
    /// Display colour `#rrggbb`.
    pub color: String,
    /// Electrical system id (none: earthed wire).
    pub system: Option<String>,
    /// Nominal voltage of the system, kV (none: earthed wire).
    pub voltage: Option<f64>,
    /// Points in the line's coordinate system.
    pub points: Vec<[f64; 3]>,
}

/// The wires of one weather case.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportCase {
    /// Case name.
    pub name: String,
    /// One-line description (temperature, ice, wind).
    pub summary: String,
    /// Wires.
    pub wires: Vec<ExportWire>,
}

/// What exporters receive.
#[derive(Debug, Clone)]
pub struct ExportModel {
    /// The line.
    pub line: Line,
    /// Solved weather cases (may be empty: geometry only).
    pub cases: Vec<ExportCase>,
}

impl ExportModel {
    /// The model of a line and its solved cases (name, summary, sections,
    /// jumpers).
    pub fn new(
        line: &Line,
        cases: &[(String, String, Vec<SectionShape>, Vec<JumperShape>)],
    ) -> Self {
        // A valid line has its systems resolved; else wires count as earthed.
        let phases = line.section_phases().unwrap_or_default();
        let system = |id: &str| -> (Option<String>, Option<f64>) {
            match phases.get(id).copied().flatten() {
                Some((k, _)) => (
                    Some(line.systems[k].id.clone()),
                    Some(line.systems[k].voltage_kv),
                ),
                None => (None, None),
            }
        };
        let cases = cases
            .iter()
            .map(|(name, summary, shapes, jumpers)| {
                let mut wires = Vec::new();
                for shape in shapes {
                    let section = line.sections.iter().find(|s| s.id == shape.section);
                    let color = section.map_or(ols_line::NEUTRAL_COLOR.to_owned(), |s| {
                        line.section_color(s)
                    });
                    for (k, span) in shape.spans.iter().enumerate() {
                        wires.push(ExportWire {
                            section: shape.section.clone(),
                            span: k,
                            circuit: section.map_or(String::new(), |s| s.circuit.clone()),
                            phase: section.map_or(String::new(), |s| s.phase_label.clone()),
                            color: color.clone(),
                            system: system(&shape.section).0,
                            voltage: system(&shape.section).1,
                            points: span.points.clone(),
                        });
                    }
                }
                for j in jumpers {
                    let def = line.jumpers.iter().find(|d| d.id == j.jumper);
                    let from =
                        def.and_then(|d| line.sections.iter().find(|s| s.points.contains(&d.from)));
                    wires.push(ExportWire {
                        section: format!("jumper:{}", j.jumper),
                        span: 0,
                        circuit: from.map_or(String::new(), |s| s.circuit.clone()),
                        phase: from.map_or(String::new(), |s| s.phase_label.clone()),
                        color: def
                            .and_then(|d| d.color.clone())
                            .or_else(|| from.map(|s| line.section_color(s)))
                            .unwrap_or_else(|| ols_line::NEUTRAL_COLOR.to_owned()),
                        system: from.and_then(|s| system(&s.id).0),
                        voltage: from.and_then(|s| system(&s.id).1),
                        points: j.points.clone(),
                    });
                }
                ExportCase {
                    name: name.clone(),
                    summary: summary.clone(),
                    wires,
                }
            })
            .collect();
        Self {
            line: line.clone(),
            cases,
        }
    }

    /// The model as JSON for plugins:
    /// `{"format": "ols-export-model", "version": 1, "crs": …, "line": <line
    /// file>, "cases": [{"name", "summary", "wires": [{"section", "span",
    /// "circuit", "phase", "color", "system", "voltage_kv", "points":
    /// [[x, y, z], …]}]}]}`.
    pub fn to_json(&self) -> String {
        let line = serde_json::to_value(&self.line).expect("the line serialises");
        let cases: Vec<serde_json::Value> = self
            .cases
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "summary": c.summary,
                    "wires": c.wires.iter().map(|w| serde_json::json!({
                        "section": w.section,
                        "span": w.span,
                        "circuit": w.circuit,
                        "phase": w.phase,
                        "color": w.color,
                        "system": w.system,
                        "voltage_kv": w.voltage,
                        "points": w.points,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        serde_json::json!({
            "format": "ols-export-model",
            "version": 1,
            "crs": self.line.crs,
            "line": line,
            "cases": cases,
        })
        .to_string()
    }
}

/// Writes a model to a file format.
pub trait Exporter {
    /// Unique id, e.g. `"kml"`.
    fn id(&self) -> &str;
    /// One-line description.
    fn description(&self) -> &str;
    /// File extension, without the dot.
    fn extension(&self) -> &str;
    /// Where it comes from (`built-in` or the plugin's name).
    fn origin(&self) -> String {
        "built-in".into()
    }
    /// The file's bytes.
    fn export(&mut self, model: &ExportModel, options: &Options) -> Result<Vec<u8>, String>;
}

/// Reads a file format into a line.
pub trait Importer {
    /// Unique id, e.g. `"structures-csv"`.
    fn id(&self) -> &str;
    /// One-line description.
    fn description(&self) -> &str;
    /// File extension, without the dot.
    fn extension(&self) -> &str;
    /// Where it comes from.
    fn origin(&self) -> String {
        "built-in".into()
    }
    /// The line read from `data`; `base` is a line to update, if any.
    fn import(
        &mut self,
        data: &[u8],
        base: Option<&Line>,
        options: &Options,
    ) -> Result<Line, String>;
}

/// The formats available.
#[derive(Default)]
pub struct Registry {
    exporters: Vec<Box<dyn Exporter>>,
    importers: Vec<Box<dyn Importer>>,
}

impl Registry {
    /// The built-in formats.
    pub fn with_builtin() -> Self {
        let mut r = Self::default();
        formats::register(&mut r);
        r
    }

    /// Adds an exporter (a later one with the same id replaces it).
    pub fn add_exporter(&mut self, e: Box<dyn Exporter>) {
        self.exporters.retain(|x| x.id() != e.id());
        self.exporters.push(e);
    }

    /// Adds an importer (a later one with the same id replaces it).
    pub fn add_importer(&mut self, i: Box<dyn Importer>) {
        self.importers.retain(|x| x.id() != i.id());
        self.importers.push(i);
    }

    /// The formats of a file-format plugin directory.
    pub fn add_plugin(
        &mut self,
        host: &ols_plugins::WasmHost,
        dir: &std::path::Path,
    ) -> Result<String, String> {
        plugin::register(self, host, dir)
    }

    /// Exporters.
    pub fn exporters(&self) -> impl Iterator<Item = &dyn Exporter> {
        self.exporters.iter().map(|e| e.as_ref())
    }

    /// Importers.
    pub fn importers(&self) -> impl Iterator<Item = &dyn Importer> {
        self.importers.iter().map(|i| i.as_ref())
    }

    /// The exporter with this id, or for this file extension.
    pub fn exporter(&mut self, id_or_extension: &str) -> Option<&mut Box<dyn Exporter>> {
        let key = id_or_extension.to_ascii_lowercase();
        let k = self
            .exporters
            .iter()
            .position(|e| e.id() == key)
            .or_else(|| self.exporters.iter().position(|e| e.extension() == key))?;
        self.exporters.get_mut(k)
    }

    /// The importer with this id, or for this file extension.
    pub fn importer(&mut self, id_or_extension: &str) -> Option<&mut Box<dyn Importer>> {
        let key = id_or_extension.to_ascii_lowercase();
        let k = self
            .importers
            .iter()
            .position(|e| e.id() == key)
            .or_else(|| self.importers.iter().position(|e| e.extension() == key))?;
        self.importers.get_mut(k)
    }
}

#[cfg(test)]
mod tests;
