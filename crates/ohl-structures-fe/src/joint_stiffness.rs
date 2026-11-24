// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! End releases and bolt slip from the joints' bolts (stage 4.4 of the
//! [joint details], queue item C8 of 2026-11-18; built 2026-11-23, in
//! active development, not approved yet). Two **options of the analysis**,
//! both off by default and stored in no file ([`crate::Analysis::end_releases`],
//! [`crate::Analysis::bolt_slip`]): a run turns them on, [`apply`] reads
//! each member end's bolts and sets the members' [`crate::Member::releases`]
//! and [`crate::Member::slip`], and the solve takes them.
//!
//! **Which ends are bolted.** An end with a `[[connection]]` that is bolted
//! (not continuous, not welded or seated), with its bolts or the member's;
//! an end without a connection only where the member's design data gives
//! bolts and the end is not in line (within 1°) with a member of the same
//! section at the joint — that one is taken as the same member running on
//! through the joint (a tower program's member split at a crossing),
//! neither released nor slipping there. Ends with no bolts known: neither.
//!
//! **End releases** (frames only: a truss is pinned already). One bolt
//! holds no bending: both bending rotations of the end are released (a
//! pin; torsion kept). Two bolts or more in **one row** along the member
//! (one gauge line: the layout's gauges, else holes in one cross-section
//! = 1) in one leg of an angle hold the bending in that leg's plane (the
//! row's couple of shear forces) but not out of it: the rotation about
//! the leg's own direction is released (the leg along the section's y →
//! θy; along z → θz; the long leg is the longer of b (y) and h (z), equal
//! legs: long → y, short → z, as the joint tools draw them). Two rows or
//! more, both legs, or rows not known: not released. Released in the
//! element by static condensation of its tangent (elastic and geometric
//! together, McGuire, Gallagher & Ziemian, Matrix Structural Analysis,
//! 2nd ed., 2000, §4.6 and §7 on member end releases; the self-weight's
//! fixed-end loads condensed alike). Where releasing every end would
//! leave a joint's rotation held by nothing (no member end there holds
//! it), the stiffest released end at that joint is kept rigid, said.
//!
//! **Bolt slip** (any kind but cables). Each bolted end whose bolt and
//! hole diameters are known slips by its clearance (hole − bolt): the
//! member lengthens or shortens by up to the sum of its ends' clearances
//! with no force (no friction of snug-tight bolts counted — the full slip
//! at any force, on the safe side for the deflections), then bears
//! elastically — the slip–bearing law of the measured joints
//! (Kitipornchai, Al-Bermani & Peyrot, "Effect of bolt slippage on
//! ultimate behavior of lattice structures", J. Struct. Eng. 120 (8),
//! 1994; Ungkurapinan, Chandrakeerthy, Rajapakse & Yue, "Joint slip in
//! steel electric transmission towers", Eng. Struct. 25 (6), 2003)
//! without its friction branch. Inside its clearance a member keeps
//! [`SLIP_FLOOR`] of its EA/L (else a joint held only by members inside
//! their clearance would be free). Solved by secant passes
//! (`solve_slipping`): each slipping member's axial stiffness taken as
//! N / (N L/EA + δ) from the last pass's force N (δ its slip, at least the
//! floor), first in first order (at most [`SLIP_PASSES_FIRST`]), then with
//! P-Delta when the run asks for it (at most [`SLIP_PASSES`]), until no
//! force and no displacement changes by more than [`SLIP_TOLERANCE`] of the
//! largest from one pass to the next (not settled: said, the last kept).
//! Where the solve with releases or slip finds a mechanism, a buckling mode
//! or implausible displacements, they are taken back at the members
//! meeting the joints it names and it is solved again
//! (`solve_with_options`, said). A joint assembled with its bolts already
//! bearing the other way can slip up to twice the clearance: not taken
//! (said in the theory).
//!
//! [joint details]: https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/joint-details.md

use nalgebra::{Matrix3, Matrix4, SymmetricEigen, Vector3};
use ohl_structures::connections::{CONTINUOUS_ANGLE_DEG, ConnectionKind};

use crate::design::ConnectedLeg;
use crate::element::{Mat12, frame_elastic};
use crate::model::{Member, MemberKind, Model, Section};
use crate::solve::{FeError, Solution};

/// The bending rotations released at one member end, local axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndRelease {
    /// The rotation about the local y (bending in the x–z plane).
    pub about_y: bool,
    /// The rotation about the local z (bending in the x–y plane).
    pub about_z: bool,
}

impl EndRelease {
    /// Nothing released.
    pub fn is_none(self) -> bool {
        !self.about_y && !self.about_z
    }

    /// Both bending rotations (a pin).
    pub fn both(self) -> bool {
        self.about_y && self.about_z
    }
}

/// A member end's bolts as the options read them.
#[derive(Debug, Clone, PartialEq)]
pub struct EndBolting {
    /// How many bolts.
    pub count: u32,
    /// Bolt rows (gauge lines) along the member, when known.
    pub rows: Option<u32>,
    /// The leg bolted, when known.
    pub leg: Option<ConnectedLeg>,
    /// Hole clearance (hole − bolt diameter), m, when both are known and
    /// the hole is the larger.
    pub clearance: Option<f64>,
    /// From the end's `[[connection]]` (else the member's design data).
    pub from_connection: bool,
}

