// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearance zones under and around the wires, with the wind stepped — the
//! **Vertical clearances** and **Radial clearances** windows of the
//! desktop application.
//!
//! - **Vertical** ([`Measure::Vertical`]): the zone straight under the
//!   wire is `width` wide on each side across the line (in plan) and cut
//!   along the line in slices `depth` deep. A target in the zone is
//!   compared with the wire's lowest point over its slice (its highest for
//!   a target above the wire): the clearance is that height difference.
//! - **Radial** ([`Measure::Radial`]): the shortest 3D distance from the
//!   wire.
//!
//! Each rule ([`ZoneRule`]) gives the wires' voltage (or any), the weather
//! cases, the distance to keep and the targets: LiDAR classes, the ground
//! surface. A window's rules are kept as a rule file (`.olsr`,
//! [`to_rule_set`], [`from_rule_set`]): one `[[rule]]` per row and target,
//! read by `ols line --rules` too.
//!
//! **Wind.** A case with wind is solved from each side chosen
//! ([`WindSteps::sides`]). With the wind stepped, it is also solved at
//! every pressure from 0 up to the case's in steps ([`WindSteps::pressures`]):
//! the wires at all of them form the surface the wire sweeps as the wind
//! rises, and each span's clearance is the lowest over all of them
//! ([`wind_states`], [`solve_states`], [`check_zones`]).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ols_core::budget::Budget;
use ols_core::units::Stress;
use ols_line::Line;
use ols_line::solve::SectionShape;
use ols_terrain::Terrain;
use ols_weather::{WeatherCase, WindSide};

use crate::checks::{Hit, PointCheck, Wire, WireSet, checked_parts, ground_check, wires_of};
use crate::rules::{
    CircuitFilter, DEFAULT_SEARCH_MARGIN, PairVoltage, Requirement, Rule, RuleSet, Target,
    WindSteps, default_band,
};

/// Default half width of the vertical zone, m.
pub const DEFAULT_WIDTH: f64 = 1.0;
/// Default depth of the slices of the vertical zone, m.
pub const DEFAULT_DEPTH: f64 = 0.2;
/// Most wind pressures per case and side (the step is made larger).
pub const MAX_STEPS: usize = 200;

/// The clearance a window checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// Straight under (or over) the wire.
    Vertical,
    /// The direct 3D distance.
    Radial,
}

impl Measure {
    /// `vertical` or `radial`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Radial => "radial",
        }
    }
}

/// The zone around the wire where targets count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    /// Vertical or radial.
    pub measure: Measure,
    /// Vertical: half width across the line, m.
    pub width: f64,
    /// Vertical: depth of a slice along the line, m.
    pub depth: f64,
}

impl Zone {
    /// The default zone of `measure`.
    pub fn new(measure: Measure) -> Self {
        Self {
            measure,
            width: DEFAULT_WIDTH,
            depth: DEFAULT_DEPTH,
        }
    }
}

/// One rule of a clearance window.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneRule {
    /// Name, for reports.
    pub name: String,
    /// Nominal voltage of the wires checked, kV (0: earthed wires); `None`:
    /// every wire.
    pub voltage: Option<f64>,
    /// Weather cases (names, `*` `?` wildcards); empty: every case.
    pub cases: Vec<String>,
    /// Clearance to keep, m.
    pub distance: f64,
    /// LiDAR classes of the targets.
    pub classes: Vec<u8>,
    /// The ground surface is a target too.
    pub ground: bool,
}

impl ZoneRule {
    /// Whether the rule applies to weather case `case`.
    pub fn applies(&self, case: &str) -> bool {
        self.cases.is_empty()
            || self
                .cases
                .iter()
                .any(|c| ols_core::pattern::matches(c, case))
    }

    /// The rules of a rule file: one for the classes, one for the ground.
    pub fn to_rules(&self, zone: &Zone) -> Vec<Rule> {
        let vertical = zone.measure == Measure::Vertical;
        let rule = |target: Target| {
            let required = Some(Requirement::Fixed(self.distance));
            Rule {
                name: self.name.clone(),
                vertical: if vertical { required.clone() } else { None },
                radial: if vertical { None } else { required },
                pair_voltage: PairVoltage::Higher,
                vertical_band: if vertical {
                    zone.width
                } else {
                    default_band(&target)
                },
                search_margin: DEFAULT_SEARCH_MARGIN,
                cases: self.cases.clone(),
                case_pairs: Vec::new(),
                exclude_near_ends: 0.0,
                exclude_spans_at: Vec::new(),
                circuits: CircuitFilter::Any,
                voltages: self.voltage.into_iter().collect(),
                vertical_depth: if vertical { zone.depth } else { 0.0 },
                target,
            }
        };
        let mut out = Vec::new();
        if !self.classes.is_empty() {
            out.push(rule(Target::Points {
                classes: self.classes.clone(),
                roles: Vec::new(),
            }));
        }
        if self.ground {
            out.push(rule(Target::Ground));
        }
        out
    }
}

