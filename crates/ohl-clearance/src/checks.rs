// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Vertical and radial clearances of wires to the ground, to points and to
//! other wires (DESIGN_GOALS §13).
//!
//! Wires are the solved spans and jumpers as dense polylines on the exact
//! catenary ([`wires_of`]); their segments are indexed on a plan grid, so
//! each target (a LiDAR point, a ground triangle, another wire's segment)
//! is compared only with the wire segments near it.
//!
//! **To the surface.** Every clearance is measured to the conductor's
//! surface: the distance from its axis (each subconductor's, for a
//! bundle) less [`Wire::allowance`] — the subconductor's radius (iced in a
//! case with ice) and the most the wire lies off its polyline (the bound
//! of the solver, [`ohl_line::solve::polyline_error`]); between two wires,
//! both wires' allowances. So the polyline can only make a clearance
//! smaller than the exact one, never larger.
//!
//! For the ground the wire is sampled every [`GROUND_STEP`] for the radial
//! distance; the shortest sample distance \\( D \\) is lowered to
//! \\( \sqrt{D^2 - (s/2)^2} \\) for samples \\( s \\) apart, a lower
//! bound of the distance from the continuous piece ([`sampled_floor`]).

use std::collections::HashMap;

use ohl_line::Line;
use ohl_line::electrical::Potential;
use ohl_line::jumper::JumperShape;
use ohl_line::solve::SectionShape;
use ohl_terrain::Terrain;

use crate::geometry::{
    P3, closest_between_segments, closest_on_segment, closest_on_triangle, crossing_in_plan,
    distance, over_in_plan,
};
use crate::rules::{Required, Rule};

/// Spacing of wire samples for the radial distance to the ground, m: the
/// closest sample is at most s²/(8d) farther than the wire (3 mm at
/// d = 10 m), and [`sampled_floor`] takes that off.
pub const GROUND_STEP: f64 = 0.5;

/// A lower bound of the distance from a straight wire piece to a surface,
/// m, from the shortest distance `d` of samples `spacing` apart along it
/// (its ends sampled): \\( \sqrt{d^2 - (s/2)^2} \\), 0 below \\( s/2 \\).
///
/// Proof: let \\( p^* \\) be the piece's closest point to the surface, at
/// distance \\( d^* \\) from its nearest point \\( q \\). If \\( p^* \\) is
/// inside the piece, the piece is square to \\( p^* - q \\) there (else
/// moving along it would come nearer \\( q \\)); a sample lies within
/// \\( s/2 \\) of \\( p^* \\) along the piece, so its distance is at most
/// \\( \sqrt{d^{*2} + (s/2)^2} \\), and \\( d \\) is too. At an end, the
/// end is a sample: \\( d = d^* \\).
pub fn sampled_floor(d: f64, spacing: f64) -> f64 {
    let h = spacing / 2.0;
    if d <= 0.0 {
        return d;
    }
    (d * d - h * h).max(0.0).sqrt()
}

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
    /// m ([`ohl_line::Section::bundle_offsets`]); empty or `[[0, 0]]` for a
    /// single wire.
    pub bundle: Vec<[f64; 2]>,
    /// Points along the wire.
    pub points: Vec<P3>,
    /// The attachments it hangs on (`structure:set:phase`): wires sharing
    /// one are connected (adjacent spans, a jumper and its sections).
    pub ends: Vec<String>,
    /// Radius of a subconductor's surface, m: half its cable's diameter,
    /// iced in a case with ice (0: measured to the axis).
    pub surface_radius: f64,
    /// The most the wire lies off its polyline `points`, m.
    pub polyline_error: f64,
}

impl Wire {
    /// Nominal voltage of its system, kV (none: earthed).
    pub fn kv(&self) -> Option<f64> {
        self.potential.map(|p| p.nominal_kv)
    }

    /// What is taken off a distance measured to its polyline (a
    /// subconductor's axis) for the distance to its surface, m: the
    /// surface radius and the polyline's error.
    pub fn allowance(&self) -> f64 {
        self.surface_radius + self.polyline_error
    }

    /// How far the wire's outer surface reaches from its centre line, m:
    /// the bundle radius and [`Wire::allowance`] (the outer envelope of the
    /// subconductors).
    pub fn outer_radius(&self) -> f64 {
        self.bundle_radius() + self.allowance()
    }

    /// Radius of the bundle, m (0 for a single wire).
    pub fn bundle_radius(&self) -> f64 {
        self.bundle
            .iter()
            .map(|o| o[0].hypot(o[1]))
            .fold(0.0, f64::max)
    }

    /// What a vertical measure takes off a distance to the polyline at
    /// piece `a`–`b` for the distance to the surface, m: the surface
    /// radius measured vertically on the sloping piece
    /// ([`vertical_of`]) and the polyline's error. Level: [`Wire::allowance`].
    pub(crate) fn vertical_allowance(&self, a: P3, b: P3) -> f64 {
        vertical_of(self.surface_radius, a, b) + self.polyline_error
    }

