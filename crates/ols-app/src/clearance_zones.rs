// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks › Ground clearances › **Vertical clearances…** and **Radial
//! clearances…**: a window each with the zone around the wires, a list of
//! rules (voltage, weather cases, distance, LiDAR classes and the ground),
//! and the wind — the final state of each case, or every wind pressure
//! from 0 up to it in steps, from the left and/or the right
//! ([`ols_clearance::zone`]). The rules are kept with the project, in a
//! rule file next to the model (`<model>.vertical-clearances.olsr`,
//! `<model>.radial-clearances.olsr`), which `ols line --rules` reads too.
//! The check runs on its own thread: the wind positions are solved, the
//! LiDAR points read again from the point cloud around the wires.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ols_clearance::zone::{
    self, Imported, Measure, WindState, Zone, ZoneReport, ZoneRule, ZoneTargets,
};
use ols_clearance::{RuleSet, WindSides, WindSteps};
use ols_core::budget::{Budget, CancelToken};

use crate::model::Model;
use crate::selection::Selection;

/// Exit clause of a check: solving every wind position and reading the
/// points. The synthetic line takes milliseconds; a long line with fine
/// steps a few minutes.
const TIME_BUDGET: Duration = Duration::from_secs(900);

/// Height of the scrolling lists (rules, results), points.
const LIST_HEIGHT: f32 = 220.0;

/// What a check found.
#[derive(Debug, Clone)]
pub struct Report {
    /// The rules checked.
    pub rules: Vec<ZoneRule>,
    /// The wind positions.
    pub states: Vec<WindState>,
    /// What was found.
    pub zone: ZoneReport,
    /// Sections that failed to solve.
    pub failed: Vec<String>,
    /// Time to solve the wind positions.
    pub solving: Duration,
    /// Time to check them.
    pub checking: Duration,
}

/// What the window asks of the application.
#[derive(Debug, Default)]
pub struct Out {
    /// Go to a span (and look at this point).
    pub go: Option<(Selection, [f64; 3])>,
    /// New violations to draw (wire point, target).
    pub marks: Option<Vec<([f64; 3], [f64; 3])>>,
}

struct Running {
    rx: Receiver<Result<Report, String>>,
    done: Arc<AtomicUsize>,
    total: usize,
    cancel: CancelToken,
    started: Instant,
}

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
    /// The project's rule file of this window (`None`: not set yet).
    pub file: Option<PathBuf>,
    loaded: bool,
    dirty: bool,
    status: Option<(String, bool)>,
    confirm: Option<usize>,
    running: Option<Running>,
    /// The last check's report.
    pub report: Option<Result<Report, String>>,
}

impl ZoneWindow {
    /// A closed window of `measure`.
    pub fn new(measure: Measure) -> Self {
        Self {
            open: false,
            zone: Zone::new(measure),
            wind: WindSteps::default(),
            rules: Vec::new(),
            file: None,
            loaded: false,
            dirty: false,
            status: None,
            confirm: None,
            running: None,
            report: None,
        }
    }

