// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Safety volumes: the space within a distance of the wires in
//! every position of the weather cases ticked (blowouts, wind steps,
//! swing arc), one closed mesh per wire, per circuit or per system
//! ([`ohl_clearance::safety_volume`]), for work near the line; the wires
//! chosen from the line's circuits or from its electrical systems. The
//! distance is a requirement by voltage (none is built in: the user's, or
//! a rule of a design-code plugin's preset). The settings are grouped and
//! explained as in the clearance windows ([`crate::check_help`]): wire
//! positions, distance, what is included, mesh, export, each with a link
//! to its user-guide section ([`GUIDE`]). Run on a background thread;
//! drawn translucent in the 3D view (each in its circuit's colour) and
//! its outline in the plan while the window is open; out of date (dimmed,
//! **Run again**) once the line or its weather cases change; saved in
//! Checks › Performed checks ([`CHECK_ID`]) with its meshes, and exported
//! as georeferenced DXF (+ `.prj`), IFC4 and KML/KMZ
//! ([`ohl_io::safety_volume`]).

use std::sync::Arc;

use eframe::egui;
use ohl_clearance::rules::{Requirement, WindSides, WindSteps};
use ohl_clearance::safety_volume::{
    DEFAULT_TOLERANCE, Grouping, Mesh, MeshStats, NO_SYSTEM, SafetyParams, SafetyVolume,
    SafetyVolumes, WindMode,
};
use ohl_io::safety_volume::{Description, Format};
use ohl_session::packing::{Reader, Writer};
use ohl_session::reports::{self, CheckReport, ReportStatus};
use serde_json::{Value, json};

use crate::check_help as help;
use crate::check_ui;
use crate::job::Job;
use crate::model::Model;

/// The user-guide page of the window's sections.
pub const GUIDE: &str = "user-guide/analyses";

/// The id of a saved run in the library of performed checks.
pub const CHECK_ID: &str = "ohl.safety-volume";
/// Its title.
pub const CHECK_TITLE: &str = "Safety volumes";
/// The version of its saved parameters and results.
pub const SAVED_VERSION: u32 = 1;
/// A run with more triangles than this is saved without its meshes (run
/// again to see them).
pub const MAX_SAVED_TRIANGLES: usize = 2_000_000;

/// What a safety volume is computed from: the line (with its weather
/// cases).
const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// The window's settings: what a run uses, saved with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The distance from the conductors' surface (none built in: 0 m
    /// until entered).
    pub distance: Requirement,
    /// Added to the distance, m.
    pub margin: f64,
    /// How far the mesh may lie outside the envelope, m.
    pub tolerance: f64,
    /// The wind.
    pub wind: WindMode,
    /// Its steps (step and sides) when stepped.
    pub steps: WindSteps,
    /// Per wire, per circuit or per system.
    pub grouping: Grouping,
    /// The wires chosen from the system, circuit or span list.
    pub scope: Scope,
    /// The circuits ticked, by name (`None`: all).
    pub circuits: Option<Vec<String>>,
    /// The systems ticked, by id (`None`: all).
    pub systems: Option<Vec<String>>,
    /// The spans ticked, `SECTION:N` (N from 1; `None`: all).
    pub spans: Option<Vec<String>>,
}

/// Where the wires are chosen from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// The line's electrical systems.
    Systems,
    /// The line's circuits.
    #[default]
    Circuits,
    /// The sections' spans.
    Spans,
}

impl Scope {
    /// Every scope, as the toggle shows them.
    pub const ALL: [Scope; 3] = [Self::Systems, Self::Circuits, Self::Spans];

    /// `systems`, `circuits` or `spans`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Systems => "systems",
            Self::Circuits => "circuits",
            Self::Spans => "spans",
        }
    }

    /// As the toggle names it.
    pub fn title(self) -> &'static str {
        match self {
            Self::Systems => "Systems",
            Self::Circuits => "Circuits",
            Self::Spans => "Spans",
        }
    }

    /// Reads [`Scope::as_str`].
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|x| x.as_str() == text)
            .ok_or_else(|| format!("scope \"{text}\": systems, circuits or spans"))
    }
}

