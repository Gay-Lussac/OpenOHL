// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Ground clearances › **Vertical clearances…** and **Radial
//! clearances…**: a window each with the zone around the wires, a list of
//! rules (voltage, weather cases, distance, LiDAR classes and the ground),
//! and the wind — the final state of each case, or every wind pressure
//! from 0 up to it in steps, from the left and/or the right
//! ([`ohl_clearance::zone`]); the circuits checked, the calculation
//! method and the distance beyond which LiDAR points are ignored. The
//! rules are kept with the project, in a rule file of its clearance rules
//! (`<model>.vertical-clearances.ohlr`, `<model>.radial-clearances.ohlr`;
//! next to a line file), which `ohl line --rules` reads too.
//! **Preset ▾** (the shared [`crate::check_ui::RuleTable`]) takes the rules of a design code's preset (kept in the
//! project's rule files when the code generated, with a weather case file
//! of the same name: the cases its rules name, matched to the project's by
//! physics first, then by name, and created where none is the same:
//! [`crate::preset_cases`]).
//! The check runs on its own thread: the wind positions are solved, the
//! LiDAR points read again from the point cloud around the wires; its
//! progress and the time left are shown, and **Stop** keeps the previous
//! results.
//!
//! The settings are grouped and explained as in the vegetation panel
//! (the shared [`crate::check_help`]): a heading per group with a link to
//! its user-guide section, a line beside each value with a longer text
//! on hover, small drawings of the zone and the wind positions, and the
//! same wind choice — none, the wind steps, the swing arc, or both — with
//! the solves it means.
//! `ohl zones` runs the same check without the window
//! ([`ohl_clearance::zone::run_zone_check`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::check_ui;
use ohl_clearance::rules::Requirement;
use ohl_clearance::zone::{self, Imported, Measure, WindState, Zone, ZoneFinding, ZoneRule};
use ohl_clearance::{CheckScope, RuleSet, WindSteps};
use ohl_core::budget::{Budget, CancelToken};
use ohl_line::condition::Method;

use crate::check_help as help;
use crate::model::Model;
use crate::selection::Selection;

/// The user guide's page of the windows' settings.
pub const GUIDE: &str = "user-guide/desktop-app";

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::ALL;

// Exit clause of a check: solving every wind position and reading the
// points, 900 s and 20 s more per million LiDAR points of the rules'
// classes (`zone::time_limit`, owner 2026-10-27: the limit scales with
// the work; said before Run and when it is reached).

/// What a check found.
pub type Report = zone::ZoneCheckResult;

/// A finding clicked: the span (or jumper) and its point, and the weather
/// case and wind position it was found in.
#[derive(Debug, Clone)]
pub struct Picked {
    /// The finding.
    pub finding: ZoneFinding,
    /// Its wind position (the case as solved, its pressure and side).
    pub state: WindState,
}

impl Picked {
    /// The span or jumper.
    pub fn selection(&self) -> Selection {
        finding_span(&self.finding)
    }

    /// Where the wire was, in words: "high wind, 300 of 600 Pa from the
    /// left", "high wind, swing arc at 23° to the right".
    pub fn says(&self) -> String {
        let f = ZoneFinding {
            state: 0,
            ..self.finding.clone()
        };
        f.position(std::slice::from_ref(&self.state))
    }
}

/// What the window asks of the application.
#[derive(Debug, Default)]
pub struct Out {
    /// Go to a span (and look at this point).
    pub go: Option<(Selection, [f64; 3])>,
    /// The finding clicked: its span drawn in its weather case and wind
    /// ([`crate::result_case`]).
    pub picked: Option<Picked>,
    /// New violations to draw (wire point, target): a new report, or
    /// the window opened again.
    pub marks: Option<Vec<([f64; 3], [f64; 3])>>,
    /// The window closed: its violations off the views (when they are
    /// the ones drawn).
    pub hide_marks: bool,
    /// The model's weather case file was written (a preset's cases
    /// added): read it again.
    pub weather_saved: Option<PathBuf>,
    /// A preset's weather case file to use for the model (the model has
    /// none).
    pub weather_use: Option<PathBuf>,
    /// A design-code plugin's preset loaded (its plugin, the preset): its
    /// weather cases missing from the model to add.
    pub plugin_preset: Option<(String, ohl_io::codes::RulePresetChecked)>,
    /// A section of the user guide to open ([`GUIDE`]).
    pub guide: Option<&'static str>,
}

struct Running {
    rx: Receiver<Result<Report, String>>,
    done: Arc<AtomicUsize>,
    total: usize,
    /// How far the reading of the points has got.
    read: Arc<ohl_pointcloud::ReadProgress>,
    /// When the reading began (seen here).
    reading_since: Option<Instant>,
    cancel: CancelToken,
    started: Instant,
    /// Stopped by the user: the previous results stay.
    stopped: bool,
}

/// Violations drawn: (wire point, target) each.
type Marks = Vec<([f64; 3], [f64; 3])>;

/// One clearance window.
pub struct ZoneWindow {
    /// Shown.
    pub open: bool,
    /// The zone.
    pub zone: Zone,
    /// The wind.
    pub wind: WindSteps,
    /// The rules.
    pub rules: Vec<ZoneRule>,
    /// The circuits checked and the calculation method (the search
    /// distance is the zone's).
    pub check: CheckScope,
    /// The circuits and routes of [`ZoneWindow::check`] as the scope
    /// picker shows them.
    scope: check_ui::Scope,
    /// The project's rule file of this window (`None`: not set yet).
    pub file: Option<PathBuf>,
    loaded: bool,
    dirty: bool,
    status: Option<(String, bool)>,
    running: Option<Running>,
    /// The last check's report.
    pub report: Option<Result<Report, String>>,
    /// What it was computed from: the model's revision and a hash of the
    /// window's rules and settings ([`crate::revision`]).
    stamp: crate::revision::Stamp,
    rules_hash: (Option<u64>, Option<u64>),
    /// Reports arrived (a new one to draw).
    generation: u64,
    /// LiDAR points of the rules' classes in the cloud (for the time
    /// limit), and what a run will do, in words: updated as shown.
    points: u64,
    estimate: String,
    /// What the views draw of this window: shown (open with a report),
    /// the report's generation.
    drawn: Option<(bool, u64)>,
}

/// A rule's distance not set yet (the placeholder of a window's first
/// rule, a row added blank: 0 m, which no regulation asks for), or a
/// table or formula not filled in ([`Requirement::problem`]).
pub fn unset(r: &ZoneRule) -> bool {
    r.distance.problem().is_some()
}

/// The rule a window starts with when it has none: the ground, every
/// weather case and voltage, its distance to set (no value is invented).
pub fn first_rule() -> ZoneRule {
    ZoneRule {
        name: "Ground".into(),
        voltages: Vec::new(),
        cases: Vec::new(),
        distance: Requirement::Fixed(0.0),
        classes: Vec::new(),
        ground: true,
        surface: None,
        wires: ohl_clearance::Wires::Both,
        width: None,
        screen: false,
    }
}

/// Why rule `r` cannot run, if it cannot.
pub fn not_ready(r: &ZoneRule) -> Option<String> {
    if let Some(p) = r.distance.problem() {
        Some(format!("\"{}\": {p}", r.name))
    } else if !r.ground && r.classes.is_empty() {
        Some(format!(
            "\"{}\": choose its targets (a terrain surface and/or LiDAR classes)",
            r.name
        ))
    } else {
        None
    }
}

