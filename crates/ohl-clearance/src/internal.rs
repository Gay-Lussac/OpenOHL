// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Internal clearances** (roadmap O6, first part; owner, 2026-10-25):
//! the electrified parts at each structure against the structure's own
//! body, as solved in each weather case — `ohl internal` (and the desktop
//! application's window, written apart).
//!
//! What is measured, per structure at detail level 2 or 3 (a body:
//! members, a pole):
//!
//! - **the conductors near it**: each span's wire from its clamp at the
//!   structure out to the rule's `reach` (default 20 m, measured in plan
//!   along the solved wire), as solved in the case (the strings' swing is
//!   in the solved shape); a bundle subconductor by subconductor;
//! - **the jumpers at it** (a jumper end on one of its wire points), as
//!   solved;
//! - **the live end of each string**: its wire point as solved (the
//!   string's bottom, not in still air).
//!
//! Each against **every member of the body**: to the member's **surface**
//! where it is known (its solid's triangles: a member whose section has a
//! profile in the section library, a pole's outline); else to its centre
//! line, which leaves out the member's size — **on the unsafe side**, said
//! in the notes per structure and on every row ([`InternalRow::to_surface`]).
//! Volumes on the strings (spheres, cylinders and the like: the rest of
//! O6) are not checked yet.
//!
//! Distances are exact on the solved polylines (segment to segment,
//! segment to triangle: [`crate::geometry`]), to the wire's **surface**:
//! the subconductor's radius in the case (iced with ice) and the bound of
//! how far the wire lies off its polyline
//! ([`ohl_line::solve::polyline_error`]) are taken off — a distance is
//! never more than the exact one (conservative). A string's wire point is
//! a point (no fitting's size counted: on the unsafe side, the volumes of
//! the next part will carry it).
//!
//! Earthed wires (earth wires: no potential) are not electrified and not
//! checked. A structure without a body (level 1) is listed as not checked.
//!
//! Rules ([`InternalRule`], `[[internal_rule]]` tables of a rule file
//! `.ohlr`) give the weather cases, the voltages, the required distance (a
//! distance or by voltage, as the clearance rules) and which parts. One
//! row per rule, structure, case and part: its smallest distance to the
//! body, the nearest member and the required distance; the worst first.

use std::collections::BTreeSet;

use ohl_line::Line;
use ohl_line::jumper::JumperShape;
use ohl_line::solve::SectionShape;

use crate::geometry::{P3, closest_between_segments, closest_segment_triangle, distance as dist};
use crate::rules::Requirement;

/// The reach along a span from its clamp when a rule gives none, m (owner,
/// 2026-10-25).
pub const DEFAULT_REACH: f64 = 20.0;

/// One rule of the internal clearance check (`[[internal_rule]]`).
#[derive(Debug, Clone, PartialEq)]
pub struct InternalRule {
    /// Name, for reports.
    pub name: String,
    /// Weather cases (names, `*` `?` wildcards); empty: every case.
    pub cases: Vec<String>,
    /// Nominal voltages of the parts checked, kV; empty: every
    /// electrified part (as the clearance windows' rules).
    pub voltages: Vec<f64>,
    /// The distance to keep: fixed, or by the part's nominal voltage.
    pub distance: Requirement,
    /// Check the conductors near the structure.
    pub conductors: bool,
    /// Check the jumpers at the structure.
    pub jumpers: bool,
    /// Check the strings' wire points.
    pub string_ends: bool,
    /// How far along each span from its clamp the conductor is checked
    /// (in plan), m.
    pub reach: f64,
}

impl InternalRule {
    /// A rule for every part in every case, required distance 0 m (to be
    /// set), reach [`DEFAULT_REACH`].
    pub fn blank(name: &str) -> Self {
        Self {
            name: name.into(),
            cases: Vec::new(),
            voltages: Vec::new(),
            distance: Requirement::Fixed(0.0),
            conductors: true,
            jumpers: true,
            string_ends: true,
            reach: DEFAULT_REACH,
        }
    }

    /// What is wrong with it (none: nothing).
    pub fn problem(&self) -> Option<String> {
        if !(self.reach.is_finite() && self.reach > 0.0) {
            return Some(format!("\"{}\": reach must be above 0 m", self.name));
        }
        if !self.conductors && !self.jumpers && !self.string_ends {
            return Some(format!("\"{}\": checks nothing", self.name));
        }
        None
    }

    /// Whether it applies to weather case `case`.
    pub fn applies(&self, case: &str) -> bool {
        self.cases.is_empty()
            || self
                .cases
                .iter()
                .any(|c| ohl_core::pattern::matches(c, case))
    }

