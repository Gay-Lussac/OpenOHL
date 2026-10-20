// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **swing arc** in the vertical and radial clearance checks
//! ([`crate::zone`]; theory: LiDAR analyses › Danger trees › Wire
//! positions checked, and Terrain › Vertical and radial clearances on the
//! swing arc): each span of a windy case swung as one about its
//! attachment axis between still air and the full blowouts of the sides
//! chosen, at the largest of their radii — the danger-tree check's arc
//! ([`SwingArc`]), shared, not copied.
//!
//! - **LiDAR points**: one closed form per point and span — radially
//!   [`SwingArc::nearest`], vertically [`SwingArc::vertical`] (the arc's
//!   lowest point over the point's plan position within the zone's
//!   width; its highest for a point above).
//! - **The ground** (a TIN): the wire swung to angles every Δφ, each
//!   checked as a solved wire ([`crate::checks::ground_check`]'s check,
//!   exact for its polyline), Δφ such that every point of the arc lies
//!   within [`ARC_TOLERANCE`] of one of them at its station.
//!   **Conservative:** each is taken [`ARC_TOLERANCE`] thicker (its
//!   clearance that much smaller), vertically each piece also that times
//!   its slope lower, and the zone that much wider and deeper on each
//!   side.

use std::collections::{HashMap, HashSet};

use ohl_line::Line;
use ohl_line::solve::SectionShape;
use ohl_terrain::Terrain;
use ohl_weather::WindSide;

use crate::checks::{
    GroundMax, Hit, PointCheck, PointEvent, Tally, Wire, WireResult, checked_parts, ground_wire,
    wires_of,
};
use crate::geometry::P3;
use crate::positions::{SwingArc, WireShape, span_axis};
use crate::rules::Rule;
use crate::zone::WindState;

/// How close the wires swung to angles every Δφ come to every point of
/// a swing arc, for the ground check, m (taken off their clearances).
pub const ARC_TOLERANCE: f64 = 0.02;

/// Plan grid cell of the arcs' index, m.
const CELL: f64 = 10.0;

/// The swing arc of a windy case: the wind positions it is built from.
#[derive(Debug, Clone, PartialEq)]
pub struct ArcCase {
    /// The weather case's name.
    pub name: String,
    /// Its still-air position (index in the states).
    pub still: usize,
    /// Blown to the left (the wind from the right) at the case's full
    /// pressure.
    pub left: Option<usize>,
    /// Blown to the right (the wind from the left).
    pub right: Option<usize>,
}

/// The swing arcs of `states` ([`crate::zone::wind_states`] with the arc
/// on): of every windy case with a still-air position and a full blowout
/// from at least one side.
pub fn arc_cases(states: &[WindState]) -> Vec<ArcCase> {
    let mut out = Vec::new();
    for (k, s) in states.iter().enumerate() {
        if !(s.full > 0.0 && s.side.is_none() && s.pressure == 0.0) {
            continue;
        }
        let find = |side: WindSide| {
            states.iter().position(|t| {
                t.name == s.name && t.side == Some(side) && (t.pressure - t.full).abs() < 1e-9
            })
        };
        let (left, right) = (find(WindSide::Right), find(WindSide::Left));
        if left.is_some() || right.is_some() {
            out.push(ArcCase {
                name: s.name.clone(),
                still: k,
                left,
                right,
            });
        }
    }
    out
}

/// A span's swing arc in a check.
#[derive(Debug, Clone)]
pub struct ZoneArc {
    /// Its case ([`ArcCase`], index).
    pub case: usize,
    /// The case's still-air position (index in the states).
    pub state: usize,
    /// The span's wire in still air (section, span, voltage, bundle…).
    pub wire: Wire,
    /// The arc.
    pub arc: SwingArc,
    /// Its largest radius, m.
    pub radius: f64,
}

