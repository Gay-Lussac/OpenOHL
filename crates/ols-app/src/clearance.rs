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

use ols_clearance::{PairMode, PointCheck, Rule, RuleSet, Target, Wire, WireResult, WireSet};
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
    for (measure, hit, required, count) in [
        (
            "vertical",
            r.vertical,
            r.required.vertical,
            r.vertical_violations,
        ),
        ("radial", r.radial, r.required.radial, r.radial_violations),
    ] {
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

/// "lowest vertical 2.00 m, radial 1.25 m" over results.
fn lowest<'a>(results: impl Iterator<Item = &'a WireResult> + Clone) -> String {
    let min = |f: fn(&WireResult) -> Option<ols_clearance::Hit>| {
        results
            .clone()
            .filter_map(f)
            .map(|h| h.distance)
            .min_by(f64::total_cmp)
    };
    let parts: Vec<String> = [
        ("vertical", min(|r| r.vertical)),
        ("radial", min(|r| r.radial)),
    ]
    .into_iter()
    .filter_map(|(k, d)| d.map(|d| format!("{k} {d:.2} m")))
    .collect();
    if parts.is_empty() {
        "nothing within reach".into()
    } else {
        format!("lowest {}", parts.join(", "))
    }
}

/// Whether `rule` applies to `case` (its `cases` list, default all).
fn applies(rule: &Rule, case: &str) -> bool {
    rule.cases.is_empty()
        || rule
            .cases
            .iter()
            .any(|c| ols_weather::name_matches(c, case))
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
    let mut voltages = Vec::new();
    for p in line.potentials().map_err(|e| e.to_string())?.into_values() {
        if !voltages.contains(&p) {
            voltages.push(p);
        }
    }
    let rules: Vec<&Rule> = input
        .rules
        .iter()
        .filter(|r| applies(r, &input.case))
        .collect();
    for rule in &rules {
        rule.check_voltages(&voltages, &[])?;
    }
    let wires = ols_clearance::wires_of(line, &input.shapes, &input.jumpers);
    let reach = rules
        .iter()
        .map(|r| r.reach(&voltages, &[]))
        .fold(0.0, f64::max);
    // Each rule's parts of the wires (exclusion zones), indexed.
    let parts: Vec<Vec<Wire>> = rules
        .iter()
        .map(|r| ols_clearance::checks::checked_parts(r, &wires))
        .collect();
    let sets: Vec<WireSet> = parts
        .iter()
        .map(|w| WireSet::new(w.clone(), reach))
        .collect();
    // Point rules: one pass over the cloud for all of them.
    let mut point_checks: Vec<Option<PointCheck>> = Vec::new();
    let mut wanted = [false; 256];
    for (k, rule) in rules.iter().enumerate() {
        if matches!(rule.target, Target::Points { .. }) {
            let classes = rule.point_classes(|role| input.scheme.codes_with_role(role));
            if classes.is_empty() {
                report.notes.push(format!(
                    "{}: no class of \"{}\" matches",
                    rule.name, input.scheme.name
                ));
                point_checks.push(None);
                continue;
            }
            for &c in &classes {
                wanted[c as usize] = true;
            }
            point_checks.push(Some(PointCheck::new(rule, &classes, &sets[k])));
        } else {
            point_checks.push(None);
        }
    }
    if point_checks.iter().any(Option::is_some) {
        match &input.cloud {
            None => report
                .notes
                .push("point rules: no point cloud in this model".into()),
            Some(spec) => {
                let (file, name) = match spec.split_once('#') {
                    Some((f, n)) => (f, Some(n)),
                    None => (spec.as_str(), None),
                };
                let source = ols_pointcloud::CloudSource::open(Path::new(file), name)
                    .map_err(|e| e.to_string())?;
                let polylines: Vec<Vec<[f64; 2]>> = line
                    .sections
                    .iter()
                    .map(|s| {
                        s.points
                            .iter()
                            .filter_map(|p| line.attachment(p))
                            .map(|a| [a.wire_point[0], a.wire_point[1]])
                            .collect()
                    })
                    .collect();
                let corridor = ols_lidar::Corridor::new(&polylines, input.corridor);
                let classes: Vec<u8> = (0..=255u8).filter(|c| wanted[*c as usize]).collect();
                // Exit clause: ~1 s per 10⁷ points decoded; 10 min is far
                // beyond any line.
                let budget = ols_core::budget::Budget::new().time_limit(Duration::from_secs(600));
                let stats = ols_pointcloud::PointSource::visit(
                    &source,
                    Some(&corridor),
                    Some(&classes),
                    &budget,
                    &mut |x, y, z, c| {
                        for (k, check) in point_checks.iter_mut().enumerate() {
                            if let Some(check) = check {
                                check.visit(&sets[k], x, y, z, c);
                            }
                        }
                    },
                )
                .map_err(|e| e.to_string())?;
                report.notes.push(format!(
                    "{} LiDAR points of the rules' classes checked",
                    stats.points_visited
                ));
            }
        }
    }
    for (k, rule) in rules.iter().enumerate() {
        if !rule.case_pairs.is_empty() {
            report.notes.push(format!(
                "{}: checked across weather cases only by `ols line --weather`",
                rule.name
            ));
            continue;
        }
        let wires = &parts[k];
        match &rule.target {
            Target::Ground | Target::Points { .. } => {
                let results: Vec<WireResult> = match (&rule.target, &input.ground) {
                    (Target::Ground, Some(g)) => {
                        ols_clearance::checks::ground_check(rule, wires, g)
                    }
                    (Target::Ground, None) => {
                        report.notes.push(format!("{}: no ground", rule.name));
                        continue;
                    }
                    _ => match point_checks[k].take() {
                        Some(c) => c.results,
                        None => continue,
                    },
                };
                report
                    .rules
                    .push((rule.name.clone(), lowest(results.iter())));
                for (w, r) in results.iter().enumerate() {
                    findings(rule, &wires[w], None, r, &mut report.findings);
                }
            }
            Target::SameLine { include_same_phase } => {
                let pairs = ols_clearance::checks::wire_pairs(
                    rule,
                    wires,
                    &sets[k],
                    PairMode::SameLine {
                        include_same_phase: *include_same_phase,
                    },
                );
                report
                    .rules
                    .push((rule.name.clone(), lowest(pairs.iter().map(|p| &p.result))));
                for p in &pairs {
                    findings(
                        rule,
                        &wires[p.a],
                        Some(&sets[k].wires[p.b]),
                        &p.result,
                        &mut report.findings,
                    );
                }
            }
            Target::OtherLines => report.notes.push(format!(
                "{}: other lines are checked by `ols line --other-line`",
                rule.name
            )),
        }
    }
    report
        .findings
        .sort_by(|a, b| (a.distance - a.required).total_cmp(&(b.distance - b.required)));
    report.took = started.elapsed();
    Ok(report)
}

/// Rule files to choose from: the one given, the `*.olsr` next to the
/// model, those in `libraries/clearance-rules`.
pub fn rule_files(model_file: &Path, given: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = given.map(Path::to_path_buf).into_iter().collect();
    let dir = model_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    for d in [dir, Path::new("libraries/clearance-rules")] {
        let Ok(entries) = std::fs::read_dir(d) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "olsr"))
            .collect();
        found.sort();
        for p in found {
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
            auto: true,
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
