// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure checks: how much of its capacity each structure uses in each
//! load case (its type's `[capacity]`, see `ols_structures::structures`).
//!
//! - **Method 1, allowable spans**: the structure's wind span (half the sum
//!   of the adjacent spans) and weight span (the wires' vertical load at
//!   the attachments, without the insulators, over their vertical load per
//!   metre — the distance between the low points of the adjacent spans for
//!   a catenary; negative under uplift) of each conductor position, both
//!   sides summed: one attachment (a suspension point, a line end), or a
//!   dead-end's back and ahead strings — separate attachments of the
//!   structure joined by a jumper between two section ends there. The
//!   largest wind span and the largest and smallest weight span over the
//!   positions are taken against the type's allowable spans at the
//!   structure's line angle. Usage: the largest of wind span / allowable
//!   and weight span / allowable; a weight span below the minimum (uplift)
//!   is 100 % plus the shortfall over the allowable weight span; a line
//!   angle beyond the type's last one fails outright. The strength factor
//!   does not apply (allowable spans are the type's design spans).
//! - **Method 2, allowable loads** (load trees): at every attachment the
//!   type gives, the factored loads in the structure's axes (summed over a
//!   V-string's legs) against the allowable loads times the load case's
//!   strength factor; per direction, or added up with a linear
//!   interaction. Usage: the largest over the attachments.
//! - **Finite elements** (`fe`): the type's structure model (its members
//!   when it is a full design, or `model = "….ohlfe"`) solved with the factored attachment loads (and its
//!   members' weight times the vertical factor, and the wind on members
//!   whose section comes from the section library); usage: the member
//!   using most of its capacity times the strength factor
//!   ([`ols_structures_fe`]).
//! - **Pole** (`pole`): the type's pole (`pole = "id"`): the bending
//!   moment of the factored loads at the type's attachment points and of
//!   the wind on the pole, P-Delta amplified, against the strength factor
//!   times the moment capacity along the pole.
//!
//! A structure's usage is the largest of its checks; a structure without
//! a type or a type with nothing to check against is listed, not checked.
//! A structure whose loads in a case miss a wire that did not solve
//! ([`crate::CaseLoads::missing`]) is **incomplete** in that case: its
//! checks are computed from the loads there are (nothing made up) and
//! marked ([`Check::incomplete`], [`StructureUsage::missing`],
//! [`UsageStatus::Incomplete`]) — never a plain pass.
//!
//! A finite-element check whose tension-only members did not settle
//! (kept active as trusses, [`ols_structures_fe::Solution::kept_active`])
//! is **approximate** ([`Check::approximate`], [`UsageStatus::Approximate`]):
//! its usage is computed and shown, marked so — never a plain pass.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ols_line::Line;
use ols_structures::{Interaction, StructureLibrary};
use ols_weather::WeatherCase;

use crate::CaseLoads;

/// Which method a check is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Allowable spans (Method 1).
    Spans,
    /// Allowable loads at the attachments (Method 2).
    Loads,
    /// The type's finite-element model.
    Fe,
    /// The type's pole.
    Pole,
}

impl Method {
    /// Name in reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Spans => "spans",
            Method::Loads => "loads",
            Method::Fe => "fe",
            Method::Pole => "pole",
        }
    }
}

/// One check of a structure in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// Which method.
    pub method: Method,
    /// Share of the capacity used (1 = 100 %; infinite: not allowed at all,
    /// e.g. a line angle beyond the type's).
    pub usage: f64,
    /// What governs, in words (e.g. "weight span 640 m of 600 m").
    pub governs: String,
    /// Computed from incomplete loads: a section or jumper at the
    /// structure did not solve in this case ([`StructureUsage::missing`]),
    /// so the usage may be too low.
    pub incomplete: bool,
    /// Approximate: the finite-element solve kept tension-only members
    /// active because their set did not settle
    /// ([`ols_structures_fe::Solution::kept_active`]); to be treated later.
    pub approximate: bool,
}