    /// Whether it takes a part of nominal voltage `kv`.
    pub fn takes_voltage(&self, kv: f64) -> bool {
        self.voltages.is_empty() || self.voltages.iter().any(|v| (v - kv).abs() < 1e-6)
    }

    /// The parts it checks, in words: "conductors (20 m), jumpers, string
    /// ends".
    pub fn describe_parts(&self) -> String {
        let mut parts = Vec::new();
        if self.conductors {
            parts.push(format!("conductors ({} m)", self.reach));
        }
        if self.jumpers {
            parts.push("jumpers".to_owned());
        }
        if self.string_ends {
            parts.push("string ends".to_owned());
        }
        parts.join(", ")
    }

    /// The rule as an `[[internal_rule]]` table of a rule file.
    pub fn to_toml(&self) -> String {
        use std::fmt::Write;
        let q = |t: &str| toml::Value::String(t.to_owned()).to_string();
        let mut t = format!("\n[[internal_rule]]\nname = {}\n", q(&self.name));
        if !self.cases.is_empty() {
            let list: Vec<String> = self.cases.iter().map(|c| q(c)).collect();
            let _ = writeln!(t, "cases = [{}]", list.join(", "));
        }
        if !self.voltages.is_empty() {
            let list: Vec<String> = self.voltages.iter().map(|v| format!("{v:?}")).collect();
            let _ = writeln!(t, "voltages = [{}]", list.join(", "));
        }
        let _ = writeln!(t, "distance = {}", self.distance.to_toml());
        for (k, v) in [
            ("conductors", self.conductors),
            ("jumpers", self.jumpers),
            ("string_ends", self.string_ends),
        ] {
            if !v {
                let _ = writeln!(t, "{k} = false");
            }
        }
        if self.reach != DEFAULT_REACH {
            let _ = writeln!(t, "reach = \"{} m\"", self.reach);
        }
        t
    }
}

fn yes() -> bool {
    true
}

/// An `[[internal_rule]]` table as read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawInternalRule {
    name: String,
    #[serde(default)]
    cases: Vec<String>,
    #[serde(default)]
    voltages: Vec<f64>,
    distance: crate::rules::RawRequirement,
    #[serde(default = "yes")]
    conductors: bool,
    #[serde(default = "yes")]
    jumpers: bool,
    #[serde(default = "yes")]
    string_ends: bool,
    #[serde(default)]
    reach: Option<ohl_core::units::Length>,
}

impl RawInternalRule {
    pub(crate) fn resolve(self) -> Result<InternalRule, String> {
        let rule = InternalRule {
            distance: self
                .distance
                .resolve()
                .map_err(|e| format!("internal rule \"{}\": distance: {e}", self.name))?,
            name: self.name,
            cases: self.cases,
            voltages: self.voltages,
            conductors: self.conductors,
            jumpers: self.jumpers,
            string_ends: self.string_ends,
            reach: self.reach.map_or(DEFAULT_REACH, |l| l.si()),
        };
        match rule.problem() {
            Some(e) => Err(format!("internal rule {e}")),
            None => Ok(rule),
        }
    }
}

/// A member of a structure's body, in project coordinates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BodyMember {
    /// The member's id (none: a body polyline, a pole).
    pub id: Option<String>,
    /// Its centre line (a polyline), m.
    pub line: Vec<[f64; 3]>,
    /// Its solid's triangles, m; empty: its size is not known (measured to
    /// its centre line).
    pub solid: Vec<[[f64; 3]; 3]>,
}

/// A structure's body, in project coordinates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructureBody {
    /// The structure's id.
    pub structure: String,
    /// Its members (a pole as one member).
    pub members: Vec<BodyMember>,
}

/// An electrified part measured.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    /// A span's conductor near the structure: the section and the span's
    /// index in it (from 0).
    Conductor {
        /// The section.
        section: String,
        /// The span's index in the section, from 0.
        span: usize,
    },
    /// A jumper (its id).
    Jumper(String),
    /// A string's wire point, by the string's label on the structure.
    StringEnd {
        /// The string's label.
        label: String,
    },
}

impl std::fmt::Display for Part {
    /// "S1 span 2", "jumper J1", "string A1 end".
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conductor { section, span } => write!(f, "{section} span {}", span + 1),
            Self::Jumper(j) => write!(f, "jumper {j}"),
            Self::StringEnd { label } => write!(f, "string {label} end"),
        }
    }
}

