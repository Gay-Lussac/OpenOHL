// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Thermal rating, in four pages (owner, 2026-11-04: "review the
//! thermal menu as all the others in the app with a setup and result and
//! explanations of the workings (maybe also separate the 4 things that
//! work in 4 sub menus)"), each with its Setup and Results:
//!
//! - **Ampacity**: per section and ambient case (`.ohlta`), the current at
//!   a maximum conductor temperature — computed as the window is drawn
//!   (microseconds each);
//! - **Temperature at a current**: per section, the steady conductor
//!   temperature for its circuit's current (the system's operating current
//!   of the line file, or one typed here) in each ambient case — at once;
//! - **Step change**: the conductor temperature over time after the
//!   current steps from one value to another, per conductor — at once;
//! - **Clearance**: on demand, in the background, either the
//!   clearance-limited temperature of every section
//!   ([`ohl_thermal::limit::clearance_limits_of`]) or the clearance check
//!   at the current's temperature ([`ohl_thermal::limit::clearance_at`]):
//!   each section solved at other temperatures in a weather case and
//!   checked by the ground and point rules of the chosen rule file; the
//!   clearances found drawn in the views.
//!
//! The model is not changed.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_clearance::{Rule, Target};
use ohl_core::budget::CancelToken;
use ohl_thermal::limit::{Governing, Limit, LimitOptions, SectionCheck, SectionLimit, Targets};
use ohl_thermal::sections::{Current, SectionHeat};
use ohl_thermal::{AmbientCases, Conductor, Method};

use crate::check_help as help;
use crate::check_ui::{self, Cell, Col};
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::ALL;

/// The window's four pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    /// The current at a maximum temperature.
    #[default]
    Ampacity,
    /// The steady temperature at a current.
    Temperature,
    /// The temperature over time after a step of current.
    Step,
    /// The clearance-limited temperature, and the clearance at the
    /// current's temperature.
    Clearance,
}

impl Page {
    /// Every page, in the order shown.
    pub const ALL: [Self; 4] = [
        Self::Ampacity,
        Self::Temperature,
        Self::Step,
        Self::Clearance,
    ];

    /// Its name, at the window's top.
    pub fn title(self) -> &'static str {
        match self {
            Self::Ampacity => "Ampacity",
            Self::Temperature => "Temperature at a current",
            Self::Step => "Step change",
            Self::Clearance => "Clearance",
        }
    }

    /// What it computes, on hover of its name.
    fn hover(self) -> &'static str {
        match self {
            Self::Ampacity => {
                "The current each section may carry at a maximum conductor temperature, per \
                 ambient case"
            }
            Self::Temperature => {
                "The steady conductor temperature of each section at its circuit's current, per \
                 ambient case"
            }
            Self::Step => {
                "The conductor temperature over time after the current steps from one value to \
                 another"
            }
            Self::Clearance => {
                "The highest temperature the ground and point clearances allow, or the clearance \
                 check at the current's temperature"
            }
        }
    }

    /// The id of its tabs, body and footer.
    fn id(self) -> &'static str {
        match self {
            Self::Ampacity => "thermal-ampacity",
            Self::Temperature => "thermal-temperature",
            Self::Step => "thermal-step",
            Self::Clearance => "thermal-clearance",
        }
    }
}

/// What the Clearance page runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClearanceMode {
    /// The highest temperature at which the clearances are met (searched).
    #[default]
    Limit,
    /// The clearances at the temperature the current gives (one solve per
    /// section).
    AtCurrent,
}

/// A run's answer.
enum Answer {
    Limits(Result<Vec<SectionLimit>, String>),
    Checks(Result<Vec<SectionCheck>, String>),
}

/// A clearance run under way.
struct Job {
    rx: Receiver<Answer>,
    /// Sections done.
    done: Arc<AtomicUsize>,
    sections: usize,
    cancel: CancelToken,
    started: Instant,
    mode: ClearanceMode,
}

/// The clearance check at the current: what it was run with and found.
pub struct CurrentCheck {
    /// Per section in scope: its current and its temperatures per ambient
    /// case (all of them, also those not checked).
    pub heats: Vec<SectionHeat>,
    /// The ambient case each section was checked in (its index), the
    /// hottest or the one chosen.
    pub cases: Vec<Option<usize>>,
    /// The checks (of the sections with a temperature), or why none.
    pub result: Result<Vec<SectionCheck>, String>,
}

/// One curve of the Step change page: the sections with the same wire,
/// bundle and currents.
#[derive(Debug, Clone)]
pub struct StepCurve {
    /// Wire type.
    pub cable: String,
    /// Subconductors.
    pub bundle: u32,
    /// The sections.
    pub sections: Vec<String>,
    /// From and to, A per phase.
    pub from: f64,
    pub to: f64,
    /// Steady temperatures before and long after the step, °C.
    pub start: Result<f64, String>,
    pub end: Result<f64, String>,
    /// (time s, temperature °C), or why none.
    pub points: Result<Vec<(f64, f64)>, String>,
}

impl StepCurve {
    /// Its name in the plot and the table.
    pub fn label(&self) -> String {
        if self.bundle > 1 {
            format!("{} ×{}, {:.0} A", self.cable, self.bundle, self.to)
        } else {
            format!("{}, {:.0} A", self.cable, self.to)
        }
    }

    /// The first time it reaches `t` °C, s.
    pub fn reaches(&self, t: f64) -> Option<f64> {
        self.points
            .as_ref()
            .ok()?
            .iter()
            .find(|(_, x)| *x >= t)
            .map(|(time, _)| *time)
    }
}

/// An ampacity in words, its heat terms, its value (A per phase).
type Rating = (String, String, Option<f64>);

/// A result made again only when what it comes from changed: its key, it,
/// and how long it took.
type Kept<T> = std::cell::RefCell<Option<(u64, T, Duration)>>;

/// The Thermal rating window's state.
pub struct ThermalWindow {
    /// The window is open.
    pub open: bool,
    /// The page shown.
    pub page: Page,
    /// The ambient case file.
    pub file: Option<PathBuf>,
    /// Its cases, or why they could not be read.
    pub ambient: Option<Result<AmbientCases, String>>,
    /// Rating method.
    pub method: Method,
    /// Maximum conductor temperature, °C (Ampacity).
    pub max_temperature: f64,
    /// The circuits' current: `None` each system's operating current
    /// (Line › Electrical systems), else this, A per phase (Temperature,
    /// Step change, Clearance at the current).
    pub current: Option<f64>,
    /// The value typed for [`ThermalWindow::current`] (kept while the
    /// line's is used).
    pub typed_current: f64,
    /// Step change: the current before the step, A per phase.
    pub step_from: f64,
    /// Step change: how long, s.
    pub duration: f64,
    /// Step change: the time step, s (`None`: the line's default values').
    pub time_step: Option<f64>,
    /// Step change: a table row every this many s.
    pub print_every: f64,
    /// Step change: the ambient case (index).
    pub step_case: usize,
    /// The weather case heated (index in the model's cases).
    pub base_case: usize,
    /// Temperature range and resolution of the search.
    pub options: LimitOptions,
    /// What the Clearance page runs.
    pub mode: ClearanceMode,
    /// Clearance at the current: the ambient case (`None`: each
    /// section's hottest).
    pub check_case: Option<usize>,
    /// The sections rated and searched (2 Scope; owner, 2026-10-31).
    pub scope: check_ui::Scope,
    /// The last search's results (or why there are none).
    pub limits: Option<Result<Vec<SectionLimit>, String>>,
    /// The last clearance check at the current.
    pub checks: Option<CurrentCheck>,
    /// How many sections the last search took (its time per section
    /// estimates the next), and how long.
    searched: usize,
    /// How long the last search took.
    pub took: Option<Duration>,
    /// How long the last check at the current took.
    pub took_check: Option<Duration>,
    /// The time of one solve and check, from the last run.
    per_solve: Option<Duration>,
    job: Option<Job>,
    /// What the last search and check were computed from
    /// ([`crate::revision`]), and their own inputs (a hash).
    stamp: crate::revision::Stamp,
    stamp_check: crate::revision::Stamp,
    limit_inputs: u64,
    check_inputs: u64,
    /// Runs arrived (a new one opens Results and is drawn anew).
    generation: u64,
    /// The marks last drawn: (shown, mode, generation).
    drawn: Option<(bool, ClearanceMode, u64)>,
    /// The pages' results as last computed.
    ratings: Kept<Vec<Vec<Rating>>>,
    heats: Kept<Vec<SectionHeat>>,
    steps: Kept<Vec<StepCurve>>,
}

impl Default for ThermalWindow {
    fn default() -> Self {
        Self {
            open: false,
            page: Page::default(),
            file: None,
            ambient: None,
            method: Method::default(),
            max_temperature: 80.0,
            current: None,
            typed_current: 600.0,
            step_from: 300.0,
            duration: 3600.0,
            time_step: None,
            print_every: 300.0,
            step_case: 0,
            base_case: 0,
            options: LimitOptions::default(),
            mode: ClearanceMode::default(),
            check_case: None,
            scope: Default::default(),
            limits: None,
            checks: None,
            searched: 0,
            took: None,
            took_check: None,
            per_solve: None,
            job: None,
            stamp: Default::default(),
            stamp_check: Default::default(),
            limit_inputs: 0,
            check_inputs: 0,
            generation: 0,
            drawn: None,
            ratings: Default::default(),
            heats: Default::default(),
            steps: Default::default(),
        }
    }
}

