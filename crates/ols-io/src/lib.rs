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

pub mod checks;
pub mod codes;
pub mod formats;
pub mod geopackage;
pub mod gis;
pub mod ifc;
pub mod landxml;
pub mod neutral;
pub mod olsxml;
pub mod plugin;
pub mod projects;
pub mod shapefile;
pub mod surfaces;
pub mod tables;
pub mod xlsx;
pub mod xml;

use std::collections::BTreeMap;

use ols_line::Line;
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;

/// Options given to a format, as key/value pairs (`--option key=value`).
pub type Options = BTreeMap<String, String>;

/// One `KEY=VALUE` option (both trimmed).
pub fn parse_option(entry: &str) -> Result<(String, String), String> {
    entry
        .split_once('=')
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| format!("option \"{}\": use KEY=VALUE", entry.trim()))
}

/// `KEY=VALUE` options separated by commas or line breaks (empty entries
/// skipped).
pub fn parse_options(text: &str) -> Result<Options, String> {
    text.split([',', '\n'])
        .filter(|e| !e.trim().is_empty())
        .map(parse_option)
        .collect()
}

/// The format of a file to read: `format` if given, else its extension
/// (lower case).
pub fn format_of(path: &std::path::Path, format: Option<&str>) -> String {
    match format.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => f.to_owned(),
        None => path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase(),
    }
}

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
    /// Horizontal tension, N (per wire; none for jumpers).
    pub horizontal_tension: Option<f64>,
    /// Largest tension in the span, N (per wire).
    pub max_tension: Option<f64>,
    /// Sag, m (largest vertical distance below the chord).
    pub sag: Option<f64>,
}

/// A string (insulator) as solved in a case, with its limits — which
/// OLS CAD does not check: a plugin compares them with the force and the
/// swing.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportString {
    /// The structure it hangs on.
    pub structure: String,
    /// Its label on the structure.
    pub label: String,
    /// The section that reported it (`jumper:<id>` for a jumper's
    /// support string); a string shared by sections (a TAK) is listed once,
    /// as the first reported it.
    pub by: String,
    /// Axial force, N (the whole bundle's; tension positive).
    pub force: f64,
    /// Its swing as solved (degrees, in its structure's axes).
    pub swing: ols_line::Swing,
    /// Its limits (its own, else its insulator's; absent: not given).
    pub limits: ols_line::StringLimits,
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
    /// The structures' strings as solved (finite-element sections and
    /// jumper supports; none from the ruling span).
    pub strings: Vec<ExportString>,
}

