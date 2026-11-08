// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Wind on the members of a lattice structure with **faces**, their
//! **solidity ratios** and **shielding** (theory: `docs/theory/structure-wind.md`).
//!
//! The geometry is OpenOHL's: the faces of the lattice (members in one
//! plane, connected, with everything else on one side of the plane: the
//! outer faces of a body or an arm), each face's outline (the convex hull
//! of its joints), its solidity ratio (the members' area seen normal to
//! the face over the outline's area), and per wind direction which face
//! stands in front of which member. The **factors** are data
//! ([`ohl_sections::StructureWind`], the `[wind]` table of the section
//! library, from a design-code plugin or the user): a drag factor per
//! shape, a face's drag factor by its solidity ratio, a shielding factor
//! by the solidity ratio of the face in front (and the spacing ratio).
//! Without them every profile takes the model's one `drag` and nothing is
//! shielded, as before; the notes say which.

use std::collections::{BTreeSet, HashMap, HashSet};

use ohl_sections::StructureWind;

use crate::model::{Model, NodalLoad, cross, dot, norm, sub};

/// Two members at a joint closer to parallel than this (sine of the
/// angle) do not define a plane.
const PARALLEL: f64 = 0.0175;
/// How a wind note marks what is not on the safe side (as
/// [`crate::dead_load_note`]).
pub const NOT_SAFE_WIND: &str = "NOT ON THE SAFE SIDE";
/// A face takes part in a wind direction when |n · w| is above this (an
/// edge-on face neither shelters nor is sheltered).
const FACING: f64 = 1e-3;

/// A face of the lattice: members in one plane, connected, the rest of
/// the structure on one side of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Its members (indices into [`Model::members`]), rising.
    pub members: Vec<usize>,
    /// Unit normal.
    pub normal: [f64; 3],
    /// `normal · p` for the points p of the plane, m.
    pub offset: f64,
    /// In-plane axis the breadth is measured across: global z in the
    /// plane (for a horizontal face, an in-plane axis).
    pub up: [f64; 3],
    /// The outline: convex hull of the face's joints, in order (3D
    /// points).
    pub outline: Vec<[f64; 3]>,
    /// Area of the outline, m².
    pub area: f64,
    /// Outline's extent along `up`, m.
    pub height: f64,
    /// Solidity ratio: the members' area seen along the normal (profiles
    /// only) over the outline's area — members overlapping at the joints
    /// counted twice (an upper bound, gusset plates aside).
    pub solidity: f64,
    /// An upper bound on the area counted twice in [`Face::solidity`]
    /// where members overlap (at the joints, where they cross), m²: the
    /// solidity with each overlap once is at least
    /// [`Face::solidity_low`].
    pub overlap: f64,
    /// Gusset plates' area in the face, m² ([`MemberWindSetup::with_plates`];
    /// 0: none given).
    pub plates: f64,
}

impl Face {
    /// The lowest the face's true solidity ratio can be: the members'
    /// area less the bound on their overlaps, over the outline's area.
    pub fn solidity_low(&self) -> f64 {
        if self.area > 0.0 {
            (self.solidity - self.overlap / self.area).max(0.0)
        } else {
            0.0
        }
    }

    /// The highest it can be: the members' area with the overlaps
    /// counted twice and the gusset plates given.
    pub fn solidity_high(&self) -> f64 {
        if self.area > 0.0 {
            self.solidity + self.plates / self.area
        } else {
            self.solidity
        }
    }

    /// Mean breadth across `up`: area over height, m.
    pub fn breadth(&self) -> f64 {
        if self.height > 0.0 {
            self.area / self.height
        } else {
            0.0
        }
    }

    /// In-plane coordinates (along `up`, along normal × up).
    fn plane(&self, p: [f64; 3]) -> [f64; 2] {
        let v = cross(self.normal, self.up);
        [dot(p, self.up), dot(p, v)]
    }

    /// Whether `p` (on the plane) is inside the outline, within `tol`.
    fn contains(&self, p: [f64; 3], tol: f64) -> bool {
        let q = self.plane(p);
        let hull: Vec<[f64; 2]> = self.outline.iter().map(|c| self.plane(*c)).collect();
        (0..hull.len()).all(|k| {
            let (a, b) = (hull[k], hull[(k + 1) % hull.len()]);
            let e = [b[0] - a[0], b[1] - a[1]];
            let len = (e[0] * e[0] + e[1] * e[1]).sqrt();
            len <= 0.0 || (e[0] * (q[1] - a[1]) - e[1] * (q[0] - a[0])) / len >= -tol
        })
    }
}