impl ZoneWindow {
    /// A closed window of `measure`.
    pub fn new(measure: Measure) -> Self {
        Self {
            open: false,
            zone: Zone::new(measure),
            wind: WindSteps::default(),
            rules: Vec::new(),
            check: CheckScope::default(),
            scope: check_ui::Scope::default(),
            file: None,
            loaded: false,
            dirty: false,
            status: None,
            running: None,
            report: None,
            stamp: Default::default(),
            rules_hash: (None, None),
            generation: 0,
            drawn: None,
            points: 0,
            estimate: String::new(),
        }
    }

    /// The violations to draw when what the views should show of this
    /// window changed: drawn while it is open (docked or moved too),
    /// taken off once it is closed, back when it is opened again (as the
    /// vegetation panel's layers). `Some(None)`: take them off.
    fn marks_change(&mut self) -> Option<Option<Marks>> {
        let report = match &self.report {
            Some(Ok(r)) if self.open => Some(r),
            _ => None,
        };
        let key = (report.is_some(), self.generation);
        if self.drawn == Some(key) {
            return None;
        }
        // Nothing drawn yet and nothing to draw: leave the views alone.
        let first = self.drawn.is_none();
        self.drawn = Some(key);
        // A vertical clearance drawn vertical, from the target straight up
        // (down) to the wire's height (owner, 2026-10-22).
        let measure = self.zone.measure;
        match report {
            Some(r) => Some(Some(
                r.zone.findings.iter().map(|f| f.drawn(measure)).collect(),
            )),
            None if first => None,
            None => Some(None),
        }
    }

    /// [`Self::marks_change`] into `out`.
    fn marks_out(&mut self, out: &mut Out) {
        match self.marks_change() {
            Some(Some(m)) => out.marks = Some(m),
            Some(None) => out.hide_marks = true,
            None => {}
        }
    }

