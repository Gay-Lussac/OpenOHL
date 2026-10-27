// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Clearance violations of the shown weather case, checked on a background
//! thread with a rule file (`.ohlr`): the wires against the ground, LiDAR
//! points of the rules' classes (read again from the point cloud each time,
//! in a corridor around the spans: ~2 s for 20 million points, nothing
//! kept in memory) and the line's own wires. After an edit the check runs
//! again.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use ohl_clearance::{CaseTargets, Outcome, Rule, RuleSet, Skipped, Wire, WireResult};
use ohl_line::Line;
use ohl_line::jumper::JumperShape;
use ohl_line::solve::SectionShape;
use ohl_terrain::Terrain;

use crate::model::Model;
use crate::selection::Selection;

/// One clearance below its requirement.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Rule name.
    pub rule: String,
    /// `vertical` or `radial`.
    pub measure: &'static str,
    /// The wire: a span or a jumper.
    pub wire: Selection,
    /// The other wire (wire rules).
    pub other: Option<Selection>,
    /// Clearance found, m.
    pub distance: f64,
    /// Clearance required, m.
    pub required: f64,
    /// Point on the wire; for a vertical clearance the point at the
    /// wire's height straight above (below) the target (drawn vertical).
    pub at: [f64; 3],
    /// Point on the target (ground, LiDAR point, other wire).
    pub target: [f64; 3],
    /// LiDAR class of the target point.
    pub class: Option<u8>,
    /// Points (or wire pieces) below the requirement.
    pub count: u64,
    /// A screen rule's band in plan, m (the distance then in plan, see
    /// [`ohl_clearance::zone::screen_text`]).
    pub band: Option<f64>,
}

/// The result of a check.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Weather case checked.
    pub case: String,
    /// Violations, worst (largest shortfall) first.
    pub findings: Vec<Finding>,
    /// Per rule: lowest clearances found, in words.
    pub rules: Vec<(String, String)>,
    /// What was skipped and why.
    pub notes: Vec<String>,
    /// How long it took.
    pub took: Duration,
}

/// What a check needs (copied: it runs on its own thread).
pub struct Input {
    /// The line (types resolved).
    pub line: Line,
    /// Weather case name.
    pub case: String,
    /// Solved sections of the case.
    pub shapes: Vec<SectionShape>,
    /// Solved jumpers of the case.
    pub jumpers: Vec<JumperShape>,
    /// Rules.
    pub rules: Vec<Rule>,
    /// Class scheme of the points.
    pub scheme: ohl_lidar::ClassScheme,
    /// Point cloud (`FILE.ohl` or `FILE.ohl#name`), if any.
    pub cloud: Option<String>,
    /// Ground surface.
    pub ground: Option<Arc<Terrain>>,
    /// The terrain surfaces the ground rules name, held already, and the
    /// project file and names of those to read
    /// ([`crate::model::Model::held_surfaces`]).
    pub surfaces: crate::model::NamedSurfaces,
    /// Half width of the corridor around the spans for points, m.
    pub corridor: f64,
    /// The circuits, routes and alignments checked (the rule file's
    /// `[check]`, as the clearance windows keep theirs).
    pub check: ohl_clearance::CheckScope,
    /// How far the reading of the points has got.
    pub progress: Arc<ohl_pointcloud::ReadProgress>,
    /// Stop.
    pub cancel: ohl_core::budget::CancelToken,
    /// LiDAR points of the rules' classes in the cloud (the time limit,
    /// [`ohl_clearance::zone::time_limit`]).
    pub points: u64,
}

fn wire_selection(w: &Wire) -> Selection {
    match w.section.strip_prefix("jumper:") {
        Some(id) => Selection::Jumper(id.to_owned()),
        None => Selection::Span {
            section: w.section.clone(),
            span: w.span,
        },
    }
}