/// Distance tolerance of a model: 1 mm plus a millionth of its size.
fn tolerance(model: &Model) -> f64 {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for n in &model.nodes {
        for k in 0..3 {
            lo[k] = lo[k].min(n.point[k]);
            hi[k] = hi[k].max(n.point[k]);
        }
    }
    let size = if model.nodes.is_empty() {
        0.0
    } else {
        norm(sub(hi, lo))
    };
    1e-3 + 1e-6 * size
}

/// The normal with a fixed sign: its largest component positive.
fn canonical(n: [f64; 3]) -> [f64; 3] {
    let k = (0..3)
        .max_by(|a, b| n[*a].abs().total_cmp(&n[*b].abs()))
        .unwrap_or(0);
    if n[k] < 0.0 { n.map(|c| -c) } else { n }
}

/// Convex hull of 2D points (monotone chain), counter-clockwise.
fn hull(mut pts: Vec<([f64; 2], [f64; 3])>) -> Vec<([f64; 2], [f64; 3])> {
    pts.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])));
    pts.dedup_by(|a, b| (a.0[0] - b.0[0]).abs() < 1e-12 && (a.0[1] - b.0[1]).abs() < 1e-12);
    if pts.len() < 3 {
        return pts;
    }
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut out: Vec<([f64; 2], [f64; 3])> = Vec::new();
    for pass in 0..2 {
        let start = out.len();
        let iter: Box<dyn Iterator<Item = &([f64; 2], [f64; 3])>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for p in iter {
            while out.len() >= start + 2
                && turn(out[out.len() - 2].0, out[out.len() - 1].0, p.0) <= 0.0
            {
                out.pop();
            }
            out.push(*p);
        }
        out.pop();
    }
    out
}

/// The faces of `model`'s lattice: for every pair of members meeting at a
/// joint (not parallel), the plane through them; per plane the members
/// lying in it (both ends within the tolerance), split into connected
/// sets of at least three members; a set is a face when every member
/// leaving its joints out of the plane goes to one side (an outer face,
/// not a plane through the middle of a body), its outline has an area,
/// and no larger face holds all its members.
///
/// Cost: one plane per pair of members at a joint (a few tens per
/// joint), each checked against every member: a few milliseconds for a
/// tower of a few hundred members.
pub fn faces(model: &Model) -> Vec<Face> {
    let tol = tolerance(model);
    let point = |n: usize| model.nodes[n].point;
    let mut at: Vec<Vec<usize>> = vec![Vec::new(); model.nodes.len()];
    for (k, m) in model.members.iter().enumerate() {
        at[m.i].push(k);
        at[m.j].push(k);
    }
    // Candidate planes, rounded to find the same one twice.
    let mut planes: Vec<([f64; 3], f64)> = Vec::new();
    let mut seen: HashSet<[i64; 4]> = HashSet::new();
    for (n, members) in at.iter().enumerate() {
        let p = point(n);
        let dir = |k: usize| {
            let m = &model.members[k];
            let other = if m.i == n { m.j } else { m.i };
            let d = sub(point(other), p);
            let l = norm(d);
            d.map(|c| c / l)
        };
        for (a, &ka) in members.iter().enumerate() {
            for &kb in &members[a + 1..] {
                let c = cross(dir(ka), dir(kb));
                let s = norm(c);
                if s.is_nan() || s <= PARALLEL {
                    continue;
                }
                let normal = canonical(c.map(|x| x / s));
                let offset = dot(normal, p);
                let key = [
                    (normal[0] * 1e4).round() as i64,
                    (normal[1] * 1e4).round() as i64,
                    (normal[2] * 1e4).round() as i64,
                    (offset / tol).round() as i64,
                ];
                if seen.insert(key) {
                    planes.push((normal, offset));
                }
            }
        }
    }
    let mut sets: Vec<(Vec<usize>, [f64; 3], f64)> = Vec::new();
    let mut found: HashSet<Vec<usize>> = HashSet::new();
    for (normal, offset) in planes {
        let on = |n: usize| (dot(normal, point(n)) - offset).abs() <= tol;
        let inside: Vec<usize> = (0..model.members.len())
            .filter(|k| on(model.members[*k].i) && on(model.members[*k].j))
            .collect();
        if inside.len() < 3 {
            continue;
        }
        for set in connected(model, &inside) {
            if set.len() >= 3 && found.insert(set.clone()) {
                sets.push((set, normal, offset));
            }
        }
    }
    // Outer faces only: the members leaving the set's joints all go to
    // one side of the plane.
    sets.retain(|(set, normal, offset)| {
        let joints: BTreeSet<usize> = set
            .iter()
            .flat_map(|k| [model.members[*k].i, model.members[*k].j])
            .collect();
        let mut sides = [false; 2];
        for &n in &joints {
            for &k in &at[n] {
                let m = &model.members[k];
                let other = if m.i == n { m.j } else { m.i };
                let d = dot(*normal, point(other)) - offset;
                if d > tol {
                    sides[0] = true;
                } else if d < -tol {
                    sides[1] = true;
                }
            }
        }
        !(sides[0] && sides[1])
    });
    // No face inside a larger one (the same plane found from a short pair
    // at a slightly different angle).
    let all: Vec<BTreeSet<usize>> = sets.iter().map(|s| s.0.iter().copied().collect()).collect();
    let mut out = Vec::new();
    for (k, (set, normal, offset)) in sets.iter().enumerate() {
        let inside_larger = all
            .iter()
            .enumerate()
            .any(|(j, o)| j != k && o.len() > all[k].len() && all[k].is_subset(o));
        if inside_larger {
            continue;
        }
        if let Some(face) = make_face(model, set.clone(), *normal, *offset) {
            out.push(face);
        }
    }
    out
}