    fn title(&self) -> &'static str {
        match self.zone.measure {
            Measure::Vertical => "Vertical clearances",
            Measure::Radial => "Radial clearances",
        }
    }

    /// The project's rule file of a window: in the project's clearance
    /// rules (its working copy), else next to the model.
    pub fn project_file(model: &Model, measure: Measure) -> PathBuf {
        let model_file = &model.sources.file;
        let name = model_file
            .file_name()
            .map_or(String::from("line"), |n| n.to_string_lossy().into_owned());
        let stem = name.split('.').next().unwrap_or("line").to_owned();
        let dir = model
            .library_dir
            .as_ref()
            .filter(|_| model.editing.is_ok())
            .map(|w| w.join("rules"))
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| ohl_core::files::dir_of(model_file));
        dir.join(format!("{stem}.{}-clearances.ohlr", measure.as_str()))
    }

    /// Another project line worked on (it has its own rule files): a
    /// window on the project's rule file starts again, reading the line's.
    pub fn line_changed(&mut self, model: &Model) {
        let ours = match (&self.file, &model.library_dir) {
            (Some(f), Some(w)) => f.starts_with(w),
            _ => false,
        };
        if !ours {
            return;
        }
        if let Some(r) = &self.running {
            r.cancel.cancel();
        }
        let open = self.open;
        *self = Self::new(self.zone.measure);
        if open {
            self.show(model);
        }
    }

    /// Opens the window; the first time, reads the project's rule file.
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        if self.loaded {
            return;
        }
        self.loaded = true;
        // A new window's zone: the project's default values.
        self.zone = Zone::with_defaults(self.zone.measure, &model.line.defaults);
        let file = self
            .file
            .get_or_insert_with(|| Self::project_file(model, self.zone.measure))
            .clone();
        if file.exists() {
            match RuleSet::load_with(&file, &(&model.line.defaults).into()) {
                Ok(set) => self.take(model, &set),
                Err(e) => self.status = Some((e, true)),
            }
        }
        self.keep_a_rule();
    }

    /// A window always has a rule: none, the ground's, its distance to
    /// set.
    fn keep_a_rule(&mut self) {
        if self.rules.is_empty() {
            self.rules.push(first_rule());
        }
    }

    /// The rules changed (as an edit in the table: written to the rule
    /// file once the mouse is up; tests).
    #[cfg(test)]
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Why the check cannot run, if it cannot (shown at Run).
    pub fn why_not_run(&self) -> Option<String> {
        if self.rules.is_empty() {
            return Some("no rule: add one first".into());
        }
        if self.scope.circuits.as_ref().is_some_and(|c| c.is_empty()) {
            return Some("no circuit ticked (2 Scope)".into());
        }
        let why: Vec<String> = self.rules.iter().filter_map(not_ready).collect();
        (!why.is_empty()).then(|| why.join("; "))
    }

    /// Takes the rules of a rule file (replacing those shown).
    fn take(&mut self, model: &Model, set: &RuleSet) {
        let Imported {
            rules,
            width,
            depth,
            wind,
            check,
            notes,
        } = zone::from_rule_set(
            set,
            self.zone.measure,
            |role| model.scheme.codes_with_role(role),
            &line_voltages(model),
        );
        self.rules = rules;
        self.keep_a_rule();
        if let Some(w) = width {
            self.zone.width = w;
        }
        if let Some(d) = depth {
            self.zone.depth = d;
        }
        if let Some(w) = wind {
            self.wind = w;
        }
        let check = check.unwrap_or_default();
        self.zone.search = check.search_distance;
        self.check = CheckScope {
            search_distance: None,
            ..check
        };
        self.scope = check_ui::Scope::of_check(&self.check);
        self.status = (!notes.is_empty()).then(|| (notes.join("\n"), false));
    }

    /// The structure and section overrides ticked (kept in the rule
    /// file's `[check]`).
    pub fn overrides(&self) -> crate::case_groups::Overrides {
        crate::case_groups::Overrides {
            structures: self.check.structure_override,
            sections: self.check.section_override,
        }
    }

    /// The rules as a rule file (those whose distance is set: a
    /// placeholder is never written).
    pub fn rule_set(&self) -> RuleSet {
        let set: Vec<ZoneRule> = self.rules.iter().filter(|r| !unset(r)).cloned().collect();
        zone::to_rule_set(self.title(), &self.zone, &self.wind, &self.check, &set)
    }

    /// Takes a preset chosen in *Preset ▾* (a plugin's or the project's):
    /// only this window's part of it, the weather cases its rules name
    /// matched to the project's (an equivalent case used, the rules naming
    /// it); merged into the rules (a rule of the same name replaced) or
    /// replacing them with its wind and what it solves. Its cases missing
    /// from the model go to [`Out::plugin_preset`] (added by the
    /// application).
    pub fn take_preset(&mut self, model: &Model, pick: check_ui::PresetPick, out: &mut Out) {
        let measure = self.zone.measure;
        let part = crate::preset_cases::Part::of(measure);
        match pick.taken(model, part) {
            Ok((p, lines)) => {
                let done = match pick.how {
                    crate::rule_editor::Load::Replace => {
                        // Its rules, wind and what it solves.
                        self.take(model, &p.rules);
                        format!("{} {} rule(s) loaded", self.rules.len(), measure.as_str())
                    }
                    crate::rule_editor::Load::Merge => {
                        let theirs = zone::from_rule_set(
                            &p.rules,
                            measure,
                            |role| model.scheme.codes_with_role(role),
                            &line_voltages(model),
                        )
                        .rules;
                        crate::rule_table::merge(&mut self.rules, theirs, pick.how)
                    }
                };
                self.status = Some((pick.said(&done, &lines), false));
                out.plugin_preset = Some((pick.group, p));
                self.dirty = true;
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    /// Starts a check.
    pub fn start(&mut self, model: &mut Model, ctx: &egui::Context) {
        if self.running.is_some() {
            return;
        }
        model.ensure_check_ground();
        let cases: Vec<ohl_weather::WeatherCase> =
            model.cases.iter().map(|c| c.case.clone()).collect();
        let rules = self.rules.clone();
        let states = zone::wind_states(&cases, |n| rules.iter().any(|r| r.applies(n)), &self.wind);
        // The swing arc with its bands' margins (2026-10-21).
        let arc = self.wind.arc.then_some(self.wind.margins());
        if let Some(why) = self.why_not_run() {
            self.report = Some(Err(format!("not run: {why}")));
            return;
        }
        if states.is_empty() {
            self.report = Some(Err("no weather case of the rules is in the model".into()));
            return;
        }
        let line = model.line.clone();
        let library = model.library.clone();
        let cloud = model.cloud.clone();
        let ground = model.ground_for_checks();
        // The surfaces the rules name (read on the check's thread).
        let named = crate::model::NamedSurfaces::of(
            model,
            rules
                .iter()
                .filter(|r| r.ground)
                .filter_map(|r| r.surface.clone()),
        );
        let zone = self.zone;
        let check = self.check.clone();
        let filter = self.overrides().filter(model);
        // The route worked on only (View › Work on route).
        let scope = model.scope().map(|s| s.sections);
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = CancelToken::new();
        let total = states.len();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let (d, c) = (done.clone(), cancel.clone());
        let read = Arc::new(ohl_pointcloud::ReadProgress::default());
        let progress = read.clone();
        let points = cloud.as_ref().map_or(0, |_| rule_points(model, &rules));
        self.stamp.started(model.revision());
        self.rules_hash.0 = Some(self.settings_hash());
        std::thread::spawn(move || {
            let surfaces = named.read();
            let r = run(
                &line,
                &library,
                zone,
                &check,
                rules,
                states,
                arc,
                cloud,
                ground,
                &surfaces,
                scope,
                &Progress {
                    done: &d,
                    cancel: &c,
                    read: &progress,
                    points,
                },
                &filter,
            );
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.running = Some(Running {
            rx,
            done,
            total,
            read,
            reading_since: None,
            cancel,
            started: Instant::now(),
            stopped: false,
        });
    }

    /// Takes a finished check's report; true when one arrived.
    fn poll(&mut self) -> bool {
        let Some(r) = &self.running else {
            return false;
        };
        let report = match r.rx.try_recv() {
            Ok(rep) => rep,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err("the check stopped".into()),
        };
        // Stopped by the user: the previous results stay, said so.
        if r.stopped && report.is_err() {
            self.status = Some(("stopped: the previous results are kept".into(), false));
            self.rules_hash.0 = None;
            self.running = None;
            return true;
        }
        match &report {
            Ok(rep) => log::info!(
                "{}: {} spans below the rules, {} wind positions ({:.1?} + {:.1?})",
                self.title(),
                rep.zone.findings.len(),
                rep.states.len(),
                rep.solving,
                rep.checking
            ),
            Err(e) => log::warn!("{}: {e}", self.title()),
        }
        self.report = Some(report);
        self.stamp.arrived();
        self.rules_hash.1 = self.rules_hash.0.take();
        self.running = None;
        true
    }

    /// A hash of the rules and the settings of the window (zone, wind,
    /// circuits and method).
    fn settings_hash(&self) -> u64 {
        crate::revision::hash_debug(&(&self.rules, &self.zone, &self.wind, &self.check))
    }

    /// What the last report was computed from that changed since, in
    /// words (`None`: nothing, or no report): the line, the LiDAR points,
    /// the ground, the window's rules and settings ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        if !matches!(self.report, Some(Ok(_))) {
            return None;
        }
        let model_changed = self.stamp.out_of_date(model, USES);
        let rules_changed = self.rules_hash.1.is_some_and(|h| h != self.settings_hash());
        match (model_changed, rules_changed) {
            (None, false) => None,
            (Some(m), false) => Some(m),
            (None, true) => Some("the rules or settings of this window".into()),
            (Some(m), true) => Some(format!("{m}, the rules or settings of this window")),
        }
    }

    /// The last report, and whether a check runs.
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.report.as_ref(), self.running.is_some())
    }

    /// The window.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &mut Model,
        presets: &[crate::rule_editor::PluginPresets],
    ) -> Out {
        let mut out = Out::default();
        if self.poll() {
            self.generation += 1;
        }
        if !self.open {
            self.marks_out(&mut out);
            return out;
        }
        let mut open = true;
        let mut run = false;
        let panel = match self.zone.measure {
            Measure::Vertical => crate::dockable::Panel::VerticalClearances,
            Measure::Radial => crate::dockable::Panel::RadialClearances,
        };
        let mut guide = None;
        let footer = self.title();
        let why = self.why_not_run();
        let findings = match &self.report {
            Some(Ok(r)) => Some(r.zone.findings.len()),
            _ => None,
        };
        help::window(ctx, panel, self.title(), [860.0, 760.0], &mut open, |ui| {
            // Out of date: said at the top, Run again beside (greyed as
            // Run).
            let can_run = self.running.is_none() && why.is_none();
            if let Some(what) = self.out_of_date(model)
                && crate::revision::banner(ui, &what, can_run)
            {
                run = true;
            }
            // Two tabs (owner, 2026-10-27): the setup scrolls, the
            // results fill theirs; Run in the footer, always visible.
            let tab = check_ui::tabs(
                ui,
                footer,
                findings,
                self.generation,
                why.is_some() && self.running.is_none(),
            );
            match tab {
                check_ui::Tab::Setup => help::body(ui, footer, |ui| {
                    guide = guide.or(check_ui::section(
                        ui,
                        1,
                        "What it checks",
                        Some("clearances-results"),
                    ));
                    self.help(ui);
                    let measure = self.zone.measure;
                    check_ui::how_computed(ui, footer, self.computed_text(), |ui| match measure {
                        Measure::Vertical => help::vertical_zone(ui, 30.0, 10.0),
                        Measure::Radial => help::radial_zone(ui),
                    });
                    ui.separator();
                    guide = guide.or(check_ui::section(
                        ui,
                        2,
                        "Scope",
                        Some("clearances-what-is-solved"),
                    ));
                    self.check_ui(ui, model);
                    ui.separator();
                    guide = guide.or(check_ui::section(
                        ui,
                        3,
                        "Rules",
                        Some("clearances-rules-and-presets"),
                    ));
                    self.rules_ui(ui, model, presets, &mut out);
                    ui.separator();
                    check_ui::section(ui, 4, "Settings", None);
                    guide = guide.or(help::heading(ui, "Wind", "clearances-wind"));
                    self.wind_ui(ui, model);
                    guide = guide.or(help::heading(
                        ui,
                        match self.zone.measure {
                            Measure::Vertical => "Zone",
                            Measure::Radial => "Distance",
                        },
                        "clearances-zone",
                    ));
                    self.zone_ui(ui);
                }),
                check_ui::Tab::Results => check_ui::results_area(ui, footer, |ui| {
                    guide = guide.or(help::heading(ui, "Results", "clearances-results"));
                    if let Some(p) = self.results_ui(ui, model) {
                        out.go = Some((p.selection(), p.finding.wire));
                        out.picked = Some(p);
                    }
                }),
            }
            self.update_estimate(model);
            run |= help::footer(ui, footer, |ui| self.run_ui(ui));
        });
        out.guide = guide;
        self.open = open;
        if !open && let Some(r) = &self.running {
            // Closing the window stops its check.
            r.cancel.cancel();
        }
        if run {
            self.start(model, ctx);
        }
        // Kept with the project once the mouse is released.
        if self.dirty && !ctx.input(|i| i.pointer.any_down()) {
            self.dirty = false;
            self.save(model);
        }
        self.marks_out(&mut out);
        out
    }

    /// What a run will do, before Run (owner, 2026-10-27: which
    /// parameters make it slow): the wind positions solved, the sections,
    /// the widest reach of a rule, the LiDAR points of the rules' classes
    /// and the time limit.
    fn update_estimate(&mut self, model: &Model) {
        self.points = model
            .cloud
            .as_ref()
            .map_or(0, |_| rule_points(model, &self.rules));
        let cases: Vec<ohl_weather::WeatherCase> =
            model.cases.iter().map(|c| c.case.clone()).collect();
        let states = zone::wind_states(
            &cases,
            |n| self.rules.iter().any(|r| r.applies(n)),
            &self.wind,
        );
        let mut sections = zone::sections_needed(&model.line, &self.rules);
        sections.retain(|id| {
            model
                .line
                .sections
                .iter()
                .find(|s| &s.id == id)
                .is_some_and(|s| self.check.takes_circuit(&s.circuit_name))
        });
        if let Some((on, _)) = self.check.wires_on(&model.line) {
            sections.retain(|id| on.contains(id));
        }
        let voltages = line_voltages(model);
        let margin = model.line.defaults.clearance_search_margin;
        let widest = self
            .rules
            .iter()
            .filter(|r| !r.classes.is_empty())
            .map(|r| {
                let d = r.distance.largest(&voltages);
                match self.zone.measure {
                    // In plan: the zone's width (a screen: its band).
                    Measure::Vertical if r.screen => (d, r),
                    Measure::Vertical => (r.width.unwrap_or(self.zone.width), r),
                    Measure::Radial => (d + margin, r),
                }
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let mut text = format!(
            "{} wind position(s) × {} section(s); {} rule(s)",
            states.len(),
            sections.len(),
            self.rules.len()
        );
        if let Some((d, r)) = widest {
            text.push_str(&format!(
                "; points counted up to {d:.1} m from the wires in plan (\"{}\")",
                r.name
            ));
        }
        if self.points > 0 {
            text.push_str(&format!(
                "; {} LiDAR points of the rules' classes in the cloud",
                zone::millions(self.points)
            ));
        }
        text.push_str(&format!(
            "; time limit {}",
            zone::time_limit_text(self.points)
                .split(" (")
                .next()
                .unwrap_or_default()
        ));
        self.estimate = text;
    }

    /// Writes the rule file; the project's working copy goes into the
    /// project.
    fn save(&mut self, model: &mut Model) {
        let Some(file) = &self.file else {
            return;
        };
        let in_project = model
            .library_dir
            .as_ref()
            .is_some_and(|w| file.starts_with(w));
        self.status = Some(match self.rule_set().save(file) {
            Ok(()) if in_project => match model.store_libraries() {
                Ok(_) => ("kept in the project".to_owned(), false),
                Err(e) => (format!("keeping the rules in the project: {e}"), true),
            },
            Ok(()) => (format!("kept in {}", file.display()), false),
            Err(e) => (e, true),
        });
    }

    fn help(&self, ui: &mut egui::Ui) {
        let (short, long) = match self.zone.measure {
            Measure::Vertical => (
                "How high the wires pass over the ground and over LiDAR points straight below \
                 them, in every wind position of the rules' weather cases.",
                "The zone is straight under the wire: a width on each side, across the line in \
                 plan, cut along the line in slices of the depth. A target in the zone is \
                 compared with the wire's lowest point over its slice; a target above the wire \
                 with its highest point. The ground counts in the same zone. Every clearance is \
                 to the conductor's surface.",
            ),
            Measure::Radial => (
                "How close the wires come to the ground and to LiDAR points in any direction, in \
                 every wind position of the rules' weather cases.",
                "The direct 3D distance from the wire (each subconductor of a bundle) to the \
                 targets: LiDAR points of the classes ticked, the ground. Every clearance is to \
                 the conductor's surface.",
            ),
        };
        help::explain(ui, short, long);
    }

    fn zone_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| self.zone_values_ui(ui));
            match self.zone.measure {
                Measure::Vertical => help::vertical_zone(ui, self.zone.width, self.zone.depth),
                Measure::Radial => help::radial_zone(ui),
            }
        });
    }

    fn zone_values_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut limited = self.zone.search.is_some();
            if ui
                .checkbox(&mut limited, "Ignore LiDAR points farther than")
                .on_hover_text(
                    "Only the points within this distance of the wires are read and compared \
                     (unticked: as far as the rules reach)",
                )
                .changed()
            {
                self.zone.search = limited.then_some(30.0);
                self.dirty = true;
            }
            if let Some(d) = self.zone.search.as_mut() {
                self.dirty |= ui
                    .add(egui::DragValue::new(d).speed(0.5).range(0.5..=500.0))
                    .changed();
                ui.label("m from the wires");
            }
        });
        help::explain(
            ui,
            "Search distance: only the LiDAR points this near the wires are read.",
            "Points farther from every wire position (and swing arc) are not read nor compared; \
             unticked, as far as the rules' largest distance reaches. A smaller distance reads \
             fewer points: faster, never a different clearance within it.",
        );
        if self.zone.measure != Measure::Vertical {
            help::explain(
                ui,
                "Radial: the shortest 3D distance from the conductor's surface.",
                "To each subconductor of a bundle; to the ground, the nearest point of the ground \
                 surface in any direction.",
            );
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Width on each side m");
            self.dirty |= ui
                .add(
                    egui::DragValue::new(&mut self.zone.width)
                        .speed(0.05)
                        .range(0.0..=50.0),
                )
                .on_hover_text("Across the line, in plan, from the wire")
                .changed();
            ui.label("Depth m");
            self.dirty |= ui
                .add(
                    egui::DragValue::new(&mut self.zone.depth)
                        .speed(0.01)
                        .range(0.0..=10.0),
                )
                .on_hover_text(
                    "Slices along the line: the wire is taken at its lowest over the slice \
                     around each target (0: straight above it)",
                )
                .changed();
        });
        help::explain(
            ui,
            "Width on each side of the wire, in plan; depth: the slice along the line.",
            "A target within the width of the wire (across the line, in plan) is compared with \
             the wire's lowest point over the slice of the depth around it (its highest for a \
             target above). A rule may have its own width (the rules' width column).",
        );
    }

    /// The wind choice (shared with the vegetation panel) and its drawing.
    fn wind_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        // The largest pressure of the rules' cases.
        let full = model
            .cases
            .iter()
            .filter(|c| self.rules.iter().any(|r| r.applies(&c.case.name)))
            .map(|c| c.case.wind_pressure.si())
            .fold(0.0, f64::max);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                self.dirty |=
                    help::wind_choice(ui, self.title(), &mut self.wind, true, full, true, true);
            });
            let n = if self.wind.stepped {
                (full / self.wind.step.max(1.0)).ceil() as usize
            } else {
                0
            };
            help::wind_positions(ui, self.wind.arc, n);
        });
    }

    /// The scope: the circuits and routes checked (kept in the rule
    /// file's `[check]`), the calculation method.
    fn check_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        if check_ui::ScopePicker::new(self.title(), &model.line)
            .wires(&mut self.scope, false)
            .ui(ui)
        {
            self.scope.write_check(&mut self.check);
            self.dirty = true;
        }
        ui.horizontal(|ui| {
            ui.label("Solved by");
            let name = |m: Option<Method>| match m {
                None => "each section's calculation",
                Some(Method::Fe) => "finite elements (FE)",
                Some(Method::Rs) => "ruling span (RS)",
            };
            egui::ComboBox::from_id_salt((self.title(), "method"))
                .selected_text(name(self.check.method))
                .show_ui(ui, |ui| {
                    for m in [None, Some(Method::Fe), Some(Method::Rs)] {
                        self.dirty |= ui
                            .selectable_value(&mut self.check.method, m, name(m))
                            .changed();
                    }
                })
                .response
                .on_hover_text(
                    "The method every section is solved with in this check (each keeps its \
                     cable condition)",
                );
        });
        // The structure and section overrides of the cases (kept in the
        // rule file's [check]).
        ui.horizontal_wrapped(|ui| {
            let mut o = self.overrides();
            if o.ui(ui) {
                self.check.structure_override = o.structures;
                self.check.section_override = o.sections;
                self.dirty = true;
            }
        });
        help::explain(
            ui,
            "Circuits: those checked. Routes: the routes and alignments checked (none ticked: \
             the whole line). Solved by: each section's own calculation, or one method for all.",
            "Only the sections of the circuits ticked, on the routes and alignments ticked, are \
             solved and checked (a section with a structure on one of them, whole). Finite elements \
             solve each section with its strings swinging; ruling span is the classic \
             approximation; each section keeps its cable condition (initial, final). The rules' \
             weather cases are set per rule (3 Rules).",
        );
    }

    /// *How it is computed*, in short.
    fn computed_text(&self) -> &'static str {
        match self.zone.measure {
            Measure::Vertical => {
                "Each span is solved in every wind position of its rules' weather cases (the \
                 steps, the swing arc or the case's own pressure, 4 Settings). Under the wire, \
                 within the zone's width on each side and in slices of its depth along the line, \
                 the ground and the LiDAR points of the rules' classes are compared with the \
                 wire's lowest point over the slice; the clearance is to the conductor's surface \
                 and is compared with the rule's distance at the wire's voltage."
            }
            Measure::Radial => {
                "Each span is solved in every wind position of its rules' weather cases (the \
                 steps, the swing arc or the case's own pressure, 4 Settings). The direct 3D \
                 distance from every subconductor's surface to the ground and to the LiDAR points \
                 of the rules' classes is compared with the rule's distance at the wire's \
                 voltage; the swing arc covers every angle between the blowouts, widened by the \
                 band margin."
            }
        }
    }

    /// The rules in the shared rule table (owner, 2026-10-28): one row
    /// each, the selected one's fields below it (its distance with the
    /// preview at the line's voltages), one *Preset ▾* (the design-code
    /// plugins' presets for this window, then the project's: only its
    /// part of one, the weather cases they name matched to the project's,
    /// the others created), Import / Export.
    fn rules_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
        out: &mut Out,
    ) {
        help::explain(
            ui,
            "A rule: the voltage, the weather cases, the distance and the targets — LiDAR \
             classes and/or the ground.",
            "Voltage: any (each wire with the distance of its own system) or one of the line's. \
             Weather cases: those the rule applies in (none ticked: every case). Distance: one \
             value, a table \"up to kV: distance\" or a base plus an amount per kV. Targets: the \
             LiDAR classes ticked (by number and name of the class scheme, with their colours) \
             and the ground surface of the checks. A preset (a design code's) is merged into \
             the rules (a rule of the same name replaced) or replaces them, with its wind and \
             what it solves.",
        );
        let measure = self.zone.measure;
        let part = crate::preset_cases::Part::of(measure);
        let voltages = line_voltages(model);
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let groups = crate::case_groups::groups(model);
        let classes = cloud_classes(model, &self.rules);
        let zone_width = self.zone.width;
        let preview_voltages = voltages.clone();
        let t = check_ui::RuleTable::new(self.title(), &mut self.rules, &voltages)
            .add(
                "A rule",
                "a new row at the end of the table: set its distance and its targets below it",
                |n| ZoneRule {
                    name: format!("Rule {}", n + 1),
                    ground: false,
                    ..first_rule()
                },
            )
            .keep_one()
            .presets(model, presets, part, move |set| {
                // Grouped by target, each rule with its distance and cases.
                let mut lines = Vec::new();
                for (group, list) in crate::preset_cases::grouped(&set.rules) {
                    lines.push(format!("— {group} —"));
                    for x in list {
                        let q = match measure {
                            Measure::Vertical => x.vertical.as_ref(),
                            Measure::Radial => x.radial.as_ref(),
                        };
                        if let Some(q) = q {
                            lines.push(format!(
                                "{}: {} ({})",
                                x.name,
                                crate::requirement_ui::preview_text(q, &preview_voltages),
                                crate::preset_cases::cases_label(x)
                            ));
                        }
                    }
                }
                lines
            })
            .ui(ui, |ui, k, r| {
                let mut changed = false;
                egui::Grid::new(("zone-rule", measure.as_str()))
                    .num_columns(2)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        ui.label("Targets");
                        changed |= targets_ui(ui, (measure, k), r, &classes, model);
                        ui.end_row();
                        ui.label("Voltages");
                        changed |= voltage_ui(ui, (measure, k), r, &voltages);
                        ui.end_row();
                        ui.label("Weather cases");
                        changed |= cases_ui(ui, (measure, k), r, &cases, &groups);
                        ui.end_row();
                        ui.label("Distance");
                        changed |= crate::requirement_ui::requirement_ui(
                            ui,
                            ("zone-distance", measure.as_str(), k),
                            &mut r.distance,
                            &voltages,
                        );
                        ui.end_row();
                        if measure == Measure::Vertical {
                            ui.label("Width [m]").on_hover_text(
                                "The rule's own half width of the zone across the line (none: \
                                 the window's)",
                            );
                            changed |= width_ui(ui, r, zone_width);
                            ui.end_row();
                        }
                        ui.label("Wires");
                        changed |= wires_ui(ui, (measure, k), r);
                        ui.end_row();
                    });
                changed
            });
        self.dirty |= t.changed;
        if let Some(pick) = t.preset {
            self.take_preset(model, pick, out);
        }
        if t.import
            && let Some(p) = crate::rule_table::import_file()
        {
            match RuleSet::load_with(&p, &(&model.line.defaults).into()) {
                Ok(set) => {
                    self.take(model, &set);
                    self.dirty = true;
                }
                Err(e) => self.status = Some((e, true)),
            }
        }
        if t.export
            && let Some(p) =
                crate::rule_table::export_file(&format!("{}-clearances.ohlr", measure.as_str()))
        {
            self.status = Some(match self.rule_set().save(&p) {
                Ok(()) => (format!("exported to {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
        if let Some((text, error)) = &self.status {
            if *error {
                crate::menu::error_label(ui, text.as_str());
            } else {
                crate::menu::message(ui, egui::RichText::new(text.as_str()).small(), None);
            }
        }
    }
    /// The Run row; true to start a check.
    fn run_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut run = false;
        let mut stop_now = false;
        match &mut self.running {
            Some(r) => {
                ui.spinner();
                let done = r.done.load(Ordering::Relaxed);
                let elapsed = r.started.elapsed().as_secs_f64();
                let read = r.read.share();
                if read.is_some() && r.reading_since.is_none() {
                    r.reading_since = Some(Instant::now());
                }
                if let (Some(share), Some(since)) = (read, r.reading_since) {
                    // The points read: blocks decoded of those listed.
                    let visited = r.read.visited.load(Ordering::Relaxed);
                    ui.add(
                        egui::ProgressBar::new(share as f32)
                            .desired_width(140.0)
                            .text(format!("{:.0} % of the points", share * 100.0)),
                    );
                    let reading = since.elapsed().as_secs_f64();
                    let left = (share > 0.02).then(|| reading / share * (1.0 - share));
                    ui.label(match left {
                        Some(left) => format!(
                            "{elapsed:.0} s, about {left:.0} s left; {} points checked",
                            zone::millions(visited)
                        ),
                        None => {
                            format!("{elapsed:.0} s; {} points checked", zone::millions(visited))
                        }
                    });
                } else if done < r.total {
                    ui.add(
                        egui::ProgressBar::new(done as f32 / r.total.max(1) as f32)
                            .desired_width(140.0)
                            .text(format!("{done} of {} solved", r.total)),
                    );
                    ui.label(match time_left(elapsed, done, r.total) {
                        Some(left) => format!("{elapsed:.0} s, about {left:.0} s left to solve"),
                        None => format!("{elapsed:.0} s"),
                    });
                } else {
                    ui.label(format!(
                        "indexing the wires, checking the ground… {elapsed:.0} s"
                    ));
                }
                if help::stop_button(ui, "Stop this check: the previous results stay") {
                    r.cancel.cancel();
                    stop_now = true;
                }
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
            None => {
                let why = self.why_not_run();
                run = help::run_button(
                    ui,
                    true,
                    "Solve the wind positions of the rules' weather cases and check every span \
                     (in the background)",
                    why.as_deref(),
                );
                if let Some(w) = &why {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        egui::RichText::new(format!("Before Run: {w}")).small(),
                    );
                } else if self.report.is_none() {
                    // What a run will do, and its time limit.
                    ui.label(egui::RichText::new(&self.estimate).small().weak())
                        .on_hover_text(format!(
                            "The run stops at its time limit: {}",
                            zone::time_limit_text(self.points)
                        ));
                }
                if let Some(Ok(r)) = &self.report {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} wind positions solved in {:.1?}{}, checked in {:.1?}",
                            r.states.len(),
                            r.solving,
                            if r.arcs.is_empty() {
                                String::new()
                            } else {
                                format!(" ({} swing arcs)", r.arc_spans)
                            },
                            r.checking
                        ))
                        .small()
                        .weak(),
                    );
                }
            }
        }
        if stop_now && let Some(r) = self.running.as_mut() {
            r.stopped = true;
        }
        run
    }

    /// What the results show.
    fn results_help(&self, ui: &mut egui::Ui) {
        help::explain(
            ui,
            "The spans below a rule, worst first; each rule's lowest clearance; drawn in the views.",
            "Per span: the rule, the distance required and found (to the conductor's surface), \
             the wind position where it is found (a pressure and side, or the swing arc and the \
             angle there), the target (the ground or a LiDAR class) and how many points (or wire \
             pieces) are too close. A click on the span goes to it.",
        );
    }

    fn results_ui(&self, ui: &mut egui::Ui, model: &Model) -> Option<Picked> {
        let report = match &self.report {
            None => {
                ui.label(egui::RichText::new("No run yet: Run at the bottom.").weak());
                return None;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return None;
            }
            Some(Ok(r)) => r,
        };
        self.results_help(ui);
        let n = report.zone.findings.len();
        let mut go: Option<&ZoneFinding> = None;
        if let Some(g) = result_banner(ui, report) {
            go = Some(g);
        }
        let picked = |f: Option<&ZoneFinding>| {
            f.map(|f| Picked {
                finding: f.clone(),
                state: report.states[f.state].clone(),
            })
        };
        let head = format!(
            "{n} span{} below a rule; {} LiDAR points read",
            if n == 1 { "" } else { "s" },
            report.zone.points
        );
        ui.label(egui::RichText::new(head).small().weak());
        // Every finding, only those in view drawn (2026-10-26); sorted,
        // filtered and grouped by the user (2026-10-27).
        let findings = &report.zone.findings;
        let rows: Vec<check_ui::Row> = findings
            .iter()
            .map(|f| {
                let span = match finding_span(f) {
                    Selection::Span { section, span } => check_ui::Cell::span(&section, span),
                    other => check_ui::Cell::text(other.describe()),
                };
                // A screen: the band b and the distance in plan, said as
                // such (owner, 2026-10-27).
                let plan = (f.wire[0] - f.target[0]).hypot(f.wire[1] - f.target[1]);
                let (required, found) = match f.band {
                    Some(b) => (
                        check_ui::Cell::sorted(format!("b {b:.2} in plan"), b),
                        check_ui::Cell::sorted(format!("{plan:.2} in plan"), plan),
                    ),
                    None => (
                        check_ui::Cell::num(Some(f.required), 2),
                        check_ui::Cell::num(Some(f.distance), 2),
                    ),
                };
                let target = match f.class {
                    None => "ground".into(),
                    Some(c) => format!(
                        "class {c}{}",
                        model
                            .scheme
                            .class(c)
                            .map_or(String::new(), |d| format!(" {}", d.name))
                    ),
                };
                let what = match (f.class, f.count) {
                    (None, 1) => "wire piece",
                    (None, _) => "wire pieces",
                    (Some(_), 1) => "point",
                    (Some(_), _) => "points",
                };
                check_ui::Row::new(
                    vec![
                        span,
                        check_ui::Cell::text(&report.rules[f.rule].name),
                        required,
                        found.bad(true),
                        check_ui::Cell::sorted(
                            format!("{:+.2}", f.distance - f.required),
                            f.distance - f.required,
                        )
                        .bad(true),
                        // The case said with the wind (owner, 2026-10-27);
                        // clicked, the span is drawn in them.
                        check_ui::Cell::text(f.position(&report.states)),
                        check_ui::Cell::text(target),
                        check_ui::Cell::sorted(format!("{} {what}", f.count), f.count as f64),
                    ],
                    true,
                )
            })
            .collect();
        let mut notes: Vec<String> = Vec::new();
        for (k, low) in report.zone.lowest.iter().enumerate() {
            let name = &report.rules[k].name;
            notes.push(match low {
                Some(f) => format!(
                    "{name}: lowest {:.2} m ({}, {})",
                    f.distance,
                    finding_span(f).describe(),
                    f.position(&report.states)
                ),
                None => format!("{name}: nothing within reach"),
            });
        }
        notes.extend(report.zone.notes.iter().cloned());
        notes.push(crate::app::SURFACE_NOTE.to_owned());
        notes.extend(report.failed.iter().map(|f| format!("not solved: {f}")));
        let id = format!("{}-results", self.title());
        let csv = format!("{}.csv", self.title().to_lowercase().replace(' ', "-"));
        let clicked = check_ui::ResultTable::new(
            &id,
            vec![
                check_ui::Col::text("Span"),
                check_ui::Col::text("Rule"),
                check_ui::Col::num("Required [m]"),
                check_ui::Col::num("To surface [m]")
                    .tip("The clearance found, to the conductor's surface"),
                check_ui::Col::num("Margin [m]").tip("Found less required"),
                check_ui::Col::text("Case and wind"),
                check_ui::Col::text("Target"),
                check_ui::Col::num("Too close"),
            ],
        )
        .rows(rows)
        .group_by(&[1, 0, 6])
        .csv(&csv)
        .below(check_ui::FOLDS_BELOW)
        .ui(ui);
        if let Some(k) = clicked {
            go = Some(&findings[k]);
        }
        check_ui::notes_fold(ui, &id, &notes);
        // Where the time went, measured (owner, 2026-10-27).
        egui::CollapsingHeader::new(
            egui::RichText::new(format!(
                "Where the time went ({:.1} s)",
                (report.solving + report.checking).as_secs_f64()
            ))
            .small(),
        )
        .id_salt((self.title(), "timing"))
        .show(ui, |ui| {
            for l in zone::breakdown(report) {
                ui.label(egui::RichText::new(l).small().weak());
            }
        });
        picked(go)
    }
}