/// One part at one structure in one case, under one rule.
#[derive(Debug, Clone, PartialEq)]
pub struct InternalRow {
    /// Index of the rule.
    pub rule: usize,
    /// The weather case.
    pub case: String,
    /// The structure.
    pub structure: String,
    /// The part.
    pub part: Part,
    /// The nearest member's id (none: a body polyline or the pole).
    pub member: Option<String>,
    /// The smallest distance of the part's surface to the body, m
    /// (negative: into it).
    pub distance: f64,
    /// The required distance, m.
    pub required: f64,
    /// Whether it is to the nearest member's surface (else to its centre
    /// line: its size not counted, on the unsafe side).
    pub to_surface: bool,
    /// The point of the part (its axis) nearest the body, m.
    pub at: [f64; 3],
    /// The point of the member nearest it, m.
    pub on_member: [f64; 3],
}

impl InternalRow {
    /// Whether the distance is at least the required one.
    pub fn pass(&self) -> bool {
        self.distance >= self.required
    }
}

/// The result of a check.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InternalReport {
    /// Every part at every structure in every case of every rule, the
    /// smallest margin (distance − required) first.
    pub rows: Vec<InternalRow>,
    /// Structures not checked, and why.
    pub not_checked: Vec<(String, String)>,
    /// What else to know (members to their centre lines, voltages a rule
    /// has no distance for, rules not checked).
    pub notes: Vec<String>,
}

impl InternalReport {
    /// The rows below their required distance.
    pub fn failures(&self) -> impl Iterator<Item = &InternalRow> {
        self.rows.iter().filter(|r| !r.pass())
    }

    /// The results as CSV.
    pub fn csv(&self, rules: &[InternalRule]) -> String {
        use std::fmt::Write;
        let q = ohl_core::text::csv_field;
        let mut out = String::from(
            "rule,case,structure,part,member,distance_m,required_m,margin_m,to,status,x,y,z,member_x,member_y,member_z\n",
        );
        for r in &self.rows {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{:.3},{:.3},{:.3},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
                q(&rules[r.rule].name),
                q(&r.case),
                q(&r.structure),
                q(&r.part.to_string()),
                q(r.member.as_deref().unwrap_or("")),
                r.distance,
                r.required,
                r.distance - r.required,
                if r.to_surface {
                    "surface"
                } else {
                    "centre line"
                },
                if r.pass() { "pass" } else { "fail" },
                r.at[0],
                r.at[1],
                r.at[2],
                r.on_member[0],
                r.on_member[1],
                r.on_member[2],
            );
        }
        out
    }
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy)]
struct Aabb {
    lo: P3,
    hi: P3,
}

impl Aabb {
    fn empty() -> Self {
        Self {
            lo: [f64::INFINITY; 3],
            hi: [f64::NEG_INFINITY; 3],
        }
    }

    fn add(&mut self, p: P3) {
        for (k, v) in p.into_iter().enumerate() {
            self.lo[k] = self.lo[k].min(v);
            self.hi[k] = self.hi[k].max(v);
        }
    }

    fn of(points: impl IntoIterator<Item = P3>) -> Self {
        let mut b = Self::empty();
        for p in points {
            b.add(p);
        }
        b
    }

    /// The smallest distance between two boxes (0 when they overlap).
    fn gap(&self, o: &Aabb) -> f64 {
        let mut s = 0.0;
        for k in 0..3 {
            let g = (self.lo[k] - o.hi[k]).max(o.lo[k] - self.hi[k]).max(0.0);
            s += g * g;
        }
        s.sqrt()
    }
}

/// A member ready to measure: its box, and each triangle's or segment's.
struct Prepared<'a> {
    member: &'a BodyMember,
    bounds: Aabb,
    triangles: Vec<Aabb>,
    segments: Vec<Aabb>,
}

fn prepare(body: &StructureBody) -> Vec<Prepared<'_>> {
    body.members
        .iter()
        .filter(|m| !m.line.is_empty() || !m.solid.is_empty())
        .map(|m| {
            let triangles: Vec<Aabb> = m.solid.iter().map(|t| Aabb::of(*t)).collect();
            let segments: Vec<Aabb> = if m.line.len() == 1 {
                vec![Aabb::of([m.line[0]])]
            } else {
                m.line.windows(2).map(|w| Aabb::of([w[0], w[1]])).collect()
            };
            let bounds = if m.solid.is_empty() {
                Aabb::of(m.line.iter().copied())
            } else {
                Aabb::of(m.solid.iter().flatten().copied())
            };
            Prepared {
                member: m,
                bounds,
                triangles,
                segments,
            }
        })
        .collect()
}

