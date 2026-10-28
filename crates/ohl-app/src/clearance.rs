// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Clearances window's check (wire clearances), on a background thread
//! with a rule file (`.ohlr`): the wires against the ground, LiDAR points
//! of the rules' classes (read once from the point cloud, in a corridor
//! around the spans) and the line's own wires — in **every weather case
//! of the rules** (owner, 2026-10-28: on the ground windows' design, not
//! only the case shown), at each wind position of the window's wind (the
//! steps from still air to the case's pressure, from the sides chosen;
//! [`ohl_clearance::zone::wind_states`]), and across the cases of the
//! rules' case pairs ([`ohl_clearance::check_case_pairs`]). The findings
//! are the envelope over the positions: per rule, measure and wire (pair)
//! the lowest clearance, with the position that governs it and how many
//! positions are below the rule.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use ohl_clearance::zone::WindState;
use ohl_clearance::{CaseTargets, Outcome, Rule, RuleSet, Skipped, Wire, WireResult};
use ohl_line::Line;
use ohl_line::jumper::JumperShape;
use ohl_line::solve::SectionShape;
use ohl_terrain::Terrain;

use crate::model::Model;
use crate::selection::Selection;

/// One clearance below its requirement: the lowest over the wind
/// positions (and case pairs) of a rule, measure and wire (pair).
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
    /// Points (or wire pieces) below the requirement, at the position
    /// that governs.
    pub count: u64,
    /// A screen rule's band in plan, m (the distance then in plan, see
    /// [`ohl_clearance::zone::screen_text`]).
    pub band: Option<f64>,
    /// Where it was found, in words: the weather case and its wind
    /// ("high wind, 300 of 600 Pa from the left"), or a case pair ("ice ×
    /// everyday").
    pub case: String,
    /// The wind position that governs (index in [`Report::states`]; a
    /// case pair: its first case's).
    pub state: usize,
    /// How many wind positions (and case pairs) are below the rule.
    pub positions: usize,
}

/// The result of a check.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The weather cases checked (names).
    pub cases: Vec<String>,
    /// The wind positions checked.
    pub states: Vec<WindState>,
    /// Violations, worst (largest shortfall) first.
    pub findings: Vec<Finding>,
    /// Per rule: lowest clearances found, in words.
    pub rules: Vec<(String, String)>,
    /// What was skipped and why.
    pub notes: Vec<String>,
    /// How long it took.
    pub took: Duration,
}

impl Report {
    /// The cases checked, in words: "case everyday", "3 weather cases (12
    /// wind positions)".
    pub fn cases_words(&self) -> String {
        match (self.cases.as_slice(), self.states.len()) {
            ([one], 1) => format!("case {one}"),
            ([one], n) => format!("case {one} ({n} wind positions)"),
            (all, n) if n == all.len() => format!("{} weather cases", all.len()),
            (all, n) => format!("{} weather cases ({n} wind positions)", all.len()),
        }
    }
}

/// What a check needs (copied: it runs on its own thread).
pub struct Input {
    /// The line (types resolved).
    pub line: Line,
    /// The wire types (the positions are solved on the check's thread).
    pub library: ohl_cables::Library,
    /// The positions of the wires checked: each weather case of the rules
    /// (and of the scope) at each wind position.
    pub states: Vec<WindState>,
    /// The positions solved already (one per state: its sections and
    /// jumpers); `None`: solved by the check.
    pub solved: Option<Vec<(Vec<SectionShape>, Vec<JumperShape>)>>,
    /// The sections solved (ids; empty: every one).
    pub sections: Vec<String>,
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
    /// The circuits, routes, alignments and cases checked (the rule
    /// file's `[check]`, as the clearance windows keep theirs).
    pub check: ohl_clearance::CheckScope,
    /// How far the reading of the points has got.
    pub progress: Arc<ohl_pointcloud::ReadProgress>,
    /// The positions solved so far.
    pub done: Arc<AtomicUsize>,
    /// Stop.
    pub cancel: ohl_core::budget::CancelToken,
    /// LiDAR points of the rules' classes in the cloud (the time limit,
    /// [`ohl_clearance::zone::time_limit`]).
    pub points: u64,
}

