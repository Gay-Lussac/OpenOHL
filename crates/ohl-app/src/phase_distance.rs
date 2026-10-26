// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › (the plugin's submenu, LeV/OLEI) › Distance between conductors: the still-air distance
//! between the conductors of each span by a phase distance code
//! (`.ohlpd`, [`ohl_clearance::phase_distance`]), in the weather cases
//! ticked (each on its own), for the circuits ticked; the insulation
//! levels from the Electrical circuits table (the LeV plugin's column).
//! Computed on demand in the background; worst pairs first; a click
//! selects the span and draws the worst station's line with d and z.
//!
//! A run is saved in the library of performed checks
//! ([`ohl_session::reports`]) with its parameters (the values file's text
//! included) and the pairs below z or within [`KEPT_WITHIN`] of it;
//! reopened, it shows whether the line changed since; run again, the new
//! run is compared with it and may replace it.
//!
//! The same window runs the distance under unequal wind
//! ([`Kind::UnequalWind`], [`ohl_clearance::unequal_wind`]): one weather
//! case, its wind stepped from none to its pressure on one conductor and
//! from a share of it to it on the other.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_clearance::phase_distance::{Conductor, PhaseDistanceSettings, conductors};
use ohl_io::checks::PluginCheck;
use ohl_plugins::{ValueTable, WasmCheckPlugin, WasmHost};
use ohl_session::reports::{self, CheckReport, LineState, ReportStatus};
use serde::{Deserialize, Serialize};

use crate::check_help as help;
use crate::job::{Job, save_csv};
use crate::model::Model;
use crate::selection::Selection;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// The extension of a phase distance values file when the check gives
/// none (the LeV plugin's `.ohlpd`).
const PHASE_DISTANCE_FILE_EXTENSION: &str = "ohlpd";

/// A worst station within this of a conductor's clamp is marked "at a
/// clamp" (f about 0), m.
const NEAR_CLAMP: f64 = 2.0;

/// The check's id in the library of performed checks.
pub const CHECK_ID: &str = "phase-distance";

/// The check in words.
/// (A check's title is its plugin's; this where there is none.)
pub const CHECK_TITLE: &str = "Distance between conductors";

/// The unequal wind check's id in the library of performed checks.
pub const UNEQUAL_WIND_CHECK_ID: &str = "unequal-wind";

/// The unequal wind check in words (where its plugin gives none).
pub const UNEQUAL_WIND_CHECK_TITLE: &str = "Distance between conductors under unequal wind";

/// The wind step when none is set, Pa.
pub const DEFAULT_WIND_STEP: f64 = 50.0;

/// Which check the window runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// The distance in the span (sag and string).
    #[default]
    Span,
    /// The distance under unequal wind.
    UnequalWind,
}

impl Kind {
    /// The plugin check's id ([`ohl_io::checks`]), also the report's.
    pub fn check_id(self) -> &'static str {
        match self {
            Kind::Span => CHECK_ID,
            Kind::UnequalWind => UNEQUAL_WIND_CHECK_ID,
        }
    }

    /// The check in words.
    pub fn title(self) -> &'static str {
        match self {
            Kind::Span => CHECK_TITLE,
            Kind::UnequalWind => UNEQUAL_WIND_CHECK_TITLE,
        }
    }

    /// The page's id.
    fn page(self) -> &'static str {
        match self {
            Kind::Span => "phase-distance-page",
            Kind::UnequalWind => "unequal-wind-page",
        }
    }
}

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
    /// Within one electrical system, or towards an earth wire: C₁.
    #[serde(alias = "same_circuit")]
    pub same_system: bool,
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
    /// The rule applied (e.g. "2.2.1"; "2.2.4 (phase spacers)").
    #[serde(default)]
    pub rule: String,
    /// Under unequal wind: the wind pressure on each, Pa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<[f64; 2]>,
    /// Under unequal wind: the side it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    /// Under unequal wind: the two conductors as measured (each at its
    /// wind), to draw the pair without solving again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wires: Vec<Vec<[f64; 3]>>,
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
    /// The k diagram (index; the first choice).
    pub diagram: usize,
    /// The plugin's choices (the k diagram first).
    #[serde(default)]
    pub choices: Vec<u32>,
    /// The plugin that ran it, with its version.
    #[serde(default)]
    pub plugin: String,
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
    /// Under unequal wind: the wind step, Pa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_step_pa: Option<f64>,
    /// Under unequal wind: the other conductor's least share of the wind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_share: Option<f64>,
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

/// A wire to draw, with its label (under unequal wind: a conductor at
/// its own wind).
pub type MeasureWire = (Vec<[f64; 3]>, String);

/// What the window asks of the application.
#[derive(Default)]
pub struct Out {
    /// A span to go to.
    pub go: Option<Selection>,
    /// The line to draw (d and z), replacing the last.
    pub measure: Option<Measure>,
    /// A command to apply (a report saved).
    pub apply: Option<(String, ohl_session::Command)>,
    /// The weather case to show (that of the result clicked), by name.
    pub case: Option<String>,
    /// The wires to draw, replacing the last (empty: none).
    pub wires: Option<Vec<MeasureWire>>,
}

/// Whether the overrides let conductor `c` (a span's wire) take weather
/// case `case`.
fn conductor_takes(
    line: &ohl_line::Line,
    filter: &ohl_weather::CaseFilter,
    c: &Conductor,
    case: &str,
) -> bool {
    crate::case_groups::span_takes(line, filter, &c.wire.section, c.wire.span, case)
}