/// The findings of one result (vertical and radial).
fn findings(rule: &Rule, a: &Wire, b: Option<&Wire>, r: &WireResult, out: &mut Vec<Finding>) {
    for (measure, hit, required, count) in r.measures() {
        if let (Some(h), Some(req)) = (hit, required)
            && h.distance < req
        {
            // A vertical clearance drawn vertical: its wire end straight
            // above (below) the target (owner, 2026-10-22).
            let (at, target) = h.drawn(measure == "vertical");
            out.push(Finding {
                rule: rule.name.clone(),
                measure,
                wire: wire_selection(a),
                other: b.map(wire_selection),
                distance: h.distance,
                required: req,
                at,
                target,
                class: h.class,
                count: count.max(1),
                band: if measure == "vertical" {
                    r.required.band
                } else {
                    None
                },
            });
        }
    }
}

/// Runs the check.
pub fn run(input: &Input) -> Result<Report, String> {
    let named = input.surfaces.read();
    let surfaces: Vec<(String, &Terrain)> =
        named.iter().map(|(n, t)| (n.clone(), t.as_ref())).collect();
    let started = Instant::now();
    let line = &input.line;
    let mut report = Report {
        case: input.case.clone(),
        ..Report::default()
    };
    // Voltages of the sections: every rule must cover them.
    let voltages = ohl_clearance::voltages_of(line)?;
    for rule in input
        .rules
        .iter()
        .filter(|r| ohl_clearance::applies(r, &input.case))
    {
        rule.check_voltages(&voltages, &[])?;
    }
    let mut wires = ohl_clearance::wires_of(line, &input.shapes, &input.jumpers);
    // The circuits, routes and alignments chosen only (owner, 2026-10-27).
    let on = input.check.wires_on(line);
    wires.retain(|w| {
        input.check.takes_circuit(&w.circuit)
            && on.as_ref().is_none_or(|(sections, jumpers)| {
                match w.section.strip_prefix("jumper:") {
                    Some(j) => jumpers.contains(j),
                    None => sections.contains(&w.section),
                }
            })
    });
    if wires.is_empty() && (!input.check.circuits.is_empty() || on.is_some()) {
        return Err("no wire of the circuits, routes and alignments chosen".into());
    }
    let limit = ohl_clearance::zone::time_limit(input.points);
    let point_classes: Vec<Vec<u8>> = input
        .rules
        .iter()
        .map(|r| r.point_classes(|role| input.scheme.codes_with_role(role)))
        .collect();
    // Points of the rules' classes, read again from the point cloud in a
    // corridor around the spans.
    let mut visited = None;
    let mut feed = |classes: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        let Some(spec) = &input.cloud else {
            return Ok(());
        };
        let source = ohl_pointcloud::open_source(spec)?;
        let corridor = ohl_lidar::Corridor::along_spans(line, input.corridor);
        // Exit clause: 900 s and 20 s per million points of the rules'
        // classes (owner, 2026-10-27: it scales with the work); Stop.
        let budget = ohl_core::budget::Budget::new()
            .time_limit(limit)
            .cancel_token(input.cancel.clone());
        let stats = source
            .visit_with(
                Some(&corridor),
                Some(classes),
                &budget,
                Some(&input.progress),
                visit,
            )
            .map_err(|e| {
                if !input.cancel.is_cancelled() && started.elapsed() >= limit {
                    format!(
                        "{e} — the time limit of {} was reached: fewer rules, a narrower \
                         corridor or fewer routes make the check faster",
                        ohl_clearance::zone::time_limit_text(input.points)
                    )
                } else {
                    e
                }
            })?;
        visited = Some(stats.points_visited);
        Ok(())
    };
    let checks = ohl_clearance::check_case(
        &input.rules,
        &input.case,
        &wires,
        CaseTargets {
            voltages: &voltages,
            other_voltages: &[],
            ground: input.ground.as_deref(),
            surfaces: &surfaces,
            point_classes: &point_classes,
            points: input.cloud.is_some().then_some(&mut feed as _),
            other_wires: &[],
        },
    )?;
    if let Some(n) = visited {
        report
            .notes
            .push(format!("{n} LiDAR points of the rules' classes checked"));
    }
    for c in &checks {
        let rule = &input.rules[c.rule];
        match &c.outcome {
            Outcome::Skipped(why) => report.notes.push(match why {
                Skipped::NoGround => format!("{}: no ground", rule.name),
                Skipped::NoClasses => format!(
                    "{}: no class of \"{}\" matches",
                    rule.name, input.scheme.name
                ),
                Skipped::NoPoints => format!("{}: no point cloud in this model", rule.name),
                Skipped::NoOtherLines => format!(
                    "{}: other lines are checked by `ohl line --other-line`",
                    rule.name
                ),
                Skipped::AcrossCases => format!(
                    "{}: checked across weather cases only by `ohl line --weather`",
                    rule.name
                ),
            }),
            Outcome::Wires(results) => {
                report.rules.push((
                    rule.name.clone(),
                    ohl_clearance::lowest(results.iter(), false),
                ));
                for (w, r) in results.iter().enumerate() {
                    findings(rule, &c.wires[w], None, r, &mut report.findings);
                }
            }
            // Wires of this line only (no other lines here).
            Outcome::Pairs(pairs) => {
                report.rules.push((
                    rule.name.clone(),
                    ohl_clearance::lowest(pairs.iter().map(|p| &p.result), false),
                ));
                for p in pairs {
                    findings(
                        rule,
                        &c.wires[p.a],
                        Some(&c.wires[p.b]),
                        &p.result,
                        &mut report.findings,
                    );
                }
            }
        }
    }
    report
        .findings
        .sort_by(|a, b| (a.distance - a.required).total_cmp(&(b.distance - b.required)));
    report.took = started.elapsed();
    Ok(report)
}