/// What a structure's checks in one load case come to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageStatus {
    /// Nothing to check against (no type, no capacity).
    NotChecked,
    /// A wire at the structure did not solve: its loads are incomplete
    /// and the checks cannot pass (whatever their usage).
    Incomplete,
    /// A check is approximate ([`Check::approximate`]: tension-only
    /// members kept active because their set did not settle): not a plain
    /// pass (whatever its usage).
    Approximate,
    /// Every check within its capacity (100 % or less).
    Pass,
    /// A check over 100 %.
    Over,
}

impl UsageStatus {
    /// In words.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotChecked => "not checked",
            Self::Incomplete => "incomplete",
            Self::Approximate => "approximate",
            Self::Pass => "pass",
            Self::Over => "over",
        }
    }
}

/// A structure in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureUsage {
    /// Structure id.
    pub structure: String,
    /// Its type, if it has one.
    pub structure_type: Option<String>,
    /// Line angle, degrees (the largest of its routes, either way).
    pub line_angle_deg: f64,
    /// Wind span, m: the largest over the conductor positions.
    pub wind_span: f64,
    /// Weight span, m: smallest and largest over the conductor positions
    /// (none: no wire load found).
    pub weight_span: Option<[f64; 2]>,
    /// Method 1, when the type gives allowable spans.
    pub spans: Option<Check>,
    /// Method 2, when the type gives allowable loads.
    pub loads: Option<Check>,
    /// The finite-element check, when the type names a model.
    pub fe: Option<Check>,
    /// The pole check, when the type names a pole.
    pub pole: Option<Check>,
    /// Why something is not checked.
    pub notes: Vec<String>,
    /// The sections and jumpers at the structure that did not solve in
    /// this case: their forces are missing from its loads, and its checks
    /// are incomplete.
    pub missing: Vec<crate::MissingWire>,
}

impl StructureUsage {
    /// The governing check (the largest usage).
    pub fn governing(&self) -> Option<&Check> {
        [
            self.spans.as_ref(),
            self.loads.as_ref(),
            self.fe.as_ref(),
            self.pole.as_ref(),
        ]
        .into_iter()
        .flatten()
        .max_by(|a, b| a.usage.total_cmp(&b.usage))
    }

    /// Whether its loads miss a wire that did not solve.
    pub fn is_incomplete(&self) -> bool {
        !self.missing.is_empty()
    }

    /// Whether a check is approximate ([`Check::approximate`]).
    pub fn is_approximate(&self) -> bool {
        [&self.spans, &self.loads, &self.fe, &self.pole]
            .into_iter()
            .flatten()
            .any(|c| c.approximate)
    }

    /// What its checks come to: incomplete whenever a wire is missing
    /// (even with no check, or one over 100 %: the loads are not all
    /// there), else approximate when a check is, else not checked, over
    /// or pass.
    pub fn status(&self) -> UsageStatus {
        if self.is_incomplete() {
            return UsageStatus::Incomplete;
        }
        if self.is_approximate() {
            return UsageStatus::Approximate;
        }
        match self.governing() {
            None => UsageStatus::NotChecked,
            Some(g) if g.usage > 1.0 => UsageStatus::Over,
            Some(_) => UsageStatus::Pass,
        }
    }

    /// The missing wires in words: `incomplete: section S12 not solved
    /// (reason); …` (empty when complete).
    pub fn incomplete_text(&self) -> String {
        if self.missing.is_empty() {
            return String::new();
        }
        format!(
            "incomplete: {}",
            self.missing
                .iter()
                .map(crate::MissingWire::describe)
                .collect::<Vec<_>>()
                .join("; ")
        )
    }

    /// The approximate checks in words: `approximate: fe (tension-only
    /// members kept active, set did not settle)` (empty when none is).
    pub fn approximate_text(&self) -> String {
        if self.fe.as_ref().is_some_and(|c| c.approximate) {
            APPROXIMATE_FE.to_owned()
        } else {
            String::new()
        }
    }