/// The members of `inside` split into sets joined at joints.
fn connected(model: &Model, inside: &[usize]) -> Vec<Vec<usize>> {
    let mut by_joint: HashMap<usize, Vec<usize>> = HashMap::new();
    for &k in inside {
        let m = &model.members[k];
        by_joint.entry(m.i).or_default().push(k);
        by_joint.entry(m.j).or_default().push(k);
    }
    let mut done: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for &k in inside {
        if !done.insert(k) {
            continue;
        }
        let mut set = vec![k];
        let mut next = vec![k];
        while let Some(c) = next.pop() {
            let m = &model.members[c];
            for j in [m.i, m.j] {
                for &o in by_joint.get(&j).into_iter().flatten() {
                    if done.insert(o) {
                        set.push(o);
                        next.push(o);
                    }
                }
            }
        }
        set.sort_unstable();
        out.push(set);
    }
    out
}

fn make_face(model: &Model, members: Vec<usize>, normal: [f64; 3], offset: f64) -> Option<Face> {
    // Global z in the plane; for a horizontal face, an in-plane axis.
    let z = [0.0, 0.0, 1.0];
    let zin = sub(z, normal.map(|c| c * dot(z, normal)));
    let up = if norm(zin) > 1e-6 {
        zin.map(|c| c / norm(zin))
    } else {
        let x = [1.0, 0.0, 0.0];
        let xin = sub(x, normal.map(|c| c * dot(x, normal)));
        let xin = if norm(xin) > 1e-6 {
            xin
        } else {
            [0.0, 1.0, 0.0]
        };
        xin.map(|c| c / norm(xin))
    };
    let v = cross(normal, up);
    let joints: BTreeSet<usize> = members
        .iter()
        .flat_map(|k| [model.members[*k].i, model.members[*k].j])
        .collect();
    let pts: Vec<([f64; 2], [f64; 3])> = joints
        .iter()
        .map(|n| {
            let p = model.nodes[*n].point;
            ([dot(p, up), dot(p, v)], p)
        })
        .collect();
    let h = hull(pts);
    if h.len() < 3 {
        return None;
    }
    let area = 0.5
        * (0..h.len())
            .map(|k| {
                let (a, b) = (h[k].0, h[(k + 1) % h.len()].0);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>();
    if area.is_nan() || area <= 1e-9 {
        return None;
    }
    let (lo, hi) = h
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.0[0]), hi.max(p.0[0]))
        });
    let shown: f64 = members
        .iter()
        .filter_map(|k| model.projected_area(*k, normal))
        .sum();
    let overlap = overlap_bound(model, &members, normal, up, v);
    Some(Face {
        members,
        normal,
        offset,
        up,
        outline: h.into_iter().map(|p| p.1).collect(),
        area,
        height: hi - lo,
        solidity: shown / area,
        overlap,
        plates: 0.0,
    })
}