/// Rule files to choose from: the one given, then the project's clearance
/// rules (`*.ohlr`; next to a line file) — but those holding only tension
/// rules (the wire tension check's, 2026-10-22: nothing to check here).
/// Rules come from the design code used: none are shipped.
pub fn rule_files(model: &Model, given: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = given.map(Path::to_path_buf).into_iter().collect();
    let tension_only = |p: &Path| {
        RuleSet::load(p).is_ok_and(|s| s.rules.is_empty() && !s.tension_rules.is_empty())
    };
    for p in model.project_files("rules") {
        if tension_only(&p) {
            continue;
        }
        // The project's copy of the file given: listed once.
        if !out.iter().any(|q| q.file_name() == p.file_name()) {
            out.push(p);
        }
    }
    out
}

/// The Clearances window's state: the rule file, the last report, the
/// check running.
#[derive(Default)]
pub struct Clearances {
    /// The window is open (Checks › Wire clearances › Show results, View ›
    /// Clearances; a page until 2026-10-27).
    pub open: bool,
    /// Rule files found.
    pub files: Vec<PathBuf>,
    /// The chosen one (index in `files`).
    pub chosen: Option<usize>,
    /// Its rules, or why they could not be read.
    pub rules: Option<Result<Vec<Rule>, String>>,
    /// The last report, or why the check failed.
    pub report: Option<Result<Report, String>>,
    /// Check again after every edit.
    pub auto: bool,
    /// The model changed since the last check started.
    pub stale: bool,
    /// What the rules take where they give none (the project's default
    /// values).
    pub defaults: ohl_clearance::rules::RuleDefaults,
    /// The circuits, routes and alignments checked: the chosen rule
    /// file's `[check]` (owner, 2026-10-27: as the clearance windows).
    pub check: ohl_clearance::CheckScope,
    running: Option<Receiver<Result<Report, String>>>,
    /// The running check's progress, start and Stop.
    progress: Option<(
        Arc<ohl_pointcloud::ReadProgress>,
        Instant,
        ohl_core::budget::CancelToken,
    )>,
}

impl Clearances {
    /// Rule files for a model; one is chosen when it was given or is the
    /// only one.
    pub fn new(model: &Model, given: Option<&Path>) -> Self {
        let files = rule_files(model, given);
        let chosen = (given.is_some() || files.len() == 1).then_some(0);
        let mut c = Self {
            files,
            ..Self::default()
        };
        if let Some(k) = chosen {
            c.choose(k);
        }
        c
    }

