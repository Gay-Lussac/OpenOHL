// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Clearance zones under and around the wires, with the wind stepped — the
//! **Vertical clearances** and **Radial clearances** windows of the
//! desktop application.
//!
//! - **Vertical** ([`Measure::Vertical`]): the zone straight under the
//!   wire is `width` wide on each side across the line (in plan; a rule
//!   may give its own, [`ZoneRule::width`]) and cut
//!   along the line in slices `depth` deep. A target in the zone is
//!   compared with the wire's lowest point over its slice (its highest for
//!   a target above the wire): the clearance is that height difference.
//! - **Radial** ([`Measure::Radial`]): the shortest 3D distance from the
//!   wire.
//!
//! Each rule ([`ZoneRule`]) gives the wires' voltages (or any), the weather
//! cases, the distance to keep and the targets: LiDAR classes, the ground
//! surface. A window's rules are kept as a rule file (`.ohlr`,
//! [`to_rule_set`], [`from_rule_set`]): one `[[rule]]` per row and target,
//! read by `ohl line --rules` too.
//!
//! **Wind.** A case with wind is solved from each side chosen
//! ([`WindSteps::sides`]). With the wind stepped, it is also solved at
//! every pressure from 0 up to the case's in steps ([`WindSteps::pressures`]):
//! the wires at all of them form the surface the wire sweeps as the wind
//! rises, and each span's clearance is the lowest over all of them
//! ([`wind_states`], [`solve_states`], [`check_zones`]).
//!
//! The whole check, as the windows and `ohl zones` run it: [`run_zone_check`].

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ohl_core::budget::Budget;
use ohl_core::units::Stress;
use ohl_line::Line;
use ohl_line::solve::SectionShape;
use ohl_terrain::Terrain;
use ohl_weather::{WeatherCase, WindSide};

use crate::checks::{
    GroundMax, Hit, PointCheck, PointEvent, Tally, Wire, WireResult, WireSet, checked_parts,
    ground_check_with, wires_of,
};
use crate::rules::{
    CheckScope, CircuitFilter, DEFAULT_SEARCH_MARGIN, PairVoltage, Requirement, Rule, RuleSet,
    Target, WindSteps, Wires, default_band,
};
pub use crate::zone_arc::{ARC_TOLERANCE, ArcCase, ZoneArc, arc_cases, arc_polylines, zone_arcs};
use crate::zone_arc::{ArcPointCheck, arc_ground, arcs_of_rule};
pub use crate::zone_run::{PointReader, ZoneCheckResult, line_voltages, run_zone_check};
use ohl_line::jumper::JumperShape;

/// Default half width of the vertical zone, m.
pub const DEFAULT_WIDTH: f64 = ohl_line::Defaults::PROGRAM.clearance_zone_width;
/// Default depth of the slices of the vertical zone, m.
pub const DEFAULT_DEPTH: f64 = ohl_line::Defaults::PROGRAM.clearance_zone_depth;
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
    /// LiDAR points farther from the wires are ignored, m (`None`: as far
    /// as the rules reach).
    pub search: Option<f64>,
}

impl Zone {
    /// The default zone of `measure`.
    pub fn new(measure: Measure) -> Self {
        Self {
            measure,
            width: DEFAULT_WIDTH,
            depth: DEFAULT_DEPTH,
            search: None,
        }
    }

    /// The zone of `measure` a project's default values give.
    pub fn with_defaults(measure: Measure, d: &ohl_line::Defaults) -> Self {
        Self {
            measure,
            width: d.clearance_zone_width,
            depth: d.clearance_zone_depth,
            search: None,
        }
    }
}

/// One rule of a clearance window.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneRule {
    /// Name, for reports.
    pub name: String,
    /// Nominal voltages of the wires checked, kV (0: earthed wires); empty:
    /// every wire. Optional with any distance: a table by voltage or base
    /// + per kV already depends on the voltage (owner, 2026-10-21).
    pub voltages: Vec<f64>,
    /// Weather cases (names, `*` `?` wildcards); empty: every case.
    pub cases: Vec<String>,
    /// Clearance to keep: a distance, or by the wire's nominal voltage (a
    /// table "up to kV", or a base plus an amount per kV).
    pub distance: Requirement,
    /// LiDAR classes of the targets.
    pub classes: Vec<u8>,
    /// The ground surface is a target too.
    pub ground: bool,
    /// The terrain surface (TIN layer) of the ground, by name (none: the
    /// surface the checks use: the project's check surface or
    /// `--terrain`).
    pub surface: Option<String>,
    /// The wires checked: the sections' spans, the jumpers or both.
    pub wires: Wires,
    /// Vertical: its own half width of the zone across the line, m
    /// (`None`: the window's, [`Zone::width`]).
    pub width: Option<f64>,
}

impl ZoneRule {
    /// Whether the rule applies to weather case `case`.
    pub fn applies(&self, case: &str) -> bool {
        self.cases.is_empty()
            || self
                .cases
                .iter()
                .any(|c| ohl_core::pattern::matches(c, case))
    }

