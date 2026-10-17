// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Vertical and radial clearances of wires to the ground, to points and to
//! other wires (DESIGN_GOALS §13).
//!
//! Wires are the solved spans and jumpers as dense polylines on the exact
//! catenary ([`wires_of`]); their segments are indexed on a plan grid, so
//! each target (a LiDAR point, a ground triangle, another wire's segment)
//! is compared only with the wire segments near it.
//!
//! Accuracy: between polyline points the wire lies below its chord by at
//! most the chord sag of a piece (a few mm for 6 m pieces); for the ground
//! the wire is sampled every [`GROUND_STEP`] for the radial distance.

use std::collections::HashMap;

use ols_line::Line;
use ols_line::electrical::Potential;
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
use ols_terrain::Terrain;

use crate::geometry::{
    P3, closest_between_segments, closest_on_segment, closest_on_triangle, crossing_in_plan,
    distance, over_in_plan,
};
use crate::rules::{Required, Rule};

/// Spacing of wire samples for the radial distance to the ground, m: the
/// distance is over-estimated by at most s²/(8d) — 3 mm at d = 10 m.
pub const GROUND_STEP: f64 = 0.5;

/// Plan grid cell size of the wire index, m.
const CELL: f64 = 10.0;

/// One wire: a span of a section, or a jumper.
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    /// Section id, or `jumper:<id>`.
    pub section: String,
    /// Span index (0 for a jumper).
    pub span: usize,
    /// Circuit name.
    pub circuit: String,
    /// Phase label.
    pub phase: String,
    /// Electrical potential of its section (none: earthed wire).
    pub potential: Option<Potential>,
    /// Bundles: subconductor positions (across, up) around the points,
    /// m ([`ols_line::Section::bundle_offsets`]); empty or `[[0, 0]]` for a
    /// single wire.
    pub bundle: Vec<[f64; 2]>,
    /// Points along the wire.
    pub points: Vec<P3>,
    /// The attachments it hangs on (`structure:set:phase`): wires sharing
    /// one are connected (adjacent spans, a jumper and its sections).
    pub ends: Vec<String>,
}

impl Wire {
    /// Nominal voltage of its system, kV (none: earthed).
    pub fn kv(&self) -> Option<f64> {
        self.potential.map(|p| p.nominal_kv)
    }

    /// Radius of the bundle, m (0 for a single wire).
    pub fn bundle_radius(&self) -> f64 {
        self.bundle
            .iter()
            .map(|o| o[0].hypot(o[1]))
            .fold(0.0, f64::max)
    }

