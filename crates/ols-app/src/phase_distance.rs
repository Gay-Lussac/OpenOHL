// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › LeV / OLEI › Distance between conductors: the still-air distance
//! between the conductors of each span by a phase distance code
//! (`.olspd`, [`ols_clearance::phase_distance`]), in the weather cases
//! ticked (each on its own), for the circuits ticked; the insulation
//! levels from the Electrical circuits table (the LeV plugin's column).
//! Computed on demand in the background; worst pairs first; a click
//! selects the span and draws the worst station's line with d and z.
//!
//! A run is saved in the library of performed checks
//! ([`ols_session::reports`]) with its parameters (the values file's text
//! included) and the pairs below z or within [`KEPT_WITHIN`] of it;
//! reopened, it shows whether the line changed since; run again, the new
//! run is compared with it and may replace it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use eframe::egui;
use ols_clearance::phase_distance::{
    Conductor, LEVEL_FIELD, Level, PhaseDistanceCode, PhaseDistanceSettings, check, conductors,
};
use ols_session::reports::{self, CheckReport, LineState, ReportStatus};
use serde::{Deserialize, Serialize};

use crate::job::{Job, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// A worst station within this of a conductor's clamp is marked "at a
/// clamp" (f about 0), m.
const NEAR_CLAMP: f64 = 2.0;

/// The check's id in the library of performed checks.
pub const CHECK_ID: &str = "phase-distance";

/// The check in words.
pub const CHECK_TITLE: &str = "Distance between conductors (LeV)";

/// A saved report keeps the pairs below z and those within this of it,
/// m (all of them are counted).
pub const KEPT_WITHIN: f64 = 1.0;

/// One pair of conductors in one weather case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// Weather case.
    pub case: String,
    /// Each conductor's span (section, span index).
    pub span: [(String, usize); 2],
    /// Each conductor's circuit and phase, in words.
    pub what: [String; 2],
    /// Same circuit, or towards an earth wire: C₁.
    pub same_circuit: bool,
    /// Between the same two structures (else a nearby pair).
    pub same_span: bool,
    /// Nominal voltage used, kV.
    pub kv: f64,
    /// Distance at the worst station, m.
    pub distance: f64,
    /// Required z there, m.
    pub required: f64,
    /// Conductor below z, m.
    pub length_below: f64,
    /// The worst station's points.
    pub at: [[f64; 3]; 2],
    /// Where along each conductor, m.
    pub along: [f64; 2],
    /// The worst station is within [`NEAR_CLAMP`] of a clamp.
    pub at_clamp: bool,
    /// Local sag of each, m.
    pub sag: [f64; 2],
    /// l_k of each, m.
    pub lk: [f64; 2],
    /// System angle, degrees.
    pub beta: f64,
    /// k of each.
    pub k: [f64; 2],
    /// C after the altitude, m.
    pub c: f64,
}

impl Row {
    /// d − z, m.
    pub fn margin(&self) -> f64 {
        self.distance - self.required
    }

    /// The pair in its case, to compare runs.
    fn key(&self) -> (String, [(String, usize); 2]) {
        (self.case.clone(), self.span.clone())
    }
}

/// A weather case as checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseParams {
    /// Its name.
    pub name: String,
    /// Wire temperature, °C.
    pub temperature_c: f64,
    /// Wind pressure, Pa.
    pub wind_pressure_pa: f64,
    /// Ice thickness, m.
    pub ice_thickness_m: f64,
    /// Ice load given, N/m.
    pub ice_load_n_m: Option<f64>,
}

/// Every parameter of a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Params {
    /// The values file's name.
    pub values_file: String,
    /// Its text, as used.
    pub values_text: String,
    /// SHA-256 of the text.
    pub values_sha256: String,
    /// The k diagram (index).
    pub diagram: usize,
    /// Its name.
    pub diagram_name: String,
    /// Station spacing, m.
    pub spacing: f64,
    /// Nearby spans too.
    pub nearby: bool,
    /// Their distance, m.
    pub max_distance: f64,
    /// Circuits ticked (none: all).
    pub circuits: Option<Vec<String>>,
    /// The weather cases.
    pub cases: Vec<CaseParams>,
    /// The insulation level used for each circuit.
    pub levels: BTreeMap<String, String>,
}

/// What a saved report keeps as results.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// Pairs checked.
    pub pairs: usize,
    /// Pairs below z.
    pub below: usize,
    /// Conductor below z, all pairs, m.
    pub length_below_m: f64,
    /// The code's name.
    pub code: String,
    /// The notes of the run.
    pub notes: Vec<String>,
    /// The pairs below z or within [`KEPT_WITHIN`] of it, worst first.
    pub rows: Vec<Row>,
}

/// What a check found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The code's name.
    pub code: String,
    /// The diagram used.
    pub diagram: String,
    /// Notes (cases not solved, sections not solved).
    pub notes: Vec<String>,
    /// Pairs of every case, worst first.
    pub rows: Vec<Row>,
    /// Pairs checked (a saved report keeps only some rows).
    pub pairs: usize,
    /// Its parameters.
    pub params: Params,
    /// The state of the line it ran on (a project).
    pub state: Option<LineState>,
}

impl Report {
    /// Pairs below z, and conductor below z (m).
    fn below(&self) -> (usize, f64) {
        let b: Vec<&Row> = self.rows.iter().filter(|r| r.margin() < 0.0).collect();
        (b.len(), b.iter().map(|r| r.length_below).sum())
    }
}

/// A pair compared with the saved report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Below z now, not before.
    New,
    /// Below z before, not now.
    Fixed,
    /// Below z both times, worse now (by more than 1 cm).
    Worse,
    /// Below z both times, better now.
    Better,
}

impl Change {
    fn text(self) -> &'static str {
        match self {
            Change::New => "new",
            Change::Fixed => "fixed",
            Change::Worse => "worse",
            Change::Better => "better",
        }
    }
}