/// The swing arcs of every span of the sections solved (`shapes`, per
/// state, as [`crate::zone::solve_states`] gives them) for `cases`, and
/// what could not be made (jumpers have none: they are checked at their
/// solved positions).
pub fn zone_arcs(
    line: &Line,
    shapes: &[Vec<SectionShape>],
    cases: &[ArcCase],
) -> (Vec<ZoneArc>, Vec<String>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for (c, case) in cases.iter().enumerate() {
        let wires = |k: Option<usize>| k.map(|k| wires_of(line, &shapes[k], &[]));
        let still = wires(Some(case.still)).unwrap_or_default();
        let (left, right) = (wires(case.left), wires(case.right));
        let mut left_out = 0;
        for w in &still {
            let pick = |list: &Option<Vec<Wire>>| -> Option<Option<Wire>> {
                match list {
                    None => Some(None),
                    Some(l) => l
                        .iter()
                        .find(|x| x.section == w.section && x.span == w.span)
                        .cloned()
                        .map(Some),
                }
            };
            let (Some(l), Some(r)) = (pick(&left), pick(&right)) else {
                left_out += 1;
                continue;
            };
            let Some(axis) = shapes[case.still]
                .iter()
                .find(|s| s.section == w.section)
                .and_then(|s| span_axis(s, w.span))
            else {
                left_out += 1;
                continue;
            };
            let shape = |x: &Wire| WireShape::new(x.clone(), &axis);
            let mid = shape(w);
            let l = l.as_ref().map_or_else(|| mid.clone(), shape);
            let r = r.as_ref().map_or_else(|| mid.clone(), shape);
            match SwingArc::new(axis, &l, &mid, &r) {
                Some(arc) => {
                    let radius = arc.extent().0;
                    out.push(ZoneArc {
                        case: c,
                        state: case.still,
                        wire: w.clone(),
                        arc,
                        radius,
                    });
                }
                None => left_out += 1,
            }
        }
        if left_out > 0 {
            notes.push(format!(
                "\"{}\": {left_out} spans without a swing arc (a wire not solved, or not running \
                 steadily along its span)",
                case.name
            ));
        }
    }
    (out, notes)
}

/// The wires of a rule's check among `arcs`: (arc index) of those the rule
/// checks (its voltages and wires).
pub(crate) fn arcs_of_rule(def: &Rule, arcs: &[ZoneArc], used: &[usize]) -> Vec<usize> {
    arcs.iter()
        .enumerate()
        .filter(|(_, a)| used.contains(&a.state))
        .filter(|(_, a)| checked_parts(def, std::slice::from_ref(&a.wire)).len() == 1)
        .map(|(k, _)| k)
        .collect()
}

/// The polylines in plan of the arcs, for reading the points around them:
/// the wire swung to angles whose plan positions are at most 1 m apart.
pub fn arc_polylines(arcs: &[ZoneArc]) -> Vec<Vec<[f64; 2]>> {
    let mut out = Vec::new();
    for a in arcs {
        let (r, from, to) = a.arc.extent();
        let n = (((to - from) * r).ceil().max(1.0) as usize).min(1000);
        for k in 0..=n {
            let phi = from + (to - from) * k as f64 / n as f64;
            out.push(a.arc.line_at(phi).iter().map(|p| [p[0], p[1]]).collect());
        }
    }
    out
}

/// A rule's check of LiDAR points against swing arcs: one result per arc
/// (with its span's group), the arcs indexed in plan.
#[derive(Debug, Clone)]
pub(crate) struct ArcPointCheck {
    /// The arcs checked (index in the arcs).
    pub items: Vec<usize>,
    /// The group (span) of each.
    pub groups: Vec<usize>,
    /// One result per arc.
    pub results: Vec<WireResult>,
    cells: HashMap<(i64, i64), Vec<u32>>,
    reach: f64,
    band: f64,
    depth: f64,
    margin: f64,
    /// The smallest clearance of each arc so far (vertical, radial).
    pub best: Vec<[f64; 2]>,
    /// The measures worked out (vertical, radial).
    measures: [bool; 2],
}

fn cell_of(v: f64) -> i64 {
    (v / CELL).floor() as i64
}