    /// The missing wires, short: `not checked: S12 not solved` (empty when
    /// complete).
    pub fn not_checked_text(&self) -> String {
        if self.missing.is_empty() {
            return String::new();
        }
        format!(
            "not checked: {}",
            self.missing
                .iter()
                .map(crate::MissingWire::short)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// [`StructureUsage::approximate_text`] of an approximate finite-element
/// check.
pub const APPROXIMATE_FE: &str =
    "approximate: fe (tension-only members kept active, set did not settle)";

/// The structures in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseUsage {
    /// Load case name.
    pub case: String,
    /// Every structure with loads in this case.
    pub structures: Vec<StructureUsage>,
}

/// A wire end: (structure, set).
pub(crate) type Key = (String, u32);

/// The loads at one point of a structure in one load case.
#[derive(Default)]
struct AtPoint {
    /// Factored, structure axes, N.
    factored: [f64; 3],
    /// The unfactored vertical load (down), N.
    vertical: f64,
    /// The sections hanging there.
    sections: Vec<String>,
    /// The sets of the wires hanging there.
    sets: Vec<u32>,
}

/// Half the horizontal spans next to each wire end, summed over the
/// sections through it, m, by (structure, set).
fn wind_spans(line: &Line) -> BTreeMap<Key, f64> {
    let mut out: BTreeMap<Key, f64> = BTreeMap::new();
    for s in &line.sections {
        let at: Vec<Option<[f64; 3]>> = s
            .points
            .iter()
            .map(|p| line.wire_point(p).map(|w| w.position))
            .collect();
        let half = |a: Option<[f64; 3]>, b: Option<[f64; 3]>| match (a, b) {
            (Some(a), Some(b)) => 0.5 * (b[0] - a[0]).hypot(b[1] - a[1]),
            _ => 0.0,
        };
        for (k, p) in s.points.iter().enumerate() {
            let back = if k > 0 { half(at[k - 1], at[k]) } else { 0.0 };
            let ahead = at.get(k + 1).map_or(0.0, |&b| half(at[k], b));
            *out.entry((p.structure.clone(), p.set)).or_default() += back + ahead;
        }
    }
    out
}

/// The conductor position of each wire end: the wire ends of a structure
/// joined by a jumper at that structure whose two ends both end a section
/// (a dead-end's back and ahead strain strings), or hanging on the same
/// strings (a TAK), are one position, keyed by the smallest of them; every
/// other end is its own position. A jumper from a section's end to a point
/// in another section's middle (a tee-off to a suspension point) joins
/// nothing: the branch is another conductor position. Ends not listed are
/// their own position.
pub(crate) fn positions(line: &Line) -> BTreeMap<Key, Key> {
    let key = |p: &ols_line::SectionPoint| (p.structure.clone(), p.set);
    let ends: std::collections::BTreeSet<Key> = line
        .sections
        .iter()
        .flat_map(|s| [s.points.first(), s.points.last()])
        .flatten()
        .map(key)
        .collect();
    let mut out: BTreeMap<Key, Key> = BTreeMap::new();
    let root = |out: &BTreeMap<Key, Key>, mut k: Key| {
        while let Some(next) = out.get(&k).filter(|n| **n != k) {
            k = next.clone();
        }
        k
    };
    let join = |out: &mut BTreeMap<Key, Key>, a: Key, b: Key| {
        let (ra, rb) = (root(out, a), root(out, b));
        let (lo, hi) = if ra <= rb { (ra, rb) } else { (rb, ra) };
        out.insert(hi, lo.clone());
        out.entry(lo.clone()).or_insert(lo);
    };
    for j in &line.jumpers {
        let (Some(a), Some(b)) = (j.from.point().map(key), j.to.point().map(key)) else {
            continue;
        };
        if a.0 != b.0 || a == b || !ends.contains(&a) || !ends.contains(&b) {
            continue;
        }
        join(&mut out, a, b);
    }
    // Wire ends on the same strings.
    for s in &line.structures {
        for w in s.wires.iter().filter(|w| w.set.is_some()) {
            let sets: Vec<u32> = s
                .component(&w.label)
                .wires
                .iter()
                .filter_map(|&i| s.wires[i].set)
                .collect();
            for pair in sets.windows(2) {
                join(&mut out, (s.id.clone(), pair[0]), (s.id.clone(), pair[1]));
            }
        }
    }
    let keys: Vec<Key> = out.keys().cloned().collect();
    for k in keys {
        let r = root(&out, k.clone());
        out.insert(k, r);
    }
    out
}

/// The largest line angle of each structure over the routes through it,
/// degrees.
fn line_angles(line: &Line) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for r in line.alignment_views() {
        for s in r.stations(line).unwrap_or_default() {
            let a = out.entry(s.structure).or_default();
            *a = a.max(s.line_angle_deg.abs());
        }
    }
    out
}

/// Vertical load per metre of a section's whole bundle in `case`, N/m.
fn bundle_weight(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    section: &str,
) -> Option<f64> {
    let s = line.sections.iter().find(|s| s.id == section)?;
    let cable = library.get(&s.cable)?;
    let w = case.wire_load(cable.diameter, cable.weight()).vertical.si();
    Some(w * f64::from(s.bundle.max(1)))
}

/// How much of its capacity every structure uses in every load case of
/// `results` ([`crate::structure_loads`] of the same `line`, `library`
/// and `weather`). `types`: the structure types (their `[capacity]`,
/// models and poles; files they name are read once per call).
///
/// Expected cost: Methods 1 and 2 are instant; a model or pole check is a
/// finite-element solve per structure and load case (milliseconds).
pub fn usage(
    line: &Line,
    types: &StructureLibrary,
    library: &ols_cables::Library,
    weather: &[WeatherCase],
    results: &[CaseLoads],
) -> Vec<CaseUsage> {
    let none = ols_sections::SectionLibrary::default();
    usage_with_sections(line, types, &none, library, weather, results)
}

/// [`usage`], the members of the structure models taking their sections
/// from `sections` (the project's section library) first, and the wind on
/// those with a profile.
pub fn usage_with_sections(
    line: &Line,
    types: &StructureLibrary,
    sections: &ols_sections::SectionLibrary,
    library: &ols_cables::Library,
    weather: &[WeatherCase],
    results: &[CaseLoads],
) -> Vec<CaseUsage> {
    let wind = wind_spans(line);
    let position = positions(line);
    let angles = line_angles(line);
    let analyses = crate::analysis::Analyses::load(types, sections);
    results
        .iter()
        .map(|r| {
            let case = weather.iter().find(|w| w.name == r.case.weather);
            // Per point of a structure: the factored loads and the
            // wires' unfactored vertical load (the wind on the structure
            // body is at no point).
            let mut at: BTreeMap<(String, String), AtPoint> = BTreeMap::new();
            for l in r.loads.iter().filter(|l| !l.point.is_empty()) {
                let e = at
                    .entry((l.structure.clone(), l.point.clone()))
                    .or_default();
                for (sum, f) in e.factored.iter_mut().zip(l.factored) {
                    *sum += f;
                }
                e.vertical += l.local[2];
                for s in &l.sections {
                    if !e.sections.contains(s) {
                        e.sections.push(s.clone());
                    }
                }
                for set in &l.sets {
                    if !e.sets.contains(set) {
                        e.sets.push(*set);
                    }
                }
            }
            type ByPoint<'a> = Vec<(&'a String, &'a AtPoint)>;
            let mut by_structure: BTreeMap<&str, ByPoint> = BTreeMap::new();
            for ((structure, label), v) in &at {
                by_structure
                    .entry(structure.as_str())
                    .or_default()
                    .push((label, v));
            }
            // A structure every wire of which did not solve has no load
            // at all: listed (incomplete) all the same.
            for m in &r.missing {
                by_structure.entry(m.structure.as_str()).or_default();
            }
            let structures = by_structure
                .into_iter()
                .map(|(id, points)| {
                    let structure = line.structure(id);
                    let type_id = structure.and_then(|s| s.structure_type.clone());
                    let type_def = type_id.as_deref().and_then(|t| types.get(t));
                    let capacity = type_def.and_then(|t| t.capacity.as_ref());
                    let analysed = type_def.is_some_and(|t| {
                        t.members_designed()
                            || t.model.is_some()
                            || t.pole_detail() == Some(ols_structures::Detail::Design)
                    });
                    let extension = structure.and_then(|s| s.extension.as_deref());
                    let line_angle_deg = angles.get(id).copied().unwrap_or(0.0);
                    // Per conductor position (a suspension point, or a
                    // dead-end's back and ahead strings together): its wire
                    // ends, the points they hang on and the strings between.
                    let mut ends: BTreeMap<Key, Vec<u32>> = BTreeMap::new();
                    for (_, p) in &points {
                        for set in &p.sets {
                            let k = (id.to_owned(), *set);
                            let pos = position.get(&k).cloned().unwrap_or(k);
                            let e = ends.entry(pos).or_default();
                            if !e.contains(set) {
                                e.push(*set);
                            }
                        }
                    }
                    // The wind span (summed over its ends) and the weight
                    // span: the wires' vertical load at its points, without
                    // the strings, over their vertical load per metre.
                    let mut by_position: BTreeMap<&Key, (f64, Option<f64>)> = BTreeMap::new();
                    for (pos, sets) in &ends {
                        let e = by_position.entry(pos).or_default();
                        for set in sets {
                            e.0 += wind.get(&(id.to_owned(), *set)).copied().unwrap_or(0.0);
                        }
                        let here: Vec<&AtPoint> = points
                            .iter()
                            .filter(|(_, p)| p.sets.iter().any(|s| sets.contains(s)))
                            .map(|(_, p)| *p)
                            .collect();
                        let vertical: f64 = here.iter().map(|p| p.vertical).sum();
                        let strings: f64 = structure.map_or(0.0, |s| {
                            let mut seen = std::collections::BTreeSet::new();
                            for set in sets {
                                if let Some(w) = s.wire_at_set(*set) {
                                    seen.extend(s.component(&w.label).strings);
                                }
                            }
                            seen.iter().map(|&i| s.strings[i].weight).sum()
                        });
                        let ws: Vec<f64> = here
                            .iter()
                            .flat_map(|p| &p.sections)
                            .filter_map(|s| bundle_weight(line, library, case?, s))
                            .collect();
                        let w = ws.iter().sum::<f64>() / ws.len().max(1) as f64;
                        if w > 0.0 {
                            e.1 = Some((vertical - strings) / w);
                        }
                    }
                    let wind_span = by_position.values().map(|v| v.0).fold(0.0, f64::max);
                    let spans: Vec<f64> = by_position.values().filter_map(|v| v.1).collect();
                    let weight_span = (!spans.is_empty()).then(|| {
                        [
                            spans.iter().copied().fold(f64::INFINITY, f64::min),
                            spans.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        ]
                    });
                    let mut notes = Vec::new();
                    match (&type_id, capacity) {
                        (None, _) => notes.push("no structure type: not checked".into()),
                        (Some(t), None) if !analysed => {
                            notes.push(format!("type {t} gives no capacity"));
                        }
                        _ => {}
                    }
                    let spans_check = capacity.filter(|c| !c.spans.is_empty()).map(|c| {
                        match c.spans_at(line_angle_deg) {
                            None => Check {
                                method: Method::Spans,
                                usage: f64::INFINITY,
                                governs: format!(
                                    "line angle {line_angle_deg:.1}° beyond the type's {:.1}°",
                                    c.max_angle().unwrap_or(0.0)
                                ),
                                incomplete: false,
                                approximate: false,
                            },
                            Some(allowed) => {
                                let mut worst = (
                                    wind_span / allowed.wind_span,
                                    format!(
                                        "wind span {wind_span:.0} m of {:.0} m",
                                        allowed.wind_span
                                    ),
                                );
                                if let Some([lo, hi]) = weight_span {
                                    let r = hi / allowed.weight_span;
                                    if r > worst.0 {
                                        worst = (
                                            r,
                                            format!(
                                                "weight span {hi:.0} m of {:.0} m",
                                                allowed.weight_span
                                            ),
                                        );
                                    }
                                    if let Some(min) = allowed.min_weight_span
                                        && lo < min
                                    {
                                        let r = 1.0 + (min - lo) / allowed.weight_span;
                                        if r > worst.0 {
                                            worst = (
                                                r,
                                                format!(
                                                    "weight span {lo:.0} m below the minimum \
                                                     {min:.0} m (uplift)"
                                                ),
                                            );
                                        }
                                    }
                                }
                                Check {
                                    method: Method::Spans,
                                    usage: worst.0,
                                    governs: worst.1,
                                    incomplete: false,
                                    approximate: false,
                                }
                            }
                        }
                    });
                    let loads_check = capacity.filter(|c| !c.attachments.is_empty()).map(|_| {
                        let phi = r.case.strength;
                        let mut worst = (0.0, String::new());
                        for (label, p) in &points {
                            let Some(cap) = type_def.and_then(|t| t.capacity_at(label)) else {
                                notes.push(format!("point {label} not in the type's load tree"));
                                continue;
                            };
                            let [t, l, v] = p.factored;
                            // (ratio, direction, load, allowable), kN in words.
                            let mut ratios: Vec<(f64, &str, f64, f64)> = Vec::new();
                            if let Some(ct) = cap.transverse {
                                ratios.push((t.abs() / (phi * ct.si()), "transverse", t, ct.si()));
                            }
                            if let Some(cl) = cap.longitudinal {
                                ratios.push((
                                    l.abs() / (phi * cl.si()),
                                    "longitudinal",
                                    l,
                                    cl.si(),
                                ));
                            }
                            if v >= 0.0 {
                                if let Some(cv) = cap.vertical {
                                    ratios.push((v / (phi * cv.si()), "vertical", v, cv.si()));
                                }
                            } else if let Some(cu) = cap.uplift {
                                ratios.push((-v / (phi * cu.si()), "uplift", v, cu.si()));
                            } else if cap.vertical.is_some() {
                                ratios.push((f64::INFINITY, "uplift (none allowed)", v, 0.0));
                            }
                            let (usage, why) = match cap.interaction {
                                Interaction::Linear => (
                                    ratios.iter().map(|r| r.0).sum::<f64>(),
                                    format!(
                                        "{label} combined {}",
                                        ratios
                                            .iter()
                                            .map(|r| format!("{} {:.0} %", r.1, r.0 * 100.0))
                                            .collect::<Vec<_>>()
                                            .join(" + ")
                                    ),
                                ),
                                Interaction::None => ratios
                                    .iter()
                                    .max_by(|a, b| a.0.total_cmp(&b.0))
                                    .map_or((0.0, String::new()), |r| {
                                        (
                                            r.0,
                                            format!(
                                                "{label} {} {:.1} kN of {:.1} kN × {phi}",
                                                r.1,
                                                r.2.abs() / 1000.0,
                                                r.3 / 1000.0
                                            ),
                                        )
                                    }),
                            };
                            if usage > worst.0 || worst.1.is_empty() {
                                worst = (usage, why);
                            }
                        }
                        Check {
                            method: Method::Loads,
                            usage: worst.0,
                            governs: worst.1,
                            incomplete: false,
                            approximate: false,
                        }
                    });
                    let factored: Vec<crate::analysis::Factored> = points
                        .iter()
                        .map(|(label, p)| ((*label).clone(), p.factored))
                        .collect();
                    let fe = type_def.and_then(|t| {
                        analyses.fe(t, extension, &factored, &r.case, case, &mut notes)
                    });
                    let pole = type_def
                        .and_then(|t| analyses.pole(t, &factored, &r.case, case, &mut notes));
                    let missing: Vec<crate::MissingWire> = r.missing_at(id).cloned().collect();
                    let mark = |c: Option<Check>| {
                        c.map(|mut c| {
                            c.incomplete = !missing.is_empty();
                            c
                        })
                    };
                    let (spans_check, loads_check, fe, pole) =
                        (mark(spans_check), mark(loads_check), mark(fe), mark(pole));
                    StructureUsage {
                        structure: id.to_owned(),
                        structure_type: type_id,
                        line_angle_deg,
                        wind_span,
                        weight_span,
                        spans: spans_check,
                        loads: loads_check,
                        fe,
                        pole,
                        notes,
                        missing,
                    }
                })
                .collect();
            CaseUsage {
                case: r.case.name.clone(),
                structures,
            }
        })
        .collect()
}

/// The governing load case of every structure: the one using most of its
/// capacity (structures not checked left out), worst first.
pub fn governing(cases: &[CaseUsage]) -> Vec<(&str, &StructureUsage)> {
    let mut best: BTreeMap<&str, (&str, &StructureUsage, f64)> = BTreeMap::new();
    for c in cases {
        for s in &c.structures {
            let Some(g) = s.governing() else { continue };
            let e = best
                .entry(s.structure.as_str())
                .or_insert((c.case.as_str(), s, g.usage));
            if g.usage > e.2 {
                *e = (c.case.as_str(), s, g.usage);
            }
        }
    }
    let mut out: Vec<(&str, &StructureUsage, f64)> = best.into_values().collect();
    out.sort_by(|a, b| b.2.total_cmp(&a.2));
    out.into_iter().map(|(c, s, _)| (c, s)).collect()
}

/// The load cases in which structure `structure` is incomplete (a wire at
/// it did not solve), with the missing wires, in the cases' order.
pub fn incomplete_cases<'a>(
    cases: &'a [CaseUsage],
    structure: &str,
) -> Vec<(&'a str, &'a StructureUsage)> {
    cases
        .iter()
        .filter_map(|c| {
            c.structures
                .iter()
                .find(|s| s.structure == structure && s.is_incomplete())
                .map(|s| (c.case.as_str(), s))
        })
        .collect()
}