/// A pair, in its case.
type PairKey = (String, [(String, usize); 2]);

/// A run compared with a saved report: each pair's change, and the saved
/// pairs below z that are no more (their spans gone).
#[derive(Debug, Clone, Default)]
pub struct Comparison {
    /// By pair.
    pub changes: BTreeMap<PairKey, Change>,
    /// Saved pairs below z not found now.
    pub gone: usize,
}

impl Comparison {
    /// Compares a new run with the saved rows.
    pub fn of(new: &[Row], saved: &[Row]) -> Self {
        let old: BTreeMap<PairKey, &Row> = saved.iter().map(|r| (r.key(), r)).collect();
        let mut out = Comparison::default();
        for r in new {
            let before = old.get(&r.key()).map(|o| o.margin());
            let now = r.margin();
            let change = match before {
                Some(b) if b < 0.0 && now >= 0.0 => Some(Change::Fixed),
                Some(b) if b < 0.0 && now < b - 0.01 => Some(Change::Worse),
                Some(b) if b < 0.0 && now > b + 0.01 => Some(Change::Better),
                Some(b) if b < 0.0 => None,
                _ if now < 0.0 => Some(Change::New),
                _ => None,
            };
            if let Some(c) = change {
                out.changes.insert(r.key(), c);
            }
        }
        let now: BTreeSet<PairKey> = new.iter().map(Row::key).collect();
        out.gone = saved
            .iter()
            .filter(|r| r.margin() < 0.0 && !now.contains(&r.key()))
            .count();
        out
    }

    /// Counts in words.
    pub fn summary(&self) -> String {
        let n = |c: Change| self.changes.values().filter(|x| **x == c).count();
        let mut s = format!(
            "{} new, {} fixed, {} worse, {} better",
            n(Change::New),
            n(Change::Fixed),
            n(Change::Worse),
            n(Change::Better)
        );
        if self.gone > 0 {
            s.push_str(&format!(", {} gone (their spans are no more)", self.gone));
        }
        s
    }
}

/// A question waiting in the window.
#[derive(Debug, Clone, PartialEq)]
enum Ask {
    /// A report of this name exists: replace it?
    Overwrite(String),
    /// Run again: replace the saved report with this run?
    Replace,
}

/// A line to draw: the worst station of a pair, with its label.
pub type Measure = ([f64; 3], [f64; 3], String);

/// What the window asks of the application.
#[derive(Default)]
pub struct Out {
    /// A span to go to.
    pub go: Option<Selection>,
    /// The line to draw (d and z), replacing the last.
    pub measure: Option<Measure>,
    /// A command to apply (a report saved).
    pub apply: Option<(String, ols_session::Command)>,
    /// The weather case to show (that of the result clicked), by name.
    pub case: Option<String>,
}

/// The Distance between conductors window.
pub struct PhaseDistanceWindow {
    /// Shown.
    pub open: bool,
    /// The code's values file.
    pub file: PathBuf,
    /// Weather cases ticked (indices in the model's cases).
    pub cases: BTreeSet<usize>,
    /// Circuits ticked (none: all).
    pub circuits: Option<BTreeSet<String>>,
    /// The settings (circuits: from `circuits`).
    pub settings: PhaseDistanceSettings,
    /// Only pairs below z.
    pub only_below: bool,
    /// The row clicked.
    pub picked: Option<usize>,
    /// The values of a saved report (file name, text), used instead of
    /// the file until another is chosen.
    pub values: Option<(String, String)>,
    /// The saved report shown or run again.
    pub loaded: Option<CheckReport>,
    /// The name to save under.
    pub save_name: String,
    /// The last run compared with the saved report.
    pub comparison: Option<Comparison>,
    ask: Option<Ask>,
    compared: bool,
    save_error: Option<String>,
    /// The file was chosen (else the project's copy is used, if any).
    chosen: bool,
    /// The values being edited (an admin).
    pub(crate) editing: Option<PhaseDistanceCode>,
    values_error: Option<String>,
    /// The line's state against the saved report's, for this line version.
    since: Option<(u64, String, bool)>,
    job: Job<Report>,
}

impl Default for PhaseDistanceWindow {
    fn default() -> Self {
        Self {
            open: false,
            file: ols_core::files::shipped_library("phase-distance").join("lev-ch.olspd"),
            cases: BTreeSet::new(),
            circuits: None,
            settings: PhaseDistanceSettings::default(),
            only_below: true,
            picked: None,
            values: None,
            loaded: None,
            save_name: String::new(),
            comparison: None,
            ask: None,
            compared: true,
            save_error: None,
            chosen: false,
            editing: None,
            values_error: None,
            since: None,
            job: Job::default(),
        }
    }
}

/// The still-air cases at 10 °C (the LeV's f), else none.
fn still_air_10(model: &Model) -> BTreeSet<usize> {
    model
        .cases
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let c = &c.case;
            (c.temperature.celsius() - 10.0).abs() < 0.01
                && c.wind_pressure.si() == 0.0
                && c.ice_thickness.si() == 0.0
                && c.ice_load.is_none_or(|l| l.si() == 0.0)
        })
        .map(|(k, _)| k)
        .collect()
}

/// The circuits of the line (names of its sections' circuits), sorted.
fn circuit_names(model: &Model) -> Vec<String> {
    let mut v: Vec<String> = model
        .line
        .sections
        .iter()
        .map(|s| s.circuit.clone())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The insulation level each circuit is checked at: its sections'
/// (the Electrical circuits table's column), else high.
fn levels(model: &Model) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for s in &model.line.sections {
        let level = s
            .fields
            .get(LEVEL_FIELD)
            .and_then(|v| Level::parse(v))
            .unwrap_or_default();
        let text = match level {
            Level::Low => "low",
            Level::High => "high",
        };
        // Sections of one circuit set differently: high wins.
        let e = out
            .entry(s.circuit.clone())
            .or_insert_with(|| text.to_owned());
        if text == "high" {
            *e = text.to_owned();
        }
    }
    out
}