/// What [`apply`] did, for the results' notes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Summary {
    /// End releases were asked for.
    pub releases_on: bool,
    /// Bolt slip was asked for.
    pub slip_on: bool,
    /// Frame ends released fully (one bolt).
    pub pinned: usize,
    /// Frame ends released about one axis (one row).
    pub one_row: usize,
    /// Frame ends bolted but held (two rows or more, both legs).
    pub held: usize,
    /// Frame ends bolted whose rows or leg are not known: not released.
    pub not_known: usize,
    /// Joints where a release was taken back so that something holds
    /// their rotation (their ids).
    pub kept_rigid: Vec<String>,
    /// Members that slip.
    pub slipping: usize,
    /// Bolted ends without a known clearance (no slip there).
    pub no_clearance: usize,
    /// Ends taken as running on through their joint (in line with a member
    /// of the same section, no connection): neither released nor slipping.
    pub running_on: usize,
    /// The largest slip of a member, m.
    pub largest_slip: f64,
}

/// The share of a slipping member's axial stiffness EA/L taken at least
/// (a member inside its clearance carries almost nothing; a share of zero
/// would leave its joints free).
pub const SLIP_FLOOR: f64 = 1e-2;
/// Most secant passes in first order (`solve_slipping`).
pub const SLIP_PASSES_FIRST: u32 = 200;
/// Most secant passes with P-Delta after those in first order.
pub const SLIP_PASSES: u32 = 30;
/// The slip passes are settled when no member force changes by more than
/// this share of the largest and no joint moves by more than this share of
/// the largest displacement from one pass to the next.
pub const SLIP_TOLERANCE: f64 = 1e-4;

/// Whether `idx`'s end `end` (0: at its first joint) lies in line, within
/// 1°, with another member of the same section at the joint.
fn runs_on(model: &Model, idx: usize, end: usize) -> bool {
    let m = &model.members[idx];
    let (node, far) = if end == 0 { (m.i, m.j) } else { (m.j, m.i) };
    let dir = |from: usize, to: usize| {
        let d = crate::model::sub(model.nodes[to].point, model.nodes[from].point);
        let l = crate::model::norm(d);
        [d[0] / l, d[1] / l, d[2] / l]
    };
    let own = dir(node, far);
    let cos = CONTINUOUS_ANGLE_DEG.to_radians().cos();
    model.members.iter().enumerate().any(|(k, o)| {
        if k == idx || o.section != m.section || (o.i != node && o.j != node) {
            return false;
        }
        let other = if o.i == node { o.j } else { o.i };
        let d = dir(node, other);
        -(own[0] * d[0] + own[1] * d[1] + own[2] * d[2]) >= cos
    })
}

/// The bolts of member `idx`'s end `end` as the options read them, or why
/// the end is not taken as bolted.
pub fn end_bolting(model: &Model, idx: usize, end: usize) -> Result<EndBolting, &'static str> {
    let m = &model.members[idx];
    let d = &m.design;
    let e = d.ends[end].as_ref();
    match e {
        Some(e) => {
            if e.continuous.is_some() {
                return Err("continuous through the joint");
            }
            if e.kind.is_some_and(|k| k != ConnectionKind::Bolted) {
                return Err("not bolted");
            }
        }
        None => {
            if d.bolts.is_none() {
                return Err("no bolts given");
            }
            if runs_on(model, idx, end) {
                return Err("running on through the joint");
            }
        }
    }
    let Some((b, _, _)) = crate::member_ends::end_bolts(d, e, &mut Vec::new()) else {
        return Err("no bolts given");
    };
    if b.count == 0 {
        return Err("no bolts given");
    }
    let rows = e
        .and_then(|e| (!e.gauges.is_empty()).then_some(e.gauges.len() as u32))
        .or(b.holes);
    let leg = e.and_then(|e| e.leg).or(d.connected_leg);
    let clearance = b.hole.map(|h| h - b.diameter).filter(|c| *c > 0.0);
    Ok(EndBolting {
        count: b.count,
        rows,
        leg,
        clearance,
        from_connection: e.is_some(),
    })
}

/// Whether a leg `leg` of `section` lies along the section's y (else z):
/// the long leg is the longer of b (y) and h (z); equal legs, or a section
/// without an angle profile: long → y, short → z. None: no leg of an
/// angle (both, none).
fn leg_along_y(section: &Section, leg: ConnectedLeg) -> Option<bool> {
    let (b, h) = match section.profile.as_ref().map(|p| &p.shape) {
        Some(ohl_sections::Shape::EqualAngle { h, .. }) => (*h, *h),
        Some(ohl_sections::Shape::UnequalAngle { h, b, .. }) => (*b, *h),
        Some(_) => return None,
        None => (1.0, 1.0),
    };
    match leg {
        ConnectedLeg::Long => Some(b >= h),
        ConnectedLeg::Short => Some(b < h),
        ConnectedLeg::Both | ConnectedLeg::None => None,
    }
}