    /// Lowest and highest subconductor relative to the centre line,
    /// measured vertically at piece `a`–`b` ([`vertical_of`]), m.
    fn vertical_extent_on(&self, a: P3, b: P3) -> (f64, f64) {
        let (lo, hi) = self.vertical_extent();
        (vertical_of(lo, a, b), vertical_of(hi, a, b))
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
            .map(|o| ohl_line::bundle_point(q, a, b, *o))
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

    /// Vertical separation of the bundle at centre-line point `q` of piece
    /// `a`–`b` from a point at height `z` below or above it (0 inside its
    /// height range): to the lowest subconductor from below, the highest
    /// from above, measured vertically on the piece ([`vertical_of`]).
    fn vertical_gap(&self, q: P3, z: f64, a: P3, b: P3) -> f64 {
        let (lo, hi) = self.vertical_extent_on(a, b);
        if z <= q[2] + lo {
            q[2] + lo - z
        } else if z >= q[2] + hi {
            z - (q[2] + hi)
        } else {
            0.0
        }
    }
}

/// A distance `x` square to a wire, measured vertically on its piece
/// `a`–`b` sloping at β (physics review 2026-10-26): \( x / \cos β \) — a
/// vertical meets a round surface of radius r about an axis sloping at β
/// at r / cos β from the axis, and a line parallel to the axis at d from
/// it (square to it, in the vertical plane) lies d / cos β above or below
/// it at the same plan position — but never more than |x| plus the
/// piece's own rise (a vertical through the piece leaves its capsule
/// through the end within that: a steep or vertical jumper piece); the
/// sign kept. A level piece: `x` itself, bit for bit.
pub(crate) fn vertical_of(x: f64, a: P3, b: P3) -> f64 {
    let rise = (b[2] - a[2]).abs();
    if rise == 0.0 {
        return x;
    }
    let h = (b[0] - a[0]).hypot(b[1] - a[1]);
    let scaled = if h > 0.0 {
        x.abs() * (h.hypot(rise) / h)
    } else {
        f64::INFINITY
    };
    scaled.min(x.abs() + rise).copysign(x)
}

fn key(p: &ohl_line::SectionPoint) -> String {
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
    let tapped: std::collections::BTreeSet<&ohl_line::Tap> =
        line.jumpers.iter().flat_map(|j| j.taps()).collect();
    for shape in shapes {
        let section = line.sections.iter().find(|s| s.id == shape.section);
        for (k, span) in shape.spans.iter().enumerate() {
            wires.push(Wire {
                section: shape.section.clone(),
                span: k,
                circuit: section.map_or(String::new(), |s| s.circuit_name.clone()),
                phase: section.map_or(String::new(), |s| s.phase_label.clone()),
                potential: potential(&shape.section),
                bundle: section.map_or(Vec::new(), |s| s.bundle_offsets()),
                points: span.points.clone(),
                surface_radius: span.surface_radius,
                polyline_error: span.polyline_error,
                ends: section.map_or(Vec::new(), |s| {
                    let mut e: Vec<String> = s.points.iter().skip(k).take(2).map(key).collect();
                    // Its clamps jumpers hang on: where taps join them to
                    // it (a spacer's clamp is not a tap).
                    e.extend(
                        s.span_points
                            .iter()
                            .filter(|p| p.span == k && is_clamp(&p.object))
                            .map(|p| {
                                ohl_line::Tap::Span(ohl_line::SpanPointRef {
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
            circuit: from.map_or(String::new(), |s| s.circuit_name.clone()),
            phase: from.map_or(String::new(), |s| s.phase_label.clone()),
            potential: from.and_then(|s| potential(&s.id)),
            bundle: def.map_or(Vec::new(), |d| line.jumper_bundle(d).offsets()),
            points: j.points.clone(),
            surface_radius: j.surface_radius,
            polyline_error: j.polyline_error,
            ends: def.map_or(Vec::new(), |d| {
                let end = |e: &ohl_line::JumperEnd| match e {
                    ohl_line::JumperEnd::Wire(p) => key(p),
                    ohl_line::JumperEnd::Tap { tap } => tap_key(tap),
                };
                let mut e = vec![end(&d.from), end(&d.to)];
                // Its clamps jumpers hang on: where they join it.
                e.extend(
                    d.points
                        .iter()
                        .filter(|p| is_clamp(&p.object))
                        .map(|p| ohl_line::Tap::Jumper {
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
fn is_clamp(o: &Option<ohl_line::LineObject>) -> bool {
    o.as_ref()
        .is_some_and(|o| o.kind == ohl_line::ObjectKind::Clamp)
}

/// The end key of a tap: a wire with a clamp there and a jumper on it
/// share it (connected).
fn tap_key(t: &ohl_line::Tap) -> String {
    match t {
        ohl_line::Tap::Span(r) => format!("tap:{}:{}", r.section, r.point),
        ohl_line::Tap::Jumper { jumper, point } => format!("tap:jumper:{jumper}:{point}"),
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
                        .any(|p| ohl_core::pattern::matches(p, structure_of(e)))
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
    /// [`Wire::allowance`] of each wire, m.
    pub(crate) allowances: Vec<f64>,
    /// [`Wire::vertical_extent`] of each wire, m.
    extents: Vec<(f64, f64)>,
    reach: f64,
    segments: Vec<(u32, u32)>,
    cells: HashMap<(i64, i64), Cell>,
}

/// The segments listed in a grid cell, in blocks of one group of wires
/// each ([`WireSet::with_groups`]; without groups, one wire each).
#[derive(Debug, Clone, Default)]
struct Cell {
    /// The segments, a block's together, in their order within a wire.
    ids: Vec<u32>,
    blocks: Vec<Block>,
}

/// A run of [`Cell::ids`] of one group: the bounds of its segments, so
/// that a point beyond the reach of all of them passes the whole run over
/// (2026-10-26: with wind steps a span's group holds its pieces in every
/// state).
#[derive(Debug, Clone)]
struct Block {
    /// Where its ids end in [`Cell::ids`].
    end: u32,
    /// Bounding box of its segments' centre lines.
    lo: P3,
    hi: P3,
    /// Largest bundle radius and allowance of its wires.
    radius: f64,
    allowance: f64,
    /// Lowest and highest subconductor of its wires.
    extent: (f64, f64),
    /// Steepest of its segments (rise over plan length).
    slope: f64,
}

/// Taken off the lower bounds of a vertical clearance: the rounding of
/// the point found on the wire (its height interpolated) keeps it below
/// the clearance worked out, so that a piece passed over could not have
/// given a smaller one.
const BOUND_EPS: f64 = 1e-6;

impl Block {
    /// The block of one segment `a`–`b` of a wire.
    fn of(a: P3, b: P3, radius: f64, allowance: f64, extent: (f64, f64)) -> Self {
        let plan = (b[0] - a[0]).hypot(b[1] - a[1]);
        Self {
            end: 0,
            lo: [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
            hi: [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
            radius,
            allowance,
            extent,
            slope: if plan > 0.0 {
                (b[2] - a[2]).abs() / plan
            } else {
                0.0
            },
        }
    }

    /// Takes `other` in; its ids now end at `end`.
    fn add(&mut self, other: &Block, end: u32) {
        self.end = end;
        for i in 0..3 {
            self.lo[i] = self.lo[i].min(other.lo[i]);
            self.hi[i] = self.hi[i].max(other.hi[i]);
        }
        self.radius = self.radius.max(other.radius);
        self.allowance = self.allowance.max(other.allowance);
        self.extent = (
            self.extent.0.min(other.extent.0),
            self.extent.1.max(other.extent.1),
        );
        self.slope = self.slope.max(other.slope);
    }

    /// A lower bound of the vertical clearance from a point at height
    /// `z` to its wires, as [`PointCheck::eval`] works it out, with the
    /// slice depth `depth`.
    fn vertical_low(&self, z: f64, depth: f64) -> f64 {
        let rise = if depth > 0.0 {
            self.slope * depth / 2.0
        } else {
            0.0
        };
        // Distances square to a sloping wire grow by 1 / cos β measured
        // vertically ([`vertical_of`]): the steepest piece's factor keeps
        // this below every piece's exact gap.
        let k = self.slope.hypot(1.0);
        (self.lo[2] + self.extent.0 * k - z)
            .max(z - (self.hi[2] + self.extent.1 * k))
            .max(0.0)
            - rise
            - self.allowance * k
            - BOUND_EPS
    }
}

fn cell_of(v: f64) -> i64 {
    (v / CELL).floor() as i64
}

impl WireSet {
    /// Indexes `wires` for queries up to `reach` (m) away.
    pub fn new(wires: Vec<Wire>, reach: f64) -> Self {
        Self::build(wires, reach, None)
    }

    /// [`WireSet::new`] with the cells' segments in blocks by group:
    /// `group[w]` for wire `w` (e.g. a span's positions in several wind
    /// states). A point checked passes a whole block over when it is
    /// beyond the reach of all of its segments ([`PointCheck`]);
    /// only the speed changes (within a wire the segments keep their
    /// order, and the wires' order does not matter to the results).
    pub fn grouped(wires: Vec<Wire>, reach: f64, group: &[usize]) -> Self {
        Self::build(wires, reach, Some(group))
    }

    fn build(wires: Vec<Wire>, reach: f64, group: Option<&[usize]>) -> Self {
        let radii: Vec<f64> = wires.iter().map(Wire::bundle_radius).collect();
        let allowances: Vec<f64> = wires.iter().map(Wire::allowance).collect();
        let extents: Vec<(f64, f64)> = wires.iter().map(Wire::vertical_extent).collect();
        let mut segments = Vec::new();
        let mut cells: HashMap<(i64, i64), Cell> = HashMap::new();
        // With groups: per cell, per group (in the order first met) its
        // block and segments, put together at the end.
        type Lists = Vec<(usize, Block, Vec<u32>)>;
        let mut lists: HashMap<(i64, i64), Lists> = HashMap::new();
        for (w, wire) in wires.iter().enumerate() {
            let g = group.map(|g| g.get(w).copied().unwrap_or(usize::MAX - w));
            // Subconductors reach out by the bundle radius, their surface
            // by the allowance.
            let reach = reach + wire.outer_radius();
            for (k, s) in wire.points.windows(2).enumerate() {
                let id = segments.len() as u32;
                segments.push((w as u32, k as u32));
                let piece = Block::of(s[0], s[1], radii[w], allowances[w], extents[w]);
                let (x0, x1) = (s[0][0].min(s[1][0]) - reach, s[0][0].max(s[1][0]) + reach);
                let (y0, y1) = (s[0][1].min(s[1][1]) - reach, s[0][1].max(s[1][1]) + reach);
                for i in cell_of(x0)..=cell_of(x1) {
                    for j in cell_of(y0)..=cell_of(y1) {
                        match g {
                            // Without groups, a block per wire: its
                            // segments come one after another.
                            None => {
                                let cell = cells.entry((i, j)).or_default();
                                let same = k > 0
                                    && cell
                                        .ids
                                        .last()
                                        .is_some_and(|&l| segments[l as usize].0 == w as u32);
                                cell.ids.push(id);
                                let end = cell.ids.len() as u32;
                                match cell.blocks.last_mut() {
                                    Some(b) if same => b.add(&piece, end),
                                    _ => cell.blocks.push(Block { end, ..piece }),
                                }
                            }
                            Some(g) => {
                                let list = lists.entry((i, j)).or_default();
                                match list.iter_mut().rev().find(|e| e.0 == g) {
                                    Some(e) => {
                                        e.1.add(&piece, 0);
                                        e.2.push(id);
                                    }
                                    None => list.push((g, piece.clone(), vec![id])),
                                }
                            }
                        }
                    }
                }
            }
        }
        for (key, list) in lists {
            let cell = cells.entry(key).or_default();
            for (_, block, ids) in list {
                cell.ids.extend(ids);
                let end = cell.ids.len() as u32;
                cell.blocks.push(Block { end, ..block });
            }
        }
        Self {
            radii,
            allowances,
            extents,
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
            .flat_map(|c| &c.ids)
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
    /// Clearance to the wire's surface, m ([`Wire::allowance`] off the
    /// distance to its axis). Vertical to the ground: the surface's height
    /// above it (negative if below); to points and other wires: the
    /// vertical separation, whichever is higher.
    pub distance: f64,
    /// Point on the wire's axis (a subconductor's) it is measured from.
    pub wire: P3,
    /// Point on the target.
    pub target: P3,
    /// Point class, for point targets.
    pub class: Option<u8>,
}

impl Hit {
    /// The segment drawn for this clearance: (its end at the wire, its end
    /// at the target). Radial: from the wire point found to the target.
    /// Vertical (owner, 2026-10-22): straight up or down from the target
    /// to the wire's height at the target's plan position — the same x and
    /// y at both ends, also for a target at the edge of the zone's width,
    /// never to the wire point beside it ([`vertical_mark`]).
    pub fn drawn(&self, vertical: bool) -> (P3, P3) {
        if vertical {
            (vertical_mark(self.wire, self.target), self.target)
        } else {
            (self.wire, self.target)
        }
    }
}

/// The wire's end of a vertical clearance's mark: the point at the height
/// of the wire point `wire` found over (under) the target, straight above
/// (below) `target` — the vertical is drawn vertical (owner, 2026-10-22).
pub fn vertical_mark(wire: P3, target: P3) -> P3 {
    [target[0], target[1], wire[2]]
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

    pub(crate) fn vertical(&mut self, hit: Hit) -> bool {
        if self.vertical.is_none_or(|h| hit.distance < h.distance) {
            self.vertical = Some(hit);
        }
        self.required.vertical.is_some_and(|r| hit.distance < r)
    }

    pub(crate) fn radial(&mut self, hit: Hit) -> bool {
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

/// What one point did to one wire's result: a smaller clearance found
/// (`hit`), and/or a requirement not met (`bad`) — so that points can be
/// worked out on several cores and their results applied in their order
/// ([`PointCheck::eval`], [`PointCheck::apply`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PointEvent {
    /// The wire (or the arc, for the swing arcs' check).
    pub item: u32,
    /// 0 vertical, 1 radial.
    pub measure: u8,
    /// Below the requirement.
    pub bad: bool,
    /// Smaller than the smallest found before it.
    pub hit: Option<Hit>,
}

/// What the point check knows of a wire while points are worked out: the
/// smallest clearance found so far (vertical, radial; infinite before
/// any) and the number of the last point found below the requirement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Best {
    /// Smallest clearance so far, m.
    pub d: [f64; 2],
    /// The last point (its number) below the requirement.
    pub bad: [u64; 2],
}

impl Best {
    const NONE: Best = Best {
        d: [f64::INFINITY; 2],
        bad: [0; 2],
    };
}

/// The counts of a stretch of points worked out on one core: per wire and
/// per group, the points below the requirement (vertical, radial), a
/// point counted once per group (its number in `stamp`). Counts add up
/// in any order: they are summed into the results after a block
/// ([`PointCheck::take_tally`]).
#[derive(Debug, Clone, Default)]
pub(crate) struct Tally {
    pub wire: Vec<[u64; 2]>,
    pub group: Vec<[u64; 2]>,
    stamp: Vec<[u64; 2]>,
}

impl Tally {
    /// Empty counts for `wires` wires and `groups` groups.
    pub fn new(wires: usize, groups: usize) -> Self {
        Self {
            wire: vec![[0; 2]; wires],
            group: vec![[0; 2]; groups],
            stamp: vec![[0; 2]; groups],
        }
    }

    /// Point `stamp` is below the requirement in group `g` (measure `m`):
    /// counted once per point.
    pub fn group_bad(&mut self, g: usize, m: usize, stamp: u64) {
        if let Some(s) = self.stamp.get_mut(g)
            && s[m] != stamp
        {
            s[m] = stamp;
            self.group[g][m] += 1;
        }
    }
}

/// The lower bound of the distance from `p` to any point of the segment
/// `a`–`b`: the distance to its bounding box.
pub(crate) fn box_distance(p: P3, a: P3, b: P3) -> f64 {
    let mut s = 0.0;
    for i in 0..3 {
        let (lo, hi) = (a[i].min(b[i]), a[i].max(b[i]));
        let d = if p[i] < lo {
            lo - p[i]
        } else if p[i] > hi {
            p[i] - hi
        } else {
            0.0
        };
        s += d * d;
    }
    s.sqrt()
}

/// Clearances of every wire to points of chosen classes, for one rule;
/// feed the points with [`PointCheck::visit`] (e.g. from a LiDAR reader).
///
/// Speed (2026-10-20; the numbers unchanged, bit for bit — the test
/// `the_point_check_gives_the_results_of_before`): a wire piece is
/// passed over without its exact distance when the distance to its
/// bounding box (in plan and the zone's width, for the vertical) already
/// shows it cannot give a smaller clearance nor one below the
/// requirement; with [`PointCheck::only`], the measure a check does not
/// need is not worked out; points may be worked out on several cores
/// (`PointCheck::eval`) and applied in their order (`PointCheck::apply`,
/// as the zone check does).
#[derive(Debug, Clone)]
pub struct PointCheck {
    rule: Rule,
    classes: [bool; 256],
    /// One result per wire of the set.
    pub results: Vec<WireResult>,
    groups: Option<Vec<usize>>,
    /// The points taken so far (the current one's number).
    taken: u64,
    /// Per group: the number of the last point counted (vertical,
    /// radial), so that a point counts once per group.
    counted: Vec<[u64; 2]>,
    /// With groups ([`PointCheck::grouped`]): the points below the
    /// vertical and the radial requirement of any wire of each group (a
    /// point counted once per group).
    pub group_violations: Vec<[u64; 2]>,
    /// The smallest clearance of each wire so far (vertical, radial):
    /// that of its result, infinite before any.
    pub(crate) best: Vec<Best>,
    /// The points offered so far (taken or not): each point's number.
    pub(crate) offered: u64,
    /// The measures worked out (vertical, radial).
    measures: [bool; 2],
    scratch: Vec<PointEvent>,
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
            groups: None,
            group_violations: Vec::new(),
            taken: 0,
            counted: Vec::new(),
            best: vec![Best::NONE; wires.wires.len()],
            offered: 0,
            measures: [true, true],
            scratch: Vec::new(),
        }
    }

    /// Works out only the vertical and/or the radial clearances (the
    /// other is left empty in the results).
    pub fn only(mut self, vertical: bool, radial: bool) -> Self {
        self.measures = [vertical, radial];
        self
    }

    /// Also counts the points too close per group of wires — `group[w]`
    /// for wire `w`, e.g. the positions of one span in several wind
    /// states: a point too close to several of them counts once.
    pub fn grouped(mut self, group: Vec<usize>) -> Self {
        let n = group.iter().max().map_or(0, |g| g + 1);
        self.group_violations = vec![[0, 0]; n];
        self.counted = vec![[0, 0]; n];
        self.groups = Some(group);
        self
    }

    /// The rule checked.
    pub fn rule(&self) -> &Rule {
        &self.rule
    }

    /// Counts the point last applied as too close in group `g` (measure
    /// `k`: 0 vertical, 1 radial), once per point: also for what is
    /// checked beside the wires of the set (a span's swing arc).
    pub(crate) fn count_in_group(&mut self, g: usize, k: usize) {
        if let Some(c) = self.counted.get_mut(g)
            && c[k] != self.taken
        {
            c[k] = self.taken;
            self.group_violations[g][k] += 1;
        }
    }

    /// Takes one point into account.
    pub fn visit(&mut self, wires: &WireSet, x: f64, y: f64, z: f64, class: u8) {
        let mut events = std::mem::take(&mut self.scratch);
        let mut best = std::mem::take(&mut self.best);
        events.clear();
        self.offered += 1;
        let taken = self.eval(
            wires,
            &mut best,
            None,
            self.offered,
            x,
            y,
            z,
            class,
            &mut events,
        );
        self.best = best;
        if taken {
            self.apply(&events);
        }
        self.scratch = events;
    }

    /// What the point (x, y, z) of `class`, number `stamp` (above every
    /// number before it), does to the results, given the smallest
    /// clearances found before it (`best`, updated): pushed on `out`, in
    /// the order of the wire pieces, a wire's "below the requirement" once
    /// per point — or, with a `tally`, counted there instead of pushed.
    /// False when its class is not checked. Reads the check only: points can be worked out on
    /// several cores, each with its own `best`, and applied in their
    /// order ([`PointCheck::apply`]).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn eval(
        &self,
        wires: &WireSet,
        best: &mut [Best],
        mut tally: Option<&mut Tally>,
        stamp: u64,
        x: f64,
        y: f64,
        z: f64,
        class: u8,
        out: &mut Vec<PointEvent>,
    ) -> bool {
        if !self.classes[class as usize] {
            return false;
        }
        let p = [x, y, z];
        let band = self.rule.vertical_band;
        let Some(cell) = wires.cells.get(&(cell_of(x), cell_of(y))) else {
            return true;
        };
        let depth = self.rule.vertical_depth;
        // Passed over (2026-10-26): a block of segments (a group's, e.g. a
        // span's in every wind state) none of which can come within the
        // reach — the radial lower bound from the block's box, the
        // vertical outside its plan box widened by the zone's width or
        // above its height lower bound. Each of its segments would be
        // passed over by the same test below.
        let mut start = 0;
        let segments = cell.blocks.iter().flat_map(|block| {
            let ids = &cell.ids[start..block.end as usize];
            start = block.end as usize;
            let radial = self.measures[1]
                && box_distance(p, block.lo, block.hi) - block.radius - block.allowance
                    <= wires.reach;
            let vertical = self.measures[0]
                && !(x < block.lo[0] - band
                    || x > block.hi[0] + band
                    || y < block.lo[1] - band
                    || y > block.hi[1] + band)
                && block.vertical_low(z, depth) <= wires.reach;
            let ids = if radial || vertical { ids } else { &[] };
            ids.iter().map(move |&id| {
                let (w, k) = wires.segments[id as usize];
                (w as usize, k as usize)
            })
        });
        for (w, k) in segments {
            let (a, b) = wires.segment(w, k);
            let wire = &wires.wires[w];
            let required = self.results[w].required;
            let reach = (required.largest() + self.rule.search_margin).min(wires.reach);
            // To the surface.
            let off = wires.allowances[w];
            let radius = wires.radii[w];
            let groups = self.groups.as_deref();
            let mut record = |m: usize, hit: Hit, out: &mut Vec<PointEvent>, best: &mut [Best]| {
                let b = &mut best[w];
                let improved = hit.distance < b.d[m];
                if improved {
                    b.d[m] = hit.distance;
                }
                let req = if m == 0 {
                    required.vertical
                } else {
                    required.radial
                };
                // Once per point and wire.
                let mut bad = b.bad[m] != stamp && req.is_some_and(|r| hit.distance < r);
                if bad {
                    b.bad[m] = stamp;
                    if let Some(t) = tally.as_deref_mut() {
                        t.wire[w][m] += 1;
                        if let Some(g) = groups {
                            t.group_bad(g[w], m, stamp);
                        }
                        bad = false;
                    }
                }
                if improved || bad {
                    out.push(PointEvent {
                        item: w as u32,
                        measure: m as u8,
                        bad,
                        hit: improved.then_some(hit),
                    });
                }
            };
            // Whether a piece with this lower bound can change nothing.
            let idle = |m: usize, lower: f64, best: &[Best]| -> bool {
                let req = if m == 0 {
                    required.vertical
                } else {
                    required.radial
                };
                let b = &best[w];
                lower >= b.d[m] && (b.bad[m] == stamp || req.is_none_or(|r| lower >= r))
            };
            if self.measures[1] {
                let lower = box_distance(p, a, b) - radius - off;
                if lower <= reach && !idle(1, lower, best) {
                    // The centre line first: subconductors only when within
                    // reach.
                    let (q, _) = closest_on_segment(p, a, b);
                    let near = distance(p, q) - radius - off <= reach;
                    let (d, sub) = if near && radius > 0.0 {
                        wire.nearest(p, a, b)
                    } else {
                        (distance(p, q), q)
                    };
                    let d = d - off;
                    if near && d <= reach {
                        record(
                            1,
                            Hit {
                                distance: d,
                                wire: sub,
                                target: p,
                                class: Some(class),
                            },
                            out,
                            best,
                        );
                    }
                }
            }
            if self.measures[0] {
                // Within the zone's width in plan only (its bounding box
                // first).
                let outside = x < a[0].min(b[0]) - band
                    || x > a[0].max(b[0]) + band
                    || y < a[1].min(b[1]) - band
                    || y > a[1].max(b[1]) + band;
                // A lower bound of the clearance (from the piece's height
                // range): a piece that can change nothing is passed over,
                // as for the radial (2026-10-26).
                let rise = if outside {
                    0.0
                } else {
                    slice_rise(a, b, depth)
                };
                let passed = outside || {
                    // Measured vertically on the sloping piece as the exact
                    // gap is (r / cos β, at most r plus the rise): never
                    // above it.
                    let (lo, hi) = wires.extents[w];
                    let (lo, hi) = (vertical_of(lo, a, b), vertical_of(hi, a, b));
                    let low = ((a[2].min(b[2]) + lo - z)
                        .max(z - (a[2].max(b[2]) + hi))
                        .max(0.0)
                        - rise
                        - vertical_of(off, a, b)
                        - BOUND_EPS)
                        .max(0.0);
                    low > reach || idle(0, low, best)
                };
                if !passed && let Some((q, _)) = over_in_plan(x, y, a, b, band) {
                    // Vertical separation, above or below the wire (a tree
                    // overhanging a low wire, a line crossing over); with a
                    // depth, to the wire's lowest (highest) in the slice.
                    // The surface radius measured vertically on the
                    // sloping piece (r / cos β), not radially.
                    // (The bounds before this follow it: keep them together.)
                    let off = wire.vertical_allowance(a, b);
                    let gap = wire.vertical_gap(q, z, a, b)
                        - slice_rise(a, b, self.rule.vertical_depth)
                        - off;
                    let hit = Hit {
                        distance: gap.max(0.0),
                        wire: q,
                        target: p,
                        class: Some(class),
                    };
                    if hit.distance <= reach {
                        record(0, hit, out, best);
                    }
                }
            }
        }
        true
    }

    /// Adds the counts of `tally` (points worked out with it) to the
    /// results.
    pub(crate) fn take_tally(&mut self, tally: &Tally) {
        for (r, c) in self.results.iter_mut().zip(&tally.wire) {
            r.vertical_violations += c[0];
            r.radial_violations += c[1];
        }
        for (g, c) in self.group_violations.iter_mut().zip(&tally.group) {
            g[0] += c[0];
            g[1] += c[1];
        }
    }

    /// The number of groups counted.
    pub(crate) fn groups(&self) -> usize {
        self.group_violations.len()
    }

    /// Applies one point's events ([`PointCheck::eval`]) to the results,
    /// as [`PointCheck::visit`] does: a smaller clearance kept (the first
    /// of equal ones), the point counted once per wire, and once per
    /// group, below a requirement.
    pub(crate) fn apply(&mut self, events: &[PointEvent]) {
        self.taken += 1;
        // Wires this point violates (each once per point: `eval` says it
        // once).
        let mut bad: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
        for e in events {
            let (w, m) = (e.item as usize, e.measure as usize);
            if let Some(h) = e.hit {
                let r = &mut self.results[w];
                let kept = if m == 0 {
                    &mut r.vertical
                } else {
                    &mut r.radial
                };
                if kept.is_none_or(|k| h.distance < k.distance) {
                    *kept = Some(h);
                    self.best[w].d[m] = h.distance;
                }
            }
            if e.bad {
                bad[m].push(w);
            }
        }
        if let Some(groups) = self.groups.take() {
            for (k, list) in bad.iter().enumerate() {
                let mut gs: Vec<usize> = list.iter().map(|w| groups[*w]).collect();
                gs.sort_unstable();
                gs.dedup();
                for g in gs {
                    self.count_in_group(g, k);
                }
            }
            self.groups = Some(groups);
        }
        for w in &bad[0] {
            self.results[*w].vertical_violations += 1;
        }
        for w in &bad[1] {
            self.results[*w].radial_violations += 1;
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

/// Cell of the [`GroundMax`] raster, m.
const GROUND_CELL: f64 = 5.0;

/// Cells per side of a tile of the [`GroundMax`] raster.
const TILE: i64 = 64;

/// The highest ground over plan cells of 5 m: for every cell, the
/// highest corner of the triangles whose bounding box touches it — an
/// upper bound of the ground surface over any area (the cells it
/// touches), for the lower bounds of the ground checks. Kept in tiles of
/// 64 × 64 cells, only where there is ground.
#[derive(Debug, Clone, Default)]
pub struct GroundMax {
    tiles: HashMap<(i64, i64), Box<[f64]>>,
}

fn ground_cell(v: f64) -> i64 {
    (v / GROUND_CELL).floor() as i64
}

impl GroundMax {
    /// The raster of `terrain`.
    pub fn new(terrain: &Terrain) -> Self {
        let pts = terrain.points();
        let mut tiles: HashMap<(i64, i64), Box<[f64]>> = HashMap::new();
        for t in terrain.triangles() {
            let c = [pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]];
            let top = c[0][2].max(c[1][2]).max(c[2][2]);
            let (x0, x1) = (
                c[0][0].min(c[1][0]).min(c[2][0]),
                c[0][0].max(c[1][0]).max(c[2][0]),
            );
            let (y0, y1) = (
                c[0][1].min(c[1][1]).min(c[2][1]),
                c[0][1].max(c[1][1]).max(c[2][1]),
            );
            for i in ground_cell(x0)..=ground_cell(x1) {
                for j in ground_cell(y0)..=ground_cell(y1) {
                    let tile = tiles
                        .entry((i.div_euclid(TILE), j.div_euclid(TILE)))
                        .or_insert_with(|| {
                            vec![f64::NEG_INFINITY; (TILE * TILE) as usize].into_boxed_slice()
                        });
                    let k = (j.rem_euclid(TILE) * TILE + i.rem_euclid(TILE)) as usize;
                    tile[k] = tile[k].max(top);
                }
            }
        }
        Self { tiles }
    }

    /// The highest ground over the plan box `x0`–`x1`, `y0`–`y1` (at
    /// least; −∞ with no ground there).
    pub fn max_in(&self, x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
        let mut m = f64::NEG_INFINITY;
        let (i0, i1) = (ground_cell(x0), ground_cell(x1));
        let (j0, j1) = (ground_cell(y0), ground_cell(y1));
        for tj in j0.div_euclid(TILE)..=j1.div_euclid(TILE) {
            for ti in i0.div_euclid(TILE)..=i1.div_euclid(TILE) {
                let Some(tile) = self.tiles.get(&(ti, tj)) else {
                    continue;
                };
                let (a0, a1) = ((i0 - ti * TILE).max(0), (i1 - ti * TILE).min(TILE - 1));
                let (b0, b1) = ((j0 - tj * TILE).max(0), (j1 - tj * TILE).min(TILE - 1));
                for b in b0..=b1 {
                    let row = &tile[(b * TILE + a0) as usize..=(b * TILE + a1) as usize];
                    for z in row {
                        m = m.max(*z);
                    }
                }
            }
        }
        m
    }
}

/// Vertical and radial clearances of every wire to the ground surface.
/// Vertical: under every wire piece at every TIN edge crossing (exact for
/// the polyline) — with the rule's `vertical_band`, the highest ground
/// within that horizontal distance of the wire; with its `vertical_depth`,
/// the wire's lowest in a slice of that depth. Radial: wire samples every [`GROUND_STEP`] to the
/// triangles within the rule's reach (narrowed to the vertical clearance
/// where that is smaller: the radial distance is never larger).
///
/// Speed (2026-10-20; the numbers unchanged, bit for bit — the test
/// `the_ground_check_gives_the_results_of_before`): the wires are checked
/// on every core; a wire piece (a sample, radially) is passed over when a
/// lower bound from the highest ground near it ([`GroundMax`]) shows it
/// cannot give a smaller clearance nor one below the requirement.
pub fn ground_check(rule: &Rule, wires: &[Wire], terrain: &Terrain) -> Vec<WireResult> {
    ground_check_with(rule, wires, terrain, &GroundMax::new(terrain))
}

/// [`ground_check`] with the terrain's [`GroundMax`] made once.
pub fn ground_check_with(
    rule: &Rule,
    wires: &[Wire],
    terrain: &Terrain,
    top: &GroundMax,
) -> Vec<WireResult> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(wires.len())
        .max(1);
    if threads <= 1 {
        return wires
            .iter()
            .map(|w| ground_wire(rule, w, terrain, top, [f64::INFINITY; 2], None))
            .collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out: std::sync::Mutex<Vec<Option<WireResult>>> =
        std::sync::Mutex::new(vec![None; wires.len()]);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(w) = wires.get(k) else {
                        return;
                    };
                    let r = ground_wire(rule, w, terrain, top, [f64::INFINITY; 2], None);
                    out.lock().expect("not poisoned")[k] = Some(r);
                }
            });
        }
    });
    out.into_inner()
        .expect("not poisoned")
        .into_iter()
        .map(|r| r.expect("every wire checked"))
        .collect()
}

/// [`ground_check_with`] for wires in groups — `group[w]` for wire `w`, a
/// span's positions in several wind states, say — when only each
/// group's smallest clearances (the first of equal ones in the wires'
/// order) and its largest counts are wanted, as the zone check reports
/// them (owner, 2026-10-26). A group's wires are checked in their order
/// on one thread, each with the group's smallest clearances so far as
/// its bound (`ground_wire`, as a swing arc's lines are): a piece or
/// sample that can come neither below them nor below the requirement is
/// passed over. The groups run in parallel. Each wire's counts are those
/// of [`ground_check_with`]; its clearances only where it is the group's
/// smallest (a wire passed over may keep none, or a larger one).
pub fn ground_check_grouped(
    rule: &Rule,
    wires: &[Wire],
    group: &[usize],
    terrain: &Terrain,
    top: &GroundMax,
) -> Vec<WireResult> {
    // Each group's wires, in their order.
    let mut lists: Vec<Vec<usize>> = Vec::new();
    let mut index: HashMap<usize, usize> = HashMap::new();
    for w in 0..wires.len() {
        let g = group.get(w).copied().unwrap_or(usize::MAX - w);
        let k = *index.entry(g).or_insert_with(|| {
            lists.push(Vec::new());
            lists.len() - 1
        });
        lists[k].push(w);
    }
    let check = |list: &[usize]| -> Vec<(usize, WireResult)> {
        let mut bound = [f64::INFINITY; 2];
        list.iter()
            .map(|&w| {
                let r = ground_wire(rule, &wires[w], terrain, top, bound, None);
                if let Some(h) = r.vertical {
                    bound[0] = bound[0].min(h.distance);
                }
                if let Some(h) = r.radial {
                    bound[1] = bound[1].min(h.distance);
                }
                (w, r)
            })
            .collect()
    };
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(lists.len())
        .max(1);
    let mut out: Vec<Option<WireResult>> = vec![None; wires.len()];
    if threads <= 1 {
        for list in &lists {
            for (w, r) in check(list) {
                out[w] = Some(r);
            }
        }
    } else {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let found: std::sync::Mutex<Vec<(usize, WireResult)>> = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(list) = lists.get(k) else {
                            return;
                        };
                        let r = check(list);
                        found.lock().expect("not poisoned").extend(r);
                    }
                });
            }
        });
        for (w, r) in found.into_inner().expect("not poisoned") {
            out[w] = Some(r);
        }
    }
    out.into_iter()
        .map(|r| r.expect("every wire checked"))
        .collect()
}

/// One wire's [`ground_check`]. `bound`: clearances (vertical, radial)
/// already found elsewhere (e.g. other wires of the same swing arc): a
/// piece or sample that cannot come below them nor below the
/// requirement is passed over (its own result may then miss a clearance
/// at or above `bound`; the counts are exact). `vertical_extra`: per
/// piece, what is taken off its vertical clearances besides the
/// allowance (none: nothing).
pub(crate) fn ground_wire(
    rule: &Rule,
    wire: &Wire,
    terrain: &Terrain,
    top: &GroundMax,
    bound: [f64; 2],
    vertical_extra: Option<&[f64]>,
) -> WireResult {
    let mut result = WireResult::with(rule.required(wire.kv()));
    let reach = result.required.largest() + rule.search_margin;
    let bundle = wire.bundle_radius();
    // To the surface.
    let off = wire.allowance();
    let band = rule.vertical_band;
    // The smallest clearances so far (with the bound).
    let best = |r: &WireResult, m: usize| -> f64 {
        let own = if m == 0 { r.vertical } else { r.radial };
        own.map_or(bound[m], |h| h.distance.min(bound[m]))
    };
    for (piece, s) in wire.points.windows(2).enumerate() {
        let (a, b) = (s[0], s[1]);
        let extra = vertical_extra.map_or(0.0, |x| x[piece]);
        let (mut vertical_bad, mut radial_bad) = (false, false);
        let rise = slice_rise(a, b, rule.vertical_depth);
        // Bundles: the lowest subconductor for the vertical clearance; it
        // and the surface radius measured vertically on the sloping piece
        // (r / cos β; physics review 2026-10-26).
        let (lowest, _) = wire.vertical_extent_on(a, b);
        let off_vertical = wire.vertical_allowance(a, b);
        // The vertical: passed over when the piece's lowest point is
        // high enough above the highest ground within the band.
        let ground_top = top.max_in(
            a[0].min(b[0]) - band,
            a[0].max(b[0]) + band,
            a[1].min(b[1]) - band,
            a[1].max(b[1]) + band,
        );
        let lower = a[2].min(b[2]) + lowest - rise - ground_top - off_vertical - extra;
        let skip = lower >= best(&result, 0) && result.required.vertical.is_none_or(|r| lower >= r);
        if !skip {
            for (t, g) in ground_under(terrain, a, b, band) {
                let z = a[2] + (b[2] - a[2]) * t + lowest;
                let hit = Hit {
                    distance: z - rise - g[2] - off_vertical - extra,
                    wire: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, z],
                    target: g,
                    class: None,
                };
                vertical_bad |= result.vertical(hit);
            }
        }
        result.vertical_violations += u64::from(vertical_bad);
        if result.required.radial.is_none() {
            continue;
        }
        let n = ((distance(a, b) / GROUND_STEP).ceil() as usize).max(1);
        // The samples' spacing, at most GROUND_STEP.
        let spacing = distance(a, b) / n as f64;
        // Passed over: a piece (a sample) whose clearances cannot come
        // below `t` — the smallest so far and, unless it is already
        // below it, the requirement. A triangle whose nearest point q is
        // within r of the sample p in plan lies under the highest ground
        // there, so |p − q| ≥ p_z − that; one farther is more than r
        // away. With r = √((t + allowance)² + (s/2)²) and the sample
        // higher than r above that ground, its clearance (the sampled
        // floor, less the allowance) is at least t.
        let threshold = |r: &WireResult, bad: bool| -> f64 {
            let b = best(r, 1);
            match r.required.radial {
                Some(q) if !bad => b.max(q),
                _ => b,
            }
        };
        let around = |t: f64| (t + off).hypot(spacing / 2.0);
        let t = threshold(&result, false);
        if t.is_finite() {
            let r = around(t);
            let high = top.max_in(
                a[0].min(b[0]) - bundle - r,
                a[0].max(b[0]) + bundle + r,
                a[1].min(b[1]) - bundle - r,
                a[1].max(b[1]) + bundle + r,
            );
            if a[2].min(b[2]) - bundle - high >= r {
                continue;
            }
        }
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
                let t = threshold(&result, radial_bad);
                if t.is_finite() {
                    let r = around(t);
                    let high = top.max_in(p[0] - r, p[0] + r, p[1] - r, p[1] + r);
                    if p[2] - high >= r {
                        continue;
                    }
                }
                let below = terrain.height(p[0], p[1]);
                let radius = below.map_or(reach, |g| (p[2] - g).max(0.0).min(reach));
                let nearest = nearest_triangle(terrain, p, radius);
                if let Some((d, q)) = nearest {
                    // Between samples the wire may come nearer:
                    // the lower bound, then to the surface.
                    let d = if below.is_some_and(|g| p[2] < g) {
                        -d
                    } else {
                        sampled_floor(d, spacing)
                    } - off;
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
}

/// Margin of the triangles' box test in [`nearest_triangle`], m: the
/// point found on a triangle may lie outside its box by the rounding of
/// its coordinates.
const TRIANGLE_BOX_MARGIN: f64 = 1e-6;

thread_local! {
    /// Per thread: the sample each triangle was last visited for (its
    /// number), to visit a triangle listed in several grid cells once.
    static VISITED: std::cell::RefCell<(Vec<u32>, u32)> =
        const { std::cell::RefCell::new((Vec::new(), 0)) };
}

/// The nearest point of the surface to `p` among the triangles near it in
/// plan (within `radius`): (distance, point), the first of equal ones in
/// the grid's order. Speed (2026-10-26; the same results bit for bit): a
/// triangle whose bounding box is already no nearer than the nearest so
/// far is not worked out, and a triangle listed in several cells is
/// visited once.
fn nearest_triangle(terrain: &Terrain, p: P3, radius: f64) -> Option<(f64, P3)> {
    VISITED.with(|visited| {
        let (stamps, stamp) = &mut *visited.borrow_mut();
        let n = terrain.triangles().len();
        if stamps.len() < n {
            stamps.resize(n, 0);
        }
        *stamp = stamp.wrapping_add(1);
        if *stamp == 0 {
            stamps.fill(0);
            *stamp = 1;
        }
        let stamp = *stamp;
        let mut nearest: Option<(f64, P3)> = None;
        terrain.triangles_near_indexed(p[0], p[1], radius, |k, [ta, tb, tc]| {
            if stamps[k] == stamp {
                return;
            }
            stamps[k] = stamp;
            if let Some((bd, _)) = nearest {
                let lo = [0, 1, 2].map(|i| ta[i].min(tb[i]).min(tc[i]));
                let hi = [0, 1, 2].map(|i| ta[i].max(tb[i]).max(tc[i]));
                if box_distance(p, lo, hi) >= bd + TRIANGLE_BOX_MARGIN {
                    return;
                }
            }
            let q = closest_on_triangle(p, ta, tb, tc);
            let d = distance(p, q);
            if nearest.is_none_or(|(bd, _)| d < bd) {
                nearest = Some((d, q));
            }
        });
        nearest
    })
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
                // To the surfaces of both.
                let off = wire.allowance() + b.allowances[j];
                // Centre lines first: bundles only when within reach.
                let (ca, cb) = closest_between_segments(p, q, c, d);
                let spread = wire.bundle_radius() + b.radii[j];
                if distance(ca, cb) - spread - off > reach && crossing.is_none() {
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
                let dist = dist - off;
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
                    // Bundles and radii measured vertically on each sloping
                    // piece (r / cos β; physics review 2026-10-26).
                    let ((alo, ahi), (blo, bhi)) = (
                        wire.vertical_extent_on(p, q),
                        other.vertical_extent_on(c, d),
                    );
                    // (`off` is the two allowances: the same, both level.)
                    let off = wire.vertical_allowance(p, q) + other.vertical_allowance(c, d);
                    let gap = if x[2] >= y[2] {
                        (x[2] + alo) - (y[2] + bhi)
                    } else {
                        (y[2] + blo) - (x[2] + ahi)
                    } - off;
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
#[path = "checks_reference.rs"]
mod reference;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{RuleSet, Target};
    use ohl_terrain::SurveyPoint;

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
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    /// Phase L1 of a three-phase system of `kv`.
    fn ac(kv: f64) -> Potential {
        Potential {
            nominal_kv: kv,
            kind: ohl_line::electrical::SystemKind::Ac,
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
        // Pieces of 1.6 m sampled 0.4 m apart: the bound √(d² − 0.2²).
        let radial = res.radial.unwrap().distance;
        assert!((radial - (50.0f64 - 0.04).sqrt()).abs() < 1e-9, "{radial}");
        assert!(res.radial_violations > 0 && res.vertical_violations == 0);
    }

    /// The highest ground of the cells is at or above the surface
    /// everywhere, over any box; none where there is no ground.
    #[test]
    fn the_highest_ground_bounds_the_surface() {
        let pts: Vec<P3> = (0..400)
            .map(|k| {
                let (x, y) = ((k % 20) as f64 * 7.3, (k / 20) as f64 * 6.1);
                [x, y, 10.0 * (x / 17.0).sin() + 4.0 * (y / 9.0).cos()]
            })
            .collect();
        let t = Terrain::from_xyz(&pts, None).unwrap();
        let top = GroundMax::new(&t);
        for k in 0..2000 {
            let (x, y) = ((k * 37 % 1390) as f64 / 10.0, (k * 53 % 1150) as f64 / 10.0);
            if let Some(z) = t.height(x, y) {
                for r in [0.0, 0.5, 3.0] {
                    assert!(top.max_in(x - r, x + r, y - r, y + r) >= z - 1e-12);
                }
            }
        }
        assert_eq!(top.max_in(-500.0, -400.0, 0.0, 1.0), f64::NEG_INFINITY);
    }

    /// [`box_distance`] never exceeds the distance to the segment.
    #[test]
    fn the_box_distance_bounds_the_segment() {
        let (a, b) = ([1.0, 2.0, 3.0], [9.0, -4.0, 7.0]);
        for k in 0..500 {
            let p = [
                (k * 7 % 31) as f64 - 10.0,
                (k * 11 % 29) as f64 - 14.0,
                (k * 13 % 23) as f64 - 6.0,
            ];
            let (q, _) = closest_on_segment(p, a, b);
            assert!(box_distance(p, a, b) <= distance(p, q) + 1e-12);
        }
    }

    /// The sampling bound: never above the true distance of the piece,
    /// within s²/(8d) of it.
    #[test]
    fn sampled_floor_bounds_the_piece() {
        // A piece along x at height 5 over a point at x = 0.37 (between
        // samples at 0.25 and 0.5): the true distance 5, the samples'
        // smallest √(5² + 0.12²).
        let d_true: f64 = 5.0;
        let nearest = d_true.hypot(0.12);
        let floor = sampled_floor(nearest, 0.5);
        assert!(floor <= d_true);
        assert!(d_true - floor <= 0.5f64.powi(2) / (8.0 * d_true) + 1e-12);
        assert_eq!(sampled_floor(0.1, 0.5), 0.0);
        assert_eq!(sampled_floor(-1.0, 0.5), -1.0);
    }

    /// Every clearance is to the surface: the wires' allowances (radius
    /// and polyline error) off each distance, both wires' between two.
    #[test]
    fn clearances_to_the_surface() {
        let flat: Vec<SurveyPoint> = [(-10.0, -20.0), (110.0, -20.0), (110.0, 20.0), (-10.0, 20.0)]
            .iter()
            .map(|&(x, y)| SurveyPoint {
                x,
                y,
                z: 0.0,
                code: String::new(),
                line: String::new(),
            })
            .collect();
        let terrain = Terrain::from_points(&flat, None).unwrap();
        let thick = |w: Wire, r: f64, e: f64| Wire {
            surface_radius: r,
            polyline_error: e,
            ..w
        };
        // Conductor radius 15 mm, iced by 10 mm: 25 mm; 2 mm polyline bound.
        let a = thick(
            straight("A", "L1", [0.0, 0.0, 10.0], [100.0, 0.0, 10.0]),
            0.025,
            0.002,
        );
        assert!((a.allowance() - 0.027).abs() < 1e-12);
        let g = rule("[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 9.0\nradial = 9.0");
        let res = &ground_check(&g, std::slice::from_ref(&a), &terrain)[0];
        assert!((res.vertical.unwrap().distance - (10.0 - 0.027)).abs() < 1e-9);
        let radial = res.radial.unwrap().distance;
        assert!((radial - (sampled_floor(10.0, 0.5) - 0.027)).abs() < 1e-9);
        // A point 3 m to the side: 3 − 0.027.
        let pr = rule(
            "[[rule]]\nname = \"p\"\ntarget = \"points\"\nclasses = [5]\nradial = 2.99\nvertical = 2.99",
        );
        let set = WireSet::new(vec![a.clone()], 10.0);
        let mut pc = PointCheck::new(&pr, &[5], &set);
        pc.visit(&set, 50.0, 3.0, 10.0, 5);
        pc.visit(&set, 50.0, 0.0, 7.0, 5);
        let r = &pc.results[0];
        assert!((r.radial.unwrap().distance - 2.973).abs() < 1e-9);
        assert!((r.vertical.unwrap().distance - 2.973).abs() < 1e-9);
        assert!(r.violates());
        // Another wire 4 m away, radius 10 mm: 4 − 0.027 − 0.010.
        let b = thick(
            straight("B", "L2", [0.0, 4.0, 10.0], [100.0, 4.0, 10.0]),
            0.010,
            0.0,
        );
        let pp = rule("[[rule]]\nname = \"pp\"\ntarget = \"other_lines\"\nradial = 4.0");
        let other = WireSet::new(vec![b], 10.0);
        let pairs = wire_pairs(&pp, &[a], &other, PairMode::OtherLines);
        let d = pairs[0].result.radial.unwrap().distance;
        assert!((d - (4.0 - 0.037)).abs() < 1e-9, "{d}");
        assert!(pairs[0].result.violates());
    }

    /// On a sloping wire the vertical meets the surface r / cos β from
    /// the axis, not r (physics review 2026-10-26: iced radius 30 mm on
    /// 30°, 4.6 mm; a bundle's lowest subconductor 0.25 m down on 30°,
    /// 39 mm); never more than r plus the piece's own rise (a vertical
    /// piece: its end); a level piece as before, bit for bit.
    #[test]
    fn the_vertical_takes_the_radius_off_along_the_vertical() {
        let level = ([0.0, 0.0, 10.0], [10.0, 0.0, 10.0]);
        assert_eq!(vertical_of(0.03, level.0, level.1), 0.03);
        assert_eq!(vertical_of(-0.25, level.0, level.1), -0.25);
        let up = 30f64.to_radians().tan() * 10.0;
        let (a, b) = ([0.0, 0.0, 10.0], [10.0, 0.0, 10.0 + up]);
        let r = vertical_of(0.03, a, b);
        assert!((r - 0.03 / 30f64.to_radians().cos()).abs() < 1e-15);
        assert!((r - 0.03 - 0.004_641).abs() < 1e-6, "{r}");
        let lo = vertical_of(-0.25, a, b);
        assert!((lo + 0.288_675).abs() < 1e-6, "{lo}");
        // Vertical, and very steep: capped at the offset plus the rise.
        assert_eq!(vertical_of(0.03, [0.0; 3], [0.0, 0.0, 2.0]), 2.03);
        let steep = vertical_of(0.03, [0.0; 3], [1e-6, 0.0, 2.0]);
        assert!((steep - 2.03).abs() < 1e-12, "{steep}");
        // The ground check of a straight wire rising 30° over flat ground:
        // its lowest clearance at its low end, the surface r / cos β below.
        let flat: Vec<SurveyPoint> = [(-10.0, -20.0), (110.0, -20.0), (110.0, 20.0), (-10.0, 20.0)]
            .iter()
            .map(|&(x, y)| SurveyPoint {
                x,
                y,
                z: 0.0,
                code: String::new(),
                line: String::new(),
            })
            .collect();
        let terrain = Terrain::from_points(&flat, None).unwrap();
        let w = Wire {
            surface_radius: 0.03,
            ..straight("A", "L1", [0.0, 0.0, 10.0], [100.0, 0.0, 10.0 + 10.0 * up])
        };
        let g = rule("[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 9.0");
        let res = &ground_check(&g, std::slice::from_ref(&w), &terrain)[0];
        let v = res.vertical.unwrap().distance;
        assert!(
            (v - (10.0 - 0.03 / 30f64.to_radians().cos())).abs() < 1e-9,
            "{v}"
        );
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
        // Less the sampling's bound (pieces of 1.6 m sampled 0.4 m apart).
        let expected = sampled_floor(expected, 0.4);
        assert!((radial - expected).abs() < 1e-6, "{radial} vs {expected}");
        assert!(res.radial_violations > 0 && res.vertical_violations == 0);
    }

    /// The vertical (owner, 2026-10-22): only targets whose plan position
    /// lies within the band of the wire count, the distance is the wire's
    /// height over the target at that plan position, and it is drawn
    /// vertical — straight up from the target, also at the band's edge.
    #[test]
    fn the_vertical_checks_only_the_band_and_is_drawn_vertical() {
        // A wire rising from 20 m to 30 m over 100 m along x: 25 m at
        // x = 50. Band 1 m, no depth, 10 m to keep.
        let wire = straight("S", "L1", [0.0, 0.0, 20.0], [100.0, 0.0, 30.0]);
        let v = rule(
            "[[rule]]\nname = \"v\"\ntarget = \"points\"\nclasses = [5]\nvertical = 10.0\n\
             vertical_band = \"1 m\"\nvertical_depth = \"0 m\"",
        );
        let set = WireSet::new(vec![wire.clone()], 20.0);
        // 0.9 m beside the wire, 18 m up: checked, 25 − 18 = 7 m.
        let mut pc = PointCheck::new(&v, &[5], &set);
        pc.visit(&set, 50.0, 0.9, 18.0, 5);
        let hit = pc.results[0].vertical.expect("within the band");
        assert!((hit.distance - 7.0).abs() < 1e-9, "{hit:?}");
        assert_eq!(pc.results[0].vertical_violations, 1);
        // Drawn vertical: the same x and y at both ends, from the point up
        // to the wire's height there (not to the wire point 0.9 m beside).
        let (top, bottom) = hit.drawn(true);
        assert_eq!([top[0], top[1]], [50.0, 0.9]);
        assert_eq!(bottom, [50.0, 0.9, 18.0]);
        assert!((top[2] - 25.0).abs() < 1e-9);
        // Radially it stays the 3D segment to the wire point.
        assert_eq!(hit.drawn(false).0, hit.wire);
        // 1.1 m beside it, even nearer below the wire: ignored entirely
        // (no sloped or 3D distance).
        let mut pc = PointCheck::new(&v, &[5], &set);
        pc.visit(&set, 50.0, 1.1, 24.0, 5);
        pc.visit(&set, 50.0, -1.1, 24.0, 5);
        assert!(pc.results[0].vertical.is_none());
        assert_eq!(pc.results[0].vertical_violations, 0);
        // The ground: a plane rising 1 m per metre to the left (+y): the
        // highest ground within the band is at its edge, y = 1 m, z = 1 m;
        // beyond it (higher) is not counted.
        let mut points = Vec::new();
        for i in 0..=20 {
            for j in -20..=20 {
                let y = j as f64 * 0.5;
                points.push(SurveyPoint {
                    x: i as f64 * 5.0,
                    y,
                    z: y,
                    code: String::new(),
                    line: String::new(),
                });
            }
        }
        let terrain = Terrain::from_points(&points, None).unwrap();
        let g = rule(
            "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 30.0\n\
             vertical_band = \"1 m\"\nvertical_depth = \"0 m\"",
        );
        let res = &ground_check(&g, std::slice::from_ref(&wire), &terrain)[0];
        let hit = res.vertical.unwrap();
        // The lowest: at the wire's start, 20 m over ground 1 m high.
        assert!((hit.distance - 19.0).abs() < 1e-9, "{hit:?}");
        assert!((hit.target[1] - 1.0).abs() < 1e-9, "{hit:?}");
        let (top, bottom) = hit.drawn(true);
        assert_eq!([top[0], top[1]], [bottom[0], bottom[1]]);
        assert!((top[2] - bottom[2] - 19.0).abs() < 1e-9);
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
        assert!((res.radial.unwrap().distance - sampled_floor(9.8, 0.4)).abs() < 1e-9);
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
