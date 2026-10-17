// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What each body generator builds: pure functions from the structure
//! (as last read) and a generator's inputs to joints and members, with
//! ids that follow from the inputs (the same inputs give the same ids, so
//! a rebuild keeps what was set on them and what refers to them).
//! Nothing is written here: [`mod@crate::rebuild`] writes them as ordinary
//! `[[node]]` and `[[member]]` entries with a `made_by`.

use std::collections::BTreeSet;

use ols_structures::StructureType;

use crate::body::{ArmShape, Bracing, CopyFace, Crossarm, Generator, Kind, Panels, Peak};

/// Distance under which two positions are one, m.
pub const SAME: f64 = 1e-3;

/// A joint a generator writes.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyJoint {
    /// Id.
    pub id: String,
    /// Position, local axes, m.
    pub at: [f64; 3],
}

/// A member a generator writes.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyMember {
    /// Id.
    pub id: String,
    /// Its joints (its own, or joints of the structure).
    pub nodes: [String; 2],
    /// Its member group.
    pub group: Option<String>,
}

/// What a generator builds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Body {
    /// New joints.
    pub joints: Vec<BodyJoint>,
    /// Members (between its joints or joints already there).
    pub members: Vec<BodyMember>,
    /// Points of the structure put on a joint: label, joint.
    pub points: Vec<(String, String)>,
}

impl Body {
    /// Where joint `id` is: one of its own, else one of `t`.
    pub fn at(&self, t: &StructureType, id: &str) -> Option<[f64; 3]> {
        self.joints
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.at)
            .or_else(|| {
                t.nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.point.map(|v| v.si()))
            })
    }

    /// Its members as lines (the preview).
    pub fn segments(&self, t: &StructureType) -> Vec<[[f64; 3]; 2]> {
        self.members
            .iter()
            .filter_map(|m| Some([self.at(t, &m.nodes[0])?, self.at(t, &m.nodes[1])?]))
            .collect()
    }
}

/// Collects joints and members, without doubles.
struct Build {
    gen_id: String,
    body: Body,
    pairs: BTreeSet<(String, String)>,
}

impl Build {
    fn new(gen_id: &str) -> Self {
        Self {
            gen_id: gen_id.to_owned(),
            body: Body::default(),
            pairs: BTreeSet::new(),
        }
    }

    fn joint(&mut self, id: String, at: [f64; 3]) -> String {
        if !self.body.joints.iter().any(|j| j.id == id) {
            self.body.joints.push(BodyJoint {
                id: id.clone(),
                at: at.map(round3),
            });
        }
        id
    }

    /// A member `a`–`b` (none when it is there already).
    fn member(&mut self, a: &str, b: &str, group: Option<String>) {
        self.member_id(None, a, b, group);
    }

    fn member_id(&mut self, id: Option<String>, a: &str, b: &str, group: Option<String>) {
        let key = if a < b {
            (a.to_owned(), b.to_owned())
        } else {
            (b.to_owned(), a.to_owned())
        };
        if a == b || !self.pairs.insert(key) {
            return;
        }
        let id = id.unwrap_or_else(|| format!("{}-M{}", self.gen_id, self.body.members.len() + 1));
        self.body.members.push(BodyMember {
            id,
            nodes: [a.to_owned(), b.to_owned()],
            group,
        });
    }
}

fn round3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

fn mid(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|k| 0.5 * (a[k] + b[k]))
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

/// What generator `g` builds on `t` (or why it cannot).
pub fn generate(t: &StructureType, g: &Generator) -> Result<Body, String> {
    g.check().map_err(|e| format!("generator {}: {e}", g.id))?;
    match &g.kind {
        Kind::Panels(p) => Ok(panels(t, &g.id, p)),
        Kind::Crossarm(c) => crossarm(t, &g.id, c),
        Kind::Peak(p) => peak(t, &g.id, p),
        Kind::CopyFace(c) => copy_face(t, &g.id, c),
    }
}

/// The corners in plan, counter-clockwise from +x +y: their signs.
const CORNERS: [[f64; 2]; 4] = [[1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]];

/// The faces: their corners (1-based) left and right seen from outside,
/// counter-clockwise: x+, y+, x−, y−.
const FACES: [(usize, usize); 4] = [(4, 1), (1, 2), (2, 3), (3, 4)];

/// The levels of a panels body (feet first) and its widths at a level.
pub struct Frustum {
    /// The levels, m: the feet, then each panel's top.
    pub levels: Vec<f64>,
    feet: [f64; 2],
    waist: [f64; 2],
}

impl Frustum {
    /// Of panels `p` on `t`.
    pub fn new(t: &StructureType, p: &Panels) -> Self {
        let mut levels = vec![p.bottom.unwrap_or_else(|| t.foot_level())];
        levels.extend(p.panels.iter().map(|x| x.top));
        Self {
            levels,
            feet: p.feet,
            waist: p.waist,
        }
    }