    /// Lowest and highest subconductor relative to the centre line, m.
    fn vertical_extent(&self) -> (f64, f64) {
        if self.bundle.is_empty() {
            return (0.0, 0.0);
        }
        self.bundle
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), o| {
                (lo.min(o[1]), hi.max(o[1]))
            })
    }

    /// The subconductors at centre-line point `q` of the segment `a`–`b`:
    /// `up` is the vertical made square to the wire, `across` horizontal to
    /// the right looking along it.
    fn subconductors(&self, q: P3, a: P3, b: P3) -> Vec<P3> {
        if self.bundle.len() <= 1 && self.bundle.first().is_none_or(|o| *o == [0.0, 0.0]) {
            return vec![q];
        }
        self.bundle
            .iter()
            .map(|o| ols_line::bundle_point(q, a, b, *o))
            .collect()
    }

    /// The subconductors' pieces along the centre-line segment `a`–`b`.
    fn sub_segments(&self, a: P3, b: P3) -> Vec<(P3, P3)> {
        self.subconductors(a, a, b)
            .into_iter()
            .zip(self.subconductors(b, a, b))
            .collect()
    }

    /// The nearest point of the subconductors along `a`–`b` to `p`:
    /// (distance, its point).
    pub(crate) fn nearest(&self, p: P3, a: P3, b: P3) -> (f64, P3) {
        self.sub_segments(a, b)
            .into_iter()
            .map(|(s, t)| {
                let (q, _) = closest_on_segment(p, s, t);
                (distance(p, q), q)
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .expect("at least one subconductor")
    }

    /// Vertical separation of the bundle at centre-line point `q` from a
    /// point at height `z` below or above it (0 inside its height range):
    /// to the lowest subconductor from below, the highest from above.
    fn vertical_gap(&self, q: P3, z: f64) -> f64 {
        let (lo, hi) = self.vertical_extent();
        if z <= q[2] + lo {
            q[2] + lo - z
        } else if z >= q[2] + hi {
            z - (q[2] + hi)
        } else {
            0.0
        }
    }
}

fn key(p: &ols_line::SectionPoint) -> String {
    format!("{}:{}", p.structure, p.set)
}

/// The wires of a solved line: every span of every section, and every
/// jumper (with the circuit, phase and potential of the section at its
/// `from` end).
pub fn wires_of(line: &Line, shapes: &[SectionShape], jumpers: &[JumperShape]) -> Vec<Wire> {
    // A valid line has potentials; an invalid one is checked as earthed.
    let potentials = line.potentials().unwrap_or_default();
    let potential = |id: &str| potentials.get(id).copied().flatten();
    let mut wires = Vec::new();
    // The clamps jumpers hang on (taps): where they join their wire.
    let tapped: std::collections::BTreeSet<&ols_line::Tap> =
        line.jumpers.iter().flat_map(|j| j.taps()).collect();
    for shape in shapes {
        let section = line.sections.iter().find(|s| s.id == shape.section);
        for (k, span) in shape.spans.iter().enumerate() {
            wires.push(Wire {
                section: shape.section.clone(),
                span: k,
                circuit: section.map_or(String::new(), |s| s.circuit.clone()),
                phase: section.map_or(String::new(), |s| s.phase_label.clone()),
                potential: potential(&shape.section),
                bundle: section.map_or(Vec::new(), |s| s.bundle_offsets()),
                points: span.points.clone(),
                ends: section.map_or(Vec::new(), |s| {
                    let mut e: Vec<String> = s.points.iter().skip(k).take(2).map(key).collect();
                    // Its clamps jumpers hang on: where taps join them to
                    // it (a spacer's clamp is not a tap).
                    e.extend(
                        s.span_points
                            .iter()
                            .filter(|p| p.span == k && is_clamp(&p.object))
                            .map(|p| {
                                ols_line::Tap::Span(ols_line::SpanPointRef {
                                    section: s.id.clone(),
                                    point: p.label.clone(),
                                })
                            })
                            .filter(|t| tapped.contains(t))
                            .map(|t| tap_key(&t)),
                    );
                    e
                }),
            });
        }
    }
    for j in jumpers {
        let def = line.jumpers.iter().find(|d| d.id == j.jumper);
        let from = def.and_then(|d| line.jumper_section(d));
        wires.push(Wire {
            section: format!("jumper:{}", j.jumper),
            span: 0,
            circuit: from.map_or(String::new(), |s| s.circuit.clone()),
            phase: from.map_or(String::new(), |s| s.phase_label.clone()),
            potential: from.and_then(|s| potential(&s.id)),
            bundle: def.map_or(Vec::new(), |d| line.jumper_bundle(d).offsets()),
            points: j.points.clone(),
            ends: def.map_or(Vec::new(), |d| {
                let end = |e: &ols_line::JumperEnd| match e {
                    ols_line::JumperEnd::Wire(p) => key(p),
                    ols_line::JumperEnd::Tap { tap } => tap_key(tap),
                };
                let mut e = vec![end(&d.from), end(&d.to)];
                // Its clamps jumpers hang on: where they join it.
                e.extend(
                    d.points
                        .iter()
                        .filter(|p| is_clamp(&p.object))
                        .map(|p| ols_line::Tap::Jumper {
                            jumper: d.id.clone(),
                            point: p.label.clone(),
                        })
                        .filter(|t| tapped.contains(t))
                        .map(|t| tap_key(&t)),
                );
                e
            }),
        });
    }
    wires
}

/// A clamp object (taps join jumpers there).
fn is_clamp(o: &Option<ols_line::LineObject>) -> bool {
    o.as_ref()
        .is_some_and(|o| o.kind == ols_line::ObjectKind::Clamp)
}

/// The end key of a tap: a wire with a clamp there and a jumper on it
/// share it (connected).
fn tap_key(t: &ols_line::Tap) -> String {
    match t {
        ols_line::Tap::Span(r) => format!("tap:{}:{}", r.section, r.point),
        ols_line::Tap::Jumper { jumper, point } => format!("tap:jumper:{jumper}:{point}"),
    }
}

/// The structure of an attachment key `structure:set:phase`.
fn structure_of(key: &str) -> &str {
    key.rsplitn(3, ':').last().unwrap_or(key)
}

/// The parts of `wires` that `rule` checks, in the same order (a wire left
/// out entirely has no points): only wires of the rule's `voltages`,
/// without spans ending at a structure of `exclude_spans_at`, and without
/// the wire within `exclude_near_ends` (horizontal distance along the
/// wire) of its ends — cut at the exact distance.
pub fn checked_parts(rule: &Rule, wires: &[Wire]) -> Vec<Wire> {
    let d = rule.exclude_near_ends;
    wires
        .iter()
        .map(|w| {
            let mut part = w.clone();
            let excluded = !rule.checks_voltage(w.kv())
                || !rule.wires.takes(&w.section)
                || w.ends.iter().any(|e| {
                    rule.exclude_spans_at
                        .iter()
                        .any(|p| ols_core::pattern::matches(p, structure_of(e)))
                });
            if excluded {
                part.points.clear();
                return part;
            }
            if d <= 0.0 || w.points.len() < 2 {
                return part;
            }
            // Horizontal distance along the wire at every point.
            let mut along = vec![0.0];
            for s in w.points.windows(2) {
                let last = *along.last().expect("starts with 0");
                along.push(last + (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]));
            }
            let total = *along.last().expect("points");
            if total <= 2.0 * d {
                part.points.clear();
                return part;
            }
            let at = |t: f64| {
                let k = along.partition_point(|&a| a < t).clamp(1, along.len() - 1);
                let (a0, a1) = (along[k - 1], along[k]);
                let f = if a1 > a0 { (t - a0) / (a1 - a0) } else { 0.0 };
                let (p, q) = (w.points[k - 1], w.points[k]);
                [
                    p[0] + (q[0] - p[0]) * f,
                    p[1] + (q[1] - p[1]) * f,
                    p[2] + (q[2] - p[2]) * f,
                ]
            };
            let mut pts = vec![at(d)];
            pts.extend(
                w.points
                    .iter()
                    .zip(&along)
                    .filter(|(_, a)| **a > d && **a < total - d)
                    .map(|(p, _)| *p),
            );
            pts.push(at(total - d));
            part.points = pts;
            part
        })
        .collect()
}

/// Wires with a plan grid of their segments, each segment listed in every
/// cell within `reach` of it.
#[derive(Debug, Clone)]
pub struct WireSet {
    /// The wires.
    pub wires: Vec<Wire>,
    /// Bundle radius of each wire, m.
    pub(crate) radii: Vec<f64>,
    reach: f64,
    segments: Vec<(u32, u32)>,
    cells: HashMap<(i64, i64), Vec<u32>>,
}

fn cell_of(v: f64) -> i64 {
    (v / CELL).floor() as i64
}

impl WireSet {
    /// Indexes `wires` for queries up to `reach` (m) away.
    pub fn new(wires: Vec<Wire>, reach: f64) -> Self {
        let mut segments = Vec::new();
        let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (w, wire) in wires.iter().enumerate() {
            // Subconductors reach out by the bundle radius.
            let reach = reach + wire.bundle_radius();
            for (k, s) in wire.points.windows(2).enumerate() {
                let id = segments.len() as u32;
                segments.push((w as u32, k as u32));
                let (x0, x1) = (s[0][0].min(s[1][0]) - reach, s[0][0].max(s[1][0]) + reach);
                let (y0, y1) = (s[0][1].min(s[1][1]) - reach, s[0][1].max(s[1][1]) + reach);
                for i in cell_of(x0)..=cell_of(x1) {
                    for j in cell_of(y0)..=cell_of(y1) {
                        cells.entry((i, j)).or_default().push(id);
                    }
                }
            }
        }
        Self {
            radii: wires.iter().map(Wire::bundle_radius).collect(),
            wires,
            reach,
            segments,
            cells,
        }
    }

    /// Distance up to which queries are answered, m.
    pub fn reach(&self) -> f64 {
        self.reach
    }

    /// Segments (wire, first point index) that may be within `reach` of
    /// the plan position (x, y).
    pub(crate) fn near(&self, x: f64, y: f64) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.cells
            .get(&(cell_of(x), cell_of(y)))
            .into_iter()
            .flatten()
            .map(|&id| {
                let (w, k) = self.segments[id as usize];
                (w as usize, k as usize)
            })
    }

    pub(crate) fn segment(&self, w: usize, k: usize) -> (P3, P3) {
        let p = &self.wires[w].points;
        (p[k], p[k + 1])
    }
}