impl Input {
    /// The input of one weather case already solved (`shapes`, `jumpers`),
    /// without wind steps: the check as it was for the case shown (tests).
    #[cfg(test)]
    pub fn one_case(
        m: &Model,
        case: &ohl_weather::WeatherCase,
        shapes: Vec<SectionShape>,
        jumpers: Vec<JumperShape>,
        rules: Vec<Rule>,
    ) -> Self {
        let full = case.wind_pressure.si();
        Self {
            line: m.line.clone(),
            library: m.library.clone(),
            states: vec![WindState {
                case: case.clone(),
                name: case.name.clone(),
                pressure: full,
                full,
                side: (full > 0.0).then_some(case.wind_from),
            }],
            solved: Some(vec![(shapes, jumpers)]),
            sections: Vec::new(),
            rules,
            scheme: m.scheme.clone(),
            cloud: m.cloud.clone(),
            ground: m.ground.clone(),
            surfaces: Default::default(),
            corridor: m.sources.corridor,
            check: Default::default(),
            progress: Default::default(),
            done: Default::default(),
            cancel: Default::default(),
            points: 0,
        }
    }
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

/// What the envelope is kept by: the rule, the measure, the wire and the
/// other wire.
type Key = (usize, &'static str, String, Option<String>);

/// The findings of one result (vertical and radial) at position `case`
/// (`state`), into the envelope `out`: per rule, measure and wire (pair)
/// the lowest, with how many positions are below.
#[allow(clippy::too_many_arguments)]
fn findings(
    k: usize,
    rule: &Rule,
    a: &Wire,
    b: Option<&Wire>,
    r: &WireResult,
    case: &str,
    state: usize,
    out: &mut BTreeMap<Key, Finding>,
) {
    for (measure, hit, required, count) in r.measures() {
        if let (Some(h), Some(req)) = (hit, required)
            && h.distance < req
        {
            // A vertical clearance drawn vertical: its wire end straight
            // above (below) the target (owner, 2026-10-22).
            let (at, target) = h.drawn(measure == "vertical");
            let f = Finding {
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
                case: case.to_owned(),
                state,
                positions: 1,
            };
            let key = (
                k,
                measure,
                f.wire.describe(),
                f.other.as_ref().map(Selection::describe),
            );
            match out.get_mut(&key) {
                Some(o) => {
                    let positions = o.positions + 1;
                    if f.distance - f.required < o.distance - o.required {
                        *o = f;
                    }
                    o.positions = positions;
                }
                None => {
                    out.insert(key, f);
                }
            }
        }
    }
}

/// Runs the check.
pub fn run(input: &Input) -> Result<Report, String> {
    let named = input.surfaces.read();
    let surfaces: Vec<(String, &Terrain)> =
        named.iter().map(|(n, t)| (n.clone(), t.as_ref())).collect();
    let started = Instant::now();
    let rules = &input.rules;
    let states = &input.states;
    let mut report = Report {
        states: states.clone(),
        ..Report::default()
    };
    for s in states {
        if !report.cases.contains(&s.name) {
            report.cases.push(s.name.clone());
        }
    }
    if states.is_empty() {
        return Err("no weather case of the rules is in the model (or in the scope)".into());
    }
    // Every section by the method chosen.
    let own;
    let line = match input.check.method {
        None => &input.line,
        Some(m) => {
            let mut l = input.line.clone();
            for s in &mut l.sections {
                s.calculation = ohl_line::condition::Calculation::of(s.calculation.condition(), m);
            }
            own = l;
            &own
        }
    };
    // Voltages of the sections: every rule must cover them.
    let voltages = ohl_clearance::voltages_of(line)?;
    for rule in rules
        .iter()
        .filter(|r| report.cases.iter().any(|c| ohl_clearance::applies(r, c)))
    {
        rule.check_voltages(&voltages, &[])?;
    }
    let limit = ohl_clearance::zone::time_limit(input.points);
    let budget = || {
        ohl_core::budget::Budget::new()
            .time_limit(limit)
            .cancel_token(input.cancel.clone())
    };
    // Every position solved (here, unless given).
    let solved: Vec<(Vec<SectionShape>, Vec<JumperShape>)> = match &input.solved {
        Some(s) => s.clone(),
        None => {
            let s = ohl_clearance::zone::solve_states(
                line,
                &input.library,
                states,
                &input.sections,
                true,
                &budget(),
                &input.done,
            )?;
            for f in &s.failed {
                report.notes.push(format!("not solved: {f}"));
            }
            s.shapes.into_iter().zip(s.jumpers).collect()
        }
    };
    // The circuits, routes and alignments chosen only (owner, 2026-10-27).
    let on = input.check.wires_on(line);
    let wires: Vec<Vec<Wire>> = solved
        .iter()
        .map(|(shapes, jumpers)| {
            let mut wires = ohl_clearance::wires_of(line, shapes, jumpers);
            wires.retain(|w| {
                input.check.takes_circuit(&w.circuit)
                    && on.as_ref().is_none_or(|(sections, jumpers)| {
                        match w.section.strip_prefix("jumper:") {
                            Some(j) => jumpers.contains(j),
                            None => sections.contains(&w.section),
                        }
                    })
            });
            wires
        })
        .collect();
    if wires.iter().all(Vec::is_empty) && (!input.check.circuits.is_empty() || on.is_some()) {
        return Err("no wire of the circuits, routes and alignments chosen".into());
    }
    let point_classes: Vec<Vec<u8>> = rules
        .iter()
        .map(|r| r.point_classes(|role| input.scheme.codes_with_role(role)))
        .collect();
    // The LiDAR points of the point rules' classes, read once from the
    // point cloud in a corridor around the spans (every position checked
    // against them).
    let mut wanted: Vec<u8> = rules
        .iter()
        .zip(&point_classes)
        .filter(|(r, _)| matches!(r.target, ohl_clearance::Target::Points { .. }))
        .filter(|(r, _)| report.cases.iter().any(|c| ohl_clearance::applies(r, c)))
        .flat_map(|(_, c)| c.iter().copied())
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut points: Vec<([f64; 3], u8)> = Vec::new();
    if let Some(spec) = input.cloud.as_ref().filter(|_| !wanted.is_empty()) {
        let source = ohl_pointcloud::open_source(spec)?;
        let corridor = ohl_lidar::Corridor::along_spans(line, input.corridor);
        // Exit clause: 900 s and 20 s per million points of the rules'
        // classes (owner, 2026-10-27: it scales with the work); Stop.
        let stats = source
            .visit_with(
                Some(&corridor),
                Some(&wanted),
                &budget(),
                Some(&input.progress),
                &mut |x, y, z, c| points.push(([x, y, z], c)),
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
        report.notes.push(format!(
            "{} LiDAR points of the rules' classes checked",
            stats.points_visited
        ));
    }
    let mut feed = |classes: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
        for (p, c) in &points {
            if classes.contains(c) {
                visit(p[0], p[1], p[2], *c);
            }
        }
        Ok(())
    };
    let mut found: BTreeMap<Key, Finding> = BTreeMap::new();
    let mut results: Vec<Vec<WireResult>> = vec![Vec::new(); rules.len()];
    let mut skipped: BTreeSet<(usize, String)> = BTreeSet::new();
    for (k, state) in states.iter().enumerate() {
        if input.cancel.is_cancelled() {
            return Err("stopped".into());
        }
        let label = state.describe();
        let checks = ohl_clearance::check_case(
            rules,
            &state.name,
            &wires[k],
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
        for c in &checks {
            let rule = &rules[c.rule];
            match &c.outcome {
                Outcome::Skipped(why) => {
                    let text = match why {
                        Skipped::NoGround => format!("{}: no ground", rule.name),
                        Skipped::NoClasses => format!(
                            "{}: no class of \"{}\" matches",
                            rule.name, input.scheme.name
                        ),
                        Skipped::NoPoints => {
                            format!("{}: no point cloud in this model", rule.name)
                        }
                        Skipped::NoOtherLines => format!(
                            "{}: other lines are checked by `ohl line --other-line`",
                            rule.name
                        ),
                        // Below: across the cases.
                        Skipped::AcrossCases => continue,
                    };
                    skipped.insert((c.rule, text));
                }
                Outcome::Wires(list) => {
                    for (w, r) in list.iter().enumerate() {
                        findings(c.rule, rule, &c.wires[w], None, r, &label, k, &mut found);
                    }
                    results[c.rule].extend(list.iter().cloned());
                }
                // Wires of this line only (no other lines here).
                Outcome::Pairs(pairs) => {
                    for p in pairs {
                        findings(
                            c.rule,
                            rule,
                            &c.wires[p.a],
                            Some(&c.wires[p.b]),
                            &p.result,
                            &label,
                            k,
                            &mut found,
                        );
                    }
                    results[c.rule].extend(pairs.iter().map(|p| p.result.clone()));
                }
            }
        }
    }
    // The case pairs: each case as it is (its own wind, from each side
    // checked; not the steps) against the other.
    let full: Vec<usize> = (0..states.len())
        .filter(|&k| (states[k].pressure - states[k].full).abs() < 1e-9)
        .collect();
    let cases: Vec<(&str, &[Wire])> = full
        .iter()
        .map(|&k| (states[k].name.as_str(), wires[k].as_slice()))
        .collect();
    let pairs = ohl_clearance::check_case_pairs(rules, &cases, &voltages);
    for c in &pairs {
        let rule = &rules[c.rule];
        let (a, b) = (full[c.first], full[c.second]);
        let label = format!("{} × {}", states[a].describe(), states[b].describe());
        for p in &c.pairs {
            findings(
                c.rule,
                rule,
                &c.wires[p.a],
                Some(&c.others[p.b]),
                &p.result,
                &label,
                a,
                &mut found,
            );
        }
        results[c.rule].extend(c.pairs.iter().map(|p| p.result.clone()));
    }
    for (k, rule) in rules.iter().enumerate() {
        if rule.case_pairs.is_empty() {
            continue;
        }
        if !matches!(rule.target, ohl_clearance::Target::SameLine { .. }) {
            skipped.insert((
                k,
                format!(
                    "{}: other lines across weather cases are checked by `ohl line --weather \
                     --other-line`",
                    rule.name
                ),
            ));
        } else if !pairs.iter().any(|c| c.rule == k) {
            skipped.insert((
                k,
                format!(
                    "{}: no case pair among the cases checked ({})",
                    rule.name,
                    rule.case_pairs
                        .iter()
                        .map(|[a, b]| format!("{a} × {b}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }
    for (k, rule) in rules.iter().enumerate() {
        if !results[k].is_empty() {
            report.rules.push((
                rule.name.clone(),
                ohl_clearance::lowest(results[k].iter(), false),
            ));
        }
    }
    report.notes.extend(skipped.into_iter().map(|(_, t)| t));
    report.findings = found.into_values().collect();
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

/// A running check's progress: the share of the points read (when
/// known), the points checked, the wind positions solved and how many,
/// the seconds since Run.
pub type CheckProgress = (Option<f64>, u64, (usize, usize), f64);

/// A running check's progress, start and Stop.
struct Running {
    read: Arc<ohl_pointcloud::ReadProgress>,
    done: Arc<AtomicUsize>,
    total: usize,
    since: Instant,
    cancel: ohl_core::budget::CancelToken,
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
    /// The circuits, routes, alignments and weather cases checked: the
    /// chosen rule file's `[check]` (owner, 2026-10-27: as the clearance
    /// windows; the cases 2026-10-28).
    pub check: ohl_clearance::CheckScope,
    /// The wind: each case's own, or the steps from still air to it, from
    /// the sides chosen (the rule file's `[wind]`, as the ground windows';
    /// owner, 2026-10-28).
    pub wind: ohl_clearance::rules::WindSteps,
    /// Counts the reports taken (a new one opens the Results tab).
    pub generation: u64,
    running: Option<Receiver<Result<Report, String>>>,
    /// The running check's progress (the points read, the positions
    /// solved of how many), start and Stop.
    progress: Option<Running>,
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
        self.wind = match &set {
            Some(Ok(s)) => s.wind.unwrap_or_default(),
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
    /// known), the points checked, the wind positions solved and how many,
    /// the seconds since Run.
    pub fn progress(&self) -> Option<CheckProgress> {
        let p = self.progress.as_ref()?;
        Some((
            p.read.share(),
            p.read.visited.load(std::sync::atomic::Ordering::Relaxed),
            (p.done.load(std::sync::atomic::Ordering::Relaxed), p.total),
            p.since.elapsed().as_secs_f64(),
        ))
    }

    /// Stops the running check (its report says so).
    pub fn stop(&self) {
        if let Some(p) = &self.progress {
            p.cancel.cancel();
        }
    }

    /// Keeps the circuits, routes, alignments and weather cases chosen in
    /// the rule file's `[check]`, the wind in its `[wind]` (read again,
    /// written back; a project's working copy then goes into the project
    /// as the windows' do).
    pub fn save_check(&mut self) -> Result<(), String> {
        let Some(path) = self.chosen.and_then(|k| self.files.get(k)) else {
            return Ok(());
        };
        let mut set = RuleSet::load(path)?;
        let mut check = set.check.clone().unwrap_or_default();
        check.circuits = self.check.circuits.clone();
        check.routes = self.check.routes.clone();
        check.alignments = self.check.alignments.clone();
        check.cases = self.check.cases.clone();
        set.check = Some(check).filter(|c| !c.is_empty());
        // The wind: written once it is not each case's own.
        if set.wind.is_some() || self.wind != Default::default() {
            set.wind = Some(self.wind);
        }
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
                        rep.cases_words(),
                        rep.findings.len(),
                        rep.took
                    ),
                    Err(e) => log::warn!("clearances: {e}"),
                }
                self.report = Some(r);
                self.running = None;
                self.progress = None;
                self.generation += 1;
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
        self.progress = Some(Running {
            read: input.progress.clone(),
            done: input.done.clone(),
            total: input.states.len(),
            since: Instant::now(),
            cancel: input.cancel.clone(),
        });
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
