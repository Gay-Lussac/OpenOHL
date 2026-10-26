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
//! **Load preset…** takes the rules of a design code's preset (kept in the
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

/// Exit clause of a check: solving every wind position and reading the
/// points. The synthetic line takes milliseconds; a long line with fine
/// steps a few minutes.
const TIME_BUDGET: Duration = Duration::from_secs(900);

/// Height of the scrolling lists (rules, results), points.
const LIST_HEIGHT: f32 = 220.0;

/// What a check found.
pub type Report = zone::ZoneCheckResult;

/// What the window asks of the application.
#[derive(Debug, Default)]
pub struct Out {
    /// Go to a span (and look at this point).
    pub go: Option<(Selection, [f64; 3])>,
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
    /// The project's rule file of this window (`None`: not set yet).
    pub file: Option<PathBuf>,
    loaded: bool,
    dirty: bool,
    status: Option<(String, bool)>,
    confirm: Option<usize>,
    running: Option<Running>,
    /// The last check's report.
    pub report: Option<Result<Report, String>>,
    /// What it was computed from: the model's revision and a hash of the
    /// window's rules and settings ([`crate::revision`]).
    stamp: crate::revision::Stamp,
    rules_hash: (Option<u64>, Option<u64>),
    /// The plugin preset chosen in the dropdown (plugin, preset).
    pub plugin_preset: Option<(usize, usize)>,
    /// A plugin preset loaded this frame, for [`Out::plugin_preset`].
    loaded_preset: Option<(String, ohl_io::codes::RulePresetChecked)>,
    /// Reports arrived (a new one to draw).
    generation: u64,
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
            file: None,
            loaded: false,
            dirty: false,
            status: None,
            confirm: None,
            running: None,
            report: None,
            plugin_preset: None,
            loaded_preset: None,
            stamp: Default::default(),
            rules_hash: (None, None),
            generation: 0,
            drawn: None,
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