/// The span a finding is on (a jumper's: the jumper).
fn finding_span(f: &ZoneFinding) -> Selection {
    match f.section.strip_prefix("jumper:") {
        Some(j) => Selection::Jumper(j.to_owned()),
        None => Selection::Span {
            section: f.section.clone(),
            span: f.span,
        },
    }
}

/// The result first, in one banner (owner: OHL-Design UI review,
/// 2026-10-26): the worst margin (clearance found less the one required)
/// over every rule, where and in which weather case, red below a rule,
/// green when every span is clear; a click on it brings the views there.
fn result_banner<'a>(ui: &mut egui::Ui, report: &'a Report) -> Option<&'a ZoneFinding> {
    let worst = report
        .zone
        .findings
        .iter()
        .chain(report.zone.lowest.iter().flatten())
        .min_by(|a, b| (a.distance - a.required).total_cmp(&(b.distance - b.required)));
    let failing = !report.zone.findings.is_empty();
    let text = match worst {
        Some(f) => {
            let margin = f.distance - f.required;
            let case = &report.states[f.state].name;
            let lead = if failing {
                let n = report.zone.findings.len();
                format!("✖ {n} span{} below a rule", if n == 1 { "" } else { "s" })
            } else {
                "✔ Every span clear".to_owned()
            };
            match f.band {
                Some(b) => format!(
                    "{lead} — {}, case {case} ({}: {})",
                    finding_span(f).describe(),
                    report.rules[f.rule].name,
                    ohl_clearance::zone::screen_text(b, f.wire, f.target)
                ),
                None => format!(
                    "{lead} — worst margin {margin:+.2} m: {}, case {case} ({}, {:.2} m of {:.2} m)",
                    finding_span(f).describe(),
                    report.rules[f.rule].name,
                    f.distance,
                    f.required
                ),
            }
        }
        None if failing => "✖ Spans below a rule".to_owned(),
        None => "✔ Every span clear (nothing within reach of the rules)".to_owned(),
    };
    if check_ui::verdict(ui, failing, &text, worst.is_some()) {
        return worst;
    }
    None
}

