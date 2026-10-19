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

use ohl_structures::StructureType;

use crate::body::{
    ArmShape, Bracing, CopyFace, Crossarm, Generator, Kind, Panels, Peak, Plan, Redundants,
};

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
    /// Members named `{gen_id}-M<n>` so far (members with an id of their
    /// own do not count, so turning an option on does not rename the
    /// others).
    numbered: usize,
}

impl Build {
    fn new(gen_id: &str) -> Self {
        Self {
            gen_id: gen_id.to_owned(),
            body: Body::default(),
            pairs: BTreeSet::new(),
            numbered: 0,
        }
    }

    /// Where its joint `id` is.
    fn pos(&self, id: &str) -> [f64; 3] {
        self.body
            .joints
            .iter()
            .find(|j| j.id == id)
            .map_or([f64::NAN; 3], |j| j.at)
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
        let id = id.unwrap_or_else(|| {
            self.numbered += 1;
            format!("{}-M{}", self.gen_id, self.numbered)
        });
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

/// The levels of a panels body (feet first) and its widths at a level:
/// a frustum, or frustums on each other where panels give their width.
pub struct Frustum {
    /// The levels, m: the feet, then each panel's top.
    pub levels: Vec<f64>,
    /// The widths given, lowest first: level, [across, along] (the feet,
    /// the panels' own, the waist).
    knots: Vec<(f64, [f64; 2])>,
}

impl Frustum {
    /// Of panels `p` on `t`.
    pub fn new(t: &StructureType, p: &Panels) -> Self {
        let mut levels = vec![p.bottom.unwrap_or_else(|| t.foot_level())];
        levels.extend(p.panels.iter().map(|x| x.top));
        let mut knots = vec![(levels[0], p.feet)];
        if let Some((_, below)) = p.panels.split_last() {
            knots.extend(below.iter().filter_map(|x| Some((x.top, x.width?))));
        }
        knots.push((*levels.last().expect("the feet"), p.waist));
        Self { levels, knots }
    }

    /// The widths across (x) and along (y) at level `z`: on the straight
    /// line between the widths given below and above (beyond the ends,
    /// the end's line carried on).
    pub fn width(&self, z: f64) -> [f64; 2] {
        let last = self.knots.len() - 1;
        let s = (0..last)
            .find(|&s| z <= self.knots[s + 1].0)
            .unwrap_or(last.saturating_sub(1));
        let ((z0, w0), (zt, wt)) = (self.knots[s], self.knots[(s + 1).min(last)]);
        let f = if zt > z0 { (z - z0) / (zt - z0) } else { 0.0 };
        [0, 1].map(|k| w0[k] + f * (wt[k] - w0[k]))
    }

    /// The levels (indices) where the slope of the faces changes: a width
    /// given on a panel off the straight line of its neighbours.
    pub fn slope_changes(&self) -> Vec<usize> {
        let slope = |a: (f64, [f64; 2]), b: (f64, [f64; 2])| {
            [0, 1].map(|k| (b.1[k] - a.1[k]) / (b.0 - a.0).max(SAME))
        };
        self.knots
            .windows(3)
            .filter(|w| {
                let (below, above) = (slope(w[0], w[1]), slope(w[1], w[2]));
                (0..2).any(|k| (below[k] - above[k]).abs() > 1e-6)
            })
            .filter_map(|w| self.levels.iter().position(|z| (z - w[1].0).abs() <= SAME))
            .collect()
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
    let redundants = |i: usize| p.panels[i - 1].redundants;
    let plans = plan_levels(t, p, &fr);
    let plan = |i: usize| plans[i];
    let needs_mid = |b: Bracing| matches!(b, Bracing::K | Bracing::Diamond);
    // Joints in the middle of the horizontal edges at level i; horizontals
    // at level i.
    let mids = |i: usize| {
        (i >= 1 && needs_mid(bracing(i))) || (i < n && bracing(i + 1) == Bracing::Diamond)
    };
    let horizontal = |i: usize| (i >= 1 && p.panels[i - 1].horizontal) || mids(i);
    // Joints in the middle of the legs of panel i.
    let leg_mids = |i: usize| bracing(i) == Bracing::Diamond || redundants(i) != Redundants::None;
    let mut b = Build::new(id);
    let corner = |i: usize, q: usize| format!("{id}-L{i}-{q}");
    let edge_mid = |i: usize, f: usize| format!("{id}-L{i}-F{f}");
    let leg_mid = |p: usize, q: usize| format!("{id}-P{p}-{q}");
    let centre = |i: usize| format!("{id}-L{i}-C");
    // Joints the options put on members written whole: the members are
    // split there at the end.
    let mut split_at: Vec<String> = Vec::new();
    // Joints: level by level, the corners, then the middles.
    for i in 0..=n {
        if i >= 1 && leg_mids(i) {
            for q in 1..=4 {
                b.joint(leg_mid(i, q), mid(fr.corner(i - 1, q), fr.corner(i, q)));
                if bracing(i) != Bracing::Diamond {
                    split_at.push(leg_mid(i, q));
                }
            }
        }
        for q in 1..=4 {
            b.joint(corner(i, q), fr.corner(i, q));
        }
        if mids(i) {
            for (f, (a, c)) in FACES.iter().enumerate() {
                b.joint(edge_mid(i, f + 1), mid(fr.corner(i, *a), fr.corner(i, *c)));
            }
        } else if plan(i) == Plan::Diamond {
            for (f, (a, c)) in FACES.iter().enumerate() {
                b.joint(edge_mid(i, f + 1), mid(fr.corner(i, *a), fr.corner(i, *c)));
                split_at.push(edge_mid(i, f + 1));
            }
        }
        if plan(i) == Plan::Diamond {
            b.joint(centre(i), [0.0, 0.0, fr.levels[i]]);
        }
    }
    let horizontals = |b: &mut Build, i: usize| {
        let group = Some(format!("{id}-horizontal-{i}"));
        if horizontal(i) {
            for (f, (a, c)) in FACES.iter().enumerate() {
                if mids(i) {
                    b.member(&corner(i, *a), &edge_mid(i, f + 1), group.clone());
                    b.member(&edge_mid(i, f + 1), &corner(i, *c), group.clone());
                } else {
                    b.member(&corner(i, *a), &corner(i, *c), group.clone());
                }
            }
        } else if plan(i) != Plan::None {
            // Plan bracing needs the level's horizontals: ids of their own.
            for (f, (a, c)) in FACES.iter().enumerate() {
                b.member_id(
                    Some(format!("{id}-H{i}-E{}", f + 1)),
                    &corner(i, *a),
                    &corner(i, *c),
                    group.clone(),
                );
            }
        }
        // Plan bracing.
        let group = Some(format!("{id}-plan-{i}"));
        let ends: Vec<(String, String)> = match plan(i) {
            Plan::None => Vec::new(),
            Plan::X => vec![(corner(i, 1), corner(i, 3)), (corner(i, 2), corner(i, 4))],
            // The diamond, and a cross from its corners to the centre: the
            // middles of the horizontals slide along them otherwise (the
            // diamond turning in plan).
            Plan::Diamond => (1..=4)
                .map(|f| (edge_mid(i, f), edge_mid(i, f % 4 + 1)))
                .chain((1..=4).map(|f| (edge_mid(i, f), centre(i))))
                .collect(),
        };
        for (k, (a, c)) in ends.iter().enumerate() {
            b.member_id(Some(format!("{id}-H{i}-{}", k + 1)), a, c, group.clone());
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
        if redundants(k) != Redundants::None {
            let mut r = Redundant {
                b: &mut b,
                id,
                k,
                face: 0,
                count: 0,
                split_at: &mut split_at,
            };
            for (f, (a, c)) in FACES.iter().enumerate() {
                r.face = f + 1;
                let face = FaceJoints {
                    bl: corner(lo, *a),
                    br: corner(lo, *c),
                    tr: corner(k, *c),
                    tl: corner(k, *a),
                    ml: leg_mid(k, *a),
                    mr: leg_mid(k, *c),
                    mb: edge_mid(lo, f + 1),
                    mt: edge_mid(k, f + 1),
                };
                r.face(bracing(k), redundants(k) == Redundants::Two, &face);
            }
        }
        horizontals(&mut b, k);
    }
    split(&mut b, &split_at);
    b.body
}

/// The plan bracing of each level of panels `p` (feet first): a panel's
/// own at its top, else `plan_auto` where the slope changes and at the
/// levels a crossarm's roots are on.
fn plan_levels(t: &StructureType, p: &Panels, fr: &Frustum) -> Vec<Plan> {
    let n = p.panels.len();
    let mut out = vec![Plan::None; n + 1];
    if p.plan_auto != Plan::None {
        let mut at = fr.slope_changes();
        at.extend(crossarm_levels(t, fr));
        for i in at {
            out[i] = p.plan_auto;
        }
    }
    for (k, panel) in p.panels.iter().enumerate() {
        if panel.plan != Plan::None {
            out[k + 1] = panel.plan;
        }
    }
    out
}

/// The levels (indices) of the frustum a crossarm of `t` starts from: for
/// each point of a crossarm generator outside the frustum, the level at
/// or below it and the next (at the top: the top and the level below),
/// as the crossarm finds its roots.
fn crossarm_levels(t: &StructureType, fr: &Frustum) -> Vec<usize> {
    let n = fr.levels.len() - 1;
    let mut out = Vec::new();
    for g in crate::body::generators(t).unwrap_or_default() {
        let Kind::Crossarm(c) = &g.kind else {
            continue;
        };
        for label in &c.points {
            let Some(pt) = t.points.iter().find(|x| &x.label == label) else {
                continue;
            };
            let p = pt.position().map(|v| v.si());
            let a = if p[0].abs() >= p[1].abs() { 0 } else { 1 };
            if p[a].abs() <= 0.5 * fr.width(p[2])[a] + SAME
                || p[2] < fr.levels[0] - SAME
                || p[2] > fr.levels[n] + SAME
            {
                continue;
            }
            let Some(lower) = fr.levels.iter().rposition(|z| *z <= p[2] + SAME) else {
                continue;
            };
            out.push(lower);
            if lower < n {
                out.push(lower + 1);
            } else if lower > 0 {
                out.push(lower - 1);
            }
        }
    }
    out
}

/// The joints of one face of a panel, seen from outside: corners bottom
/// left, bottom right, top right, top left; the legs' middles; the
/// middles of the bottom and top edges.
struct FaceJoints {
    bl: String,
    br: String,
    tr: String,
    tl: String,
    ml: String,
    mr: String,
    mb: String,
    mt: String,
}

/// Writes the redundants of panel `k`'s faces.
struct Redundant<'a> {
    b: &'a mut Build,
    id: &'a str,
    k: usize,
    face: usize,
    count: usize,
    split_at: &'a mut Vec<String>,
}

impl Redundant<'_> {
    /// A joint `name` at `at` on a member (split there at the end).
    fn joint(&mut self, name: &str, at: [f64; 3]) -> String {
        let j = self
            .b
            .joint(format!("{}-R{}-F{}-{name}", self.id, self.k, self.face), at);
        self.split_at.push(j.clone());
        j
    }

    /// A joint `name` in the middle of `a`–`c`, braced to `to`.
    fn mid_to(&mut self, name: &str, a: &str, c: &str, to: &str) -> String {
        let at = mid(self.b.pos(a), self.b.pos(c));
        let j = self.joint(name, at);
        self.member(&j, to);
        j
    }

    fn member(&mut self, a: &str, c: &str) {
        self.count += 1;
        let id = format!("{}-R{}-{}", self.id, self.k, self.count);
        let group = Some(format!("{}-redundant-{}", self.id, self.k));
        self.b.member_id(Some(id), a, c, group);
    }

    /// The redundants of one face with bracing `bracing`; `two`: two
    /// levels. Each new joint lies on a member and is braced to a joint
    /// held already (a corner, a leg's middle or a joint braced before),
    /// so the face stays triangulated.
    fn face(&mut self, bracing: Bracing, two: bool, f: &FaceJoints) {
        let FaceJoints {
            bl,
            br,
            tr,
            tl,
            ml,
            mr,
            mb,
            mt,
        } = f;
        match bracing {
            Bracing::Single | Bracing::Warren => {
                // The diagonal from a (bottom) to c (top); ma, mc the
                // middles of their legs.
                let (a, c, ma, mc) = if bracing == Bracing::Single || self.k % 2 == 1 {
                    (bl, tr, ml, mr)
                } else {
                    (br, tl, mr, ml)
                };
                // Its middle, on a horizontal between the legs' middles;
                // the quadrilaterals either side braced from a and c.
                let m = self.mid_to("C", a, c, ma);
                self.member(&m, mc);
                self.member(a, mc);
                self.member(c, ma);
                if two {
                    self.mid_to("D1", a, &m, ma);
                    self.mid_to("D2", &m, c, mc);
                }
            }
            Bracing::X => {
                // A joint at the crossing, braced to the legs' middles.
                let at = crossing(
                    [self.b.pos(bl), self.b.pos(tr)],
                    [self.b.pos(br), self.b.pos(tl)],
                );
                let m = self.joint("C", at);
                self.member(&m, ml);
                self.member(&m, mr);
                if two {
                    self.mid_to("D1", bl, &m, ml);
                    self.mid_to("D2", &m, tl, ml);
                    self.mid_to("D3", br, &m, mr);
                    self.mid_to("D4", &m, tr, mr);
                }
            }
            Bracing::K => {
                // Each diagonal's middle, braced to its leg's middle and to
                // the top corner on its side.
                let m1 = self.mid_to("C1", bl, mt, ml);
                self.member(&m1, tl);
                let m2 = self.mid_to("C2", br, mt, mr);
                self.member(&m2, tr);
                if two {
                    self.mid_to("D1", bl, &m1, ml);
                    self.mid_to("D2", &m1, mt, tl);
                    self.mid_to("D3", br, &m2, mr);
                    self.mid_to("D4", &m2, mt, tr);
                }
            }
            Bracing::Diamond => {
                // Each side of the diamond: its middle braced to the corner
                // it faces.
                for (n, (u, v, to)) in [(ml, mb, bl), (mb, mr, br), (mr, mt, tr), (mt, ml, tl)]
                    .into_iter()
                    .enumerate()
                {
                    let m = self.mid_to(&format!("C{}", n + 1), u, v, to);
                    if two {
                        self.mid_to(&format!("D{}", 2 * n + 1), u, &m, to);
                        self.mid_to(&format!("D{}", 2 * n + 2), &m, v, to);
                    }
                }
            }
        }
    }
}

/// Where lines `p` and `q` cross (the middle of their closest points).
fn crossing(p: [[f64; 3]; 2], q: [[f64; 3]; 2]) -> [f64; 3] {
    let d1 = [0, 1, 2].map(|k| p[1][k] - p[0][k]);
    let d2 = [0, 1, 2].map(|k| q[1][k] - q[0][k]);
    let r = [0, 1, 2].map(|k| p[0][k] - q[0][k]);
    let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|k| a[k] * b[k]).sum::<f64>();
    let (a, b, c, d, e) = (
        dot(d1, d1),
        dot(d1, d2),
        dot(d2, d2),
        dot(d1, r),
        dot(d2, r),
    );
    let den = a * c - b * b;
    let (s, u) = if den.abs() < 1e-12 {
        (0.5, 0.5)
    } else {
        ((b * e - c * d) / den, (a * e - b * d) / den)
    };
    let x = [0, 1, 2].map(|k| p[0][k] + s * d1[k]);
    let y = [0, 1, 2].map(|k| q[0][k] + u * d2[k]);
    mid(x, y)
}

/// Every member with joints of `at` lying on it (not at its ends) cut
/// there: the pieces `{its id}-1`, `-2`… in its group.
fn split(b: &mut Build, at: &[String]) {
    if at.is_empty() {
        return;
    }
    let points: Vec<(&String, [f64; 3])> = at.iter().map(|j| (j, b.pos(j))).collect();
    let mut out = Vec::with_capacity(b.body.members.len());
    for m in std::mem::take(&mut b.body.members) {
        let (p0, p1) = (b.pos(&m.nodes[0]), b.pos(&m.nodes[1]));
        let d = [0, 1, 2].map(|k| p1[k] - p0[k]);
        let len2: f64 = d.iter().map(|v| v * v).sum();
        let len = len2.sqrt();
        let mut on: Vec<(f64, &String)> = points
            .iter()
            .filter(|(j, _)| **j != m.nodes[0] && **j != m.nodes[1])
            .filter_map(|(j, q)| {
                let s = (0..3).map(|k| (q[k] - p0[k]) * d[k]).sum::<f64>() / len2;
                let foot = [0, 1, 2].map(|k| p0[k] + s * d[k]);
                (s * len > SAME && (1.0 - s) * len > SAME && dist(foot, *q) <= 2.0 * SAME)
                    .then_some((s, *j))
            })
            .collect();
        if on.is_empty() {
            out.push(m);
            continue;
        }
        on.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut chain = vec![m.nodes[0].clone()];
        chain.extend(on.into_iter().map(|(_, j)| j.clone()));
        chain.push(m.nodes[1].clone());
        for (k, w) in chain.windows(2).enumerate() {
            out.push(BodyMember {
                id: format!("{}-{}", m.id, k + 1),
                nodes: [w[0].clone(), w[1].clone()],
                group: m.group.clone(),
            });
        }
    }
    b.body.members = out;
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

/// A root of a crossarm: a level of the body face, its two joints (id,
/// position) at the ends along the other axis.
type Root = (f64, [(String, [f64; 3]); 2]);

/// Where the arm to point `label` (at `p`, its tip joint `tip_id`)
/// starts: the axis it goes along (0 x, 1 y), its side (±1), the lower
/// and the upper root.
fn arm_roots(
    t: &StructureType,
    id: &str,
    label: &str,
    tip_id: &str,
    p: [f64; 3],
) -> Result<(usize, f64, Root, Root), String> {
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
    let faces: Vec<Root> = by_level(&near)
        .iter()
        .filter_map(|(z, j)| Some((*z, face(j)?)))
        .collect();
    let at = faces
        .iter()
        .rposition(|(z, _)| *z <= p[2] + SAME)
        .ok_or_else(|| {
            format!("point {label}: no body face at or below it on its side (build the body first)")
        })?;
    let (lower, upper) = match (faces.get(at + 1), at.checked_sub(1)) {
        (Some(up), _) => (faces[at].clone(), up.clone()),
        (None, Some(below)) => (faces[below].clone(), faces[at].clone()),
        (None, None) => {
            return Err(format!(
                "point {label}: the body face on its side has one level only"
            ));
        }
    };
    Ok((a, s, lower, upper))
}

/// Crossarms: for each point, from the face of the body on its side (with
/// `one_arm`: one arm for the points sharing their roots).
fn crossarm(t: &StructureType, id: &str, c: &Crossarm) -> Result<Body, String> {
    let mut b = Build::new(id);
    if c.one_arm {
        return one_arm(t, id, c, b);
    }
    for label in &c.points {
        let (tip_id, p) = tip(t, id, label, &mut b)?;
        let (_, _, lower, upper) = arm_roots(t, id, label, &tip_id, p)?;
        let (lower, upper) = (&lower, &upper);
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
            // The point on the bottom chord, the other chord's tip one
            // level up; at the top of the body (no level above) the point
            // is on the top chord and the other tip one level down.
            ArmShape::Rectangular if p[2] < upper.0 - SAME => {
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
            ArmShape::Rectangular => {
                let bottom = b.joint(
                    format!("{id}-{label}-bottom"),
                    [p[0], p[1], p[2] - (upper.0 - lower.0)],
                );
                for (r, _) in &lower.1 {
                    b.member(r, &bottom, group("bottom"));
                }
                for (r, _) in &upper.1 {
                    b.member(r, &tip_id, group("top"));
                }
                b.member(&bottom, &tip_id, group("bracing"));
                for (r, _) in &lower.1 {
                    b.member(r, &tip_id, group("bracing"));
                }
            }
        }
    }
    Ok(b.body)
}

/// A point of a crossarm with its joint and roots.
struct ArmPoint {
    label: String,
    joint: String,
    p: [f64; 3],
    a: usize,
    s: f64,
    lower: Root,
    upper: Root,
}

impl ArmPoint {
    /// The arm it is on: its axis, side and root joints.
    fn arm(&self) -> (usize, bool, [&str; 4]) {
        (
            self.a,
            self.s > 0.0,
            [
                &self.lower.1[0].0,
                &self.lower.1[1].0,
                &self.upper.1[0].0,
                &self.upper.1[1].0,
            ],
        )
    }

    /// Its distance from the structure's axis along the arm.
    fn reach(&self) -> f64 {
        self.s * self.p[self.a]
    }
}

/// Where the line from `r` to `e` is at coordinate `x` along axis `a`.
fn along(r: [f64; 3], e: [f64; 3], a: usize, x: f64) -> [f64; 3] {
    let f = (x - r[a]) / (e[a] - r[a]);
    [0, 1, 2].map(|k| r[k] + f * (e[k] - r[k]))
}

/// One arm for the points sharing their roots (an arm per side and level):
/// the outermost point at the tip as a single arm has it; the arm cut by a
/// section at each other point. A section: joints where the arm's two
/// chords pass (the chord the points are on and the other), the point
/// braced to the four, a member up each side and one across the other
/// chord. Between sections: the chords, a diagonal in each side face (from
/// the lower chord to the upper one, outwards) and two members from the
/// section inside to the next point. With one point this is the arm of
/// [`crossarm`] without `one_arm`.
fn one_arm(t: &StructureType, id: &str, c: &Crossarm, mut b: Build) -> Result<Body, String> {
    let mut points = Vec::new();
    for label in &c.points {
        let (joint, p) = tip(t, id, label, &mut b)?;
        let (a, s, lower, upper) = arm_roots(t, id, label, &joint, p)?;
        points.push(ArmPoint {
            label: label.clone(),
            joint,
            p,
            a,
            s,
            lower,
            upper,
        });
    }
    // The arms, in the order of their first point.
    let mut arms: Vec<Vec<&ArmPoint>> = Vec::new();
    for pt in &points {
        match arms.iter_mut().find(|g| g[0].arm() == pt.arm()) {
            Some(g) => g.push(pt),
            None => arms.push(vec![pt]),
        }
    }
    let group = |g: &str| Some(format!("{id}-{g}"));
    for mut arm in arms {
        arm.sort_by(|x, y| x.reach().total_cmp(&y.reach()));
        for w in arm.windows(2) {
            if w[1].reach() - w[0].reach() <= SAME {
                return Err(format!(
                    "points {} and {} are as far out on one arm: put one on another crossarm \
                     generator",
                    w[0].label, w[1].label
                ));
            }
        }
        let tip = *arm.last().expect("a point");
        let (a, s, p) = (tip.a, tip.s, tip.p);
        let (lower, upper) = (&tip.lower, &tip.upper);
        let depth = upper.0 - lower.0;
        // The chord the points are on and the other: its roots, group and
        // tip joint; whether the points' chord is the lower one.
        let (on_roots, on_group, other_roots, other_group, other_tip, on_lower) = match c.shape {
            ArmShape::Triangular => (lower, "bottom", upper, "top", tip.joint.clone(), true),
            ArmShape::Rectangular if p[2] < upper.0 - SAME => {
                let j = b.joint(
                    format!("{id}-{}-top", tip.label),
                    [p[0], p[1], p[2] + depth],
                );
                (lower, "bottom", upper, "top", j, true)
            }
            ArmShape::Rectangular => {
                let j = b.joint(
                    format!("{id}-{}-bottom", tip.label),
                    [p[0], p[1], p[2] - depth],
                );
                (upper, "top", lower, "bottom", j, false)
            }
        };
        let other_end = b.pos(&other_tip);
        // The sections, from the roots out: the points' chord's joints,
        // the other chord's, the point.
        type Section = ([String; 2], [String; 2], Option<String>);
        let mut sections: Vec<Section> = vec![(
            [on_roots.1[0].0.clone(), on_roots.1[1].0.clone()],
            [other_roots.1[0].0.clone(), other_roots.1[1].0.clone()],
            None,
        )];
        for pt in &arm[..arm.len() - 1] {
            let x = pt.p[a];
            if on_roots
                .1
                .iter()
                .chain(&other_roots.1)
                .any(|r| s * x <= s * r.1[a] + SAME)
            {
                return Err(format!(
                    "point {}: not beyond the body face, so not on the arm",
                    pt.label
                ));
            }
            let chord = |b: &mut Build, roots: &Root, end: [f64; 3], name: &str| {
                [0, 1].map(|i| {
                    b.joint(
                        format!("{id}-{}-{name}-{}", pt.label, i + 1),
                        along(roots.1[i].1, end, a, x),
                    )
                })
            };
            let on = chord(&mut b, on_roots, p, on_group);
            let other = chord(&mut b, other_roots, other_end, other_group);
            sections.push((on, other, Some(pt.joint.clone())));
        }
        sections.push((
            [tip.joint.clone(), tip.joint.clone()],
            [other_tip.clone(), other_tip.clone()],
            Some(tip.joint.clone()),
        ));
        for w in sections.windows(2) {
            let ((on0, other0, _), (on1, other1, point)) = (&w[0], &w[1]);
            let point = point.as_ref().expect("a section with its point");
            for i in 0..2 {
                b.member(&on0[i], &on1[i], group(on_group));
                b.member(&other0[i], &other1[i], group(other_group));
            }
            for i in 0..2 {
                b.member(&on0[i], point, group("bracing"));
                let (lo, up) = if on_lower {
                    (&on0[i], &other1[i])
                } else {
                    (&other0[i], &on1[i])
                };
                b.member(lo, up, group("bracing"));
            }
            if on1[0] == on1[1] {
                // The tip.
                b.member(point, &other1[0], group("bracing"));
            } else {
                for i in 0..2 {
                    b.member(point, &on1[i], group("bracing"));
                    b.member(point, &other1[i], group("bracing"));
                    b.member(&on1[i], &other1[i], group("bracing"));
                }
                b.member(&other1[0], &other1[1], group("bracing"));
            }
        }
    }
    Ok(b.body)
}

/// A peak: from the joints at the top of the body to each tip.
fn peak(t: &StructureType, id: &str, p: &Peak) -> Result<Body, String> {
    let mut b = Build::new(id);
    // The body's joints: not the crossarms' (an arm's tip at the top level
    // would pull the peak's centre aside).
    let arms: BTreeSet<String> = crate::body::generators(t)
        .unwrap_or_default()
        .into_iter()
        .filter(|g| matches!(g.kind, Kind::Crossarm(_)))
        .map(|g| g.id)
        .collect();
    let joints: Vec<(&str, [f64; 3])> = t
        .nodes
        .iter()
        .filter(|n| {
            n.made_by
                .as_deref()
                .is_none_or(|m| m != id && !arms.contains(m))
        })
        .map(|n| (n.id.as_str(), n.point.map(|v| v.si())))
        .collect();
    let levels = by_level(&joints);
    // The highest level of the body itself (an arm's tips above it are
    // fewer than three).
    let (top_z, top) = levels.iter().rev().find(|(_, j)| j.len() >= 3).ok_or(
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
    let on: Vec<&ohl_structures::structures::TypeMember> = t
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