impl ArcPointCheck {
    /// The check of rule `def` on `items` of `arcs` (groups `groups`), up
    /// to `reach` m.
    pub fn new(
        def: &Rule,
        arcs: &[ZoneArc],
        items: Vec<usize>,
        groups: Vec<usize>,
        reach: f64,
    ) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        let mut results = Vec::new();
        for (i, &k) in items.iter().enumerate() {
            let a = &arcs[k];
            results.push(WireResult::with(def.required(a.wire.kv())));
            let grow = a.radius + a.arc.outer + reach + def.vertical_band + def.vertical_depth;
            let mut mine: HashSet<(i64, i64)> = HashSet::new();
            let centres: Vec<P3> = a
                .arc
                .stations
                .iter()
                .map(|&s| a.arc.place(s).centre)
                .collect();
            for w in centres.windows(2) {
                let (x0, x1) = (w[0][0].min(w[1][0]) - grow, w[0][0].max(w[1][0]) + grow);
                let (y0, y1) = (w[0][1].min(w[1][1]) - grow, w[0][1].max(w[1][1]) + grow);
                for i in cell_of(x0)..=cell_of(x1) {
                    for j in cell_of(y0)..=cell_of(y1) {
                        mine.insert((i, j));
                    }
                }
            }
            for c in mine {
                cells.entry(c).or_default().push(i as u32);
            }
        }
        Self {
            best: vec![[f64::INFINITY; 2]; items.len()],
            items,
            groups,
            results,
            cells,
            reach,
            band: def.vertical_band,
            depth: def.vertical_depth,
            margin: def.search_margin,
            measures: [def.vertical.is_some(), def.radial.is_some()],
        }
    }

    /// What the point (x, y, z) of `class` does to the arcs' results,
    /// given the smallest clearances found before it (`best`, updated),
    /// pushed on `out` ([`PointCheck::eval`]: the same events; with a
    /// `tally`, a span below the requirement counted there, once per
    /// point `stamp`). The class is the caller's to check.
    #[allow(clippy::too_many_arguments)]
    pub fn eval(
        &self,
        arcs: &[ZoneArc],
        best: &mut [[f64; 2]],
        mut tally: Option<&mut Tally>,
        stamp: u64,
        q: [f64; 3],
        class: u8,
        out: &mut Vec<PointEvent>,
    ) {
        let [x, y, _] = q;
        let Some(list) = self.cells.get(&(cell_of(x), cell_of(y))) else {
            return;
        };
        for &i in list {
            let i = i as usize;
            let a = &arcs[self.items[i]];
            let required = self.results[i].required;
            let reach = (required.largest() + self.margin).min(self.reach);
            // The capsule: nothing of the arc nearer than this.
            if a.arc.axis.distance(q) - a.radius - a.arc.outer - self.band - self.depth > reach {
                continue;
            }
            let mut record = |m: usize, hit: Hit| {
                let improved = hit.distance < best[i][m];
                if improved {
                    best[i][m] = hit.distance;
                }
                let req = if m == 0 {
                    required.vertical
                } else {
                    required.radial
                };
                let mut bad = req.is_some_and(|r| hit.distance < r);
                if bad && let Some(t) = tally.as_deref_mut() {
                    // Counted in the stretch's tally, once per point.
                    t.group_bad(self.groups[i], m, stamp);
                    bad = false;
                }
                if improved || bad {
                    out.push(PointEvent {
                        item: i as u32,
                        measure: m as u8,
                        bad,
                        hit: improved.then_some(hit),
                    });
                }
            };
            if self.measures[1] {
                let (d, p) = a.arc.nearest(q);
                let d = d - a.arc.outer;
                if d <= reach {
                    record(
                        1,
                        Hit {
                            distance: d,
                            wire: p,
                            target: q,
                            class: Some(class),
                        },
                    );
                }
            }
            if self.measures[0]
                && let Some(v) = a.arc.vertical(q, self.band, self.depth)
            {
                let hit = Hit {
                    distance: (v.gap - a.arc.outer).max(0.0),
                    wire: v.point,
                    target: q,
                    class: Some(class),
                };
                if hit.distance <= reach {
                    record(0, hit);
                }
            }
        }
    }

    /// Applies one point's events (after `points` applied its own): the
    /// smaller clearances kept, the point counted once per span below a
    /// requirement.
    pub fn apply(&mut self, events: &[PointEvent], points: &mut PointCheck) {
        for e in events {
            let (i, m) = (e.item as usize, e.measure as usize);
            if let Some(h) = e.hit {
                let r = &mut self.results[i];
                let kept = if m == 0 {
                    &mut r.vertical
                } else {
                    &mut r.radial
                };
                if kept.is_none_or(|k| h.distance < k.distance) {
                    *kept = Some(h);
                    self.best[i][m] = h.distance;
                }
            }
            if e.bad {
                points.count_in_group(self.groups[i], m);
            }
        }
    }
}