    fn title(&self) -> &'static str {
        match self.zone.measure {
            Measure::Vertical => "Vertical clearances",
            Measure::Radial => "Radial clearances",
        }
    }

    /// The project's rule file of a window: next to the model.
    pub fn project_file(model_file: &Path, measure: Measure) -> PathBuf {
        let name = model_file
            .file_name()
            .map_or(String::from("line"), |n| n.to_string_lossy().into_owned());
        let stem = name.split('.').next().unwrap_or("line").to_owned();
        ols_core::files::dir_of(model_file)
            .join(format!("{stem}.{}-clearances.olsr", measure.as_str()))
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
            .get_or_insert_with(|| Self::project_file(&model.sources.file, self.zone.measure))
            .clone();
        if file.exists() {
            match RuleSet::load_with(&file, &(&model.line.defaults).into()) {
                Ok(set) => self.take(model, &set),
                Err(e) => self.status = Some((e, true)),
            }
        }
    }

    /// Takes the rules of a rule file (replacing those shown).
    fn take(&mut self, model: &Model, set: &RuleSet) {
        let Imported {
            rules,
            width,
            depth,
            wind,
            notes,
        } = zone::from_rule_set(
            set,
            self.zone.measure,
            |role| model.scheme.codes_with_role(role),
            &line_voltages(model),
        );
        self.rules = rules;
        if let Some(w) = width {
            self.zone.width = w;
        }
        if let Some(d) = depth {
            self.zone.depth = d;
        }
        if let Some(w) = wind {
            self.wind = w;
        }
        self.status = (!notes.is_empty()).then(|| (notes.join("\n"), false));
    }

    /// The rules as a rule file.
    pub fn rule_set(&self) -> RuleSet {
        zone::to_rule_set(self.title(), &self.zone, &self.wind, &self.rules)
    }

    /// Starts a check.
    pub fn start(&mut self, model: &mut Model, ctx: &egui::Context) {
        if self.running.is_some() {
            return;
        }
        model.ensure_check_ground();
        let cases: Vec<ols_weather::WeatherCase> =
            model.cases.iter().map(|c| c.case.clone()).collect();
        let rules = self.rules.clone();
        let states = zone::wind_states(&cases, |n| rules.iter().any(|r| r.applies(n)), &self.wind);
        if rules.is_empty() {
            self.report = Some(Err("no rule: add one first".into()));
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
        let zone = self.zone;
        // The route worked on only (View › Work on route).
        let scope = model.scope().map(|s| s.sections);
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = CancelToken::new();
        let total = states.len();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let (d, c) = (done.clone(), cancel.clone());
        std::thread::spawn(move || {
            let r = run(
                &line, &library, zone, rules, states, cloud, ground, scope, &d, &c,
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
        self.running = None;
        true
    }

    /// The last report, and whether a check runs.
    #[cfg(test)]
    pub fn result(&self) -> (Option<&Result<Report, String>>, bool) {
        (self.report.as_ref(), self.running.is_some())
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &mut Model) -> Out {
        let mut out = Out::default();
        if self.poll()
            && let Some(Ok(r)) = &self.report
        {
            out.marks = Some(r.zone.findings.iter().map(|f| (f.wire, f.target)).collect());
        }
        if !self.open {
            return out;
        }
        let mut open = true;
        let mut run = false;
        egui::Window::new(self.title())
            .open(&mut open)
            .default_width(820.0)
            .show(ctx, |ui| {
                self.help(ui);
                ui.separator();
                self.zone_ui(ui);
                self.wind_ui(ui);
                ui.separator();
                self.rules_ui(ui, model);
                ui.separator();
                run = self.run_ui(ui);
                if let Some(go) = self.results_ui(ui, model) {
                    out.go = Some(go);
                }
            });
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
            self.save();
        }
        out
    }

    fn save(&mut self) {
        let Some(file) = &self.file else {
            return;
        };
        self.status = Some(match self.rule_set().save(file) {
            Ok(()) => (format!("kept in {}", file.display()), false),
            Err(e) => (e, true),
        });
    }

    fn help(&self, ui: &mut egui::Ui) {
        let text = match self.zone.measure {
            Measure::Vertical => {
                "The zone is straight under the wire: a width on each side, across the line \
                 in plan, cut along the line in slices of the depth. A target in the zone is \
                 compared with the wire's lowest point over its slice; a target above the wire \
                 with its highest point. The ground counts in the same zone."
            }
            Measure::Radial => {
                "The direct 3D distance from the wire (each subconductor of a bundle) to the \
                 targets: LiDAR points of the classes ticked, the ground."
            }
        };
        ui.label(egui::RichText::new(text).small().weak());
    }

    fn zone_ui(&mut self, ui: &mut egui::Ui) {
        if self.zone.measure != Measure::Vertical {
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
    }

    fn wind_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            self.dirty |= ui
                .checkbox(&mut self.wind.stepped, "Step the wind")
                .on_hover_text(
                    "Solve every wind pressure from 0 up to the case's in steps: the wires at \
                     every step form the surface checked. Unticked: the case's wind only.",
                )
                .changed();
            ui.add_enabled_ui(self.wind.stepped, |ui| {
                ui.label("step Pa");
                self.dirty |= ui
                    .add(
                        egui::DragValue::new(&mut self.wind.step)
                            .speed(5.0)
                            .range(1.0..=10_000.0),
                    )
                    .changed();
            });
            ui.label("Wind from");
            let name = |s: WindSides| match s {
                WindSides::Both => "the left and the right",
                WindSides::Left => "the left only",
                WindSides::Right => "the right only",
            };
            egui::ComboBox::from_id_salt((self.title(), "sides"))
                .selected_text(name(self.wind.sides))
                .show_ui(ui, |ui| {
                    for s in WindSides::ALL {
                        self.dirty |= ui
                            .selectable_value(&mut self.wind.sides, s, name(s))
                            .changed();
                    }
                });
        });
        ui.label(
            egui::RichText::new("Left and right looking ahead along the line.")
                .small()
                .weak(),
        );
    }

    fn rules_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        ui.horizontal(|ui| {
            ui.strong("Rules");
            if ui.button("Add rule").clicked() {
                self.rules.push(ZoneRule {
                    name: format!("Rule {}", self.rules.len() + 1),
                    voltage: None,
                    cases: Vec::new(),
                    distance: 5.0,
                    classes: Vec::new(),
                    ground: false,
                    wires: ols_clearance::Wires::Both,
                });
                self.dirty = true;
            }
            if ui
                .button("Import…")
                .on_hover_text("Rules from a rule file (.olsr): they replace those listed")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "Clearance rules",
                        &[ols_clearance::rules::RULE_FILE_EXTENSION],
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
                .on_hover_text("The rules as a rule file (.olsr), for ols line --rules too")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "Clearance rules",
                        &[ols_clearance::rules::RULE_FILE_EXTENSION],
                    )
                    .set_file_name(format!("{}-clearances.olsr", self.zone.measure.as_str()))
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
            ui.label(
                egui::RichText::new(
                    "No rule yet: Add rule, then choose the voltage, the weather cases, the \
                     distance and the targets.",
                )
                .small()
                .weak(),
            );
            return;
        }
        let voltages = line_voltages(model);
        let cases: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        let classes = cloud_classes(model, &self.rules);
        let mut remove = None;
        let mut duplicate = None;
        egui::ScrollArea::vertical()
            .id_salt((self.title(), "rules"))
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new((self.title(), "rule-grid"))
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for h in [
                            "name",
                            "voltage",
                            "weather cases",
                            "distance m",
                            "wires",
                            "targets",
                            "",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (k, r) in self.rules.iter_mut().enumerate() {
                            self.dirty |= ui
                                .add_sized([150.0, 20.0], egui::TextEdit::singleline(&mut r.name))
                                .changed();
                            self.dirty |= voltage_ui(ui, (self.zone.measure, k), r, &voltages);
                            self.dirty |= cases_ui(ui, (self.zone.measure, k), r, &cases);
                            self.dirty |= ui
                                .add(
                                    egui::DragValue::new(&mut r.distance)
                                        .speed(0.05)
                                        .range(0.0..=200.0)
                                        .max_decimals(3),
                                )
                                .changed();
                            self.dirty |= wires_ui(ui, (self.zone.measure, k), r);
                            self.dirty |= targets_ui(ui, (self.zone.measure, k), r, &classes);
                            ui.horizontal(|ui| {
                                if ui.small_button("Duplicate").clicked() {
                                    duplicate = Some(k);
                                }
                                if ui.small_button("Remove").clicked() {
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
                    egui::Color32::from_rgb(255, 180, 90),
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
    }

    /// The Run row; true to start a check.
    fn run_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut run = false;
        ui.horizontal(|ui| match &self.running {
            Some(r) => {
                ui.spinner();
                let done = r.done.load(Ordering::Relaxed);
                ui.label(if done < r.total {
                    format!("solving the wind positions: {done} of {}", r.total)
                } else {
                    "checking the clearances…".into()
                });
                ui.label(format!("{:.1} s", r.started.elapsed().as_secs_f64()));
                if ui.button("Stop").clicked() {
                    r.cancel.cancel();
                }
                ui.ctx().request_repaint_after(Duration::from_millis(200));
            }
            None => {
                run = ui
                    .button("Run")
                    .on_hover_text(
                        "Solve the wind positions of the rules' weather cases and check \
                             every span (in the background)",
                    )
                    .clicked();
                if let Some(Ok(r)) = &self.report {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} wind positions solved in {:.1?}, checked in {:.1?}",
                            r.states.len(),
                            r.solving,
                            r.checking
                        ))
                        .small()
                        .weak(),
                    );
                }
            }
        });
        run
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
        let head = format!(
            "{n} span{} below a rule; {} LiDAR points read",
            if n == 1 { "" } else { "s" },
            report.zone.points
        );
        if n == 0 {
            ui.colored_label(egui::Color32::LIGHT_GREEN, head);
        } else {
            ui.colored_label(egui::Color32::from_rgb(255, 120, 100), head);
        }
        let mut notes: Vec<String> = Vec::new();
        for (k, low) in report.zone.lowest.iter().enumerate() {
            let name = &report.rules[k].name;
            notes.push(match low {
                Some(f) => format!(
                    "{name}: lowest {:.2} m ({} span {}, {})",
                    f.distance,
                    f.section,
                    f.span + 1,
                    report.states[f.state].describe()
                ),
                None => format!("{name}: nothing within reach"),
            });
        }
        notes.extend(report.zone.notes.iter().cloned());
        notes.extend(report.failed.iter().map(|f| format!("not solved: {f}")));
        crate::menu::message(
            ui,
            egui::RichText::new(notes.join("\n")).small().weak(),
            None,
        );
        if n == 0 {
            return None;
        }
        let mut go = None;
        egui::ScrollArea::vertical()
            .id_salt((self.title(), "results"))
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new((self.title(), "result-grid"))
                    .striped(true)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        for h in [
                            "span",
                            "rule",
                            "required m",
                            "found m",
                            "wind",
                            "target",
                            "too close",
                        ] {
                            ui.strong(h);
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
                            if ui
                                .link(span.describe())
                                .on_hover_text("Select the span and bring the views to it")
                                .clicked()
                            {
                                go = Some((span, f.wire));
                            }
                            ui.label(&report.rules[f.rule].name);
                            ui.label(format!("{:.2}", f.required));
                            ui.colored_label(
                                egui::Color32::LIGHT_RED,
                                format!("{:.2}", f.distance),
                            );
                            let s = &report.states[f.state];
                            ui.label(match s.side {
                                Some(side) => {
                                    format!("{:.0} Pa from the {}", s.pressure, side.as_str())
                                }
                                None => "no wind".into(),
                            })
                            .on_hover_text(s.describe());
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

/// The check, on its own thread.
#[allow(clippy::too_many_arguments)]
fn run(
    line: &ols_line::Line,
    library: &ols_cables::Library,
    zone: Zone,
    rules: Vec<ZoneRule>,
    states: Vec<WindState>,
    cloud: Option<String>,
    ground: Option<Arc<ols_terrain::Terrain>>,
    scope: Option<std::collections::BTreeSet<String>>,
    done: &AtomicUsize,
    cancel: &CancelToken,
) -> Result<Report, String> {
    let mut sections = zone::sections_needed(line, &rules);
    if let Some(on) = &scope {
        sections.retain(|s| on.contains(s));
        if sections.is_empty() {
            return Err("no section of the rules' voltages on the route worked on".into());
        }
    }
    let budget = Budget::new()
        .time_limit(TIME_BUDGET)
        .cancel_token(cancel.clone());
    let started = Instant::now();
    let solved = zone::solve_states(
        line,
        library,
        &states,
        &sections,
        zone::jumpers_needed(&rules),
        &budget,
        done,
    )?;
    let solving = started.elapsed();
    let started = Instant::now();
    // Points around the wires at every wind position, within reach.
    let reach = rules.iter().map(|r| r.distance).fold(0.0, f64::max)
        + line.defaults.clearance_search_margin
        + zone.width
        + 1.0;
    let polylines: Vec<Vec<[f64; 2]>> = solved
        .shapes
        .iter()
        .flatten()
        .flat_map(|s| &s.spans)
        .map(|s| s.points.iter().map(|p| [p[0], p[1]]).collect())
        .chain(
            solved
                .jumpers
                .iter()
                .flatten()
                .map(|j| j.points.iter().map(|p| [p[0], p[1]]).collect()),
        )
        .collect();
    let mut feed = |classes: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        let Some(spec) = &cloud else {
            return Ok(());
        };
        let source = ols_pointcloud::open_source(spec)?;
        let corridor = ols_lidar::Corridor::new(&polylines, reach);
        source.visit(Some(&corridor), Some(classes), &budget, visit)?;
        Ok(())
    };
    let report = zone::check_zones(
        line,
        &zone,
        &rules,
        &states,
        &solved.shapes,
        &solved.jumpers,
        ZoneTargets {
            ground: ground.as_deref(),
            points: cloud.is_some().then_some(&mut feed as _),
        },
        &budget,
    )?;
    Ok(Report {
        rules,
        states,
        zone: report,
        failed: solved.failed,
        solving,
        checking: started.elapsed(),
    })
}

/// The nominal voltages of the line's sections, kV (0: earthed), lowest
/// first.
fn line_voltages(model: &Model) -> Vec<f64> {
    let mut v: Vec<f64> = ols_clearance::voltages_of(&model.line)
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.map_or(0.0, |p| p.nominal_kv))
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

/// "220 kV", "earthed (0 kV)".
fn voltage_name(kv: f64) -> String {
    if kv == 0.0 {
        "earthed (0 kV)".into()
    } else {
        format!("{kv} kV")
    }
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

/// The voltage of a rule: any, or one of the line's.
fn voltage_ui(ui: &mut egui::Ui, id: (Measure, usize), r: &mut ZoneRule, voltages: &[f64]) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(("zone-voltage", id.0.as_str(), id.1))
        .selected_text(r.voltage.map_or("any".into(), voltage_name))
        .width(120.0)
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(&mut r.voltage, None, "any").changed();
            let mut all: Vec<f64> = voltages.to_vec();
            if let Some(v) = r.voltage
                && !all.iter().any(|x| (x - v).abs() < 1e-6)
            {
                all.push(v);
            }
            for v in all {
                changed |= ui
                    .selectable_value(&mut r.voltage, Some(v), voltage_name(v))
                    .changed();
            }
        });
    changed
}