/// A hash of what `parts` print.
fn key_of(parts: &[&dyn std::fmt::Debug]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        format!("{p:?}").hash(&mut h);
    }
    h.finish()
}

/// "812 A" in words with its heat terms, or why there is none.
fn terms(b: &ohl_thermal::HeatBalance) -> String {
    format!(
        "Joule {:.1}, solar {:.1}, convection {:.1} ({}), radiation {:.1} W/m",
        b.joule,
        b.solar.heat,
        b.convection,
        b.convection_mode.as_str(),
        b.radiation
    )
}

/// "Rule: span 2 vertical 4.99 m, 5.00 m required".
fn governing_words(g: &Governing) -> String {
    format!(
        "{}: span {} {} {:.2} m, {:.2} m required",
        g.rule,
        g.span + 1,
        g.measure,
        g.clearance,
        g.required
    )
}

impl ThermalWindow {
    /// Opens the window; the first time with the project's `.ohlta` file
    /// (next to a line file), if there is one.
    pub fn show(&mut self, model: &Model) {
        self.open = true;
        if self.file.is_none()
            && let Some(p) = model.project_files("ambient").into_iter().next()
        {
            self.choose(p);
        }
    }

    /// Reads ambient case file `path`.
    pub fn choose(&mut self, path: PathBuf) {
        self.ambient = Some(AmbientCases::load(&path));
        self.file = Some(path);
    }

    /// The ambient cases read (none: none read).
    pub fn cases(&self) -> &[ohl_thermal::Ambient] {
        match &self.ambient {
            Some(Ok(set)) => &set.cases,
            _ => &[],
        }
    }

    /// Whether a clearance run is under way.
    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    /// The circuits' current as the pages take it.
    pub fn current_source(&self) -> Current {
        match self.current {
            Some(i) => Current::Given(i),
            None => Current::Line,
        }
    }