/// Where the values come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// A saved report's.
    Report,
    /// The project's copy (an admin's edit).
    Project,
    /// A file.
    File,
}

/// Kind of the project's library files holding edited values.
const VALUES_KIND: &str = "phase-distance";

/// The project's copy of the values file `name`, if an admin made one.
fn project_values(model: &Model, name: &str) -> Option<String> {
    let s = &model.editing.as_ref().ok()?.session;
    s.library_files(VALUES_KIND)
        .ok()?
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, t)| t)
}

/// The user is an admin of the project.
fn is_admin(model: &Model) -> bool {
    model.editing.as_ref().is_ok_and(|e| {
        e.session
            .role(&e.user)
            .is_ok_and(|r| r == Some(ols_session::Role::Admin))
    })
}

/// Keeps `code` as the project's copy of values file `name` (admins).
fn save_values(model: &Model, name: &str, code: &PhaseDistanceCode) -> Result<(), String> {
    let e = model
        .editing
        .as_ref()
        .map_err(|e| format!("read-only: {e}"))?;
    if !is_admin(model) {
        return Err("only an admin of the project may change the values".into());
    }
    let time = e.session.now().map_err(|e| e.to_string())?;
    let text = code.to_text(&format!(
        "Edited in OLS CAD by {} on {time} (the project's copy of {name}).",
        e.user
    ));
    // Read back: refused when it does not hold.
    PhaseDistanceCode::parse(&text)?;
    e.session
        .put_library_file(&e.user, VALUES_KIND, name, &text)
        .map_err(|e| e.to_string())
}

/// Removes the project's copy of values file `name` (admins).
fn reset_values(model: &Model, name: &str) -> Result<(), String> {
    let e = model
        .editing
        .as_ref()
        .map_err(|e| format!("read-only: {e}"))?;
    if !is_admin(model) {
        return Err("only an admin of the project may change the values".into());
    }
    e.session
        .delete_library_file(&e.user, VALUES_KIND, name)
        .map_err(|e| e.to_string())
}

/// The values as tables: C₁/C₂ by voltage, each diagram's k lines, the
/// altitude rule; typed in when `edit`.
fn code_table(ui: &mut egui::Ui, code: &mut PhaseDistanceCode, edit: bool) {
    let num = |ui: &mut egui::Ui, v: &mut f64, speed: f64, decimals: usize| {
        if edit {
            ui.add(
                egui::DragValue::new(v)
                    .speed(speed)
                    .min_decimals(decimals)
                    .max_decimals(decimals + 2),
            );
        } else {
            ui.label(egui::RichText::new(format!("{v:.decimals$}")).monospace());
        }
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("C by nominal voltage, m (Table 2.4.1)").strong());
            egui::Grid::new("phase-distance-voltages")
                .striped(true)
                .spacing([10.0, 2.0])
                .show(ui, |ui| {
                    for h in ["up to kV", "C₁ low", "C₁ high", "C₂ low", "C₂ high"] {
                        ui.label(egui::RichText::new(h).small().strong());
                    }
                    ui.end_row();
                    for r in &mut code.voltages {
                        num(ui, &mut r.kv, 1.0, 0);
                        for v in r.c1.iter_mut().chain(r.c2.iter_mut()) {
                            num(ui, v, 0.01, 2);
                        }
                        ui.end_row();
                    }
                });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Altitude: from").small());
                if code.altitude_from.is_finite() {
                    num(ui, &mut code.altitude_from, 10.0, 0);
                } else {
                    ui.label("—");
                }
                ui.label(egui::RichText::new("m, + share per 1000 m").small());
                num(ui, &mut code.altitude_per_1000, 0.01, 2);
            });
        });
        ui.separator();
        for (i, d) in code.diagrams.iter_mut().enumerate() {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "k, diagram {} ({:.0} Pa)",
                        d.name, d.wind_pressure
                    ))
                    .strong(),
                );
                egui::Grid::new(("phase-distance-diagram-lines", i))
                    .striped(true)
                    .spacing([10.0, 2.0])
                    .show(ui, |ui| {
                        for h in ["w = d/q up to", "k at β 0°", "k at β 90°"] {
                            ui.label(egui::RichText::new(h).small().strong());
                        }
                        ui.end_row();
                        for l in &mut d.lines {
                            match &mut l.w_max {
                                Some(w) => num(ui, w, 0.1, 1),
                                None => {
                                    ui.label("above");
                                }
                            }
                            num(ui, &mut l.k0, 0.01, 2);
                            num(ui, &mut l.k90, 0.01, 2);
                            ui.end_row();
                        }
                    });
            });
        }
    });
}

/// The plugin that sets the insulation levels, with its version, when in
/// use.
fn plugin_of(model: &Model) -> Option<String> {
    model
        .plugin_columns
        .iter()
        .find(|c| c.in_use && c.key == LEVEL_FIELD)
        .map(|c| c.plugin.clone())
}