/// The nearest approach of the polylines `wires` to the body `members`:
/// (distance between the axes, the point on the wire, the point on the
/// member, the member's index). A polyline of one point is that point.
fn nearest(wires: &[Vec<P3>], members: &[Prepared]) -> Option<(f64, P3, P3, usize)> {
    let mut best: Option<(f64, P3, P3, usize)> = None;
    for w in wires {
        if w.is_empty() {
            continue;
        }
        let segs: Vec<(P3, P3, Aabb)> = if w.len() == 1 {
            vec![(w[0], w[0], Aabb::of([w[0]]))]
        } else {
            w.windows(2)
                .map(|s| (s[0], s[1], Aabb::of([s[0], s[1]])))
                .collect()
        };
        let wb = Aabb::of(w.iter().copied());
        // Members by the gap of their boxes: none beyond the best so far
        // can be nearer.
        let mut order: Vec<(f64, usize)> = members
            .iter()
            .enumerate()
            .map(|(k, m)| (m.bounds.gap(&wb), k))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (gap, k) in order {
            let limit = best.map_or(f64::INFINITY, |b| b.0);
            if gap >= limit {
                break;
            }
            let m = &members[k];
            for (p, q, sb) in &segs {
                let limit = best.map_or(f64::INFINITY, |b| b.0);
                if sb.gap(&m.bounds) >= limit {
                    continue;
                }
                let keep = |best: &mut Option<(f64, P3, P3, usize)>, a: P3, b: P3| {
                    let d = dist(a, b);
                    if best.is_none_or(|x| d < x.0) {
                        *best = Some((d, a, b, k));
                    }
                };
                if m.member.solid.is_empty() {
                    let line = &m.member.line;
                    if line.len() == 1 {
                        let (a, b) = closest_between_segments(*p, *q, line[0], line[0]);
                        keep(&mut best, a, b);
                    }
                    for (i, s) in line.windows(2).enumerate() {
                        if sb.gap(&m.segments[i]) >= best.map_or(f64::INFINITY, |b| b.0) {
                            continue;
                        }
                        let (a, b) = closest_between_segments(*p, *q, s[0], s[1]);
                        keep(&mut best, a, b);
                    }
                } else {
                    // An end inside the (closed) solid: into it, 0.
                    for e in [*p, *q] {
                        if m.bounds.gap(&Aabb::of([e])) == 0.0 && inside(e, &m.member.solid) {
                            keep(&mut best, e, e);
                        }
                    }
                    for (i, t) in m.member.solid.iter().enumerate() {
                        if sb.gap(&m.triangles[i]) >= best.map_or(f64::INFINITY, |b| b.0) {
                            continue;
                        }
                        let (a, b) = closest_segment_triangle(*p, *q, t[0], t[1], t[2]);
                        keep(&mut best, a, b);
                    }
                }
            }
        }
    }
    best
}

/// Whether `p` is inside the closed triangle mesh `solid` (a ray's
/// crossings counted: odd inside; a hollow member's inner faces count as
/// well, so its hole is outside).
fn inside(p: P3, solid: &[[[f64; 3]; 3]]) -> bool {
    // A direction unlikely to graze an edge of a drawn profile.
    let far = 1e4;
    let q = [
        p[0] + far * 0.573_576_436,
        p[1] + far * 0.469_471_563,
        p[2] + far * 0.671_558_955,
    ];
    solid
        .iter()
        .filter(|t| crate::geometry::segment_through_triangle(p, q, t[0], t[1], t[2]).is_some())
        .count()
        % 2
        == 1
}

/// The part of polyline `points` within `reach` (in plan, along it) of its
/// start (`from_start`) or of its end, in its own order.
fn near_end(points: &[P3], reach: f64, from_start: bool) -> Vec<P3> {
    let mut pts: Vec<P3> = points.to_vec();
    if !from_start {
        pts.reverse();
    }
    let mut out: Vec<P3> = Vec::new();
    let mut run = 0.0;
    for (k, p) in pts.iter().enumerate() {
        if k == 0 {
            out.push(*p);
            continue;
        }
        let a = pts[k - 1];
        let h = (p[0] - a[0]).hypot(p[1] - a[1]);
        if run + h >= reach {
            let t = if h > 0.0 { (reach - run) / h } else { 1.0 };
            out.push([0, 1, 2].map(|i| a[i] + (p[i] - a[i]) * t));
            break;
        }
        run += h;
        out.push(*p);
    }
    if !from_start {
        out.reverse();
    }
    out
}

/// One part to measure at a structure: what it is, its section (for the
/// overrides), its voltage, its polylines (subconductors) and what is
/// taken off the distance between the axes (radius and polyline bound).
struct Piece {
    part: Part,
    kv: f64,
    wires: Vec<Vec<P3>>,
    allowance: f64,
}