/// The release of a frame end bolted as `b` and why ("one bolt", "one row
/// of 3 bolts in the long leg"); none released: why not.
pub fn release_of(section: &Section, b: &EndBolting) -> (EndRelease, String) {
    if b.count == 1 {
        return (
            EndRelease {
                about_y: true,
                about_z: true,
            },
            "one bolt: pinned".into(),
        );
    }
    match (b.rows, b.leg) {
        (Some(1), Some(leg)) => match leg_along_y(section, leg) {
            Some(y) => (
                EndRelease {
                    about_y: y,
                    about_z: !y,
                },
                format!(
                    "one row of {} bolts in the {} leg: free out of its plane",
                    b.count,
                    leg.as_str()
                ),
            ),
            None if matches!(leg, ConnectedLeg::Both) => {
                (EndRelease::default(), "both legs bolted: held".into())
            }
            // Not an angle's leg (a channel, a tube…): a row's plane is not
            // known here — counted with the ends whose leg is not known.
            None => (
                EndRelease::default(),
                "not an angle: the leg bolted not known".into(),
            ),
        },
        (Some(1), None) => (EndRelease::default(), "the leg bolted not known".into()),
        (Some(_), _) => (EndRelease::default(), "two rows or more: held".into()),
        (None, _) => (EndRelease::default(), "bolt rows not known".into()),
    }
}

/// Sets each member's releases and slip from its ends' bolts as the
/// model's analysis options ask ([`crate::Analysis::end_releases`],
/// [`crate::Analysis::bolt_slip`]); both off: every member's set back to
/// none. Kept in [`Model::joint_stiffness`] for the notes.
pub fn apply(model: &mut Model) -> Summary {
    let mut sum = Summary {
        releases_on: model.analysis.end_releases,
        slip_on: model.analysis.bolt_slip,
        ..Summary::default()
    };
    for m in &mut model.members {
        m.releases = Default::default();
        m.slip = 0.0;
        m.axial_share = 1.0;
    }
    if !sum.releases_on && !sum.slip_on {
        model.joint_stiffness = None;
        return sum;
    }
    for idx in 0..model.members.len() {
        let kind = model.members[idx].kind;
        let frame = model.members[idx].bends_by_kind();
        let mut slip = 0.0;
        let mut releases = [EndRelease::default(); 2];
        for (end, release) in releases.iter_mut().enumerate() {
            let b = match end_bolting(model, idx, end) {
                Ok(b) => b,
                Err(why) => {
                    if why == "running on through the joint" {
                        sum.running_on += 1;
                    }
                    continue;
                }
            };
            if sum.releases_on && frame {
                let (r, why) = release_of(&model.sections[model.members[idx].section], &b);
                if r.both() {
                    sum.pinned += 1;
                } else if !r.is_none() {
                    sum.one_row += 1;
                } else if why.ends_with("held") {
                    sum.held += 1;
                } else {
                    sum.not_known += 1;
                }
                *release = r;
            }
            if sum.slip_on && kind != MemberKind::Cable {
                match b.clearance {
                    Some(c) => slip += c,
                    None => sum.no_clearance += 1,
                }
            }
        }
        let m = &mut model.members[idx];
        m.releases = releases;
        m.slip = slip;
        if slip > 0.0 {
            sum.slipping += 1;
            sum.largest_slip = sum.largest_slip.max(slip);
        }
    }
    if sum.releases_on {
        sum.kept_rigid = keep_joints_held(model, &mut sum);
    }
    model.joint_stiffness = Some(sum.clone());
    sum
}

/// The rotational stiffness a joint gets from its frame ends (structure
/// axes): each end's torsion GJ/L and bending 4EI/L about the axes it
/// holds, or all of them (`releases` false).
fn joint_rotation(model: &Model, node: usize, releases: bool) -> Matrix3<f64> {
    let mut s = Matrix3::zeros();
    for m in model.members.iter().filter(|m| {
        if releases {
            m.is_frame()
        } else {
            m.bends_by_kind()
        }
    }) {
        let end = if m.i == node {
            0
        } else if m.j == node {
            1
        } else {
            continue;
        };
        let sec = &model.sections[m.section];
        let l = model.length(m);
        let r = crate::element::axes(model, m);
        let rel = if releases {
            m.releases[end]
        } else {
            EndRelease::default()
        };
        let d = Vector3::new(
            sec.g * sec.j / l,
            if rel.about_y {
                0.0
            } else {
                4.0 * sec.e * sec.iy / l
            },
            if rel.about_z {
                0.0
            } else {
                4.0 * sec.e * sec.iz / l
            },
        );
        s += r.transpose() * Matrix3::from_diagonal(&d) * r;
    }
    for a in 0..3 {
        if model.nodes[node].fixed[3 + a] {
            s[(a, a)] += f64::MAX.sqrt();
        }
        s[(a, a)] += model.nodes[node].spring[3 + a];
    }
    s
}

/// The number of directions of `s` with stiffness, against `scale`.
fn rank(s: &Matrix3<f64>, scale: f64) -> usize {
    SymmetricEigen::new(*s)
        .eigenvalues
        .iter()
        .filter(|v| **v > crate::stabilise::FREE * scale)
        .count()
}