impl PhaseDistanceWindow {
    /// Opens the window: the still-air 10 °C cases ticked when none is.
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        self.cases.retain(|k| *k < model.cases.len());
        if self.cases.is_empty() {
            self.cases = still_air_10(model);
        }
    }

    /// The last check's report, and whether one runs.
    #[cfg(test)]
    pub fn report(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// Starts the check in the background (cases not solved yet are
    /// solved there).
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        if self.cases.is_empty() {
            self.job.result = Some(Err("tick a weather case".into()));
            return;
        }
        self.picked = None;
        self.comparison = None;
        self.ask = None;
        self.compared = self.loaded.is_none();
        let (values_file, text) = match self.current_values(model) {
            Ok((name, text, _)) => (name, text),
            Err(e) => {
                self.job.result = Some(Err(e));
                return;
            }
        };
        let mut params = Params {
            values_sha256: ols_session::versions::hash(&text),
            values_file,
            values_text: text,
            diagram: self.settings.diagram,
            diagram_name: String::new(),
            spacing: self.settings.spacing,
            nearby: self.settings.nearby,
            max_distance: self.settings.max_distance,
            circuits: self.circuits.as_ref().map(|c| c.iter().cloned().collect()),
            cases: self
                .cases
                .iter()
                .filter_map(|k| model.cases.get(*k))
                .map(|c| CaseParams {
                    name: c.case.name.clone(),
                    temperature_c: c.case.temperature.celsius(),
                    wind_pressure_pa: c.case.wind_pressure.si(),
                    ice_thickness_m: c.case.ice_thickness.si(),
                    ice_load_n_m: c.case.ice_load.map(|l| l.si()),
                })
                .collect(),
            levels: levels(model),
        };
        let state = model
            .editing
            .as_ref()
            .ok()
            .and_then(|e| e.session.check_state().ok());
        let line = model.line.clone();
        let library = model.library.clone();
        let wind = model.wind;
        let cases: Vec<(crate::model::Solved, bool)> = self
            .cases
            .iter()
            .filter_map(|k| model.cases.get(*k))
            .map(|c| (c.clone(), c.solved_for(wind)))
            .collect();
        let mut settings = self.settings.clone();
        settings.circuits = self.circuits.clone();
        self.job.start(ctx, move || {
            let code = PhaseDistanceCode::parse(&params.values_text)
                .map_err(|e| format!("{}: {e}", params.values_file))?;
            let diagram = code
                .diagrams
                .get(settings.diagram)
                .map_or_else(String::new, |d| d.name.clone());
            let mut notes = Vec::new();
            let mut rows = Vec::new();
            for (solved, ready) in cases {
                let solved = if ready {
                    solved
                } else {
                    crate::model::Solved::with_wind(&line, &library, solved.case.clone(), wind)
                };
                let name = solved.case.name.clone();
                notes.extend(
                    solved
                        .failed
                        .iter()
                        .map(|f| format!("\"{name}\": not solved: {f}")),
                );
                let cs = conductors(&line, &solved.shapes, &library);
                // Exit clause: about a second for a 25 km line.
                let budget =
                    ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(300));
                let pairs = check(&code, &settings, &cs, &budget)?;
                let length = |c: &Conductor| {
                    c.wire
                        .points
                        .windows(2)
                        .map(|w| {
                            (0..3)
                                .map(|i| (w[1][i] - w[0][i]).powi(2))
                                .sum::<f64>()
                                .sqrt()
                        })
                        .sum::<f64>()
                };
                let what = |c: &Conductor| {
                    if c.wire.potential.is_none() {
                        format!("{} (earth wire)", c.wire.circuit)
                    } else {
                        format!("{} {}", c.wire.circuit, c.wire.phase)
                    }
                };
                for p in pairs {
                    let (a, b) = (&cs[p.a], &cs[p.b]);
                    let w = &p.worst;
                    let near = |c: &Conductor, s: f64| s < NEAR_CLAMP || length(c) - s < NEAR_CLAMP;
                    rows.push(Row {
                        case: name.clone(),
                        span: [
                            (a.wire.section.clone(), a.wire.span),
                            (b.wire.section.clone(), b.wire.span),
                        ],
                        what: [what(a), what(b)],
                        same_circuit: p.same_circuit,
                        same_span: p.same_span,
                        kv: p.kv,
                        distance: w.distance,
                        required: w.required,
                        length_below: p.length_below,
                        at: w.at,
                        along: w.along,
                        at_clamp: near(a, w.along[0]) || near(b, w.along[1]),
                        sag: w.sag,
                        lk: w.lk,
                        beta: w.beta,
                        k: w.k,
                        c: w.c,
                    });
                }
            }
            rows.sort_by(|a, b| a.margin().total_cmp(&b.margin()));
            params.diagram_name = diagram.clone();
            Ok(Report {
                code: code.name,
                diagram,
                notes,
                pairs: rows.len(),
                rows,
                params,
                state,
            })
        });
    }

    /// Shows saved report `r`: its parameters set (its values used until
    /// another file is chosen), its results shown; Check runs it again.
    pub fn load(&mut self, r: &CheckReport, model: &Model) -> Result<(), String> {
        if r.check != CHECK_ID {
            return Err(format!("{} is not a {CHECK_TITLE} report", r.id));
        }
        let params: Params = serde_json::from_value(r.parameters.clone())
            .map_err(|e| format!("report {}: parameters: {e}", r.id))?;
        let saved: Saved = serde_json::from_value(r.results.clone())
            .map_err(|e| format!("report {}: results: {e}", r.id))?;
        let mut notes = saved.notes.clone();
        self.cases.clear();
        for c in &params.cases {
            match model.cases.iter().position(|m| m.case.name == c.name) {
                Some(k) => {
                    let m = &model.cases[k].case;
                    if (m.temperature.celsius() - c.temperature_c).abs() > 1e-9
                        || (m.wind_pressure.si() - c.wind_pressure_pa).abs() > 1e-9
                        || (m.ice_thickness.si() - c.ice_thickness_m).abs() > 1e-12
                    {
                        notes.push(format!(
                            "weather case \"{}\" has other values now than in the report",
                            c.name
                        ));
                    }
                    self.cases.insert(k);
                }
                None => notes.push(format!(
                    "weather case \"{}\" of the report is not in the model",
                    c.name
                )),
            }
        }
        self.settings.diagram = params.diagram;
        self.settings.spacing = params.spacing;
        self.settings.nearby = params.nearby;
        self.settings.max_distance = params.max_distance;
        self.circuits = params.circuits.clone().map(|c| c.into_iter().collect());
        self.values = Some((params.values_file.clone(), params.values_text.clone()));
        self.save_name = r.id.clone();
        self.picked = None;
        self.comparison = None;
        self.ask = None;
        self.compared = true;
        self.job.result = Some(Ok(Report {
            code: saved.code.clone(),
            diagram: params.diagram_name.clone(),
            notes,
            rows: saved.rows.clone(),
            pairs: saved.pairs,
            params,
            state: Some(r.state.clone()),
        }));
        self.loaded = Some(r.clone());
        self.only_below = true;
        self.open = true;
        Ok(())
    }

    /// The last run as a report named `name` (status draft; a report it
    /// replaces keeps its note).
    fn to_check_report(&self, model: &Model, name: &str) -> Result<CheckReport, String> {
        let Some(Ok(r)) = &self.job.result else {
            return Err("run the check first".into());
        };
        let editing = model
            .editing
            .as_ref()
            .map_err(|e| format!("read-only: {e}"))?;
        let state = r
            .state
            .clone()
            .ok_or("the run was not made on a project: open the project and run it again")?;
        let (below, length) = r.below();
        let saved = Saved {
            pairs: r.pairs,
            below,
            length_below_m: length,
            code: r.code.clone(),
            notes: r.notes.clone(),
            rows: r
                .rows
                .iter()
                .filter(|x| x.margin() < KEPT_WITHIN)
                .cloned()
                .collect(),
        };
        let note = editing
            .session
            .report(name)
            .map(|o| o.note)
            .unwrap_or_default();
        Ok(CheckReport {
            format: reports::REPORT_FILE_FORMAT.into(),
            version: reports::REPORT_FILE_VERSION,
            id: name.to_owned(),
            check: CHECK_ID.into(),
            title: CHECK_TITLE.into(),
            time: editing.session.now().map_err(|e| e.to_string())?,
            user: editing.user.clone(),
            app: format!("OLS CAD {}", env!("CARGO_PKG_VERSION")),
            plugin: plugin_of(model),
            status: ReportStatus::Draft,
            note,
            state,
            summary: format!(
                "{} pairs, {below} below z ({length:.0} m of conductor)",
                r.pairs
            ),
            parameters: serde_json::to_value(&r.params).map_err(|e| e.to_string())?,
            results: serde_json::to_value(&saved).map_err(|e| e.to_string())?,
        })
    }

    /// Saves the last run as report `name` (asked first when one of that
    /// name exists and is not the one run again).
    fn save(
        &mut self,
        model: &Model,
        name: &str,
        confirmed: bool,
    ) -> Option<(String, ols_session::Command)> {
        let name = name.trim();
        if name.is_empty() {
            self.ask = None;
            self.save_error = Some("give the report a name".into());
            return None;
        }
        let exists = model
            .editing
            .as_ref()
            .ok()
            .is_some_and(|e| e.session.report(name).is_some());
        if exists && !confirmed {
            self.ask = Some(Ask::Overwrite(name.to_owned()));
            return None;
        }
        match self.to_check_report(model, name) {
            Ok(r) => {
                self.ask = None;
                self.comparison = None;
                let what = format!("saved check report {name}");
                let command = r.put();
                // The run shown is the saved report now.
                if let Some(Ok(rep)) = &mut self.job.result {
                    rep.rows.retain(|x| x.margin() < KEPT_WITHIN);
                }
                self.loaded = Some(r);
                Some((what, command))
            }
            Err(e) => {
                self.ask = None;
                self.save_error = Some(e);
                None
            }
        }
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) -> Out {
        self.job.poll();
        // A run again of a saved report: compared with it, then asked.
        if !self.compared
            && !self.job.running()
            && let (Some(Ok(new)), Some(old)) = (&self.job.result, &self.loaded)
        {
            self.compared = true;
            if let Ok(saved) = serde_json::from_value::<Saved>(old.results.clone()) {
                self.comparison = Some(Comparison::of(&new.rows, &saved.rows));
                self.ask = Some(Ask::Replace);
            }
        }
        let mut out = Out::default();
        let mut open = self.open;
        egui::Window::new("Distance between conductors (LeV)")
            .open(&mut open)
            .default_width(980.0)
            .show(ctx, |ui| {
                self.settings_ui(ui, model);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.job.running(), egui::Button::new("Check"))
                        .clicked()
                    {
                        self.start(model, ui.ctx());
                    }
                    self.job.status_ui(ui);
                });
                self.library_ui(ui, model, &mut out);
                ui.separator();
                // How it computes and with which values: open, in a box of
                // its own height so the results keep their place.
                egui::ScrollArea::vertical()
                    .id_salt("phase-distance-how-and-values")
                    .max_height(260.0)
                    .show(ui, |ui| {
                        explanation(ui);
                        self.values_ui(ui, model);
                    });
                ui.separator();
                self.results_ui(ui, &mut out);
            });
        self.open = open;
        out
    }

    /// The saved report shown (its line's state now), the questions, and
    /// saving the last run.
    fn library_ui(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        let session = model.editing.as_ref().ok().map(|e| &e.session);
        if let Some(r) = &self.loaded {
            if self
                .since
                .as_ref()
                .is_none_or(|s| s.0 != model.line_version)
            {
                let now = session.and_then(|s| s.check_state().ok());
                self.since = Some(match now {
                    Some(now) => {
                        let c = reports::changed_since(&r.state, &now);
                        (model.line_version, c.summary(), c.is_empty())
                    }
                    None => (model.line_version, "not a project".into(), false),
                });
            }
            let (_, since, same) = self.since.clone().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Saved report \"{}\" — {} by {}, {}:",
                        r.id,
                        r.time,
                        r.user,
                        r.status.as_str()
                    ))
                    .strong(),
                );
                let t = egui::RichText::new(if same {
                    since
                } else {
                    format!("stale — {since}")
                });
                ui.label(if same {
                    t.color(egui::Color32::LIGHT_GREEN)
                } else {
                    t.color(egui::Color32::YELLOW)
                });
                ui.label(
                    egui::RichText::new("Check runs it again with its parameters")
                        .small()
                        .weak(),
                );
            });
        }
        match self.ask.clone() {
            Some(Ask::Replace) => {
                let (name, summary) = (
                    self.loaded.as_ref().map_or(String::new(), |r| r.id.clone()),
                    self.comparison
                        .as_ref()
                        .map_or(String::new(), Comparison::summary),
                );
                crate::menu::message(
                    ui,
                    format!("Compared with the saved report: {summary}."),
                    Some(egui::Color32::YELLOW),
                );
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Replace the saved report \"{name}\" with this run?"
                    ));
                    if ui.button("Replace").clicked() {
                        out.apply = self.save(model, &name, true);
                    }
                    if ui.button("Keep the saved one").clicked() {
                        self.ask = None;
                    }
                });
            }
            Some(Ask::Overwrite(name)) => {
                ui.horizontal(|ui| {
                    crate::menu::message(
                        ui,
                        format!("A report \"{name}\" exists: replace it with this run?"),
                        Some(egui::Color32::YELLOW),
                    );
                    if ui.button("Replace").clicked() {
                        out.apply = self.save(model, &name, true);
                    }
                    if ui.button("Cancel").clicked() {
                        self.ask = None;
                    }
                });
            }
            None => {}
        }
        let ran = matches!(self.job.result, Some(Ok(_))) && !self.job.running();
        ui.horizontal(|ui| {
            if self.save_name.is_empty() {
                self.save_name = CHECK_TITLE.into();
            }
            ui.label("Save in the library of performed checks as");
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(240.0));
            let why = match (ran, session) {
                (false, _) => Some("run the check first"),
                (_, None) => Some("open a project to keep reports"),
                _ => None,
            };
            let r = ui.add_enabled(
                why.is_none() && self.ask.is_none(),
                egui::Button::new("Save"),
            );
            let r = match why {
                Some(w) => r.on_disabled_hover_text(w),
                None => r.on_hover_text(
                    "Kept with this line's branch: its parameters, the pairs below z and \
                     within 1 m of it, the state of the line",
                ),
            };
            if r.clicked() {
                let name = self.save_name.clone();
                out.apply = self.save(model, &name, false);
            }
        });
        if let Some(e) = &self.save_error {
            crate::menu::error_label(ui, e);
        }
        if out.apply.is_some() {
            self.save_error = None;
            self.since = None;
        }
    }

    /// The values used: a saved report's, else the file chosen, else the
    /// project's copy (edited by an admin), else the shipped file: (file
    /// name, text, where from).
    fn current_values(&self, model: &Model) -> Result<(String, String, Source), String> {
        if let Some((name, text)) = &self.values {
            return Ok((name.clone(), text.clone(), Source::Report));
        }
        let name = self
            .file
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if !self.chosen
            && let Some(text) = project_values(model, &name)
        {
            return Ok((name, text, Source::Project));
        }
        std::fs::read_to_string(&self.file)
            .map(|t| (name, t, Source::File))
            .map_err(|e| format!("{}: {e}", self.file.display()))
    }

    /// The values as a table (read only; an admin may edit them: kept in
    /// the project).
    fn values_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let current = self.current_values(model);
        let admin = is_admin(model);
        egui::CollapsingHeader::new("Values used")
            .id_salt("phase-distance-values")
            .default_open(true)
            .show(ui, |ui| {
                let (name, text, source) = match &current {
                    Ok(c) => c.clone(),
                    Err(e) => {
                        crate::menu::error_label(ui, e);
                        return;
                    }
                };
                let code = match PhaseDistanceCode::parse(&text) {
                    Ok(c) => c,
                    Err(e) => {
                        crate::menu::error_label(ui, format!("{name}: {e}"));
                        return;
                    }
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} — {name}, {}",
                            code.name,
                            match source {
                                Source::Report => "as saved in the report",
                                Source::Project => "the project's copy (edited by an admin)",
                                Source::File => "the file",
                            }
                        ))
                        .small(),
                    );
                    if self.editing.is_none() {
                        let b = ui.add_enabled(admin, egui::Button::new("Edit values"));
                        let b = if admin {
                            b.on_hover_text(
                                "Change the values; saved as the project's copy, used by every \
                                 run on this project",
                            )
                        } else {
                            b.on_disabled_hover_text(
                                "Locked: only an admin of the project may edit them",
                            )
                        };
                        if b.clicked() {
                            self.editing = Some(code.clone());
                            self.values_error = None;
                        }
                        if admin
                            && source == Source::Project
                            && ui
                                .button("Back to the file")
                                .on_hover_text("Remove the project's copy: the file's values again")
                                .clicked()
                        {
                            self.values_error = reset_values(model, &name).err();
                        }
                    }
                });
                let editing = self.editing.is_some();
                let mut code = self.editing.take().unwrap_or(code);
                let mut keep = editing;
                if editing {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Editing the values (admin):")
                                .color(egui::Color32::YELLOW),
                        );
                        if ui.button("Save in the project").clicked() {
                            match save_values(model, &name, &code) {
                                Ok(()) => {
                                    keep = false;
                                    self.values = None;
                                    self.chosen = false;
                                    self.values_error = None;
                                }
                                Err(e) => self.values_error = Some(e),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                            self.values_error = None;
                        }
                    });
                }
                code_table(ui, &mut code, editing);
                if keep {
                    self.editing = Some(code);
                }
                if let Some(e) = &self.values_error {
                    crate::menu::error_label(ui, e);
                }
            });
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let code = self
            .current_values(model)
            .ok()
            .and_then(|(_, t, _)| PhaseDistanceCode::parse(&t).ok());
        ui.horizontal(|ui| {
            if ui
                .button("Choose a values file…")
                .on_hover_text("Another .olspd file instead of the values used")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Phase distance code", &["olspd"])
                    .set_directory(ols_core::files::shipped_library("phase-distance"))
                    .pick_file()
            {
                self.file = p;
                self.values = None;
                self.chosen = true;
            }
            match &code {
                Some(c) => {
                    ui.label("k diagram");
                    let shown = c
                        .diagrams
                        .get(self.settings.diagram)
                        .map_or("—".to_owned(), |d| d.name.clone());
                    egui::ComboBox::from_id_salt("phase-distance-diagram")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (k, d) in c.diagrams.iter().enumerate() {
                                ui.selectable_value(&mut self.settings.diagram, k, &d.name);
                            }
                        });
                }
                None => {
                    crate::menu::error_label(ui, "the values file does not read");
                }
            }
        });
        ui.horizontal(|ui| {
            let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
            let title = match self.cases.len() {
                0 => "Weather cases: none".to_owned(),
                1 => format!(
                    "Weather case: {}",
                    names
                        .get(*self.cases.first().expect("one"))
                        .map_or("", String::as_str)
                ),
                n => format!("Weather cases: {n}"),
            };
            crate::menu::options_menu(ui, &title, |ui| {
                ui.label(
                    egui::RichText::new("each checked on its own (LeV: f at 10 °C, still air)")
                        .small()
                        .weak(),
                );
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (k, n) in names.iter().enumerate() {
                            let mut on = self.cases.contains(&k);
                            if ui.checkbox(&mut on, n).changed() {
                                if on {
                                    self.cases.insert(k);
                                } else {
                                    self.cases.remove(&k);
                                }
                            }
                        }
                    });
            });
            let all = circuit_names(model);
            let ticked = |c: &Option<BTreeSet<String>>, n: &String| {
                c.as_ref().is_none_or(|set| set.contains(n))
            };
            let n = all.iter().filter(|n| ticked(&self.circuits, n)).count();
            let title = if n == all.len() {
                "Circuits: all".to_owned()
            } else {
                format!("Circuits: {n} of {}", all.len())
            };
            crate::menu::options_menu(ui, &title, |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("Select all").clicked() {
                        self.circuits = None;
                    }
                    if ui.small_button("Select none").clicked() {
                        self.circuits = Some(BTreeSet::new());
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for name in &all {
                            let mut on = ticked(&self.circuits, name);
                            let label = if name.is_empty() {
                                "(no circuit name)"
                            } else {
                                name.as_str()
                            };
                            if ui.checkbox(&mut on, label).changed() {
                                let set = self
                                    .circuits
                                    .get_or_insert_with(|| all.iter().cloned().collect());
                                if on {
                                    set.insert(name.clone());
                                } else {
                                    set.remove(name);
                                }
                                if set.len() == all.len() {
                                    self.circuits = None;
                                }
                            }
                        }
                    });
            });
            ui.label("stations every");
            ui.add(
                egui::DragValue::new(&mut self.settings.spacing)
                    .range(0.1..=50.0)
                    .speed(0.1)
                    .suffix(" m"),
            );
            ui.checkbox(&mut self.settings.nearby, "nearby spans too")
                .on_hover_text(
                    "Also conductors not between the same two structures (another line, the \
                     next span), measured where closer than the distance",
                );
            ui.add_enabled(
                self.settings.nearby,
                egui::DragValue::new(&mut self.settings.max_distance)
                    .range(1.0..=200.0)
                    .speed(0.5)
                    .prefix("within ")
                    .suffix(" m"),
            )
            .on_hover_text("Keep it above the largest z (about 12 m at 380 kV)");
        });
        ui.label(
            egui::RichText::new(
                "Insulation level (low / high row of Table 2.4.1) per circuit: the Electrical \
                 circuits table, column of the LeV plugin; not set: high.",
            )
            .small()
            .weak(),
        );
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, out: &mut Out) {
        let report = match &self.job.result {
            None => return,
            Some(Err(e)) => {
                crate::menu::error_label(ui, e);
                return;
            }
            Some(Ok(r)) => r,
        };
        for n in &report.notes {
            crate::menu::message(ui, egui::RichText::new(n).small(), None);
        }
        let (below, length) = report.below();
        let kept = report.rows.len() < report.pairs;
        let mut export = false;
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} — {}: {} pairs, {below} below z ({length:.0} m of conductor){}",
                report.code,
                report.diagram,
                report.pairs,
                if kept {
                    format!("; {} kept (within {KEPT_WITHIN} m of z)", report.rows.len())
                } else {
                    String::new()
                }
            ));
            ui.checkbox(&mut self.only_below, "only those");
            export = ui.button("Export CSV…").clicked();
        });
        if export {
            save_csv("phase-distance.csv", &csv(report));
        }
        let mut clicked = None;
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("phase-distance-rows")
                    .striped(true)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                        for h in [
                            "case", "span", "circuit", "other", "circuit", "C", "kV", "d m", "z m",
                            "d − z", "below m", "f m", "l_k m", "β °", "k", "C m", "",
                        ] {
                            ui.strong(h);
                        }
                        if self.comparison.is_some() {
                            ui.strong("vs saved");
                        }
                        ui.end_row();
                        for (i, r) in report
                            .rows
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| !self.only_below || r.margin() < 0.0)
                            .take(5000)
                        {
                            let picked = self.picked == Some(i);
                            ui.label(&r.case);
                            for k in 0..2 {
                                let (s, n) = &r.span[k];
                                if ui
                                    .selectable_label(picked, format!("{s} span {}", n + 1))
                                    .on_hover_text(
                                        "Select this span and draw the worst station's line \
                                         (d and z)",
                                    )
                                    .clicked()
                                {
                                    clicked = Some((i, k));
                                }
                                ui.label(&r.what[k]);
                            }
                            ui.label(if r.same_circuit { "C₁" } else { "C₂" });
                            ui.label(format!("{:.0}", r.kv));
                            let mono = |v: f64| egui::RichText::new(format!("{v:.2}")).monospace();
                            let m = mono(r.margin());
                            let m = if r.margin() < 0.0 {
                                m.color(egui::Color32::LIGHT_RED)
                            } else {
                                m
                            };
                            // The values too: a click goes to the worst station.
                            for v in [mono(r.distance), mono(r.required), m] {
                                if ui
                                    .selectable_label(picked, v)
                                    .on_hover_text(
                                        "Go to the worst station: its line drawn with d and z, \
                                         in its weather case",
                                    )
                                    .clicked()
                                {
                                    clicked = Some((i, 0));
                                }
                            }
                            ui.label(format!("{:.0}", r.length_below));
                            ui.label(format!("{:.2} / {:.2}", r.sag[0], r.sag[1]));
                            ui.label(format!("{:.2} / {:.2}", r.lk[0], r.lk[1]));
                            ui.label(format!("{:.0}", r.beta));
                            ui.label(format!("{:.2} / {:.2}", r.k[0], r.k[1]));
                            ui.label(format!("{:.2}", r.c));
                            let mut note = Vec::new();
                            if r.at_clamp {
                                note.push("at a clamp (f ≈ 0)");
                            }
                            if !r.same_span {
                                note.push("nearby span");
                            }
                            ui.label(egui::RichText::new(note.join(", ")).small().weak());
                            if let Some(c) = &self.comparison {
                                let change = c.changes.get(&r.key()).copied();
                                let text = change.map_or("", Change::text);
                                let color = match change {
                                    Some(Change::New | Change::Worse) => egui::Color32::LIGHT_RED,
                                    Some(Change::Fixed | Change::Better) => {
                                        egui::Color32::LIGHT_GREEN
                                    }
                                    None => ui.visuals().weak_text_color(),
                                };
                                ui.label(egui::RichText::new(text).color(color));
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some((i, k)) = clicked {
            let r = &report.rows[i];
            self.picked = Some(i);
            out.case = Some(r.case.clone());
            out.go = Some(Selection::Span {
                section: r.span[k].0.clone(),
                span: r.span[k].1,
            });
            out.measure = Some((
                r.at[0],
                r.at[1],
                format!("d {:.2} m · z {:.2} m", r.distance, r.required),
            ));
        }
    }
}