/// The rows of the scope's list on `line`: (key, label). Systems: name
/// and voltage; circuits: name, system and voltage; spans: section, span
/// number, the two structures and the length in plan.
pub fn scope_items(line: &ohl_line::Line, scope: Scope) -> Vec<crate::tick_list::Item> {
    use crate::tick_list::Item;
    let system_name = |x: &ohl_line::electrical::System| {
        if x.name.is_empty() {
            x.id.clone()
        } else {
            x.name.clone()
        }
    };
    match scope {
        Scope::Systems => {
            let mut out: Vec<Item> = line
                .systems
                .iter()
                .map(|x| Item::new(&x.id, format!("{} — {} kV", system_name(x), x.voltage_kv)))
                .collect();
            // The wires without a system (2026-10-21): earthed wires and
            // sections without one, ticked like a system.
            let map = ohl_clearance::safety_volume::system_map(line);
            let n = line
                .sections
                .iter()
                .filter(|x| !map.contains_key(&x.id))
                .count();
            out.push(Item::new(
                NO_SYSTEM,
                match n {
                    0 => format!("{NO_SYSTEM} — earthed wires (none on this line)"),
                    1 => format!("{NO_SYSTEM} — earthed wires, 1 section"),
                    n => format!("{NO_SYSTEM} — earthed wires, {n} sections"),
                },
            ));
            out
        }
        Scope::Circuits => {
            let map = ohl_clearance::safety_volume::system_map(line);
            let mut names: Vec<&str> = line
                .sections
                .iter()
                .map(|x| x.circuit_name.as_str())
                .filter(|n| !n.is_empty())
                .collect();
            names.sort_unstable();
            names.dedup();
            names
                .into_iter()
                .map(|n| {
                    let system = line
                        .sections
                        .iter()
                        .filter(|x| x.circuit_name == n)
                        .find_map(|x| map.get(&x.id))
                        .and_then(|m| line.systems.iter().find(|x| x.id == m.id));
                    Item::new(
                        n,
                        match system {
                            Some(x) => {
                                format!("{n} — {}, {} kV", system_name(x), x.voltage_kv)
                            }
                            None => format!("{n} — earthed"),
                        },
                    )
                })
                .collect()
        }
        Scope::Spans => {
            let at = |id: &str| line.structure(id).map(|x| [x.position[0], x.position[1]]);
            let mut out = Vec::new();
            for x in &line.sections {
                for (k, w) in x.points.windows(2).enumerate() {
                    let length = match (at(&w[0].structure), at(&w[1].structure)) {
                        (Some(a), Some(b)) => {
                            format!(", {:.1} m", (b[0] - a[0]).hypot(b[1] - a[1]))
                        }
                        _ => String::new(),
                    };
                    out.push(Item::new(
                        format!("{}:{}", x.id, k + 1),
                        format!(
                            "{} span {}: {}–{}{length}",
                            x.id,
                            k + 1,
                            w[0].structure,
                            w[1].structure
                        ),
                    ));
                }
            }
            out
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let p = SafetyParams::default();
        Self {
            distance: p.distance,
            margin: p.margin,
            tolerance: DEFAULT_TOLERANCE,
            wind: WindMode::None,
            steps: WindSteps::default(),
            grouping: Grouping::Wire,
            scope: Scope::Circuits,
            circuits: None,
            systems: None,
            spans: None,
        }
    }
}

impl Settings {
    /// The parameters of a run (only the list of the scope chosen
    /// counts).
    pub fn params(&self) -> SafetyParams {
        let (mut circuits, mut systems, mut spans) = (Vec::new(), Vec::new(), Vec::new());
        match self.scope {
            Scope::Circuits => circuits = self.circuits.clone().unwrap_or_default(),
            Scope::Systems => systems = self.systems.clone().unwrap_or_default(),
            Scope::Spans => {
                spans = self
                    .spans
                    .iter()
                    .flatten()
                    .filter_map(|t| ohl_clearance::safety_volume::parse_span(t).ok())
                    .collect()
            }
        }
        SafetyParams {
            distance: self.distance.clone(),
            margin: self.margin,
            tolerance: self.tolerance,
            grouping: self.grouping,
            circuits,
            systems,
            spans,
            ..SafetyParams::default()
        }
    }

    /// What keeps a run from starting: nothing ticked in the list chosen.
    pub fn scope_problem(&self) -> Option<String> {
        match self.scope {
            Scope::Circuits if self.circuits.as_ref().is_some_and(Vec::is_empty) => {
                Some("tick a circuit (What is included)".into())
            }
            Scope::Systems if self.systems.as_ref().is_some_and(Vec::is_empty) => {
                Some("tick a system (What is included)".into())
            }
            Scope::Spans if self.spans.as_ref().is_some_and(Vec::is_empty) => {
                Some("tick a span (What is included)".into())
            }
            _ => None,
        }
    }

    /// As saved (JSON), with the weather cases' names.
    pub fn to_json(&self, cases: &[String]) -> Value {
        json!({
            "cases": cases,
            "distance": self.distance.to_toml(),
            "margin": self.margin,
            "tolerance": self.tolerance,
            "wind": self.wind.as_str(),
            "band_margin": self.steps.band_margin,
            "jumper_band_margin": self.steps.jumper_band_margin,
            "wind_step": self.steps.step,
            "wind_sides": self.steps.sides.as_str(),
            "per": self.grouping.as_str(),
            "scope": self.scope.as_str(),
            "circuits": self.circuits.clone().unwrap_or_default(),
            "systems": self.systems.clone().unwrap_or_default(),
            "spans": self.spans.clone().unwrap_or_default(),
        })
    }

    /// Read back from [`Settings::to_json`]: the settings and the cases.
    pub fn from_json(v: &Value) -> Result<(Self, Vec<String>), String> {
        let d = Self::default();
        let num = |k: &str, or: f64| v.get(k).and_then(Value::as_f64).unwrap_or(or);
        let text = |k: &str| v.get(k).and_then(Value::as_str);
        let list = |k: &str| -> Vec<String> {
            v.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let distance = match text("distance") {
            Some(t) => Requirement::parse(t).map_err(|e| format!("distance: {e}"))?,
            None => d.distance.clone(),
        };
        Ok((
            Self {
                distance,
                margin: num("margin", d.margin),
                tolerance: num("tolerance", d.tolerance),
                wind: text("wind").map_or(Ok(d.wind), WindMode::parse)?,
                steps: WindSteps {
                    stepped: false,
                    arc: false,
                    step: num("wind_step", d.steps.step),
                    sides: text("wind_sides")
                        .and_then(|s| WindSides::parse(s).ok())
                        .unwrap_or(WindSides::Both),
                    // Runs saved before 2026-10-21: the margin then fixed.
                    band_margin: num("band_margin", d.steps.band_margin),
                    // Runs saved before the jumpers' own margin
                    // (2026-10-21): their bands took the spans' margin.
                    jumper_band_margin: num(
                        "jumper_band_margin",
                        num("band_margin", d.steps.band_margin),
                    ),
                },
                grouping: text("per").map_or(Ok(d.grouping), Grouping::parse)?,
                // Runs saved before 2026-10-21: by circuit.
                scope: text("scope").map_or(Ok(Scope::Circuits), Scope::parse)?,
                circuits: Some(list("circuits")).filter(|l| !l.is_empty()),
                systems: Some(list("systems")).filter(|l| !l.is_empty()),
                spans: Some(list("spans")).filter(|l| !l.is_empty()),
            },
            list("cases"),
        ))
    }

    /// The wind in words, with the band's margin when the arc is taken.
    pub fn wind_text(&self) -> String {
        ohl_clearance::safety_volume::wind_text(self.wind, self.steps.margins())
    }

    /// What of `self` (the settings now) differs from `run`'s (a report's)
    /// that the report does not show by itself: the bands' margins with
    /// the arc on (the spans', the jumpers').
    pub fn changed_since(&self, run: &Settings) -> Option<String> {
        if !run.wind.arc() {
            return None;
        }
        let span = self.steps.band_margin != run.steps.band_margin;
        let jumper = self.steps.jumper_band_margin != run.steps.jumper_band_margin;
        match (span, jumper) {
            (true, true) => Some("the band margins".to_owned()),
            (true, false) => Some("the band margin".to_owned()),
            (false, true) => Some("the jumper band margin".to_owned()),
            (false, false) => None,
        }
    }

    /// The distance and margin in words.
    pub fn distance_text(&self) -> String {
        if self.margin > 0.0 {
            format!("{} + {:.2} m", self.distance, self.margin)
        } else {
            self.distance.to_string()
        }
    }
}

/// What a run gave (or a saved run holds).
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The volumes (without meshes when saved without them).
    pub volumes: Arc<SafetyVolumes>,
    /// Each volume's colour (its circuit's).
    pub colors: Vec<[u8; 3]>,
    /// The settings of the run.
    pub settings: Settings,
    /// The saved report it was read from, if any.
    pub saved: Option<String>,
    /// Whether it holds the meshes (a saved run may not).
    pub meshes: bool,
}

/// The colour of each volume: its first section's (the circuit's wire
/// colour, [`ohl_line::Line::section_color`]).
fn colors(model: &Model, v: &SafetyVolumes) -> Vec<[u8; 3]> {
    v.volumes
        .iter()
        .map(|s| {
            s.sections
                .first()
                .and_then(|id| model.line.sections.iter().find(|x| &x.id == id))
                .map_or([230, 120, 40], |x| {
                    crate::scene::rgb(&model.line.section_color(x))
                })
        })
        .collect()
}

/// What the views draw of a report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// The volumes (none: nothing drawn).
    pub volumes: Option<Arc<SafetyVolumes>>,
    /// Their colours.
    pub colors: Vec<[u8; 3]>,
    /// Each volume's outline seen from above (plan).
    pub outlines: Arc<Vec<Vec<[[f64; 3]; 2]>>>,
    /// The report is out of date: drawn dimmed.
    pub stale: bool,
}