/// Takes back, joint by joint, the release of the stiffest released end
/// where the releases leave the joint's rotation held in fewer directions
/// than without them while a member still bends there (a joint where every
/// member became a bar has no rotations); the joints' ids.
fn keep_joints_held(model: &mut Model, sum: &mut Summary) -> Vec<String> {
    let mut kept = Vec::new();
    let mut at: Vec<Vec<(usize, usize)>> = vec![Vec::new(); model.nodes.len()];
    for (k, m) in model.members.iter().enumerate() {
        for (end, n) in [m.i, m.j].into_iter().enumerate() {
            if !m.releases[end].is_none() {
                at[n].push((k, end));
            }
        }
    }
    for (node, ends) in at.iter().enumerate() {
        if ends.is_empty() {
            continue;
        }
        let full = joint_rotation(model, node, false);
        let scale = SymmetricEigen::new(full)
            .eigenvalues
            .iter()
            .fold(0.0_f64, |a, b| a.max(*b));
        let want = rank(&full, scale);
        let mut changed = false;
        loop {
            // No member bends at the joint any more: a bars' joint, without
            // rotations (as a truss's).
            let bending = model
                .members
                .iter()
                .any(|m| (m.i == node || m.j == node) && m.is_frame());
            let now = joint_rotation(model, node, true);
            if !bending || rank(&now, scale) >= want {
                break;
            }
            // The stiffest end still released here.
            let Some(&(k, end)) = ends
                .iter()
                .filter(|(k, e)| !model.members[*k].releases[*e].is_none())
                .max_by(|a, b| {
                    let f = |(k, _): &(usize, usize)| {
                        let m = &model.members[*k];
                        let s = &model.sections[m.section];
                        s.iy.min(s.iz) / model.length(m)
                    };
                    f(a).total_cmp(&f(b))
                })
            else {
                break;
            };
            let r = model.members[k].releases[end];
            if r.both() {
                sum.pinned -= 1;
            } else {
                sum.one_row -= 1;
            }
            model.members[k].releases[end] = EndRelease::default();
            changed = true;
        }
        if changed {
            kept.push(model.nodes[node].id.clone());
        }
    }
    kept
}

/// The local degrees of freedom a member's releases free (none: none),
/// for a member that bends in the solve.
pub(crate) fn released_dofs(m: &Member) -> Option<[bool; 12]> {
    if m.releases.iter().all(|r| r.is_none()) {
        return None;
    }
    let mut out = [false; 12];
    for (end, r) in m.releases.iter().enumerate() {
        out[6 * end + 4] = r.about_y;
        out[6 * end + 5] = r.about_z;
    }
    Some(out)
}

/// A member's elastic stiffness (local, frames; zero otherwise), its axial
/// stiffness EA/L as the solve takes it (its slip share,
/// [`Member::axial_share`]) and the degrees of freedom its releases free.
pub(crate) fn member_elastic(
    m: &Member,
    l: f64,
    s: &Section,
    bends: bool,
) -> (Mat12, f64, Option<[bool; 12]>) {
    let share = m.axial_share;
    let mut ke = if bends {
        frame_elastic(l, s)
    } else {
        Mat12::zeros()
    };
    if bends && share != 1.0 {
        for (a, b) in [(0, 0), (0, 6), (6, 0), (6, 6)] {
            ke[(a, b)] *= share;
        }
    }
    let ea_l = if share == 1.0 {
        s.e * s.area / l
    } else {
        s.e * s.area / l * share
    };
    (ke, ea_l, if bends { released_dofs(m) } else { None })
}

/// The inverse of the released block of `k` (at most four rotations),
/// padded with the identity; none when it does not invert (the member at
/// its own buckling load in the released mode).
fn released_inverse(k: &Mat12, r: &[usize]) -> Option<Matrix4<f64>> {
    let mut krr = Matrix4::identity();
    for (p, &i) in r.iter().enumerate() {
        for (q, &j) in r.iter().enumerate() {
            krr[(p, q)] = k[(i, j)];
        }
    }
    let scale = r.iter().map(|&i| k[(i, i)].abs()).fold(0.0, f64::max);
    let inv = krr.try_inverse()?;
    // A block near singular (a pivot under 10⁻¹² of its diagonal) is not
    // taken.
    (krr.determinant().abs() > 1e-12 * scale.powi(r.len() as i32)).then_some(inv)
}

fn released_list(released: &[bool; 12]) -> Vec<usize> {
    (0..12).filter(|&i| released[i]).collect()
}

/// `k` with its `released` degrees of freedom condensed out (K_kk −
/// K_kr K_rr⁻¹ K_rk; zero rows and columns at the released ones). A block
/// that does not invert: `k` with the released rows and columns set to
/// zero (no coupling through them).
pub(crate) fn condense(k: &Mat12, released: &[bool; 12]) -> Mat12 {
    let r = released_list(released);
    let mut out = *k;
    match released_inverse(k, &r) {
        Some(inv) => {
            for a in 0..12 {
                for b in 0..12 {
                    if released[a] || released[b] {
                        out[(a, b)] = 0.0;
                        continue;
                    }
                    let mut v = k[(a, b)];
                    for (p, &i) in r.iter().enumerate() {
                        for (q, &j) in r.iter().enumerate() {
                            v -= k[(a, i)] * inv[(p, q)] * k[(j, b)];
                        }
                    }
                    out[(a, b)] = v;
                }
            }
        }
        None => {
            for a in 0..12 {
                for b in 0..12 {
                    if released[a] || released[b] {
                        out[(a, b)] = 0.0;
                    }
                }
            }
        }
    }
    out
}