/// How the check computes, in brief (always shown at the top).
fn explanation(ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("How it is computed")
        .id_salt("phase-distance-how")
        .default_open(true)
        .show(ui, |ui| {
            let small = |ui: &mut egui::Ui, t: &str| {
                ui.label(egui::RichText::new(t).small());
            };
            small(
                ui,
                "LeV Annex 6, 2.2.1, still air: the conductors of a span must be at least \
                 z = k·√(f + l_k) + C apart (the larger z of the two conductors).",
            );
            small(
                ui,
                "Pairs: the conductors between the same two structures (and, ticked, nearby \
                 spans within the distance); not two earth wires, one circuit's same phase, \
                 or two sharing a clamp; a bundle by its subconductors.",
            );
            small(
                ui,
                "Stations every Δs along each conductor in turn; at each, the closest point \
                 of the other, measured in the vertical plane through both: d their distance, \
                 β the angle of the line joining them from the vertical.",
            );
            small(
                ui,
                "f: the local sag there, below the chord between the clamps (0 at the \
                 clamps). l_k: the suspension string's length at each end (I-string: its \
                 length; V, tension or clamped: 0; below a V: the string below; TAK: its \
                 suspension string), linear between the ends.",
            );
            small(
                ui,
                "k: by β (0° one above the other, 90° side by side) on the line of the \
                 conductor's swing class w = d/q of the diagram chosen. C: C₁ within a circuit \
                 and towards an earth wire, C₂ between circuits, by the higher nominal \
                 voltage and the circuit's insulation level (low / high row), +14 % per \
                 1000 m of mean conductor height above 1000 m.",
            );
            small(
                ui,
                "Per pair: the worst station (smallest d − z) and the length of conductor \
                 below z. A worst station at a clamp has f ≈ 0: z there is C plus the string \
                 term.",
            );
        });
}