/// The Distance between conductors window (and the one under unequal
/// wind).
pub struct PhaseDistanceWindow {
    /// The check it runs.
    pub kind: Kind,
    /// The structure and section overrides of its cases.
    pub overrides: crate::case_groups::Overrides,
    /// To be shown (the app opens its page).
    pub open: bool,
    /// A values file chosen (none: the project's copy, else the
    /// plugin's).
    pub file: Option<PathBuf>,
    /// Weather cases ticked (indices in the model's cases).
    pub cases: BTreeSet<usize>,
    /// Circuits ticked (none: all).
    pub circuits: Option<BTreeSet<String>>,
    /// The settings (circuits: from `circuits`).
    pub settings: PhaseDistanceSettings,
    /// The plugin's choices (the k diagram).
    pub choices: Vec<u32>,
    /// Under unequal wind: the wind step, Pa.
    pub wind_step: f64,
    /// Under unequal wind: the other conductor's least share of the wind.
    pub low_share: f64,
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
    /// The values being edited (an admin): the file's text.
    pub(crate) editing: Option<String>,
    /// A cell being typed: (table, row, column), its text.
    cell: Option<([u32; 3], String)>,
    /// The plugin running the check, for the plugins in use.
    plugin: Option<Loaded>,
    /// The values shown as tables, for their text.
    tables: Option<(String, Result<Vec<ValueTable>, String>)>,
    values_error: Option<String>,
    /// The project's copy of a values file to keep (`Some` text) or
    /// remove (`None`), done by [`PhaseDistanceWindow::apply_values`]
    /// after the window (a logged change of the project).
    pending_values: Option<(String, Option<String>)>,
    /// The line's state against the saved report's, for this line version.
    since: Option<(u64, String, bool)>,
    pub(crate) job: Job<Report>,
}

impl Default for PhaseDistanceWindow {
    fn default() -> Self {
        Self::new(Kind::Span)
    }
}

impl PhaseDistanceWindow {
    /// The window of `kind`.
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            overrides: Default::default(),
            open: false,
            file: None,
            cases: BTreeSet::new(),
            circuits: None,
            settings: PhaseDistanceSettings::default(),
            choices: Vec::new(),
            wind_step: DEFAULT_WIND_STEP,
            low_share: ohl_clearance::unequal_wind::UnequalWindSettings::default().low_share,
            only_below: true,
            picked: None,
            values: None,
            loaded: None,
            save_name: String::new(),
            comparison: None,
            ask: None,
            compared: true,
            save_error: None,
            editing: None,
            cell: None,
            plugin: None,
            tables: None,
            values_error: None,
            pending_values: None,
            since: None,
            job: Job::default(),
        }
    }
}

/// The check plugin, loaded for the plugins in use.
struct Loaded {
    /// The check plugins' directories it was looked for in.
    dirs: Vec<PathBuf>,
    /// The host, the check, an instance for the window's calls.
    check: Result<(Arc<WasmHost>, PluginCheck, WasmCheckPlugin), String>,
}

/// The plugins' check `id` (of the distance between conductors), loaded.
fn load_plugin(
    model: &Model,
    id: &str,
) -> Result<(Arc<WasmHost>, PluginCheck, WasmCheckPlugin), String> {
    let check = model
        .checks
        .iter()
        .find(|c| c.info.id == id)
        .cloned()
        .ok_or_else(|| {
            let mut why = "no check plugin in use runs it: install a plugin that does or turn it \
                           on (File › Plugins › Plugin manager)"
                .to_owned();
            if !model.check_problems.is_empty() {
                why.push_str(&format!(" ({})", model.check_problems.join("; ")));
            }
            why
        })?;
    let host = Arc::new(WasmHost::new().map_err(|e| e.to_string())?);
    let instance = host.load_check(&check.dir).map_err(|e| e.to_string())?;
    Ok((host, check, instance))
}

/// The name of the plugin's values file (`<plugin>.<extension>`).
fn values_name(check: &PluginCheck) -> String {
    let plugin = check.plugin.split(' ').next().unwrap_or_default();
    format!("{plugin}.{}", check.info.values_extension)
}

/// The plugin's circuit column (the insulation level) and its default
/// value (none set by the column: `high`).
fn level_column(model: &Model, check: &PluginCheck) -> (Option<String>, String) {
    let key = check.info.circuit_columns.first().cloned();
    let default = key
        .as_ref()
        .and_then(|k| model.plugin_columns.iter().find(|c| &c.key == k))
        .and_then(|c| c.default.clone())
        .unwrap_or_else(|| "high".into());
    (key, default)
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
        .map(|s| s.circuit_name.clone())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The value of plugin column `key` each circuit is checked with: its
/// sections' (the Electrical circuits table's column), else `default`;
/// sections of a circuit set differently: the last value in order
/// (`high` over `low`).
fn levels(model: &Model, key: &str, default: &str) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for s in &model.line.sections {
        let v = s
            .fields
            .get(key)
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_else(|| default.to_owned());
        let e = out
            .entry(s.circuit_name.clone())
            .or_insert_with(|| v.clone());
        if v > *e {
            *e = v;
        }
    }
    out
}

/// Where the values come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// A saved report's.
    Report,
    /// The plugin's own.
    Plugin,
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
            .is_ok_and(|r| r == Some(ohl_session::Role::Admin))
    })
}