/// A member's consistent loads `f` (local) with its `released` degrees of
/// freedom condensed out alike (f_k − K_kr K_rr⁻¹ f_r; zero at the
/// released ones), `k` its elastic stiffness.
pub(crate) fn condense_load(k: &Mat12, f: &[f64; 12], released: &[bool; 12]) -> [f64; 12] {
    let r = released_list(released);
    let mut out = *f;
    let inv = released_inverse(k, &r);
    for a in 0..12 {
        if released[a] {
            out[a] = 0.0;
            continue;
        }
        if let Some(inv) = &inv {
            for (p, &i) in r.iter().enumerate() {
                for (q, &j) in r.iter().enumerate() {
                    out[a] -= k[(a, i)] * inv[(p, q)] * f[j];
                }
            }
        }
    }
    out
}

/// How a solve is asked for in the slip passes: the model, the start of
/// the tension-only members' set, and whether in first order (else as the
/// run asks).
pub(crate) type SolveFn<'a> =
    dyn Fn(&Model, Option<&[bool]>, bool) -> Result<Solution, FeError> + 'a;

/// One round of secant passes on `work`'s slipping members (indices
/// `slipping`) from solution `s`, each pass a solve (`first_order` or as
/// the run asks), at most `most` passes: the solution and whether the
/// passes settled.
#[allow(clippy::too_many_arguments)]
fn secant_passes(
    work: &mut Model,
    slipping: &[usize],
    mut s: Solution,
    solve: &SolveFn<'_>,
    first_order: bool,
    most: u32,
    passes: &mut u32,
) -> Result<(Solution, bool), FeError> {
    let mut last_change = f64::INFINITY;
    let mut relax: f64 = 1.0;
    for _ in 0..most {
        for &k in slipping {
            let m = &work.members[k];
            let sec = &work.sections[m.section];
            let ea_l = sec.e * sec.area / s.members[k].length;
            let n = s.members[k].axial.abs();
            let target = (n / (n + m.slip * ea_l)).max(SLIP_FLOOR);
            let old = m.axial_share;
            work.members[k].axial_share = old + relax * (target - old);
        }
        let held: Vec<bool> = s.members.iter().map(|f| f.slack || f.bowed).collect();
        let next = solve(work, Some(&held), first_order)?;
        *passes += 1;
        let n_max = next
            .members
            .iter()
            .map(|f| f.axial.abs())
            .fold(0.0, f64::max)
            .max(f64::MIN_POSITIVE);
        let u_max = next
            .displacements
            .iter()
            .flat_map(|d| d[..3].iter())
            .map(|v| v.abs())
            .fold(0.0, f64::max)
            .max(f64::MIN_POSITIVE);
        let dn = next
            .members
            .iter()
            .zip(&s.members)
            .map(|(a, b)| (a.axial - b.axial).abs())
            .fold(0.0, f64::max)
            / n_max;
        let du = next
            .displacements
            .iter()
            .zip(&s.displacements)
            .flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs()))
            .fold(0.0, f64::max)
            / u_max;
        let change = dn.max(du);
        // Passes that swing (the change grows): the shares moved halfway.
        if change > last_change {
            relax = (relax * 0.5).max(0.25);
        }
        last_change = change;
        s = next;
        if change <= SLIP_TOLERANCE {
            return Ok((s, true));
        }
    }
    Ok((s, false))
}

/// The bolt-slip passes: `solve` once as the model is (no slip), then
/// secant passes — each slipping member's axial stiffness EA/L taken as
/// N / (N/(EA/L) + δ) from the last pass's force N (δ its slip; at least
/// [`SLIP_FLOOR`] of it; a share moved halfway while the passes swing) —
/// first in first order (cheap: one factorisation each) until they settle
/// or after [`SLIP_PASSES_FIRST`], then, when the run asks for P-Delta,
/// with it from there until they settle again or after [`SLIP_PASSES`].
/// Settled: no member force changes by more than [`SLIP_TOLERANCE`] of the
/// largest and no joint moves by more than that share of the largest
/// displacement from one pass to the next. The solution says how many
/// solves and whether they settled ([`Solution::slip_passes`],
/// [`Solution::slip_settled`]).
pub(crate) fn solve_slipping(
    model: &Model,
    start: Option<&[bool]>,
    nonlinear: bool,
    solve: &SolveFn<'_>,
) -> Result<Solution, FeError> {
    let slipping: Vec<usize> = (0..model.members.len())
        .filter(|&k| model.members[k].slip > 0.0)
        .collect();
    if slipping.is_empty() {
        // End releases alone: one solve as the run asks.
        return solve(model, start, false);
    }
    let s = solve(model, start, true)?;
    let mut work = model.clone();
    let mut passes = 1;
    let (s, settled) = secant_passes(
        &mut work,
        &slipping,
        s,
        solve,
        true,
        SLIP_PASSES_FIRST,
        &mut passes,
    )?;
    let (mut s, settled) = if nonlinear {
        let s = solve(&work, start, false)?;
        passes += 1;
        secant_passes(
            &mut work,
            &slipping,
            s,
            solve,
            false,
            SLIP_PASSES,
            &mut passes,
        )?
    } else {
        (s, settled)
    };
    s.slip_passes = passes;
    s.slip_settled = settled;
    Ok(s)
}