impl Overlay {
    fn of(r: &Report) -> Self {
        Self {
            outlines: Arc::new(
                r.volumes
                    .volumes
                    .iter()
                    .map(|v| v.mesh.silhouette())
                    .collect(),
            ),
            volumes: Some(Arc::clone(&r.volumes)),
            colors: r.colors.clone(),
            stale: false,
        }
    }
}

/// What the window asks of the application.
#[derive(Default)]
pub struct Out {
    /// A command to apply (a run saved), with what it does.
    pub apply: Option<(String, ohl_session::Command)>,
    /// A message for the status bar (an error when true).
    pub status: Option<(String, bool)>,
    /// A section of the user guide to open (its anchor in [`GUIDE`]).
    pub guide: Option<&'static str>,
}

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "safety-volume-window";

/// The project setting holding the window's setup (per project line:
/// `app.safety_volumes.<line>`; owner, 2026-10-27).
pub const SETTING: &str = "app.safety_volumes";

/// A part of the window's settings, in the order of the setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// What is included (2 Scope).
    Included,
    /// The distance (3 Rules).
    Distance,
    /// The wire positions (4 Settings).
    Positions,
}

/// The Safety volumes window.
pub struct SafetyVolumeWindow {
    /// Shown (closed: its layer is taken off the views).
    pub open: bool,
    /// The structure and section overrides of its cases.
    pub overrides: crate::case_groups::Overrides,
    /// Weather cases ticked (indices in the model's cases).
    pub cases: Vec<usize>,
    /// The settings.
    pub settings: Settings,
    pub(crate) job: Job<Report>,
    /// Counts the reports taken (a new one is drawn).
    generation: u64,
    /// What the views draw now: shown, report.
    drawn: Option<(bool, u64)>,
    /// A saved run shown: what changed since it was saved.
    saved_stale: Option<String>,
    /// The name to save the run as, and what the last save did.
    save_name: String,
    save_message: Option<(String, bool)>,
    overwrite: Option<String>,
    /// The preset and rule chosen to fill the distance.
    preset: Option<(usize, usize)>,
    rule: usize,
    /// The formats ticked for export.
    pub formats: Vec<Format>,
    /// What the last export did.
    export_message: Option<(String, bool)>,
    /// The setup kept in the project ([`SETTING`]).
    kept: check_ui::Kept,
}

impl Default for SafetyVolumeWindow {
    fn default() -> Self {
        Self {
            open: false,
            overrides: Default::default(),
            cases: Vec::new(),
            settings: Settings::default(),
            job: Job::default(),
            generation: 0,
            drawn: None,
            saved_stale: None,
            save_name: "safety volumes".into(),
            save_message: None,
            overwrite: None,
            preset: None,
            rule: 0,
            formats: vec![Format::Dxf, Format::Ifc, Format::Kmz],
            export_message: None,
            kept: Default::default(),
        }
    }
}

impl SafetyVolumeWindow {
    /// Opens the window on the case shown (ticked, if nothing is).
    pub fn show(&mut self, case: usize) {
        self.open = true;
        if self.cases.is_empty() {
            self.cases.push(case);
        }
    }

    /// The report shown.
    pub fn report(&self) -> Option<&Report> {
        match &self.job.result {
            Some(Ok(r)) => Some(r),
            _ => None,
        }
    }

