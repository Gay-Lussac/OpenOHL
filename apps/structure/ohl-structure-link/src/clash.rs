// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Steel in the same place at a joint (stage 1 of the automatic joint
//! layout, owner 2026-11-18: "so that it's physically doable (no crossing
//! members or beams)"). Every solid at a joint — each member's steel as
//! placed (from where it stops to half its length: the other half is the
//! other joint's), the plates and a splice's covers, the bolts with their
//! heads and nuts — is tested against every other it does not bolt to.
//!
//! What bolts to what, and so is not tested:
//!
//! - a member and what it bolts onto (`onto`, `other_onto`: a member, a
//!   plate, a splice's covers), the member it runs on into as one part;
//! - a plate and the member it lies on (its `on`, else the joint's leg;
//!   and a member running on in line
//!   through the joint, within 20°), the plates of one welded plate;
//! - a bolt and its own member (it passes through it, its head on it); a
//!   bolt's shank and what its member bolts onto (it passes through: the
//!   head and the nut are still tested, they lie on its faces).
//!
//! Each solid is cut into convex pieces (a profile's outline into convex
//! polygons run along the member; a bolt's head, shank and nut) and each
//! pair of pieces tested by separating axes (the faces' normals and the
//! edges' cross products: exact for convex solids): they **clash** when
//! they run into each other by more than [`CLASH_TOLERANCE`] along every
//! axis — the depth is the least of those overlaps. Faces lying on each
//! other (a leg on a leg, a nut on a flange) touch, they do not clash.
//!
//! Display and detailing only: no calculation reads it.

use std::collections::BTreeMap;

use ohl_core::geometry::cross;

use crate::joints::{Arrival, BoltPart, PlateDetail, SpliceDetail};
use crate::member_solids::{ear_clip, outline_at, steel_start};

/// How far two solids may run into each other and still only touch, m:
/// faces lying on each other and the drawings' rounding (an assumption of
/// the program, not a standard's value).
pub const CLASH_TOLERANCE: f64 = 0.001;

/// A part at a joint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClashPart {
    /// A member's steel.
    Member(String),
    /// A gusset plate.
    Plate(String),
    /// A splice's cover: the splice and the cover in words.
    Cover(String, String),
    /// A bolt of a member end (its place among the end's bolts, as
    /// [`Arrival::bolts_with_axes`] gives them) and the piece that runs
    /// deepest into the other part.
    Bolt {
        /// The member whose end it bolts.
        member: String,
        /// Its place among the end's bolts (0: the first).
        index: usize,
        /// Head, shank or nut.
        piece: BoltPiece,
    },
}

/// A bolt's piece, ordered for the clash list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BoltPiece {
    /// The head.
    Head,
    /// The shank.
    Shank,
    /// The nut.
    Nut,
}

impl BoltPiece {
    fn of(p: BoltPart) -> Self {
        match p {
            BoltPart::Head => Self::Head,
            BoltPart::Shank => Self::Shank,
            BoltPart::Nut => Self::Nut,
        }
    }

    /// In words.
    pub fn words(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Shank => "shank",
            Self::Nut => "nut",
        }
    }
}

impl ClashPart {
    /// In words: "D12", "plate G2", "the nut of D12's bolt 2".
    pub fn words(&self) -> String {
        match self {
            Self::Member(m) => m.clone(),
            Self::Plate(p) => format!("plate {p}"),
            Self::Cover(s, c) => format!("splice {s}'s {c} cover"),
            Self::Bolt {
                member,
                index,
                piece,
            } => format!("the {} of {member}'s bolt {}", piece.words(), index + 1),
        }
    }

    /// The member it is or belongs to (a member's steel, a bolt of its
    /// end); none for a plate or a cover.
    pub fn member(&self) -> Option<&str> {
        match self {
            Self::Member(m) | Self::Bolt { member: m, .. } => Some(m),
            _ => None,
        }
    }