/// Most rounds of taking releases and slip back (`solve_with_options`).
pub const TAKE_BACK_ROUNDS: usize = 40;

/// Whether any member has an end release or a slip (the options did
/// something).
pub(crate) fn any(model: &Model) -> bool {
    model
        .members
        .iter()
        .any(|m| m.slip > 0.0 || m.releases.iter().any(|r| !r.is_none()))
}

/// Most rings of joints around those an error names that a round takes
/// releases and slip back at (`solve_with_options`).
pub const TAKE_BACK_RINGS: usize = 3;

/// The joints `names` with every joint a member joins them to.
fn neighbours(model: &Model, names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = names.to_vec();
    let nodes: Vec<usize> = names
        .iter()
        .filter_map(|n| model.nodes.iter().position(|x| &x.id == n))
        .collect();
    for m in &model.members {
        for (a, b) in [(m.i, m.j), (m.j, m.i)] {
            if nodes.contains(&a) {
                let id = &model.nodes[b].id;
                if !out.contains(id) {
                    out.push(id.clone());
                }
            }
        }
    }
    out
}

/// The releases and the slip of the members meeting the joints `names`
/// taken back in `model` (both ends of each); whether anything was.
fn take_back(model: &mut Model, names: &[String]) -> bool {
    let nodes: Vec<usize> = names
        .iter()
        .filter_map(|n| model.nodes.iter().position(|x| &x.id == n))
        .collect();
    let mut changed = false;
    for m in &mut model.members {
        if !nodes.contains(&m.i) && !nodes.contains(&m.j) {
            continue;
        }
        for r in &mut m.releases {
            if !r.is_none() {
                *r = EndRelease::default();
                changed = true;
            }
        }
        if m.slip > 0.0 {
            m.slip = 0.0;
            m.axial_share = 1.0;
            changed = true;
        }
    }
    changed
}

/// The releases taken back in `work` of the members that the model without
/// the options (`model` with every release and slip taken out, solved in
/// first order) compresses beyond nine tenths of their elastic buckling
/// load π²EI/L² with their releases (I about the axis released, the least
/// of a pinned bar's); the joints at their ends, none when there is no
/// such member or that solve fails.
fn released_beyond_buckling(
    model: &Model,
    work: &mut Model,
    solve: &SolveFn<'_>,
) -> Option<Vec<String>> {
    let mut plain = model.clone();
    for m in &mut plain.members {
        m.releases = Default::default();
        m.slip = 0.0;
        m.axial_share = 1.0;
    }
    let s = solve(&plain, None, true).ok()?;
    let mut names = Vec::new();
    for (k, m) in work.members.iter_mut().enumerate() {
        let r = m.releases;
        if r.iter().all(|r| r.is_none()) {
            continue;
        }
        let sec = &model.sections[m.section];
        let i = if r.iter().all(|r| r.both()) {
            sec.i_min.min(sec.iy).min(sec.iz)
        } else {
            let y = r.iter().any(|r| r.about_y);
            let z = r.iter().any(|r| r.about_z);
            match (y, z) {
                (true, true) => sec.iy.min(sec.iz),
                (true, false) => sec.iy,
                _ => sec.iz,
            }
        };
        let l = s.members[k].length;
        let euler = std::f64::consts::PI.powi(2) * sec.e * i / (l * l);
        if -s.members[k].axial > 0.9 * euler {
            m.releases = Default::default();
            for n in [m.i, m.j] {
                let id = model.nodes[n].id.clone();
                if !names.contains(&id) {
                    names.push(id);
                }
            }
        }
    }
    (!names.is_empty()).then_some(names)
}