/// A clearance found: its value and where.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Clearance, m. Vertical to the ground: wire height above it
    /// (negative if below); to points and other wires: the vertical
    /// separation, whichever is higher.
    pub distance: f64,
    /// Point on the wire.
    pub wire: P3,
    /// Point on the target.
    pub target: P3,
    /// Point class, for point targets.
    pub class: Option<u8>,
}

/// Results of one rule for one wire (or one pair of wires).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WireResult {
    /// Smallest vertical clearance found within reach.
    pub vertical: Option<Hit>,
    /// Smallest radial clearance found within reach.
    pub radial: Option<Hit>,
    /// Below the required vertical clearance: number of points for point
    /// targets; number of wire segments (polyline pieces) for the ground
    /// and wire pairs.
    pub vertical_violations: u64,
    /// Below the required radial clearance, counted the same way.
    pub radial_violations: u64,
    /// The requirements that applied (by the wire's or pair's voltage).
    pub required: Required,
}

impl WireResult {
    /// An empty result with these requirements.
    pub fn with(required: Required) -> Self {
        Self {
            required,
            ..Self::default()
        }
    }

    fn vertical(&mut self, hit: Hit) -> bool {
        if self.vertical.is_none_or(|h| hit.distance < h.distance) {
            self.vertical = Some(hit);
        }
        self.required.vertical.is_some_and(|r| hit.distance < r)
    }

    fn radial(&mut self, hit: Hit) -> bool {
        if self.radial.is_none_or(|h| hit.distance < h.distance) {
            self.radial = Some(hit);
        }
        self.required.radial.is_some_and(|r| hit.distance < r)
    }

    /// Both measures: `vertical` and `radial`, the clearance found, the
    /// clearance required and the count below it.
    pub fn measures(&self) -> [(&'static str, Option<Hit>, Option<f64>, u64); 2] {
        [
            (
                "vertical",
                self.vertical,
                self.required.vertical,
                self.vertical_violations,
            ),
            (
                "radial",
                self.radial,
                self.required.radial,
                self.radial_violations,
            ),
        ]
    }

    /// Whether a requirement is not met.
    pub fn violates(&self) -> bool {
        let below =
            |hit: Option<Hit>, req: Option<f64>| hit.zip(req).is_some_and(|(h, r)| h.distance < r);
        below(self.vertical, self.required.vertical) || below(self.radial, self.required.radial)
    }
}

/// Clearances of every wire to points of chosen classes, for one rule;
/// feed the points with [`PointCheck::visit`] (e.g. from a LiDAR reader).
#[derive(Debug, Clone)]
pub struct PointCheck {
    rule: Rule,
    classes: [bool; 256],
    /// One result per wire of the set.
    pub results: Vec<WireResult>,
    seen: Vec<usize>,
    groups: Option<Vec<usize>>,
    /// With groups ([`PointCheck::grouped`]): the points below the
    /// vertical and the radial requirement of any wire of each group (a
    /// point counted once per group).
    pub group_violations: Vec<[u64; 2]>,
}

impl PointCheck {
    /// A check of `rule` on the wires of `wires` against points of
    /// `classes`.
    pub fn new(rule: &Rule, classes: &[u8], wires: &WireSet) -> Self {
        let mut mask = [false; 256];
        for &c in classes {
            mask[c as usize] = true;
        }
        Self {
            rule: rule.clone(),
            classes: mask,
            results: wires
                .wires
                .iter()
                .map(|w| WireResult::with(rule.required(w.kv())))
                .collect(),
            seen: Vec::new(),
            groups: None,
            group_violations: Vec::new(),
        }
    }

    /// Also counts the points too close per group of wires — `group[w]`
    /// for wire `w`, e.g. the positions of one span in several wind
    /// states: a point too close to several of them counts once.
    pub fn grouped(mut self, group: Vec<usize>) -> Self {
        let n = group.iter().max().map_or(0, |g| g + 1);
        self.group_violations = vec![[0, 0]; n];
        self.groups = Some(group);
        self
    }

    /// The rule checked.
    pub fn rule(&self) -> &Rule {
        &self.rule
    }