/// The check, on its own thread ([`zone::run_zone_check`]; the points
/// read again from the point cloud around the wires).
#[allow(clippy::too_many_arguments)]
fn run(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    zone: Zone,
    check: &CheckScope,
    rules: Vec<ZoneRule>,
    states: Vec<WindState>,
    arc: Option<ohl_clearance::positions::BandMargins>,
    cloud: Option<String>,
    ground: Option<Arc<ohl_terrain::Terrain>>,
    surfaces: &[(String, Arc<ohl_terrain::Terrain>)],
    scope: Option<std::collections::BTreeSet<String>>,
    progress: &Progress,
    filter: &ohl_weather::CaseFilter,
) -> Result<Report, String> {
    let surfaces: Vec<(String, &ohl_terrain::Terrain)> = surfaces
        .iter()
        .map(|(n, t)| (n.clone(), t.as_ref()))
        .collect();
    let limit = zone::time_limit(progress.points);
    let budget = Budget::new()
        .time_limit(limit)
        .cancel_token(progress.cancel.clone());
    let started = Instant::now();
    let read = |polylines: &[Vec<[f64; 2]>],
                reach: f64,
                classes: &[u8],
                budget: &Budget,
                visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        let spec = cloud.as_deref().unwrap_or_default();
        ohl_pointcloud::visit_around_with(
            spec,
            polylines,
            reach,
            classes,
            budget,
            Some(progress.read),
            visit,
        )
        .map(|_| ())
    };
    let done = progress.done;
    let result = zone::run_zone_check(
        line,
        library,
        zone,
        check,
        rules,
        states,
        arc,
        cloud.is_some().then_some(&read as _),
        ground.as_deref(),
        &surfaces,
        scope.as_ref(),
        &budget,
        done,
        filter,
    );
    // The time limit reached: said, with what to change.
    result.map_err(|e| {
        if !progress.cancel.is_cancelled() && started.elapsed() >= limit {
            format!(
                "{e} — the time limit of {} was reached: a larger wind step, fewer rules or \
                 weather cases, a smaller search distance or fewer routes make the check faster",
                zone::time_limit_text(progress.points)
            )
        } else {
            e
        }
    })
}