    /// The same part, whatever piece of a bolt.
    fn key(&self) -> ClashPart {
        match self {
            Self::Bolt { member, index, .. } => Self::Bolt {
                member: member.clone(),
                index: *index,
                piece: BoltPiece::Head,
            },
            other => other.clone(),
        }
    }
}

/// Two parts of a joint in the same place.
#[derive(Debug, Clone, PartialEq)]
pub struct Clash {
    /// One part.
    pub a: ClashPart,
    /// The other.
    pub b: ClashPart,
    /// How deep they run into each other, m (the least overlap over the
    /// separating axes of the deepest pair of pieces).
    pub depth: f64,
    /// About where: the middle of the two pieces' common box, m.
    pub at: [f64; 3],
}

impl Clash {
    /// In words: "D12 and D13 run 6 mm into each other".
    pub fn words(&self) -> String {
        format!(
            "{} and {} run {:.0} mm into each other",
            self.a.words(),
            self.b.words(),
            (self.depth * 1000.0).max(1.0)
        )
    }

    /// It involves `part` (a bolt: any piece).
    pub fn involves(&self, part: &ClashPart) -> bool {
        self.a.key() == part.key() || self.b.key() == part.key()
    }

    /// It involves plate `id`.
    pub fn involves_plate(&self, id: &str) -> bool {
        matches!(&self.a, ClashPart::Plate(p) if p == id)
            || matches!(&self.b, ClashPart::Plate(p) if p == id)
    }

    /// The other part than one of member `m` (its steel or bolts); none
    /// when neither is `m`'s.
    pub fn other_than(&self, m: &str) -> Option<&ClashPart> {
        if self.a.member() == Some(m) {
            Some(&self.b)
        } else if self.b.member() == Some(m) {
            Some(&self.a)
        } else {
            None
        }
    }
}

/// A convex piece of a part: its corners, its faces' normals and its
/// edges' directions (each once, either way), its box.
#[derive(Debug, Clone)]
struct Piece {
    part: usize,
    bolt: Option<BoltPiece>,
    pts: Vec<[f64; 3]>,
    normals: Vec<[f64; 3]>,
    edges: Vec<[f64; 3]>,
    lo: [f64; 3],
    hi: [f64; 3],
}

impl Piece {
    /// A convex solid from its faces (each a planar convex polygon).
    fn from_faces(part: usize, bolt: Option<BoltPiece>, faces: &[Vec<[f64; 3]>]) -> Option<Self> {
        let mut pts: Vec<[f64; 3]> = Vec::new();
        let mut normals: Vec<[f64; 3]> = Vec::new();
        let mut edges: Vec<[f64; 3]> = Vec::new();
        for f in faces {
            if f.len() < 3 {
                continue;
            }
            for p in f {
                if !pts.iter().any(|q| norm(sub(*p, *q)) < 1e-9) {
                    pts.push(*p);
                }
            }
            if let Some(n) = newell(f) {
                add_direction(&mut normals, n);
            }
            for k in 0..f.len() {
                let e = sub(f[(k + 1) % f.len()], f[k]);
                let l = norm(e);
                if l > 1e-9 {
                    add_direction(&mut edges, scale(e, 1.0 / l));
                }
            }
        }
        if pts.len() < 4 {
            return None;
        }
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &pts {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        Some(Self {
            part,
            bolt,
            pts,
            normals,
            edges,
            lo,
            hi,
        })
    }

    /// The convex prism of `near` (a planar convex polygon) run to `far`
    /// (the same polygon moved).
    fn prism(part: usize, near: &[[f64; 3]], far: &[[f64; 3]]) -> Option<Self> {
        let n = near.len();
        if n < 3 || far.len() != n {
            return None;
        }
        let mut faces = vec![near.to_vec(), far.to_vec()];
        for k in 0..n {
            let k2 = (k + 1) % n;
            faces.push(vec![near[k], near[k2], far[k2], far[k]]);
        }
        Self::from_faces(part, None, &faces)
    }

    fn span(&self, axis: [f64; 3]) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for p in &self.pts {
            let d = dot(*p, axis);
            lo = lo.min(d);
            hi = hi.max(d);
        }
        (lo, hi)
    }
}