/// A solve with the end releases and the bolt slip (`solve_slipping`):
/// where it finds a mechanism, an instability (a buckling mode) or joints
/// moving implausibly far, the releases and slip of every member meeting
/// the joints the error names are taken back (rigid, not slipping — the
/// model as without the options there; nothing left there: the joints next
/// to them, up to [`TAKE_BACK_RINGS`] rings) and it is solved again — at a
/// first buckling mode, first the releases of every member compressed
/// beyond its buckling load as released (`released_beyond_buckling`) —,
/// at most
/// [`TAKE_BACK_ROUNDS`] times; the joints are in the solution
/// ([`Solution::taken_back`]) and said. An error at joints with nothing to
/// take back stands.
pub(crate) fn solve_with_options(
    model: &Model,
    start: Option<&[bool]>,
    nonlinear: bool,
    solve: &SolveFn<'_>,
) -> Result<Solution, FeError> {
    let mut work: Option<Model> = None;
    let mut taken: Vec<String> = Vec::new();
    let mut round = 0;
    loop {
        let m = work.as_ref().unwrap_or(model);
        let names = match solve_slipping(m, start, nonlinear, solve) {
            Ok(mut s) => {
                s.taken_back = taken;
                return Ok(s);
            }
            Err(e) => match &e {
                FeError::Mechanism { node, joints, .. } => {
                    let mut n = vec![node.clone()];
                    n.extend(joints.iter().cloned());
                    (n, e)
                }
                FeError::Unstable { joints, .. } | FeError::Implausible { joints, .. } => {
                    (joints.clone(), e)
                }
                _ => return Err(e),
            },
        };
        let (names, e) = names;
        let mut next = m.clone();
        // A buckling mode, the first time: the released members that the
        // model without the options compresses beyond their elastic
        // buckling load with their releases (a slender member pinned) are
        // taken back first, all at once — the mode would otherwise move
        // from one such member to the next.
        if round == 0
            && matches!(e, FeError::Unstable { .. })
            && let Some(names) = released_beyond_buckling(model, &mut next, solve)
        {
            round += 1;
            for n in names {
                if !taken.contains(&n) {
                    taken.push(n);
                }
            }
            work = Some(next);
            continue;
        }
        if round >= TAKE_BACK_ROUNDS {
            return Err(e);
        }
        // Nothing left to take back at the joints named: the joints next
        // to them too, ring by ring (at most [`TAKE_BACK_RINGS`]).
        let mut names = names;
        let mut ring = 0;
        while !take_back(&mut next, &names) {
            ring += 1;
            let grown = neighbours(m, &names);
            if ring > TAKE_BACK_RINGS || grown.len() == names.len() {
                return Err(e);
            }
            names = grown;
        }
        round += 1;
        for n in names {
            if !taken.contains(&n) {
                taken.push(n);
            }
        }
        work = Some(next);
    }
}