    /// The rules of a rule file: one for the classes, one for the ground.
    pub fn to_rules(&self, zone: &Zone) -> Vec<Rule> {
        let vertical = zone.measure == Measure::Vertical;
        let rule = |target: Target| {
            let required = Some(self.distance.clone());
            Rule {
                name: self.name.clone(),
                vertical: if vertical { required.clone() } else { None },
                radial: if vertical { None } else { required },
                pair_voltage: PairVoltage::Higher,
                vertical_band: if vertical {
                    self.width.unwrap_or(zone.width)
                } else {
                    default_band(&target)
                },
                search_margin: DEFAULT_SEARCH_MARGIN,
                cases: self.cases.clone(),
                case_pairs: Vec::new(),
                exclude_near_ends: 0.0,
                exclude_spans_at: Vec::new(),
                circuits: CircuitFilter::Any,
                voltages: self.voltages.clone(),
                wires: self.wires,
                vertical_depth: if vertical { zone.depth } else { 0.0 },
                surface: matches!(target, Target::Ground)
                    .then(|| self.surface.clone())
                    .flatten(),
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

/// A rule file of a window's rules, its zone, its wind and what it
/// solves (`check`, with the zone's search distance: written when it
/// changes something).
pub fn to_rule_set(
    name: &str,
    zone: &Zone,
    wind: &WindSteps,
    check: &CheckScope,
    rules: &[ZoneRule],
) -> RuleSet {
    RuleSet {
        name: name.to_owned(),
        rules: rules.iter().flat_map(|r| r.to_rules(zone)).collect(),
        wind: Some(*wind),
        check: Some(CheckScope {
            search_distance: zone.search,
            ..check.clone()
        })
        .filter(|c| !c.is_empty()),
        vegetation: None,
        checks: Vec::new(),
        tension_rules: Vec::new(),
        string_rules: Vec::new(),
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
    /// What the file's window solves (`[check]`).
    pub check: Option<CheckScope>,
    /// Rules left out or changed, and why.
    pub notes: Vec<String>,
}

/// The rules of `set` for a window of `measure`: ground and point rules
/// with a requirement of that measure, one row per rule with the rule's
/// voltages (none: any voltage; never one row per voltage, 2026-10-21)
/// and its requirement as written — a distance, a table by voltage or base + per
/// kV (`voltages`: those of the line, 0 for earthed wires: those a
/// table does not reach are noted). Roles become the classes of `classes_with_role`. A ground
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
        check: set.check.clone(),
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
                    "left out: a rule between wires (Clearances tab, ohl line)",
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
        // One row per rule, with the rule's voltages (none: any voltage)
        // and its requirement: a requirement by voltage stays one, the
        // voltages it does not reach noted (and left out of the row).
        let rows: Vec<(Vec<f64>, Requirement)> = if rule.voltages.is_empty() {
            if required.by_voltage() {
                for &kv in voltages {
                    if let Err(e) = required.at(kv) {
                        out.notes.push(note(&e));
                    }
                }
            }
            vec![(Vec::new(), required.clone())]
        } else {
            let mut kept = Vec::new();
            for &kv in &rule.voltages {
                match required.at(kv) {
                    Ok(_) => kept.push(kv),
                    Err(e) => out.notes.push(note(&e)),
                }
            }
            if kept.is_empty() {
                out.notes
                    .push(note("left out: no required clearance for its voltages"));
                Vec::new()
            } else {
                vec![(kept, required.clone())]
            }
        };
        if out.width.is_none() && !rows.is_empty() {
            out.width = Some(rule.vertical_band);
            out.depth = Some(rule.vertical_depth);
        }
        // A vertical rule's own half width: one other than the window's
        // (the first rule's).
        let width = (measure == Measure::Vertical)
            .then_some(rule.vertical_band)
            .filter(|w| out.width.is_some_and(|z| (z - w).abs() > 1e-9));
        for (voltages, distance) in rows {
            let row = ZoneRule {
                name: rule.name.clone(),
                voltages,
                cases: rule.cases.clone(),
                distance,
                classes: classes.clone(),
                ground: matches!(rule.target, Target::Ground),
                surface: rule.surface.clone(),
                wires: rule.wires,
                width,
            };
            // The ground half of a row written as two rules.
            if let Some(last) = out.rules.last_mut()
                && row.ground
                && !last.ground
                && !last.classes.is_empty()
                && (
                    &last.name,
                    &last.voltages,
                    &last.cases,
                    &last.distance,
                    last.wires,
                ) == (
                    &row.name,
                    &row.voltages,
                    &row.cases,
                    &row.distance,
                    row.wires,
                )
                && last.width == row.width
            {
                last.ground = true;
                last.surface = row.surface;
                continue;
            }
            out.rules.push(row);
        }
    }
    out
}

/// The terrain surface of a ground rule naming `surface` (none: the
/// checks' own, [`ZoneTargets::ground`]).
fn ground_of<'a>(
    surface: &Option<String>,
    ground: Option<&'a Terrain>,
    surfaces: &[(String, &'a Terrain)],
) -> Option<&'a Terrain> {
    match surface {
        Some(name) => surfaces.iter().find(|(n, _)| n == name).map(|(_, t)| *t),
        None => ground,
    }
}

/// One state solved: one result per section, then per jumper.
type StateSolve = (
    Vec<Result<SectionShape, String>>,
    Vec<Result<JumperShape, String>>,
);

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
/// (0 Pa once; with the swing arc, still air too).
pub fn wind_states(
    cases: &[WeatherCase],
    wanted: impl Fn(&str) -> bool,
    wind: &WindSteps,
) -> Vec<WindState> {
    let mut out = Vec::new();
    for case in cases.iter().filter(|c| wanted(&c.name)) {
        let full = case.wind_pressure.si();
        let mut pressures = wind.pressures(full, MAX_STEPS);
        // The swing arc swings through still air: solved too.
        if wind.arc && full > 0.0 && pressures.first().is_some_and(|p| *p > 0.0) {
            pressures.insert(0, 0.0);
        }
        for p in pressures {
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
    /// Per state: the jumpers solved (those between the sections solved).
    pub jumpers: Vec<Vec<JumperShape>>,
    /// Sections that failed, with why (once per section).
    pub failed: Vec<String>,
}

/// Solves the sections `sections` (ids; empty: every one) of `line` in
/// every state, spread over the cores, and with `jumpers` the jumpers
/// between them ([`jumpers_needed`]). Sections hanging on a common
/// assembly are solved together. `done` counts the states solved. Stops
/// when `budget` runs out (time limit, cancelled), with an error.
pub fn solve_states(
    line: &Line,
    library: &ohl_cables::Library,
    states: &[WindState],
    sections: &[String],
    jumpers: bool,
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
        for group in ohl_line::solve::coupled_groups(line) {
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
        // The jumpers whose sections are solved.
        let kept: Vec<String> = part.sections.iter().map(|s| s.id.clone()).collect();
        let jumpers: Vec<ohl_line::Jumper> = line
            .jumpers
            .iter()
            .filter(|j| {
                [&j.from, &j.to].into_iter().all(|end| match end {
                    ohl_line::JumperEnd::Wire(at) => line
                        .sections
                        .iter()
                        .filter(|s| s.points.contains(at))
                        .all(|s| kept.contains(&s.id)),
                    ohl_line::JumperEnd::Tap { tap } => line
                        .tap_section(tap, 0)
                        .is_none_or(|s| kept.contains(&s.id)),
                })
            })
            .cloned()
            .collect();
        part.jumpers = jumpers;
        &part
    };
    let run = budget.start();
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let results: Mutex<Vec<Option<StateSolve>>> = Mutex::new(vec![None; states.len()]);
    let mut fe = ohl_wirefe::default_fe_budget();
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
                        ohl_line::solve::solve_line_with(line, library, &state.case, &fe)
                            .into_iter()
                            .map(|r| r.map_err(|e| e.to_string()))
                            .collect();
                    let ok: Vec<SectionShape> = solved
                        .iter()
                        .filter_map(|r| r.as_ref().ok().cloned())
                        .collect();
                    let jumpers = if !jumpers || line.jumpers.is_empty() {
                        Vec::new()
                    } else {
                        ohl_line::jumper::solve_jumpers(line, library, &state.case, &ok)
                            .into_iter()
                            .map(|r| r.map_err(|e| e.to_string()))
                            .collect()
                    };
                    results.lock().expect("not poisoned")[k] = Some((solved, jumpers));
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
        let (r, jr) = r.unwrap_or_default();
        let mut solved_jumpers = Vec::new();
        for (jumper, r) in line.jumpers.iter().zip(jr) {
            match r {
                Ok(j) => solved_jumpers.push(j),
                Err(e) => {
                    let id = format!("jumper {}", jumper.id);
                    if !out.failed.iter().any(|f| f.starts_with(&id)) {
                        out.failed.push(format!("{id} ({}): {e}", state.describe()));
                    }
                }
            }
        }
        out.jumpers.push(solved_jumpers);
        let mut shapes = Vec::new();
        for (section, r) in line.sections.iter().zip(r) {
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
    /// The wind position of the lowest clearance (index in the states;
    /// on a swing arc, its case's still air).
    pub state: usize,
    /// On a swing arc: the angle where the clearance is found, degrees
    /// from the downward vertical about the span's attachment axis,
    /// positive to the right looking along the span.
    pub arc: Option<f64>,
}

impl ZoneFinding {
    /// The segment drawn for it in a check of `measure`: (its end at the
    /// wire, its end at the target) — vertically straight above (below)
    /// the target ([`crate::checks::Hit::drawn`]).
    pub fn drawn(&self, measure: Measure) -> ([f64; 3], [f64; 3]) {
        match measure {
            Measure::Vertical => (
                crate::checks::vertical_mark(self.wire, self.target),
                self.target,
            ),
            Measure::Radial => (self.wire, self.target),
        }
    }

    /// Where the wire is: "high wind, 600 of 600 Pa from the left",
    /// "high wind, swing arc at 23° to the right".
    pub fn position(&self, states: &[WindState]) -> String {
        let s = &states[self.state];
        match self.arc {
            Some(a) => format!("{}, swing arc at {}", s.name, arc_angle(a)),
            None => s.describe(),
        }
    }

    /// The wind in short: "300 Pa from the right", "no wind", "arc 23° to
    /// the right".
    pub fn wind(&self, states: &[WindState]) -> String {
        let s = &states[self.state];
        match (self.arc, s.side) {
            (Some(a), _) => format!("arc {}", arc_angle(a)),
            (None, Some(side)) => format!("{:.0} Pa from the {}", s.pressure, side.as_str()),
            (None, None) => "no wind".into(),
        }
    }
}

/// "23° to the right", "4° to the left", "0° (hanging)".
pub fn arc_angle(deg: f64) -> String {
    if deg.abs() < 0.5 {
        "0° (hanging)".into()
    } else if deg > 0.0 {
        format!("{deg:.0}° to the right")
    } else {
        format!("{:.0}° to the left", -deg)
    }
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
    /// The ground surface (of the rules naming none).
    pub ground: Option<&'a Terrain>,
    /// The terrain surfaces the rules name ([`ZoneRule::surface`]), by
    /// name.
    pub surfaces: &'a [(String, &'a Terrain)],
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
    /// The swing arcs checked (index in the arcs) and their groups.
    arcs: Vec<usize>,
    arc_groups: Vec<usize>,
    arc_points: Option<ArcPointCheck>,
}

/// Checks `rules` in the zone `zone` against the wires of every state
/// (`shapes` and `jumpers`, as [`solve_states`] gives them; `jumpers` may
/// be empty) and the spans' swing arcs `arcs` ([`zone_arcs`]; none: no
/// arc): each span's (and jumper's) lowest clearance over all the
/// states of each rule's cases and their arcs.
#[allow(clippy::too_many_arguments)]
pub fn check_zones(
    line: &Line,
    zone: &Zone,
    rules: &[ZoneRule],
    states: &[WindState],
    shapes: &[Vec<SectionShape>],
    jumpers: &[Vec<JumperShape>],
    arcs: &[ZoneArc],
    targets: ZoneTargets,
    budget: &Budget,
) -> Result<ZoneReport, String> {
    let run = budget.start();
    let mut report = ZoneReport {
        lowest: vec![None; rules.len()],
        ..ZoneReport::default()
    };
    let wires: Vec<Vec<Wire>> = shapes
        .iter()
        .enumerate()
        .map(|(k, s)| wires_of(line, s, jumpers.get(k).map_or(&[][..], Vec::as_slice)))
        .collect();
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
                    // A requirement by voltage reaches every wire checked
                    // (else an error before any check, as `ohl line`).
                    if let Err(e) = rule.distance.at(w.kv().unwrap_or(0.0)) {
                        return Err(format!("rule \"{}\": {e}", rule.name));
                    }
                    meta.push((s, group(&w)));
                    ws.push(w);
                }
            }
            if ws.is_empty() {
                report.notes.push(format!(
                    "\"{}\": no {} of {}",
                    rule.name,
                    match rule.wires {
                        Wires::Both => "wire",
                        Wires::Sections => "section",
                        Wires::Jumpers => "jumper",
                    },
                    voltages_text(&rule.voltages)
                ));
                continue;
            }
            let arc_items = arcs_of_rule(&def, arcs, &used);
            let arc_groups = arc_items.iter().map(|&a| group(&arcs[a].wire)).collect();
            parts.push(Part {
                rule: k,
                def,
                meta,
                wires: ws,
                points: None,
                set: None,
                arcs: arc_items,
                arc_groups,
                arc_points: None,
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
        // The index reaches the zone's edge too, not beyond the search
        // distance.
        // The largest requirement of its wires (by their voltages).
        let largest = p
            .wires
            .iter()
            .map(|w| p.def.required(w.kv()).largest())
            .fold(0.0, f64::max);
        let reach = (largest + p.def.search_margin)
            .max(p.def.vertical_band)
            .min(zone.search.unwrap_or(f64::INFINITY));
        let set = WireSet::new(p.wires.clone(), reach);
        let mut group: Vec<usize> = p.meta.iter().map(|m| m.1).collect();
        // Room for the arcs' groups too.
        group.extend(p.arc_groups.iter().copied());
        p.points = Some(
            PointCheck::new(&p.def, classes, &set)
                .only(p.def.vertical.is_some(), p.def.radial.is_some())
                .grouped(group),
        );
        p.set = Some(set);
        if !p.arcs.is_empty() {
            p.arc_points = Some(ArcPointCheck::new(
                &p.def,
                arcs,
                p.arcs.clone(),
                p.arc_groups.clone(),
                reach,
            ));
        }
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
                // In blocks, worked out on every core and applied in the
                // points' order: the results of one point at a time.
                let mut block: Vec<([f64; 3], u8)> = Vec::with_capacity(BLOCK);
                feed(&classes, &mut |x, y, z, c| {
                    n += 1;
                    block.push(([x, y, z], c));
                    if block.len() == BLOCK {
                        check_block(&mut parts, arcs, &block);
                        block.clear();
                    }
                })?;
                check_block(&mut parts, arcs, &block);
                report.points = n;
            }
        }
    }
    run.check()
        .map_err(|e| format!("checking the clearances {e}"))?;
    let vertical = zone.measure == Measure::Vertical;
    let mut findings: Vec<ZoneFinding> = Vec::new();
    // The highest ground per cell, made once per surface for every rule.
    let mut tops: Vec<(Option<String>, GroundMax)> = Vec::new();
    for p in &parts {
        // (result, state, group, arc index) of every wire and arc.
        type Entry = (WireResult, usize, usize, Option<usize>);
        let (results, counts): (Vec<Entry>, Option<&[[u64; 2]]>) = match (&p.def.target, &p.points)
        {
            (Target::Points { .. }, Some(check)) if have_points => {
                let mut all: Vec<Entry> = check
                    .results
                    .iter()
                    .zip(&p.meta)
                    .map(|(r, &(s, g))| (r.clone(), s, g, None))
                    .collect();
                if let Some(a) = &p.arc_points {
                    for (k, r) in a.results.iter().enumerate() {
                        let arc = a.items[k];
                        all.push((r.clone(), arcs[arc].state, a.groups[k], Some(arc)));
                    }
                }
                (all, Some(&check.group_violations))
            }
            (Target::Points { .. }, _) => continue,
            _ => match ground_of(&p.def.surface, targets.ground, targets.surfaces) {
                Some(ground) => {
                    let k = match tops.iter().position(|(n, _)| *n == p.def.surface) {
                        Some(k) => k,
                        None => {
                            tops.push((p.def.surface.clone(), GroundMax::new(ground)));
                            tops.len() - 1
                        }
                    };
                    let top = &tops[k].1;
                    let mut all: Vec<Entry> = ground_check_with(&p.def, &p.wires, ground, top)
                        .into_iter()
                        .zip(&p.meta)
                        .map(|(r, &(s, g))| (r, s, g, None))
                        .collect();
                    let found = crate::positions::parallel(p.arcs.len(), budget, |k| {
                        Ok(arc_ground(&p.def, &arcs[p.arcs[k]], ground, top))
                    })
                    .map_err(|e| format!("checking the clearances {e}"))?;
                    for ((&arc, &g), r) in p.arcs.iter().zip(&p.arc_groups).zip(found) {
                        all.push((r, arcs[arc].state, g, Some(arc)));
                    }
                    (all, None)
                }
                None => {
                    report.notes.push(match &p.def.surface {
                        Some(n) => format!(
                            "\"{}\": no terrain surface \"{n}\" in the project",
                            rules[p.rule].name
                        ),
                        None => format!("\"{}\": no terrain surface", rules[p.rule].name),
                    });
                    continue;
                }
            },
        };
        run.check()
            .map_err(|e| format!("checking the clearances {e}"))?;
        // The lowest per span over the states, and the count.
        // (hit, state, the wire's requirement: a span's wires are of one
        // section, one voltage)
        // (hit, state, requirement, arc) of each span.
        type Lowest = Option<(Hit, usize, f64, Option<usize>)>;
        let mut best: Vec<Lowest> = vec![None; groups_n];
        let mut ground_counts = vec![0u64; groups_n];
        for (r, state, g, arc) in &results {
            let (state, g, arc) = (*state, *g, *arc);
            let (hit, n, required) = if vertical {
                (r.vertical, r.vertical_violations, r.required.vertical)
            } else {
                (r.radial, r.radial_violations, r.required.radial)
            };
            let required = required.unwrap_or(0.0);
            ground_counts[g] = ground_counts[g].max(n);
            if let Some(h) = hit
                && best[g].is_none_or(|(b, ..)| h.distance < b.distance)
            {
                best[g] = Some((h, state, required, arc));
            }
        }
        for (g, b) in best.into_iter().enumerate() {
            let Some((hit, state, required, arc)) = b else {
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
                arc: arc.map(|a| arcs[a].arc.angle_of(hit.wire).to_degrees()),
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

/// Points worked out together on every core.
const BLOCK: usize = 1 << 16;

/// Checks a block of points against every part: each core works out a
/// stretch of the block ([`PointCheck::eval`], [`ArcPointCheck::eval`])
/// from the results so far, then the events are applied in the points'
/// order — the results, the counts and the first of equal clearances as
/// one point at a time gives them.
fn check_block(parts: &mut [Part], arcs: &[ZoneArc], block: &[([f64; 3], u8)]) {
    if block.is_empty() {
        return;
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(block.len().div_ceil(1024))
        .max(1);
    let size = block.len().div_ceil(threads);
    /// One stretch's events: per point, whether it was taken and where
    /// its wire and arc events end.
    struct Stretch {
        taken: Vec<bool>,
        wire_end: Vec<usize>,
        arc_end: Vec<usize>,
        wire: Vec<PointEvent>,
        arc: Vec<PointEvent>,
        /// The counts (added up in any order).
        tally: Tally,
    }
    for p in parts.iter_mut() {
        let (Some(check), Some(set)) = (p.points.as_mut(), p.set.as_ref()) else {
            continue;
        };
        let arc_check = p.arc_points.as_mut();
        let stretches: Vec<Stretch> = {
            let (check, arc_check) = (&*check, arc_check.as_deref());
            std::thread::scope(|s| {
                let handles: Vec<_> = block
                    .chunks(size)
                    .enumerate()
                    .map(|(k, chunk)| {
                        // The points' numbers, above every one before.
                        let first = check.offered + (k * size) as u64;
                        s.spawn(move || {
                            let mut best = check.best.clone();
                            let mut arc_best =
                                arc_check.map(|a| a.best.clone()).unwrap_or_default();
                            let mut out = Stretch {
                                taken: Vec::with_capacity(chunk.len()),
                                wire_end: Vec::with_capacity(chunk.len()),
                                arc_end: Vec::with_capacity(chunk.len()),
                                wire: Vec::new(),
                                arc: Vec::new(),
                                tally: Tally::new(check.best.len(), check.groups()),
                            };
                            for (i, &(q, c)) in chunk.iter().enumerate() {
                                let stamp = first + i as u64 + 1;
                                let taken = check.eval(
                                    set,
                                    &mut best,
                                    Some(&mut out.tally),
                                    stamp,
                                    q[0],
                                    q[1],
                                    q[2],
                                    c,
                                    &mut out.wire,
                                );
                                if taken && let Some(a) = arc_check {
                                    a.eval(
                                        arcs,
                                        &mut arc_best,
                                        Some(&mut out.tally),
                                        stamp,
                                        q,
                                        c,
                                        &mut out.arc,
                                    );
                                }
                                out.taken.push(taken);
                                out.wire_end.push(out.wire.len());
                                out.arc_end.push(out.arc.len());
                            }
                            out
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().expect("a point check thread"))
                    .collect()
            })
        };
        check.offered += block.len() as u64;
        let mut arc_check = arc_check;
        for st in &stretches {
            let (mut w0, mut a0) = (0, 0);
            for k in 0..st.taken.len() {
                let (w1, a1) = (st.wire_end[k], st.arc_end[k]);
                if st.taken[k] {
                    check.apply(&st.wire[w0..w1]);
                    if let Some(a) = arc_check.as_deref_mut() {
                        a.apply(&st.arc[a0..a1], check);
                    }
                }
                (w0, a0) = (w1, a1);
            }
            check.take_tally(&st.tally);
        }
    }
}

/// A rule's voltages in words: "any voltage", "220 kV", "110 or 220 kV".
pub fn voltages_text(voltages: &[f64]) -> String {
    match voltages {
        [] => "any voltage".into(),
        [v] => format!("{v} kV"),
        [first @ .., last] => format!(
            "{} or {last} kV",
            first
                .iter()
                .map(f64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Whether a rule checks jumpers.
pub fn jumpers_needed(rules: &[ZoneRule]) -> bool {
    rules.iter().any(|r| r.wires != Wires::Sections)
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
                .any(|r| r.voltages.is_empty() || r.voltages.iter().any(|v| (v - kv).abs() < 1e-6))
        })
        .map(|s| s.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::WindSides;
    use ohl_core::units::Temperature;
    use std::path::Path;

    fn example() -> (Line, ohl_cables::Library) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
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
            voltages: Vec::new(),
            cases: Vec::new(),
            distance: Requirement::Fixed(distance),
            classes: classes.to_vec(),
            ground: false,
            surface: None,
            wires: Wires::Both,
            width: None,
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
            jumpers_needed(rules),
            &budget,
            &AtomicUsize::new(0),
        )
        .unwrap();
        // Taut jumpers in strong wind are reported, the sections solve.
        assert!(
            solved.failed.iter().all(|f| f.starts_with("jumper ")),
            "{:?}",
            solved.failed
        );
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
            &solved.jumpers,
            &[],
            ZoneTargets {
                ground: None,
                surfaces: &[],
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
        ohl_line::solve::solve_one(&line, s, &library, case)
            .unwrap()
            .spans[0]
            .mid_point
    }

    /// The whole check: the points through the caller's reader, the
    /// circuits chosen.
    #[test]
    fn a_whole_zone_check() {
        let (line, library) = example();
        let case = WeatherCase::still("still", Temperature::from_celsius(15.0));
        let mid = s1_middle(&case);
        let states = wind_states(std::slice::from_ref(&case), |_| true, &WindSteps::default());
        let read = |polylines: &[Vec<[f64; 2]>],
                    reach: f64,
                    classes: &[u8],
                    _: &Budget,
                    visit: &mut dyn FnMut(f64, f64, f64, u8)| {
            assert!(!polylines.is_empty());
            // The rule's 5 m, the margin, the zone's width and 1 m.
            assert!(reach > 5.0, "{reach}");
            assert_eq!(classes, [5]);
            visit(mid[0], mid[1], mid[2] - 3.0, 5);
            Ok(())
        };
        let run = |check: &CheckScope| {
            run_zone_check(
                &line,
                &library,
                Zone::new(Measure::Vertical),
                check,
                vec![rule(5.0, &[5])],
                states.clone(),
                None,
                Some(&read),
                None,
                &[],
                None,
                &Budget::new(),
                &AtomicUsize::new(0),
            )
        };
        let r = run(&CheckScope::default()).unwrap();
        assert_eq!(r.sections.len(), line.sections.len());
        assert_eq!(r.zone.points, 1);
        let f = s1(&r.zone).unwrap();
        assert!((f.distance - 3.0).abs() < 0.05, "{}", f.distance);
        assert_eq!(r.violations(), 1);
        let none = run(&CheckScope {
            circuits: vec!["no such circuit".into()],
            ..CheckScope::default()
        })
        .unwrap_err();
        assert!(none.contains("circuits chosen"), "{none}");
    }

    /// A rule by voltage: each wire meets the row of its system's nominal
    /// voltage (S1 is of the 220 kV system: the 245 kV row, 4 m; a 3 m
    /// clearance is below it, below the 0 kV row's 1 m it would not be);
    /// a table that ends below 220 kV stops the check with an error.
    #[test]
    fn a_zone_rule_by_voltage() {
        let case = WeatherCase::still("still", Temperature::from_celsius(15.0));
        let mid = s1_middle(&case);
        let point = [mid[0], mid[1], mid[2] - 3.0];
        let mut r = rule(1.0, &[5]);
        r.distance = Requirement::ByVoltage(vec![(0.0, 1.0), (245.0, 4.0)]);
        let zone = Zone::new(Measure::Vertical);
        let (_, report) = check(
            &zone,
            std::slice::from_ref(&r),
            std::slice::from_ref(&case),
            &WindSteps::default(),
            &[point],
        );
        let f = s1(&report).unwrap();
        assert_eq!(f.required, 4.0);
        assert!((f.distance - 3.0).abs() < 0.05, "{}", f.distance);
        // base + per kV: 1 m + 0.02 m/kV · (220 − 100) kV = 3.4 m.
        r.distance = Requirement::Formula {
            base: 1.0,
            per_kv: 0.02,
            above_kv: 100.0,
        };
        let (_, report) = check(
            &zone,
            std::slice::from_ref(&r),
            std::slice::from_ref(&case),
            &WindSteps::default(),
            &[point],
        );
        assert!((s1(&report).unwrap().required - 3.4).abs() < 1e-12);
        // The table ends at 150 kV: no requirement for the 220 kV wires.
        let (line, library) = example();
        r.distance = Requirement::ByVoltage(vec![(0.0, 1.0), (150.0, 2.0)]);
        let states = wind_states(std::slice::from_ref(&case), |_| true, &WindSteps::default());
        let budget = Budget::new();
        let rules = vec![r];
        let solved = solve_states(
            &line,
            &library,
            &states,
            &sections_needed(&line, &rules),
            false,
            &budget,
            &AtomicUsize::new(0),
        )
        .unwrap();
        let e = check_zones(
            &line,
            &zone,
            &rules,
            &states,
            &solved.shapes,
            &[],
            &[],
            ZoneTargets {
                ground: None,
                surfaces: &[],
                points: None,
            },
            &budget,
        )
        .unwrap_err();
        assert!(e.contains("220 kV") && e.contains("150 kV"), "{e}");
    }

    #[test]
    fn pressures_step_up_to_the_case() {
        let w = |stepped, step| WindSteps {
            stepped,
            step,
            sides: WindSides::Both,
            arc: false,
            ..Default::default()
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

    /// A rule takes the sections' spans, the jumpers or both: a point under
    /// a jumper is found by the jumper rules only.
    #[test]
    fn rules_for_sections_jumpers_or_both() {
        let (line, library) = example();
        let still = [windy(0.0)];
        let states = wind_states(&still, |_| true, &WindSteps::default());
        let solved = solve_states(
            &line,
            &library,
            &states,
            &[],
            true,
            &Budget::new(),
            &AtomicUsize::new(0),
        )
        .unwrap();
        let j = &solved.jumpers[0][0];
        let low = j.low_point;
        let point = [low[0], low[1], low[2] - 1.5];
        let zone = Zone::new(Measure::Vertical);
        let found = |wires: Wires| {
            let rules = [ZoneRule {
                wires,
                ..rule(3.0, &[5])
            }];
            let (_, report) = check(&zone, &rules, &still, &WindSteps::default(), &[point]);
            report
                .findings
                .iter()
                .map(|f| f.section.clone())
                .collect::<Vec<_>>()
        };
        let jumper = format!("jumper:{}", j.jumper);
        assert!(found(Wires::Jumpers).contains(&jumper));
        assert!(found(Wires::Both).contains(&jumper));
        assert!(!found(Wires::Sections).contains(&jumper));
        assert!(
            found(Wires::Jumpers)
                .iter()
                .all(|s| s.starts_with("jumper:"))
        );
        assert!(!jumpers_needed(&[ZoneRule {
            wires: Wires::Sections,
            ..rule(3.0, &[5])
        }]));
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
            arc: false,
            ..Default::default()
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
            arc: false,
            ..Default::default()
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
        // Points beyond the search distance are ignored.
        let near = Zone {
            search: Some(2.0),
            ..Zone::new(Measure::Radial)
        };
        let (_, r) = check(
            &near,
            &rules,
            std::slice::from_ref(&still),
            &wind,
            &[beside],
        );
        assert!(s1(&r).is_none());
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
        // So does a rule's own width in the narrow zone.
        let own = [ZoneRule {
            width: Some(2.5),
            ..rules[0].clone()
        }];
        let (_, v) = check(
            &Zone::new(Measure::Vertical),
            &own,
            std::slice::from_ref(&still),
            &wind,
            &[beside],
        );
        assert!(s1(&v).is_some());
        // Every section is of the 220 kV system (S3 through the jumper):
        // a 110 kV rule checks none.
        let at = |kv| {
            [ZoneRule {
                voltages: vec![kv],
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
                arc: false,
                ..Default::default()
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
            search: Some(30.0),
        };
        let wind = WindSteps {
            stepped: true,
            step: 25.0,
            sides: WindSides::Left,
            arc: false,
            ..Default::default()
        };
        let rows = vec![
            ZoneRule {
                name: "Trees \"high\"".into(),
                voltages: vec![220.0],
                cases: vec!["high wind".into(), "max temperature".into()],
                distance: Requirement::Fixed(4.5),
                classes: vec![5, 6],
                ground: true,
                surface: None,
                wires: Wires::Both,
                width: None,
            },
            ZoneRule {
                name: "Ground points".into(),
                voltages: Vec::new(),
                cases: Vec::new(),
                distance: Requirement::Fixed(3.25),
                classes: vec![2],
                ground: false,
                surface: None,
                wires: Wires::Sections,
                // Its own half width.
                width: Some(4.0),
            },
            ZoneRule {
                name: "Terrain, earth wires".into(),
                voltages: vec![0.0],
                cases: Vec::new(),
                distance: Requirement::Fixed(2.0),
                classes: Vec::new(),
                ground: true,
                surface: None,
                wires: Wires::Jumpers,
                width: None,
            },
        ];
        let scope = CheckScope {
            circuits: vec!["North".into(), String::new()],
            method: Some(ohl_line::condition::Method::Fe),
            search_distance: None,
        };
        let set = to_rule_set("Window rules", &zone, &wind, &scope, &rows);
        assert_eq!(set.rules.len(), 4);
        assert_eq!(set.rules[2].vertical_band, 4.0);
        assert_eq!(set.rules[3].vertical_band, 1.5);
        let text = set.to_toml();
        assert!(
            text.contains(
                "[check]\ncircuits = [\"North\", \"\"]\nmethod = \"fe\"\nsearch_distance = \"30 m\""
            ),
            "{text}"
        );
        let back = RuleSet::parse(&text).unwrap();
        assert_eq!(back, set, "{text}");
        let imported = from_rule_set(&back, Measure::Vertical, |_| Vec::new(), &[]);
        assert_eq!(imported.rules, rows);
        assert_eq!((imported.width, imported.depth), (Some(1.5), Some(0.3)));
        assert_eq!(imported.wind, Some(wind));
        assert_eq!(
            imported.check,
            Some(CheckScope {
                search_distance: Some(30.0),
                ..scope
            })
        );
        // Nothing to say: no [check].
        let near = Zone {
            search: None,
            ..zone
        };
        let plain = to_rule_set("w", &near, &wind, &CheckScope::default(), &rows);
        assert!(plain.check.is_none() && !plain.to_toml().contains("[check]"));
        assert!(imported.notes.is_empty(), "{:?}", imported.notes);
        // The radial window finds no radial clearance in it.
        let radial = from_rule_set(&back, Measure::Radial, |_| Vec::new(), &[]);
        assert!(radial.rules.is_empty());
        assert_eq!(radial.notes.len(), 4);
    }

    /// Any rule file is written back as the same set; a requirement by
    /// voltage stays one row with its table or formula (the window edits
    /// it as written), the line's voltages it does not reach noted.
    #[test]
    fn rule_files_round_trip() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let set =
            RuleSet::load(&root.join("examples/synthetic-line/clearance-rules.ohlr")).unwrap();
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
        let got: Vec<(Vec<f64>, Requirement, Vec<u8>)> = v
            .rules
            .iter()
            .map(|r| (r.voltages.clone(), r.distance.clone(), r.classes.clone()))
            .collect();
        assert_eq!(
            got,
            vec![(
                Vec::new(),
                Requirement::ByVoltage(vec![(0.0, 5.0), (150.0, 6.0), (245.0, 6.5)]),
                vec![6]
            )]
        );
        assert_eq!(v.notes.len(), 1, "exclusion zones: {:?}", v.notes);
        // At 220 kV: 6.5 m (the 245 kV row); 5 + 0.01 · (220 − 50) = 6.7 m.
        assert_eq!(v.rules[0].distance.at(220.0), Ok(6.5));
        let r = from_rule_set(&set, Measure::Radial, roles, &[220.0]);
        assert!((r.rules[0].distance.at(220.0).unwrap() - 6.7).abs() < 1e-12);
        // Written back by the window: the same requirements.
        let zone = Zone::new(Measure::Vertical);
        let back = to_rule_set(
            "w",
            &zone,
            &WindSteps::default(),
            &CheckScope::default(),
            &v.rules,
        );
        assert_eq!(back.rules[0].vertical, set.rules[0].vertical);
        // A line voltage beyond the table is noted.
        let high = from_rule_set(&set, Measure::Vertical, roles, &[380.0]);
        assert!(
            high.notes.iter().any(|n| n.contains("380 kV")),
            "{:?}",
            high.notes
        );
    }

    /// A rule with a table by voltage *and* a filter of several voltages
    /// (an older file): one window row with those voltages, never one row
    /// per voltage (2026-10-21); written back the same, and checking only
    /// the wires of its voltages. A voltage the table does not reach is
    /// noted and left out of the row.
    #[test]
    fn a_rule_of_several_voltages_is_one_row() {
        let text = r#"
            format = "ohl-clearance-rules"
            version = 1
            [[rule]]
            name = "Trees"
            target = "ground"
            vertical = { by_voltage = [[0, "5 m"], [150, "6 m"], [400, "7 m"]] }
            voltages = [110, 220, 500]
        "#;
        let set = RuleSet::parse(text).unwrap();
        let v = from_rule_set(&set, Measure::Vertical, |_| vec![], &[0.0, 220.0]);
        assert_eq!(v.rules.len(), 1);
        assert_eq!(v.rules[0].voltages, vec![110.0, 220.0]);
        assert!(v.notes.iter().any(|n| n.contains("500")), "{:?}", v.notes);
        let back = to_rule_set(
            "w",
            &Zone::new(Measure::Vertical),
            &WindSteps::default(),
            &CheckScope::default(),
            &v.rules,
        );
        assert_eq!(back.rules.len(), 1);
        assert_eq!(back.rules[0].voltages, vec![110.0, 220.0]);
        assert_eq!(back.rules[0].vertical, set.rules[0].vertical);
        assert!(back.rules[0].checks_voltage(Some(220.0)));
        assert!(!back.rules[0].checks_voltage(None));
        assert_eq!(voltages_text(&[110.0, 220.0]), "110 or 220 kV");
        assert_eq!(voltages_text(&[]), "any voltage");
    }
}