    /// The presets of the project: its rule files (but the clearance
    /// windows' own) with a weather case file of the same name, that have
    /// a rule of this window's measure — (name shown, rule file, weather
    /// case file).
    pub fn presets(&self, model: &Model) -> Vec<(String, PathBuf, PathBuf)> {
        let Some(work) = model.library_dir.as_ref().filter(|_| model.editing.is_ok()) else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(work.join("rules")) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                ohl_core::files::has_extension(p, ohl_clearance::rules::RULE_FILE_EXTENSION)
            })
            .collect();
        files.sort();
        let mut out = Vec::new();
        for r in files {
            let name = r
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            if name.ends_with("-clearances.ohlr") || name.ends_with("-clearances.olsr") {
                continue;
            }
            let stem = r
                .file_stem()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            let w = work
                .join("weather")
                .join(format!("{stem}.{}", ohl_weather::WEATHER_FILE_EXTENSION));
            if !w.is_file() {
                continue;
            }
            let Ok(set) = RuleSet::load(&r) else {
                continue;
            };
            if crate::preset_cases::Part::of(self.zone.measure).lists(&set) {
                let shown = if set.name.is_empty() {
                    name
                } else {
                    set.name.clone()
                };
                out.push((shown, r, w));
            }
        }
        out
    }

    /// Takes a preset: its rules replace those shown; its weather cases are
    /// matched to the project's ([`crate::preset_cases`]: an equivalent
    /// case used, the rules naming it), the others added to the model's
    /// weather case file
    /// (or its file is used for the model when the model has none).
    pub fn load_preset(&mut self, model: &Model, rules: &Path, weather: &Path, out: &mut Out) {
        let mut set = match RuleSet::load_with(rules, &(&model.line.defaults).into()) {
            Ok(s) => s,
            Err(e) => {
                self.status = Some((e, true));
                return;
            }
        };
        // Only this window's rules and the weather cases they name (owner,
        // 2026-10-21: presets are per check), matched to the project's:
        // the same weather used (the rules naming the project's), the
        // others created.
        let part = crate::preset_cases::Part::of(self.zone.measure);
        let matched = std::fs::read_to_string(weather)
            .map_err(|e| format!("{}: {e}", weather.display()))
            .and_then(|text| {
                let wc = ohl_weather::WeatherCases::parse(&text)
                    .map_err(|e| format!("{}: {e}", weather.display()))?;
                let names = part.cases(&set, &wc);
                let text = crate::preset_cases::keep_cases_text(&text, &names)?;
                let wc = ohl_weather::WeatherCases::parse(&text)?;
                let plan =
                    crate::preset_cases::Plan::new(&crate::preset_cases::project_cases(model), &wc);
                let text = plan.weather_text(&text)?;
                Ok((plan, text))
            });
        set.rules.retain(|r| part.takes(r));
        set.vegetation = None;
        let (plan, weather_text) = match matched {
            Ok(x) => x,
            Err(e) => {
                self.status = Some((e, true));
                return;
            }
        };
        plan.rename_rule_set(&mut set);
        self.take(model, &set);
        self.dirty = true;
        let have: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let added = match &model.sources.weather {
            None => {
                out.weather_use = Some(weather.to_path_buf());
                Ok("its weather cases are used for the model".to_owned())
            }
            Some(file) => {
                let tmp = std::env::temp_dir().join(format!(
                    "ohl-preset-cases-{}.{}",
                    std::process::id(),
                    ohl_weather::WEATHER_FILE_EXTENSION
                ));
                let added = std::fs::write(&tmp, &weather_text)
                    .map_err(|e| format!("{}: {e}", tmp.display()))
                    .and_then(|()| add_missing_cases(file, &tmp, &have));
                let _ = std::fs::remove_file(&tmp);
                added.map(|added| {
                    if added.is_empty() {
                        "its weather cases are in the model".to_owned()
                    } else {
                        out.weather_saved = Some(file.clone());
                        format!("weather cases added to the model: {}", added.join(", "))
                    }
                })
            }
        };
        let (mut text, error) = match added {
            Ok(m) => (format!("preset {}: {m}", set.name), false),
            Err(e) => (format!("preset {}: {e}", set.name), true),
        };
        for line in plan.lines() {
            text = format!("{text}\n{line}");
        }
        if let Some((notes, _)) = &self.status {
            text = format!("{text}\n{notes}");
        }
        self.status = Some((text, error));
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
        self.stamp.started(model.revision());
        self.rules_hash.0 = Some(self.settings_hash());
        std::thread::spawn(move || {
            let surfaces = named.read();
            let r = run(
                &line, &library, zone, &check, rules, states, arc, cloud, ground, &surfaces, scope,
                &d, &c, &filter,
            );
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.running = Some(Running {
            rx,
            done,
            total,
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

    /// The presets of the design-code plugins for this window (those for
    /// its check, [`crate::preset_cases::Part`]): a dropdown grouped by
    /// plugin, the preset's rules of this measure previewed by target with
    /// their cases, then loaded (they replace those listed; the weather
    /// cases they name matched to the project's, the others created).
    fn plugin_presets_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        presets: &[crate::rule_editor::PluginPresets],
    ) {
        let measure = self.zone.measure;
        // Only the presets for this window, and only its part of one
        // (owner, 2026-10-21: presets are per check).
        let part = crate::preset_cases::Part::of(measure);
        let ours = move |p: &ohl_io::codes::RulePresetChecked| part.lists(&p.rules);
        let mut lines: Vec<String> = Vec::new();
        ui.horizontal(|ui| {
            ui.label("Plugin preset");
            crate::rule_editor::preset_dropdown(
                ui,
                &format!("{}-plugin-preset", self.title()),
                presets,
                ours,
                &mut self.plugin_preset,
            );
            let chosen = self.plugin_preset.and_then(|(i, k)| {
                let pp = presets.get(i)?;
                Some((pp.plugin.clone(), pp.presets.as_ref().ok()?.get(k)?.clone()))
            });
            let Some((plugin, p)) = chosen else {
                return;
            };
            // This window's rules, their weather cases matched to the
            // project's (the same weather used, the rules naming the
            // project's).
            let (p, cases) = match crate::preset_cases::for_project(model, &p, part) {
                Ok((adapted, plan)) => (adapted, plan.lines()),
                Err(e) => {
                    lines.push(format!("preset {}: {e}", p.name));
                    return;
                }
            };
            let voltages = ohl_clearance::zone::line_voltages(&model.line);
            // Grouped by target, each rule with its distance and cases.
            let mut preview: Vec<String> = Vec::new();
            for (group, list) in crate::preset_cases::grouped(&p.rules.rules) {
                preview.push(format!("— {group} —"));
                for x in list {
                    let q = match measure {
                        Measure::Vertical => x.vertical.as_ref(),
                        Measure::Radial => x.radial.as_ref(),
                    };
                    if let Some(q) = q {
                        preview.push(format!(
                            "{}: {} ({})",
                            x.name,
                            crate::requirement_ui::preview_text(q, &voltages),
                            crate::preset_cases::cases_label(x)
                        ));
                    }
                }
            }
            if ui
                .button("Load")
                .on_hover_text(format!(
                    "{}\n{}\n{}\nIts {} rules replace those listed; your weather cases of the \
                     same weather are used, the others created",
                    p.description,
                    preview.join("\n"),
                    cases.join("\n"),
                    measure.as_str()
                ))
                .clicked()
            {
                self.take(model, &p.rules);
                self.dirty = true;
                let mut text = format!(
                    "preset {plugin} {}: {} {} rule(s) loaded",
                    p.name,
                    p.rules.rules.len(),
                    measure.as_str()
                );
                for line in &cases {
                    text = format!("{text}\n{line}");
                }
                self.status = Some((text, false));
                self.loaded_preset = Some((plugin, p));
            }
            lines = cases;
        });
        // What Load does with its weather cases.
        for line in lines {
            ui.label(egui::RichText::new(line).small().weak());
        }
    }

    /// The window.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &mut Model,
        presets: &[crate::rule_editor::PluginPresets],
    ) -> Out {
        let mut out = Out {
            plugin_preset: self.loaded_preset.take(),
            ..Out::default()
        };
        if self.poll() {
            self.generation += 1;
        }
        if !self.open {
            self.marks_out(&mut out);
            return out;
        }
        let mut open = true;
        let mut run = false;
        let mut preset = None;
        let panel = match self.zone.measure {
            Measure::Vertical => crate::dockable::Panel::VerticalClearances,
            Measure::Radial => crate::dockable::Panel::RadialClearances,
        };
        let window = egui::Window::new(self.title())
            .default_width(820.0)
            .default_height(760.0);
        let mut guide = None;
        let footer = self.title();
        crate::dockable::show(ctx, panel, self.title(), window, Some(&mut open), |ui| {
            // Scrolled: with the distance editors in place a window may be
            // taller than the screen; it stays closable. Run in the footer
            // at the bottom left, always visible (owner, 2026-10-21).
            help::body(ui, footer, |ui| {
                {
                    self.help(ui);
                    // Out of date: said at the top, Run again beside.
                    // Run again as Run: greyed alike.
                    let can_run = self.running.is_none() && self.why_not_run().is_none();
                    if let Some(what) = self.out_of_date(model)
                        && crate::revision::banner(ui, &what, can_run)
                    {
                        run = true;
                    }
                    // The results first, the settings below.
                    guide = guide.or(help::heading(ui, "Results", "clearances-results"));
                    self.results_help(ui);
                    if let Some(go) = self.results_ui(ui, model) {
                        out.go = Some(go);
                    }
                    ui.separator();
                    guide = guide.or(help::heading(
                        ui,
                        "Rules and presets",
                        "clearances-rules-and-presets",
                    ));
                    self.plugin_presets_ui(ui, model, presets);
                    if let Some(chosen) = self.rules_ui(ui, model) {
                        preset = Some(chosen);
                    }
                    ui.separator();
                    guide = guide.or(help::heading(ui, "Wind", "clearances-wind"));
                    self.wind_ui(ui, model);
                    ui.separator();
                    guide = guide.or(help::heading(
                        ui,
                        match self.zone.measure {
                            Measure::Vertical => "Zone",
                            Measure::Radial => "Distance",
                        },
                        "clearances-zone",
                    ));
                    self.zone_ui(ui);
                    ui.separator();
                    guide = guide.or(help::heading(
                        ui,
                        "What is solved",
                        "clearances-what-is-solved",
                    ));
                    self.check_ui(ui, model);
                    // The structure and section overrides of the cases
                    // (in the body here: the footer stays one line).
                    ui.horizontal_wrapped(|ui| {
                        let mut o = self.overrides();
                        if o.ui(ui) {
                            // Kept in the rule file's [check].
                            self.check.structure_override = o.structures;
                            self.check.section_override = o.sections;
                            self.dirty = true;
                        }
                    });
                }
            });
            run |= help::footer(ui, footer, |ui| self.run_ui(ui));
        });
        out.guide = guide;
        self.open = open;
        if let Some((r, w)) = preset {
            self.load_preset(model, &r, &w, &mut out);
        }
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
                self.dirty |= help::wind_choice(ui, self.title(), &mut self.wind, true, full, true);
            });
            let n = if self.wind.stepped {
                (full / self.wind.step.max(1.0)).ceil() as usize
            } else {
                0
            };
            help::wind_positions(ui, self.wind.arc, n);
        });
    }

    /// The circuits checked and the calculation method.
    fn check_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let mut names: Vec<String> = model
            .line
            .sections
            .iter()
            .map(|s| s.circuit_name.clone())
            .collect();
        names.extend(self.check.circuits.iter().cloned());
        names.sort();
        names.dedup();
        let shown = |n: &str| {
            if n.is_empty() {
                "(no circuit name)".to_owned()
            } else {
                n.to_owned()
            }
        };
        ui.horizontal(|ui| {
            ui.label("Circuits");
            let title = match self.check.circuits.len() {
                0 => "every circuit".to_owned(),
                1 => shown(&self.check.circuits[0]),
                n => format!("{n} circuits"),
            };
            ui.push_id((self.title(), "circuits"), |ui| {
                crate::menu::options_menu(ui, &title, |ui| {
                    ui.label(
                        egui::RichText::new("None ticked: every circuit")
                            .small()
                            .weak(),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(LIST_HEIGHT)
                        .show(ui, |ui| {
                            for n in &names {
                                let mut on = self.check.circuits.contains(n);
                                if ui.checkbox(&mut on, shown(n)).changed() {
                                    if on {
                                        self.check.circuits.push(n.clone());
                                        self.check.circuits.sort();
                                    } else {
                                        self.check.circuits.retain(|c| c != n);
                                    }
                                    self.dirty = true;
                                }
                            }
                        });
                });
            });
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
        help::explain(
            ui,
            "Circuits: those checked (none ticked: every one). Solved by: each section's own \
             calculation, or one method for all.",
            "Only the sections of the circuits ticked are solved and checked. Finite elements \
             solve each section with its strings swinging; ruling span is the classic \
             approximation; each section keeps its cable condition (initial, final).",
        );
    }

    /// The rules; a preset chosen (rule file, weather case file).
    fn rules_ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<(PathBuf, PathBuf)> {
        let mut preset = None;
        help::explain(
            ui,
            "A rule: the voltage, the weather cases, the distance and the targets — LiDAR \
             classes and/or the ground.",
            "Voltage: any (each wire with the distance of its own system) or one of the line's. \
             Weather cases: those the rule applies in (none ticked: every case). Distance: one \
             value, a table \"up to kV: distance\" or a base plus an amount per kV. Targets: the \
             LiDAR classes ticked (by number and name of the class scheme, with their colours) \
             and the ground surface of the checks. A preset (a design code's) replaces the rules \
             with its own, with its wind and what it solves.",
        );
        ui.horizontal(|ui| {
            if ui
                .button("Add rule")
                .on_hover_text(
                    "A new row at the end of the table: set its distance and its targets \
                     there (edited in place, kept with the project)",
                )
                .clicked()
            {
                self.rules.push(ZoneRule {
                    name: format!("Rule {}", self.rules.len() + 1),
                    ground: false,
                    ..first_rule()
                });
                self.dirty = true;
            }
            ui.push_id((self.title(), "presets"), |ui| {
                ui.menu_button("Load preset…", |ui| {
                    let list = self.presets(model);
                    if list.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No preset in the project: a design code gives them when it \
                                 generates (Line › Criteria › Design code).",
                            )
                            .small()
                            .weak(),
                        );
                    }
                    for (name, r, w) in list {
                        let file = r
                            .file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                        if ui
                            .button(name)
                            .on_hover_text(format!(
                                "{file}: its rules replace those listed; its weather cases go \
                                 to the model where missing"
                            ))
                            .clicked()
                        {
                            preset = Some((r, w));
                        }
                    }
                });
            });
            if ui
                .button("Import…")
                .on_hover_text("Rules from a rule file (.ohlr): they replace those listed")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "Clearance rules",
                        &ohl_core::files::open_extensions(
                            ohl_clearance::rules::RULE_FILE_EXTENSION,
                        ),
                    )
                    .pick_file()
            {
                match RuleSet::load_with(&p, &(&model.line.defaults).into()) {
                    Ok(set) => {
                        self.take(model, &set);
                        self.dirty = true;
                    }
                    Err(e) => self.status = Some((e, true)),
                }
            }
            if ui
                .button("Export…")
                .on_hover_text("The rules as a rule file (.ohlr), for ohl line --rules too")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "Clearance rules",
                        &[ohl_clearance::rules::RULE_FILE_EXTENSION],
                    )
                    .set_file_name(format!("{}-clearances.ohlr", self.zone.measure.as_str()))
                    .save_file()
            {
                self.status = Some(match self.rule_set().save(&p) {
                    Ok(()) => (format!("exported to {}", p.display()), false),
                    Err(e) => (e, true),
                });
            }
        });
        if let Some((text, error)) = &self.status {
            if *error {
                crate::menu::error_label(ui, text.as_str());
            } else {
                crate::menu::message(ui, egui::RichText::new(text.as_str()).small(), None);
            }
        }
        if self.rules.is_empty() {
            // (Never left so: a window always has a rule.)
            return preset;
        }
        let voltages = line_voltages(model);
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let groups = crate::case_groups::groups(model);
        let classes = cloud_classes(model, &self.rules);
        let mut remove = None;
        let mut duplicate = None;
        let n = self.rules.len();
        egui::ScrollArea::both()
            .id_salt((self.title(), "rules"))
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new((self.title(), "rule-grid"))
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        let vertical = self.zone.measure == Measure::Vertical;
                        for h in [
                            "Name",
                            "Voltages",
                            "Weather cases",
                            "Distance",
                            "Width [m]",
                            "Wires",
                            "Targets",
                            "",
                        ] {
                            if h == "Width [m]" {
                                if vertical {
                                    ui.strong(h).on_hover_text(
                                        "The rule's own half width of the zone across the \
                                         line (none: the window's)",
                                    );
                                }
                                continue;
                            }
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (k, r) in self.rules.iter_mut().enumerate() {
                            self.dirty |= ui
                                .add_sized([150.0, 20.0], egui::TextEdit::singleline(&mut r.name))
                                .changed();
                            self.dirty |= voltage_ui(ui, (self.zone.measure, k), r, &voltages);
                            self.dirty |= cases_ui(ui, (self.zone.measure, k), r, &cases, &groups);
                            self.dirty |= crate::requirement_ui::requirement_ui(
                                ui,
                                ("zone-distance", self.zone.measure.as_str(), k),
                                &mut r.distance,
                                &voltages,
                            );
                            if vertical {
                                self.dirty |= width_ui(ui, r, self.zone.width);
                            }
                            self.dirty |= wires_ui(ui, (self.zone.measure, k), r);
                            self.dirty |=
                                targets_ui(ui, (self.zone.measure, k), r, &classes, model);
                            ui.horizontal(|ui| {
                                if ui.small_button("Duplicate").clicked() {
                                    duplicate = Some(k);
                                }
                                if ui
                                    .add_enabled(n > 1, egui::Button::new("Remove").small())
                                    .on_disabled_hover_text(
                                        "The window always has a rule: change this one",
                                    )
                                    .clicked()
                                {
                                    remove = Some(k);
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        if let Some(k) = duplicate {
            let mut r = self.rules[k].clone();
            r.name.push_str(" (copy)");
            self.rules.insert(k + 1, r);
            self.dirty = true;
        }
        if remove.is_some() {
            self.confirm = remove;
        }
        if let Some(k) = self.confirm
            && k < self.rules.len()
        {
            ui.horizontal(|ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Remove the rule \"{}\"?", self.rules[k].name),
                );
                if ui.button("Remove").clicked() {
                    self.rules.remove(k);
                    self.confirm = None;
                    self.dirty = true;
                }
                if ui.button("Keep").clicked() {
                    self.confirm = None;
                }
            });
        }
        preset
    }

    /// The Run row; true to start a check.
    fn run_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut run = false;
        let mut stop_now = false;
        match &self.running {
            Some(r) => {
                ui.spinner();
                let done = r.done.load(Ordering::Relaxed);
                let elapsed = r.started.elapsed().as_secs_f64();
                if done < r.total {
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
                    ui.label(format!("checking the clearances… {elapsed:.0} s"));
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

    fn results_ui(&self, ui: &mut egui::Ui, model: &Model) -> Option<(Selection, [f64; 3])> {
        let report = match &self.report {
            None => return None,
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return None;
            }
            Some(Ok(r)) => r,
        };
        let n = report.zone.findings.len();
        let mut go = None;
        if let Some(g) = result_banner(ui, report) {
            go = Some(g);
        }
        let head = format!(
            "{n} span{} below a rule; {} LiDAR points read",
            if n == 1 { "" } else { "s" },
            report.zone.points
        );
        ui.label(egui::RichText::new(head).small().weak());
        let mut notes: Vec<String> = Vec::new();
        for (k, low) in report.zone.lowest.iter().enumerate() {
            let name = &report.rules[k].name;
            notes.push(match low {
                Some(f) => format!(
                    "{name}: lowest {:.2} m ({} span {}, {})",
                    f.distance,
                    f.section,
                    f.span + 1,
                    f.position(&report.states)
                ),
                None => format!("{name}: nothing within reach"),
            });
        }
        notes.extend(report.zone.notes.iter().cloned());
        notes.push(crate::app::SURFACE_NOTE.to_owned());
        notes.extend(report.failed.iter().map(|f| format!("not solved: {f}")));
        crate::menu::message(
            ui,
            egui::RichText::new(notes.join("\n")).small().weak(),
            None,
        );
        if n == 0 {
            return go;
        }
        egui::ScrollArea::vertical()
            .id_salt((self.title(), "results"))
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new((self.title(), "result-grid"))
                    .striped(true)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        for (k, h) in [
                            "Span",
                            "Rule",
                            "Required [m]",
                            "To surface [m]",
                            "Wind",
                            "Target",
                            "Too close",
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            if (2..4).contains(&k) {
                                num(ui, |ui| ui.strong(h));
                            } else {
                                ui.strong(h);
                            }
                        }
                        ui.end_row();
                        for f in &report.zone.findings {
                            let span = match f.section.strip_prefix("jumper:") {
                                Some(j) => Selection::Jumper(j.to_owned()),
                                None => Selection::Span {
                                    section: f.section.clone(),
                                    span: f.span,
                                },
                            };
                            // A click anywhere on the row: the views go
                            // to it (owner: OHL-Design UI review).
                            let tip = "Select the span and bring the views to it";
                            let cell = |ui: &mut egui::Ui, t: egui::RichText| {
                                ui.add(egui::Label::new(t).sense(egui::Sense::click()))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .on_hover_text(tip)
                                    .clicked()
                            };
                            let mut clicked = ui.link(span.describe()).on_hover_text(tip).clicked();
                            clicked |= cell(ui, egui::RichText::new(&report.rules[f.rule].name));
                            clicked |= num(ui, |ui| {
                                cell(ui, egui::RichText::new(format!("{:.2}", f.required)))
                            });
                            clicked |= num(ui, |ui| {
                                cell(
                                    ui,
                                    egui::RichText::new(format!("{:.2}", f.distance))
                                        .color(ui.visuals().error_fg_color),
                                )
                            });
                            ui.label(f.wind(&report.states))
                                .on_hover_text(f.position(&report.states));
                            if clicked {
                                go = Some((span.clone(), f.wire));
                            }
                            ui.label(match f.class {
                                None => "ground".into(),
                                Some(c) => format!(
                                    "class {c}{}",
                                    model
                                        .scheme
                                        .class(c)
                                        .map_or(String::new(), |d| format!(" {}", d.name))
                                ),
                            });
                            ui.label(format!(
                                "{} {}",
                                f.count,
                                match (f.class, f.count) {
                                    (None, 1) => "wire piece",
                                    (None, _) => "wire pieces",
                                    (Some(_), 1) => "point",
                                    (Some(_), _) => "points",
                                }
                            ));
                            ui.end_row();
                        }
                    });
            });
        go
    }
}