/// A rule file of a window's rules, its zone and its wind.
pub fn to_rule_set(name: &str, zone: &Zone, wind: &WindSteps, rules: &[ZoneRule]) -> RuleSet {
    RuleSet {
        name: name.to_owned(),
        rules: rules.iter().flat_map(|r| r.to_rules(zone)).collect(),
        wind: Some(*wind),
    }
}

/// The rules of a rule file as a window reads them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Imported {
    /// The rules.
    pub rules: Vec<ZoneRule>,
    /// Half width of the vertical zone (from the first rule), m.
    pub width: Option<f64>,
    /// Depth of its slices, m.
    pub depth: Option<f64>,
    /// The wind of the file.
    pub wind: Option<WindSteps>,
    /// Rules left out or changed, and why.
    pub notes: Vec<String>,
}

/// The rules of `set` for a window of `measure`: ground and point rules
/// with a requirement of that measure, one row per voltage of the rule
/// (`voltages`: those of the line, 0 for earthed wires, for requirements
/// by voltage). Roles become the classes of `classes_with_role`. A ground
/// rule following a point rule with the same name and requirement joins
/// its row (as [`to_rule_set`] writes a row with both).
pub fn from_rule_set(
    set: &RuleSet,
    measure: Measure,
    classes_with_role: impl Fn(&str) -> Vec<u8>,
    voltages: &[f64],
) -> Imported {
    let mut out = Imported {
        wind: set.wind,
        ..Imported::default()
    };
    for rule in &set.rules {
        let note = |why: &str| format!("\"{}\": {why}", rule.name);
        let classes = match &rule.target {
            Target::Ground => Vec::new(),
            Target::Points { .. } => {
                let c = rule.point_classes(&classes_with_role);
                if c.is_empty() {
                    out.notes
                        .push(note("left out: no class of the scheme matches its roles"));
                    continue;
                }
                c
            }
            Target::SameLine { .. } | Target::OtherLines => {
                out.notes.push(note(
                    "left out: a rule between wires (Clearances tab, ols line)",
                ));
                continue;
            }
        };
        let required = match measure {
            Measure::Vertical => &rule.vertical,
            Measure::Radial => &rule.radial,
        };
        let Some(required) = required else {
            out.notes.push(note(&format!(
                "left out: no {} clearance",
                measure.as_str()
            )));
            continue;
        };
        if rule.exclude_near_ends > 0.0 || !rule.exclude_spans_at.is_empty() {
            out.notes.push(note("its exclusion zones are not kept"));
        }
        // One row per voltage: the rule's, or each of the line's for a
        // requirement by voltage, or any voltage.
        let rows: Vec<(Option<f64>, f64)> = match required {
            Requirement::Fixed(d) if rule.voltages.is_empty() => vec![(None, *d)],
            Requirement::Fixed(d) => rule.voltages.iter().map(|v| (Some(*v), *d)).collect(),
            r => {
                let kvs: Vec<f64> = if rule.voltages.is_empty() {
                    voltages.to_vec()
                } else {
                    rule.voltages.clone()
                };
                let mut rows = Vec::new();
                for kv in kvs {
                    match r.at(kv) {
                        Ok(d) => rows.push((Some(kv), d)),
                        Err(e) => out.notes.push(note(&e)),
                    }
                }
                if rows.is_empty() {
                    out.notes.push(note("left out: no voltage of the line"));
                }
                rows
            }
        };
        if out.width.is_none() && !rows.is_empty() {
            out.width = Some(rule.vertical_band);
            out.depth = Some(rule.vertical_depth);
        }
        for (voltage, distance) in rows {
            let row = ZoneRule {
                name: rule.name.clone(),
                voltage,
                cases: rule.cases.clone(),
                distance,
                classes: classes.clone(),
                ground: matches!(rule.target, Target::Ground),
            };
            // The ground half of a row written as two rules.
            if let Some(last) = out.rules.last_mut()
                && row.ground
                && !last.ground
                && !last.classes.is_empty()
                && (&last.name, last.voltage, &last.cases, last.distance)
                    == (&row.name, row.voltage, &row.cases, row.distance)
            {
                last.ground = true;
                continue;
            }
            out.rules.push(row);
        }
    }
    out
}