    /// Takes one point into account.
    pub fn visit(&mut self, wires: &WireSet, x: f64, y: f64, z: f64, class: u8) {
        if !self.classes[class as usize] {
            return;
        }
        let p = [x, y, z];
        // Wires this point violates (each counted once per point).
        let mut vertical_bad: Vec<usize> = Vec::new();
        let mut radial_bad: Vec<usize> = Vec::new();
        self.seen.clear();
        for (w, k) in wires.near(x, y) {
            let (a, b) = wires.segment(w, k);
            let wire = &wires.wires[w];
            let result = &mut self.results[w];
            let reach = (result.required.largest() + self.rule.search_margin).min(wires.reach);
            // The centre line first: subconductors only when within reach.
            let (q, _) = closest_on_segment(p, a, b);
            let near = distance(p, q) - wires.radii[w] <= reach;
            let (d, sub) = if near && wires.radii[w] > 0.0 {
                wire.nearest(p, a, b)
            } else {
                (distance(p, q), q)
            };
            if near && d <= reach {
                let hit = Hit {
                    distance: d,
                    wire: sub,
                    target: p,
                    class: Some(class),
                };
                if result.radial(hit) && !radial_bad.contains(&w) {
                    radial_bad.push(w);
                }
            }
            if let Some((q, _)) = over_in_plan(x, y, a, b, self.rule.vertical_band) {
                // Vertical separation, above or below the wire (a tree
                // overhanging a low wire, a line crossing over); with a
                // depth, to the wire's lowest (highest) in the slice.
                let gap = wire.vertical_gap(q, z) - slice_rise(a, b, self.rule.vertical_depth);
                let hit = Hit {
                    distance: gap.max(0.0),
                    wire: q,
                    target: p,
                    class: Some(class),
                };
                if hit.distance <= reach && result.vertical(hit) && !vertical_bad.contains(&w) {
                    vertical_bad.push(w);
                }
            }
        }
        if let Some(groups) = &self.groups {
            for (k, bad) in [&vertical_bad, &radial_bad].into_iter().enumerate() {
                let mut gs: Vec<usize> = bad.iter().map(|w| groups[*w]).collect();
                gs.sort_unstable();
                gs.dedup();
                for g in gs {
                    self.group_violations[g][k] += 1;
                }
            }
        }
        for w in vertical_bad {
            self.results[w].vertical_violations += 1;
        }
        for w in radial_bad {
            self.results[w].radial_violations += 1;
        }
    }
}

/// How much the wire piece `a`–`b` rises or falls over half a slice of
/// `depth` along the line, m: a vertical clearance is that much smaller
/// over a slice than straight below the wire.
fn slice_rise(a: P3, b: P3, depth: f64) -> f64 {
    let plan = (b[0] - a[0]).hypot(b[1] - a[1]);
    if depth <= 0.0 || plan <= 0.0 {
        return 0.0;
    }
    (b[2] - a[2]).abs() / plan * depth / 2.0
}

/// The ground under the wire piece `a`–`b` within `band` (horizontal) of
/// it: (t along the piece, ground point) at every TIN edge crossing under
/// the piece and under its two edges `band` to each side, and every TIN
/// vertex in between — the highest ground of the strip is at one of them.
/// Without a band, the crossings straight below.
fn ground_under(terrain: &Terrain, a: P3, b: P3, band: f64) -> Vec<(f64, P3)> {
    let mut out: Vec<(f64, P3)> = Vec::new();
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    let mut lines = vec![[0.0, 0.0]];
    if band > 0.0 && len > 0.0 {
        let n = [-dy / len * band, dx / len * band];
        lines.push(n);
        lines.push([-n[0], -n[1]]);
    }
    for o in lines {
        for run in terrain.profile([a[0] + o[0], a[1] + o[1]], [b[0] + o[0], b[1] + o[1]]) {
            out.extend(run.into_iter().map(|g| (g.t, g.point)));
        }
    }
    if band > 0.0 && len > 0.0 {
        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        let mut corners: Vec<P3> = Vec::new();
        terrain.triangles_near(mid[0], mid[1], len / 2.0 + band, |tri| corners.extend(tri));
        for c in corners {
            let t = ((c[0] - a[0]) * dx + (c[1] - a[1]) * dy) / (len * len);
            if (0.0..=1.0).contains(&t)
                && over_in_plan(c[0], c[1], a, b, band).is_some()
                && !out.iter().any(|(_, p)| *p == c)
            {
                out.push((t, c));
            }
        }
    }
    out
}

/// Vertical and radial clearances of every wire to the ground surface.
/// Vertical: under every wire piece at every TIN edge crossing (exact for
/// the polyline) — with the rule's `vertical_band`, the highest ground
/// within that horizontal distance of the wire; with its `vertical_depth`,
/// the wire's lowest in a slice of that depth. Radial: wire samples every [`GROUND_STEP`] to the
/// triangles within the rule's reach (narrowed to the vertical clearance
/// where that is smaller: the radial distance is never larger).
pub fn ground_check(rule: &Rule, wires: &[Wire], terrain: &Terrain) -> Vec<WireResult> {
    wires
        .iter()
        .map(|wire| {
            let mut result = WireResult::with(rule.required(wire.kv()));
            let reach = result.required.largest() + rule.search_margin;
            // Bundles: the lowest subconductor for the vertical clearance.
            let (lowest, _) = wire.vertical_extent();
            for s in wire.points.windows(2) {
                let (a, b) = (s[0], s[1]);
                let (mut vertical_bad, mut radial_bad) = (false, false);
                let rise = slice_rise(a, b, rule.vertical_depth);
                for (t, g) in ground_under(terrain, a, b, rule.vertical_band) {
                    let z = a[2] + (b[2] - a[2]) * t + lowest;
                    let hit = Hit {
                        distance: z - rise - g[2],
                        wire: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, z],
                        target: g,
                        class: None,
                    };
                    vertical_bad |= result.vertical(hit);
                }
                result.vertical_violations += u64::from(vertical_bad);
                if result.required.radial.is_none() {
                    continue;
                }
                let n = ((distance(a, b) / GROUND_STEP).ceil() as usize).max(1);
                for i in 0..=n {
                    let t = i as f64 / n as f64;
                    let centre = [
                        a[0] + (b[0] - a[0]) * t,
                        a[1] + (b[1] - a[1]) * t,
                        a[2] + (b[2] - a[2]) * t,
                    ];
                    // Each subconductor of a bundle (one point for a wire).
                    // The ground straight below bounds the search (radial is
                    // never larger than vertical); where it is far below or
                    // missing (a hole), the ground beside may still be near:
                    // searched over the whole reach.
                    for p in wire.subconductors(centre, a, b) {
                        let below = terrain.height(p[0], p[1]);
                        let radius = below.map_or(reach, |g| (p[2] - g).max(0.0).min(reach));
                        let mut best: Option<(f64, P3)> = None;
                        terrain.triangles_near(p[0], p[1], radius, |[ta, tb, tc]| {
                            let q = closest_on_triangle(p, ta, tb, tc);
                            let d = distance(p, q);
                            if best.is_none_or(|(bd, _)| d < bd) {
                                best = Some((d, q));
                            }
                        });
                        if let Some((d, q)) = best {
                            let d = if below.is_some_and(|g| p[2] < g) {
                                -d
                            } else {
                                d
                            };
                            let hit = Hit {
                                distance: d,
                                wire: p,
                                target: q,
                                class: None,
                            };
                            radial_bad |= result.radial(hit);
                        }
                    }
                }
                result.radial_violations += u64::from(radial_bad);
            }
            result
        })
        .collect()
}

/// Whether two wires share an end point (within 1 cm).
fn connected(a: &Wire, b: &Wire) -> bool {
    if !a.ends.is_empty() && !b.ends.is_empty() {
        return a.ends.iter().any(|e| b.ends.contains(e));
    }
    let ends = |w: &Wire| [w.points.first().copied(), w.points.last().copied()];
    ends(a)
        .into_iter()
        .flatten()
        .any(|p| ends(b).into_iter().flatten().any(|q| distance(p, q) < 0.01))
}

/// Clearance between two wires.
#[derive(Debug, Clone, PartialEq)]
pub struct PairResult {
    /// Index of the first wire (in its set).
    pub a: usize,
    /// Index of the second wire (in its set).
    pub b: usize,
    /// Clearances: radial (shortest distance) and vertical (upper wire
    /// above the lower one where they cross in plan).
    pub result: WireResult,
}

/// Which wire pairs [`wire_pairs`] checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairMode {
    /// `b` holds the same wires as `a` (one weather case): each pair once,
    /// not a wire with itself, nor connected wires (sharing an attachment),
    /// nor wires of the same circuit and phase unless asked.
    SameLine {
        /// Also check wires of the same circuit and phase.
        include_same_phase: bool,
    },
    /// `b` holds the same line's wires in another weather case: every
    /// ordered pair except a wire with itself, connected wires, and (unless
    /// asked) the same circuit and phase.
    SameLineOtherCase {
        /// Also check wires of the same circuit and phase.
        include_same_phase: bool,
    },
    /// `b` holds another line's wires: every pair.
    OtherLines,
}