/// A member's silhouette in a face: the strip of its profile's width
/// (seen along the face's normal) round its centre line, in the face's
/// in-plane coordinates (counter-clockwise), with its axis.
struct Strip {
    corners: [[f64; 2]; 4],
    start: [f64; 2],
    along: [f64; 2],
    width: f64,
}

fn strip(model: &Model, k: usize, normal: [f64; 3], up: [f64; 3], v: [f64; 3]) -> Option<Strip> {
    let m = &model.members[k];
    let len = model.length(m);
    let area = model.projected_area(k, normal)?;
    if len <= 0.0 || area <= 0.0 {
        return None;
    }
    let to2 = |p: [f64; 3]| [dot(p, up), dot(p, v)];
    let (a, b) = (to2(model.nodes[m.i].point), to2(model.nodes[m.j].point));
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if l2 <= 0.0 {
        return None;
    }
    let along = [d[0] / l2, d[1] / l2];
    let width = area / len;
    let h = [-along[1] * width / 2.0, along[0] * width / 2.0];
    Some(Strip {
        corners: [
            [a[0] - h[0], a[1] - h[1]],
            [b[0] - h[0], b[1] - h[1]],
            [b[0] + h[0], b[1] + h[1]],
            [a[0] + h[0], a[1] + h[1]],
        ],
        start: a,
        along,
        width,
    })
}

/// The part of convex polygon `subject` inside convex polygon `clip`
/// (both counter-clockwise; Sutherland–Hodgman).
fn clip_convex(subject: &[[f64; 2]], clip: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = subject.to_vec();
    for k in 0..clip.len() {
        if out.is_empty() {
            break;
        }
        let (a, b) = (clip[k], clip[(k + 1) % clip.len()]);
        let side = |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
    }
    out
}

/// An upper bound on the area the members' strips of a face share
/// (counted twice in the sum of their areas): each strip, in turn, can
/// share with the strips before it at most its width times the length
/// of its axis their overlaps cover. So the union of the strips is at
/// least their sum less this, whatever the number of members meeting at
/// a joint (a sum over pairs would take a joint of k members k(k−1)/2
/// times).
fn overlap_bound(
    model: &Model,
    members: &[usize],
    normal: [f64; 3],
    up: [f64; 3],
    v: [f64; 3],
) -> f64 {
    let strips: Vec<Strip> = members
        .iter()
        .filter_map(|k| strip(model, *k, normal, up, v))
        .collect();
    let bbox = |s: &Strip| {
        s.corners.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |b, p| {
                [
                    b[0].min(p[0]),
                    b[1].min(p[1]),
                    b[2].max(p[0]),
                    b[3].max(p[1]),
                ]
            },
        )
    };
    let boxes: Vec<[f64; 4]> = strips.iter().map(bbox).collect();
    let mut total = 0.0;
    for (i, s) in strips.iter().enumerate() {
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for (j, o) in strips.iter().enumerate().take(i) {
            let (a, b) = (boxes[i], boxes[j]);
            if a[0] > b[2] || b[0] > a[2] || a[1] > b[3] || b[1] > a[3] {
                continue;
            }
            let shared = clip_convex(&s.corners, &o.corners);
            if shared.len() < 3 {
                continue;
            }
            let at =
                |p: &[f64; 2]| (p[0] - s.start[0]) * s.along[0] + (p[1] - s.start[1]) * s.along[1];
            let lo = shared.iter().map(at).fold(f64::INFINITY, f64::min);
            let hi = shared.iter().map(at).fold(f64::NEG_INFINITY, f64::max);
            if hi > lo {
                spans.push((lo, hi));
            }
        }
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut covered = 0.0;
        let mut reach = f64::NEG_INFINITY;
        for (lo, hi) in spans {
            let lo = lo.max(reach);
            if hi > lo {
                covered += hi - lo;
                reach = hi;
            }
        }
        total += covered * s.width;
    }
    total
}

/// Whether wind factors need the faces (a face drag or shielding).
pub fn needs_faces(factors: Option<&StructureWind>) -> bool {
    factors.is_some_and(|f| f.face_drag.is_some() || f.shielding.is_some())
}