/// The sections of one state solved: one result per section.
type StateSolve = Vec<Result<SectionShape, String>>;

/// One position of the wires: a weather case with its wind at one
/// pressure, from one side.
#[derive(Debug, Clone, PartialEq)]
pub struct WindState {
    /// The weather case as solved (its wind pressure and side set).
    pub case: WeatherCase,
    /// The case's own name.
    pub name: String,
    /// Wind pressure, Pa.
    pub pressure: f64,
    /// The case's full wind pressure, Pa.
    pub full: f64,
    /// Side the wind blows from (`None`: no wind).
    pub side: Option<WindSide>,
}

impl WindState {
    /// "high wind, 300 of 600 Pa from the left", "everyday".
    pub fn describe(&self) -> String {
        match self.side {
            None if self.full > 0.0 => format!("{}, no wind", self.name),
            None => self.name.clone(),
            Some(side) => format!(
                "{}, {:.0} of {:.0} Pa from the {}",
                self.name,
                self.pressure,
                self.full,
                side.as_str()
            ),
        }
    }
}

/// The positions of the wires to check: of every case `wanted`, the
/// pressures of `wind` ([`WindSteps::pressures`]) from each of its sides
/// (0 Pa once).
pub fn wind_states(
    cases: &[WeatherCase],
    wanted: impl Fn(&str) -> bool,
    wind: &WindSteps,
) -> Vec<WindState> {
    let mut out = Vec::new();
    for case in cases.iter().filter(|c| wanted(&c.name)) {
        let full = case.wind_pressure.si();
        for p in wind.pressures(full, MAX_STEPS) {
            let sides: Vec<Option<WindSide>> = if p > 0.0 {
                wind.sides.sides().iter().copied().map(Some).collect()
            } else {
                vec![None]
            };
            for side in sides {
                let mut c = case.clone();
                c.wind_pressure = Stress::from_si(p);
                if let Some(s) = side {
                    c.wind_from = s;
                }
                out.push(WindState {
                    name: case.name.clone(),
                    case: c,
                    pressure: p,
                    full,
                    side,
                });
            }
        }
    }
    out
}

/// The solved sections of every state, and the sections that failed.
#[derive(Debug, Clone, Default)]
pub struct Solved {
    /// Per state: the sections solved.
    pub shapes: Vec<Vec<SectionShape>>,
    /// Sections that failed, with why (once per section).
    pub failed: Vec<String>,
}