/// The usage as CSV, one row per load case and structure:
/// `case,structure,type,line_angle_deg,wind_span,weight_span_min,
/// weight_span_max,spans_usage,loads_usage,fe_usage,pole_usage,usage,
/// governs,notes,incomplete`
/// (usages in %, empty when not checked; `incomplete`: the wires missing
/// from the structure's loads in that case, `incomplete: section S12 not
/// solved (reason); …`, empty when its loads are complete — the usages
/// are then those of the loads there are; an approximate check adds
/// [`APPROXIMATE_FE`]).
pub fn usage_csv(cases: &[CaseUsage]) -> String {
    use ols_core::text::csv_field;
    let mut out = String::from(
        "case,structure,type,line_angle_deg,wind_span,weight_span_min,weight_span_max,\
         spans_usage,loads_usage,fe_usage,pole_usage,usage,governs,notes,incomplete\n",
    );
    let pct = |c: &Option<Check>| {
        c.as_ref()
            .map_or(String::new(), |c| format!("{:.1}", c.usage * 100.0))
    };
    for c in cases {
        for s in &c.structures {
            let g = s.governing();
            let (lo, hi) = s
                .weight_span
                .map_or((String::new(), String::new()), |[a, b]| {
                    (format!("{a:.1}"), format!("{b:.1}"))
                });
            let _ = writeln!(
                out,
                "{},{},{},{:.2},{:.1},{lo},{hi},{},{},{},{},{},{},{},{}",
                csv_field(&c.case),
                csv_field(&s.structure),
                csv_field(s.structure_type.as_deref().unwrap_or("")),
                s.line_angle_deg,
                s.wind_span,
                pct(&s.spans),
                pct(&s.loads),
                pct(&s.fe),
                pct(&s.pole),
                g.map_or(String::new(), |g| format!("{:.1}", g.usage * 100.0)),
                csv_field(g.map_or("", |g| g.governs.as_str())),
                csv_field(&s.notes.join("; ")),
                csv_field(
                    &[s.incomplete_text(), s.approximate_text()]
                        .into_iter()
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            );
        }
    }
    out
}
