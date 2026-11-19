// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Orient angles from the tower's shape** (2026-11-18, owner: the legs'
//! turn of an imported tower is an import artefact — the export carries no
//! member orientation — "and yeah only turn where it helps"): the rolls of
//! a tower without orientation data (an import, a tower drawn by hand)
//! worked out from where its members are.
//!
//! - **Legs and chords** — the members of class `leg` and the angles that
//!   run on in line (within [`IN_LINE_DEG`]) through a joint: each turned
//!   so that its heel points out of the tower and its two flanges lie in
//!   the two faces of the tower meeting at it. The faces are those of its
//!   neighbouring legs' lines (a leg's from the other legs, a chord's from
//!   the other chords): the tower's legs and chords beside it
//!   (within [`PARALLEL_DEG`] of it, their lines joined to its line by a
//!   member — a face's bracing, a plan member; each by its point nearest
//!   the member's middle) seen along it make a polygon; the member is a corner
//!   of its outline (else it is left: inside the tower, on a face, or with
//!   fewer than two neighbours), the two corners next to it are its
//!   neighbours. An
//!   angle's legs are at 90°, the faces seldom exactly: the angle is
//!   turned so that the middle of its legs is the middle of the faces
//!   (each flange off its face by half the difference, said). A built-up
//!   leg of two angles in a star or four in a cross is turned the same way
//!   (its legs along the faces); two angles back to back are left.
//! - **Plan and hip members** — horizontal angles (within
//!   [`HORIZONTAL_OFF`]) lying in no steep face of the tower: one leg
//!   flat (horizontal) and the other pointing down (assumed: no water
//!   lying in it — a choice of this command, not a standard's), the flat
//!   leg towards the tower's middle.
//! - **Bracing** is left: the layout lays it flat on what it bolts onto
//!   ([`crate::lay_out`], turned only where that helps).
//!
//! A roll given by hand (a `roll`, `reference` or `orientation` in the
//! file not set by this command or by the layout) is kept unless replaced.
//! What it sets is marked automatic in the joint layout's record
//! (`oriented`: the member, its roll before, `chord` or `plan`); the
//! layout keeps a leg's or chord's roll set here (the tower's shape) and
//! may turn a plan member's. The same file gives the same result: a run
//! again changes nothing.

use std::collections::{BTreeMap, BTreeSet};

use ohl_sections::{SectionLibrary, Shape};
use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

use crate::joints::HORIZONTAL_OFF;
use crate::layout_run::LayoutRecord;

/// Two members running on in line through a joint: within this angle of
/// each other, degrees (a taper change of a tower's leg is a few degrees;
/// an assumption of this command, not a standard's).
pub const IN_LINE_DEG: f64 = 10.0;

/// The legs and chords taken into a member's section of the tower: within
/// this angle of it, degrees (a tapering body's opposite legs, a
/// crossarm's converging chords).
pub const PARALLEL_DEG: f64 = 70.0;

/// The faces at a leg or chord meeting at an angle within these, degrees
/// (seen along it): a corner its angle is turned to (each flange then at
/// most 22.5° off its face); else it is left (an assumption of this
/// command, not a standard's).
pub const CORNER_DEG: [f64; 2] = [45.0, 135.0];

/// A member lies in a face of the tower when its ends lie within this of
/// the face's plane (and half a centimetre per metre of its length), m (an
/// assumption of this command, as for the plan members).
const IN_FACE: f64 = 0.02;

/// A corner of the tower's section lying within this distance of the line
/// through its neighbours is on a face, not a corner, m.
const ON_FACE: f64 = 0.005;

/// What a member was turned as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientKind {
    /// A leg or chord: heel outward, its flanges in the two faces.
    Chord,
    /// A plan or hip member: one leg flat, the other down.
    Plan,
    /// A member of a steep face (bracing, a redundant): one leg in the
    /// face, the other out of the tower.
    Face,
}