/// What a run reports its progress to, and what it may take.
struct Progress<'a> {
    /// Wind positions solved.
    done: &'a AtomicUsize,
    /// Stop.
    cancel: &'a CancelToken,
    /// Points read.
    read: &'a ohl_pointcloud::ReadProgress,
    /// LiDAR points of the rules' classes in the cloud (the time limit).
    points: u64,
}

/// The LiDAR points of the rules' classes in the model's point cloud
/// (counted without reading them).
pub fn rule_points(model: &Model, rules: &[ZoneRule]) -> u64 {
    model
        .class_counts
        .iter()
        .filter(|(c, _)| rules.iter().any(|r| r.classes.contains(c)))
        .map(|(_, n)| n)
        .sum()
}

/// The seconds left to solve `total` positions after `done` in
/// `elapsed` s (none before the first is solved).
pub fn time_left(elapsed: f64, done: usize, total: usize) -> Option<f64> {
    (done > 0 && done < total).then(|| elapsed / done as f64 * (total - done) as f64)
}

/// The nominal voltages of the line's sections, kV (0: earthed), lowest
/// first.
fn line_voltages(model: &Model) -> Vec<f64> {
    zone::line_voltages(&model.line)
}

/// The classes to tick: (code, name) of the point cloud's classes, else
/// the class scheme's, and those the rules use.
fn cloud_classes(model: &Model, rules: &[ZoneRule]) -> Vec<(u8, String)> {
    let mut codes: std::collections::BTreeSet<u8> =
        model.class_counts.iter().map(|(c, _)| *c).collect();
    codes.extend(model.points.iter().map(|p| p.1));
    if codes.is_empty() {
        codes.extend(model.scheme.classes.iter().map(|c| c.code));
    }
    codes.extend(rules.iter().flat_map(|r| r.classes.iter().copied()));
    codes
        .into_iter()
        .map(|c| {
            (
                c,
                model
                    .scheme
                    .class(c)
                    .map_or_else(|| "unknown".to_owned(), |d| d.name.clone()),
            )
        })
        .collect()
}