    /// Takes a finished run.
    pub fn poll(&mut self) {
        let Some(job) = &self.job else {
            return;
        };
        let r = match job.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => match job.mode {
                ClearanceMode::Limit => Answer::Limits(Err("the search stopped".into())),
                ClearanceMode::AtCurrent => Answer::Checks(Err("the check stopped".into())),
            },
        };
        let took = job.started.elapsed();
        let sections = job.sections.max(1);
        match r {
            Answer::Limits(r) => {
                match &r {
                    Ok(v) => {
                        log::info!(
                            "thermal: clearance limits of {} sections in {took:.1?}",
                            v.len()
                        );
                        let solves: u32 = v.iter().map(|l| l.solves).sum();
                        if solves > 0 {
                            self.per_solve = Some(took / solves);
                        }
                    }
                    Err(e) => log::warn!("thermal: {e}"),
                }
                self.took = Some(took);
                self.limits = Some(r);
                self.stamp.arrived();
            }
            Answer::Checks(r) => {
                match &r {
                    Ok(v) => {
                        log::info!(
                            "thermal: clearance at the current of {} sections in {took:.1?}",
                            v.len()
                        );
                        self.per_solve = Some(took / sections as u32);
                    }
                    Err(e) => log::warn!("thermal: {e}"),
                }
                self.took_check = Some(took);
                if let Some(c) = &mut self.checks {
                    c.result = r;
                }
                self.stamp_check.arrived();
            }
        }
        self.generation += 1;
        self.job = None;
    }

    /// The inputs of the search besides the model (a hash).
    fn limit_key(&self) -> u64 {
        key_of(&[&self.base_case, &self.options])
    }

    /// The inputs of the check at the current besides the model (a hash).
    fn check_key(&self) -> u64 {
        key_of(&[
            &self.base_case,
            &self.method,
            &self.current,
            &self.check_case,
            &self.cases(),
        ])
    }

    /// What the result of the Clearance page's mode was computed from
    /// that changed since, in words (`None`: nothing, or none computed):
    /// the line, the LiDAR points, the ground ([`crate::revision`]), or the
    /// page's own inputs.
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        match self.mode {
            ClearanceMode::Limit => matches!(self.limits, Some(Ok(_)))
                .then(|| {
                    self.stamp.out_of_date(model, USES).or_else(|| {
                        (self.limit_key() != self.limit_inputs)
                            .then(|| "the weather case heated or the search range".to_owned())
                    })
                })
                .flatten(),
            ClearanceMode::AtCurrent => matches!(&self.checks, Some(c) if c.result.is_ok())
                .then(|| {
                    self.stamp_check.out_of_date(model, USES).or_else(|| {
                        (self.check_key() != self.check_inputs).then(|| {
                            "the current, the ambient cases, the method or the weather case"
                                .to_owned()
                        })
                    })
                })
                .flatten(),
        }
    }

    /// The clearances the Clearance page found, to draw in the views
    /// (from the wire to the target): the search's failing clearance just
    /// above each section's limit, or the check's failing clearances at
    /// the current (all of them when none fails: the least margins).
    pub fn marks(&self) -> Vec<([f64; 3], [f64; 3])> {
        match self.mode {
            ClearanceMode::Limit => match &self.limits {
                Some(Ok(v)) => v
                    .iter()
                    .filter_map(|l| l.governing.as_ref())
                    .map(|g| (g.at, g.target))
                    .collect(),
                _ => Vec::new(),
            },
            ClearanceMode::AtCurrent => match self.checks.as_ref().map(|c| &c.result) {
                Some(Ok(v)) => {
                    let found: Vec<&Governing> = v
                        .iter()
                        .filter_map(|c| c.result.as_ref().ok()?.as_ref())
                        .collect();
                    let failing: Vec<&&Governing> = found.iter().filter(|g| !g.met()).collect();
                    if failing.is_empty() {
                        found.iter().map(|g| (g.at, g.target)).collect()
                    } else {
                        failing.iter().map(|g| (g.at, g.target)).collect()
                    }
                }
                _ => Vec::new(),
            },
        }
    }

    /// The marks to draw now, when they changed (shown while the window
    /// is open, as the other checks' marks): `Some(empty)` takes them off.
    fn marks_change(&mut self) -> Option<Vec<([f64; 3], [f64; 3])>> {
        let shown = self.open
            && match self.mode {
                ClearanceMode::Limit => matches!(self.limits, Some(Ok(_))),
                ClearanceMode::AtCurrent => {
                    matches!(&self.checks, Some(c) if c.result.is_ok())
                }
            };
        let key = (shown, self.mode, self.generation);
        if self.drawn == Some(key) {
            return None;
        }
        let first = self.drawn.is_none();
        self.drawn = Some(key);
        if shown {
            Some(self.marks())
        } else if first {
            None
        } else {
            Some(Vec::new())
        }
    }

    /// Starts the Clearance page's run (its mode) on the model as it is
    /// now.
    pub fn start(&mut self, model: &Model, rules: Vec<Rule>, ctx: &egui::Context) {
        let Some(base) = model.cases.get(self.base_case).map(|c| c.case.clone()) else {
            return;
        };
        let only = self.scope.keeps(&model.line);
        let mode = self.mode;
        // At the current: the temperatures first (microseconds each).
        let temps: Vec<(String, f64)> = match mode {
            ClearanceMode::Limit => Vec::new(),
            ClearanceMode::AtCurrent => {
                let heats = ohl_thermal::sections::section_temperatures(
                    &model.line,
                    &model.library,
                    self.method,
                    self.cases(),
                    self.current_source(),
                    only.as_ref(),
                );
                let cases: Vec<Option<usize>> = heats
                    .iter()
                    .map(|h| match self.check_case {
                        None => h.hottest().map(|(k, _)| k),
                        Some(k) => h.in_case(k).map(|_| k),
                    })
                    .collect();
                let temps = heats
                    .iter()
                    .zip(&cases)
                    .filter_map(|(h, k)| Some((h.section.clone(), h.in_case((*k)?)?)))
                    .collect();
                self.checks = Some(CurrentCheck {
                    heats,
                    cases,
                    result: Err("not run yet".into()),
                });
                self.check_inputs = self.check_key();
                self.stamp_check.started(model.revision());
                temps
            }
        };
        if mode == ClearanceMode::Limit {
            self.limit_inputs = self.limit_key();
            self.stamp.started(model.revision());
            self.limits = None;
        }
        let line = model.line.clone();
        let library = model.library.clone();
        let ground = model.ground_for_checks();
        // The surfaces the ground rules name (read on the run's thread).
        let named =
            crate::model::NamedSurfaces::of(model, rules.iter().filter_map(|r| r.surface.clone()));
        let cloud = model.cloud.clone();
        let scheme = model.scheme.clone();
        let corridor = model.sources.corridor;
        let options = self.options;
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = CancelToken::new();
        let (tx, rx) = channel();
        let (c, d, token) = (ctx.clone(), done.clone(), cancel.clone());
        let sections = match mode {
            ClearanceMode::Limit => only.as_ref().map_or(line.sections.len(), BTreeSet::len),
            ClearanceMode::AtCurrent => temps.len(),
        };
        if mode == ClearanceMode::Limit {
            self.searched = sections;
        }
        std::thread::spawn(move || {
            // What the rules check against, then the run.
            let run = |targets: &mut Targets| -> Answer {
                let mut progress = |k: usize, _: usize| {
                    d.store(k, Ordering::Relaxed);
                    c.request_repaint();
                };
                match mode {
                    ClearanceMode::Limit => {
                        let budget = ohl_thermal::limit::default_budget(&options, sections)
                            .cancel_token(token.clone());
                        Answer::Limits(ohl_thermal::limit::clearance_limits_of(
                            &line,
                            &library,
                            &base,
                            &rules,
                            targets,
                            &options,
                            only.as_ref(),
                            &budget,
                            &mut progress,
                        ))
                    }
                    ClearanceMode::AtCurrent => {
                        if temps.is_empty() {
                            return Answer::Checks(Err(
                                "no section in scope has a temperature at the current (Results: \
                                 why, per section)"
                                    .into(),
                            ));
                        }
                        let budget =
                            ohl_thermal::limit::check_budget(sections).cancel_token(token.clone());
                        Answer::Checks(ohl_thermal::limit::clearance_at(
                            &line,
                            &library,
                            &base,
                            &rules,
                            targets,
                            &temps,
                            &budget,
                            &mut progress,
                        ))
                    }
                }
            };
            let fail = |e: String| match mode {
                ClearanceMode::Limit => Answer::Limits(Err(e)),
                ClearanceMode::AtCurrent => Answer::Checks(Err(e)),
            };
            let answer = (|| -> Result<Answer, String> {
                let classes: Vec<Vec<u8>> = rules
                    .iter()
                    .map(|r| r.point_classes(|role| scheme.codes_with_role(role)))
                    .collect();
                // The points of the point rules' classes, read once.
                let wanted: Vec<u8> = rules
                    .iter()
                    .zip(&classes)
                    .filter(|(r, _)| {
                        matches!(r.target, Target::Points { .. })
                            && ohl_clearance::applies(r, &base.name)
                    })
                    .flat_map(|(_, c)| c.iter().copied())
                    .collect();
                let mut points: Vec<(f64, f64, f64, u8)> = Vec::new();
                if let (Some(spec), false) = (&cloud, wanted.is_empty()) {
                    let (file, name) = ohl_lidar::cloud_spec(spec);
                    let source =
                        ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
                    let corridor = ohl_lidar::Corridor::along_spans(&line, corridor);
                    // Exit clause: ~1 s per 10⁷ points decoded.
                    let budget = ohl_core::budget::Budget::new()
                        .time_limit(Duration::from_secs(600))
                        .cancel_token(token.clone());
                    ohl_pointcloud::PointSource::visit(
                        &source,
                        Some(&corridor),
                        Some(&wanted),
                        &budget,
                        &mut |x, y, z, k| points.push((x, y, z, k)),
                    )?;
                }
                let mut feed = |_: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
                    for &(x, y, z, k) in &points {
                        visit(x, y, z, k);
                    }
                    Ok(())
                };
                let named = named.read();
                let surfaces: Vec<(String, &ohl_terrain::Terrain)> =
                    named.iter().map(|(n, t)| (n.clone(), t.as_ref())).collect();
                let mut targets = Targets {
                    ground: ground.as_deref(),
                    surfaces: &surfaces,
                    point_classes: &classes,
                    points: (!points.is_empty()).then_some(&mut feed as _),
                };
                Ok(run(&mut targets))
            })();
            let _ = tx.send(answer.unwrap_or_else(fail));
            c.request_repaint();
        });
        self.job = Some(Job {
            rx,
            done,
            sections,
            cancel,
            started: Instant::now(),
            mode,
        });
    }

    /// Per section in scope: its conductor (the bundle's subconductor),
    /// or why there is none.
    fn conductors<'a>(
        &self,
        model: &'a Model,
    ) -> Vec<(&'a ohl_line::Section, Result<Conductor, String>)> {
        let ids = self.scope.sections_of(&model.line);
        model
            .line
            .sections
            .iter()
            .filter(|s| ids.contains(&s.id))
            .map(|s| {
                let c = ohl_thermal::sections::conductor_of(&model.line, &model.library, &s.cable);
                (s, c)
            })
            .collect()
    }

    /// The Temperature page's results, made again when their inputs
    /// changed.
    fn heats_now(
        &self,
        model: &Model,
    ) -> std::cell::Ref<'_, Option<(u64, Vec<SectionHeat>, Duration)>> {
        let only = self.scope.keeps(&model.line);
        let key = key_of(&[
            &model.line_version,
            &self.method,
            &self.current,
            &self.cases(),
            &only,
        ]);
        if self
            .heats
            .borrow()
            .as_ref()
            .is_none_or(|(k, _, _)| *k != key)
        {
            let t = Instant::now();
            let v = ohl_thermal::sections::section_temperatures(
                &model.line,
                &model.library,
                self.method,
                self.cases(),
                self.current_source(),
                only.as_ref(),
            );
            *self.heats.borrow_mut() = Some((key, v, t.elapsed()));
        }
        self.heats.borrow()
    }

    /// The time step of the Step change page, s.
    fn time_step_of(&self, model: &Model) -> f64 {
        self.time_step
            .unwrap_or(model.line.defaults.thermal_time_step)
    }

    /// The Step change page's curves, made again when their inputs
    /// changed.
    fn steps_now(
        &self,
        model: &Model,
    ) -> std::cell::Ref<'_, Option<(u64, Vec<StepCurve>, Duration)>> {
        let only = self.scope.keeps(&model.line);
        let step = self.time_step_of(model);
        let ambient = self.cases().get(self.step_case).cloned();
        let key = key_of(&[
            &model.line_version,
            &self.method,
            &self.current,
            &ambient,
            &only,
            &self.step_from,
            &self.duration,
            &step,
        ]);
        if self
            .steps
            .borrow()
            .as_ref()
            .is_none_or(|(k, _, _)| *k != key)
        {
            let t = Instant::now();
            let v = ambient.map_or_else(Vec::new, |a| {
                self.step_curves(model, &a, only.as_ref(), step)
            });
            *self.steps.borrow_mut() = Some((key, v, t.elapsed()));
        }
        self.steps.borrow()
    }

    /// One curve per wire type, bundle and current among the sections in
    /// `only` that carry a current.
    fn step_curves(
        &self,
        model: &Model,
        ambient: &ohl_thermal::Ambient,
        only: Option<&BTreeSet<String>>,
        step: f64,
    ) -> Vec<StepCurve> {
        let heats = ohl_thermal::sections::section_temperatures(
            &model.line,
            &model.library,
            self.method,
            &[],
            self.current_source(),
            only,
        );
        let mut curves: Vec<StepCurve> = Vec::new();
        for h in heats {
            let Ok(to) = h.current else { continue };
            if let Some(c) = curves
                .iter_mut()
                .find(|c| c.cable == h.cable && c.bundle == h.bundle && c.to == to)
            {
                c.sections.push(h.section);
                continue;
            }
            let n = f64::from(h.bundle.max(1));
            let conductor =
                ohl_thermal::sections::conductor_of(&model.line, &model.library, &h.cable);
            let budget = ohl_thermal::default_steady_budget();
            let steady = |i: f64| -> Result<f64, String> {
                let c = conductor.as_ref().map_err(Clone::clone)?;
                ohl_thermal::temperature(self.method, c, ambient, i / n, &budget)
                    .map(|b| b.temperature)
                    .map_err(|e| e.to_string())
            };
            let start = steady(self.step_from);
            let end = steady(to);
            let points = match (&conductor, &start) {
                (Ok(c), Ok(t0)) => ohl_thermal::transient(
                    self.method,
                    c,
                    ambient,
                    *t0,
                    to / n,
                    self.duration,
                    step,
                )
                .map_err(|e| e.to_string()),
                (Err(e), _) | (_, Err(e)) => Err(e.clone()),
            };
            curves.push(StepCurve {
                cable: h.cable,
                bundle: h.bundle,
                sections: vec![h.section],
                from: self.step_from,
                to,
                start,
                end,
                points,
            });
        }
        curves
    }
}

/// What the window asks of the app.
#[derive(Default)]
pub struct Out {
    /// The spans of the section of a result row clicked, to select (as
    /// the other windows' row clicks select theirs).
    pub go: Option<Vec<crate::selection::Selection>>,
    /// The clearances to draw (wire, target), when they changed; empty:
    /// taken off.
    pub marks: Option<Vec<([f64; 3], [f64; 3])>>,
}

/// The Thermal rating window.
pub fn window(
    ctx: &egui::Context,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) -> Out {
    w.poll();
    let mut out = Out::default();
    if w.open {
        let mut open = true;
        help::window(
            ctx,
            crate::dockable::Panel::Thermal,
            "Thermal rating",
            [900.0, 780.0],
            &mut open,
            |ui| {
                out.go = page_ui(ui, w, model, rules);
            },
        );
        w.open = open;
    }
    out.marks = w.marks_change();
    out
}