/// Solves the sections `sections` (ids; empty: every one) of `line` in
/// every state, spread over the cores. Sections hanging on a common
/// assembly are solved together. `done` counts the states solved. Stops
/// when `budget` runs out (time limit, cancelled), with an error.
pub fn solve_states(
    line: &Line,
    library: &ols_cables::Library,
    states: &[WindState],
    sections: &[String],
    budget: &Budget,
    done: &AtomicUsize,
) -> Result<Solved, String> {
    // Only the sections asked for, and those coupled to them.
    let mut part;
    let line = if sections.is_empty() {
        line
    } else {
        let mut keep: Vec<bool> = line
            .sections
            .iter()
            .map(|s| sections.contains(&s.id))
            .collect();
        for group in ols_line::solve::coupled_groups(line) {
            if group.iter().any(|&k| keep[k]) {
                for k in group {
                    keep[k] = true;
                }
            }
        }
        part = line.clone();
        let mut k = 0;
        part.sections.retain(|_| {
            k += 1;
            keep[k - 1]
        });
        &part
    };
    let run = budget.start();
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let results: Mutex<Vec<Option<StateSolve>>> = Mutex::new(vec![None; states.len()]);
    let mut fe = ols_wirefe::default_fe_budget();
    if let Some(token) = budget.token() {
        fe = fe.cancel_token(token.clone());
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(states.len())
        .max(1);
    let stopped: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Err(e) = run.check() {
                        stop.store(true, Ordering::Relaxed);
                        *stopped.lock().expect("not poisoned") = Some(e.to_string());
                        return;
                    }
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    let Some(state) = states.get(k) else {
                        return;
                    };
                    let solved: Vec<Result<SectionShape, String>> =
                        ols_line::solve::solve_line_with(line, library, &state.case, &fe)
                            .into_iter()
                            .map(|r| r.map_err(|e| e.to_string()))
                            .collect();
                    results.lock().expect("not poisoned")[k] = Some(solved);
                    done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    if let Some(e) = stopped.into_inner().expect("not poisoned") {
        return Err(format!(
            "solving the wind positions {e} ({} of {} solved)",
            done.load(Ordering::Relaxed),
            states.len()
        ));
    }
    let mut out = Solved::default();
    for (state, r) in states
        .iter()
        .zip(results.into_inner().expect("not poisoned"))
    {
        let mut shapes = Vec::new();
        for (section, r) in line.sections.iter().zip(r.unwrap_or_default()) {
            match r {
                Ok(s) => shapes.push(s),
                Err(e) => {
                    if !out.failed.iter().any(|f| f.starts_with(&section.id)) {
                        out.failed
                            .push(format!("{} ({}): {e}", section.id, state.describe()));
                    }
                }
            }
        }
        out.shapes.push(shapes);
    }
    Ok(out)
}

/// One clearance below its requirement (or the lowest found for a rule).
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneFinding {
    /// Index of the rule.
    pub rule: usize,
    /// Section of the span.
    pub section: String,
    /// Span index in the section.
    pub span: usize,
    /// Clearance found, m.
    pub distance: f64,
    /// Clearance required, m.
    pub required: f64,
    /// Point on the wire.
    pub wire: [f64; 3],
    /// Point of the target.
    pub target: [f64; 3],
    /// LiDAR class of the target (`None`: the ground surface).
    pub class: Option<u8>,
    /// Points too close at any wind position (each counted once); for the
    /// ground, wire pieces too close at the worst one.
    pub count: u64,
    /// The wind position of the lowest clearance (index in the states).
    pub state: usize,
}

/// What a check found.
#[derive(Debug, Clone, Default)]
pub struct ZoneReport {
    /// Spans below a requirement, worst (largest shortfall) first.
    pub findings: Vec<ZoneFinding>,
    /// Per rule: the lowest clearance found within reach.
    pub lowest: Vec<Option<ZoneFinding>>,
    /// Points of the rules' classes read.
    pub points: u64,
    /// What was not checked, and why.
    pub notes: Vec<String>,
}

/// What a zone check runs against.
pub struct ZoneTargets<'a> {
    /// The ground surface.
    pub ground: Option<&'a Terrain>,
    /// Feeds the points of the classes asked for to the visitor; `None`:
    /// no points.
    #[allow(clippy::type_complexity)]
    pub points:
        Option<&'a mut dyn FnMut(&[u8], &mut dyn FnMut(f64, f64, f64, u8)) -> Result<(), String>>,
}

/// One rule's check of one kind of target.
struct Part {
    rule: usize,
    def: Rule,
    /// The wires checked: (state, span group) of each.
    meta: Vec<(usize, usize)>,
    wires: Vec<Wire>,
    points: Option<PointCheck>,
    set: Option<WireSet>,
}