/// The weather cases of a rule, ticked (none ticked: every case).
fn cases_ui(ui: &mut egui::Ui, id: (Measure, usize), r: &mut ZoneRule, cases: &[String]) -> bool {
    let mut changed = false;
    let on: Vec<bool> = cases
        .iter()
        .map(|c| r.cases.iter().any(|p| ols_core::pattern::matches(p, c)))
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

/// The wires a rule checks: the sections' spans, the jumpers or both.
fn wires_ui(ui: &mut egui::Ui, id: (Measure, usize), r: &mut ZoneRule) -> bool {
    use ols_clearance::Wires;
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

/// The targets of a rule: LiDAR classes and the ground, ticked.
fn targets_ui(
    ui: &mut egui::Ui,
    id: (Measure, usize),
    r: &mut ZoneRule,
    classes: &[(u8, String)],
) -> bool {
    let mut changed = false;
    let mut parts: Vec<String> = r.classes.iter().map(u8::to_string).collect();
    if r.ground {
        parts.insert(0, "ground".into());
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
            changed |= ui
                .checkbox(&mut r.ground, "Ground surface")
                .on_hover_text("The terrain surface of the checks (Terrain menu)")
                .changed();
            ui.separator();
            ui.label(egui::RichText::new("LiDAR classes").small().weak());
            egui::ScrollArea::vertical()
                .max_height(LIST_HEIGHT)
                .show(ui, |ui| {
                    for (code, name) in classes {
                        let mut on = r.classes.contains(code);
                        if ui.checkbox(&mut on, format!("{code} {name}")).changed() {
                            if on {
                                r.classes.push(*code);
                                r.classes.sort_unstable();
                            } else {
                                r.classes.retain(|c| c != code);
                            }
                            changed = true;
                        }
                    }
                });
        });
    });
    changed
}