    /// The last run's answer, and whether one runs (tests).
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.job.result.as_ref(), self.job.running())
    }

    /// Sets the name a run is saved as (tests).
    #[cfg(test)]
    pub fn set_save_name(&mut self, name: &str) {
        self.save_name = name.to_owned();
    }

    /// What the views should draw, when it changed since the last call.
    pub fn overlay_change(&mut self) -> Option<Overlay> {
        let shown = self.open && self.report().is_some_and(|r| r.meshes);
        let key = (shown, self.generation);
        if self.drawn == Some(key) {
            return None;
        }
        self.drawn = Some(key);
        Some(match (shown, self.report()) {
            (true, Some(r)) => Overlay::of(r),
            _ => Overlay::default(),
        })
    }

    /// What the report shown was computed from that changed since, in
    /// words (`None`: nothing, or no report).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        let report = self.report()?;
        let line = match &self.saved_stale {
            Some(s) => Some(s.clone()),
            None => self.job.stamp.out_of_date(model, USES),
        };
        // The band's margin changed since (2026-10-21).
        let settings = self.settings.changed_since(&report.settings);
        match (line, settings) {
            (Some(a), Some(b)) => Some(format!("{a}, {b}")),
            (a, b) => a.or(b),
        }
    }

    /// Starts a run on the model as it is.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let cases: Vec<ohl_weather::WeatherCase> = self
            .cases
            .iter()
            .filter_map(|&k| model.cases.get(k).map(|c| c.case.clone()))
            .collect();
        if cases.is_empty() {
            self.job.result = Some(Err("tick a weather case".into()));
            return;
        }
        if let Some(p) = self.settings.scope_problem() {
            self.job.result = Some(Err(p));
            return;
        }
        let params = self.settings.params();
        if let Some(p) = params.problem() {
            self.job.result = Some(Err(p));
            return;
        }
        self.saved_stale = None;
        let line = model.line.clone();
        let library = model.library.clone();
        let settings = self.settings.clone();
        // The colours are the line's: taken now, kept with the report.
        let sections: Vec<(String, [u8; 3])> = model
            .line
            .sections
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    crate::scene::rgb(&model.line.section_color(s)),
                )
            })
            .collect();
        let filter = self.overrides.filter(model);
        self.job.start_at(ctx, model.revision(), move || {
            // Exit clause: an hour is far beyond what a line needs.
            let budget =
                ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(3600));
            let v = ohl_clearance::safety_volume::run(
                &line,
                &library,
                &cases,
                settings.wind,
                settings.steps,
                &params,
                &budget,
                &filter,
            )?;
            let colors = v
                .volumes
                .iter()
                .map(|s| {
                    s.sections
                        .first()
                        .and_then(|id| sections.iter().find(|x| &x.0 == id))
                        .map_or([230, 120, 40], |x| x.1)
                })
                .collect();
            Ok(Report {
                volumes: Arc::new(v),
                colors,
                settings,
                saved: None,
                meshes: true,
            })
        });
    }

    /// The saved run `r` shown (its settings, cases and volumes), with
    /// what changed since it was saved.
    pub fn load_saved(&mut self, r: &CheckReport, model: &Model) -> Result<(), String> {
        let (settings, cases) = Settings::from_json(&r.parameters)?;
        let mut volumes = saved::read(&r.results)?;
        volumes.cases = cases.clone();
        let meshes = volumes.volumes.iter().any(|v| !v.mesh.triangles.is_empty());
        self.saved_stale = None;
        if let Some(s) = model.session()
            && let Ok(now) = s.check_state()
            && !reports::changed_since(&r.state, &now).is_empty()
        {
            self.saved_stale = Some("the line".into());
        }
        // The ice and wind per stretched or unstretched metre (2026-10-21).
        if let Some(o) = crate::revision::saved_loads_per_changed(&r.parameters, model) {
            self.saved_stale = Some(match self.saved_stale.take() {
                Some(l) => format!("{l} and {o}"),
                None => o,
            });
        }
        self.job
            .stamp
            .set(crate::revision::saved_revision(&r.parameters, model));
        self.settings = settings.clone();
        self.cases = cases
            .iter()
            .filter_map(|n| model.cases.iter().position(|c| &c.case.name == n))
            .collect();
        self.save_name = r.id.clone();
        let colors = colors(model, &volumes);
        self.job.result = Some(Ok(Report {
            volumes: Arc::new(volumes),
            colors,
            settings,
            saved: Some(r.id.clone()),
            meshes,
        }));
        self.generation += 1;
        self.open = true;
        Ok(())
    }

    /// The run shown as a report named `name` of the performed checks.
    fn to_check_report(&self, model: &Model, name: &str) -> Result<CheckReport, String> {
        let report = self.report().ok_or("run it first")?;
        let editing = model
            .editing
            .as_ref()
            .map_err(|e| format!("read-only: {e}"))?;
        let s = &editing.session;
        let mut parameters = report.settings.to_json(&report.volumes.cases);
        parameters["version"] = json!(SAVED_VERSION);
        // The ice and wind per stretched or unstretched metre (2026-10-21).
        parameters["loads_per"] = json!(self.job.stamp.loads_per(model).as_str());
        let triangles: usize = report
            .volumes
            .volumes
            .iter()
            .map(|v| v.mesh.triangles.len())
            .sum();
        Ok(CheckReport {
            format: reports::REPORT_FILE_FORMAT.into(),
            version: reports::REPORT_FILE_VERSION,
            id: name.to_owned(),
            check: CHECK_ID.into(),
            title: CHECK_TITLE.into(),
            time: s.now().map_err(|e| e.to_string())?,
            user: editing.user.clone(),
            app: format!("OHL-Design {}", env!("CARGO_PKG_VERSION")),
            plugin: None,
            status: ReportStatus::Draft,
            note: s.report(name).map(|o| o.note).unwrap_or_default(),
            state: s.check_state().map_err(|e| e.to_string())?,
            summary: format!(
                "{} volumes, {} from the conductors, {}; {} triangles",
                report.volumes.volumes.len(),
                report.settings.distance_text(),
                report.settings.wind_text(),
                triangles
            ),
            parameters,
            results: saved::results(&report.volumes, triangles <= MAX_SAVED_TRIANGLES)?,
        })
    }

    /// Writes the volumes shown in the formats ticked into `dir`.
    pub fn export_to(&mut self, model: &Model, dir: &std::path::Path) -> Result<String, String> {
        let report = self.report().ok_or("run it first")?;
        if !report.meshes {
            return Err("this saved run holds no meshes: run it again".into());
        }
        if self.formats.is_empty() {
            return Err("tick a format".into());
        }
        let (date, _) = ohl_io::landxml::utc_now();
        let d = Description {
            line: model.line.name.clone(),
            crs: model.line.crs.clone(),
            distance: report.settings.distance_text(),
            wind: report.settings.wind_text(),
            date,
        };
        let stem: String = format!("{} safety volume", model.line.name)
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let (files, notes) =
            ohl_io::safety_volume::write(dir, &stem, &report.volumes, &d, &self.formats)?;
        let mut text = format!(
            "written: {}",
            files
                .iter()
                .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for n in notes {
            text.push_str(&format!("; {n}"));
        }
        Ok(text)
    }

    /// The window; what it asks of the app.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
    ) -> Out {
        let was = self.job.running();
        self.job.poll();
        if was && !self.job.running() {
            self.generation += 1;
        }
        let mut out = Out::default();
        if !self.open {
            return out;
        }
        let mut open = true;
        let mut again = false;
        let stale = self.out_of_date(model);
        let why = self.why_not_run(model);
        let findings = self.report().map(|r| r.volumes.volumes.len());
        help::window(
            ctx,
            crate::dockable::Panel::SafetyVolumes,
            CHECK_TITLE,
            [860.0, 760.0],
            &mut open,
            |ui| {
                if let Some(what) = &stale {
                    again = crate::revision::banner(ui, what, !self.job.running() && why.is_none());
                }
                // Two tabs (owner, 2026-10-27): the setup scrolls, the
                // results fill theirs; Run in the footer, always visible.
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    findings,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "The space within the distance of the wires in every position of the \
                             cases ticked, for work near the line: one closed volume per wire, \
                             circuit or system, shown in the 3D view and the plan, exported as \
                             DXF, IFC or KML.",
                            "No safety distance is built in: type yours (or take a design-code \
                             plugin's preset) in 3 Rules.",
                        );
                        check_ui::how_computed(
                            ui,
                            WINDOW,
                            "The wires are solved in every position of the cases ticked (still \
                             air, the blowouts, the wind steps or the swing arc). Around each \
                             position the space within the distance plus the margin of the \
                             conductors' surface is a tube; the tubes of a wire, circuit or \
                             system are joined into one closed mesh that holds the exact volume \
                             whole and lies outside it by at most the tolerance where it is \
                             smooth (4 Settings).",
                            |ui| {
                                ui.horizontal(|ui| {
                                    help::safety_distance(ui, 4.0, 1.0);
                                    help::mesh_outside(ui, self.settings.tolerance);
                                });
                            },
                        );
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        let names: Vec<String> =
                            model.cases.iter().map(|c| c.case.name.clone()).collect();
                        check_ui::ScopePicker::new(WINDOW, &model.line)
                            .cases(check_ui::Cases::Many(&names, &mut self.cases))
                            .ui(ui);
                        self.settings_ui(ui, model, presets, &mut out, Part::Included);
                        ui.separator();
                        check_ui::section(ui, 3, "Rules", None);
                        self.settings_ui(ui, model, presets, &mut out, Part::Distance);
                        ui.separator();
                        check_ui::section(ui, 4, "Settings", None);
                        self.settings_ui(ui, model, presets, &mut out, Part::Positions);
                        ui.separator();
                        if let Some(a) = help::heading(ui, "Export", "safety-volumes-export") {
                            out.guide = Some(a);
                        }
                        self.export_ui(ui, model);
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| match &self
                        .job
                        .result
                    {
                        None => {
                            ui.label(egui::RichText::new("No run yet: Run at the bottom.").weak());
                        }
                        Some(Err(e)) => {
                            crate::menu::error_label(ui, e);
                        }
                        Some(Ok(_)) => {
                            // A few volumes: the table, the save and the
                            // export scroll together in the tab.
                            egui::ScrollArea::vertical()
                                .id_salt("safety-volume-results")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.report_ui(ui);
                                    self.save_ui(ui, model, &mut out);
                                    if let Some(a) =
                                        help::heading(ui, "Export", "safety-volumes-export")
                                    {
                                        out.guide = Some(a);
                                    }
                                    self.export_ui(ui, model);
                                });
                        }
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    let overrides = &mut self.overrides;
                    again |= check_ui::job_footer(
                        ui,
                        &mut self.job,
                        why.clone(),
                        "Solve the cases ticked and build the volumes (in the background)",
                        None,
                        |ui| {
                            overrides.ui(ui);
                        },
                    );
                });
            },
        );
        self.open = open;
        if again && why.is_none() {
            self.start(model, ctx);
        }
        out
    }

    /// Why the volumes cannot be built, if they cannot ("Before Run: …").
    pub fn why_not_run(&self, model: &Model) -> Option<String> {
        if !self.cases.iter().any(|&k| k < model.cases.len()) {
            return Some("tick a weather case (2 Scope)".into());
        }
        if let Some(p) = self.settings.scope_problem() {
            return Some(p);
        }
        self.settings
            .distance
            .problem()
            .map(|p| format!("the distance: {p} (3 Rules)"))
    }

    /// The setup as kept in the project: the settings and the cases (by
    /// name), the overrides (JSON).
    fn stored(&self, model: &Model) -> String {
        let cases: Vec<String> = self
            .cases
            .iter()
            .filter_map(|&k| model.cases.get(k).map(|c| c.case.name.clone()))
            .collect();
        serde_json::json!({
            "settings": self.settings.to_json(&cases),
            "overrides": [self.overrides.structures, self.overrides.sections],
        })
        .to_string()
    }

    /// Takes the setup kept in the project (none: as it is).
    fn take_stored(&mut self, model: &Model, text: Option<&str>) {
        let Some(v) = text.and_then(|t| serde_json::from_str::<Value>(t).ok()) else {
            return;
        };
        if let Some(s) = v.get("settings")
            && let Ok((settings, cases)) = Settings::from_json(s)
        {
            self.settings = settings;
            let found: Vec<usize> = cases
                .iter()
                .filter_map(|n| model.cases.iter().position(|c| &c.case.name == n))
                .collect();
            if !found.is_empty() {
                self.cases = found;
            }
        }
        if let Some([a, b]) = v
            .get("overrides")
            .and_then(|o| serde_json::from_value::<[bool; 2]>(o.clone()).ok())
        {
            self.overrides.structures = a;
            self.overrides.sections = b;
        }
    }

    /// Reads the setup from the project when not read yet for the line
    /// worked on, and writes it into it when it changed (owner,
    /// 2026-10-27: project setting [`SETTING`] per project line).
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        let json = self.stored(model);
        let mut kept = std::mem::take(&mut self.kept);
        if let check_ui::Step::Load(text) =
            kept.step(model, ctx, SETTING, "safety volume settings", &json)
        {
            self.take_stored(model, text.as_deref());
            kept.loaded(self.stored(model));
        }
        self.kept = kept;
    }

    /// A part of the settings: `part`.
    fn settings_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
        part: Part,
    ) {
        let voltages = ohl_clearance::zone::line_voltages(&model.line);
        let mut heading = |ui: &mut egui::Ui, title: &str, anchor: &'static str| {
            if let Some(a) = help::heading(ui, title, anchor) {
                out.guide = Some(a);
            }
        };
        let cases = &mut self.cases;
        let s = &mut self.settings;
        // Wire positions: the wind (the cases in the scope).
        if part == Part::Positions {
            heading(ui, "Wire positions", "safety-volumes-wire-positions");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                help::explain(
                    ui,
                    "Each case ticked: still air as it is, a windy case blown to the left and to \
                     the right at its full pressure.",
                    "The wires are solved in each case ticked by the line's solver; a case with \
                     wind is solved with the wind from the right (blown left) and from the left \
                     (blown right). The volume holds every position.",
                );
                // The wind choice shared with the danger-tree check (the
                // full blowouts from both sides; the sides for the steps).
                let full = cases
                    .iter()
                    .filter_map(|&k| model.cases.get(k))
                    .map(|c| c.case.wind_pressure.si())
                    .fold(0.0, f64::max);
                let mut wind = WindSteps {
                    arc: s.wind.arc(),
                    stepped: s.wind.stepped(),
                    ..s.steps
                };
                help::wind_choice(ui, "safety-volume", &mut wind, false, full, true);
                s.wind = match (wind.arc, wind.stepped) {
                    (false, false) => WindMode::None,
                    (false, true) => WindMode::Steps,
                    (true, false) => WindMode::Arc,
                    (true, true) => WindMode::Both,
                };
                s.steps = WindSteps {
                    arc: false,
                    stepped: false,
                    ..wind
                };
            });
                let n = if s.wind.stepped() {
                    let full = cases
                        .iter()
                        .filter_map(|&k| model.cases.get(k))
                        .map(|c| c.case.wind_pressure.si())
                        .fold(0.0, f64::max);
                    (full / s.steps.step.max(1.0)).ceil() as usize
                } else {
                    0
                };
                help::wind_positions(ui, s.wind.arc(), n);
            });
        }
        // Distance: the rule, a preset's, the margin.
        if part == Part::Distance {
            heading(ui, "Distance", "safety-volumes-distance");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                ui.label("from the conductors' surface");
                crate::requirement_ui::requirement_ui_stacked(
                    ui,
                    "safety-volume-distance",
                    &mut s.distance,
                    &voltages,
                );
                help::explain(
                    ui,
                    "No safety distance is built in: enter the one of the rules you work to, or \
                     fill it from a preset's rule below.",
                    "One distance, a table by voltage or a base plus an amount per kV: each wire \
                     takes the distance of its own system's nominal voltage (earthed wires: \
                     0 kV), measured from its surface (each subconductor of a bundle, iced in a \
                     case with ice).",
                );
                // A preset's rule fills the distance (its radial distance,
                // else its vertical one).
                ui.horizontal(|ui| {
                    ui.label("preset");
                    if crate::rule_editor::preset_dropdown(
                        ui,
                        "safety-volume-preset",
                        presets,
                        |p| {
                            p.rules
                                .rules
                                .iter()
                                .any(|r| r.radial.is_some() || r.vertical.is_some())
                        },
                        &mut self.preset,
                    ) {
                        self.rule = 0;
                    }
                });
                if let Some(preset) = self
                    .preset
                    .and_then(|(i, k)| presets.get(i)?.presets.as_ref().ok()?.get(k).cloned())
                {
                    let rules: Vec<(String, Requirement)> = preset
                        .rules
                        .rules
                        .iter()
                        .filter_map(|r| {
                            r.radial
                                .clone()
                                .or_else(|| r.vertical.clone())
                                .map(|q| (r.name.clone(), q))
                        })
                        .collect();
                    ui.horizontal(|ui| {
                        ui.label("rule");
                        let shown = rules.get(self.rule).map_or("-", |r| r.0.as_str());
                        egui::ComboBox::from_id_salt("safety-volume-preset-rule")
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for (k, (name, q)) in rules.iter().enumerate() {
                                    ui.selectable_value(&mut self.rule, k, format!("{name}: {q}"));
                                }
                            });
                        if let Some((name, q)) = rules.get(self.rule)
                            && ui
                                .button("Fill the distance")
                                .on_hover_text(format!(
                                    "The rule \"{name}\"'s radial distance (else its vertical \
                                     one): {q}"
                                ))
                                .clicked()
                        {
                            s.distance = q.clone();
                        }
                    });
                    if let Some((_, q)) = rules.get(self.rule) {
                        crate::requirement_ui::requirement_view(
                            ui,
                            "safety-volume-preset-rule",
                            q,
                            &voltages,
                        );
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("margin");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut s.margin)
                            .speed(0.05)
                            .range(0.0..=50.0)
                            .suffix(" m"),
                    )
                    .on_hover_text(
                        "Added to the distance (e.g. for the survey's accuracy or tools)",
                    );
                });
                help::explain(
                    ui,
                    "Added to the distance, for every wire.",
                    "The margin covers what the distance does not: the survey's accuracy, tools \
                     or loads swinging near the line.",
                );
            });
                let d = voltages
                    .iter()
                    .filter_map(|kv| s.distance.at(*kv).ok())
                    .fold(0.0, f64::max);
                help::safety_distance(ui, d, s.margin);
            });
        }
        // What is included: the circuits or systems, the grouping.
        if part == Part::Included {
            heading(ui, "What is included", "safety-volumes-what-is-included");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("scope by");
                    for (scope, tip) in [
                        (
                            Scope::Systems,
                            "Tick the line's electrical systems; the row (no system) holds the \
                             earthed wires and any section without a system",
                        ),
                        (Scope::Circuits, "Tick the line's circuits"),
                        (
                            Scope::Spans,
                            "Tick spans: every wire on them, and the jumpers whose two ends are \
                             on spans ticked",
                        ),
                    ] {
                        if ui
                            .add(
                                egui::Button::new(scope.title())
                                    .selected(s.scope == scope)
                                    .frame_when_inactive(true),
                            )
                            .on_hover_text(tip)
                            .clicked()
                        {
                            s.scope = scope;
                        }
                    }
                });
                let items = scope_items(&model.line, s.scope);
                let chosen = match s.scope {
                    Scope::Systems => &mut s.systems,
                    Scope::Circuits => &mut s.circuits,
                    Scope::Spans => &mut s.spans,
                };
                crate::tick_list::tick_list(
                    ui,
                    ("safety-volume-scope", s.scope.as_str()),
                    &items,
                    chosen,
                    130.0,
                );
                if let Some(p) = s.scope_problem() {
                    ui.colored_label(ui.visuals().warn_fg_color, egui::RichText::new(p).small());
                }
                help::explain(
                    ui,
                    "The wires of the systems, circuits or spans ticked.",
                    "All with nothing searched: every wire of the line, also of what is added \
                     later. By system, the row (no system) is the earthed wires and any section \
                     without a system (at 0 kV), ticked like a system. By span: every wire on \
                     the spans ticked, and a jumper when the spans at both its ends are \
                     ticked.",
                );
                ui.horizontal(|ui| {
                    ui.label("one volume per");
                    ui.radio_value(&mut s.grouping, Grouping::Wire, "wire")
                        .on_hover_text("A circuit's phase through all its sections");
                    ui.radio_value(&mut s.grouping, Grouping::Circuit, "circuit")
                        .on_hover_text("A circuit's phases together");
                    ui.radio_value(&mut s.grouping, Grouping::System, "system")
                        .on_hover_text(
                            "Every circuit of one electrical system together (the wires without \
                             a system together: (no system))",
                        );
                });
                help::explain(
                    ui,
                    "How the wires are grouped into closed volumes; each wire keeps the distance \
                     of its own voltage.",
                    "Per wire: a circuit's phase through all its sections. Per circuit: its \
                     phases in one mesh. Per system: every circuit of the system in one mesh, \
                     and the wires without a system (earthed wires) together in one named (no \
                     system). A volume of several wires is the union of theirs.",
                );
                help::explain(
                    ui,
                    "Jumpers included, structures not.",
                    "The jumpers at their solved positions in every case and wind position, so \
                     the volume is continuous at the structures; with the swing arc, each jumper \
                     also on a swing band of its own about the line through its two end clamps, \
                     widened by the band margin. The structures and their strings are not part \
                     of the volumes.",
                );
            });
                help::volume_grouping(ui, s.grouping);
            });
            ui.separator();
            // Mesh: the tolerance.
            heading(ui, "Mesh", "safety-volumes-mesh");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("tolerance");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut s.tolerance)
                            .speed(0.005)
                            .range(0.005..=1.0)
                            .suffix(" m"),
                    )
                    .on_hover_text(
                        "How far the mesh may lie outside the exact volume where it is smooth; \
                         it is never inside it. Smaller: more triangles.",
                    );
                });
                help::explain(
                    ui,
                    "The mesh holds the exact volume whole; outside it by at most this where it \
                     is smooth.",
                    "Smaller: more triangles, a longer run, larger files. The results give each \
                     volume's \"outside by at most\", measured: where two tubes meet, the \
                     triangles bridge the groove between them (up to one grid cell), so it can \
                     be larger than the tolerance.",
                );
            });
                help::mesh_outside(ui, s.tolerance);
            });
        }
    }

    fn report_ui(&self, ui: &mut egui::Ui) {
        let Some(r) = self.report() else { return };
        let v = &r.volumes;
        ui.label(format!(
            "{} volumes from {} wire positions; weather cases: {}; {} from the conductors, {}.",
            v.volumes.len(),
            v.positions,
            v.cases.join(", "),
            r.settings.distance_text(),
            r.settings.wind_text()
        ));
        if !r.meshes {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Saved without its meshes (too large): run it again to see and export them.",
            );
        }
        egui::Grid::new("safety-volume-table")
            .striped(true)
            .num_columns(7)
            .show(ui, |ui| {
                for h in [
                    "volume",
                    "voltage",
                    "distance, m",
                    "triangles",
                    "volume, m³",
                    "outside by ≤, m",
                    "time, s",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for (s, c) in v.volumes.iter().zip(&r.colors) {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            egui::Color32::from_rgb(c[0], c[1], c[2]),
                        );
                        ui.label(&s.name);
                    });
                    ui.label(s.kv.map_or("earthed".into(), |k| format!("{k} kV")));
                    ui.label(format!("{:.2}", s.distance + s.margin));
                    ui.label(s.mesh.triangles.len().to_string());
                    ui.label(if r.meshes {
                        format!("{:.0}", s.mesh.volume())
                    } else {
                        "-".into()
                    });
                    ui.label(format!("{:.3}", s.allowance));
                    ui.label(format!("{:.2}", s.seconds));
                    ui.end_row();
                }
            });
        ui.label(
            egui::RichText::new(
                "Each mesh holds the whole exact volume (never inside it) and lies outside it by \
                 at most the value shown where the volume is smooth; where tubes meet, its \
                 triangles may bridge the groove by up to one grid cell.",
            )
            .small()
            .weak(),
        );
        if !v.notes.is_empty() {
            help::list(
                ui,
                "safety-volume-list-2",
                110.0,
                egui::ScrollArea::vertical().id_salt("safety-volume-notes"),
                |ui| {
                    for n in &v.notes {
                        ui.label(egui::RichText::new(n).small());
                    }
                },
            );
        }
    }

    fn save_ui(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        ui.horizontal(|ui| {
            ui.label("Save in the library of performed checks as");
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(160.0));
            let exists = model
                .session()
                .is_some_and(|s| s.report(self.save_name.trim()).is_some());
            let asked = self.overwrite.as_deref() == Some(self.save_name.trim());
            let label = if exists && asked {
                "Replace it"
            } else {
                "Save in Performed checks"
            };
            if ui
                .button(label)
                .on_hover_text(
                    "Kept with the project line; reopened from Checks › Performed checks",
                )
                .clicked()
            {
                let name = self.save_name.trim().to_owned();
                if name.is_empty() {
                    self.save_message = Some(("give it a name".into(), true));
                } else if exists && !asked {
                    self.overwrite = Some(name.clone());
                    self.save_message = Some((
                        format!("\"{name}\" exists: click again to replace it"),
                        true,
                    ));
                } else {
                    self.overwrite = None;
                    match self.to_check_report(model, &name) {
                        Ok(report) => {
                            out.apply =
                                Some((format!("saved the safety volumes {name}"), report.put()));
                            self.save_message = Some((format!("saved as \"{name}\""), false));
                            if let Some(Ok(rep)) = &mut self.job.result {
                                rep.saved = Some(name);
                            }
                        }
                        Err(e) => self.save_message = Some((e, true)),
                    }
                }
            }
        });
        if let Some((m, error)) = &self.save_message {
            crate::menu::message(
                ui,
                egui::RichText::new(m).small(),
                error.then_some(ui.visuals().warn_fg_color),
            );
        }
    }

    fn export_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let ready = self.report().is_some_and(|r| r.meshes);
        ui.add_enabled_ui(ready, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (f, name, hover) in [
                    (
                        Format::Dxf,
                        "DXF + .prj",
                        "3D faces in the line's coordinates, a layer per volume; the coordinate \
                         system as WKT beside it",
                    ),
                    (
                        Format::Ifc,
                        "IFC4",
                        "One element per volume with its properties; georeferenced by a map \
                         conversion to the projected system (EPSG code)",
                    ),
                    (
                        Format::Kml,
                        "KML",
                        "WGS84 with absolute heights (the line's, not corrected to a geoid)",
                    ),
                    (Format::Kmz, "KMZ", "The KML zipped"),
                ] {
                    let mut on = self.formats.contains(&f);
                    if ui.checkbox(&mut on, name).on_hover_text(hover).changed() {
                        if on {
                            self.formats.push(f);
                        } else {
                            self.formats.retain(|x| *x != f);
                        }
                    }
                }
                if ui
                    .button("Export to a folder…")
                    .on_hover_text("Write the formats ticked into a folder chosen")
                    .clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.export_message = Some(match self.export_to(model, &dir) {
                        Ok(m) => (m, false),
                        Err(e) => (e, true),
                    });
                }
            });
        });
        help::explain(
            ui,
            if ready {
                "The volumes shown, in the formats ticked."
            } else {
                "Run first (a saved run without meshes: run it again)."
            },
            "DXF for CAD, IFC for BIM models, KML/KMZ for Google Earth. Each file carries the \
             line, the distance, the wind and the date.",
        );
        help::explain(
            ui,
            &format!(
                "Georeferenced in the line's coordinate system: {}",
                if model.line.crs.is_empty() {
                    "none set (KML needs one)"
                } else {
                    model.line.crs.as_str()
                }
            ),
            "DXF: the line's coordinates, the system as WKT in the .prj. IFC: a map conversion \
             to the projected system named by its EPSG code. KML: converted to WGS84, heights \
             kept as they are (above the line's height datum, not corrected to a geoid). Set \
             the system in the line's properties (CRS).",
        );
        if let Some((m, error)) = &self.export_message {
            crate::menu::message(
                ui,
                egui::RichText::new(m).small(),
                error.then_some(ui.visuals().warn_fg_color),
            );
        }
    }
}