    /// The widths across (x) and along (y) at level `z`.
    pub fn width(&self, z: f64) -> [f64; 2] {
        let (z0, zt) = (self.levels[0], *self.levels.last().expect("the feet"));
        let f = if zt > z0 { (z - z0) / (zt - z0) } else { 0.0 };
        [0, 1].map(|k| self.feet[k] + f * (self.waist[k] - self.feet[k]))
    }

    /// Corner `q` (1 to 4) at level `i`.
    pub fn corner(&self, i: usize, q: usize) -> [f64; 3] {
        let z = self.levels[i];
        let w = self.width(z);
        let s = CORNERS[q - 1];
        [s[0] * 0.5 * w[0], s[1] * 0.5 * w[1], z]
    }

    /// Whether `p` lies on face (`axis`, `side`) within [`SAME`].
    pub fn on_face(&self, p: [f64; 3], axis: usize, side: f64) -> bool {
        let (z0, zt) = (self.levels[0], *self.levels.last().expect("the feet"));
        p[2] > z0 - SAME
            && p[2] < zt + SAME
            && (side * p[axis] - 0.5 * self.width(p[2])[axis]).abs() <= SAME
    }
}

/// Legs and panels.
fn panels(t: &StructureType, id: &str, p: &Panels) -> Body {
    let fr = Frustum::new(t, p);
    let n = p.panels.len();
    let bracing = |i: usize| p.panels[i - 1].bracing;
    let needs_mid = |b: Bracing| matches!(b, Bracing::K | Bracing::Diamond);
    // Joints in the middle of the horizontal edges at level i; horizontals
    // at level i.
    let mids = |i: usize| {
        (i >= 1 && needs_mid(bracing(i))) || (i < n && bracing(i + 1) == Bracing::Diamond)
    };
    let horizontal = |i: usize| (i >= 1 && p.panels[i - 1].horizontal) || mids(i);
    let mut b = Build::new(id);
    let corner = |i: usize, q: usize| format!("{id}-L{i}-{q}");
    let edge_mid = |i: usize, f: usize| format!("{id}-L{i}-F{f}");
    let leg_mid = |p: usize, q: usize| format!("{id}-P{p}-{q}");
    // Joints: level by level, the corners, then the middles.
    for i in 0..=n {
        if i >= 1 && bracing(i) == Bracing::Diamond {
            for q in 1..=4 {
                b.joint(leg_mid(i, q), mid(fr.corner(i - 1, q), fr.corner(i, q)));
            }
        }
        for q in 1..=4 {
            b.joint(corner(i, q), fr.corner(i, q));
        }
        if mids(i) {
            for (f, (a, c)) in FACES.iter().enumerate() {
                b.joint(edge_mid(i, f + 1), mid(fr.corner(i, *a), fr.corner(i, *c)));
            }
        }
    }
    let horizontals = |b: &mut Build, i: usize| {
        if !horizontal(i) {
            return;
        }
        let group = Some(format!("{id}-horizontal-{i}"));
        for (f, (a, c)) in FACES.iter().enumerate() {
            if mids(i) {
                b.member(&corner(i, *a), &edge_mid(i, f + 1), group.clone());
                b.member(&edge_mid(i, f + 1), &corner(i, *c), group.clone());
            } else {
                b.member(&corner(i, *a), &corner(i, *c), group.clone());
            }
        }
    };
    horizontals(&mut b, 0);
    for k in 1..=n {
        let lo = k - 1;
        let leg = Some(format!("{id}-leg"));
        let brace = Some(format!("{id}-bracing-{k}"));
        for q in 1..=4 {
            if bracing(k) == Bracing::Diamond {
                b.member(&corner(lo, q), &leg_mid(k, q), leg.clone());
                b.member(&leg_mid(k, q), &corner(k, q), leg.clone());
            } else {
                b.member(&corner(lo, q), &corner(k, q), leg.clone());
            }
        }
        for (f, (a, c)) in FACES.iter().enumerate() {
            // Bottom left, bottom right, top right, top left.
            let (bl, br, tr, tl) = (corner(lo, *a), corner(lo, *c), corner(k, *c), corner(k, *a));
            match bracing(k) {
                Bracing::Single => b.member(&bl, &tr, brace.clone()),
                Bracing::X => {
                    b.member(&bl, &tr, brace.clone());
                    b.member(&br, &tl, brace.clone());
                }
                Bracing::K => {
                    let m = edge_mid(k, f + 1);
                    b.member(&bl, &m, brace.clone());
                    b.member(&br, &m, brace.clone());
                }
                Bracing::Diamond => {
                    let (mb, mt) = (edge_mid(lo, f + 1), edge_mid(k, f + 1));
                    let (ml, mr) = (leg_mid(k, *a), leg_mid(k, *c));
                    b.member(&mb, &mr, brace.clone());
                    b.member(&mr, &mt, brace.clone());
                    b.member(&mt, &ml, brace.clone());
                    b.member(&ml, &mb, brace.clone());
                }
                Bracing::Warren => {
                    if k % 2 == 1 {
                        b.member(&bl, &tr, brace.clone());
                    } else {
                        b.member(&br, &tl, brace.clone());
                    }
                }
            }
        }
        horizontals(&mut b, k);
    }
    b.body
}