/// Keeps `text` as the project's copy of values file `name` (admins),
/// headed by who edited it when.
fn save_values(model: &mut Model, name: &str, text: &str) -> Result<(), String> {
    if !is_admin(model) {
        return Err("only an admin of the project may change the values".into());
    }
    let e = model
        .editing
        .as_mut()
        .map_err(|e| format!("read-only: {e}"))?;
    let time = e.session.now().map_err(|e| e.to_string())?;
    let body: String = text
        .lines()
        .skip_while(|l| {
            // The header's former wording, from before OpenOHL.
            l.starts_with("# Edited in OHL-Design") || l.starts_with("# Edited in OLS CAD")
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let text = format!(
        "# Edited in OHL-Design by {} on {time} (the project's copy of {name}).\n{body}",
        e.user
    );
    e.session
        .put_library_file(&e.user, VALUES_KIND, name, &text)
        .map_err(|e| e.to_string())
}

/// Removes the project's copy of values file `name` (admins).
fn reset_values(model: &mut Model, name: &str) -> Result<(), String> {
    if !is_admin(model) {
        return Err("only an admin of the project may change the values".into());
    }
    let e = model
        .editing
        .as_mut()
        .map_err(|e| format!("read-only: {e}"))?;
    e.session
        .delete_library_file(&e.user, VALUES_KIND, name)
        .map_err(|e| e.to_string())
}

/// The values as the plugin shows them; typed in (a cell at a time,
/// through the plugin) when `edit`: the cell changed and its text.
fn value_tables_ui(
    ui: &mut egui::Ui,
    tables: &[ValueTable],
    edit: bool,
    cell: &mut Option<([u32; 3], String)>,
) -> Option<([u32; 3], String)> {
    let mut changed = None;
    ui.horizontal_wrapped(|ui| {
        for (ti, t) in tables.iter().enumerate() {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&t.title).strong())
                    .on_hover_text(&t.note);
                egui::Grid::new(("phase-distance-values", ti))
                    .striped(true)
                    .spacing([10.0, 2.0])
                    .show(ui, |ui| {
                        for h in &t.columns {
                            ui.label(egui::RichText::new(h).small().strong());
                        }
                        ui.end_row();
                        for (ri, row) in t.rows.iter().enumerate() {
                            for (ci, text) in row.iter().enumerate() {
                                let id = [ti as u32, ri as u32, ci as u32];
                                let editable = edit && t.editable.get(ci).copied().unwrap_or(false);
                                if !editable {
                                    ui.label(egui::RichText::new(text).monospace());
                                    continue;
                                }
                                let mut typed = match cell {
                                    Some((c, s)) if *c == id => s.clone(),
                                    _ => text.clone(),
                                };
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut typed)
                                        .desired_width(56.0)
                                        .font(egui::TextStyle::Monospace),
                                );
                                if r.changed() {
                                    *cell = Some((id, typed.clone()));
                                }
                                if r.lost_focus()
                                    && cell.as_ref().is_some_and(|(c, _)| *c == id)
                                    && let Some((c, s)) = cell.take()
                                    && s != *text
                                {
                                    changed = Some((c, s));
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
            ui.separator();
        }
    });
    changed
}

impl PhaseDistanceWindow {
    /// Keeps or removes the project's copy of the values chosen in the
    /// window (a change of the project, logged).
    pub fn apply_values(&mut self, model: &mut Model) {
        let Some((name, text)) = self.pending_values.take() else {
            return;
        };
        let saving = text.is_some();
        let result = match text {
            Some(t) => save_values(model, &name, &t),
            None => reset_values(model, &name),
        };
        match result {
            Ok(()) if saving => {
                self.editing = None;
                self.values = None;
                self.file = None;
                self.values_error = None;
            }
            Ok(()) => self.values_error = None,
            Err(e) => self.values_error = Some(e),
        }
    }

    /// Opens the window: the still-air 10 °C cases ticked when none is.
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        self.ensure_plugin(model);
        self.cases.retain(|k| *k < model.cases.len());
        match self.kind {
            Kind::Span if self.cases.is_empty() => self.cases = still_air_10(model),
            Kind::UnequalWind if self.cases.len() != 1 => {
                // One case: the first with the most wind.
                let windiest = model
                    .cases
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.case.wind_pressure.si() > 0.0)
                    .max_by(|(i, a), (j, b)| {
                        a.case
                            .wind_pressure
                            .si()
                            .total_cmp(&b.case.wind_pressure.si())
                            .then(j.cmp(i))
                    })
                    .map(|(k, _)| k);
                let first = self.cases.first().copied();
                self.cases = first.or(windiest).into_iter().collect();
            }
            _ => {}
        }
    }

    /// Loads the check plugin when the plugins in use changed.
    fn ensure_plugin(&mut self, model: &Model) {
        if self
            .plugin
            .as_ref()
            .is_some_and(|p| p.dirs == model.check_plugins)
        {
            return;
        }
        let check = load_plugin(model, self.kind.check_id());
        if let Ok((_, c, _)) = &check {
            self.choices = c.info.choices.iter().map(|x| x.default).collect();
        }
        self.plugin = Some(Loaded {
            dirs: model.check_plugins.clone(),
            check,
        });
        self.tables = None;
    }

    /// The check plugin, or why there is none.
    pub(crate) fn check(&self) -> Result<&PluginCheck, String> {
        match self.plugin.as_ref().map(|p| &p.check) {
            Some(Ok((_, c, _))) => Ok(c),
            Some(Err(e)) => Err(e.clone()),
            None => Err("the check plugin is not loaded yet".into()),
        }
    }

    /// The values of `text` as tables (the plugin's), kept for the text.
    pub(crate) fn tables_of(&mut self, text: &str) -> Result<Vec<ValueTable>, String> {
        if let Some((t, r)) = &self.tables
            && t == text
        {
            return r.clone();
        }
        let r = match self.plugin.as_mut().map(|p| &mut p.check) {
            Some(Ok((_, c, instance))) => instance
                .value_tables(&c.info.id, text)
                .map_err(|e| e.to_string())
                .and_then(|r| r),
            Some(Err(e)) => Err(e.clone()),
            None => Err("the check plugin is not loaded yet".into()),
        };
        self.tables = Some((text.to_owned(), r.clone()));
        r
    }

    /// `text` with a cell set, through the plugin.
    pub(crate) fn edit_cell(
        &mut self,
        text: &str,
        cell: [u32; 3],
        value: &str,
    ) -> Result<String, String> {
        match self.plugin.as_mut().map(|p| &mut p.check) {
            Some(Ok((_, c, instance))) => instance
                .edit_value(&c.info.id, text, cell, value)
                .map_err(|e| e.to_string())?,
            Some(Err(e)) => Err(e.clone()),
            None => Err("the check plugin is not loaded yet".into()),
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
        if self.kind == Kind::UnequalWind && self.cases.len() != 1 {
            self.job.result = Some(Err("choose one weather case (its wind)".into()));
            return;
        }
        self.ensure_plugin(model);
        let (host, check) = match self.plugin.as_ref().map(|p| &p.check) {
            Some(Ok((h, c, _))) => (h.clone(), c.clone()),
            Some(Err(e)) => {
                self.job.result = Some(Err(e.clone()));
                return;
            }
            None => return,
        };
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
        let (key, default) = level_column(model, &check);
        let choices = self.choices.clone();
        let diagram_name = check
            .info
            .choices
            .first()
            .zip(choices.first())
            .and_then(|(c, k)| c.options.get(*k as usize).cloned())
            .unwrap_or_default();
        let mut params = Params {
            values_sha256: ohl_session::versions::hash(&text),
            values_file,
            values_text: text,
            diagram: choices.first().copied().unwrap_or(0) as usize,
            choices: choices.clone(),
            plugin: check.plugin.clone(),
            diagram_name: diagram_name.clone(),
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
            levels: key
                .as_deref()
                .map(|k| levels(model, k, &default))
                .unwrap_or_default(),
            wind_step_pa: (self.kind == Kind::UnequalWind).then_some(self.wind_step),
            low_share: (self.kind == Kind::UnequalWind).then_some(self.low_share),
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
        let kind = self.kind;
        let unequal = ohl_clearance::unequal_wind::UnequalWindSettings {
            pairs: settings.clone(),
            low_share: self.low_share,
        };
        let wind_step = self.wind_step;
        let filter = self.overrides.filter(model);
        self.job.start_at(ctx, model.revision(), move || {
            let mut notes = Vec::new();
            let mut rows = Vec::new();
            if kind == Kind::UnequalWind
                && let Some((solved, _)) = cases.first()
            {
                let case = &solved.case;
                let (mut levels, n) = ohl_clearance::unequal_wind::solve_levels(
                    &line,
                    &library,
                    case,
                    wind_step,
                    unequal.low_share,
                    |c| {
                        let s = crate::model::Solved::new_memo(&line, &library, c.clone());
                        (s.shapes, s.failed)
                    },
                )?;
                notes.extend(n.into_iter().map(|n| format!("\"{}\" {n}", case.name)));
                // The overrides: the conductors whose span does not take
                // the case left out, the same at every level.
                if let Some(first) = levels.first() {
                    let keep: Vec<bool> = first
                        .conductors
                        .iter()
                        .map(|c| conductor_takes(&line, &filter, c, &case.name))
                        .collect();
                    for l in &mut levels {
                        let mut k = 0;
                        l.conductors.retain(|_| {
                            k += 1;
                            keep.get(k - 1).copied().unwrap_or(true)
                        });
                    }
                }
                // Exit clause: the pairs of a 25 km line at about 20
                // levels in seconds.
                let budget =
                    ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
                let pairs = ohl_io::checks::unequal_wind(
                    &host,
                    &check,
                    &params.values_text,
                    &choices,
                    &unequal,
                    &levels,
                    &budget,
                )
                .map_err(|e| format!("{}: {e}", check.plugin))?;
                let cs = levels
                    .first()
                    .map(|l| &l.conductors[..])
                    .unwrap_or_default();
                for p in pairs {
                    let (a, b) = (&cs[p.a], &cs[p.b]);
                    rows.push(Row {
                        case: case.name.clone(),
                        span: [
                            (a.wire.section.clone(), a.wire.span),
                            (b.wire.section.clone(), b.wire.span),
                        ],
                        what: [what(a), what(b)],
                        same_system: p.same_system,
                        same_span: p.same_span,
                        kv: p.kv,
                        distance: p.distance,
                        required: p.required,
                        length_below: 0.0,
                        at: p.at,
                        along: p.along,
                        at_clamp: false,
                        sag: [0.0; 2],
                        lk: [0.0; 2],
                        beta: {
                            let d = (0..3)
                                .map(|k| (p.at[0][k] - p.at[1][k]).powi(2))
                                .sum::<f64>()
                                .sqrt();
                            let dz = (p.at[0][2] - p.at[1][2]).abs();
                            if d > 0.0 {
                                (dz / d).clamp(0.0, 1.0).acos().to_degrees()
                            } else {
                                0.0
                            }
                        },
                        k: [f64::NAN; 2],
                        c: p.values.first().copied().unwrap_or(p.required),
                        rule: p.rule.clone(),
                        pressure: Some(p.pressure),
                        side: Some(p.side.as_str().to_owned()),
                        // The wires the distance was measured between
                        // (a bundle's subconductor), each at its wind.
                        wires: p.wires.to_vec(),
                    });
                }
            }
            for (solved, ready) in cases.into_iter().filter(|_| kind == Kind::Span) {
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
                let mut cs = conductors(&line, &solved.shapes, &library);
                cs.retain(|c| conductor_takes(&line, &filter, c, &name));
                // Exit clause: about a second for a 25 km line.
                let budget =
                    ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(300));
                let pairs = ohl_io::checks::phase_distance(
                    &host,
                    &check,
                    &params.values_text,
                    &choices,
                    &settings,
                    &cs,
                    &budget,
                )
                .map_err(|e| format!("{}: {e}", check.plugin))?;
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
                for p in pairs {
                    let (a, b) = (&cs[p.a], &cs[p.b]);
                    let w = &p.worst;
                    let near = |c: &Conductor, s: f64| s < NEAR_CLAMP || length(c) - s < NEAR_CLAMP;
                    // The plugin's numbers: k of each, C.
                    let v = |i: usize| w.values.get(i).copied().unwrap_or(f64::NAN);
                    rows.push(Row {
                        case: name.clone(),
                        span: [
                            (a.wire.section.clone(), a.wire.span),
                            (b.wire.section.clone(), b.wire.span),
                        ],
                        what: [what(a), what(b)],
                        same_system: p.same_system,
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
                        k: [v(0), v(1)],
                        c: v(2),
                        rule: p.rule.clone(),
                        pressure: None,
                        side: None,
                        wires: Vec::new(),
                    });
                }
            }
            rows.sort_by(|a, b| a.margin().total_cmp(&b.margin()));
            params.diagram_name = diagram_name.clone();
            Ok(Report {
                code: params.plugin.clone(),
                diagram: diagram_name,
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
        if r.check != self.kind.check_id() {
            return Err(format!("{} is not a {} report", r.id, self.kind.title()));
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
        self.choices = if params.choices.is_empty() {
            vec![params.diagram as u32]
        } else {
            params.choices.clone()
        };
        if let Some(step) = params.wind_step_pa {
            self.wind_step = step;
        }
        if let Some(share) = params.low_share {
            self.low_share = share;
        }
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
        // A saved report: compared with the line by its own state.
        self.job.stamp.clear();
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
            check: self.kind.check_id().into(),
            title: self
                .check()
                .map_or(self.kind.title().to_owned(), |c| c.info.title.clone()),
            time: editing.session.now().map_err(|e| e.to_string())?,
            user: editing.user.clone(),
            app: format!("OHL-Design {}", env!("CARGO_PKG_VERSION")),
            plugin: self.check().ok().map(|c| c.plugin.clone()),
            status: ReportStatus::Draft,
            note,
            state,
            summary: if self.kind == Kind::UnequalWind {
                format!("{} pairs, {below} below z (unequal wind)", r.pairs)
            } else {
                format!(
                    "{} pairs, {below} below z ({length:.0} m of conductor)",
                    r.pairs
                )
            },
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
    ) -> Option<(String, ohl_session::Command)> {
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

    /// The page (a dock page: floating first, it may be docked). Run in
    /// the footer at the bottom left, always visible (owner, 2026-10-21);
    /// the rest scrolls above it.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Out {
        let page = self.kind.page();
        let out = help::body(ui, page, |ui| self.body_ui(ui, model));
        help::footer(ui, page, |ui| {
            self.overrides.ui(ui);
            if help::run_button(
                ui,
                !self.job.running(),
                "Compute the distances between the conductors with the plugin's values",
                None,
            ) {
                self.start(model, ui.ctx());
            }
            self.job.status_ui(ui);
        });
        out
    }

    /// The page above its footer.
    fn body_ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Out {
        self.job.poll();
        self.ensure_plugin(model);
        if let Err(e) = self.check() {
            crate::menu::error_label(ui, e);
        }
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
        self.settings_ui(ui, model);
        self.levels_ui(ui, model, &mut out);
        self.library_ui(ui, model, &mut out);
        ui.separator();
        // How it computes and with which values: open, in a box of
        // its own height so the results keep their place.
        egui::ScrollArea::vertical()
            .id_salt("phase-distance-how-and-values")
            .max_height(260.0)
            .show(ui, |ui| {
                let plugin = self.check().ok().map(|c| c.info.explanation.clone());
                explanation(ui, plugin.as_deref(), self.kind);
                self.values_ui(ui, model);
            });
        ui.separator();
        if let Some(what) = self.out_of_date(model)
            && crate::revision::banner(ui, &what, !self.job.running())
        {
            self.start(model, ui.ctx());
        }
        self.results_ui(ui, &mut out);
        out
    }

    /// What the last run was computed from that changed since, in words
    /// (`None`: nothing, no run, or a saved report — compared with the
    /// line by its own state): the line, its cases and files
    /// ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
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
                    t.color(crate::menu::ok_color(ui))
                } else {
                    t.color(ui.visuals().warn_fg_color)
                });
                ui.label(
                    egui::RichText::new("Run runs it again with its parameters")
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
                    Some(ui.visuals().warn_fg_color),
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
                        Some(ui.visuals().warn_fg_color),
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
                self.save_name = self
                    .check()
                    .map_or(self.kind.title().to_owned(), |c| c.info.title.clone());
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
    /// project's copy (edited by an admin), else the plugin's: (file
    /// name, text, where from).
    fn current_values(&self, model: &Model) -> Result<(String, String, Source), String> {
        if let Some((name, text)) = &self.values {
            return Ok((name.clone(), text.clone(), Source::Report));
        }
        if let Some(file) = &self.file {
            let name = file
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            return std::fs::read_to_string(file)
                .map(|t| (name, t, Source::File))
                .map_err(|e| format!("{}: {e}", file.display()));
        }
        let check = self.check()?;
        let name = values_name(check);
        if let Some(text) = project_values(model, &name) {
            return Ok((name, text, Source::Project));
        }
        Ok((name, check.info.values.clone(), Source::Plugin))
    }

    /// The values as tables (read only; an admin may edit them: kept in
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
                let editing = self.editing.is_some();
                let shown = self.editing.clone().unwrap_or_else(|| text.clone());
                let tables = match self.tables_of(&shown) {
                    Ok(t) => t,
                    Err(e) => {
                        crate::menu::error_label(ui, format!("{name}: {e}"));
                        return;
                    }
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{name}, {}",
                            match source {
                                Source::Report => "as saved in the report",
                                Source::Project => "the project's copy (edited by an admin)",
                                Source::File => "the file chosen",
                                Source::Plugin => "the plugin's",
                            }
                        ))
                        .small(),
                    );
                    if !editing {
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
                            self.editing = Some(text.clone());
                            self.values_error = None;
                        }
                        if admin
                            && source == Source::Project
                            && ui
                                .button("Back to the plugin's")
                                .on_hover_text(
                                    "Remove the project's copy: the plugin's values again",
                                )
                                .clicked()
                        {
                            self.pending_values = Some((name.clone(), None));
                        }
                    }
                });
                if editing {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Editing the values (admin):")
                                .color(ui.visuals().warn_fg_color),
                        );
                        if ui.button("Save in the project").clicked() {
                            let edited = self.editing.clone().unwrap_or_default();
                            self.pending_values = Some((name.clone(), Some(edited)));
                        }
                        if ui.button("Cancel").clicked() {
                            self.editing = None;
                            self.cell = None;
                            self.values_error = None;
                        }
                    });
                }
                let mut cell = self.cell.take();
                let changed = value_tables_ui(ui, &tables, editing, &mut cell);
                self.cell = cell;
                if let Some((c, value)) = changed {
                    match self.edit_cell(&shown, c, &value) {
                        Ok(new) => {
                            self.editing = Some(new);
                            self.values_error = None;
                        }
                        Err(e) => self.values_error = Some(e),
                    }
                }
                if let Some(e) = &self.values_error {
                    crate::menu::error_label(ui, e);
                }
            });
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let choices = self
            .check()
            .map(|c| c.info.choices.clone())
            .unwrap_or_default();
        let ext = self
            .check()
            .map_or(PHASE_DISTANCE_FILE_EXTENSION.to_owned(), |c| {
                c.info.values_extension.clone()
            });
        ui.horizontal(|ui| {
            if ui
                .button("Choose a values file…")
                .on_hover_text("Another values file instead of the values used")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Values", &[&ext])
                    .pick_file()
            {
                self.file = Some(p);
                self.values = None;
            }
            if self.file.is_some()
                && ui
                    .small_button("Forget the file")
                    .on_hover_text("The project's copy, else the plugin's values")
                    .clicked()
            {
                self.file = None;
            }
            self.choices.resize(choices.len(), 0);
            for (k, c) in choices.iter().enumerate() {
                ui.label(&c.title).on_hover_text(&c.help);
                let shown = c
                    .options
                    .get(self.choices[k] as usize)
                    .cloned()
                    .unwrap_or_else(|| "—".into());
                egui::ComboBox::from_id_salt(("phase-distance-choice", k))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (i, o) in c.options.iter().enumerate() {
                            ui.selectable_value(&mut self.choices[k], i as u32, o);
                        }
                    });
            }
        });
        ui.horizontal(|ui| {
            let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
            if self.kind == Kind::UnequalWind {
                let shown = self
                    .cases
                    .first()
                    .and_then(|k| model.cases.get(*k))
                    .map_or("—".to_owned(), |c| {
                        format!("{} ({:.0} Pa)", c.case.name, c.case.wind_pressure.si())
                    });
                ui.label("Weather case")
                    .on_hover_text("Its wind (the most), temperature and ice");
                egui::ComboBox::from_id_salt("unequal-wind-case")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (k, c) in model.cases.iter().enumerate() {
                            let text =
                                format!("{} ({:.0} Pa)", c.case.name, c.case.wind_pressure.si());
                            if ui
                                .selectable_label(self.cases.first() == Some(&k), text)
                                .clicked()
                            {
                                self.cases = BTreeSet::from([k]);
                            }
                        }
                    });
                ui.label("wind step");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut self.wind_step)
                        .range(1.0..=1000.0)
                        .speed(1.0)
                        .suffix(" Pa"),
                )
                .on_hover_text(
                    "The wind on one conductor from none to the case's in these steps (the \
                     case's always included); the line is solved at each, and at the share of \
                     each",
                );
                let mut share = self.low_share * 100.0;
                ui.label("the other from");
                if ui
                    .add(
                        egui::DragValue::new(&mut share)
                            .range(0.0..=100.0)
                            .speed(1.0)
                            .suffix(" %"),
                    )
                    .on_hover_text(
                        "The other conductor's wind from this share of the first's up to it \
                         (LeV: up to 40 % less, 60 %)",
                    )
                    .changed()
                {
                    self.low_share = share / 100.0;
                }
            } else {
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
                    let mut ticked: Vec<usize> = self.cases.iter().copied().collect();
                    let groups = crate::case_groups::groups(model);
                    if crate::case_groups::pick_indices(ui, model, &groups, &mut ticked) {
                        self.cases = ticked.into_iter().collect();
                    }
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
            }
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
            // Under unequal wind the closest approach is exact: no stations.
            if self.kind == Kind::Span {
                ui.label("stations every");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut self.settings.spacing)
                        .range(0.1..=50.0)
                        .speed(0.1)
                        .suffix(" m"),
                );
            }
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
    }

    /// The circuits ticked with their voltage and the plugin's circuit
    /// column (the insulation level), set here (on all the circuit's
    /// sections: the Electrical circuits table's column).
    fn levels_ui(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        let editable = model.can_edit().is_ok();
        let Ok(check) = self.check() else {
            return;
        };
        let (Some(key), default) = level_column(model, check) else {
            return;
        };
        let column = model.plugin_columns.iter().find(|c| c.key == key).cloned();
        let title = column.as_ref().map_or(key.clone(), |c| c.title.clone());
        egui::CollapsingHeader::new(format!("Circuits checked and their {title}"))
            .id_salt("phase-distance-levels")
            .default_open(true)
            .show(ui, |ui| {
                if let Some(c) = &column {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} The same as the Electrical circuits table's column ({}).",
                            c.help, c.plugin
                        ))
                        .small()
                        .weak(),
                    );
                }
                let options: Vec<String> =
                    column.as_ref().map_or_else(Vec::new, |c| c.values.clone());
                let line = &model.line;
                let ticked = |n: &String| self.circuits.as_ref().is_none_or(|s| s.contains(n));
                egui::ScrollArea::vertical()
                    .id_salt("phase-distance-levels-rows")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        egui::Grid::new("phase-distance-levels-grid")
                            .striped(true)
                            .spacing([12.0, 2.0])
                            .show(ui, |ui| {
                                for h in ["circuit", "kV", title.as_str()] {
                                    ui.label(egui::RichText::new(h).small().strong());
                                }
                                ui.end_row();
                                for name in circuit_names(model).iter().filter(|n| ticked(n)) {
                                    let sections: Vec<&ohl_line::Section> = line
                                        .sections
                                        .iter()
                                        .filter(|s| &s.circuit_name == name)
                                        .collect();
                                    let kv = sections
                                        .iter()
                                        .filter_map(|s| s.system.as_deref())
                                        .filter_map(|id| line.systems.iter().find(|x| x.id == id))
                                        .map(|x| x.voltage_kv)
                                        .fold(None, |m: Option<f64>, v| {
                                            Some(m.map_or(v, |m| m.max(v)))
                                        });
                                    ui.label(if name.is_empty() {
                                        "(no circuit name)"
                                    } else {
                                        name.as_str()
                                    });
                                    let Some(kv) = kv else {
                                        ui.label("—");
                                        ui.label(
                                            egui::RichText::new("earthed (by the phase's)")
                                                .small()
                                                .weak(),
                                        );
                                        ui.end_row();
                                        continue;
                                    };
                                    ui.label(format!("{kv:.0}"));
                                    let mut set: Vec<&str> = sections
                                        .iter()
                                        .map(|s| s.fields.get(&key).map_or("", String::as_str))
                                        .collect();
                                    set.sort_unstable();
                                    set.dedup();
                                    let shown = match set.as_slice() {
                                        [""] => format!("{default} (not set)"),
                                        [one] => (*one).to_owned(),
                                        _ => "mixed".to_owned(),
                                    };
                                    let mut chosen: Option<String> = None;
                                    ui.add_enabled_ui(editable, |ui| {
                                        egui::ComboBox::from_id_salt((
                                            "phase-distance-level",
                                            name,
                                        ))
                                        .selected_text(shown)
                                        .width(120.0)
                                        .show_ui(
                                            ui,
                                            |ui| {
                                                for v in &options {
                                                    if ui
                                                        .selectable_label(set == [v.as_str()], v)
                                                        .clicked()
                                                    {
                                                        chosen = Some(v.clone());
                                                    }
                                                }
                                            },
                                        );
                                    });
                                    if let Some(v) = chosen
                                        && set != [v.as_str()]
                                    {
                                        let commands = sections
                                            .iter()
                                            .map(|s| {
                                                let mut s = (*s).clone();
                                                s.fields.insert(key.clone(), v.clone());
                                                ohl_session::Command::Put {
                                                    kind: ohl_session::Kind::Section,
                                                    id: s.id.clone(),
                                                    value: serde_json::to_value(&s)
                                                        .expect("serialises"),
                                                    ord: None,
                                                }
                                            })
                                            .collect();
                                        out.apply = Some((
                                            format!("{name}: {title} {v}"),
                                            ohl_session::Command::Batch { commands },
                                        ));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
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
        let unequal = self.kind == Kind::UnequalWind;
        let mut export = false;
        ui.horizontal(|ui| {
            let how = if unequal {
                format!(
                    "{}: {} pairs, {below} below z (z = C₃ within a system, C₄ between)",
                    report.code, report.pairs
                )
            } else {
                format!(
                    "{} — {}: {} pairs, {below} below z ({length:.0} m of conductor)",
                    report.code, report.diagram, report.pairs
                )
            };
            ui.label(format!(
                "{how}{}",
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
            let name = if unequal {
                "unequal-wind.csv"
            } else {
                "phase-distance.csv"
            };
            save_csv(name, &csv(report));
        }
        let mut clicked = None;
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("phase-distance-rows")
                    .striped(true)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                        let heads: &[&str] = if unequal {
                            &[
                                "case", "span", "circuit", "other", "circuit", "C", "kV", "d m",
                                "z m", "d − z", "wind Pa", "from", "β °", "rule", "",
                            ]
                        } else {
                            &[
                                "case", "span", "circuit", "other", "circuit", "C", "kV", "d m",
                                "z m", "d − z", "below m", "f m", "l_k m", "β °", "k", "C m",
                                "rule", "",
                            ]
                        };
                        for h in heads {
                            ui.strong(*h);
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
                            ui.label(match (unequal, r.same_system) {
                                (false, true) => "C₁",
                                (false, false) => "C₂",
                                (true, true) => "C₃",
                                (true, false) => "C₄",
                            });
                            ui.label(format!("{:.0}", r.kv));
                            let mono = |v: f64| egui::RichText::new(format!("{v:.2}")).monospace();
                            let m = mono(r.margin());
                            let m = if r.margin() < 0.0 {
                                m.color(ui.visuals().error_fg_color)
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
                            if unequal {
                                let [pa, pb] = r.pressure.unwrap_or([f64::NAN; 2]);
                                ui.label(format!("{pa:.0} / {pb:.0}")).on_hover_text(
                                    "The wind pressure on the first and on the other conductor \
                                     at the closest approach",
                                );
                                ui.label(r.side.as_deref().unwrap_or("—"));
                                ui.label(format!("{:.0}", r.beta));
                            } else {
                                ui.label(format!("{:.0}", r.length_below));
                                ui.label(format!("{:.2} / {:.2}", r.sag[0], r.sag[1]));
                                ui.label(format!("{:.2} / {:.2}", r.lk[0], r.lk[1]));
                                ui.label(format!("{:.0}", r.beta));
                                ui.label(format!("{:.2} / {:.2}", r.k[0], r.k[1]));
                                ui.label(format!("{:.2}", r.c));
                            }
                            ui.label(&r.rule);
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
                                    Some(Change::New | Change::Worse) => {
                                        ui.visuals().error_fg_color
                                    }
                                    Some(Change::Fixed | Change::Better) => {
                                        crate::menu::ok_color(ui)
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
            if r.pressure.is_none() {
                // The worst station is in its case: shown.
                out.case = Some(r.case.clone());
            }
            out.go = Some(Selection::Span {
                section: r.span[k].0.clone(),
                span: r.span[k].1,
            });
            out.measure = Some((
                r.at[0],
                r.at[1],
                format!("d {:.2} m · z {:.2} m", r.distance, r.required),
            ));
            if let Some(p) = r.pressure {
                // Only the pair, as measured (each at its wind): nothing
                // solved, the case shown kept.
                let side = r.side.as_deref().unwrap_or("right");
                out.wires = Some(
                    r.wires
                        .iter()
                        .zip(p)
                        .zip(&r.what)
                        .map(|((w, p), what)| {
                            let label = if p > 0.0 {
                                format!("{what} at {p:.0} Pa from the {side}")
                            } else {
                                format!("{what} in still air")
                            };
                            (w.clone(), label)
                        })
                        .collect(),
                );
            }
        }
    }
}

/// A conductor's circuit and phase, in words.
fn what(c: &Conductor) -> String {
    if c.wire.potential.is_none() {
        format!("{} (earth wire)", c.wire.circuit)
    } else {
        format!("{} {}", c.wire.circuit, c.wire.phase)
    }
}

/// How the check computes, in brief (always shown at the top): the
/// measuring (OpenOHL's), then the rules (the plugin's).
fn explanation(ui: &mut egui::Ui, rules: Option<&str>, kind: Kind) {
    egui::CollapsingHeader::new("How it is computed")
        .id_salt("phase-distance-how")
        .default_open(true)
        .show(ui, |ui| {
            let small = |ui: &mut egui::Ui, t: &str| {
                ui.label(egui::RichText::new(t).small());
            };
            for p in rules
                .unwrap_or_default()
                .split("\n\n")
                .filter(|p| !p.trim().is_empty())
            {
                small(ui, &p.replace('\n', " "));
            }
            if kind == Kind::UnequalWind {
                small(
                    ui,
                    "Measured by OpenOHL — pairs as for the distance in the span (in still \
                     air; nearby spans where they come within the distance at any wind). The \
                     line is solved in the weather case (its temperature and ice) with its \
                     wind every step from none to the case's, from each side, and at the \
                     share of each step (so the largest difference is always among them).",
                );
                small(
                    ui,
                    "For each pair, one conductor at a wind p, the other at every level from \
                     the share of p to p, both ways, on each side: the shortest distance \
                     between their axes (a bundle by its subconductors), exact — segment \
                     against segment, not at stations. Kept: the closest, with the winds and \
                     the side. Required: C₃ within one system and towards earth wires, C₄ \
                     between systems (the plugin's values; no sag or string term).",
                );
                return;
            }
            small(
                ui,
                "Measured by OpenOHL — pairs: the conductors between the same two structures \
                 (and, ticked, nearby spans within the distance); not two earth wires, one \
                 circuit's same phase, or two sharing a clamp; a bundle by its subconductors.",
            );
            small(
                ui,
                "Stations every Δs along each conductor in turn; at each, the closest point \
                 of the other: d their distance, β the angle of the line joining them from \
                 the vertical.",
            );
            small(
                ui,
                "f: the local sag there, below the chord between the clamps (0 at the \
                 clamps). l_k: the suspension string's length at each end (one string: its \
                 length; two strings to one wire point, tension or clamped: 0; a string \
                 below two: that string; two tension strings from a string: that string), \
                 linear between the ends. Conductors joined by \
                 interphase spacers in the span: each one's largest sag between its spacer \
                 clamps (and the span's clamps).",
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
    use ohl_core::text::csv_field as q;
    use std::fmt::Write as _;
    let mut out = String::from(
        "case,section,span,circuit,other_section,other_span,other_circuit,c,kv,distance_m,\
         required_m,margin_m,length_below_m,sag_m,other_sag_m,lk_m,other_lk_m,beta_deg,k,\
         other_k,c_m,at_clamp,same_span,along_m,other_along_m,x,y,z,other_x,other_y,other_z,\
         wind_pa,other_wind_pa,wind_from\n",
    );
    for row in &r.rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.1},{:.3},{:.3},{:.3},{:.3},{:.1},\
             {:.3},{:.3},{:.3},{},{},{:.1},{:.1},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{}",
            q(&row.case),
            q(&row.span[0].0),
            row.span[0].1 + 1,
            q(&row.what[0]),
            q(&row.span[1].0),
            row.span[1].1 + 1,
            q(&row.what[1]),
            if row.same_system { "C1" } else { "C2" },
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
            row.pressure
                .map_or(String::new(), |p| format!("{:.1}", p[0])),
            row.pressure
                .map_or(String::new(), |p| format!("{:.1}", p[1])),
            row.side.as_deref().unwrap_or_default(),
        );
    }
    out
}