impl OrientKind {
    /// As the record writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chord => "chord",
            Self::Plan => "plan",
            Self::Face => "face",
        }
    }

    /// From the record.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "chord" => Some(Self::Chord),
            "plan" => Some(Self::Plan),
            "face" => Some(Self::Face),
            _ => None,
        }
    }
}

/// One member turned.
#[derive(Debug, Clone, PartialEq)]
pub struct OrientedMember {
    /// The member.
    pub member: String,
    /// As what.
    pub kind: OrientKind,
    /// Its roll before, as written (empty: none).
    pub before: String,
    /// Its roll now, degrees (as written: to a hundredth).
    pub roll: f64,
    /// The turn, degrees (−180 to 180).
    pub turn: f64,
    /// A leg or chord: each flange's angle off its face, degrees (half the
    /// faces' difference from 90°).
    pub off: f64,
    /// Its neighbours (a leg or chord: the members whose lines make its
    /// two faces).
    pub neighbours: Vec<String>,
}

/// What to orient.
#[derive(Debug, Clone, Default)]
pub struct OrientAsk {
    /// The members (none: every member).
    pub members: Option<BTreeSet<String>>,
    /// Rolls given by hand replaced too.
    pub replace_hand: bool,
}

/// A run of Orient angles on a structure file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrientRun {
    /// The file after (the same text when nothing changed).
    pub text: String,
    /// The members turned (their roll changed), in the file's order.
    pub turned: Vec<OrientedMember>,
    /// The members already as this command turns them (unchanged).
    pub same: Vec<OrientedMember>,
    /// The members whose roll given by hand was kept, and as what they
    /// would be turned.
    pub kept: Vec<OrientedMember>,
    /// Legs (class `leg`) not turned, with why (an angle running on that
    /// is no corner of the tower's section is bracing: not listed).
    pub left: Vec<(String, String)>,
    /// Plan and face members whose roll the joint layout chose for the
    /// whole member (J3: a roll laying its ends flat, the layout's record
    /// `rolled`), kept: the layout and this command agree (a run of either
    /// after the other changes nothing); as what this command would turn
    /// them.
    pub by_layout: Vec<OrientedMember>,
    /// How long it took, s.
    pub seconds: f64,
}

/// A member as this command sees it.
struct Line {
    id: String,
    a: [f64; 3],
    b: [f64; 3],
    x: [f64; 3],
    /// Its local y and z with no roll (its reference, or the default).
    base: [[f64; 3]; 2],
    /// A single angle, or angles in a star or a cross; false: two angles
    /// back to back (a chord left).
    turnable: bool,
    /// An angle of some kind.
    angle: bool,
    class_leg: bool,
    /// A single angle.
    single: bool,
    /// An unequal angle's leg bolted lies along y (its design data);
    /// none: equal legs, or not said.
    bolted_y: Option<bool>,
}

/// A face of the tower: through a leg or chord and a neighbour of it (its
/// lines' members), its normal out of the tower.
struct FaceOf {
    normal: [f64; 3],
    at: [f64; 3],
    lines: [usize; 2],
}

impl FaceOf {
    /// How far member `l`'s ends lie off its plane, m.
    fn off(&self, l: &Line) -> f64 {
        dot(sub(l.a, self.at), self.normal)
            .abs()
            .max(dot(sub(l.b, self.at), self.normal).abs())
    }