/// The page selector at the top, then the page: its Setup and Results
/// tabs, its footer.
fn page_ui(
    ui: &mut egui::Ui,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) -> Option<Vec<crate::selection::Selection>> {
    // The four calculations as pages (owner, 2026-11-04), switched as
    // the other windows switch modes; each keeps its own tab.
    ui.horizontal(|ui| {
        for p in Page::ALL {
            ui.selectable_value(&mut w.page, p, p.title())
                .on_hover_text(p.hover());
        }
    });
    ui.separator();
    let page = w.page;
    let id = page.id();
    let (generation, blocked) = match page {
        Page::Clearance => (w.generation, !matches!(rules, Some(Ok(_))) && !w.running()),
        _ => (0, false),
    };
    let mut go = None;
    match check_ui::tabs(ui, id, None, generation, blocked) {
        check_ui::Tab::Setup => {
            help::body(ui, id, |ui| setup(ui, w, model));
        }
        check_ui::Tab::Results => {
            go = check_ui::results_area(ui, id, |ui| results(ui, w, model, rules));
        }
    }
    help::footer(ui, id, |ui| footer(ui, w, model, rules));
    go
}

// ------------------------------------------------------------- the setup

/// The Setup tab of the page shown, in the check pages' sections: 1 What
/// it checks (and how), 2 Scope, 3 What to compute, 4 Model (folded).
fn setup(ui: &mut egui::Ui, w: &mut ThermalWindow, model: &Model) {
    let page = w.page;
    check_ui::section(ui, 1, "What it checks", None);
    what_it_checks(ui, w);
    ui.separator();
    check_ui::section(ui, 2, "Scope", None);
    scope_ui(ui, w, model);
    ui.separator();
    check_ui::section(ui, 3, "What to compute", None);
    match page {
        Page::Ampacity => ampacity_inputs(ui, w),
        Page::Temperature => {
            method_ui(ui, w, page);
            current_ui(ui, w, model, page);
        }
        Page::Step => step_inputs(ui, w, model),
        Page::Clearance => clearance_inputs(ui, w, model),
    }
    ui.separator();
    match page {
        Page::Step => {
            check_ui::model_section(ui, page.id(), 4, |ui| step_model(ui, w, model));
        }
        Page::Clearance if w.mode == ClearanceMode::Limit => {
            check_ui::model_section(ui, page.id(), 4, |ui| search_model(ui, w));
        }
        _ => {}
    }
}

/// 1 What it checks: in short, and *How it is computed* with its
/// drawings.
fn what_it_checks(ui: &mut egui::Ui, w: &ThermalWindow) {
    let id = w.page.id();
    match w.page {
        Page::Ampacity => {
            help::explain(
                ui,
                "Per section: the current it may carry (its ampacity) in each ambient case \
                 without its conductor passing the maximum temperature.",
                "Computed at once as the window is drawn, by IEEE 738 or CIGRE TB 601 / TB 207. \
                 Per phase: a bundle's subconductors together. The model is not changed.",
            );
            check_ui::how_computed(
                ui,
                id,
                "Heat in (Joule I²R, the sun) equals heat out (convection by the wind, \
                 radiation) at the maximum conductor temperature: solved for the current I of \
                 one subconductor, times the bundle's subconductors.",
                help::heat_balance,
            );
        }
        Page::Temperature => {
            help::explain(
                ui,
                "Per section: the steady conductor temperature for the current its circuit \
                 carries, in each ambient case; the hottest case governs.",
                "The current is each system's operating current (Line › Electrical systems, \
                 column A) or one typed here, shared by a bundle's subconductors. Computed at \
                 once. Earth wires carry no operating current and are left out. The model is \
                 not changed.",
            );
            check_ui::how_computed(
                ui,
                id,
                "The same heat balance as the ampacity, solved the other way: for the conductor \
                 temperature at which heat in (I²R(T), the sun) equals heat out (convection, \
                 radiation) — by halving between the air temperature and a temperature where \
                 the conductor loses more than it gains, to a millionth of a degree.",
                help::heat_balance,
            );
        }
        Page::Step => {
            help::explain(
                ui,
                "The conductor temperature over time after the current steps from one value to \
                 another: how long before it reaches a temperature.",
                "Per wire type, bundle and current among the sections in scope (sections alike \
                 heat alike: one curve), in one ambient case. Starts at the steady temperature \
                 of the current before the step and tends to the steady temperature of the \
                 current after it. Computed at once.",
            );
            check_ui::how_computed(
                ui,
                id,
                "m·c · dT/dt = I²R(T) + sun − convection − radiation, stepped in time by \
                 fourth-order Runge–Kutta from the steady temperature before the step; m·c the \
                 heat capacity per metre estimated from the wire's materials and mass.",
                |ui| {
                    ui.horizontal_top(|ui| {
                        help::heat_balance(ui);
                        help::step_change(ui);
                    });
                },
            );
        }
        Page::Clearance => {
            help::explain(
                ui,
                "Per section: the hottest the conductor may run before a ground or point \
                 clearance rule fails — or, at the current, whether the clearances are met at \
                 the temperature it gives.",
                "Run solves each section again at other temperatures in the weather case heated \
                 and checks it by the ground and point rules of the wire clearances' rule file. \
                 At the current: the temperature from Temperature at a current (the hottest \
                 ambient case, or one), then the same solve and check once. The clearances \
                 found are drawn in the views. The model is not changed.",
            );
            check_ui::how_computed(
                ui,
                id,
                "The clearance limit: a scan upwards by the step, then halving to the \
                 tolerance; the ampacity is given again at the limit found. At the current: the \
                 steady temperature (heat balance), the section solved at it in the weather \
                 case heated — the same solve and check as the search, at one temperature.",
                |ui| {
                    ui.horizontal_top(|ui| {
                        help::heat_balance(ui);
                        help::clearance_limit(ui);
                    });
                },
            );
        }
    }
}

/// 2 Scope: the circuits, routes and sections; the ambient cases; on the
/// Clearance page the weather case heated.
fn scope_ui(ui: &mut egui::Ui, w: &mut ThermalWindow, model: &Model) {
    let running = w.running();
    ui.add_enabled_ui(!running, |ui| {
        check_ui::ScopePicker::new("thermal-window", &model.line)
            .route_of(model)
            .wires(&mut w.scope, true)
            .ui(ui);
    });
    egui::Grid::new(("thermal-scope", w.page.id()))
        .num_columns(2)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            ui.label("Ambient cases");
            ui.horizontal(|ui| {
                let name = w.file.as_ref().map_or("none".to_owned(), |p| {
                    p.file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                });
                ui.label(egui::RichText::new(name).strong());
                if ui
                    .button("Choose…")
                    .on_hover_text(
                        "An ambient case file (.ohlta): air, wind, sun. Each case gives its own \
                         column of results; only the heat balance takes them",
                    )
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter(
                            "Thermal ambient cases",
                            &[ohl_thermal::ambient::AMBIENT_FILE_EXTENSION],
                        )
                        .pick_file()
                {
                    w.choose(p);
                }
            });
            ui.end_row();
            if w.page == Page::Clearance {
                ui.label("Weather case heated");
                ui.add_enabled_ui(!running, |ui| {
                    let name = |k: usize| model.cases.get(k).map_or("", |c| c.case.name.as_str());
                    egui::ComboBox::from_id_salt("thermal-base")
                        .selected_text(name(w.base_case))
                        .show_ui(ui, |ui| {
                            for k in 0..model.cases.len() {
                                ui.selectable_value(&mut w.base_case, k, name(k));
                            }
                        })
                        .response
                        .on_hover_text(
                            "The loads on the wires (wind, ice, the creep state) while their \
                             temperature is raised: usually still air. Its rules apply (a rule \
                             limited to other cases is not checked)",
                        );
                });
                ui.end_row();
            }
        });
    match &w.ambient {
        Some(Ok(set)) => {
            for c in &set.cases {
                ui.label(
                    egui::RichText::new(format!("{}: {}", c.name, c.summary()))
                        .small()
                        .weak(),
                );
            }
        }
        Some(Err(e)) => {
            crate::menu::error_label(ui, e.as_str());
        }
        None => {
            ui.label(
                egui::RichText::new(
                    "Choose an ambient case file (.ohlta): one column of results per case.",
                )
                .small()
                .weak(),
            );
        }
    }
}

/// The rating method.
fn method_ui(ui: &mut egui::Ui, w: &mut ThermalWindow, page: Page) {
    ui.horizontal(|ui| {
        ui.label("Method");
        ui.add_enabled_ui(!(page == Page::Clearance && w.running()), |ui| {
            egui::ComboBox::from_id_salt(("thermal-method", page.id()))
                .selected_text(w.method.title())
                .show_ui(ui, |ui| {
                    for m in Method::ALL {
                        ui.selectable_value(&mut w.method, m, m.title());
                    }
                })
                .response
                .on_hover_text(
                    "IEEE 738 and CIGRE TB 601 differ mostly in convection at low wind and in \
                     the sun's heat (a few % to 10 % on the current, a few °C on the \
                     temperature); CIGRE TB 207 is the older CIGRE method. The same for every \
                     page",
                );
        });
    });
}