/// `d` (unit) added to `list` unless it, or its opposite, is there.
fn add_direction(list: &mut Vec<[f64; 3]>, d: [f64; 3]) {
    if !list.iter().any(|e| norm(cross(*e, d)) < 1e-6) {
        list.push(d);
    }
}

/// How deep two convex pieces run into each other, m: the least overlap
/// of their spans over the separating axes; at most `tol` (or below 0)
/// when they are apart or only touch (it stops at the first such axis).
fn depth(a: &Piece, b: &Piece, tol: f64) -> f64 {
    let mut best = f64::INFINITY;
    // The boxes first (the coordinate axes separate as well as any).
    for k in 0..3 {
        let o = a.hi[k].min(b.hi[k]) - a.lo[k].max(b.lo[k]);
        if o <= tol {
            return o;
        }
        best = best.min(o);
    }
    let mut test = |axis: [f64; 3]| -> bool {
        let l = norm(axis);
        if l < 1e-9 {
            return true;
        }
        let u = scale(axis, 1.0 / l);
        let (a0, a1) = a.span(u);
        let (b0, b1) = b.span(u);
        let o = a1.min(b1) - a0.max(b0);
        best = best.min(o);
        o > tol
    };
    for n in a.normals.iter().chain(&b.normals) {
        if !test(*n) {
            return best;
        }
    }
    for e in &a.edges {
        for f in &b.edges {
            if !test(cross(*e, *f)) {
                return best;
            }
        }
    }
    best
}

/// A simple polygon (2D) cut into convex polygons: its ears, merged
/// where two sharing an edge stay convex. Corners' indices.
pub(crate) fn convex_parts(poly: &[[f64; 2]]) -> Vec<Vec<usize>> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let turn = if area >= 0.0 { 1.0 } else { -1.0 };
    let convex = |p: &[usize]| {
        let m = p.len();
        (0..m).all(|k| {
            let (a, b, c) = (poly[p[k]], poly[p[(k + 1) % m]], poly[p[(k + 2) % m]]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) * turn >= -1e-15
        })
    };
    let mut parts: Vec<Vec<usize>> = ear_clip(poly)
        .into_iter()
        .map(|t| {
            // In the outline's turning.
            let v = t.to_vec();
            if convex(&v) {
                v
            } else {
                vec![t[0], t[2], t[1]]
            }
        })
        .collect();
    loop {
        let mut merged = false;
        'pairs: for i in 0..parts.len() {
            for j in i + 1..parts.len() {
                if let Some(m) = merge(&parts[i], &parts[j])
                    && convex(&m)
                {
                    parts[i] = m;
                    parts.remove(j);
                    merged = true;
                    break 'pairs;
                }
            }
        }
        if !merged {
            break;
        }
    }
    parts
}

/// Two polygons (corners' indices, same turning) sharing an edge, as
/// one (none: no edge shared).
fn merge(p: &[usize], q: &[usize]) -> Option<Vec<usize>> {
    let (m, n) = (p.len(), q.len());
    for i in 0..m {
        let (a, b) = (p[i], p[(i + 1) % m]);
        // The same edge runs b → a in q.
        for j in 0..n {
            if q[j] == b && q[(j + 1) % n] == a {
                // p from b round to a, then q from a round to b (its
                // other corners).
                let mut out: Vec<usize> = (0..m).map(|k| p[(i + 1 + k) % m]).collect();
                out.extend((0..n - 2).map(|k| q[(j + 2 + k) % n]));
                return Some(out);
            }
        }
    }
    None
}

/// The parts of a joint and their pieces, and which pairs bolt together.
struct Parts {
    parts: Vec<ClashPart>,
    pieces: Vec<Piece>,
}

impl Parts {
    fn add(&mut self, part: ClashPart) -> usize {
        self.parts.push(part);
        self.parts.len() - 1
    }
}