/// Where a member's drag factor comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragFrom {
    /// The model's one `drag` (no factor for it in the data).
    Generic,
    /// The data's factor for its profile's shape.
    Shape,
    /// The data's face drag by the solidity ratio of its face.
    Face,
}

/// The wind on one member.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberWindRow {
    /// The member (index into [`Model::members`]).
    pub member: usize,
    /// Drag factor applied.
    pub drag: f64,
    /// Where it comes from.
    pub drag_from: DragFrom,
    /// Shielding factor applied (1: not shielded).
    pub shielding: f64,
    /// The face in front of it (index into the faces the wind was
    /// computed with, [`MemberWindSetup::faces`]) when it is shielded,
    /// with the spacing ratio.
    pub shielded_by: Option<(usize, f64)>,
    /// The force on it, N.
    pub force: f64,
}

/// The member wind of one wind direction, with how it was made.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemberWind {
    /// Loads at the joints (half of each member's at each end).
    pub loads: Vec<NodalLoad>,
    /// Per member with a profile and a force.
    pub members: Vec<MemberWindRow>,
    /// Per gusset plate facing the wind (its load at its joint comes
    /// after the members' in [`MemberWind::loads`]).
    pub plates: Vec<PlateWindRow>,
    /// What was applied and what was not, in words.
    pub notes: Vec<String>,
}

impl MemberWind {
    /// Each member's wind as one force (its two halves at its ends
    /// together), N, structure axes: for [`crate::Loads::member`], so the
    /// check takes the bending it puts on the member between its joints.
    pub fn member_forces(&self) -> Vec<(usize, [f64; 3])> {
        self.members
            .iter()
            .enumerate()
            .map(|(row, r)| {
                let mut f = [0.0; 3];
                for l in self.loads.iter().skip(2 * row).take(2) {
                    for (a, b) in f.iter_mut().zip(l.force) {
                        *a += b;
                    }
                }
                (r.member, f)
            })
            .collect()
    }

    /// The sum of the loads, N (structure axes).
    pub fn total(&self) -> [f64; 3] {
        let mut s = [0.0; 3];
        for l in &self.loads {
            for (a, f) in s.iter_mut().zip(l.force) {
                *a += f;
            }
        }
        s
    }

    /// The force on the member of row `row` of [`MemberWind::members`],
    /// N (structure axes): its two loads at the joints summed.
    pub fn member_force(&self, row: usize) -> [f64; 3] {
        let mut s = [0.0; 3];
        for l in self.loads.iter().skip(2 * row).take(2) {
            for (a, f) in s.iter_mut().zip(l.force) {
                *a += f;
            }
        }
        s
    }
}

/// Faces for a model, computed once and used for every wind direction
/// and pressure ([`MemberWindSetup::wind`]).
pub struct MemberWindSetup<'a> {
    model: &'a Model,
    factors: Option<&'a StructureWind>,
    faces: Vec<Face>,
    /// The gusset plates given (none: no joint details, said).
    plates: Option<Vec<WindPlate>>,
}

/// A gusset plate (or a splice's cover) in the wind: its area, the plane
/// it lies in and the joint it is at — from the joint details, which the
/// structure model itself does not hold.
#[derive(Debug, Clone, PartialEq)]
pub struct WindPlate {
    /// Its id, for notes.
    pub id: String,
    /// The joint (index into [`Model::nodes`]).
    pub node: usize,
    /// The area of one of its faces, m².
    pub area: f64,
    /// Unit normal of its plane.
    pub normal: [f64; 3],
    /// The members bolted onto it (indices into [`Model::members`]).
    pub members: Vec<usize>,
}

/// The wind on one gusset plate.
#[derive(Debug, Clone, PartialEq)]
pub struct PlateWindRow {
    /// The plate (index into the plates given).
    pub plate: usize,
    /// Drag factor applied: the largest of the members bolted onto it.
    pub drag: f64,
    /// The force on it, N.
    pub force: f64,
}

impl<'a> MemberWindSetup<'a> {
    /// The faces of `model` when `factors` need them (a face drag or
    /// shielding), else none.
    pub fn new(model: &'a Model, factors: Option<&'a StructureWind>) -> Self {
        Self {
            model,
            factors,
            faces: if needs_faces(factors) {
                faces(model)
            } else {
                Vec::new()
            },
            plates: None,
        }
    }