/// Checks the electrified parts at each structure of `line` with a body in
/// `bodies` against that body, in each weather case of `solved` (its name,
/// the line's sections and jumpers solved in it) that `rules` apply to and
/// `filter` lets through at the structure (and the part's section).
///
/// Expected cost: per structure and case, the parts near it against its
/// members, pruned by boxes — milliseconds per structure; one budget step
/// per structure in each case and rule (`budget`: the exit clause).
pub fn check(
    line: &Line,
    solved: &[(&str, &[SectionShape], &[JumperShape])],
    bodies: &[StructureBody],
    rules: &[InternalRule],
    filter: &ohl_weather::CaseFilter,
    budget: &ohl_core::budget::Budget,
) -> Result<InternalReport, String> {
    let mut report = InternalReport::default();
    let potentials = line.potentials().map_err(|e| e.to_string())?;
    let kv_of = |section: &str| -> Option<f64> {
        potentials
            .get(section)
            .copied()
            .flatten()
            .map(|p| p.nominal_kv)
    };
    // The structures with a body, in the line's order.
    let mut checked: Vec<(&str, Vec<Prepared>)> = Vec::new();
    for st in &line.structures {
        match bodies.iter().find(|b| b.structure == st.id) {
            None => report
                .not_checked
                .push((st.id.clone(), "level 1: no body".into())),
            Some(b) => {
                let prepared = prepare(b);
                if prepared.is_empty() {
                    report
                        .not_checked
                        .push((st.id.clone(), "its body has no member".into()));
                    continue;
                }
                let lines = prepared
                    .iter()
                    .filter(|m| m.member.solid.is_empty())
                    .count();
                if lines > 0 {
                    report.notes.push(format!(
                        "{}: {lines} of {} members to the members' centre lines: their size not counted — on the unsafe side",
                        st.id,
                        prepared.len()
                    ));
                }
                checked.push((st.id.as_str(), prepared));
            }
        }
    }
    let mut run = budget.start();
    for (k, rule) in rules.iter().enumerate() {
        if let Some(e) = rule.problem() {
            report.notes.push(format!("{e}: not checked"));
            continue;
        }
        let mut any = false;
        let mut no_distance: BTreeSet<String> = BTreeSet::new();
        for (case, shapes, jumpers) in solved {
            if !rule.applies(case) {
                continue;
            }
            any = true;
            for (structure, members) in &checked {
                run.step()
                    .map_err(|e| format!("internal clearances: {e}"))?;
                let pieces =
                    pieces_at(line, structure, case, shapes, jumpers, rule, filter, &kv_of);
                for piece in pieces {
                    let required = match rule.distance.at(piece.kv) {
                        Ok(d) => d,
                        Err(e) => {
                            no_distance.insert(e);
                            continue;
                        }
                    };
                    let Some((d, at, on, m)) = nearest(&piece.wires, members) else {
                        continue;
                    };
                    let member = members[m].member;
                    report.rows.push(InternalRow {
                        rule: k,
                        case: (*case).to_owned(),
                        structure: (*structure).to_owned(),
                        part: piece.part,
                        member: member.id.clone(),
                        distance: d - piece.allowance,
                        required,
                        to_surface: !member.solid.is_empty(),
                        at,
                        on_member: on,
                    });
                }
            }
        }
        if !any {
            report.notes.push(format!(
                "\"{}\": none of its weather cases is solved",
                rule.name
            ));
        }
        for e in no_distance {
            report
                .notes
                .push(format!("\"{}\": {e}: those parts not checked", rule.name));
        }
    }
    report.rows.sort_by(|a, b| {
        (a.distance - a.required)
            .total_cmp(&(b.distance - b.required))
            .then_with(|| (&a.structure, &a.case, &a.part).cmp(&(&b.structure, &b.case, &b.part)))
    });
    Ok(report)
}