    /// Member `k` lies in it: both its ends within [`IN_FACE`] (and half a
    /// centimetre per metre of its length) of its plane, and its middle
    /// between its two lines (beside the panel of the first).
    fn holds(&self, lines: &[Line], k: usize) -> bool {
        if self.lines.contains(&k) {
            return false;
        }
        let l = &lines[k];
        if self.off(l) > IN_FACE + 0.005 * norm(sub(l.b, l.a)) {
            return false;
        }
        let m = scale(add(l.a, l.b), 0.5);
        let near = |q: &Line, limit: bool| -> Option<[f64; 3]> {
            let d = sub(q.b, q.a);
            let s = dot(sub(m, q.a), d) / dot(d, d).max(1e-18);
            (!limit || (-0.1..=1.1).contains(&s)).then(|| add(q.a, scale(d, s)))
        };
        let (Some(pa), Some(pb)) = (
            near(&lines[self.lines[0]], true),
            near(&lines[self.lines[1]], false),
        ) else {
            return false;
        };
        let w = sub(pb, pa);
        let width = norm(w);
        if width < 1e-6 {
            return false;
        }
        let across = dot(sub(m, pa), scale(w, 1.0 / width));
        across > ON_FACE && across < width - ON_FACE
    }
}

/// The tower's legs and chords and its plan members turned as they stand
/// (as the module docs say) in `text`, as `ask` says.
pub fn orient_angles(
    text: &str,
    library: &SectionLibrary,
    ask: &OrientAsk,
) -> Result<OrientRun, String> {
    let start = std::time::Instant::now();
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let model = ohl_structures_fe::model::Model::from_structure_type_with(text, library)
        .map_err(|e| format!("the model is not built ({e}): nothing to orient"))?;
    let record = LayoutRecord::of(&t);
    let mut lines: Vec<Line> = Vec::new();
    for m in &t.members {
        let Some(fm) = model.members.iter().find(|x| x.id == m.id) else {
            continue;
        };
        let (a, b) = (model.nodes[fm.i].point, model.nodes[fm.j].point);
        let d = sub(b, a);
        let l = norm(d);
        if l <= 1e-9 {
            continue;
        }
        // Its axes with no roll (its reference, or the default).
        let [x, y, z] = crate::member_axes(a, b, fm.reference, 0.0);
        let shape = m
            .section
            .as_deref()
            .and_then(|s| library.section(s))
            .map(|s| &s.shape);
        // An unequal angle's leg bolted (the member's design data): along
        // y (the leg b) or z; none: equal legs, or not said.
        let bolted_y = match shape {
            Some(Shape::UnequalAngle { b, h, .. }) if (b - h).abs() > 1e-9 => m
                .design
                .as_ref()
                .and_then(|d| d.get("connected_leg"))
                .and_then(toml::Value::as_str)
                .and_then(|c| match c {
                    "long" => Some(b >= h),
                    "short" => Some(b < h),
                    _ => None,
                }),
            _ => None,
        };
        let (angle, turnable) = match shape {
            Some(Shape::EqualAngle { .. } | Shape::UnequalAngle { .. }) => (true, true),
            Some(Shape::StarAngle { .. } | Shape::CrossAngle { .. }) => (true, true),
            Some(Shape::DoubleAngle { .. }) => (true, false),
            _ => (false, false),
        };
        let class_leg = m
            .design
            .as_ref()
            .and_then(|d| d.get("class"))
            .and_then(toml::Value::as_str)
            == Some("leg");
        lines.push(Line {
            id: m.id.clone(),
            a,
            b,
            x,
            base: [y, z],
            turnable,
            angle,
            class_leg,
            single: matches!(
                shape,
                Some(Shape::EqualAngle { .. } | Shape::UnequalAngle { .. })
            ),
            bolted_y,
        });
    }
    // Running on in line through a joint: at each joint, the pairs of
    // angles nearest in line first.
    let mut running: BTreeSet<usize> = BTreeSet::new();
    // Each member's line (the members running on in line, joined).
    let mut line_of: Vec<usize> = (0..lines.len()).collect();
    let mut at_joint: BTreeMap<String, Vec<(usize, [f64; 3])>> = BTreeMap::new();
    for (k, l) in lines.iter().enumerate() {
        if !l.angle {
            continue;
        }
        let m = t.members.iter().find(|m| m.id == l.id).expect("listed");
        at_joint
            .entry(m.nodes[0].clone())
            .or_default()
            .push((k, l.x));
        at_joint
            .entry(m.nodes[1].clone())
            .or_default()
            .push((k, scale(l.x, -1.0)));
    }
    for ends in at_joint.values() {
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        for i in 0..ends.len() {
            for j in i + 1..ends.len() {
                let dev = 180.0 - angle_deg(ends[i].1, ends[j].1);
                if dev <= IN_LINE_DEG {
                    pairs.push((dev, ends[i].0, ends[j].0));
                }
            }
        }
        pairs.sort_by(|p, q| p.0.total_cmp(&q.0).then((p.1, p.2).cmp(&(q.1, q.2))));
        let mut used: BTreeSet<usize> = BTreeSet::new();
        for (_, i, j) in pairs {
            if used.contains(&i) || used.contains(&j) {
                continue;
            }
            used.insert(i);
            used.insert(j);
            running.insert(i);
            running.insert(j);
            join(&mut line_of, i, j);
        }
    }
    let chords: Vec<usize> = (0..lines.len())
        .filter(|&k| lines[k].angle && (lines[k].class_leg || running.contains(&k)))
        .collect();
    // The lines of legs and chords next to each other: joined by a member
    // (a face's bracing, a plan member) from a joint of one to a joint of
    // the other, lying in one plane with both (within [`HORIZONTAL_OFF`]).
    // Only those make a member's faces: a portal's other column, a far
    // crossarm's chords, an arm's chord joined to a leg by the arm's
    // bracing are not its neighbours however parallel.
    let mut lines_at: BTreeMap<&str, Vec<(usize, usize)>> = BTreeMap::new();
    for &k in &chords {
        let m = t
            .members
            .iter()
            .find(|m| m.id == lines[k].id)
            .expect("listed");
        let r = root(&mut line_of, k);
        for n in &m.nodes {
            lines_at.entry(n.as_str()).or_default().push((r, k));
        }
    }
    let position: BTreeMap<&str, [f64; 3]> = t
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.point.map(|v| v.si())))
        .collect();
    let flat = HORIZONTAL_OFF.to_radians().sin();
    let mut next_to: BTreeSet<(usize, usize)> = BTreeSet::new();
    for m in &t.members {
        let (Some(a), Some(b)) = (
            lines_at.get(m.nodes[0].as_str()),
            lines_at.get(m.nodes[1].as_str()),
        ) else {
            continue;
        };
        let (Some(pa), Some(pb)) = (
            position.get(m.nodes[0].as_str()),
            position.get(m.nodes[1].as_str()),
        ) else {
            continue;
        };
        let d = sub(*pb, *pa);
        if norm(d) <= 1e-9 {
            continue;
        }
        let d = unit(d);
        for &(x, kx) in a {
            for &(y, ky) in b {
                if x == y {
                    continue;
                }
                let n = cross(lines[kx].x, d);
                if norm(n) < flat || dot(unit(n), lines[ky].x).abs() > flat {
                    continue;
                }
                next_to.insert((x, y));
                next_to.insert((y, x));
            }
        }
    }
    let asked = |id: &str| ask.members.as_ref().is_none_or(|s| s.contains(id));
    // The tower's middle (plan), for faces whose inside is not known.
    let centre = {
        let n = model.nodes.len().max(1) as f64;
        let s = model
            .nodes
            .iter()
            .fold([0.0; 3], |acc, x| add(acc, x.point));
        [s[0] / n, s[1] / n]
    };
    // Each leg's or chord's target axes, and the tower's faces.
    let mut target: BTreeMap<usize, (f64, f64, Vec<String>)> = BTreeMap::new();
    let mut faces: Vec<FaceOf> = Vec::new();
    let mut left: Vec<(String, String)> = Vec::new();
    // The legs first (their faces from the other legs), then the angles
    // running on in line that lie in no face of the legs (a face's
    // diagonal running on through its crossing is bracing of that face,
    // not a chord: tuning on real towers, 2026-11-18), their faces from
    // each other.
    let legs: Vec<usize> = chords
        .iter()
        .copied()
        .filter(|&k| lines[k].class_leg)
        .collect();
    let corner = |pool: &[usize],
                  line_of: &mut Vec<usize>,
                  target: &mut BTreeMap<usize, (f64, f64, Vec<String>)>,
                  faces: &mut Vec<FaceOf>,
                  left: &mut Vec<(String, String)>| {
        for &k in pool {
            let l = &lines[k];
            let p = scale(add(l.a, l.b), 0.5);
            // The section normal to it through its middle.
            let e1 = l.base[0];
            let e2 = l.base[1];
            let mut pts: Vec<([f64; 2], usize)> = vec![([0.0, 0.0], k)];
            let own = root(line_of, k);
            for &n in pool {
                if n == k {
                    continue;
                }
                let other = root(line_of, n);
                if !next_to.contains(&(own, other)) {
                    continue;
                }
                let o = &lines[n];
                if angle_deg(l.x, o.x).min(180.0 - angle_deg(l.x, o.x)) > PARALLEL_DEG {
                    continue;
                }
                // Its point nearest the member's middle (beside it, not far
                // along: a leg of the same panel, or of the next past a taper
                // change). Any point of a neighbouring leg's line lies in their
                // common face: seen along the member, it gives the face.
                let d = sub(o.b, o.a);
                let s = dot(sub(p, o.a), d) / dot(d, d).max(1e-18);
                if !(-0.5..=1.5).contains(&s) {
                    continue;
                }
                let q = sub(add(o.a, scale(d, s.clamp(0.0, 1.0))), p);
                let q2 = [dot(q, e1), dot(q, e2)];
                if pts
                    .iter()
                    .any(|(x, _)| (x[0] - q2[0]).hypot(x[1] - q2[1]) < ON_FACE)
                {
                    continue;
                }
                pts.push((q2, n));
            }
            let hull = hull(&pts.iter().map(|x| x.0).collect::<Vec<_>>());
            let to3 = |q: [f64; 2]| add(scale(e1, q[0]), scale(e2, q[1]));
            // A face through the member and neighbour `n` (at `q` seen
            // along it), its normal out of the section (`inside`: a point
            // inside it, seen along the member; none: the tower's middle).
            let face = |q: [f64; 2], n: usize, inside: Option<[f64; 2]>| -> FaceOf {
                let mut nv = unit(cross(l.x, unit(to3(q))));
                let towards = match inside {
                    Some(c) => to3(c),
                    None => [centre[0] - p[0], centre[1] - p[1], 0.0],
                };
                if dot(nv, towards) > 0.0 {
                    nv = scale(nv, -1.0);
                }
                FaceOf {
                    normal: nv,
                    at: p,
                    lines: [k, n],
                }
            };
            let Some(at) = hull.iter().position(|&i| i == 0) else {
                if asked(&l.id) && l.class_leg {
                    left.push((
                        l.id.clone(),
                        if hull.len() < 3 {
                            format!(
                                "fewer than two neighbouring legs or chords across it ({} in \
                                 its section): its faces are not known",
                                pts.len() - 1
                            )
                        } else {
                            "not at a corner of the tower's section (inside it, or on a face)"
                                .into()
                        },
                    ));
                }
                continue;
            };
            if hull.len() < 3 {
                // One line of legs or chords (a frame of two legs): its plane
                // is a face all the same (for the plan members).
                if let Some((q, n)) = pts
                    .iter()
                    .skip(1)
                    .max_by(|a, b| a.0[0].hypot(a.0[1]).total_cmp(&b.0[0].hypot(b.0[1])))
                {
                    faces.push(face(*q, *n, None));
                }
                if asked(&l.id) && l.class_leg {
                    left.push((
                        l.id.clone(),
                        format!(
                            "fewer than two neighbouring legs or chords across it ({} in its \
                             section): its faces are not known",
                            pts.len() - 1
                        ),
                    ));
                }
                continue;
            }
            let prev = hull[(at + hull.len() - 1) % hull.len()];
            let next = hull[(at + 1) % hull.len()];
            let f1 = unit(to3(pts[prev].0));
            let f2 = unit(to3(pts[next].0));
            let theta = angle_deg(f1, f2);
            // The section's middle (its outline's corners), seen along it.
            let mid = {
                let n = hull.len() as f64;
                let s = hull
                    .iter()
                    .fold([0.0; 2], |a, &i| [a[0] + pts[i].0[0], a[1] + pts[i].0[1]]);
                [s[0] / n, s[1] / n]
            };
            for i in [prev, next] {
                faces.push(face(pts[i].0, pts[i].1, Some(mid)));
            }
            if !l.turnable {
                if asked(&l.id) && l.class_leg {
                    left.push((
                        l.id.clone(),
                        "two angles back to back: no corner to turn to the faces".into(),
                    ));
                }
                continue;
            }
            // Faces meeting far from square (nearly one plane, or a sharp
            // edge): no corner an angle's legs can follow.
            if !(CORNER_DEG[0]..=CORNER_DEG[1]).contains(&theta) {
                if asked(&l.id) && l.class_leg {
                    left.push((
                        l.id.clone(),
                        format!(
                            "its two faces meet at {theta:.0}°, too far from square for an \
                             angle's legs (a corner between {:.0}° and {:.0}°)",
                            CORNER_DEG[0], CORNER_DEG[1]
                        ),
                    ));
                }
                continue;
            }
            let bis = unit(add(f1, f2));
            let c = cross(l.x, bis);
            let y = scale(sub(bis, c), std::f64::consts::FRAC_1_SQRT_2);
            let roll = dot(y, l.base[1]).atan2(dot(y, l.base[0]));
            target.insert(
                k,
                (
                    roll,
                    (theta - 90.0).abs() / 2.0,
                    vec![lines[pts[prev].1].id.clone(), lines[pts[next].1].id.clone()],
                ),
            );
        }
    };
    corner(&legs, &mut line_of, &mut target, &mut faces, &mut left);
    let leg_faces = faces.len();
    let others: Vec<usize> = chords
        .iter()
        .copied()
        .filter(|&k| !lines[k].class_leg)
        .filter(|&k| !faces[..leg_faces].iter().any(|f| f.holds(&lines, k)))
        .collect();
    corner(&others, &mut line_of, &mut target, &mut faces, &mut left);
    let steep: Vec<&FaceOf> = faces
        .iter()
        .filter(|f| f.normal[2].abs() < std::f64::consts::FRAC_1_SQRT_2)
        .collect();
    // Plan and hip members: horizontal angles in no steep face.
    let mut plan: BTreeMap<usize, f64> = BTreeMap::new();
    // Face members: angles lying in a steep face between its legs or
    // chords, one leg in the face.
    let mut in_face: BTreeMap<usize, f64> = BTreeMap::new();
    for (k, l) in lines.iter().enumerate() {
        if target.contains_key(&k) || l.class_leg || !l.angle || !l.turnable {
            continue;
        }
        if l.x[2].abs() <= HORIZONTAL_OFF.to_radians().sin() {
            let len = norm(sub(l.b, l.a));
            let tol = IN_FACE + 0.005 * len;
            let on_steep = steep.iter().any(|f| {
                dot(sub(l.a, f.at), f.normal).abs() <= tol
                    && dot(sub(l.b, f.at), f.normal).abs() <= tol
            });
            if !on_steep {
                // Down, and the flat leg across it towards the middle.
                let down = [0.0, 0.0, -1.0];
                let mut flat = unit(cross(l.x, down));
                let mid = scale(add(l.a, l.b), 0.5);
                let inward = [centre[0] - mid[0], centre[1] - mid[1], 0.0];
                if dot(flat, inward) < 0.0 {
                    flat = scale(flat, -1.0);
                }
                // Right-handed (z = x × y): the flat leg along y when x × it
                // points down, else the leg down along y.
                let y = if dot(cross(l.x, flat), down) > 0.0 {
                    flat
                } else {
                    down
                };
                plan.insert(k, dot(y, l.base[1]).atan2(dot(y, l.base[0])));
                continue;
            }
        }
        if !l.single {
            continue;
        }
        // The steep face it lies in (the nearest of those holding it).
        let Some(f) = steep
            .iter()
            .filter(|f| f.holds(&lines, k))
            .min_by(|a, b| a.off(&lines[k]).total_cmp(&b.off(&lines[k])))
        else {
            continue;
        };
        // Its leg bolted in the face, the other out of the tower (on the
        // outer face of the legs' flanges), the leg in the face from its
        // heel downwards where it can be (no water lying in it: an
        // assumption of this command, as for the plan members).
        let out = f.normal;
        let mut d = unit(cross(out, l.x));
        let mid = scale(add(l.a, l.b), 0.5);
        let inward = [centre[0] - mid[0], centre[1] - mid[1], 0.0];
        if d[2] > 1e-9 || (d[2].abs() <= 1e-9 && dot(d, inward) < 0.0) {
            d = scale(d, -1.0);
        }
        // Right-handed: the leg along y in the face when x × d is out,
        // else the leg along z in it (y out). An unequal angle's leg
        // bolted decides which; the leg in the face then upwards if need be.
        let y_in_face = match l.bolted_y {
            Some(by) => {
                if by != (dot(cross(l.x, d), out) > 0.0) {
                    d = scale(d, -1.0);
                }
                by
            }
            None => dot(cross(l.x, d), out) > 0.0,
        };
        let y = if y_in_face { d } else { out };
        in_face.insert(k, dot(y, l.base[1]).atan2(dot(y, l.base[0])));
    }
    // Written: hand values kept unless replaced.
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let mut rec = record.clone();
    let mut run = OrientRun::default();
    let mut all: Vec<(usize, OrientKind, f64, f64, Vec<String>)> = target
        .iter()
        .map(|(k, (r, off, n))| (*k, OrientKind::Chord, *r, *off, n.clone()))
        .chain(
            plan.iter()
                .map(|(k, r)| (*k, OrientKind::Plan, *r, 0.0, Vec::new())),
        )
        .chain(
            in_face
                .iter()
                .map(|(k, r)| (*k, OrientKind::Face, *r, 0.0, Vec::new())),
        )
        .collect();
    all.sort_by_key(|x| x.0);
    for (k, kind, roll, off, neighbours) in all {
        let l = &lines[k];
        if !asked(&l.id) {
            continue;
        }
        let m = t.members.iter().find(|m| m.id == l.id).expect("listed");
        let before = roll_text(m.roll.as_ref());
        let now = m.roll.as_ref().map_or(0.0, roll_deg);
        let deg = round_deg(roll.to_degrees());
        let auto = rec.oriented.contains_key(&l.id) || rec.rolled.contains_key(&l.id);
        let by_hand =
            !auto && (m.roll.is_some() || m.reference.is_some() || m.orientation.is_some());
        let o = OrientedMember {
            member: l.id.clone(),
            kind,
            before: before.clone(),
            roll: deg,
            turn: round_deg(deg - now),
            off,
            neighbours,
        };
        if by_hand && !ask.replace_hand {
            if o.turn.abs() >= 0.005 {
                run.kept.push(o);
            } else {
                run.same.push(o);
            }
            continue;
        }
        // A plan or face member the joint layout turned so that its ends
        // lie flat (one roll for the whole member, J3): its roll kept — the
        // layout's choice starts from this command's and departs from it
        // only where the ends need it (a leg's or chord's roll the layout
        // never turns).
        if kind != OrientKind::Chord && rec.rolled.contains_key(&l.id) {
            if o.turn.abs() >= 0.005 {
                run.by_layout.push(o);
            } else {
                run.same.push(o);
            }
            continue;
        }
        // The roll before this command (or the layout) first turned it.
        let first = rec
            .oriented
            .get(&l.id)
            .map(|x| x.0.clone())
            .or_else(|| rec.rolled.get(&l.id).cloned())
            .unwrap_or_else(|| before.clone());
        rec.rolled.remove(&l.id);
        if o.turn.abs() < 0.005 {
            rec.oriented.insert(l.id.clone(), (first, kind));
            run.same.push(o);
            continue;
        }
        ohl_structure_edit::edits::set_member_roll(&mut doc, &l.id, deg.to_radians());
        rec.oriented.insert(l.id.clone(), (first, kind));
        run.turned.push(o);
    }
    if rec != record {
        rec.write(&mut doc);
    }
    run.text = if rec == record && run.turned.is_empty() {
        text.to_owned()
    } else {
        doc.to_string()
    };
    run.left = left;
    run.seconds = start.elapsed().as_secs_f64();
    Ok(run)
}