/// The parts of a planar outline (3D, its corners in order) run along
/// `through` by `thickness`: convex pieces.
fn slab_pieces(part: usize, outline: &[[f64; 3]], through: [f64; 3], thickness: f64) -> Vec<Piece> {
    if outline.len() < 3 {
        return Vec::new();
    }
    let u0 = sub(outline[1], outline[0]);
    let u = scale(u0, 1.0 / norm(u0).max(1e-12));
    let n = newell(outline).unwrap_or(through);
    let v = cross(n, u);
    let flat: Vec<[f64; 2]> = outline
        .iter()
        .map(|q| {
            let d = sub(*q, outline[0]);
            [dot(d, u), dot(d, v)]
        })
        .collect();
    let mv = scale(through, thickness);
    convex_parts(&flat)
        .iter()
        .filter_map(|ix| {
            let near: Vec<[f64; 3]> = ix.iter().map(|&k| outline[k]).collect();
            let far: Vec<[f64; 3]> = near.iter().map(|p| add(*p, mv)).collect();
            Piece::prism(part, &near, &far)
        })
        .collect()
}

/// The clashes at a joint at `at` (m): its member ends `arrivals`, its
/// plates and splices as the joint details place them. Each pair of parts
/// once, the deepest pair of their pieces; in the parts' order (members,
/// plates, covers, bolts).
pub fn clashes(
    at: [f64; 3],
    arrivals: &[Arrival],
    plates: &[PlateDetail],
    splices: &[SpliceDetail],
    tol: f64,
) -> Vec<Clash> {
    let (p, bolt_of) = parts_of(at, arrivals, plates, splices);
    clashes_of(&p, &bolt_of, arrivals, plates, splices, tol)
}

/// The convex pieces of the parts at a joint at `at` (m) that `keep`
/// selects (the part, and for a bolt its head, shank or nut), each as its
/// corners from the work point, m (the automatic joint layout's fixed
/// steel, 2026-11-19: what a member end laid out keeps clear of).
pub(crate) fn piece_corners(
    at: [f64; 3],
    arrivals: &[Arrival],
    plates: &[PlateDetail],
    splices: &[SpliceDetail],
    keep: impl Fn(&ClashPart, Option<BoltPiece>) -> bool,
) -> Vec<(ClashPart, Vec<[f64; 3]>)> {
    let (p, _) = parts_of(at, arrivals, plates, splices);
    p.pieces
        .iter()
        .filter(|x| keep(&p.parts[x.part], x.bolt))
        .map(|x| {
            let part = match (&p.parts[x.part], x.bolt) {
                (ClashPart::Bolt { member, index, .. }, Some(piece)) => ClashPart::Bolt {
                    member: member.clone(),
                    index: *index,
                    piece,
                },
                (other, _) => other.clone(),
            };
            (part, x.pts.iter().map(|q| sub(*q, at)).collect())
        })
        .collect()
}