/// The circuits' current: each system's operating current, or typed.
fn current_ui(ui: &mut egui::Ui, w: &mut ThermalWindow, model: &Model, page: Page) {
    let running = page == Page::Clearance && w.running();
    let given: Vec<String> = model
        .line
        .systems
        .iter()
        .filter_map(|s| s.current_a.map(|i| format!("{} {i:.0} A", s.id)))
        .collect();
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(if page == Page::Step {
                "To (after the step)"
            } else {
                "Current"
            });
            let mut line = w.current.is_none();
            let from_line = ui
                .radio_value(&mut line, true, "each circuit's operating current")
                .on_hover_text(format!(
                    "Each section takes its system's operating current (Line › Electrical \
                     systems, column A; {}): nothing typed twice. A system without one gives no \
                     result (said per section)",
                    if given.is_empty() {
                        "none given yet".to_owned()
                    } else {
                        given.join(", ")
                    }
                ))
                .changed();
            let typed = ui
                .radio_value(&mut line, false, "typed")
                .on_hover_text("One current in every phase section in scope")
                .changed();
            if from_line || typed {
                w.current = (!line).then_some(w.typed_current);
            }
            if let Some(i) = w.current.as_mut() {
                crate::style::field(
                    ui,
                    egui::DragValue::new(i)
                        .range(0.0..=20_000.0)
                        .speed(5.0)
                        .suffix(" A"),
                )
                .on_hover_text(
                    "rms in each phase, shared by a bundle's subconductors. The Joule heat grows \
                     with its square: 10 % more current, about 20 % more heat and a hotter \
                     conductor",
                );
                w.typed_current = *i;
            }
        });
    });
    if w.current.is_none() && given.is_empty() {
        ui.label(
            egui::RichText::new(
                "No system gives an operating current: add one in Line › Electrical systems \
                 (column A), or type a current.",
            )
            .small()
            .color(ui.visuals().warn_fg_color),
        );
    }
}

/// Ampacity's inputs: the method and the maximum temperature.
fn ampacity_inputs(ui: &mut egui::Ui, w: &mut ThermalWindow) {
    method_ui(ui, w, Page::Ampacity);
    ui.horizontal(|ui| {
        ui.label("Maximum temperature");
        crate::style::field(
            ui,
            egui::DragValue::new(&mut w.max_temperature)
                .range(-20.0..=300.0)
                .suffix(" °C"),
        )
        .on_hover_text(
            "The conductor temperature the ampacity is computed at: higher, more current (the \
             cooling grows with the temperature above the air). Must be above the air \
             temperature of the case",
        );
    });
    help::explain(
        ui,
        "The ampacity at this conductor temperature, per ambient case: shown at once (well \
         under a millisecond per section).",
        "The heat terms of each value are on hover in the Results.",
    );
}

/// Step change's inputs.
fn step_inputs(ui: &mut egui::Ui, w: &mut ThermalWindow, model: &Model) {
    method_ui(ui, w, Page::Step);
    ui.horizontal(|ui| {
        ui.label("Ambient case");
        let names: Vec<String> = w.cases().iter().map(|c| c.name.clone()).collect();
        let shown = names.get(w.step_case).cloned().unwrap_or_default();
        egui::ComboBox::from_id_salt("thermal-step-case")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (k, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut w.step_case, k, n);
                }
            })
            .response
            .on_hover_text(
                "The weather the step happens in: more wind, a cooler air or no sun, a slower \
                 and smaller rise",
            );
    });
    ui.horizontal(|ui| {
        ui.label("From (before the step)");
        crate::style::field(
            ui,
            egui::DragValue::new(&mut w.step_from)
                .range(0.0..=20_000.0)
                .speed(5.0)
                .suffix(" A"),
        )
        .on_hover_text(
            "rms per phase, carried long enough for the conductor to be at its steady \
             temperature: where the curve starts",
        );
    });
    current_ui(ui, w, model, Page::Step);
    ui.horizontal(|ui| {
        ui.label("Duration");
        crate::style::field(
            ui,
            egui::DragValue::new(&mut w.duration)
                .range(10.0..=86_400.0)
                .speed(30.0)
                .suffix(" s"),
        )
        .on_hover_text(
            "How long after the step: the curve and the table end there (a line conductor \
             takes several of its time constants — some minutes each — to settle). Longer, \
             more steps to compute (still well under a second)",
        );
        ui.label("rows every");
        crate::style::field(
            ui,
            egui::DragValue::new(&mut w.print_every)
                .range(1.0..=86_400.0)
                .speed(10.0)
                .suffix(" s"),
        )
        .on_hover_text(
            "A row of the Results table every this many seconds (the plot and the CSV of the \
             plot's curve keep every time step); the results are not changed",
        );
    });
    help::explain(
        ui,
        "One curve per wire type, bundle and current; shown at once.",
        "The time to reach the Ampacity page's maximum temperature is given per curve.",
    );
}

/// Step change's Model: the time step.
fn step_model(ui: &mut egui::Ui, w: &mut ThermalWindow, model: &Model) {
    let default = model.line.defaults.thermal_time_step;
    let mut step = w.time_step_of(model);
    let took = w.steps.borrow().as_ref().map(|(_, _, t)| *t);
    ui.horizontal(|ui| {
        ui.label("Time step");
        let r = crate::style::field(
            ui,
            egui::DragValue::new(&mut step)
                .range(0.1..=600.0)
                .speed(0.5)
                .suffix(" s"),
        )
        .on_hover_text(format!(
            "The step of the Runge–Kutta integration. Smaller: a smoother, more exact curve \
             (the error falls with the step's fourth power) and more steps — {} the last time; \
             larger than about a tenth of the conductor's time constant, the curve becomes \
             coarse or diverges (said). Default: the line's default values ({default} s)",
            took.map_or("not measured yet".to_owned(), |t| format!(
                "{:.1} ms",
                t.as_secs_f64() * 1e3
            ))
        ));
        if r.changed() {
            w.time_step = Some(step);
        }
        if w.time_step.is_some()
            && ui
                .small_button("default")
                .on_hover_text(format!("The line's default values' time step, {default} s"))
                .clicked()
        {
            w.time_step = None;
        }
    });
}

/// The Clearance page's inputs: what to run, its range or its current.
fn clearance_inputs(ui: &mut egui::Ui, w: &mut ThermalWindow, model: &Model) {
    let running = w.running();
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal(|ui| {
            ui.label("Find");
            ui.selectable_value(&mut w.mode, ClearanceMode::Limit, "the clearance limit")
                .on_hover_text(
                    "Search per section the highest conductor temperature at which the ground \
                     and point rules are met (a scan and halving: about 20 solves per section)",
                );
            ui.selectable_value(
                &mut w.mode,
                ClearanceMode::AtCurrent,
                "the clearance at the current",
            )
            .on_hover_text(
                "Check per section the clearances at the temperature the current gives (one \
                     solve per section): pass or fail, the least clearance",
            );
        });
    });
    match w.mode {
        ClearanceMode::Limit => {
            method_ui(ui, w, Page::Clearance);
            ui.add_enabled_ui(!running, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Searched from");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut w.options.min_temperature)
                            .range(-50.0..=w.options.max_temperature - 1.0)
                            .suffix(" °C"),
                    )
                    .on_hover_text(
                        "The lowest temperature tried: a section failing even there is said \
                         \"below\" the range",
                    );
                    ui.label("to");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut w.options.max_temperature)
                            .range(w.options.min_temperature + 1.0..=400.0)
                            .suffix(" °C"),
                    )
                    .on_hover_text(format!(
                        "The highest temperature tried: a section passing there is said \
                         \"above\" the range. A wider range: about one more solve per step per \
                         section ({})",
                        per_solve_words(w)
                    ));
                });
            });
            help::explain(
                ui,
                "Run searches each section's clearance-limited temperature in this range; the \
                 ampacity at it per ambient case (Method) is given beside it.",
                "The rules are the ground and point rules of the wire clearances' rule file \
                 (Checks › Wire clearances › Rule file) that apply to the weather case heated.",
            );
        }
        ClearanceMode::AtCurrent => {
            method_ui(ui, w, Page::Clearance);
            current_ui(ui, w, model, Page::Clearance);
            ui.add_enabled_ui(!running, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Ambient case");
                    let names: Vec<String> = w.cases().iter().map(|c| c.name.clone()).collect();
                    let shown = match w.check_case {
                        None => "the hottest of each section".to_owned(),
                        Some(k) => names.get(k).cloned().unwrap_or_default(),
                    };
                    egui::ComboBox::from_id_salt("thermal-check-case")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut w.check_case,
                                None,
                                "the hottest of each section",
                            );
                            for (k, n) in names.iter().enumerate() {
                                ui.selectable_value(&mut w.check_case, Some(k), n);
                            }
                        })
                        .response
                        .on_hover_text(
                            "The ambient case whose temperature the sections are checked at: the \
                             hottest of each (on the safe side), or one. A hotter case, more sag \
                             and less clearance",
                        );
                });
            });
            help::explain(
                ui,
                "Run solves each section once at the temperature its current gives and checks \
                 its ground and point clearances.",
                "The temperatures are those of Temperature at a current. The rules are the \
                 ground and point rules of the wire clearances' rule file that apply to the \
                 weather case heated; one solve per section.",
            );
        }
    }
}

/// The time of one solve in words, from the last run.
fn per_solve_words(w: &ThermalWindow) -> String {
    match w.per_solve {
        Some(t) => format!(
            "each solve and check took about {} in the last run",
            if t.as_secs_f64() < 1.0 {
                format!("{:.0} ms", t.as_secs_f64() * 1e3)
            } else {
                format!("{:.1} s", t.as_secs_f64())
            }
        ),
        None => "the time of a solve is measured on the first run".to_owned(),
    }
}