/// The line member `k` is on (union-find with path halving).
fn root(line_of: &mut [usize], mut k: usize) -> usize {
    while line_of[k] != k {
        line_of[k] = line_of[line_of[k]];
        k = line_of[k];
    }
    k
}

/// Members `a` and `b` on one line.
fn join(line_of: &mut [usize], a: usize, b: usize) {
    let (x, y) = (root(line_of, a), root(line_of, b));
    if x != y {
        line_of[x.max(y)] = x.min(y);
    }
}

/// A roll as written (empty: none).
fn roll_text(v: Option<&toml::Value>) -> String {
    match v {
        None => String::new(),
        Some(toml::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// A roll value in degrees.
fn roll_deg(v: &toml::Value) -> f64 {
    v.clone()
        .try_into::<ohl_core::units::Angle>()
        .map_or(0.0, |a| a.radians().to_degrees())
}

/// Degrees in (−180, 180], to a hundredth (as the files write a roll).
fn round_deg(d: f64) -> f64 {
    let mut d = d % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d <= -180.0 {
        d += 360.0;
    }
    let r = (d * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

/// The convex outline of `pts` (2D), counter-clockwise, as indices; points
/// within [`ON_FACE`] of an edge left out.
fn hull(pts: &[[f64; 2]]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    idx.sort_by(|&a, &b| {
        pts[a][0]
            .total_cmp(&pts[b][0])
            .then(pts[a][1].total_cmp(&pts[b][1]))
    });
    if idx.len() < 3 {
        return idx;
    }
    // Left of o→a by more than ON_FACE (distance of b from the line).
    let left = |o: usize, a: usize, b: usize| {
        let (p, q, r) = (pts[o], pts[a], pts[b]);
        let cr = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
        let len = (q[0] - p[0]).hypot(q[1] - p[1]).max(1e-12);
        cr / len
    };
    let mut lower: Vec<usize> = Vec::new();
    for &i in &idx {
        while lower.len() >= 2 && left(lower[lower.len() - 2], lower[lower.len() - 1], i) <= ON_FACE
        {
            lower.pop();
        }
        lower.push(i);
    }
    let mut upper: Vec<usize> = Vec::new();
    for &i in idx.iter().rev() {
        while upper.len() >= 2 && left(upper[upper.len() - 2], upper[upper.len() - 1], i) <= ON_FACE
        {
            upper.pop();
        }
        upper.push(i);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    (dot(a, b) / (norm(a) * norm(b)).max(1e-300))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    scale(a, 1.0 / norm(a).max(1e-300))
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

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