    /// The same with the faces found before (by [`faces`] on the same
    /// model).
    pub fn with_faces(
        model: &'a Model,
        factors: Option<&'a StructureWind>,
        faces: Vec<Face>,
    ) -> Self {
        Self {
            model,
            factors,
            faces,
            plates: None,
        }
    }

    /// The same with the joints' gusset plates in the wind: each takes
    /// q × c × A × |n · w| along the wind at its joint, c the largest drag
    /// factor of the members bolted onto it (of those at its joint when
    /// none is), not shielded; its area raises the upper bound of the
    /// solidity ratio of the face it lies in ([`Face::solidity_high`]).
    /// Without plates (the setup as made) every wind says, not on the
    /// safe side, that their own wind area is missing.
    pub fn with_plates(mut self, plates: Vec<WindPlate>) -> Self {
        let tol = tolerance(self.model);
        for face in &mut self.faces {
            let joints: BTreeSet<usize> = face
                .members
                .iter()
                .flat_map(|k| [self.model.members[*k].i, self.model.members[*k].j])
                .collect();
            face.plates = plates
                .iter()
                .filter(|p| {
                    joints.contains(&p.node)
                        && dot(p.normal, face.normal).abs() > 1.0 - PARALLEL
                        && (dot(face.normal, self.model.nodes[p.node].point) - face.offset).abs()
                            <= tol
                })
                .map(|p| p.area)
                .sum();
        }
        self.plates = Some(plates);
        self
    }

    /// The faces found.
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// The wind of `pressure` (Pa) blowing in `direction` (structure
    /// axes) on the members with a profile: per member q × c × η × d × L ×
    /// sin²θ ([`Model::member_wind`]), c the drag factor (the face's by
    /// its solidity, else the shape's, else the model's `drag`), η the
    /// shielding factor of a member every face of which facing the wind
    /// stands behind another face (1 otherwise).
    pub fn wind(&self, pressure: f64, direction: [f64; 3]) -> MemberWind {
        self.wind_iced(pressure, direction, &[])
    }