/// Clearances between the wires of `a` and those of the indexed set `b`:
/// radial for pairs closer than the rule's reach, vertical wherever they
/// cross in plan (crossing clearances are always reported). With
/// `same_set`, `b` holds the same wires as `a`: each pair is checked once,
/// a wire not with itself, and wires of the same circuit and phase only if
/// `include_same_phase`. Wires sharing an end point (within 1 cm) are
/// connected — adjacent spans, a jumper and its sections — and not checked
/// against each other.
pub fn wire_pairs(rule: &Rule, a: &[Wire], b: &WireSet, mode: PairMode) -> Vec<PairResult> {
    let mut pairs: HashMap<(usize, usize), WireResult> = HashMap::new();
    for (i, wire) in a.iter().enumerate() {
        for s in wire.points.windows(2) {
            let (p, q) = (s[0], s[1]);
            let mut candidates: Vec<(usize, usize)> = Vec::new();
            for t in [0.0, 0.5, 1.0] {
                let (x, y) = (p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t);
                candidates.extend(b.near(x, y));
            }
            candidates.sort_unstable();
            candidates.dedup();
            // Wires this segment violates (counted once per segment).
            let mut bad: Vec<(usize, bool, bool)> = Vec::new();
            for (j, k) in candidates {
                let other = &b.wires[j];
                let same_phase = other.circuit == wire.circuit && other.phase == wire.phase;
                let circuits = rule.circuits.admits(&wire.circuit, &other.circuit);
                let skip = match mode {
                    PairMode::SameLine { include_same_phase } => {
                        j <= i
                            || !circuits
                            || (!include_same_phase && same_phase)
                            || connected(wire, other)
                    }
                    PairMode::SameLineOtherCase { include_same_phase } => {
                        (other.section == wire.section && other.span == wire.span)
                            || !circuits
                            || (!include_same_phase && same_phase)
                            || connected(wire, other)
                    }
                    PairMode::OtherLines => false,
                };
                if skip {
                    continue;
                }
                let (c, d) = b.segment(j, k);
                let crossing = crossing_in_plan(p, q, c, d);
                let required = rule
                    .required(rule.pair_voltage(wire.potential.as_ref(), other.potential.as_ref()));
                let reach = (required.largest() + rule.search_margin).min(b.reach);
                // Centre lines first: bundles only when within reach.
                let (ca, cb) = closest_between_segments(p, q, c, d);
                let spread = wire.bundle_radius() + b.radii[j];
                if distance(ca, cb) - spread > reach && crossing.is_none() {
                    continue;
                }
                // Bundles: the nearest pair of subconductor pieces.
                let (dist, on_a, on_b) = if spread == 0.0 {
                    (distance(ca, cb), ca, cb)
                } else {
                    let (sa, sb) = (wire.sub_segments(p, q), other.sub_segments(c, d));
                    sa.iter()
                        .flat_map(|x| sb.iter().map(move |y| (x, y)))
                        .map(|(x, y)| {
                            let (u, w) = closest_between_segments(x.0, x.1, y.0, y.1);
                            (distance(u, w), u, w)
                        })
                        .min_by(|x, y| x.0.total_cmp(&y.0))
                        .expect("at least one subconductor each")
                };
                if dist > reach && crossing.is_none() {
                    continue;
                }
                let result = pairs
                    .entry((i, j))
                    .or_insert_with(|| WireResult::with(required));
                let mut radial_bad = false;
                if dist <= reach {
                    let hit = Hit {
                        distance: dist,
                        wire: on_a,
                        target: on_b,
                        class: None,
                    };
                    radial_bad = result.radial(hit);
                }
                let mut vertical_bad = false;
                if let Some((x, y)) = crossing {
                    // Bundles: the upper one's lowest subconductor over the
                    // lower one's highest.
                    let ((alo, ahi), (blo, bhi)) =
                        (wire.vertical_extent(), other.vertical_extent());
                    let gap = if x[2] >= y[2] {
                        (x[2] + alo) - (y[2] + bhi)
                    } else {
                        (y[2] + blo) - (x[2] + ahi)
                    };
                    let hit = Hit {
                        distance: gap,
                        wire: x,
                        target: y,
                        class: None,
                    };
                    vertical_bad = result.vertical(hit);
                }
                match bad.iter_mut().find(|e| e.0 == j) {
                    Some(e) => {
                        e.1 |= vertical_bad;
                        e.2 |= radial_bad;
                    }
                    None => bad.push((j, vertical_bad, radial_bad)),
                }
            }
            for (j, vertical_bad, radial_bad) in bad {
                let result = pairs.get_mut(&(i, j)).expect("entry made above");
                result.vertical_violations += u64::from(vertical_bad);
                result.radial_violations += u64::from(radial_bad);
            }
        }
    }
    let mut out: Vec<PairResult> = pairs
        .into_iter()
        .map(|((a, b), result)| PairResult { a, b, result })
        .collect();
    out.sort_by_key(|p| (p.a, p.b));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{RuleSet, Target};
    use ols_terrain::SurveyPoint;

    fn straight(section: &str, phase: &str, from: P3, to: P3) -> Wire {
        let n = 50;
        Wire {
            section: section.into(),
            span: 0,
            circuit: "c".into(),
            phase: phase.into(),
            points: (0..=n)
                .map(|k| {
                    let t = k as f64 / n as f64;
                    [
                        from[0] + (to[0] - from[0]) * t,
                        from[1] + (to[1] - from[1]) * t,
                        from[2] + (to[2] - from[2]) * t,
                    ]
                })
                .collect(),
            ends: vec![],
            potential: None,
            bundle: vec![],
        }
    }

    /// Phase L1 of a three-phase system of `kv`.
    fn ac(kv: f64) -> Potential {
        Potential {
            nominal_kv: kv,
            kind: ols_line::electrical::SystemKind::Ac,
            frequency_hz: Some(50.0),
            to_earth: [kv / 3f64.sqrt(), 0.0],
        }
    }

    fn rule(text: &str) -> Rule {
        RuleSet::parse(text).unwrap().rules.remove(0)
    }

    #[test]
    fn exclusion_zones() {
        let mut w = straight("S1", "L1", [0.0, 0.0, 20.0], [100.0, 0.0, 30.0]);
        w.ends = vec!["T1:1:1".into(), "con_7:2:1".into()];
        let other = Wire {
            ends: vec!["T2:1:1".into(), "T3:1:1".into()],
            ..straight("S2", "L1", [0.0, 5.0, 20.0], [100.0, 5.0, 20.0])
        };
        let near = rule(
            "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 1.0\nexclude_near_ends = \"10 m\"",
        );
        let parts = checked_parts(&near, &[w.clone(), other.clone()]);
        // Cut 10 m in from each end, exactly (heights interpolated).
        let (first, last) = (parts[0].points[0], *parts[0].points.last().unwrap());
        assert!(
            (first[0] - 10.0).abs() < 1e-9 && (first[2] - 21.0).abs() < 1e-9,
            "{first:?}"
        );
        assert!(
            (last[0] - 90.0).abs() < 1e-9 && (last[2] - 29.0).abs() < 1e-9,
            "{last:?}"
        );
        assert!(
            parts[0]
                .points
                .iter()
                .all(|p| p[0] >= 10.0 - 1e-9 && p[0] <= 90.0 + 1e-9)
        );
        // Spans at a cable termination: left out entirely.
        let at = rule(
            "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 1.0\nexclude_spans_at = [\"CON_*\"]",
        );
        let parts = checked_parts(&at, &[w, other]);
        assert!(parts[0].points.is_empty());
        assert_eq!(parts[1].points.len(), 51);
    }

    #[test]
    fn same_line_in_another_case() {
        // Two phases 4 m apart, and a next span of phase 1 sharing the
        // attachment B (connected), in two cases that differ (the second
        // case's wires 0.5 m lower and their ends moved).
        let with_ends = |mut w: Wire, ends: [&str; 2]| {
            w.ends = ends.iter().map(|e| e.to_string()).collect();
            w
        };
        let case = |dz: f64| {
            vec![
                with_ends(
                    straight("S1", "L1", [0.0, 0.0, 20.0 - dz], [100.0, 0.0, 20.0 - dz]),
                    ["A:1:1", "B:1:1"],
                ),
                with_ends(
                    straight("S2", "L2", [0.0, 4.0, 20.0 - dz], [100.0, 4.0, 20.0 - dz]),
                    ["A:1:2", "B:1:2"],
                ),
                with_ends(
                    straight("S3", "L9", [100.0, 0.0, 20.0 - dz], [200.0, 0.0, 20.0 - dz]),
                    ["B:1:1", "C:1:1"],
                ),
            ]
        };
        let (iced, hot) = (case(0.0), case(0.5));
        let set = WireSet::new(hot, 20.0);
        let pp = rule("[[rule]]\nname = \"pp\"\ntarget = \"same_line\"\nradial = 5.0");
        let pairs = wire_pairs(
            &pp,
            &iced,
            &set,
            PairMode::SameLineOtherCase {
                include_same_phase: true,
            },
        );
        let found: Vec<(usize, usize)> = pairs.iter().map(|p| (p.a, p.b)).collect();
        // Both orderings of S1–S2 and S2–S3; not a wire with itself; not
        // S1–S3 (they share B:1:1), although their ends no longer meet.
        assert_eq!(found, vec![(0, 1), (1, 0), (1, 2), (2, 1)], "{found:?}");
        let d = pairs[0].result.radial.unwrap().distance;
        assert!((d - 4.0f64.hypot(0.5)).abs() < 1e-9, "{d}");
    }

    #[test]
    fn circuit_to_circuit_and_phase_to_phase() {
        // A double-circuit span: circuit A phases at y = 0 and 4, circuit
        // B at y = 10; B's wire in the second case 1 m lower (ice drop of
        // the other circuit's wire), with case pairs.
        let wire = |section: &str, circuit: &str, phase: &str, y: f64, z: f64| Wire {
            circuit: circuit.into(),
            ..straight(section, phase, [0.0, y, z], [100.0, y, z])
        };
        let case = |drop: f64| {
            vec![
                wire("A1", "A", "L1", 0.0, 20.0),
                wire("A2", "A", "L2", 4.0, 20.0),
                wire("B1", "B", "L1", 10.0, 20.0 - drop),
            ]
        };
        let pairs_of = |text: &str, other: Vec<Wire>| {
            let r = rule(text);
            let set = WireSet::new(other, 20.0);
            wire_pairs(
                &r,
                &case(0.0),
                &set,
                PairMode::SameLineOtherCase {
                    include_same_phase: false,
                },
            )
            .into_iter()
            .map(|p| (p.a, p.b, p.result.radial.unwrap().distance))
            .collect::<Vec<_>>()
        };
        let base = "[[rule]]\nname = \"x\"\ntarget = \"same_line\"\nradial = 3.0\n";
        // Circuit to circuit: only pairs with B, both orderings.
        let cc = pairs_of(&format!("{base}circuits = \"different\""), case(1.0));
        let found: Vec<(usize, usize)> = cc.iter().map(|p| (p.0, p.1)).collect();
        assert_eq!(found, vec![(1, 2), (2, 1)], "{cc:?}");
        // A2 (cold) to the dropped B1: 6 m across, 1 m down.
        assert!((cc[0].2 - 6.0f64.hypot(1.0)).abs() < 1e-9);
        // Phase to phase: only A1–A2.
        let pp = pairs_of(&format!("{base}circuits = \"same\""), case(1.0));
        let found: Vec<(usize, usize)> = pp.iter().map(|p| (p.0, p.1)).collect();
        assert_eq!(found, vec![(0, 1), (1, 0)], "{pp:?}");
    }

    #[test]
    fn points_vertical_and_radial() {
        // A level wire along x at z = 20.
        let wires = WireSet::new(
            vec![straight("S", "L1", [0.0, 0.0, 20.0], [100.0, 0.0, 20.0])],
            20.0,
        );
        let r = rule(
            "[[rule]]\nname = \"veg\"\ntarget = \"points\"\nclasses = [5]\nvertical = 8.0\nradial = 6.0",
        );
        let mut check = PointCheck::new(&r, &[5], &wires);
        // Tree top 10 m below the wire: vertical and radial 10.
        check.visit(&wires, 50.0, 0.2, 10.0, 5);
        // A tree beside the wire: 5 m to the side at wire height, radial 5
        // (violation), not under the wire (no vertical).
        check.visit(&wires, 60.0, 5.0, 20.0, 5);
        // A branch 3 m above the wire: vertical separation 3 (violation).
        check.visit(&wires, 80.0, -0.3, 23.0, 5);
        // Another class: ignored.
        check.visit(&wires, 70.0, 0.0, 19.0, 2);
        let res = &check.results[0];
        let v = res.vertical.unwrap();
        assert!((v.distance - 3.0).abs() < 1e-9 && v.target[2] == 23.0);
        assert!((res.radial.unwrap().distance - 3.0f64.hypot(0.3)).abs() < 1e-9);
        assert_eq!((res.vertical_violations, res.radial_violations), (1, 2));
        assert!(res.violates());
    }

    #[test]
    fn ground_radial_on_a_slope_is_shorter_than_vertical() {
        // Ground rising across the wire at 45°: z = y. Wire along x at
        // y = 0, z = 10: vertical 10, radial 10·cos 45° = 7.07.
        let mut points = Vec::new();
        for i in 0..=10 {
            for j in -10..=10 {
                let (x, y) = (i as f64 * 10.0, j as f64 * 2.0);
                points.push(SurveyPoint {
                    x,
                    y,
                    z: y,
                    code: String::new(),
                    line: String::new(),
                });
            }
        }
        let terrain = Terrain::from_points(&points, None).unwrap();
        let wires = vec![straight("S", "L1", [10.0, 0.0, 10.0], [90.0, 0.0, 10.0])];
        let r = rule("[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 9.0\nradial = 8.0");
        let res = &ground_check(&r, &wires, &terrain)[0];
        assert!((res.vertical.unwrap().distance - 10.0).abs() < 1e-9);
        let radial = res.radial.unwrap().distance;
        assert!((radial - 10.0 / 2f64.sqrt()).abs() < 1e-9, "{radial}");
        assert!(res.radial_violations > 0 && res.vertical_violations == 0);
    }

    #[test]
    fn ground_radial_beside_a_wire_high_above_the_valley() {
        // Valley floor z = 0 up to y = 4, a cliff rising to z = 20 at
        // y = 6. Wire along x at y = 0, z = 15: vertical 15 (beyond the
        // reach), but the cliff is about 5 m to the side.
        let mut points = Vec::new();
        for i in 0..=10 {
            for j in -5..=5 {
                let (x, y) = (i as f64 * 10.0, j as f64 * 2.0);
                points.push(SurveyPoint {
                    x,
                    y,
                    z: if y <= 4.0 { 0.0 } else { 20.0 },
                    code: String::new(),
                    line: String::new(),
                });
            }
        }
        let terrain = Terrain::from_points(&points, None).unwrap();
        let wires = vec![straight("S", "L1", [10.0, 0.0, 15.0], [90.0, 0.0, 15.0])];
        let r = rule("[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 6.0\nradial = 8.0");
        let res = &ground_check(&r, &wires, &terrain)[0];
        let radial = res.radial.unwrap().distance;
        // To the cliff face from (4, 0) to (6, 20).
        let expected = {
            let (dy, dz) = (2.0f64, 20.0f64);
            let t = ((0.0 - 4.0) * dy + 15.0 * dz) / (dy * dy + dz * dz);
            (4.0 + t * dy).hypot(t * dz - 15.0)
        };
        assert!((radial - expected).abs() < 1e-6, "{radial} vs {expected}");
        assert!(res.radial_violations > 0 && res.vertical_violations == 0);
    }

    #[test]
    fn bundles_are_checked_at_their_subconductors() {
        // Flat ground z = 0; a 4-bundle (0.4 m spacing, a square with a
        // flat bottom: subconductors 0.2 m below and above the centre line)
        // 10 m above it, along x.
        let mut points = Vec::new();
        for i in 0..=10 {
            for j in -10..=10 {
                points.push(SurveyPoint {
                    x: i as f64 * 10.0,
                    y: j as f64 * 5.0,
                    z: 0.0,
                    code: String::new(),
                    line: String::new(),
                });
            }
        }
        let terrain = Terrain::from_points(&points, None).unwrap();
        let square = vec![[0.2, -0.2], [0.2, 0.2], [-0.2, 0.2], [-0.2, -0.2]];
        let bundle = Wire {
            bundle: square.clone(),
            ..straight("B", "L1", [10.0, 0.0, 10.0], [90.0, 0.0, 10.0])
        };
        let g = rule("[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 9.0\nradial = 9.0");
        let res = &ground_check(&g, std::slice::from_ref(&bundle), &terrain)[0];
        assert!((res.vertical.unwrap().distance - 9.8).abs() < 1e-9);
        assert!((res.radial.unwrap().distance - 9.8).abs() < 1e-9);
        // A point 3 m to the right (across, y < 0 looking along +x) at the
        // centre line's height: 2.8 m from the nearest subconductors.
        let pr = rule("[[rule]]\nname = \"p\"\ntarget = \"points\"\nclasses = [5]\nradial = 2.9");
        let set = WireSet::new(vec![bundle.clone()], 10.0);
        let mut pc = PointCheck::new(&pr, &[5], &set);
        pc.visit(&set, 50.0, -3.0, 10.0, 5);
        let hit = pc.results[0].radial.unwrap();
        assert!((hit.distance - 2.8f64.hypot(0.2)).abs() < 1e-9, "{hit:?}");
        assert!((hit.wire[1] + 0.2).abs() < 1e-9);
        assert!(pc.results[0].violates());
        // A point straight below: vertical to the lowest subconductors.
        let vr = rule("[[rule]]\nname = \"v\"\ntarget = \"points\"\nclasses = [5]\nvertical = 3.0");
        let mut pc = PointCheck::new(&vr, &[5], &set);
        pc.visit(&set, 50.0, 0.0, 7.0, 5);
        assert!((pc.results[0].vertical.unwrap().distance - 2.8).abs() < 1e-9);
        // Two bundles crossing 3 m apart (centre lines): 2.6 m between the
        // upper one's bottom and the lower one's top.
        let lower = WireSet::new(
            vec![Wire {
                bundle: square,
                ..straight("C", "L1", [50.0, -20.0, 7.0], [50.0, 20.0, 7.0])
            }],
            10.0,
        );
        let x =
            rule("[[rule]]\nname = \"x\"\ntarget = \"other_lines\"\nvertical = 3.0\nradial = 3.0");
        let pairs = wire_pairs(&x, &[bundle], &lower, PairMode::OtherLines);
        let r = &pairs[0].result;
        assert!((r.vertical.unwrap().distance - 2.6).abs() < 1e-9);
        assert!((r.radial.unwrap().distance - 2.6).abs() < 1e-9);
    }

    #[test]
    fn requirements_follow_each_wires_voltage() {
        // Flat ground at z = 0; three wires 6.5 m above it: 132 kV, 380 kV
        // and an earth wire (no voltage).
        let mut points = Vec::new();
        for i in 0..=10 {
            for j in -10..=10 {
                points.push(SurveyPoint {
                    x: i as f64 * 10.0,
                    y: j as f64 * 5.0,
                    z: 0.0,
                    code: String::new(),
                    line: String::new(),
                });
            }
        }
        let terrain = Terrain::from_points(&points, None).unwrap();
        let wire = |name: &str, y: f64, kv: Option<f64>| Wire {
            potential: kv.map(ac),
            ..straight(name, "L1", [10.0, y, 6.5], [90.0, y, 6.5])
        };
        let wires = vec![
            wire("S132", -10.0, Some(132.0)),
            wire("S380", 0.0, Some(380.0)),
            wire("EW", 10.0, None),
        ];
        let r = rule(
            "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = { by_voltage = [[0, 5.0], [150, 6.0], [420, 7.0]] }",
        );
        let res = ground_check(&r, &wires, &terrain);
        let req: Vec<Option<f64>> = res.iter().map(|x| x.required.vertical).collect();
        assert_eq!(req, [Some(6.0), Some(7.0), Some(5.0)]);
        let bad: Vec<bool> = res.iter().map(WireResult::violates).collect();
        assert_eq!(bad, [false, true, false]);
        assert!(res[1].vertical_violations > 0 && res[0].vertical_violations == 0);

        // A point 6.8 m from each wire: below the 380 kV radial
        // requirement only (formula 5 m + 0.01 m/kV above 110 kV).
        let pr = rule(
            "[[rule]]\nname = \"v\"\ntarget = \"points\"\nclasses = [5]\nradial = { base = 5.0, per_kv = 0.01, above_kv = 110 }",
        );
        let set = WireSet::new(wires.clone(), 20.0);
        let mut pc = PointCheck::new(&pr, &[5], &set);
        for y in [-10.0, 0.0, 10.0] {
            pc.visit(&set, 50.0, y, 6.5 - 6.8, 5);
        }
        let req: Vec<Option<f64>> = pc.results.iter().map(|x| x.required.radial).collect();
        assert!((req[0].unwrap() - 5.22).abs() < 1e-12);
        assert!((req[1].unwrap() - 7.7).abs() < 1e-12);
        assert_eq!(req[2], Some(5.0));
        let bad: Vec<bool> = pc.results.iter().map(WireResult::violates).collect();
        assert_eq!(bad, [false, true, false]);

        // Wire pairs: the higher voltage of the pair decides.
        let x = rule(
            "[[rule]]\nname = \"x\"\ntarget = \"other_lines\"\nradial = { by_voltage = [[150, 3.0], [420, 4.0]] }",
        );
        let other = WireSet::new(
            vec![Wire {
                potential: Some(ac(110.0)),
                ..straight("O", "L1", [50.0, -13.5, 6.5], [50.0, 13.5, 6.5])
            }],
            20.0,
        );
        let a = vec![
            Wire {
                potential: Some(ac(132.0)),
                ..straight("A", "L1", [0.0, -3.5, 10.0], [100.0, -3.5, 10.0])
            },
            Wire {
                potential: Some(ac(380.0)),
                ..straight("B", "L1", [0.0, 3.5, 10.0], [100.0, 3.5, 10.0])
            },
        ];
        // Both 3.5 m above the other line's wire where they cross.
        let pairs = wire_pairs(&x, &a, &other, PairMode::OtherLines);
        let got: Vec<(Option<f64>, bool)> = pairs
            .iter()
            .map(|p| (p.result.required.radial, p.result.violates()))
            .collect();
        assert_eq!(got, [(Some(3.0), false), (Some(4.0), true)]);
    }

    #[test]
    fn wires_of_the_same_line_and_other_lines() {
        // Two phases 4 m apart in the same span; a crossing wire 5 m below.
        let line = vec![
            straight("S1", "L1", [0.0, 0.0, 20.0], [100.0, 0.0, 20.0]),
            straight("S2", "L2", [0.0, 4.0, 20.0], [100.0, 4.0, 20.0]),
            // Same phase as S1 (e.g. its next section): skipped by default.
            straight("S3", "L1", [100.0, 0.0, 20.0], [200.0, 0.0, 20.0]),
        ];
        let same = WireSet::new(line.clone(), 20.0);
        let pp = rule("[[rule]]\nname = \"pp\"\ntarget = \"same_line\"\nradial = 5.0");
        let pairs = wire_pairs(
            &pp,
            &line,
            &same,
            PairMode::SameLine {
                include_same_phase: false,
            },
        );
        assert_eq!(pairs.len(), 2, "{pairs:?}"); // S1–S2, S2–S3; not S1–S3
        let s1s2 = pairs.iter().find(|p| (p.a, p.b) == (0, 1)).unwrap();
        assert!((s1s2.result.radial.unwrap().distance - 4.0).abs() < 1e-9);
        assert!(s1s2.result.violates());
        // With same-phase pairs included, S1–S3 is still skipped: they
        // meet end to end (connected).
        assert_eq!(
            wire_pairs(
                &pp,
                &line,
                &same,
                PairMode::SameLine {
                    include_same_phase: true
                }
            )
            .len(),
            2
        );

        let other = WireSet::new(
            vec![straight(
                "X1",
                "L1",
                [50.0, -50.0, 15.0],
                [50.0, 50.0, 15.0],
            )],
            20.0,
        );
        let cross = rule("[[rule]]\nname = \"x\"\ntarget = \"other_lines\"\nvertical = 6.0");
        let pairs = wire_pairs(&cross, &line, &other, PairMode::OtherLines);
        // Crosses S1 and S2 (5 m below each), not S3.
        assert_eq!(pairs.len(), 2);
        for p in &pairs {
            assert!((p.result.vertical.unwrap().distance - 5.0).abs() < 1e-9);
            // Counted per wire segment: the crossing is at a polyline
            // vertex of the line's wire here, so both segments touch it.
            assert!((1..=2).contains(&p.result.vertical_violations));
            assert!(p.result.violates());
        }
        assert!(matches!(cross.target, Target::OtherLines));
        // A crossing far below (30 m, beyond the reach) is still reported.
        let deep = WireSet::new(
            vec![straight(
                "X2",
                "L1",
                [50.0, -50.0, -10.0],
                [50.0, 50.0, -10.0],
            )],
            20.0,
        );
        let pairs = wire_pairs(&cross, &line, &deep, PairMode::OtherLines);
        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().all(|p| p.result.radial.is_none()
            && (p.result.vertical.unwrap().distance - 30.0).abs() < 1e-9));
        // Connected wires (a jumper from S1's end) are not checked together.
        let mut with_jumper = line.clone();
        with_jumper.push(straight(
            "jumper:J",
            "L9",
            [100.0, 0.0, 20.0],
            [100.0, 2.0, 18.0],
        ));
        let set = WireSet::new(with_jumper.clone(), 20.0);
        let pairs = wire_pairs(
            &pp,
            &with_jumper,
            &set,
            PairMode::SameLine {
                include_same_phase: true,
            },
        );
        assert!(
            pairs.iter().all(|p| !(p.b == 3 && (p.a == 0 || p.a == 2))),
            "{pairs:?}"
        );
        assert!(pairs.iter().any(|p| (p.a, p.b) == (1, 3)));
    }
}