/// The parts at a joint and their pieces; each bolt's part and whether it
/// is of its end's other leg.
fn parts_of(
    at: [f64; 3],
    arrivals: &[Arrival],
    plates: &[PlateDetail],
    splices: &[SpliceDetail],
) -> (Parts, Vec<(usize, bool)>) {
    let mut p = Parts {
        parts: Vec::new(),
        pieces: Vec::new(),
    };
    // The members' steel near this joint: from where it stops to half its
    // length (the other half is the other joint's).
    for a in arrivals {
        let Some(prof) = a.profile.as_ref() else {
            continue;
        };
        let s0 = steel_start(a);
        let s1 = a.length / 2.0;
        if s1 - s0 < 1e-6 {
            continue;
        }
        let k = p.add(ClashPart::Member(a.member.clone()));
        for outline in prof.pieces() {
            for ix in convex_parts(outline) {
                let o: Vec<[f64; 2]> = ix.iter().map(|&i| outline[i]).collect();
                let near: Vec<[f64; 3]> = outline_at(a, prof, &o, s0)
                    .into_iter()
                    .map(|q| add(q, at))
                    .collect();
                let far: Vec<[f64; 3]> = outline_at(a, prof, &o, s1)
                    .into_iter()
                    .map(|q| add(q, at))
                    .collect();
                if let Some(x) = Piece::prism(k, &near, &far) {
                    p.pieces.push(x);
                }
            }
        }
    }
    for pl in plates {
        let Some(through) = pl.through else { continue };
        let k = p.add(ClashPart::Plate(pl.id.clone()));
        let pieces = slab_pieces(k, &pl.outline, through, pl.thickness);
        p.pieces.extend(pieces);
    }
    // Each cover with its splice.
    for s in splices {
        for c in &s.covers {
            let Some(through) = c.through else { continue };
            let k = p.add(ClashPart::Cover(s.id.clone(), c.words()));
            let pieces = slab_pieces(k, &c.outline, through, c.thickness);
            p.pieces.extend(pieces);
        }
    }
    // The bolts: head, shank and nut, each its own piece.
    let mut bolt_of: Vec<(usize, bool)> = Vec::new();
    for a in arrivals {
        let Some(b) = &a.bolts.value else { continue };
        let own = a.bolt_points.len();
        for (index, (q, axis, grip)) in a.bolts_with_axes().enumerate() {
            let k = p.add(ClashPart::Bolt {
                member: a.member.clone(),
                index,
                piece: BoltPiece::Head,
            });
            bolt_of.push((k, index >= own));
            let faces = crate::bolt_faces(q, axis, b.diameter, grip, b.drawn);
            for part in [BoltPart::Head, BoltPart::Shank, BoltPart::Nut] {
                let f: Vec<Vec<[f64; 3]>> = faces
                    .iter()
                    .filter(|(_, x)| *x == part)
                    .map(|(f, _)| f.clone())
                    .collect();
                if let Some(x) = Piece::from_faces(k, Some(BoltPiece::of(part)), &f) {
                    p.pieces.push(x);
                }
            }
        }
    }
    (p, bolt_of)
}