/// The search's Model: its scan step and tolerance.
fn search_model(ui: &mut egui::Ui, w: &mut ThermalWindow) {
    let running = w.running();
    ui.add_enabled_ui(!running, |ui| {
        egui::Grid::new("thermal-model")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Scan step");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut w.options.scan_step)
                        .range(0.5..=50.0)
                        .suffix(" °C"),
                )
                .on_hover_text(format!(
                    "The search goes up by this step until a rule fails. Smaller: finds a limit \
                     hidden between two steps (rare), one more solve per step per section ({})",
                    per_solve_words(w)
                ));
                ui.end_row();
                ui.label("Tolerance");
                crate::style::field(
                    ui,
                    egui::DragValue::new(&mut w.options.tolerance)
                        .range(0.05..=10.0)
                        .suffix(" °C"),
                )
                .on_hover_text(format!(
                    "The limit is found to this: the temperature given passes, one tolerance \
                     above fails. Halved: one more solve per section ({})",
                    per_solve_words(w)
                ));
                ui.end_row();
            });
    });
    help::explain(
        ui,
        &format!(
            "About range/step + log₂(step/tolerance) solves per section: {} now.",
            w.options.solves_per_section()
        ),
        "A smaller step finds a limit hidden between two steps (a rule failing then passing \
         again is rare: a span lowering onto an obstacle and moving past it); a smaller \
         tolerance adds one solve per halving.",
    );
}

// ------------------------------------------------------------- the footer

/// The footer: Run on the Clearance page; on the others, computed at once
/// (and how long it took).
fn footer(
    ui: &mut egui::Ui,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) {
    let took = match w.page {
        Page::Clearance => {
            run_ui(ui, w, model, rules);
            return;
        }
        Page::Ampacity => w.ratings.borrow().as_ref().map(|k| k.2),
        Page::Temperature => w.heats.borrow().as_ref().map(|k| k.2),
        Page::Step => w.steps.borrow().as_ref().map(|k| k.2),
    };
    ui.label(
        egui::RichText::new(format!(
            "Computed at once as the setup changes, no Run needed{} · {}",
            took.map_or(String::new(), |t| format!(
                " (last in {:.2} ms)",
                t.as_secs_f64() * 1e3
            )),
            check_ui::scope_words(&model.line, &w.scope, true)
        ))
        .small()
        .weak(),
    );
}

/// The Clearance page's Run footer: its mode's run of the sections in
/// scope (Stop and the progress while it runs); before Run, what it will
/// do and about how long (from the last run).
fn run_ui(
    ui: &mut egui::Ui,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) {
    let usable = match rules {
        Some(Ok(r)) => Some(r.clone()),
        _ => None,
    };
    let running = w.job.as_ref().map(|job| {
        let done = job.done.load(Ordering::Relaxed);
        let mut p = check_ui::Progress::of(job.started.elapsed(), done, job.sections);
        p.text = format!("section {done} of {}; {}", job.sections, p.text);
        p
    });
    let (sections, _) = check_ui::scope_counts(&model.line, &w.scope);
    let solves = match w.mode {
        ClearanceMode::Limit => sections as u64 * w.options.solves_per_section(),
        ClearanceMode::AtCurrent => sections as u64,
    };
    let estimate = match (w.mode, w.took, &w.limits, w.per_solve) {
        (ClearanceMode::Limit, Some(t), Some(Ok(_)), _) if w.searched > 0 => format!(
            " — {} (from the last search)",
            check_ui::plan_duration(t.as_secs_f64() * sections as f64 / w.searched as f64)
        ),
        (_, _, _, Some(s)) => format!(
            " — at most {} (from the last run)",
            check_ui::plan_duration(s.as_secs_f64() * solves as f64)
        ),
        _ => " — time measured on the first run".to_owned(),
    };
    let last = match w.mode {
        ClearanceMode::Limit => w.took,
        ClearanceMode::AtCurrent => w.took_check,
    }
    .map(|t| format!(" · last run {:.1} s", t.as_secs_f64()))
    .unwrap_or_default();
    let why = usable
        .is_none()
        .then(|| "choose a rule file: Checks › Wire clearances › Rule file".to_owned())
        .or_else(|| w.scope.why_not(&model.line))
        .or_else(|| {
            model
                .cases
                .is_empty()
                .then(|| "the model has no weather case".to_owned())
        })
        .or_else(|| {
            (w.mode == ClearanceMode::AtCurrent && w.cases().is_empty())
                .then(|| "choose an ambient case file (2 Scope)".to_owned())
        });
    let what = match w.mode {
        ClearanceMode::Limit => "clearance limit",
        ClearanceMode::AtCurrent => "clearance at the current",
    };
    let f = check_ui::RunFooter {
        running,
        why_not: why,
        hover: match w.mode {
            ClearanceMode::Limit => {
                "The clearance-limited temperature: per section, the highest conductor \
                 temperature at which the ground and point rules of the rule file are met"
            }
            ClearanceMode::AtCurrent => {
                "The clearance at the current: per section, solved at the temperature its \
                 current gives and checked by the ground and point rules of the rule file"
            }
        },
        scope: Some(check_ui::scope_words(&model.line, &w.scope, true)),
        info: Some(format!(
            "{what} of {}{estimate}{last}",
            check_ui::count(sections, "section"),
        )),
    };
    match check_ui::run_footer(ui, f, |_| {}) {
        check_ui::Action::Run => {
            if let Some(r) = usable {
                w.start(model, r, ui.ctx());
            }
        }
        check_ui::Action::Stop => {
            if let Some(job) = &w.job {
                job.cancel.cancel();
            }
        }
        check_ui::Action::None => {}
    }
}

// ------------------------------------------------------------- the results

/// The Results tab of the page shown; the spans of the section of a row
/// clicked.
fn results(
    ui: &mut egui::Ui,
    w: &mut ThermalWindow,
    model: &Model,
    rules: Option<&Result<Vec<Rule>, String>>,
) -> Option<Vec<crate::selection::Selection>> {
    if w.cases().is_empty() {
        ui.label(
            egui::RichText::new("Choose an ambient case file (Setup › 2 Scope).")
                .small()
                .weak(),
        );
        if w.page != Page::Clearance {
            return None;
        }
    }
    let section = match w.page {
        Page::Ampacity => ampacity_table(ui, w, model),
        Page::Temperature => temperature_table(ui, w, model),
        Page::Step => {
            step_results(ui, w, model);
            None
        }
        Page::Clearance => {
            if let Some(what) = w.out_of_date(model)
                && crate::revision::banner(ui, &what, !w.running())
                && let Some(Ok(r)) = rules
            {
                w.start(model, r.clone(), ui.ctx());
            }
            match w.mode {
                ClearanceMode::Limit => limit_table(ui, w, model),
                ClearanceMode::AtCurrent => check_table(ui, w),
            }
        }
    }?;
    let s = model.line.sections.iter().find(|s| s.id == section)?;
    Some(
        (0..s.points.len().saturating_sub(1))
            .map(|span| crate::selection::Selection::Span {
                section: s.id.clone(),
                span,
            })
            .collect(),
    )
}

/// The ampacity of one subconductor of `conductor` in `ambient` at `t`:
/// the phase's (times `bundle`) in words, the heat terms, the value.
fn rating(
    method: Method,
    conductor: &Result<Conductor, String>,
    ambient: &ohl_thermal::Ambient,
    t: f64,
    bundle: u32,
) -> Rating {
    match conductor {
        Ok(c) => match ohl_thermal::ampacity(method, c, ambient, t) {
            Ok(b) => {
                let n = f64::from(bundle.max(1));
                let phase = b.current * n;
                let each = if bundle > 1 {
                    format!("{:.0} A per subconductor × {bundle}; ", b.current)
                } else {
                    String::new()
                };
                (
                    format!("{phase:.0} A"),
                    format!("{each}{}", terms(&b)),
                    Some(phase),
                )
            }
            Err(e) => ("—".into(), e.to_string(), None),
        },
        Err(e) => ("—".into(), e.clone(), None),
    }
}

/// Ampacity: per section its wire type, bundle and the ampacity per case;
/// the section of the row clicked.
fn ampacity_table(ui: &mut egui::Ui, w: &ThermalWindow, model: &Model) -> Option<String> {
    let cases = w.cases();
    let conductors = w.conductors(model);
    let key = key_of(&[
        &model.line_version,
        &w.method,
        &w.max_temperature,
        &cases,
        &conductors.iter().map(|(s, _)| &s.id).collect::<Vec<_>>(),
    ]);
    if w.ratings
        .borrow()
        .as_ref()
        .is_none_or(|(k, _, _)| *k != key)
    {
        let t = Instant::now();
        let rows = conductors
            .iter()
            .map(|(s, c)| {
                cases
                    .iter()
                    .map(|a| rating(w.method, c, a, w.max_temperature, s.bundle))
                    .collect()
            })
            .collect();
        *w.ratings.borrow_mut() = Some((key, rows, t.elapsed()));
    }
    let kept = w.ratings.borrow();
    let ratings = &kept.as_ref().expect("made above").1;
    let mut cols = vec![
        Col::text("Section"),
        Col::text("Wire type"),
        Col::num("Bundle").tip("Subconductors per phase"),
    ];
    for c in cases {
        cols.push(
            Col::num(format!("{} at {} °C [A]", c.name, w.max_temperature)).tip(
                "Per phase: the bundle's subconductors together; the heat terms of one on hover",
            ),
        );
    }
    let rows: Vec<check_ui::Row> = conductors
        .iter()
        .zip(ratings)
        .map(|((s, _), at_max)| {
            let mut cells = vec![
                Cell::text(&s.id),
                Cell::text(&s.cable),
                Cell::num(Some(f64::from(s.bundle.max(1))), 0),
            ];
            cells.extend(at_max.iter().map(|(text, hover, v)| match v {
                Some(v) => Cell::sorted(text, *v).tip(hover.as_str()),
                None => Cell::text(text).tip(hover.as_str()),
            }));
            check_ui::Row::new(cells, false)
        })
        .collect();
    let k = check_ui::ResultTable::new("thermal-ampacity-rows", cols)
        .rows(rows)
        .no_filter()
        .csv("thermal-ampacity.csv")
        .ui(ui)?;
    conductors.get(k).map(|(s, _)| s.id.clone())
}