/// A clearance window's rule as a row of the shared rule table.
impl check_ui::RuleRow for ZoneRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn summary(&self, _: &[f64]) -> check_ui::Summary {
        use ohl_clearance::Wires;
        let mut target: Vec<String> = Vec::new();
        if self.ground {
            target.push(match &self.surface {
                Some(n) => format!("TIN {n}"),
                None => "ground".into(),
            });
        }
        if !self.classes.is_empty() {
            let list: Vec<String> = self.classes.iter().map(|c| c.to_string()).collect();
            target.push(format!(
                "class{} {}",
                if self.classes.len() == 1 { "" } else { "es" },
                list.join(", ")
            ));
        }
        let distance = crate::rule_table::distance_words(&self.distance);
        check_ui::Summary {
            target: if target.is_empty() {
                "choose its targets".into()
            } else {
                target.join(" + ")
            },
            voltages: crate::rule_table::voltages_words(&self.voltages),
            cases: crate::rule_table::cases_words(&self.cases),
            distance: if self.screen {
                format!("screen, band {distance}")
            } else {
                distance
            },
            wires: match self.wires {
                Wires::Both => "sections + jumpers",
                Wires::Sections => "sections",
                Wires::Jumpers => "jumpers",
            }
            .into(),
        }
    }
}

/// The voltages of a rule ([`crate::requirement_ui::voltage_filter_ui`]):
/// optional, hidden behind a toggle when its distance depends on the
/// voltage.
fn voltage_ui(ui: &mut egui::Ui, id: (Measure, usize), r: &mut ZoneRule, voltages: &[f64]) -> bool {
    crate::requirement_ui::voltage_filter_ui(
        ui,
        ("zone-voltage", id.0.as_str(), id.1),
        &mut r.voltages,
        r.distance.by_voltage(),
        voltages,
    )
}