/// `add` right-aligned in its cell (a number column of a grid).
fn num<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
        .inner
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
fn result_banner(ui: &mut egui::Ui, report: &Report) -> Option<(Selection, [f64; 3])> {
    let worst = report
        .zone
        .findings
        .iter()
        .chain(report.zone.lowest.iter().flatten())
        .min_by(|a, b| (a.distance - a.required).total_cmp(&(b.distance - b.required)));
    let failing = !report.zone.findings.is_empty();
    let color = if failing {
        ui.visuals().error_fg_color
    } else {
        crate::menu::ok_color(ui)
    };
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
            format!(
                "{lead} — worst margin {margin:+.2} m: {}, case {case} ({}, {:.2} m of {:.2} m)",
                finding_span(f).describe(),
                report.rules[f.rule].name,
                f.distance,
                f.required
            )
        }
        None if failing => "✖ Spans below a rule".to_owned(),
        None => "✔ Every span clear (nothing within reach of the rules)".to_owned(),
    };
    let r = egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.5, color))
        .fill(color.gamma_multiply(0.12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(egui::RichText::new(&text).strong().color(color))
                    .sense(egui::Sense::click()),
            )
        })
        .inner;
    let r = match worst {
        Some(_) => r
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Bring the views to it"),
        None => r,
    };
    if r.clicked() {
        return worst.map(|f| (finding_span(f), f.wire));
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
    done: &AtomicUsize,
    cancel: &CancelToken,
    filter: &ohl_weather::CaseFilter,
) -> Result<Report, String> {
    let surfaces: Vec<(String, &ohl_terrain::Terrain)> = surfaces
        .iter()
        .map(|(n, t)| (n.clone(), t.as_ref()))
        .collect();
    let budget = Budget::new()
        .time_limit(TIME_BUDGET)
        .cancel_token(cancel.clone());
    let read = |polylines: &[Vec<[f64; 2]>],
                reach: f64,
                classes: &[u8],
                budget: &Budget,
                visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        let spec = cloud.as_deref().unwrap_or_default();
        ohl_pointcloud::visit_around(spec, polylines, reach, classes, budget, visit).map(|_| ())
    };
    zone::run_zone_check(
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
    )
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
            egui::ScrollArea::vertical()
                .max_height(LIST_HEIGHT)
                .show(ui, |ui| {
                    for (k, c) in cases.iter().enumerate() {
                        changed |= ui.checkbox(&mut ticks[k], c).changed();
                    }
                });
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
            egui::ScrollArea::vertical()
                .max_height(LIST_HEIGHT)
                .show(ui, |ui| {
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
                });
        });
    });
    changed
}