/// Rule `def`'s clearances of arc `a` to the ground: the wire swung to
/// angles every Δφ ≤ 2e / R (e = [`ARC_TOLERANCE`], R the arc's largest
/// radius), each checked as a solved wire ([`ground_wire`], exact for its
/// polyline): every point of the arc lies within e of one of them at its
/// station. Each is taken e thicker, in a zone e wider (and 2e deeper);
/// vertically each piece also e·s lower (s its rise per plan length: an
/// arc point e away from the wire's point lies over ground the piece
/// passes over up to e·s higher or lower; s the steeper of the piece and
/// its neighbours). The lowest is kept (the counts:
/// of the worst angle). The angles nearest the hanging position first; a
/// piece that cannot come below the lowest found so far nor below the
/// requirement is passed over (its bound from the highest ground near
/// it).
pub(crate) fn arc_ground(
    def: &Rule,
    a: &ZoneArc,
    terrain: &Terrain,
    top: &GroundMax,
) -> WireResult {
    let e = ARC_TOLERANCE;
    let (r, from, to) = a.arc.extent();
    let n = if to > from && r > 0.0 {
        (((to - from) * r / (2.0 * e)).ceil() as usize).clamp(1, 20_000)
    } else {
        0
    };
    let wide = Rule {
        vertical_band: def.vertical_band + e,
        vertical_depth: if def.vertical_depth > 0.0 {
            def.vertical_depth + 2.0 * e
        } else {
            0.0
        },
        ..def.clone()
    };
    let mut angles: Vec<f64> = (0..=n)
        .map(|k| {
            if n == 0 {
                from
            } else {
                from + (to - from) * k as f64 / n as f64
            }
        })
        .collect();
    angles.sort_by(|x, y| x.abs().total_cmp(&y.abs()));
    let mut best = WireResult::with(def.required(a.wire.kv()));
    for phi in angles {
        let line = Wire {
            points: a.arc.line_at(phi),
            polyline_error: a.arc.allowance() - a.wire.surface_radius + e,
            ..a.wire.clone()
        };
        // Per piece: e times its rise per plan length.
        let slope: Vec<f64> = line
            .points
            .windows(2)
            .map(|w| {
                let plan = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
                if plan > 1e-12 {
                    e * (w[1][2] - w[0][2]).abs() / plan
                } else {
                    // A vertical piece (on the axis at an end): its
                    // whole rise.
                    (w[1][2] - w[0][2]).abs()
                }
            })
            .collect();
        // A point near a station may lie over the next piece: the larger
        // of the neighbours'.
        let slope: Vec<f64> = (0..slope.len())
            .map(|k| {
                let lo = k.saturating_sub(1);
                let hi = (k + 1).min(slope.len() - 1);
                slope[lo].max(slope[k]).max(slope[hi])
            })
            .collect();
        let bound = [
            best.vertical.map_or(f64::INFINITY, |h| h.distance),
            best.radial.map_or(f64::INFINITY, |h| h.distance),
        ];
        let r = ground_wire(&wide, &line, terrain, top, bound, Some(&slope));
        if let Some(h) = r.vertical
            && best.vertical.is_none_or(|b| h.distance < b.distance)
        {
            best.vertical = Some(h);
        }
        if let Some(h) = r.radial
            && best.radial.is_none_or(|b| h.distance < b.distance)
        {
            best.radial = Some(h);
        }
        best.vertical_violations = best.vertical_violations.max(r.vertical_violations);
        best.radial_violations = best.radial_violations.max(r.radial_violations);
    }
    best
}
