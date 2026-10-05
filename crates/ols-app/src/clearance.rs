// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearance violations of the shown weather case, checked on a background
//! thread with a rule file (`.olsr`): the wires against the ground, LiDAR
//! points of the rules' classes (read again from the point cloud each time,
//! in a corridor around the spans: ~2 s for 20 million points, nothing
//! kept in memory) and the line's own wires. After an edit the check runs
//! again.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use ols_clearance::{CaseTargets, Outcome, Rule, RuleSet, Skipped, Wire, WireResult};
use ols_line::Line;
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
use ols_terrain::Terrain;

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
    /// Point on the wire.
    pub at: [f64; 3],
    /// Point on the target (ground, LiDAR point, other wire).
    pub target: [f64; 3],
    /// LiDAR class of the target point.
    pub class: Option<u8>,
    /// Points (or wire pieces) below the requirement.
    pub count: u64,
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
    pub scheme: ols_lidar::ClassScheme,
    /// Point cloud (`FILE.ols` or `FILE.ols#name`), if any.
    pub cloud: Option<String>,
    /// Ground surface.
    pub ground: Option<Arc<Terrain>>,
    /// Half width of the corridor around the spans for points, m.
    pub corridor: f64,
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
            out.push(Finding {
                rule: rule.name.clone(),
                measure,
                wire: wire_selection(a),
                other: b.map(wire_selection),
                distance: h.distance,
                required: req,
                at: h.wire,
                target: h.target,
                class: h.class,
                count: count.max(1),
            });
        }
    }
}

/// Runs the check.
pub fn run(input: &Input) -> Result<Report, String> {
    let started = Instant::now();
    let line = &input.line;
    let mut report = Report {
        case: input.case.clone(),
        ..Report::default()
    };
    // Voltages of the sections: every rule must cover them.
    let voltages = ols_clearance::voltages_of(line)?;
    for rule in input
        .rules
        .iter()
        .filter(|r| ols_clearance::applies(r, &input.case))
    {
        rule.check_voltages(&voltages, &[])?;
    }
    let wires = ols_clearance::wires_of(line, &input.shapes, &input.jumpers);
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
        let source = ols_pointcloud::open_source(spec)?;
        let corridor = ols_lidar::Corridor::along_spans(line, input.corridor);
        // Exit clause: ~1 s per 10⁷ points decoded; 10 min is far beyond
        // any line.
        let budget = ols_core::budget::Budget::new().time_limit(Duration::from_secs(600));
        let stats = source.visit(Some(&corridor), Some(classes), &budget, visit)?;
        visited = Some(stats.points_visited);
        Ok(())
    };
    let checks = ols_clearance::check_case(
        &input.rules,
        &input.case,
        &wires,
        CaseTargets {
            voltages: &voltages,
            other_voltages: &[],
            ground: input.ground.as_deref(),
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
                    "{}: other lines are checked by `ols line --other-line`",
                    rule.name
                ),
                Skipped::AcrossCases => format!(
                    "{}: checked across weather cases only by `ols line --weather`",
                    rule.name
                ),
            }),
            Outcome::Wires(results) => {
                report.rules.push((
                    rule.name.clone(),
                    ols_clearance::lowest(results.iter(), false),
                ));
                for (w, r) in results.iter().enumerate() {
                    findings(rule, &c.wires[w], None, r, &mut report.findings);
                }
            }
            // Wires of this line only (no other lines here).
            Outcome::Pairs(pairs) => {
                report.rules.push((
                    rule.name.clone(),
                    ols_clearance::lowest(pairs.iter().map(|p| &p.result), false),
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

/// Rule files to choose from: the one given, the `*.olsr` next to the
/// model, those in the shipped `libraries/clearance-rules`.
pub fn rule_files(model_file: &Path, given: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = given.map(Path::to_path_buf).into_iter().collect();
    let dirs = [
        ols_core::files::dir_of(model_file),
        ols_core::files::shipped_library("clearance-rules"),
    ];
    for d in &dirs {
        for p in ols_core::files::files_with_extension(d, "olsr") {
            if !out.iter().any(|q| q == &p) {
                out.push(p);
            }
        }
    }
    out
}

/// The Clearances tab's state: the rule file, the last report, the check
/// running.
#[derive(Default)]
pub struct Clearances {
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
    running: Option<Receiver<Result<Report, String>>>,
}

impl Clearances {
    /// Rule files for a model; one is chosen when it was given or is the
    /// only one.
    pub fn new(model_file: &Path, given: Option<&Path>) -> Self {
        let files = rule_files(model_file, given);
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

    /// Chooses rule file `k` (reads it).
    pub fn choose(&mut self, k: usize) {
        self.chosen = Some(k);
        self.rules = self.files.get(k).map(|p| RuleSet::load(p).map(|r| r.rules));
        self.report = None;
        self.stale = true;
    }

    /// Whether a check is running.
    pub fn running(&self) -> bool {
        self.running.is_some()
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
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.report = Some(Err("the check stopped".into()));
                self.running = None;
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