    /// The rule files listed again (another project line has its own):
    /// the one chosen kept by name and read again, else the only one.
    pub fn refresh(&mut self, model: &Model, given: Option<&Path>) {
        let name = self
            .chosen
            .and_then(|k| self.files.get(k))
            .and_then(|p| p.file_name())
            .map(ToOwned::to_owned);
        self.files = rule_files(model, given);
        let k = name
            .and_then(|n| self.files.iter().position(|p| p.file_name() == Some(&n)))
            .or((self.files.len() == 1).then_some(0));
        match k {
            Some(k) => self.choose(k),
            None => {
                (self.chosen, self.rules, self.report) = (None, None, None);
                self.stale = true;
            }
        }
    }

    /// Chooses rule file `k` (reads it).
    pub fn choose(&mut self, k: usize) {
        self.chosen = Some(k);
        let set = self
            .files
            .get(k)
            .map(|p| RuleSet::load_with(p, &self.defaults));
        self.check = match &set {
            Some(Ok(s)) => s.check.clone().unwrap_or_default(),
            _ => Default::default(),
        };
        self.rules = set.map(|r| r.map(|r| r.rules));
        self.report = None;
        self.stale = true;
    }

    /// The project's default values: the rules read again when they
    /// changed.
    pub fn set_defaults(&mut self, d: &ohl_line::Defaults) {
        let d = ohl_clearance::rules::RuleDefaults::from(d);
        if d != self.defaults {
            self.defaults = d;
            if let Some(k) = self.chosen {
                self.choose(k);
            }
        }
    }

    /// Whether a check is running.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// The running check's progress: the share of the points read (when
    /// known), the points checked, the seconds since Run.
    pub fn progress(&self) -> Option<(Option<f64>, u64, f64)> {
        let (p, since, _) = self.progress.as_ref()?;
        Some((
            p.share(),
            p.visited.load(std::sync::atomic::Ordering::Relaxed),
            since.elapsed().as_secs_f64(),
        ))
    }

    /// Stops the running check (its report says so).
    pub fn stop(&self) {
        if let Some((_, _, c)) = &self.progress {
            c.cancel();
        }
    }

    /// Keeps the circuits, routes and alignments chosen in the rule
    /// file's `[check]` (read again, written back; a project's working
    /// copy then goes into the project as the windows' do).
    pub fn save_check(&mut self) -> Result<(), String> {
        let Some(path) = self.chosen.and_then(|k| self.files.get(k)) else {
            return Ok(());
        };
        let mut set = RuleSet::load(path)?;
        let mut check = set.check.clone().unwrap_or_default();
        check.circuits = self.check.circuits.clone();
        check.routes = self.check.routes.clone();
        check.alignments = self.check.alignments.clone();
        set.check = Some(check).filter(|c| !c.is_empty());
        set.save(path)?;
        self.stale = true;
        Ok(())
    }

    /// Takes a finished check's report; true when one arrived.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.running else {
            return false;
        };
        match rx.try_recv() {
            Ok(r) => {
                match &r {
                    Ok(rep) => log::info!(
                        "clearances ({}): {} violations in {:.1?}",
                        rep.case,
                        rep.findings.len(),
                        rep.took
                    ),
                    Err(e) => log::warn!("clearances: {e}"),
                }
                self.report = Some(r);
                self.running = None;
                self.progress = None;
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.report = Some(Err("the check stopped".into()));
                self.running = None;
                self.progress = None;
                true
            }
        }
    }

    /// Starts a check (unless one runs: then it runs again after).
    pub fn start(&mut self, ctx: &eframe::egui::Context, input: impl FnOnce(Vec<Rule>) -> Input) {
        if self.running.is_some() {
            self.stale = true;
            return;
        }
        let Some(Ok(rules)) = &self.rules else {
            return;
        };
        self.stale = false;
        let input = input(rules.clone());
        self.progress = Some((input.progress.clone(), Instant::now(), input.cancel.clone()));
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(run(&input));
            ctx.request_repaint();
        });
        self.running = Some(rx);
    }

    /// The violations to draw.
    pub fn findings(&self) -> &[Finding] {
        match &self.report {
            Some(Ok(r)) => &r.findings,
            _ => &[],
        }
    }
}