/// Checks `rules` in the zone `zone` against the wires of every state
/// (`shapes`, as [`solve_states`] gives them): each span's lowest
/// clearance over all the states of each rule's cases.
pub fn check_zones(
    line: &Line,
    zone: &Zone,
    rules: &[ZoneRule],
    states: &[WindState],
    shapes: &[Vec<SectionShape>],
    targets: ZoneTargets,
    budget: &Budget,
) -> Result<ZoneReport, String> {
    let run = budget.start();
    let mut report = ZoneReport {
        lowest: vec![None; rules.len()],
        ..ZoneReport::default()
    };
    let wires: Vec<Vec<Wire>> = shapes.iter().map(|s| wires_of(line, s, &[])).collect();
    let mut groups: HashMap<(String, usize), usize> = HashMap::new();
    let mut spans: Vec<(String, usize)> = Vec::new();
    let mut group = |w: &Wire| -> usize {
        let key = (w.section.clone(), w.span);
        *groups.entry(key.clone()).or_insert_with(|| {
            spans.push(key);
            spans.len() - 1
        })
    };
    let mut parts: Vec<Part> = Vec::new();
    for (k, rule) in rules.iter().enumerate() {
        if rule.classes.is_empty() && !rule.ground {
            report
                .notes
                .push(format!("\"{}\": no class and no ground ticked", rule.name));
            continue;
        }
        let used: Vec<usize> = (0..states.len())
            .filter(|&s| rule.applies(&states[s].name))
            .collect();
        if used.is_empty() {
            report.notes.push(format!(
                "\"{}\": none of its weather cases is in the model",
                rule.name
            ));
            continue;
        }
        for def in rule.to_rules(zone) {
            let mut meta = Vec::new();
            let mut ws = Vec::new();
            for &s in &used {
                for w in checked_parts(&def, &wires[s]) {
                    if w.points.len() < 2 {
                        continue;
                    }
                    meta.push((s, group(&w)));
                    ws.push(w);
                }
            }
            if ws.is_empty() {
                report.notes.push(format!(
                    "\"{}\": no wire of {}",
                    rule.name,
                    rule.voltage
                        .map_or("any voltage".into(), |v| format!("{v} kV"))
                ));
                continue;
            }
            parts.push(Part {
                rule: k,
                def,
                meta,
                wires: ws,
                points: None,
                set: None,
            });
        }
    }
    let groups_n = spans.len();
    // Points: one pass for every point rule.
    let mut wanted = [false; 256];
    for p in &mut parts {
        let Target::Points { classes, .. } = &p.def.target else {
            continue;
        };
        for c in classes {
            wanted[*c as usize] = true;
        }
        let reach = rules[p.rule].distance + p.def.search_margin;
        let set = WireSet::new(p.wires.clone(), reach);
        let group: Vec<usize> = p.meta.iter().map(|m| m.1).collect();
        p.points = Some(PointCheck::new(&p.def, classes, &set).grouped(group));
        p.set = Some(set);
    }
    let have_points = targets.points.is_some();
    let classes: Vec<u8> = (0..=255u8).filter(|c| wanted[*c as usize]).collect();
    if !classes.is_empty() {
        match targets.points {
            None => report
                .notes
                .push("no point cloud: the LiDAR classes are not checked".into()),
            Some(feed) => {
                let mut n = 0u64;
                let mut checks: Vec<(&mut PointCheck, &WireSet)> = parts
                    .iter_mut()
                    .filter_map(|p| p.points.as_mut().zip(p.set.as_ref()))
                    .collect();
                feed(&classes, &mut |x, y, z, c| {
                    n += 1;
                    for (check, set) in checks.iter_mut() {
                        check.visit(set, x, y, z, c);
                    }
                })?;
                report.points = n;
            }
        }
    }
    run.check()
        .map_err(|e| format!("checking the clearances {e}"))?;
    let vertical = zone.measure == Measure::Vertical;
    let mut findings: Vec<ZoneFinding> = Vec::new();
    for p in &parts {
        let required = rules[p.rule].distance;
        let (results, counts): (Vec<_>, Option<&[[u64; 2]]>) = match (&p.def.target, &p.points) {
            (Target::Points { .. }, Some(check)) if have_points => {
                (check.results.clone(), Some(&check.group_violations))
            }
            (Target::Points { .. }, _) => continue,
            _ => match targets.ground {
                Some(ground) => (ground_check(&p.def, &p.wires, ground), None),
                None => {
                    report
                        .notes
                        .push(format!("\"{}\": no ground surface", rules[p.rule].name));
                    continue;
                }
            },
        };
        run.check()
            .map_err(|e| format!("checking the clearances {e}"))?;
        // The lowest per span over the states, and the count.
        let mut best: Vec<Option<(Hit, usize)>> = vec![None; groups_n];
        let mut ground_counts = vec![0u64; groups_n];
        for (r, &(state, g)) in results.iter().zip(&p.meta) {
            let (hit, n) = if vertical {
                (r.vertical, r.vertical_violations)
            } else {
                (r.radial, r.radial_violations)
            };
            ground_counts[g] = ground_counts[g].max(n);
            if let Some(h) = hit
                && best[g].is_none_or(|(b, _)| h.distance < b.distance)
            {
                best[g] = Some((h, state));
            }
        }
        for (g, b) in best.into_iter().enumerate() {
            let Some((hit, state)) = b else {
                continue;
            };
            let count = match counts {
                Some(c) => c.get(g).map_or(0, |v| v[usize::from(!vertical)]),
                None => ground_counts[g],
            };
            let f = ZoneFinding {
                rule: p.rule,
                section: spans[g].0.clone(),
                span: spans[g].1,
                distance: hit.distance,
                required,
                wire: hit.wire,
                target: hit.target,
                class: hit.class,
                count: count.max(1),
                state,
            };
            let low = &mut report.lowest[p.rule];
            if low.as_ref().is_none_or(|l| f.distance < l.distance) {
                *low = Some(f.clone());
            }
            if f.distance < required {
                findings.push(f);
            }
        }
    }
    findings.sort_by(|a, b| (a.distance - a.required).total_cmp(&(b.distance - b.required)));
    report.findings = findings;
    Ok(report)
}