/// The end releases and bolt slip of a model in words (none: both options
/// off): what was released and what slips. A case's solve:
/// [`case_note`].
pub fn note(model: &Model) -> Option<String> {
    let sum = model.joint_stiffness.as_ref()?;
    let mut parts = Vec::new();
    if sum.releases_on {
        let mut t = format!(
            "end releases from the joints' bolts (option): {} frame end(s) pinned (one bolt), {} \
             free out of the bolted leg's plane (one row), {} held (two rows or both legs), {} \
             with rows or leg not known (not released)",
            sum.pinned, sum.one_row, sum.held, sum.not_known
        );
        if !sum.kept_rigid.is_empty() {
            const NAMED: usize = 5;
            t.push_str(&format!(
                "; kept rigid at {} joint(s) nothing else holds against turning: {}{}",
                sum.kept_rigid.len(),
                sum.kept_rigid[..sum.kept_rigid.len().min(NAMED)].join(", "),
                if sum.kept_rigid.len() > NAMED {
                    ", …"
                } else {
                    ""
                }
            ));
        }
        t.push_str(
            " — the forces shared differently: each member's usage taken as the larger of with \
             and without the option (on the safe side for the members)",
        );
        parts.push(t);
    }
    if sum.slip_on {
        parts.push(format!(
            "bolt slip (option): {} member(s) slip by their ends' hole clearances (at most {:.1} \
             mm), friction not counted — larger deflections, the forces shared differently: each \
             member's usage taken as the larger of with and without the option (on the safe side \
             for the members); {} bolted end(s) without a known clearance (no slip)",
            sum.slipping,
            sum.largest_slip * 1e3,
            sum.no_clearance
        ));
    }
    if sum.running_on > 0 {
        parts.push(format!(
            "{} end(s) without a connection in line with a member of the same section taken as \
             running on through the joint (neither released nor slipping)",
            sum.running_on
        ));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// A case's solve with the end releases or the bolt slip in words (none:
/// nothing to say): the slip passes, settled or not, and the joints where
/// releases and slip were taken back.
pub fn case_note(solution: &Solution) -> Option<String> {
    let mut parts = Vec::new();
    if solution.slip_passes > 0 {
        parts.push(if solution.slip_settled {
            format!("bolt slip: {} solves", solution.slip_passes)
        } else {
            format!(
                "NOT ON THE SAFE SIDE: the bolt-slip passes did not settle in {} solves (the \
                 last kept)",
                solution.slip_passes
            )
        });
    }
    if !solution.taken_back.is_empty() {
        const NAMED: usize = 5;
        let t = &solution.taken_back;
        parts.push(format!(
            "releases and slip taken back at the members meeting {} joint(s) where the solve \
             with them found a mechanism, a buckling mode or implausible displacements (rigid, \
             not slipping there): {}{}",
            t.len(),
            t[..t.len().min(NAMED)].join(", "),
            if t.len() > NAMED { ", …" } else { "" }
        ));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The effect of the end releases and the bolt slip on one case (the
/// "settings explain their effect" rule): with the options against
/// without them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effect {
    /// The largest joint displacement with and without, m.
    pub largest: [f64; 2],
    /// Member forces over 1 kN without the options, and how many of them
    /// changed by more than 10 %.
    pub forces: (usize, usize),
    /// The largest usage and its member, with and without.
    pub worst: [Option<(f64, String)>; 2],
    /// Member usages up and down by more than one point.
    pub up_down: (usize, usize),
    /// The solve times with and without, s.
    pub seconds: [f64; 2],
    /// Members whose usage governs with the options and without them
    /// ([`governs_without`]): each member's usage is the larger of the two
    /// (stage 4.4 follow-up, 2026-11-24). Members with no usage in either:
    /// in neither.
    pub governed: (usize, usize),
}

/// Whether a member's usage `without` the joint options governs over its
/// usage `with` them (stage 4.4 follow-up, 2026-11-24, the owner's
/// question 2 of 2026-11-23): strictly larger ([`crate::check::usage_order`],
/// NaN above any number), or a usage where with them there is none. Equal:
/// the solve with the options governs. Each member's usage is the larger of
/// the two, so neither option can lower a usage — as the foot movements
/// ([`crate::feet::larger_usage`]).
pub fn governs_without(with: Option<f64>, without: Option<f64>) -> bool {
    match (without, with) {
        (Some(a), Some(b)) => crate::check::usage_order(a, b).is_gt(),
        (Some(_), None) => true,
        _ => false,
    }
}

/// A member of a solved case for [`Effect::of`]: its id, axial force (N)
/// and usage.
pub type EffectMember = (String, f64, Option<f64>);

impl Effect {
    /// From two cases' members ([`EffectMember`], in one order) and their
    /// largest displacements (m) and solve times (s).
    pub fn of(
        with: &[EffectMember],
        without: &[EffectMember],
        largest: [f64; 2],
        seconds: [f64; 2],
    ) -> Self {
        let mut e = Effect {
            largest,
            seconds,
            ..Effect::default()
        };
        for (x, y) in with.iter().zip(without) {
            if y.1.abs() > 1e3 {
                e.forces.0 += 1;
                if (x.1 - y.1).abs() > 0.1 * y.1.abs() {
                    e.forces.1 += 1;
                }
            }
            if governs_without(x.2, y.2) {
                e.governed.1 += 1;
            } else if x.2.is_some() {
                e.governed.0 += 1;
            }
            if let (Some(p), Some(q)) = (x.2, y.2) {
                if p > q + 0.01 {
                    e.up_down.0 += 1;
                } else if p < q - 0.01 {
                    e.up_down.1 += 1;
                }
            }
        }
        let worst = |v: &[EffectMember]| {
            v.iter()
                .filter_map(|m| m.2.map(|u| (u, m.0.clone())))
                .max_by(|a, b| a.0.total_cmp(&b.0))
        };
        e.worst = [worst(with), worst(without)];
        e
    }

    /// In words, for the results.
    pub fn words(&self) -> String {
        let [a, b] = self.largest;
        let usage = match &self.worst {
            [Some((p, kp)), Some((q, kq))] => format!(
                "largest usage {:.1} % ({kp}) against {:.1} % ({kq}) without",
                100.0 * p,
                100.0 * q
            ),
            _ => "no usage".into(),
        };
        format!(
            "effect of the joint options: largest displacement {:.1} mm against {:.1} mm \
             without ({:+.0} %); {} of {} member forces over 1 kN changed by more than 10 %; \
             {usage}; {} member usage(s) up and {} down by more than 1 point; solved in {:.2} s \
             against {:.2} s; each member's usage the larger of the two: {} member(s) governed by \
             the solve with the options, {} by the solve without them",
            a * 1e3,
            b * 1e3,
            if b > 0.0 { 100.0 * (a / b - 1.0) } else { 0.0 },
            self.forces.1,
            self.forces.0,
            self.up_down.0,
            self.up_down.1,
            self.seconds[0],
            self.seconds[1],
            self.governed.0,
            self.governed.1
        )
    }
}

/// The effect of the end releases and the bolt slip on one case of
/// `model` ([`Effect`]): each solution with its usage and solve time.
pub fn effect(
    model: &Model,
    with: (&Solution, &[crate::MemberUsage], std::time::Duration),
    without: (&Solution, &[crate::MemberUsage], std::time::Duration),
) -> Effect {
    let largest = |s: &Solution| {
        s.displacements
            .iter()
            .map(|d| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
            .fold(0.0, f64::max)
    };
    let members = |s: &Solution, u: &[crate::MemberUsage]| {
        let mut usage = vec![None; model.members.len()];
        for x in u {
            if let Some(slot) = usage.get_mut(x.member)
                && slot.is_none()
            {
                *slot = x.usage;
            }
        }
        model
            .members
            .iter()
            .zip(&s.members)
            .zip(usage)
            .map(|((m, f), u)| (m.id.clone(), f.axial, u))
            .collect::<Vec<_>>()
    };
    Effect::of(
        &members(with.0, with.1),
        &members(without.0, without.1),
        [largest(with.0), largest(without.0)],
        [with.2.as_secs_f64(), without.2.as_secs_f64()],
    )
}