    /// [`MemberWindSetup::wind`] with members made wider: member k's width
    /// for the wind grows by `extra_width[k]` (m; none or 0: as it is) —
    /// an iced member, 2·t wider for a radial ice thickness t (a
    /// conservative assumption: the iced outline is not rounded, theory:
    /// `docs/theory/structure-wind.md`). The faces, their solidity ratios
    /// and the shielding stay those of the bare members. With no extra
    /// width it is exactly [`MemberWindSetup::wind`].
    pub fn wind_iced(&self, pressure: f64, direction: [f64; 3], extra_width: &[f64]) -> MemberWind {
        let model = self.model;
        let mut out = MemberWind::default();
        let n = norm(direction);
        if pressure == 0.0 || n.is_nan() || n <= 0.0 {
            return out;
        }
        let w = direction.map(|d| d / n);
        let tol = tolerance(model);
        let mut faces_of: HashMap<usize, Vec<usize>> = HashMap::new();
        for (f, face) in self.faces.iter().enumerate() {
            if dot(face.normal, w).abs() > FACING {
                for &k in &face.members {
                    faces_of.entry(k).or_default().push(f);
                }
            }
        }
        let generic = model.analysis.drag;
        let mut missing_shapes: BTreeSet<&str> = BTreeSet::new();
        let mut outside_face_drag = 0usize;
        let mut outside_shielding = 0usize;
        for (k, m) in model.members.iter().enumerate() {
            let Some((normal, sin, width)) = model.across(m, w) else {
                continue;
            };
            let width = match extra_width.get(k) {
                Some(&e) if e != 0.0 => width + e,
                _ => width,
            };
            let shape = model.sections[m.section]
                .profile
                .as_ref()
                .map_or("", |p| p.shape.as_str());
            let mine = faces_of.get(&k).map_or(&[][..], Vec::as_slice);
            // Drag: its face's (the one facing the wind most), else its
            // shape's, else the generic one.
            let mut drag = None;
            if let Some(curve) = self.factors.and_then(|f| f.face_drag.as_ref())
                && let Some(&f) = mine.iter().max_by(|a, b| {
                    dot(self.faces[**a].normal, w)
                        .abs()
                        .total_cmp(&dot(self.faces[**b].normal, w).abs())
                })
            {
                // Its face's solidity is known within bounds (overlaps,
                // plates): the largest drag over them, on the safe side.
                let face = &self.faces[f];
                match curve.largest_in(face.solidity_low(), face.solidity_high()) {
                    Some(c) => drag = Some((c, DragFrom::Face)),
                    // Outside the table: its largest face drag, or the
                    // shape's when larger (on the safe side, 2026-11-08;
                    // before, the shape's or the generic one).
                    None => {
                        outside_face_drag += 1;
                        let top = curve.factor.iter().copied().fold(0.0, f64::max);
                        let own = self.factors.and_then(|f| f.drag_of(shape)).unwrap_or(0.0);
                        drag = Some((top.max(own), DragFrom::Face));
                    }
                }
            }
            if drag.is_none() {
                drag = match self.factors.and_then(|f| f.drag_of(shape)) {
                    Some(c) => Some((c, DragFrom::Shape)),
                    None => {
                        if self.factors.is_some() {
                            missing_shapes.insert(shape);
                        }
                        Some((generic, DragFrom::Generic))
                    }
                };
            }
            let (drag, drag_from) = drag.unwrap_or((generic, DragFrom::Generic));
            // Shielding: a member of a face facing the wind whose middle,
            // seen along the wind, lies inside another face facing the
            // wind in front of it (the nearest one) takes the factor of
            // that face's solidity and the spacing ratio.
            let mut shielding = 1.0;
            let mut shielded_by = None;
            if let Some(table) = self.factors.and_then(|f| f.shielding.as_ref())
                && !mine.is_empty()
            {
                let (a, b) = (model.nodes[m.i].point, model.nodes[m.j].point);
                let mid = [
                    0.5 * (a[0] + b[0]),
                    0.5 * (a[1] + b[1]),
                    0.5 * (a[2] + b[2]),
                ];
                let front = self
                    .faces
                    .iter()
                    .enumerate()
                    .filter(|(_, face)| {
                        !face.members.contains(&k) && dot(face.normal, w).abs() > FACING
                    })
                    .filter_map(|(g, face)| {
                        let nw = dot(face.normal, w);
                        // How far back along the wind the face's plane is.
                        let t = (dot(face.normal, mid) - face.offset) / nw;
                        if t <= tol {
                            return None;
                        }
                        let p = [mid[0] - w[0] * t, mid[1] - w[1] * t, mid[2] - w[2] * t];
                        face.contains(p, tol).then_some((g, t * nw.abs()))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((g, gap)) = front {
                    let breadth = self.faces[g].breadth();
                    let spacing = if breadth > 0.0 {
                        gap / breadth
                    } else {
                        f64::INFINITY
                    };
                    // The least shelter over the front face's solidity
                    // bounds, on the safe side.
                    let face = &self.faces[g];
                    match table.largest_in(face.solidity_low(), face.solidity_high(), spacing) {
                        Some(eta) => {
                            shielding = eta;
                            shielded_by = Some((g, spacing));
                        }
                        None => outside_shielding += 1,
                    }
                }
            }
            // q·c·η·d·L·sin²θ along the unit normal: q·c·η·d·L·sin θ × normal.
            let f = pressure * drag * shielding * width * model.length(m) * sin;
            for node in [m.i, m.j] {
                out.loads.push(NodalLoad {
                    node,
                    force: normal.map(|c| 0.5 * f * c),
                    moment: [0.0; 3],
                });
            }
            out.members.push(MemberWindRow {
                member: k,
                drag,
                drag_from,
                shielding,
                shielded_by,
                force: f * sin,
            });
        }
        // The gusset plates, after the members (whose loads come two per
        // row first): q·c·A·|n·w| along the wind at their joint.
        for (p, plate) in self.plates.iter().flatten().enumerate() {
            let facing = dot(plate.normal, w).abs();
            if plate.area <= 0.0 || facing <= FACING {
                continue;
            }
            let largest = |pick: &dyn Fn(usize) -> bool| {
                out.members
                    .iter()
                    .filter(|r| pick(r.member))
                    .map(|r| r.drag)
                    .fold(None, |m: Option<f64>, c| Some(m.map_or(c, |m| m.max(c))))
            };
            let drag = largest(&|k| plate.members.contains(&k))
                .or_else(|| {
                    largest(&|k| {
                        let m = &model.members[k];
                        m.i == plate.node || m.j == plate.node
                    })
                })
                .unwrap_or(model.analysis.drag);
            let f = pressure * drag * plate.area * facing;
            out.loads.push(NodalLoad {
                node: plate.node,
                force: w.map(|c| f * c),
                moment: [0.0; 3],
            });
            out.plates.push(PlateWindRow {
                plate: p,
                drag,
                force: f,
            });
        }
        out.notes = self.notes(&out, &missing_shapes, outside_face_drag, outside_shielding);
        if !out.members.is_empty() {
            out.notes.push(self.plates_note(&out));
        }
        out
    }

    /// The gusset plates in words: their wind, or — none given — that
    /// their own wind area is missing (not on the safe side).
    fn plates_note(&self, out: &MemberWind) -> String {
        let Some(plates) = &self.plates else {
            return format!(
                "{NOT_SAFE_WIND}: gusset plates' own wind area not included (no joint \
                 details here; OHL-Structure adds them from the joints)"
            );
        };
        let area: f64 = plates.iter().map(|p| p.area).sum();
        let force: f64 = out.plates.iter().map(|r| r.force).sum();
        format!(
            "gusset plates: {} plate(s), {area:.2} m², {:.2} kN on the {} facing the wind \
             (the largest drag of the members bolted onto each, not shielded)",
            plates.len(),
            force / 1000.0,
            out.plates.len()
        )
    }

    fn notes(
        &self,
        out: &MemberWind,
        missing_shapes: &BTreeSet<&str>,
        outside_face_drag: usize,
        outside_shielding: usize,
    ) -> Vec<String> {
        let generic = self.model.analysis.drag;
        let count = |from: DragFrom| out.members.iter().filter(|r| r.drag_from == from).count();
        let mut notes = Vec::new();
        let Some(f) = self.factors.filter(|f| !f.is_empty()) else {
            notes.push(format!(
                "member wind: drag factor {generic} on every profile (the model's generic drag: \
                 no code drag factors given); no shielding (no shielding factors given)"
            ));
            return notes;
        };
        let source = f.source_name();
        let (by_face, by_shape, by_generic) = (
            count(DragFrom::Face),
            count(DragFrom::Shape),
            count(DragFrom::Generic),
        );
        let mut drag = format!(
            "member wind: drag factors from {source}: {by_face} member(s) by their face's \
             solidity, {by_shape} by their shape, {by_generic} the generic {generic}"
        );
        let missing: Vec<&str> = missing_shapes
            .iter()
            .copied()
            .filter(|s| !s.is_empty())
            .collect();
        if by_generic > 0 && !missing.is_empty() {
            drag.push_str(&format!(
                " (no drag factor given for {})",
                missing.join(", ")
            ));
        }
        if outside_face_drag > 0 {
            drag.push_str(&format!(
                "; {outside_face_drag} member(s) on faces whose solidity is outside the face \
                 drag table took the table's largest face drag (or their shape's when larger: on \
                 the safe side)"
            ));
        }
        notes.push(drag);
        match &f.shielding {
            None => notes.push("no shielding (no shielding factors given)".into()),
            Some(_) => {
                let shielded: Vec<f64> = out
                    .members
                    .iter()
                    .filter(|r| r.shielded_by.is_some())
                    .map(|r| r.shielding)
                    .collect();
                let mut s = if self.faces.is_empty() {
                    "shielding: no lattice faces found, nothing shielded".to_owned()
                } else if shielded.is_empty() {
                    format!(
                        "shielding from {source}: {} face(s) found, no member behind another face",
                        self.faces.len()
                    )
                } else {
                    let lo = shielded.iter().copied().fold(f64::INFINITY, f64::min);
                    let hi = shielded.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    format!(
                        "shielding from {source}: {} member(s) behind another face of {} face(s) \
                         found, factors {lo:.2}–{hi:.2}",
                        shielded.len(),
                        self.faces.len()
                    )
                };
                if outside_shielding > 0 {
                    s.push_str(&format!(
                        "; {outside_shielding} member(s) behind a face outside the shielding \
                         table not shielded"
                    ));
                }
                notes.push(s);
            }
        }
        notes
    }
}