/// Temperature at a current: per section its current and the steady
/// temperature per case (the heat terms on hover), the hottest; the
/// section of the row clicked.
fn temperature_table(ui: &mut egui::Ui, w: &ThermalWindow, model: &Model) -> Option<String> {
    let cases = w.cases().to_vec();
    let kept = w.heats_now(model);
    let heats = &kept.as_ref().expect("made above").1;
    let mut cols = vec![
        Col::text("Section"),
        Col::text("Wire type"),
        Col::num("Bundle").tip("Subconductors per phase"),
        Col::num("Current [A]").tip(
            "rms per phase: the system's operating current (Line › Electrical systems) or the \
             one typed",
        ),
    ];
    for c in &cases {
        cols.push(
            Col::num(format!("{} [°C]", c.name))
                .tip("Steady conductor temperature; the heat terms on hover"),
        );
    }
    cols.push(Col::num("Hottest [°C]").tip("The hottest ambient case's temperature"));
    let rows: Vec<check_ui::Row> = heats
        .iter()
        .map(|h| {
            let mut cells = vec![
                Cell::text(&h.section),
                Cell::text(&h.cable),
                Cell::num(Some(f64::from(h.bundle)), 0),
            ];
            match &h.current {
                Ok(i) => cells.push(Cell::num(Some(*i), 0).tip(match (w.current, h.bundle) {
                    (None, 1) => "its system's operating current".to_owned(),
                    (None, n) => format!(
                        "its system's operating current; {:.0} A per subconductor",
                        i / f64::from(n)
                    ),
                    (Some(_), 1) => "typed".to_owned(),
                    (Some(_), n) => format!("typed; {:.0} A per subconductor", i / f64::from(n)),
                })),
                Err(e) => cells.push(Cell::text("—").tip(e.as_str())),
            }
            for r in &h.cases {
                cells.push(match r {
                    Ok(b) => Cell::num(Some(b.temperature), 1).tip(terms(b)),
                    Err(e) => Cell::text("—").tip(e.as_str()),
                });
            }
            cells.push(match h.hottest() {
                Some((k, t)) => Cell::num(Some(t), 1).tip(format!("in {}", cases[k].name)),
                None => Cell::text("—").tip(h.current.as_ref().err().cloned().unwrap_or_default()),
            });
            check_ui::Row::new(cells, false)
        })
        .collect();
    let k = check_ui::ResultTable::new("thermal-temperature-rows", cols)
        .rows(rows)
        .no_filter()
        .csv("thermal-temperature.csv")
        .ui(ui)?;
    heats.get(k).map(|h| h.section.clone())
}

/// The plot's colours, by curve.
const CURVE_COLORS: [egui::Color32; 6] = [
    egui::Color32::from_rgb(228, 87, 46),
    egui::Color32::from_rgb(76, 155, 232),
    egui::Color32::from_rgb(118, 176, 65),
    egui::Color32::from_rgb(160, 92, 200),
    egui::Color32::from_rgb(242, 193, 78),
    egui::Color32::from_rgb(23, 190, 187),
];

/// Step change: a line per curve (start, after the duration, steady, the
/// time to the maximum temperature), the plot, the table of times.
fn step_results(ui: &mut egui::Ui, w: &ThermalWindow, model: &Model) {
    let kept = w.steps_now(model);
    let curves = &kept.as_ref().expect("made above").1;
    if curves.is_empty() {
        ui.label(
            egui::RichText::new(
                "No section in scope carries a current: give the systems an operating current \
                 (Line › Electrical systems) or type one (Setup).",
            )
            .small()
            .color(ui.visuals().warn_fg_color),
        );
        return;
    }
    let case = w
        .cases()
        .get(w.step_case)
        .map_or(String::new(), |c| c.name.clone());
    ui.label(
        egui::RichText::new(format!(
            "{} to the current, in {case}, for {} s:",
            if curves.iter().all(|c| c.from == curves[0].from) {
                format!("From {:.0} A", curves[0].from)
            } else {
                "From each curve's current".to_owned()
            },
            w.duration
        ))
        .small(),
    );
    for (k, c) in curves.iter().enumerate() {
        let words = match (&c.points, &c.start, &c.end) {
            (Ok(p), Ok(t0), Ok(t1)) => format!(
                "{} ({}): {t0:.1} °C rising to {:.1} °C after {:.0} s, steady {t1:.1} °C; {}",
                c.label(),
                c.sections.join(", "),
                p.last().map_or(*t0, |x| x.1),
                w.duration,
                match c.reaches(w.max_temperature) {
                    Some(s) => format!("{} °C after {s:.0} s", w.max_temperature),
                    None => format!("not {} °C within the duration", w.max_temperature),
                }
            ),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                format!("{} ({}): {e}", c.label(), c.sections.join(", "))
            }
        };
        ui.label(
            egui::RichText::new(words)
                .small()
                .color(CURVE_COLORS[k % CURVE_COLORS.len()]),
        );
    }
    plot(ui, curves, w.max_temperature);
    // The table: a row every `print_every` s, a column per curve.
    let mut cols = vec![Col::num("Time [s]")];
    for c in curves {
        cols.push(Col::num(format!("{} [°C]", c.label())).tip(c.sections.join(", ")));
    }
    let longest = curves
        .iter()
        .filter_map(|c| c.points.as_ref().ok())
        .max_by_key(|p| p.len());
    let mut rows = Vec::new();
    if let Some(times) = longest {
        let every = w.print_every.max(1e-9);
        let mut next = 0.0;
        for (k, (time, _)) in times.iter().enumerate() {
            if *time < next - 1e-9 && k + 1 != times.len() {
                continue;
            }
            next += every;
            let mut cells = vec![Cell::num(Some(*time), 0)];
            for c in curves {
                let v = c.points.as_ref().ok().and_then(|p| p.get(k)).map(|x| x.1);
                cells.push(Cell::num(v, 1).bad(v.is_some_and(|v| v > w.max_temperature)));
            }
            rows.push(check_ui::Row::new(cells, false));
        }
    }
    check_ui::ResultTable::new("thermal-step-rows", cols)
        .rows(rows)
        .no_filter()
        .csv("thermal-step.csv")
        .ui(ui);
}

/// A small plot of the curves: temperature over time, the maximum
/// temperature dashed; the values under the pointer on hover.
fn plot(ui: &mut egui::Ui, curves: &[StepCurve], max_t: f64) {
    let size = egui::vec2(ui.available_width().clamp(200.0, 700.0), 170.0);
    let (response, p) = ui.allocate_painter(size, egui::Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "plot: temperature over time")
    });
    let v = ui.visuals();
    let r = p.clip_rect();
    p.rect_filled(r, 3.0, v.extreme_bg_color);
    let all: Vec<(f64, f64)> = curves
        .iter()
        .filter_map(|c| c.points.as_ref().ok())
        .flatten()
        .copied()
        .collect();
    if all.is_empty() {
        return;
    }
    let t_end = all.iter().map(|x| x.0).fold(1e-9, f64::max);
    let lo = all
        .iter()
        .map(|x| x.1)
        .fold(f64::INFINITY, f64::min)
        .min(max_t);
    let hi = all
        .iter()
        .map(|x| x.1)
        .fold(f64::NEG_INFINITY, f64::max)
        .max(max_t);
    let (lo, hi) = (lo - 2.0, hi + 2.0);
    let inner =
        egui::Rect::from_min_max(r.min + egui::vec2(38.0, 8.0), r.max - egui::vec2(8.0, 18.0));
    let at = |time: f64, t: f64| {
        egui::pos2(
            inner.left() + (time / t_end) as f32 * inner.width(),
            inner.bottom() - ((t - lo) / (hi - lo)) as f32 * inner.height(),
        )
    };
    let ink = v.weak_text_color();
    p.line_segment(
        [inner.left_bottom(), inner.right_bottom()],
        egui::Stroke::new(1.0, ink),
    );
    p.line_segment(
        [inner.left_bottom(), inner.left_top()],
        egui::Stroke::new(1.0, ink),
    );
    let font = egui::FontId::proportional(9.0);
    for (t, align) in [
        (lo, egui::Align2::RIGHT_BOTTOM),
        (hi, egui::Align2::RIGHT_TOP),
    ] {
        p.text(
            egui::pos2(inner.left() - 3.0, at(0.0, t).y),
            align,
            format!("{t:.0} °C"),
            font.clone(),
            ink,
        );
    }
    p.text(
        inner.right_bottom() + egui::vec2(0.0, 3.0),
        egui::Align2::RIGHT_TOP,
        format!("{t_end:.0} s"),
        font.clone(),
        ink,
    );
    help::dashed(&p, at(0.0, max_t), at(t_end, max_t), v.warn_fg_color);
    for (k, c) in curves.iter().enumerate() {
        if let Ok(points) = &c.points {
            let line: Vec<egui::Pos2> = points.iter().map(|(time, t)| at(*time, *t)).collect();
            p.add(egui::Shape::line(
                line,
                egui::Stroke::new(1.6, CURVE_COLORS[k % CURVE_COLORS.len()]),
            ));
        }
    }
    if let Some(pos) = response.hover_pos()
        && inner.contains(pos)
    {
        let time = f64::from((pos.x - inner.left()) / inner.width()) * t_end;
        let words: Vec<String> = curves
            .iter()
            .filter_map(|c| {
                let points = c.points.as_ref().ok()?;
                let near = points
                    .iter()
                    .min_by(|a, b| (a.0 - time).abs().total_cmp(&(b.0 - time).abs()))?;
                Some(format!("{}: {:.1} °C", c.label(), near.1))
            })
            .collect();
        p.line_segment(
            [
                egui::pos2(pos.x, inner.top()),
                egui::pos2(pos.x, inner.bottom()),
            ],
            egui::Stroke::new(1.0, ink),
        );
        response.on_hover_text(format!("{time:.0} s\n{}", words.join("\n")));
    }
}