fn csv(r: &Report) -> String {
    use ols_core::text::csv_field as q;
    use std::fmt::Write as _;
    let mut out = String::from(
        "case,section,span,circuit,other_section,other_span,other_circuit,c,kv,distance_m,\
         required_m,margin_m,length_below_m,sag_m,other_sag_m,lk_m,other_lk_m,beta_deg,k,\
         other_k,c_m,at_clamp,same_span,along_m,other_along_m,x,y,z,other_x,other_y,other_z\n",
    );
    for row in &r.rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.1},{:.3},{:.3},{:.3},{:.3},{:.1},\
             {:.3},{:.3},{:.3},{},{},{:.1},{:.1},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
            q(&row.case),
            q(&row.span[0].0),
            row.span[0].1 + 1,
            q(&row.what[0]),
            q(&row.span[1].0),
            row.span[1].1 + 1,
            q(&row.what[1]),
            if row.same_circuit { "C1" } else { "C2" },
            row.kv,
            row.distance,
            row.required,
            row.margin(),
            row.length_below,
            row.sag[0],
            row.sag[1],
            row.lk[0],
            row.lk[1],
            row.beta,
            row.k[0],
            row.k[1],
            row.c,
            row.at_clamp,
            row.same_span,
            row.along[0],
            row.along[1],
            row.at[0][0],
            row.at[0][1],
            row.at[0][2],
            row.at[1][0],
            row.at[1][1],
            row.at[1][2],
        );
    }
    out
}