/// What the app does with the window's asks.
pub fn handle(r: &mut crate::app::Ready, out: Out) {
    if let Some((what, command)) = out.apply {
        r.perform(crate::edit::Action::Command { what, command });
    }
    if let Some(s) = out.status {
        r.status = Some(s);
    }
    if let Some(anchor) = out.guide {
        r.status = Some(match crate::help::open_section(GUIDE, anchor) {
            Ok(m) => (format!("user guide opened: {m}"), false),
            Err(e) => (e, true),
        });
    }
}

/// The 3D layer: each volume's triangles translucent in its colour (its
/// circuit's), dimmed while out of date.
pub fn scene(origin: [f64; 3], overlay: &Overlay) -> ohl_render::Scene {
    let mut s = ohl_render::Scene::new(origin);
    let Some(v) = &overlay.volumes else {
        return s;
    };
    for (vol, rgb) in v.volumes.iter().zip(&overlay.colors) {
        let c = crate::revision::dimmed([rgb[0], rgb[1], rgb[2], 70], overlay.stale);
        for t in &vol.mesh.triangles {
            s.triangle(vol.mesh.corners(t), [c; 3]);
        }
    }
    s
}

/// The plan: each volume's outline seen from above, in its colour.
pub fn paint_plan(
    painter: &egui::Painter,
    rect: egui::Rect,
    to_screen: &impl Fn([f64; 3]) -> egui::Pos2,
    overlay: &Overlay,
) {
    for (edges, rgb) in overlay.outlines.iter().zip(&overlay.colors) {
        let c = crate::revision::dimmed32(
            egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
            overlay.stale,
        );
        let stroke = egui::Stroke::new(1.0, c);
        for [a, b] in edges {
            let (p, q) = (to_screen(*a), to_screen(*b));
            if rect.contains(p) || rect.contains(q) {
                painter.line_segment([p, q], stroke);
            }
        }
    }
}