/// What exporters receive.
#[derive(Debug, Clone)]
pub struct ExportModel {
    /// The line.
    pub line: Line,
    /// Solved weather cases (may be empty: geometry only).
    pub cases: Vec<ExportCase>,
    /// Further report tables (structure loads, usage…) for table formats
    /// such as XLSX; empty unless the program solving the line adds them.
    pub tables: Vec<tables::Table>,
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
                            circuit: section.map_or(String::new(), |s| s.circuit_name.clone()),
                            phase: section.map_or(String::new(), |s| s.phase_label.clone()),
                            color: color.clone(),
                            system: system(&shape.section).0,
                            voltage: system(&shape.section).1,
                            points: span.points.clone(),
                            horizontal_tension: Some(span.horizontal_tension),
                            max_tension: Some(span.max_tension),
                            sag: Some(span.sag),
                        });
                    }
                }
                for j in jumpers {
                    let def = line.jumpers.iter().find(|d| d.id == j.jumper);
                    let from = def.and_then(|d| line.jumper_section(d));
                    wires.push(ExportWire {
                        section: format!("jumper:{}", j.jumper),
                        span: 0,
                        circuit: from.map_or(String::new(), |s| s.circuit_name.clone()),
                        phase: from.map_or(String::new(), |s| s.phase_label.clone()),
                        color: def
                            .and_then(|d| d.color.clone())
                            .or_else(|| from.map(|s| line.section_color(s)))
                            .unwrap_or_else(|| ols_line::NEUTRAL_COLOR.to_owned()),
                        system: from.and_then(|s| system(&s.id).0),
                        voltage: from.and_then(|s| system(&s.id).1),
                        points: j.points.clone(),
                        horizontal_tension: None,
                        max_tension: Some(j.max_tension),
                        sag: Some(j.sag),
                    });
                }
                let mut strings: Vec<ExportString> = Vec::new();
                let mut push = |x: ExportString| {
                    if !strings
                        .iter()
                        .any(|y| y.structure == x.structure && y.label == x.label)
                    {
                        strings.push(x);
                    }
                };
                for shape in shapes {
                    let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
                        continue;
                    };
                    for (point, states) in section.points.iter().zip(&shape.strings) {
                        for x in states {
                            push(ExportString {
                                structure: point.structure.clone(),
                                label: x.label.clone(),
                                by: section.id.clone(),
                                force: x.force,
                                swing: x.swing,
                                limits: x.limits,
                            });
                        }
                    }
                }
                for j in jumpers {
                    for support in j.supports.iter().filter(|s| !s.structure.is_empty()) {
                        for x in &support.strings {
                            push(ExportString {
                                structure: support.structure.clone(),
                                label: x.label.clone(),
                                by: format!("jumper:{}", j.jumper),
                                force: x.force,
                                swing: x.swing,
                                limits: x.limits,
                            });
                        }
                    }
                }
                ExportCase {
                    name: name.clone(),
                    summary: summary.clone(),
                    wires,
                    strings,
                }
            })
            .collect();
        Self {
            line: line.clone(),
            cases,
            tables: Vec::new(),
        }
    }

    /// The model as JSON for plugins:
    /// `{"format": "ols-export-model", "version": 1, "crs": …, "line": <line
    /// file>, "cases": [{"name", "summary", "wires": [{"section", "span",
    /// "circuit", "phase", "color", "system", "voltage_kv",
    /// "horizontal_tension_n", "max_tension_n", "sag_m", "points":
    /// [[x, y, z], …]}], "strings": [{"structure", "label", "by",
    /// "force_n", "swing_deg", "swing_across_deg", "swing_along_deg",
    /// "max_load_n", "max_swing_deg", "max_swing_across_deg",
    /// "max_swing_along_deg"}]}]}` — a string's `max_*` are null when not
    /// given (OLS CAD checks none of them). The line's strings carry the
    /// same limits (`max_load`, `max_swing_deg`, …, structure types
    /// placed and insulators' limits taken).
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
                        "horizontal_tension_n": w.horizontal_tension,
                        "max_tension_n": w.max_tension,
                        "sag_m": w.sag,
                        "points": w.points,
                    })).collect::<Vec<_>>(),
                    "strings": c.strings.iter().map(|x| serde_json::json!({
                        "structure": x.structure,
                        "label": x.label,
                        "by": x.by,
                        "force_n": x.force,
                        "swing_deg": x.swing.total_deg,
                        "swing_across_deg": x.swing.across_deg,
                        "swing_along_deg": x.swing.along_deg,
                        "max_load_n": x.limits.max_load,
                        "max_swing_deg": x.limits.max_swing_deg,
                        "max_swing_across_deg": x.limits.max_swing_across_deg,
                        "max_swing_along_deg": x.limits.max_swing_along_deg,
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
    surface_importers: Vec<Box<dyn surfaces::SurfaceImporter>>,
    project_importers: Vec<Box<dyn projects::ProjectImporter>>,
}

impl Registry {
    /// The built-in formats.
    pub fn with_builtin() -> Self {
        let mut r = Self::default();
        formats::register(&mut r);
        surfaces::register(&mut r);
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

    /// Adds a terrain surface reader (a later one with the same id
    /// replaces it).
    pub fn add_surface_importer(&mut self, i: Box<dyn surfaces::SurfaceImporter>) {
        self.surface_importers.retain(|x| x.id() != i.id());
        self.surface_importers.push(i);
    }

    /// Adds a project reader (a later one with the same id replaces it).
    pub fn add_project_importer(&mut self, i: Box<dyn projects::ProjectImporter>) {
        self.project_importers.retain(|x| x.id() != i.id());
        self.project_importers.push(i);
    }

    /// Project readers (from plugins).
    pub fn project_importers(&self) -> impl Iterator<Item = &dyn projects::ProjectImporter> {
        self.project_importers.iter().map(|i| i.as_ref())
    }

    /// The project reader with this id, or for this file extension.
    pub fn project_importer(
        &mut self,
        id_or_extension: &str,
    ) -> Option<&mut Box<dyn projects::ProjectImporter>> {
        let key = id_or_extension.to_ascii_lowercase();
        let k = self
            .project_importers
            .iter()
            .position(|e| e.id() == key)
            .or_else(|| {
                self.project_importers
                    .iter()
                    .position(|e| e.extension() == key)
            })?;
        self.project_importers.get_mut(k)
    }

    /// Terrain surface readers.
    pub fn surface_importers(&self) -> impl Iterator<Item = &dyn surfaces::SurfaceImporter> {
        self.surface_importers.iter().map(|i| i.as_ref())
    }

    /// The surface reader with this id, or for this file extension.
    pub fn surface_importer(
        &mut self,
        id_or_extension: &str,
    ) -> Option<&mut Box<dyn surfaces::SurfaceImporter>> {
        let key = id_or_extension.to_ascii_lowercase();
        let k = self
            .surface_importers
            .iter()
            .position(|e| e.id() == key)
            .or_else(|| {
                self.surface_importers
                    .iter()
                    .position(|e| e.extension() == key)
            })?;
        self.surface_importers.get_mut(k)
    }

    /// The formats of a plugin directory: a file-format plugin (extension
    /// point `io`), a terrain plugin (`terrain`), or both.
    pub fn add_plugin(
        &mut self,
        host: &ols_plugins::WasmHost,
        dir: &std::path::Path,
    ) -> Result<String, String> {
        let manifest = ols_plugins::Manifest::load(dir).map_err(|e| e.to_string())?;
        let points = &manifest.plugin.extension_points;
        let mut name = None;
        if points.iter().any(|p| p == "io") {
            name = Some(plugin::register(self, host, dir)?);
        }
        if points.iter().any(|p| p == "terrain") {
            name = Some(surfaces::register_plugin(self, host, dir)?);
        }
        if points.iter().any(|p| p == "project") {
            name = Some(projects::register_plugin(self, host, dir)?);
        }
        name.ok_or_else(|| {
            format!(
                "plugin {}: no format extension point (io, terrain or project)",
                manifest.plugin.name
            )
        })
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
mod format_tests;
#[cfg(test)]
mod tests;