/// The joints of `t` not made by generator `id`: id, position.
fn others<'a>(t: &'a StructureType, id: &str) -> Vec<(&'a str, [f64; 3])> {
    t.nodes
        .iter()
        .filter(|n| n.made_by.as_deref() != Some(id))
        .map(|n| (n.id.as_str(), n.point.map(|v| v.si())))
        .collect()
}

/// The tip joint at point `label`: the joint it is on (not one of this
/// generator's), else a new one `{id}-{label}` and the point put on it.
fn tip(
    t: &StructureType,
    id: &str,
    label: &str,
    b: &mut Build,
) -> Result<(String, [f64; 3]), String> {
    let p = t
        .points
        .iter()
        .find(|p| p.label == label)
        .ok_or_else(|| format!("no point {label}"))?;
    let at = p.position().map(|v| v.si());
    if p.member.is_some() {
        return Err(format!(
            "point {label} lies along a member: give it its own position first"
        ));
    }
    if let Some(n) = &p.node
        && t.nodes
            .iter()
            .any(|x| &x.id == n && x.made_by.as_deref() != Some(id))
    {
        return Ok((n.clone(), at));
    }
    let j = b.joint(format!("{id}-{label}"), at);
    b.body.points.push((label.to_owned(), j.clone()));
    Ok((j, at))
}

/// Joints of one level: its z, the joints (id, position).
type Level<'a> = (f64, Vec<(&'a str, [f64; 3])>);

/// Groups of joints by level (within [`SAME`]), lowest first.
fn by_level<'a>(joints: &[(&'a str, [f64; 3])]) -> Vec<Level<'a>> {
    let mut sorted = joints.to_vec();
    sorted.sort_by(|a, b| a.1[2].total_cmp(&b.1[2]));
    let mut out: Vec<Level> = Vec::new();
    for j in sorted {
        match out.last_mut() {
            Some((z, v)) if (j.1[2] - *z).abs() <= SAME => v.push(j),
            _ => out.push((j.1[2], vec![j])),
        }
    }
    out
}

/// Crossarms: for each point, from the face of the body on its side.
fn crossarm(t: &StructureType, id: &str, c: &Crossarm) -> Result<Body, String> {
    let mut b = Build::new(id);
    for label in &c.points {
        let (tip_id, p) = tip(t, id, label, &mut b)?;
        let a = if p[0].abs() >= p[1].abs() { 0 } else { 1 };
        let o = 1 - a;
        if p[a].abs() < SAME {
            return Err(format!(
                "point {label} is on the structure's axis: a peak, not an arm"
            ));
        }
        let s = p[a].signum();
        let near: Vec<(&str, [f64; 3])> = others(t, id)
            .into_iter()
            .filter(|(j, q)| *j != tip_id && s * q[a] > SAME && s * q[a] < s * p[a] - SAME)
            .collect();
        // At a level, the outermost joints on that side: the face, its two
        // ends along the other axis.
        let face = |joints: &[(&str, [f64; 3])]| -> Option<[(String, [f64; 3]); 2]> {
            let out = joints
                .iter()
                .map(|j| s * j.1[a])
                .fold(f64::NEG_INFINITY, f64::max);
            let on: Vec<&(&str, [f64; 3])> =
                joints.iter().filter(|j| s * j.1[a] >= out - SAME).collect();
            let lo = on.iter().min_by(|x, y| x.1[o].total_cmp(&y.1[o]))?;
            let hi = on.iter().max_by(|x, y| x.1[o].total_cmp(&y.1[o]))?;
            (hi.1[o] - lo.1[o] > SAME).then(|| [(lo.0.to_owned(), lo.1), (hi.0.to_owned(), hi.1)])
        };
        // The face at each level: the lower root at or below the point, the
        // upper the next above; a point at the top of the body hangs from
        // the top and the level below it.
        type Root = (f64, [(String, [f64; 3]); 2]);
        let faces: Vec<Root> = by_level(&near)
            .iter()
            .filter_map(|(z, j)| Some((*z, face(j)?)))
            .collect();
        let at = faces
            .iter()
            .rposition(|(z, _)| *z <= p[2] + SAME)
            .ok_or_else(|| {
                format!(
                    "point {label}: no body face at or below it on its side (build the body first)"
                )
            })?;
        let (lower, upper) = match (faces.get(at + 1), at.checked_sub(1)) {
            (Some(up), _) => (&faces[at], up),
            (None, Some(below)) => (&faces[below], &faces[at]),
            (None, None) => {
                return Err(format!(
                    "point {label}: the body face on its side has one level only"
                ));
            }
        };
        let group = |g: &str| Some(format!("{id}-{g}"));
        match c.shape {
            ArmShape::Triangular => {
                for (r, _) in &lower.1 {
                    b.member(r, &tip_id, group("bottom"));
                }
                for (r, _) in &upper.1 {
                    b.member(r, &tip_id, group("top"));
                }
            }
            ArmShape::Rectangular => {
                let top = b.joint(
                    format!("{id}-{label}-top"),
                    [p[0], p[1], p[2] + (upper.0 - lower.0)],
                );
                for (r, _) in &lower.1 {
                    b.member(r, &tip_id, group("bottom"));
                }
                for (r, _) in &upper.1 {
                    b.member(r, &top, group("top"));
                }
                b.member(&tip_id, &top, group("bracing"));
                for (r, _) in &lower.1 {
                    b.member(r, &top, group("bracing"));
                }
            }
        }
    }
    Ok(b.body)
}