/// The parts at `structure` in `case` that `rule` checks and `filter`
/// takes.
#[allow(clippy::too_many_arguments)]
fn pieces_at(
    line: &Line,
    structure: &str,
    case: &str,
    shapes: &[SectionShape],
    jumpers: &[JumperShape],
    rule: &InternalRule,
    filter: &ohl_weather::CaseFilter,
    kv_of: &dyn Fn(&str) -> Option<f64>,
) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut labels: BTreeSet<String> = BTreeSet::new();
    for shape in shapes {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        // Electrified sections only, of the rule's voltages.
        let Some(kv) = kv_of(&section.id).filter(|kv| rule.takes_voltage(*kv)) else {
            continue;
        };
        if !filter.takes(Some(structure), Some(&section.id), case) {
            continue;
        }
        for (p, sp) in section.points.iter().enumerate() {
            if sp.structure != structure {
                continue;
            }
            if rule.conductors {
                // The span ending here (back) and the one starting here.
                let near = [
                    (p.checked_sub(1), false),
                    ((p + 1 < section.points.len()).then_some(p), true),
                ];
                for (span, from_start) in near {
                    let Some(span) = span else { continue };
                    let Some(s) = shape.spans.get(span) else {
                        continue;
                    };
                    let piece = near_end(&s.points, rule.reach, from_start);
                    if piece.is_empty() {
                        continue;
                    }
                    out.push(Piece {
                        part: Part::Conductor {
                            section: section.id.clone(),
                            span,
                        },
                        kv,
                        wires: section.subconductor_polylines(&piece),
                        allowance: s.surface_radius + s.polyline_error,
                    });
                }
            }
            if rule.string_ends {
                for st in shape.strings.get(p).into_iter().flatten() {
                    // A string shared by two sections (a TAK's): once.
                    if labels.insert(st.label.clone()) {
                        out.push(Piece {
                            part: Part::StringEnd {
                                label: st.label.clone(),
                            },
                            kv,
                            wires: vec![vec![st.bottom]],
                            allowance: 0.0,
                        });
                    }
                }
            }
        }
    }
    if rule.jumpers {
        for js in jumpers {
            let Some(j) = line.jumpers.iter().find(|j| j.id == js.jumper) else {
                continue;
            };
            if j.from.structure() != Some(structure) && j.to.structure() != Some(structure) {
                continue;
            }
            let Some(section) = line.jumper_section(j) else {
                continue;
            };
            let Some(kv) = kv_of(&section.id).filter(|kv| rule.takes_voltage(*kv)) else {
                continue;
            };
            if !filter.takes(Some(structure), Some(&section.id), case) {
                continue;
            }
            out.push(Piece {
                part: Part::Jumper(j.id.clone()),
                kv,
                wires: line.jumper_bundle(j).polylines(&js.points),
                allowance: js.surface_radius + js.polyline_error,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{PresetCheck, RuleSet};
    use ohl_core::budget::Budget;
    use ohl_core::units::Temperature;
    use ohl_weather::WeatherCase;

    fn example() -> (Line, ohl_cables::Library) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        (
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap(),
            ohl_cables::Library::load(&root.join("libraries/cables")).unwrap(),
        )
    }

    fn solve(line: &Line, library: &ohl_cables::Library, case: &WeatherCase) -> Vec<SectionShape> {
        ohl_line::solve::solve_line(line, library, case)
            .into_iter()
            .map(Result::unwrap)
            .collect()
    }

    /// A box member (a solid of 12 triangles) from `lo` to `hi`.
    fn solid_box(lo: P3, hi: P3) -> Vec<[[f64; 3]; 3]> {
        let c = |i: usize| {
            [
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ]
        };
        let faces = [
            [0, 1, 3, 2],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 3, 7, 5],
        ];
        faces
            .iter()
            .flat_map(|f| [[c(f[0]), c(f[1]), c(f[2])], [c(f[0]), c(f[2]), c(f[3])]])
            .collect()
    }

    /// A vertical member and a wire passing 3 m from it; a box member and
    /// a wire 2 m from its face: the exact distances, to the line and to
    /// the surface.
    #[test]
    fn nearest_on_a_synthetic_body() {
        let body = StructureBody {
            structure: "T".into(),
            members: vec![
                BodyMember {
                    id: Some("leg".into()),
                    line: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 30.0]],
                    solid: Vec::new(),
                },
                BodyMember {
                    id: Some("box".into()),
                    line: vec![[10.0, -0.5, 19.5], [10.0, 0.5, 20.5]],
                    solid: solid_box([9.5, -0.5, 19.5], [10.5, 0.5, 20.5]),
                },
            ],
        };
        let prepared = prepare(&body);
        // Horizontal wire across y at x = 3, z = 15: 3 m from the leg.
        let wire = vec![vec![
            [3.0, -10.0, 15.0],
            [3.0, 0.0, 15.0],
            [3.0, 10.0, 15.0],
        ]];
        let (d, at, on, k) = nearest(&wire, &prepared).unwrap();
        assert!((d - 3.0).abs() < 1e-12, "{d}");
        assert_eq!(k, 0);
        assert!((at[0] - 3.0).abs() < 1e-12 && at[1].abs() < 1e-12);
        assert!((on[2] - 15.0).abs() < 1e-12);
        // A wire along y at x = 10, z = 22.5: 2 m above the box's top
        // face (its centre line 2.5 m below: the surface is nearer).
        let wire = vec![vec![[10.0, -5.0, 22.5], [10.0, 5.0, 22.5]]];
        let (d, _, on, k) = nearest(&wire, &prepared).unwrap();
        assert_eq!(k, 1);
        assert!((d - 2.0).abs() < 1e-12, "{d}");
        assert!((on[2] - 20.5).abs() < 1e-12);
        // Through the box: 0.
        let wire = vec![vec![[8.0, 0.0, 20.0], [12.0, 0.0, 20.0]]];
        assert!(nearest(&wire, &prepared).unwrap().0 < 1e-12);
        // A point inside the box, and a wire ending in it: 0.
        assert!(nearest(&[vec![[10.0, 0.1, 20.2]]], &prepared).unwrap().0 < 1e-12);
        let wire = vec![vec![[10.1, 0.0, 20.0], [10.2, 0.0, 30.0]]];
        assert!(nearest(&wire, &prepared).unwrap().0 < 1e-12);
        // A point (a string end) beside the box's side, 1.5 m off.
        let (d, _, _, _) = nearest(&[vec![[12.0, 0.0, 20.0]]], &prepared).unwrap();
        assert!((d - 1.5).abs() < 1e-12, "{d}");
        // The reach: 20 m in plan from the clamp, in the wire's order.
        let pts: Vec<P3> = (0..=10).map(|i| [i as f64 * 5.0, 0.0, 0.0]).collect();
        let a = near_end(&pts, 12.0, true);
        assert_eq!(a.last().unwrap(), &[12.0, 0.0, 0.0]);
        let b = near_end(&pts, 12.0, false);
        assert_eq!(b.first().unwrap(), &[38.0, 0.0, 0.0]);
        assert_eq!(b.last().unwrap(), &[50.0, 0.0, 0.0]);
    }

    /// A body around the example line's first structure with a body: the
    /// structure's own position, a vertical member 2 m beside each wire
    /// point there (INVENTED geometry): every part found, the string ends
    /// 2 m off (no radius), the conductors at most that less their
    /// radius; the other structures listed as level 1; the overrides take
    /// a structure out of a case; a required distance by voltage.
    #[test]
    fn the_example_line() {
        let (line, library) = example();
        let still = WeatherCase::still("still", Temperature::from_celsius(10.0));
        let shapes = solve(&line, &library, &still);
        // The first structure holding strings, and every string's wire
        // point there (of every section).
        let (sid, _): (String, Vec<P3>) = line
            .sections
            .iter()
            .zip(&shapes)
            .flat_map(|(s, sh)| {
                s.points
                    .iter()
                    .zip(&sh.strings)
                    .map(|(p, st)| (p.structure.clone(), st.iter().map(|x| x.bottom).collect()))
            })
            .find(|(_, b): &(String, Vec<P3>)| !b.is_empty())
            .unwrap();
        let bottoms: Vec<P3> = line
            .sections
            .iter()
            .zip(&shapes)
            .flat_map(|(s, sh)| s.points.iter().zip(&sh.strings))
            .filter(|(p, _)| p.structure == sid)
            .flat_map(|(_, st)| st.iter().map(|x| x.bottom))
            .collect();
        let members: Vec<BodyMember> = bottoms
            .iter()
            .enumerate()
            .map(|(k, b)| BodyMember {
                id: Some(format!("m{k}")),
                line: vec![
                    [b[0] + 2.0, b[1], b[2] - 5.0],
                    [b[0] + 2.0, b[1], b[2] + 5.0],
                ],
                solid: Vec::new(),
            })
            .collect();
        let bodies = vec![StructureBody {
            structure: sid.clone(),
            members,
        }];
        let rules = vec![InternalRule {
            distance: Requirement::Fixed(1.0),
            ..InternalRule::blank("invented 1 m")
        }];
        let jumpers: Vec<JumperShape> = Vec::new();
        let r = check(
            &line,
            &[("still", &shapes, &jumpers)],
            &bodies,
            &rules,
            &Default::default(),
            &Budget::new().max_iterations(1000),
        )
        .unwrap();
        assert!(r.rows.iter().all(|x| x.structure == sid));
        let ends: Vec<&InternalRow> = r
            .rows
            .iter()
            .filter(|x| matches!(x.part, Part::StringEnd { .. }))
            .collect();
        assert!(!ends.is_empty());
        for e in &ends {
            assert!(e.distance <= 2.0 + 1e-9 && e.distance > 0.0, "{e:?}");
            assert!(!e.to_surface);
        }
        let wires: Vec<&InternalRow> = r
            .rows
            .iter()
            .filter(|x| matches!(x.part, Part::Conductor { .. }))
            .collect();
        assert!(!wires.is_empty());
        assert!(wires.iter().all(|x| x.distance < 2.0));
        // The worst first; centre lines said.
        assert!(
            r.rows
                .windows(2)
                .all(|w| w[0].distance - w[0].required <= w[1].distance - w[1].required)
        );
        assert!(r.notes.iter().any(|n| n.contains("on the unsafe side")));
        // Every other structure: level 1.
        assert_eq!(r.not_checked.len(), line.structures.len() - 1);
        assert!(
            r.not_checked
                .iter()
                .all(|(s, why)| *s != sid && why.contains("level 1"))
        );
        assert!(r.csv(&rules).starts_with("rule,case,structure,part"));
        // The structure override: the structure in a group without the
        // case: nothing checked there.
        let groups = vec![ohl_weather::CaseGroup {
            name: "g".into(),
            description: String::new(),
            cases: vec!["other".into()],
        }];
        let names = vec!["g".to_owned()];
        let filter = ohl_weather::CaseFilter {
            structures: Some(ohl_weather::OwnCases::new(
                [(sid.as_str(), names.as_slice())],
                &groups,
            )),
            sections: None,
        };
        let none = check(
            &line,
            &[("still", &shapes, &jumpers)],
            &bodies,
            &rules,
            &filter,
            &Budget::new(),
        )
        .unwrap();
        assert!(none.rows.is_empty());
        // A rule of other voltages: nothing; one by voltage below the
        // line's: noted, not checked.
        let other = InternalRule {
            voltages: vec![1.0],
            ..rules[0].clone()
        };
        let low = InternalRule {
            distance: Requirement::ByVoltage(vec![(0.5, 1.0)]),
            ..rules[0].clone()
        };
        let r = check(
            &line,
            &[("still", &shapes, &jumpers)],
            &bodies,
            &[other, low],
            &Default::default(),
            &Budget::new(),
        )
        .unwrap();
        assert!(r.rows.is_empty());
        assert!(
            r.notes.iter().any(|n| n.contains("no required clearance")),
            "{:?}",
            r.notes
        );
        // The exit clause: a budget of no step stops it.
        let e = check(
            &line,
            &[("still", &shapes, &jumpers)],
            &bodies,
            &rules,
            &Default::default(),
            &Budget::new().max_iterations(0),
        )
        .unwrap_err();
        assert!(e.contains("internal clearances"), "{e}");
    }

    /// Internal rules in a rule file read and write back; defaults (every
    /// part, 20 m); bad values refused.
    #[test]
    fn internal_rules_in_a_rule_file() {
        let text = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"i\"\n\
            [[internal_rule]]\nname = \"all\"\ndistance = \"1.5 m\"\n\
            [[internal_rule]]\nname = \"wind\"\ncases = [\"wind*\"]\nvoltages = [220.0]\n\
            distance = { by_voltage = [[150.0, \"1 m\"], [245.0, \"2 m\"]] }\n\
            jumpers = false\nreach = \"12 m\"\n";
        let set = RuleSet::parse(text).unwrap();
        assert_eq!(set.internal_rules.len(), 2);
        let (a, b) = (&set.internal_rules[0], &set.internal_rules[1]);
        assert_eq!(
            a,
            &InternalRule {
                distance: Requirement::Fixed(1.5),
                ..InternalRule::blank("all")
            }
        );
        assert!(b.conductors && !b.jumpers && b.string_ends);
        assert_eq!(b.reach, 12.0);
        assert_eq!(b.distance.at(220.0).unwrap(), 2.0);
        assert!(b.takes_voltage(220.0) && !b.takes_voltage(380.0));
        assert!(b.applies("wind 500") && !b.applies("still"));
        assert!(set.is_for(PresetCheck::Internal));
        assert!(!set.is_for(PresetCheck::Strings));
        let back = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(back.internal_rules, set.internal_rules);
        for bad in [
            "",
            "distance = \"1 m\"\nreach = \"0 m\"",
            "distance = \"1 m\"\nconductors = false\njumpers = false\nstring_ends = false",
            "distance = \"1 m\"\nsize = 2",
        ] {
            let e = RuleSet::parse(&format!(
                "format = \"ohl-clearance-rules\"\nversion = 1\n[[internal_rule]]\nname = \"x\"\n{bad}\n"
            ));
            assert!(e.is_err(), "{bad}");
        }
    }
}