/// The sections the rules check: those of the voltages asked for (every
/// section when a rule takes any voltage).
pub fn sections_needed(line: &Line, rules: &[ZoneRule]) -> Vec<String> {
    let potentials = line.potentials().unwrap_or_default();
    line.sections
        .iter()
        .filter(|s| {
            let kv = potentials
                .get(&s.id)
                .copied()
                .flatten()
                .map_or(0.0, |p| p.nominal_kv);
            rules
                .iter()
                .any(|r| r.voltage.is_none_or(|v| (v - kv).abs() < 1e-6))
        })
        .map(|s| s.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::WindSides;
    use ols_core::units::Temperature;
    use std::path::Path;

    fn example() -> (Line, ols_cables::Library) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
        let library = ols_cables::Library::load(&root.join("libraries/cables")).unwrap();
        (line, library)
    }

    /// A windy case: 15 °C, `pa` Pa.
    fn windy(pa: f64) -> WeatherCase {
        let mut c = WeatherCase::still("wind", Temperature::from_celsius(15.0));
        c.wind_pressure = Stress::from_si(pa);
        c
    }

    fn rule(distance: f64, classes: &[u8]) -> ZoneRule {
        ZoneRule {
            name: "r".into(),
            voltage: None,
            cases: Vec::new(),
            distance,
            classes: classes.to_vec(),
            ground: false,
        }
    }

    /// Solves and checks `rules` against `points` ([x, y, z], class 5).
    fn check(
        zone: &Zone,
        rules: &[ZoneRule],
        cases: &[WeatherCase],
        wind: &WindSteps,
        points: &[[f64; 3]],
    ) -> (Vec<WindState>, ZoneReport) {
        let (line, library) = example();
        let states = wind_states(cases, |_| true, wind);
        let budget = Budget::new();
        let solved = solve_states(
            &line,
            &library,
            &states,
            &sections_needed(&line, rules),
            &budget,
            &AtomicUsize::new(0),
        )
        .unwrap();
        assert!(solved.failed.is_empty(), "{:?}", solved.failed);
        let mut feed = |classes: &[u8], visit: &mut dyn FnMut(f64, f64, f64, u8)| {
            if classes.contains(&5) {
                for p in points {
                    visit(p[0], p[1], p[2], 5);
                }
            }
            Ok(())
        };
        let report = check_zones(
            &line,
            zone,
            rules,
            &states,
            &solved.shapes,
            ZoneTargets {
                ground: None,
                points: Some(&mut feed),
            },
            &budget,
        )
        .unwrap();
        (states, report)
    }

    /// The finding of S1's first span.
    fn s1(report: &ZoneReport) -> Option<&ZoneFinding> {
        report
            .findings
            .iter()
            .find(|f| (f.section.as_str(), f.span) == ("S1", 0))
    }

    /// The middle of S1's first span in `case`.
    fn s1_middle(case: &WeatherCase) -> [f64; 3] {
        let (line, library) = example();
        let s = &line.sections[0];
        ols_line::solve::solve_one(&line, s, &library, case)
            .unwrap()
            .spans[0]
            .mid_point
    }

    #[test]
    fn pressures_step_up_to_the_case() {
        let w = |stepped, step| WindSteps {
            stepped,
            step,
            sides: WindSides::Both,
        };
        assert_eq!(w(false, 50.0).pressures(600.0, 200), vec![600.0]);
        assert_eq!(w(true, 50.0).pressures(0.0, 200), vec![0.0]);
        assert_eq!(
            w(true, 250.0).pressures(600.0, 200),
            vec![0.0, 250.0, 500.0, 600.0]
        );
        assert_eq!(w(true, 50.0).pressures(600.0, 200).len(), 13);
        // At most `max` steps: the step grows.
        assert_eq!(w(true, 1.0).pressures(600.0, 10).len(), 11);
        // 0 Pa once, then each side.
        let states = wind_states(&[windy(600.0)], |_| true, &w(true, 200.0));
        assert_eq!(states.len(), 1 + 3 * 2);
        assert_eq!(states[0].side, None);
        assert_eq!(states[0].describe(), "wind, no wind");
        assert_eq!(states[2].describe(), "wind, 200 of 600 Pa from the right");
    }

    /// A point where the wire passes at half the wind: missed by the final
    /// state, found when the wind is stepped.
    #[test]
    fn the_wind_envelope_holds_the_intermediate_positions() {
        let full = windy(600.0);
        let mut half = full.clone();
        half.wind_pressure = Stress::from_si(300.0);
        half.wind_from = WindSide::Right;
        let p = s1_middle(&half);
        let last = s1_middle(&full);
        let between =
            ((p[0] - last[0]).powi(2) + (p[1] - last[1]).powi(2) + (p[2] - last[2]).powi(2)).sqrt();
        assert!(between > 2.0, "{between}");
        let zone = Zone::new(Measure::Radial);
        let rules = [rule(1.0, &[5])];
        let wind = |stepped| WindSteps {
            stepped,
            step: 100.0,
            sides: WindSides::Right,
        };
        let started = std::time::Instant::now();
        let (states, stepped) = check(
            &zone,
            &rules,
            std::slice::from_ref(&full),
            &wind(true),
            &[p],
        );
        eprintln!(
            "{} wind positions solved and checked in {:.2?}",
            states.len(),
            started.elapsed()
        );
        let f = s1(&stepped).expect("found at 300 Pa");
        assert!(f.distance < 0.1, "{f:?}");
        assert_eq!(states[f.state].pressure, 300.0);
        assert_eq!(states[f.state].side, Some(WindSide::Right));
        assert_eq!(f.count, 1);
        // Fine steps from both sides: still found (the cost, printed).
        let fine = WindSteps {
            stepped: true,
            step: 10.0,
            sides: WindSides::Both,
        };
        let started = std::time::Instant::now();
        let (states, r) = check(&zone, &rules, std::slice::from_ref(&full), &fine, &[p]);
        eprintln!(
            "{} wind positions solved and checked in {:.2?}",
            states.len(),
            started.elapsed()
        );
        assert_eq!(states.len(), 1 + 60 * 2);
        assert!(s1(&r).is_some_and(|f| f.distance < 0.1));
        let (_, last_only) = check(&zone, &rules, &[full], &wind(false), &[p]);
        assert!(s1(&last_only).is_none(), "{:?}", last_only.findings);
        let low = last_only.lowest[0].as_ref().unwrap();
        assert!(low.distance > 1.0);
    }

    /// A point 3 m under the wire and one 2 m to its side: the vertical
    /// zone (1 m each side) holds the first only, radially both are near.
    #[test]
    fn vertical_and_radial_zones() {
        let still = WeatherCase::still("still", Temperature::from_celsius(15.0));
        let m = s1_middle(&still);
        let (line, _) = example();
        let s = &line.sections[0];
        let a = line.wire_point(&s.points[0]).unwrap().position;
        let b = line.wire_point(&s.points[1]).unwrap().position;
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy);
        let across = [-dy / l, dx / l];
        let under = [m[0], m[1], m[2] - 3.0];
        let beside = [m[0] + 2.0 * across[0], m[1] + 2.0 * across[1], m[2] - 3.0];
        let wind = WindSteps::default();
        let rules = [rule(5.0, &[5])];
        let (_, v) = check(
            &Zone::new(Measure::Vertical),
            &rules,
            std::slice::from_ref(&still),
            &wind,
            &[under, beside],
        );
        let f = s1(&v).unwrap();
        assert_eq!(f.count, 1);
        // 3 m, less the rise of the wire over half a 0.2 m slice (mm).
        assert!((f.distance - 3.0).abs() < 0.02, "{f:?}");
        assert!(f.distance <= 3.0);
        let (_, r) = check(
            &Zone::new(Measure::Radial),
            &rules,
            std::slice::from_ref(&still),
            &wind,
            &[under, beside],
        );
        let f = s1(&r).unwrap();
        assert_eq!(f.count, 2);
        assert!((f.distance - 3.0).abs() < 0.02, "{f:?}");
        // The beside point alone: radially √(2² + 3²), vertically not in
        // the zone.
        let (_, r) = check(
            &Zone::new(Measure::Radial),
            &rules,
            std::slice::from_ref(&still),
            &wind,
            &[beside],
        );
        assert!((s1(&r).unwrap().distance - 13f64.sqrt()).abs() < 0.05);
        let (_, v) = check(
            &Zone::new(Measure::Vertical),
            &rules,
            std::slice::from_ref(&still),
            &wind,
            &[beside],
        );
        assert!(s1(&v).is_none());
        // A wider zone holds it.
        let wide = Zone {
            width: 2.5,
            ..Zone::new(Measure::Vertical)
        };
        let (_, v) = check(
            &wide,
            &rules,
            std::slice::from_ref(&still),
            &wind,
            &[beside],
        );
        assert!(s1(&v).is_some());
        // Every section is of the 220 kV system (S3 through the jumper):
        // a 110 kV rule checks none.
        let at = |kv| {
            [ZoneRule {
                voltage: Some(kv),
                ..rule(5.0, &[5])
            }]
        };
        assert_eq!(sections_needed(&line, &at(220.0)).len(), 3);
        assert!(sections_needed(&line, &at(110.0)).is_empty());
        let (_, v) = check(
            &Zone::new(Measure::Vertical),
            &at(110.0),
            std::slice::from_ref(&still),
            &wind,
            &[under],
        );
        assert!(s1(&v).is_none());
    }

    /// A point where the wire is blown from the left: found with the wind
    /// from the left or both sides, not from the right only.
    #[test]
    fn wind_sides() {
        let mut left = windy(600.0);
        left.wind_from = WindSide::Left;
        let p = s1_middle(&left);
        let zone = Zone::new(Measure::Radial);
        let rules = [rule(1.0, &[5])];
        let run = |sides| {
            let wind = WindSteps {
                stepped: false,
                step: 50.0,
                sides,
            };
            let (states, r) = check(&zone, &rules, &[windy(600.0)], &wind, &[p]);
            s1(&r).map(|f| states[f.state].side)
        };
        assert_eq!(run(WindSides::Left), Some(Some(WindSide::Left)));
        assert_eq!(run(WindSides::Both), Some(Some(WindSide::Left)));
        assert_eq!(run(WindSides::Right), None);
    }

    #[test]
    fn rules_export_and_import() {
        let zone = Zone {
            measure: Measure::Vertical,
            width: 1.5,
            depth: 0.3,
        };
        let wind = WindSteps {
            stepped: true,
            step: 25.0,
            sides: WindSides::Left,
        };
        let rows = vec![
            ZoneRule {
                name: "Trees \"high\"".into(),
                voltage: Some(220.0),
                cases: vec!["high wind".into(), "max temperature".into()],
                distance: 4.5,
                classes: vec![5, 6],
                ground: true,
            },
            ZoneRule {
                name: "Ground points".into(),
                voltage: None,
                cases: Vec::new(),
                distance: 3.25,
                classes: vec![2],
                ground: false,
            },
            ZoneRule {
                name: "Terrain, earth wires".into(),
                voltage: Some(0.0),
                cases: Vec::new(),
                distance: 2.0,
                classes: Vec::new(),
                ground: true,
            },
        ];
        let set = to_rule_set("Window rules", &zone, &wind, &rows);
        assert_eq!(set.rules.len(), 4);
        let text = set.to_toml();
        let back = RuleSet::parse(&text).unwrap();
        assert_eq!(back, set, "{text}");
        let imported = from_rule_set(&back, Measure::Vertical, |_| Vec::new(), &[]);
        assert_eq!(imported.rules, rows);
        assert_eq!((imported.width, imported.depth), (Some(1.5), Some(0.3)));
        assert_eq!(imported.wind, Some(wind));
        assert!(imported.notes.is_empty(), "{:?}", imported.notes);
        // The radial window finds no radial clearance in it.
        let radial = from_rule_set(&back, Measure::Radial, |_| Vec::new(), &[]);
        assert!(radial.rules.is_empty());
        assert_eq!(radial.notes.len(), 4);
    }

    /// Any rule file is written back as the same set; requirements by
    /// voltage become one row per voltage of the line.
    #[test]
    fn rule_files_round_trip() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let set =
            RuleSet::load(&root.join("examples/synthetic-line/clearance-rules.olsr")).unwrap();
        assert_eq!(RuleSet::parse(&set.to_toml()).unwrap(), set);
        let text = r#"
            [[rule]]
            name = "Buildings"
            target = "points"
            roles = ["building"]
            vertical = { by_voltage = [[0, "5 m"], [150, "6 m"], [245, "6.5 m"]] }
            radial = { base = "5 m", per_kv = "0.01 m", above_kv = 50 }
            exclude_near_ends = "10 m"
        "#;
        let set = RuleSet::parse(text).unwrap();
        assert_eq!(RuleSet::parse(&set.to_toml()).unwrap(), set);
        let roles = |r: &str| if r == "building" { vec![6] } else { vec![] };
        let v = from_rule_set(&set, Measure::Vertical, roles, &[0.0, 220.0]);
        let got: Vec<(Option<f64>, f64, Vec<u8>)> = v
            .rules
            .iter()
            .map(|r| (r.voltage, r.distance, r.classes.clone()))
            .collect();
        assert_eq!(
            got,
            vec![(Some(0.0), 5.0, vec![6]), (Some(220.0), 6.5, vec![6])]
        );
        assert_eq!(v.notes.len(), 1, "exclusion zones: {:?}", v.notes);
        let r = from_rule_set(&set, Measure::Radial, roles, &[220.0]);
        assert!((r.rules[0].distance - 6.7).abs() < 1e-12);
    }
}