/// [`clashes`] of the parts built.
fn clashes_of(
    p: &Parts,
    bolt_of: &[(usize, bool)],
    arrivals: &[Arrival],
    plates: &[PlateDetail],
    splices: &[SpliceDetail],
    tol: f64,
) -> Vec<Clash> {
    let arrival = |m: &str| arrivals.iter().find(|a| a.member == m);
    let in_line = |a: &Arrival, b: &Arrival| {
        let c = dot(a.direction, b.direction);
        c < -(20f64.to_radians().cos())
    };
    // Member `o` is `x` or runs on from it in line through the joint (a
    // leg in two members: what bolts onto one lies on both).
    let same_run = |x: &str, o: &str| {
        x == o
            || arrival(x)
                .zip(arrival(o))
                .is_some_and(|(a, b)| in_line(a, b))
    };
    // Whether parts `x` and `y` bolt together (so are not tested), and
    // whether only the shank of a bolt among them is excused.
    let together = |x: usize, y: usize| -> Together {
        use ClashPart::*;
        let (px, py) = (&p.parts[x], &p.parts[y]);
        match (px, py) {
            (Member(a), Member(b)) => {
                let (Some(ea), Some(eb)) = (arrival(a), arrival(b)) else {
                    return Together::No;
                };
                let onto = |e: &Arrival, o: &str| {
                    e.onto.value.as_deref().is_some_and(|x| same_run(x, o))
                        || e.other_onto.as_deref() == Some(o)
                        || e.continuous.as_deref() == Some(o)
                };
                if onto(ea, b) || onto(eb, a) {
                    Together::Yes
                } else {
                    Together::No
                }
            }
            (Member(m), Plate(id)) | (Plate(id), Member(m)) => {
                let Some(e) = arrival(m) else {
                    return Together::No;
                };
                let pl = plates.iter().find(|q| &q.id == id);
                let on = e.onto.value.as_deref() == Some(id.as_str())
                    || e.other_onto.as_deref() == Some(id.as_str())
                    || pl.is_some_and(|q| {
                        q.members.contains(m) && !(q.between && q.leg.as_deref() == Some(m))
                    });
                // The leg it lies on, and a leg running on through the
                // joint in line with it — (L15) not a built-up member it
                // lies between the angles of: tested against them (it must
                // lie in the gap, touching them only).
                let leg = pl
                    .filter(|q| !q.between)
                    .and_then(|q| q.leg.as_deref())
                    .is_some_and(|l| l == m || arrival(l).is_some_and(|el| in_line(el, e)));
                if on || leg {
                    Together::Yes
                } else {
                    Together::No
                }
            }
            (Member(m), Cover(s, _)) | (Cover(s, _), Member(m)) => {
                let Some(e) = arrival(m) else {
                    return Together::No;
                };
                let sp = splices.iter().find(|q| &q.id == s);
                // (O44 J7: an end bolted through its outer cover lies on it.)
                if e.onto.value.as_deref() == Some(s.as_str())
                    || sp.is_some_and(|q| q.members.contains(m) || q.bolted.contains(m))
                    || e.through.as_ref().is_some_and(|(id, _)| id == s)
                {
                    Together::Yes
                } else {
                    Together::No
                }
            }
            (Plate(a), Plate(b)) => {
                let welded = plates
                    .iter()
                    .find(|q| &q.id == a)
                    .is_some_and(|q| q.assembly.contains(b) || q.welded_to.contains(b));
                if welded { Together::Yes } else { Together::No }
            }
            (Cover(a, _), Cover(b, _)) if a == b => Together::Yes,
            (Bolt { member, .. }, other) | (other, Bolt { member, .. }) => {
                let Some(e) = arrival(member) else {
                    return Together::No;
                };
                let me = if matches!(px, Bolt { .. }) { x } else { y };
                let other_leg = bolt_of.iter().any(|(k, o)| *k == me && *o);
                // The part this bolt's leg bolts onto.
                let onto = if other_leg {
                    e.other_onto.as_deref().or(e.onto.value.as_deref())
                } else {
                    e.onto.value.as_deref()
                };
                // Onto a built-up member: through it (a bolt through the
                // angles it is made of, with their fillers), not only the
                // angle it is seated on.
                let built_up = |m: &str| {
                    arrival(m)
                        .and_then(|a| a.profile.as_ref())
                        .is_some_and(|p| !p.parts.is_empty())
                };
                match other {
                    Member(m) if m == member => Together::Yes,
                    Member(m) if onto.is_some_and(|o| same_run(o, m)) && built_up(m) => {
                        Together::Yes
                    }
                    Member(m) if onto.is_some_and(|o| same_run(o, m)) => Together::ShankOnly,
                    // The member bolting onto this bolt's member: the
                    // bolts may be the same.
                    Member(m)
                        if arrival(m).is_some_and(|x| {
                            x.onto.value.as_deref().is_some_and(|o| same_run(o, member))
                        }) =>
                    {
                        Together::ShankOnly
                    }
                    // (L10, L9's question 6) Its own plate, its leg lying
                    // within the flatness allowed of it ([`crate::FLAT_OFF`]:
                    // drawn and checked as flat on it, the bolts pulling it
                    // flat): its head and nut lie on the plate's faces —
                    // they touch, as on a flange.
                    Plate(m)
                        if Some(m.as_str()) == onto
                            && plates
                                .iter()
                                .find(|q| &q.id == m)
                                .and_then(|q| q.through)
                                .is_some_and(|n| {
                                    let axis = if other_leg { e.other_axis } else { e.bolt_axis };
                                    dot(axis, n).abs() >= crate::FLAT_OFF.to_radians().cos() - 1e-9
                                }) =>
                    {
                        Together::Yes
                    }
                    Plate(m) if Some(m.as_str()) == onto => Together::ShankOnly,
                    // A bolt of the leg a plate lies on (a splice's, at a
                    // leg running on through the joint): through the plate
                    // too (a common bolt).
                    Plate(m)
                        if plates
                            .iter()
                            .find(|q| &q.id == m)
                            .and_then(|q| q.leg.as_deref())
                            .is_some_and(|l| same_run(l, member)) =>
                    {
                        Together::Yes
                    }
                    // A bolt of a member end bolted onto this one's member
                    // (or the other way): they may be the same bolts.
                    Bolt { member: m2, .. }
                        if m2 != member
                            && (onto.is_some_and(|o| same_run(o, m2))
                                || arrival(m2).is_some_and(|x| {
                                    x.onto.value.as_deref().is_some_and(|o| same_run(o, member))
                                })) =>
                    {
                        Together::Yes
                    }
                    Cover(s, _) if Some(s.as_str()) == onto => Together::ShankOnly,
                    // Through the outer cover (O44 J7): its shank through it.
                    Cover(s, _)
                        if !other_leg && e.through.as_ref().is_some_and(|(id, _)| id == s) =>
                    {
                        Together::ShankOnly
                    }
                    _ => Together::No,
                }
            }
            _ => Together::No,
        }
    };
    // Each pair of parts: the deepest pair of their pieces.
    let mut found: BTreeMap<(usize, usize), Deepest> = BTreeMap::new();
    let mut verdict: BTreeMap<(usize, usize), Together> = BTreeMap::new();
    for i in 0..p.pieces.len() {
        for j in i + 1..p.pieces.len() {
            let (a, b) = (&p.pieces[i], &p.pieces[j]);
            if a.part == b.part {
                continue;
            }
            // Boxes apart: nothing to test.
            if (0..3).any(|k| a.hi[k].min(b.hi[k]) - a.lo[k].max(b.lo[k]) <= tol) {
                continue;
            }
            let key = (a.part.min(b.part), a.part.max(b.part));
            let t = *verdict.entry(key).or_insert_with(|| together(key.0, key.1));
            match t {
                Together::Yes => continue,
                Together::ShankOnly
                    if a.bolt == Some(BoltPiece::Shank) || b.bolt == Some(BoltPiece::Shank) =>
                {
                    continue;
                }
                _ => {}
            }
            let d = depth(a, b, tol);
            if d <= tol {
                continue;
            }
            let mid: [f64; 3] =
                std::array::from_fn(|k| (a.hi[k].min(b.hi[k]) + a.lo[k].max(b.lo[k])) / 2.0);
            let (pa, pb) = if a.part == key.0 {
                (a.bolt, b.bolt)
            } else {
                (b.bolt, a.bolt)
            };
            let e = found.entry(key).or_insert((d, mid, pa, pb));
            if d > e.0 {
                *e = (d, mid, pa, pb);
            }
        }
    }
    found
        .into_iter()
        .map(|((x, y), (depth, at, pa, pb))| {
            let name = |k: usize, piece: Option<BoltPiece>| match (&p.parts[k], piece) {
                (ClashPart::Bolt { member, index, .. }, Some(pc)) => ClashPart::Bolt {
                    member: member.clone(),
                    index: *index,
                    piece: pc,
                },
                (other, _) => other.clone(),
            };
            Clash {
                a: name(x, pa),
                b: name(y, pb),
                depth,
                at,
            }
        })
        .collect()
}

/// The deepest pair of pieces of two parts: depth, where, the bolt pieces.
type Deepest = (f64, [f64; 3], Option<BoltPiece>, Option<BoltPiece>);

/// Whether two parts bolt together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Together {
    /// No: tested.
    No,
    /// Yes: not tested.
    Yes,
    /// A bolt through the other part: its shank not tested, its head and
    /// nut tested.
    ShankOnly,
}

fn newell(poly: &[[f64; 3]]) -> Option<[f64; 3]> {
    let mut n = [0.0; 3];
    for (k, a) in poly.iter().enumerate() {
        let b = poly[(k + 1) % poly.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let l = norm(n);
    (l >= 1e-15).then(|| scale(n, 1.0 / l))
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