/// The saved form of a run: its volumes (name, circuit, voltage,
/// distances, statistics) and, up to [`MAX_SAVED_TRIANGLES`], their
/// meshes packed ([`ohl_session::packing`]: vertices in mm as differences,
/// triangle corners as differences).
pub mod saved {
    use super::*;

    fn pack_mesh(m: &Mesh) -> Result<String, String> {
        let mm = |v: f64| (v * 1000.0).round() as i64;
        let mut w = Writer::default();
        w.u(m.vertices.len() as u64);
        for k in 0..3 {
            w.deltas(m.vertices.iter().map(|p| mm(p[k])));
        }
        w.u(m.triangles.len() as u64);
        for k in 0..3 {
            w.deltas(m.triangles.iter().map(|t| i64::from(t[k])));
        }
        ohl_session::packing::pack(&w.0)
    }

    fn unpack_mesh(text: &str) -> Result<Mesh, String> {
        let raw = ohl_session::packing::unpack(text, 1 << 31)?;
        let mut r = Reader(&raw);
        let n = usize::try_from(r.u()?).map_err(|e| e.to_string())?;
        if n > raw.len() {
            return Err(format!("{n} vertices in {} bytes", raw.len()));
        }
        let cols: Vec<Vec<i64>> = (0..3).map(|_| r.deltas(n)).collect::<Result<_, _>>()?;
        let vertices = (0..n)
            .map(|i| [0, 1, 2].map(|k| cols[k][i] as f64 / 1000.0))
            .collect();
        let m = usize::try_from(r.u()?).map_err(|e| e.to_string())?;
        if m > raw.len() {
            return Err(format!("{m} triangles in {} bytes", raw.len()));
        }
        let cols: Vec<Vec<i64>> = (0..3).map(|_| r.deltas(m)).collect::<Result<_, _>>()?;
        let mut triangles = Vec::with_capacity(m);
        for ((&a, &b), &c) in cols[0].iter().zip(&cols[1]).zip(&cols[2]) {
            let t = [a, b, c];
            if t.iter().any(|&x| x < 0 || x as usize >= n) {
                return Err("a triangle's corner out of range".into());
            }
            triangles.push(t.map(|x| x as u32));
        }
        if !r.done() {
            return Err("bytes left over".into());
        }
        Ok(Mesh {
            vertices,
            triangles,
        })
    }