/// A peak: from the joints at the top of the body to each tip.
fn peak(t: &StructureType, id: &str, p: &Peak) -> Result<Body, String> {
    let mut b = Build::new(id);
    let joints = others(t, id);
    let levels = by_level(&joints);
    let (top_z, top) = levels.last().filter(|(_, j)| j.len() >= 3).ok_or(
        "the top of the body: three joints at least at its highest level (build the body first)",
    )?;
    let mut tips = Vec::new();
    if p.points.is_empty() {
        let n = top.len() as f64;
        let c = [0, 1].map(|k| top.iter().map(|j| j.1[k]).sum::<f64>() / n);
        tips.push((
            b.joint(format!("{id}-tip"), [c[0], c[1], top_z + p.height]),
            top_z + p.height,
        ));
    } else {
        for label in &p.points {
            let (j, at) = tip(t, id, label, &mut b)?;
            tips.push((j, at[2]));
        }
    }
    for (j, z) in &tips {
        if *z <= top_z + SAME {
            return Err(format!(
                "the peak's tip {j} must be above the top of the body ({top_z:.3} m)"
            ));
        }
        for (r, _) in top {
            b.member(r, j, Some(format!("{id}-peak")));
        }
    }
    Ok(b.body)
}

/// A face of a panels body copied to the opposite face.
fn copy_face(t: &StructureType, id: &str, c: &CopyFace) -> Result<Body, String> {
    let gens = crate::body::generators(t)?;
    let Some(Kind::Panels(p)) = gens.iter().find(|g| g.id == c.body).map(|g| &g.kind) else {
        return Err(format!("no legs-and-panels generator {}", c.body));
    };
    let fr = Frustum::new(t, p);
    let (axis, side) = c.face.axis();
    let joints = others(t, id);
    let at = |j: &str| joints.iter().find(|x| x.0 == j).map(|x| x.1);
    let mut b = Build::new(id);
    for m in t
        .members
        .iter()
        .filter(|m| m.made_by.as_deref() != Some(id))
    {
        b.pairs.insert(if m.nodes[0] < m.nodes[1] {
            (m.nodes[0].clone(), m.nodes[1].clone())
        } else {
            (m.nodes[1].clone(), m.nodes[0].clone())
        });
    }
    let on: Vec<&ols_structures::structures::TypeMember> = t
        .members
        .iter()
        .filter(|m| m.made_by.as_deref() != Some(id))
        .filter(|m| {
            m.nodes
                .iter()
                .all(|n| at(n).is_some_and(|q| fr.on_face(q, axis, side)))
        })
        .collect();
    if on.is_empty() {
        return Err(format!("no member on face {} of {}", c.face.key(), c.body));
    }
    for m in on {
        let mut ends = Vec::new();
        for n in &m.nodes {
            let mut q = at(n).expect("on the face");
            q[axis] = -q[axis];
            let there = joints
                .iter()
                .find(|j| dist(j.1, q) <= SAME)
                .map(|j| j.0.to_owned());
            ends.push(match there {
                Some(j) => j,
                None => b.joint(format!("{id}-{n}"), q),
            });
        }
        b.member_id(
            Some(format!("{id}-{}", m.id)),
            &ends[0],
            &ends[1],
            m.group.clone(),
        );
    }
    Ok(b.body)
}
