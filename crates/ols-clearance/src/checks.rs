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
use ols_line::jumper::JumperShape;
use ols_line::solve::SectionShape;
use ols_terrain::Terrain;

use crate::geometry::{
    P3, closest_between_segments, closest_on_segment, closest_on_triangle, crossing_in_plan,
    distance, over_in_plan,
};
use crate::rules::Rule;

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
    /// Points along the wire.
    pub points: Vec<P3>,
}

/// The wires of a solved line: every span of every section, and every
/// jumper (with the circuit and phase of the section at its `from` end).
pub fn wires_of(line: &Line, shapes: &[SectionShape], jumpers: &[JumperShape]) -> Vec<Wire> {
    let mut wires = Vec::new();
    for shape in shapes {
        let section = line.sections.iter().find(|s| s.id == shape.section);
        for (k, span) in shape.spans.iter().enumerate() {
            wires.push(Wire {
                section: shape.section.clone(),
                span: k,
                circuit: section.map_or(String::new(), |s| s.circuit.clone()),
                phase: section.map_or(String::new(), |s| s.phase_label.clone()),
                points: span.points.clone(),
            });
        }
    }
    for j in jumpers {
        let from = line
            .jumpers
            .iter()
            .find(|d| d.id == j.jumper)
            .and_then(|d| line.sections.iter().find(|s| s.points.contains(&d.from)));
        wires.push(Wire {
            section: format!("jumper:{}", j.jumper),
            span: 0,
            circuit: from.map_or(String::new(), |s| s.circuit.clone()),
            phase: from.map_or(String::new(), |s| s.phase_label.clone()),
            points: j.points.clone(),
        });
    }
    wires
}

/// Wires with a plan grid of their segments, each segment listed in every
/// cell within `reach` of it.
#[derive(Debug, Clone)]
pub struct WireSet {
    /// The wires.
    pub wires: Vec<Wire>,
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
    fn near(&self, x: f64, y: f64) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.cells
            .get(&(cell_of(x), cell_of(y)))
            .into_iter()
            .flatten()
            .map(|&id| {
                let (w, k) = self.segments[id as usize];
                (w as usize, k as usize)
            })
    }

    fn segment(&self, w: usize, k: usize) -> (P3, P3) {
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
}

impl WireResult {
    fn vertical(&mut self, hit: Hit, required: Option<f64>) -> bool {
        if self.vertical.is_none_or(|h| hit.distance < h.distance) {
            self.vertical = Some(hit);
        }
        required.is_some_and(|r| hit.distance < r)
    }

    fn radial(&mut self, hit: Hit, required: Option<f64>) -> bool {
        if self.radial.is_none_or(|h| hit.distance < h.distance) {
            self.radial = Some(hit);
        }
        required.is_some_and(|r| hit.distance < r)
    }