/// The weather cases of a rule, ticked (none ticked: every case).
fn cases_ui(
    ui: &mut egui::Ui,
    id: (Measure, usize),
    r: &mut ZoneRule,
    cases: &[String],
    groups: &[ohl_weather::CaseGroup],
) -> bool {
    let mut changed = false;
    let on: Vec<bool> = cases
        .iter()
        .map(|c| r.cases.iter().any(|p| ohl_core::pattern::matches(p, c)))
        .collect();
    let n = on.iter().filter(|x| **x).count();
    let title = if r.cases.is_empty() {
        "every case".to_owned()
    } else if n == 1 {
        cases[on.iter().position(|x| *x).unwrap_or(0)].clone()
    } else {
        format!("{n} cases")
    };
    ui.push_id(("zone-cases", id.0.as_str(), id.1), |ui| {
        crate::menu::options_menu(ui, &title, |ui| {
            ui.label(
                egui::RichText::new("None ticked: every weather case")
                    .small()
                    .weak(),
            );
            // A group's button: its cases added.
            let mut named = r.cases.clone();
            if crate::case_groups::pick_names(ui, groups, &mut named) {
                r.cases = named;
                changed = true;
                return;
            }
            let mut ticks = on.clone();
            help::list(
                ui,
                "clearance-zones-list-1",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical(),
                |ui| {
                    for (k, c) in cases.iter().enumerate() {
                        changed |= ui.checkbox(&mut ticks[k], c).changed();
                    }
                },
            );
            if changed {
                r.cases = cases
                    .iter()
                    .zip(&ticks)
                    .filter(|(_, t)| **t)
                    .map(|(c, _)| c.clone())
                    .collect();
            }
        });
    });
    changed
}

/// A vertical rule's own half width of the zone (none: the window's,
/// `zone`).
fn width_ui(ui: &mut egui::Ui, r: &mut ZoneRule, zone: f64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| match r.width.as_mut() {
        Some(w) => {
            changed |= ui
                .add(
                    egui::DragValue::new(w)
                        .speed(0.05)
                        .range(0.0..=50.0)
                        .max_decimals(3),
                )
                .changed();
            if ui
                .small_button("×")
                .on_hover_text("Back to the window's width")
                .clicked()
            {
                r.width = None;
                changed = true;
            }
        }
        None => {
            if ui
                .small_button(format!("({zone})"))
                .on_hover_text("The window's width: click to give this rule its own")
                .clicked()
            {
                r.width = Some(zone);
                changed = true;
            }
        }
    });
    changed
}

/// Adds the cases of weather case file `from` whose names are not in
/// `have` to the weather case file `to` (its text kept, the cases
/// appended); the names added.
pub fn add_missing_cases(to: &Path, from: &Path, have: &[String]) -> Result<Vec<String>, String> {
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let source: toml_edit::DocumentMut = read(from)?
        .parse()
        .map_err(|e| format!("{}: {e}", from.display()))?;
    let mut doc: toml_edit::DocumentMut = read(to)?
        .parse()
        .map_err(|e| format!("{}: {e}", to.display()))?;
    let Some(cases) = source
        .get("case")
        .and_then(toml_edit::Item::as_array_of_tables)
    else {
        return Ok(Vec::new());
    };
    let mut added: Vec<String> = Vec::new();
    for c in cases.iter() {
        let Some(name) = c.get("name").and_then(toml_edit::Item::as_str) else {
            continue;
        };
        if have.iter().any(|h| h == name) || added.iter().any(|a| a == name) {
            continue;
        }
        let list = doc
            .entry("case")
            .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
        let Some(list) = list.as_array_of_tables_mut() else {
            return Err(format!("{}: case is not a list of tables", to.display()));
        };
        list.push(c.clone());
        added.push(name.to_owned());
    }
    if !added.is_empty() {
        let text = doc.to_string();
        ohl_weather::WeatherCases::parse(&text).map_err(|e| format!("{}: {e}", to.display()))?;
        std::fs::write(to, text).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Ok(added)
}

/// The wires a rule checks: the sections' spans, the jumpers or both.
fn wires_ui(ui: &mut egui::Ui, id: (Measure, usize), r: &mut ZoneRule) -> bool {
    use ohl_clearance::Wires;
    let mut changed = false;
    let name = |w: Wires| match w {
        Wires::Both => "sections + jumpers",
        Wires::Sections => "sections",
        Wires::Jumpers => "jumpers",
    };
    egui::ComboBox::from_id_salt(("zone-wires", id.0.as_str(), id.1))
        .selected_text(name(r.wires))
        .width(130.0)
        .show_ui(ui, |ui| {
            for w in [Wires::Both, Wires::Sections, Wires::Jumpers] {
                changed |= ui.selectable_value(&mut r.wires, w, name(w)).changed();
            }
        })
        .response
        .on_hover_text("The wires the rule checks: the sections' spans, the jumpers, or both");
    changed
}

/// The targets of a rule: a terrain surface (none, the checks' or a TIN
/// layer by name) and LiDAR classes, either or both.
fn targets_ui(
    ui: &mut egui::Ui,
    id: (Measure, usize),
    r: &mut ZoneRule,
    classes: &[(u8, String)],
    model: &Model,
) -> bool {
    let scheme = &model.scheme;
    let mut changed = false;
    let mut parts: Vec<String> = r.classes.iter().map(u8::to_string).collect();
    if r.ground {
        parts.insert(
            0,
            match &r.surface {
                Some(n) => format!("TIN {n}"),
                None => "ground".into(),
            },
        );
    }
    let title = if parts.is_empty() {
        "choose…".to_owned()
    } else if parts.len() > 4 {
        format!(
            "{} classes{}",
            r.classes.len(),
            if r.ground { " + ground" } else { "" }
        )
    } else {
        parts.join(", ")
    };
    ui.push_id(("zone-targets", id.0.as_str(), id.1), |ui| {
        crate::menu::options_menu(ui, &title, |ui| {
            ui.label(egui::RichText::new("Terrain surface (TIN)").small().weak());
            let mut choice = (r.ground, r.surface.clone());
            ui.radio_value(&mut choice, (false, None), "none");
            let checks = match &model.check_surface {
                Some(n) => format!("the surface of the checks ({n})"),
                None => "the surface of the checks (the ground shown)".to_owned(),
            };
            ui.radio_value(&mut choice, (true, None), checks)
                .on_hover_text("The project's surface for the checks (Line › Terrain)");
            let in_project = crate::surfaces::names(model);
            let mut names = in_project.clone();
            if let Some(n) = &r.surface
                && !names.contains(n)
            {
                // Named by a rule file but not in the project: kept, said.
                names.push(n.clone());
            }
            for n in names {
                let known = in_project.contains(&n);
                let text = if known {
                    n.clone()
                } else {
                    format!("{n} (not in the project)")
                };
                ui.radio_value(&mut choice, (true, Some(n)), text)
                    .on_hover_text("This TIN layer, whatever the surface of the checks");
            }
            if choice != (r.ground, r.surface.clone()) {
                (r.ground, r.surface) = choice;
                changed = true;
            }
            ui.separator();
            ui.label(egui::RichText::new("LiDAR classes").small().weak());
            help::list(
                ui,
                "clearance-zones-list-2",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical(),
                |ui| {
                    for (code, name) in classes {
                        let mut on = r.classes.contains(code);
                        ui.horizontal(|ui| {
                            help::class_swatch(ui, scheme, *code);
                            if ui.checkbox(&mut on, format!("{code} {name}")).changed() {
                                if on {
                                    r.classes.push(*code);
                                    r.classes.sort_unstable();
                                } else {
                                    r.classes.retain(|c| c != code);
                                }
                                changed = true;
                            }
                        });
                    }
                },
            );
        });
    });
    changed
}