    /// The results of a run (`meshes`: with them).
    pub fn results(v: &SafetyVolumes, meshes: bool) -> Result<Value, String> {
        let list: Vec<Value> = v
            .volumes
            .iter()
            .map(|s| -> Result<Value, String> {
                Ok(json!({
                    "name": s.name,
                    "circuit": s.circuit,
                    "phases": s.phases,
                    "sections": s.sections,
                    "kv": s.kv,
                    "distance": s.distance,
                    "margin": s.margin,
                    "outer": s.outer,
                    "positions": s.positions,
                    "arc_error": s.arc_error,
                    "arc_error_median": s.arc_error_median,
                    "allowance": s.allowance,
                    "cell": s.stats.cell,
                    "level": s.stats.level,
                    "triangles": s.mesh.triangles.len(),
                    "volume_m3": s.mesh.volume(),
                    "seconds": s.seconds,
                    "mesh": if meshes { Some(pack_mesh(&s.mesh)?) } else { None },
                }))
            })
            .collect::<Result<_, _>>()?;
        Ok(json!({
            "volumes": list,
            "positions": v.positions,
            "notes": v.notes,
        }))
    }

    /// Read back from [`results`] (the cases come from the parameters).
    pub fn read(v: &Value) -> Result<SafetyVolumes, String> {
        let num = |o: &Value, k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let text = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let list = |o: &Value, k: &str| -> Vec<String> {
            o.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut volumes = Vec::new();
        for o in v
            .get("volumes")
            .and_then(Value::as_array)
            .ok_or("no volumes in the saved run")?
        {
            let mesh = match o.get("mesh").and_then(Value::as_str) {
                Some(t) => unpack_mesh(t)?,
                None => Mesh::default(),
            };
            volumes.push(SafetyVolume {
                name: text(o, "name"),
                circuit: text(o, "circuit"),
                phases: list(o, "phases"),
                sections: list(o, "sections"),
                kv: o.get("kv").and_then(Value::as_f64),
                distance: num(o, "distance"),
                margin: num(o, "margin"),
                outer: num(o, "outer"),
                positions: num(o, "positions") as usize,
                arc_error: num(o, "arc_error"),
                arc_error_median: num(o, "arc_error_median"),
                simplified: Default::default(),
                timings: [0.0; 2],
                allowance: num(o, "allowance"),
                stats: MeshStats {
                    cell: num(o, "cell"),
                    level: num(o, "level"),
                    ..MeshStats::default()
                },
                mesh,
                seconds: num(o, "seconds"),
            });
        }
        Ok(SafetyVolumes {
            volumes,
            cases: Vec::new(),
            positions: v.get("positions").and_then(Value::as_u64).unwrap_or(0) as usize,
            notes: list(v, "notes"),
        })
    }
}