    /// Whether a requirement of `rule` is not met.
    pub fn violates(&self, rule: &Rule) -> bool {
        let below =
            |hit: Option<Hit>, req: Option<f64>| hit.zip(req).is_some_and(|(h, r)| h.distance < r);
        below(self.vertical, rule.vertical) || below(self.radial, rule.radial)
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
            results: vec![WireResult::default(); wires.wires.len()],
            seen: Vec::new(),
        }
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
        let reach = self.rule.reach().min(wires.reach);
        // Wires this point violates (each counted once per point).
        let mut vertical_bad: Vec<usize> = Vec::new();
        let mut radial_bad: Vec<usize> = Vec::new();
        self.seen.clear();
        for (w, k) in wires.near(x, y) {
            let (a, b) = wires.segment(w, k);
            let (q, _) = closest_on_segment(p, a, b);
            let d = distance(p, q);
            let result = &mut self.results[w];
            if d <= reach {
                let hit = Hit {
                    distance: d,
                    wire: q,
                    target: p,
                    class: Some(class),
                };
                if result.radial(hit, self.rule.radial) && !radial_bad.contains(&w) {
                    radial_bad.push(w);
                }
            }
            if let Some((q, _)) = over_in_plan(x, y, a, b, self.rule.vertical_band) {
                // Vertical separation, above or below the wire (a tree
                // overhanging a low wire, a line crossing over).
                let hit = Hit {
                    distance: (q[2] - z).abs(),
                    wire: q,
                    target: p,
                    class: Some(class),
                };
                if hit.distance <= reach
                    && result.vertical(hit, self.rule.vertical)
                    && !vertical_bad.contains(&w)
                {
                    vertical_bad.push(w);
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

/// Vertical and radial clearances of every wire to the ground surface.
/// Vertical: under every wire piece at every TIN edge crossing (exact for
/// the polyline). Radial: wire samples every [`GROUND_STEP`] to the
/// triangles near them, searched only where the ground is within the
/// rule's reach below the wire (the radial distance is never larger than
/// the vertical one).
pub fn ground_check(rule: &Rule, wires: &[Wire], terrain: &Terrain) -> Vec<WireResult> {
    let reach = rule.reach();
    wires
        .iter()
        .map(|wire| {
            let mut result = WireResult::default();
            for s in wire.points.windows(2) {
                let (a, b) = (s[0], s[1]);
                let (mut vertical_bad, mut radial_bad) = (false, false);
                for run in terrain.profile([a[0], a[1]], [b[0], b[1]]) {
                    for g in run {
                        let z = a[2] + (b[2] - a[2]) * g.t;
                        let hit = Hit {
                            distance: z - g.point[2],
                            wire: [g.point[0], g.point[1], z],
                            target: g.point,
                            class: None,
                        };
                        vertical_bad |= result.vertical(hit, rule.vertical);
                    }
                }
                result.vertical_violations += u64::from(vertical_bad);
                if rule.radial.is_none() {
                    continue;
                }
                let n = ((distance(a, b) / GROUND_STEP).ceil() as usize).max(1);
                for i in 0..=n {
                    let t = i as f64 / n as f64;
                    let p = [
                        a[0] + (b[0] - a[0]) * t,
                        a[1] + (b[1] - a[1]) * t,
                        a[2] + (b[2] - a[2]) * t,
                    ];
                    let Some(below) = terrain.height(p[0], p[1]) else {
                        continue;
                    };
                    let radius = (p[2] - below).max(0.0).min(reach);
                    if p[2] - below > reach {
                        continue;
                    }
                    let mut best: Option<(f64, P3)> = None;
                    terrain.triangles_near(p[0], p[1], radius, |[ta, tb, tc]| {
                        let q = closest_on_triangle(p, ta, tb, tc);
                        let d = distance(p, q);
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, q));
                        }
                    });
                    if let Some((d, q)) = best {
                        let d = if p[2] < below { -d } else { d };
                        let hit = Hit {
                            distance: d,
                            wire: p,
                            target: q,
                            class: None,
                        };
                        radial_bad |= result.radial(hit, rule.radial);
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

/// Clearances between the wires of `a` and those of the indexed set `b`:
/// radial for pairs closer than the rule's reach, vertical wherever they
/// cross in plan (crossing clearances are always reported). With
/// `same_set`, `b` holds the same wires as `a`: each pair is checked once,
/// a wire not with itself, and wires of the same circuit and phase only if
/// `include_same_phase`. Wires sharing an end point (within 1 cm) are
/// connected — adjacent spans, a jumper and its sections — and not checked
/// against each other.
pub fn wire_pairs(
    rule: &Rule,
    a: &[Wire],
    b: &WireSet,
    same_set: bool,
    include_same_phase: bool,
) -> Vec<PairResult> {
    let reach = rule.reach().min(b.reach);
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
                if same_set {
                    let other = &b.wires[j];
                    if j <= i
                        || (!include_same_phase
                            && other.circuit == wire.circuit
                            && other.phase == wire.phase)
                    {
                        continue;
                    }
                }
                if connected(wire, &b.wires[j]) {
                    continue;
                }
                let (c, d) = b.segment(j, k);
                let (on_a, on_b) = closest_between_segments(p, q, c, d);
                let dist = distance(on_a, on_b);
                let crossing = crossing_in_plan(p, q, c, d);
                if dist > reach && crossing.is_none() {
                    continue;
                }
                let result = pairs.entry((i, j)).or_default();
                let mut radial_bad = false;
                if dist <= reach {
                    let hit = Hit {
                        distance: dist,
                        wire: on_a,
                        target: on_b,
                        class: None,
                    };
                    radial_bad = result.radial(hit, rule.radial);
                }
                let mut vertical_bad = false;
                if let Some((x, y)) = crossing {
                    let (upper, lower) = if x[2] >= y[2] { (x, y) } else { (y, x) };
                    let hit = Hit {
                        distance: upper[2] - lower[2],
                        wire: x,
                        target: y,
                        class: None,
                    };
                    vertical_bad = result.vertical(hit, rule.vertical);
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
        }
    }

    fn rule(text: &str) -> Rule {
        RuleSet::parse(text).unwrap().rules.remove(0)
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
        assert!(res.violates(&r));
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
        let pairs = wire_pairs(&pp, &line, &same, true, false);
        assert_eq!(pairs.len(), 2, "{pairs:?}"); // S1–S2, S2–S3; not S1–S3
        let s1s2 = pairs.iter().find(|p| (p.a, p.b) == (0, 1)).unwrap();
        assert!((s1s2.result.radial.unwrap().distance - 4.0).abs() < 1e-9);
        assert!(s1s2.result.violates(&pp));
        // With same-phase pairs included, S1–S3 is still skipped: they
        // meet end to end (connected).
        assert_eq!(wire_pairs(&pp, &line, &same, true, true).len(), 2);

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
        let pairs = wire_pairs(&cross, &line, &other, false, false);
        // Crosses S1 and S2 (5 m below each), not S3.
        assert_eq!(pairs.len(), 2);
        for p in &pairs {
            assert!((p.result.vertical.unwrap().distance - 5.0).abs() < 1e-9);
            // Counted per wire segment: the crossing is at a polyline
            // vertex of the line's wire here, so both segments touch it.
            assert!((1..=2).contains(&p.result.vertical_violations));
            assert!(p.result.violates(&cross));
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
        let pairs = wire_pairs(&cross, &line, &deep, false, false);
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
        let pairs = wire_pairs(&pp, &with_jumper, &set, true, true);
        assert!(
            pairs.iter().all(|p| !(p.b == 3 && (p.a == 0 || p.a == 2))),
            "{pairs:?}"
        );
        assert!(pairs.iter().any(|p| (p.a, p.b) == (1, 3)));
    }
}