/// The clearance limit: per section the limit, the governing clearance and
/// the ampacity at the limit per case; the section of the row clicked.
fn limit_table(ui: &mut egui::Ui, w: &ThermalWindow, model: &Model) -> Option<String> {
    let limits: &[SectionLimit] = match &w.limits {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            crate::menu::error_label(ui, e.as_str());
            return None;
        }
        None => {
            ui.label(if w.running() {
                "Searching…"
            } else {
                "The highest temperature each section may run at before a ground or point \
                 clearance fails. Run to see it."
            });
            return None;
        }
    };
    // What the limits leave out, not on the safe side (2026-11-08): each
    // note once, in the not-safe colour.
    let mut said: Vec<&str> = Vec::new();
    for n in limits.iter().flat_map(|l| &l.notes) {
        if !said.contains(&n.as_str()) {
            said.push(n);
        }
    }
    if !said.is_empty() {
        let [r, g, b] = crate::loads::UsageMark::NotSafe.color();
        for n in said {
            ui.label(egui::RichText::new(n).color(egui::Color32::from_rgb(r, g, b)));
        }
    }
    let cases = w.cases();
    let mut cols = vec![
        Col::text("Section"),
        Col::text("Wire type"),
        Col::num("Clearance limit [°C]"),
        Col::text("Governing").tip("The clearance that fails just above the limit"),
    ];
    for c in cases {
        cols.push(
            Col::num(format!("{} at limit [A]", c.name))
                .tip("The ampacity at the limit, per phase (above the range: a lower bound)"),
        );
    }
    let rows: Vec<check_ui::Row> = limits
        .iter()
        .map(|l| {
            let t = match &l.result {
                Ok(Limit::At(t)) => Some(*t),
                Ok(Limit::AboveRange) => Some(w.options.max_temperature),
                _ => None,
            };
            let mut cells = vec![Cell::text(&l.section), Cell::text(&l.cable)];
            cells.push(match &l.result {
                Ok(Limit::At(t)) => Cell::num(Some(*t), 1),
                Ok(Limit::AboveRange) => Cell::sorted(
                    format!("above {}", w.options.max_temperature),
                    w.options.max_temperature + 1e-6,
                ),
                Ok(Limit::BelowRange) => Cell::sorted(
                    format!("below {}", w.options.min_temperature),
                    w.options.min_temperature - 1e-6,
                )
                .bad(true),
                Err(e) => Cell::text(format!("error: {e}")).bad(true),
            });
            cells.push(Cell::text(
                l.governing.as_ref().map_or(String::new(), |g| {
                    format!("{} at {:.1} °C", governing_words(g), g.temperature)
                }),
            ));
            let bundle = model
                .line
                .sections
                .iter()
                .find(|s| s.id == l.section)
                .map_or(1, |s| s.bundle);
            let conductor =
                ohl_thermal::sections::conductor_of(&model.line, &model.library, &l.cable);
            for c in cases {
                cells.push(match t {
                    Some(t) => {
                        let (text, hover, v) = rating(w.method, &conductor, c, t, bundle);
                        match v {
                            Some(v) => Cell::sorted(text, v).tip(hover),
                            None => Cell::text(text).tip(hover),
                        }
                    }
                    None => Cell::text("—"),
                });
            }
            let below = matches!(l.result, Ok(Limit::BelowRange) | Err(_));
            check_ui::Row::new(cells, below)
        })
        .collect();
    let k = check_ui::ResultTable::new("thermal-limit-rows", cols)
        .rows(rows)
        .no_filter()
        .csv("thermal-clearance-limit.csv")
        .ui(ui)?;
    limits.get(k).map(|l| l.section.clone())
}

/// The clearance at the current: per section its current, the case and
/// temperature, the least clearance against its requirement, pass or
/// fail; the section of the row clicked.
fn check_table(ui: &mut egui::Ui, w: &ThermalWindow) -> Option<String> {
    let Some(check) = &w.checks else {
        ui.label(
            "Whether each section meets its ground and point clearances at the temperature its \
             current gives. Run to see it.",
        );
        return None;
    };
    if w.running() {
        ui.label("Checking…");
        return None;
    }
    let checks = match &check.result {
        Ok(v) => v,
        Err(e) => {
            crate::menu::error_label(ui, e.as_str());
            return None;
        }
    };
    let failing = checks.iter().filter(|c| c.passes() == Some(false)).count();
    let passing = checks.iter().filter(|c| c.passes() == Some(true)).count();
    check_ui::verdict(
        ui,
        failing > 0,
        &format!(
            "{} fail, {passing} pass at the current",
            check_ui::count(failing, "section")
        ),
        false,
    );
    let names: Vec<String> = w.cases().iter().map(|c| c.name.clone()).collect();
    let cols = vec![
        Col::text("Section"),
        Col::text("Wire type"),
        Col::num("Current [A]").tip("rms per phase"),
        Col::text("Ambient case"),
        Col::num("Temperature [°C]").tip("The steady conductor temperature it was solved at"),
        Col::num("Least clearance [m]")
            .tip("The clearance with the least margin over its requirement"),
        Col::num("Required [m]"),
        Col::num("Margin [m]").tip("Clearance minus requirement: negative fails"),
        Col::text("Result"),
        Col::text("Governing").tip("Rule, span and measure of the least clearance"),
    ];
    let rows: Vec<check_ui::Row> = check
        .heats
        .iter()
        .zip(&check.cases)
        .map(|(h, case)| {
            let found = checks.iter().find(|c| c.section == h.section);
            let mut cells = vec![
                Cell::text(&h.section),
                Cell::text(&h.cable),
                match &h.current {
                    Ok(i) => Cell::num(Some(*i), 0),
                    Err(e) => Cell::text("—").tip(e.as_str()),
                },
                Cell::text(case.and_then(|k| names.get(k).cloned()).unwrap_or_default()),
            ];
            let temperature = case.and_then(|k| h.cases.get(k)?.as_ref().ok());
            cells.push(match temperature {
                Some(b) => Cell::num(Some(b.temperature), 1).tip(terms(b)),
                None => Cell::text("—"),
            });
            let mut failing = false;
            match found.map(|c| &c.result) {
                Some(Ok(Some(g))) => {
                    failing = !g.met();
                    cells.push(Cell::num(Some(g.clearance), 2).bad(failing));
                    cells.push(Cell::num(Some(g.required), 2));
                    cells.push(Cell::num(Some(g.margin()), 2).bad(failing));
                    cells.push(Cell::text(if failing { "fail" } else { "pass" }).bad(failing));
                    cells.push(Cell::text(governing_words(g)));
                }
                other => {
                    let why = match other {
                        Some(Err(e)) => e.clone(),
                        Some(Ok(None)) => "nothing of it checked".to_owned(),
                        _ => h
                            .current
                            .as_ref()
                            .err()
                            .cloned()
                            .or_else(|| h.cases.iter().find_map(|r| r.as_ref().err().cloned()))
                            .unwrap_or_else(|| "not checked".to_owned()),
                    };
                    cells.extend([
                        Cell::text("—"),
                        Cell::text("—"),
                        Cell::text("—"),
                        Cell::text("not checked").tip(why.as_str()),
                        Cell::text(why),
                    ]);
                }
            }
            check_ui::Row::new(cells, failing)
        })
        .collect();
    let k = check_ui::ResultTable::new("thermal-check-rows", cols)
        .rows(rows)
        .csv("thermal-clearance-at-current.csv")
        .ui(ui)?;
    check.heats.get(k).map(|h| h.section.clone())
}

#[cfg(test)]
impl ThermalWindow {
    /// The Temperature page's results now (tests).
    pub fn temperatures(&self, model: &Model) -> Vec<SectionHeat> {
        self.heats_now(model)
            .as_ref()
            .map(|k| k.1.clone())
            .unwrap_or_default()
    }

    /// The Step change page's curves now (tests).
    pub fn step_curves_now(&self, model: &Model) -> Vec<StepCurve> {
        self.steps_now(model)
            .as_ref()
            .map(|k| k.1.clone())
            .unwrap_or_default()
    }
}
