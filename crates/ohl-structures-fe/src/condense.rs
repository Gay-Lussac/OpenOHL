// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure's stiffness at its attachment points (calculation levels 3
//! and 4 of the line solve, plan of 2026-11-15, phase 1): the structure's
//! finite-element model **condensed** to the three translations of each
//! attachment point, with every coupling between the points.
//!
//! - **Flexibility** C (m/N): how far each point moves in each direction
//!   under a unit force at any point in any direction — a pull at one
//!   phase moves the others' points too. Found by static condensation:
//!   the model's tangent stiffness K is factorised once (the solver's
//!   skyline LDLᵀ), a unit force is solved at each attachment direction,
//!   and the attachment points' displacements read off: C = Pᵀ K⁻¹ P, P
//!   the unit loads. Exact for a linear structure (the Guyan reduction of
//!   the model to those degrees of freedom; theory: Structure flexibility
//!   in the line solve).
//! - **Stiffness** K_c = C⁻¹ (N/m) when C is positive definite; none when
//!   two points cannot move apart (two points of one rigid link, a point
//!   on a fixed node): the line's iteration then works with C
//!   ([`Condensed::stiffness`]).
//! - **Points** are mapped exactly as the structure check maps their
//!   loads ([`Model::attachment_load`], shared with it): a point on a
//!   node, its offset a rigid link where a member that is always a frame
//!   meets the node (u + θ × r), else the node's translation only (said);
//!   a placed structure's own link (a relink, an offset) over the type's.
//!   Points whose unit loads are the same (several points on one node,
//!   no offset turned) share one **place**: one block of C.
//! - **Linearisation** ([`Linearisation`]): the elastic structure (first
//!   order, every tension-only member active), or its tangent at a
//!   reference solution — the stringing state (owner 2026-11-15) — with
//!   that solution's tension-only members and guys as they are (slack or
//!   bowed ones out of the stiffness, as the solver leaves them out), the
//!   geometric stiffness of its axial forces (P-Delta, when the model
//!   asks for it) and its stabilised joints.
//! - **Refused** ([`Rigid`]): a structure is taken rigid, with the
//!   reason, when a point is unlinked or on no joint, when a point lies
//!   on a joint only bars meet in a line or a plane (no stiffness across
//!   them), when a unit force at a point would be carried by a joint held
//!   to the ground (a direction no member holds: its flexibility would
//!   mean nothing), when the reference solve fails, or when the tangent
//!   is a mechanism or unstable.
//! - **Poles** ([`condense_pole`]): the pole check's frame model of the
//!   pole above the ground line (fixed there), a node at each point's
//!   height, its offset from the axis a rigid link (the crossarm rigid).
//!
//! Nothing here changes a solve: it is read by the line's coupling of
//! structures and wires (in active development, not built yet).

use std::fmt::{self, Debug, Write as _};
use std::hash::{DefaultHasher, Hasher};

use nalgebra::{DMatrix, Matrix3};
use ohl_core::budget::Budget;

use crate::element::{Mat12, axes, axes_between, frame_elastic, transformation};
use crate::links::PointLoad;
use crate::model::{Model, cross};
use crate::pole::{Pole, PoleLoad};
use crate::skyline::{Pivot, Skyline, reverse_cuthill_mckee};
use crate::solve::{DOF_NAMES, HELD_SHARE, Loads, Options, Prepared, Solution, assemble, solve};
use crate::stabilise::Stabilisation;

/// How a placed structure links one of its attachment points (the line's
/// structure point): as the structure type says, to a joint of the type
/// at an offset (its own relink wins over the type's), or unlinked (its
/// loads refused until it is linked again).
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    /// As the type links it ([`Model::point_load`]).
    Type,
    /// To this joint, at this offset from it (structure axes, m;
    /// [`Model::linked_load`]).
    Joint {
        /// The joint's id.
        joint: String,
        /// From the joint to the point, m.
        offset: [f64; 3],
    },
    /// Unlinked.
    Unlinked,
}

/// An attachment point to condense to: its label and its link.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    /// The point's label.
    pub label: String,
    /// How it is linked.
    pub link: Link,
}

impl Attachment {
    /// A point linked as the type says.
    pub fn of_type(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            link: Link::Type,
        }
    }
}

impl Model {
    /// The load of attachment point `label` with `force` (N, structure
    /// axes), linked by `link`: the one mapping of a point to the model,
    /// shared by the structure check (its loads) and the condensation
    /// (its stiffness), so the two never place a point differently. None:
    /// unlinked, or no joint of the model carries it.
    pub fn attachment_load(&self, label: &str, link: &Link, force: [f64; 3]) -> Option<PointLoad> {
        match link {
            Link::Type => self.point_load(label, force),
            Link::Joint { joint, offset } => self.linked_load(label, joint, *offset, force),
            Link::Unlinked => None,
        }
    }
}

/// The state the structure's stiffness is taken at.
#[derive(Debug, Clone, PartialEq)]
pub enum Linearisation {
    /// The elastic structure: first order, every tension-only member and
    /// guy active (the stiffest; no loads).
    Unloaded,
    /// The tangent at the model's solution under these loads (its own
    /// settings: P-Delta, stabilisation): its slack and bowed members out
    /// of the stiffness, the geometric stiffness of its axial forces —
    /// at the stringing state, the structure's self-weight and the wires'
    /// stringing forces (owner 2026-11-15).
    Tangent(Loads),
}

impl Linearisation {
    /// In words, for the results.
    pub fn words(&self) -> &'static str {
        match self {
            Linearisation::Unloaded => {
                "linear elastic structure (first order, every tension-only member active)"
            }
            Linearisation::Tangent(_) => {
                "linear stiffness at the reference state: tension-only members and guys as at \
                 that state, P-Delta of its axial forces"
            }
        }
    }
}

/// Why a structure is taken rigid (calculation levels 3 and 4 list them
/// with the structure: "T5 rigid: no structure model").
#[derive(Debug, Clone, PartialEq)]
pub enum Rigid {
    /// Its type has no computable model (detail level 1 or 2, no
    /// separate model, no pole).
    NoModel,
    /// Its model could not be read.
    ModelNotRead(String),
    /// A pole without an elasticity.
    PoleWithoutElasticity,
    /// No attachment point to condense to.
    NoPoints,
    /// An attachment point unlinked.
    Unlinked(String),
    /// An attachment point no joint of the model carries.
    NotInModel(String),
    /// An attachment point on a joint only bars (trusses, cables,
    /// tension-only members) meet, all in a line or a plane: no stiffness
    /// across them.
    FreeAcross {
        /// The point.
        label: String,
        /// Its joint.
        joint: String,
    },
    /// A unit force at an attachment point would be carried by a joint
    /// held to the ground (stabilised in a direction no member holds).
    HeldToGround {
        /// The point.
        label: String,
        /// The held joint.
        joint: String,
        /// Its direction in words.
        dof: String,
        /// The share of the unit force it takes.
        share: f64,
    },
    /// The reference solve failed.
    Reference(String),
    /// The tangent is a mechanism.
    Mechanism {
        /// The joint free to move.
        joint: String,
        /// Its direction.
        dof: &'static str,
    },
    /// The tangent is not positive definite (compression above a buckling
    /// load at the reference state).
    Unstable {
        /// Negative pivots.
        modes: usize,
    },
    /// An exit clause stopped the unit solves.
    Stopped(String),
}

impl fmt::Display for Rigid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rigid::NoModel => write!(f, "no structure model (detail level 1 or 2)"),
            Rigid::ModelNotRead(e) => write!(f, "its model could not be read: {e}"),
            Rigid::PoleWithoutElasticity => write!(f, "pole without elasticity"),
            Rigid::NoPoints => write!(f, "no attachment point"),
            Rigid::Unlinked(l) => write!(f, "attachment point {l} not linked to a joint"),
            Rigid::NotInModel(l) => {
                write!(f, "attachment point {l}: no joint of the model carries it")
            }
            Rigid::FreeAcross { label, joint } => write!(
                f,
                "attachment point {label}: only bars meet its joint {joint}, in a line or a \
                 plane — no stiffness across them"
            ),
            Rigid::HeldToGround {
                label,
                joint,
                dof,
                share,
            } => write!(
                f,
                "attachment point {label}: {:.1} % of a force there would be held at joint \
                 {joint} ({dof}), a direction no member holds",
                share * 100.0
            ),
            Rigid::Reference(e) => write!(f, "its model did not solve at the reference: {e}"),
            Rigid::Mechanism { joint, dof } => write!(
                f,
                "its model is a mechanism at the reference: joint {joint} free in {dof}"
            ),
            Rigid::Unstable { modes } => write!(
                f,
                "its model is unstable at the reference ({modes} buckling mode(s))"
            ),
            Rigid::Stopped(e) => write!(f, "condensation stopped: {e}"),
        }
    }
}

impl std::error::Error for Rigid {}

/// Where one or more attachment points are in the model: a node and the
/// offset whose rotation moves them (zero: the node's translation only).
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// Node (index).
    pub node: usize,
    /// From the node to the points, turned with the node, structure axes,
    /// m (zero: no offset, or one at a joint no frame member meets).
    pub offset: [f64; 3],
    /// The points at it (labels).
    pub labels: Vec<String>,
}

/// A structure condensed to its attachment points.
#[derive(Debug, Clone, PartialEq)]
pub struct Condensed {
    /// The attachment points in the order asked.
    pub labels: Vec<String>,
    /// Per point (as `labels`), its place.
    pub place_of: Vec<usize>,
    /// The places, three rows and columns of the matrices each.
    pub places: Vec<Place>,
    /// Flexibility, m/N: 3 × places square, symmetric; row 3p + i, column
    /// 3q + j: place p's displacement along axis i under a unit force at
    /// place q along axis j. Structure axes (x right of the line, y ahead,
    /// z up) unless turned ([`Condensed::turned`]).
    pub flexibility: DMatrix<f64>,
    /// Stiffness K_c = C⁻¹, N/m, when the flexibility is positive
    /// definite (none: two places cannot move apart — two points of one
    /// rigid link, a point on a fixed node; use the flexibility).
    pub stiffness: Option<DMatrix<f64>>,
    /// Approximate: the reference solution was (tension-only members kept
    /// active as a last resort, or iterations stopped just above the
    /// tolerance); said in `notes`.
    pub approximate: bool,
    /// What it leaves out or assumes, in words.
    pub notes: Vec<String>,
    /// The linearisation, in words ([`Linearisation::words`]).
    pub linearisation: &'static str,
    /// Per extra load set given ([`condense_with`]; the structure's own
    /// wind and ice of a case, plan §4.6), each place's displacement
    /// under it, m, in the matrices' axes: the same tangent, one more
    /// substitution each. Empty: none asked.
    pub extra: Vec<Vec<[f64; 3]>>,
}

impl Condensed {
    /// The 3 × 3 block of the flexibility between points `a` and `b`
    /// (labels): a's displacement under a unit force at b, m/N.
    pub fn flexibility_between(&self, a: &str, b: &str) -> Option<Matrix3<f64>> {
        let p = self.place_of[self.labels.iter().position(|l| l == a)?];
        let q = self.place_of[self.labels.iter().position(|l| l == b)?];
        Some(
            self.flexibility
                .fixed_view::<3, 3>(3 * p, 3 * q)
                .into_owned(),
        )
    }

    /// The displacement of each point (as `labels`) under `forces` at the
    /// points (label, N; in the matrices' axes), m: u = C F. Forces at an
    /// unknown label are left out.
    pub fn deflection(&self, forces: &[(&str, [f64; 3])]) -> Vec<[f64; 3]> {
        let mut f = nalgebra::DVector::<f64>::zeros(3 * self.places.len());
        for (label, force) in forces {
            if let Some(k) = self.labels.iter().position(|l| l == label) {
                let p = self.place_of[k];
                for c in 0..3 {
                    f[3 * p + c] += force[c];
                }
            }
        }
        let u = &self.flexibility * f;
        self.place_of
            .iter()
            .map(|&p| [u[3 * p], u[3 * p + 1], u[3 * p + 2]])
            .collect()
    }

    /// The matrices turned to other axes: a vector v in the matrices'
    /// axes is R v in the new ones (`rotation` R, row by row), and each
    /// 3 × 3 block B becomes R B Rᵀ. For a
    /// placed structure R takes the structure's axes to the line's (its
    /// orientation, `ohl_line::links::from_structure_axes`; the height
    /// adjustment, a translation, changes nothing).
    pub fn turned(&self, rotation: [[f64; 3]; 3]) -> Self {
        let r = Matrix3::from_row_slice(&rotation.concat());
        let turn = |m: &DMatrix<f64>| {
            let n = m.nrows() / 3;
            let mut out = DMatrix::<f64>::zeros(m.nrows(), m.ncols());
            for p in 0..n {
                for q in 0..n {
                    let b = m.fixed_view::<3, 3>(3 * p, 3 * q).into_owned();
                    out.fixed_view_mut::<3, 3>(3 * p, 3 * q)
                        .copy_from(&(r * b * r.transpose()));
                }
            }
            out
        };
        let turn_vector = |v: &[f64; 3]| {
            let w = r * nalgebra::Vector3::from_column_slice(v);
            [w[0], w[1], w[2]]
        };
        Self {
            flexibility: turn(&self.flexibility),
            stiffness: self.stiffness.as_ref().map(turn),
            extra: self
                .extra
                .iter()
                .map(|set| set.iter().map(turn_vector).collect())
                .collect(),
            ..self.clone()
        }
    }

    /// Each point's displacement (as `labels`, m, the matrices' axes)
    /// under extra load set `k` ([`Condensed::extra`]); none when it was
    /// not asked.
    pub fn extra_at_points(&self, k: usize) -> Option<Vec<[f64; 3]>> {
        let set = self.extra.get(k)?;
        Some(self.place_of.iter().map(|&p| set[p]).collect())
    }
}

/// A 128-bit fingerprint of values' `Debug` text (every field, the
/// numbers exactly): the inputs of a condensation, for a memo's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint(u64, u64);

/// Feeds `Debug` text into two independent hashes.
struct Hashes(DefaultHasher, DefaultHasher);

impl fmt::Write for Hashes {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.write(s.as_bytes());
        self.1.write(s.as_bytes());
        Ok(())
    }
}

/// The fingerprint of `parts` (each one's `Debug` text, separated).
pub fn fingerprint(parts: &[&dyn Debug]) -> Fingerprint {
    let mut h = Hashes(DefaultHasher::new(), DefaultHasher::new());
    h.1.write_u8(0xA5);
    for p in parts {
        // Writing into a hash never fails.
        let _ = write!(h, "{p:?}\u{1f}");
    }
    Fingerprint(h.0.finish(), h.1.finish())
}

/// The key of a condensation for a memo (per structure type and
/// extension, with the fingerprint of everything it is computed from:
/// the model or pole, the points and their links, the linearisation —
/// so an edited model is another key). A placed structure's orientation
/// is not in it: the matrices are turned per placed structure
/// ([`Condensed::turned`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CondenseKey {
    /// The structure type's id.
    pub structure_type: String,
    /// The extension it stands on (none: as written).
    pub extension: Option<String>,
    /// The inputs' fingerprint.
    pub inputs: Fingerprint,
}

impl CondenseKey {
    /// The key of [`condense`] of `model` for a type standing on
    /// `extension`.
    pub fn of_model(
        structure_type: &str,
        extension: Option<&str>,
        model: &Model,
        points: &[Attachment],
        linearisation: &Linearisation,
    ) -> Self {
        Self {
            structure_type: structure_type.to_owned(),
            extension: extension.map(str::to_owned),
            inputs: fingerprint(&[&"model", model, &points, linearisation]),
        }
    }

    /// This key with extra load sets ([`condense_with`],
    /// [`condense_pole_with`]): another key unless there are none.
    pub fn with_extra(self, extra: &dyn Debug, empty: bool) -> Self {
        if empty {
            return self;
        }
        Self {
            inputs: fingerprint(&[&self.inputs, &"extra", extra]),
            ..self
        }
    }

    /// The key of [`condense_pole`].
    pub fn of_pole(
        structure_type: &str,
        pole: &Pole,
        points: &[(String, [f64; 3])],
        reference: Option<&[PoleLoad]>,
    ) -> Self {
        Self {
            structure_type: structure_type.to_owned(),
            extension: None,
            inputs: fingerprint(&[&"pole", pole, &points, &reference]),
        }
    }
}

/// The share of a unit force at a point a joint held to the ground may
/// take before the structure is refused as flexible: the solver's limit
/// for a load ([`HELD_SHARE`]).
pub const HELD_LIMIT: f64 = HELD_SHARE;

/// A flexibility whose smallest eigenvalue is below this share of its
/// largest is singular: no stiffness ([`Condensed::stiffness`]).
pub const SINGULAR: f64 = 1e-12;

/// Condenses `model` to the three translations of each of `points`
/// (module docs), its stiffness taken as `linearisation` says.
///
/// Expected cost: one reference solve (none for [`Linearisation::Unloaded`]
/// beyond a first-order one without loads), one skyline factorisation of
/// the tangent (a 1000-member lattice tower: a few milliseconds), and a
/// forward and back substitution per attachment direction (3 per place);
/// measured on a generated 1000-member tower with 20 points: see the
/// theory page. `budget` bounds the reference solve's iterations and,
/// with its time limit and cancel token, the unit solves.
pub fn condense(
    model: &Model,
    points: &[Attachment],
    linearisation: &Linearisation,
    budget: &Budget,
) -> Result<Condensed, Rigid> {
    condense_with(model, points, linearisation, &[], budget)
}

/// [`condense`] with, for each of `extra` (loads at the model's nodes,
/// structure axes, N — the structure's own wind and ice of a case, as
/// the structure check puts them on its joints), each place's
/// displacement under it on the same tangent ([`Condensed::extra`]: the
/// linear answer, P-Delta of the reference's forces included).
///
/// Expected cost: as [`condense`], plus one forward and back
/// substitution per extra load set.
pub fn condense_with(
    model: &Model,
    points: &[Attachment],
    linearisation: &Linearisation,
    extra: &[Vec<crate::model::NodalLoad>],
    budget: &Budget,
) -> Result<Condensed, Rigid> {
    if points.is_empty() {
        return Err(Rigid::NoPoints);
    }
    // Each point's unit loads, mapped as the structure check maps loads.
    let mut patterns: Vec<(usize, [[f64; 3]; 3])> = Vec::with_capacity(points.len());
    let mut notes = Vec::new();
    for p in points {
        if p.link == Link::Unlinked {
            return Err(Rigid::Unlinked(p.label.clone()));
        }
        let mut node = None;
        let mut moments = [[0.0; 3]; 3];
        let mut left_out = false;
        for k in 0..3 {
            let mut e = [0.0; 3];
            e[k] = 1.0;
            let load = model
                .attachment_load(&p.label, &p.link, e)
                .ok_or_else(|| Rigid::NotInModel(p.label.clone()))?;
            let [l] = load.loads.as_slice() else {
                return Err(Rigid::NotInModel(p.label.clone()));
            };
            node = Some(l.node);
            moments[k] = l.moment;
            left_out |= load.moment != [0.0; 3] && !load.moment_applied;
        }
        let node = node.expect("three loads");
        if !model.bar_joint_free(node).is_empty() {
            return Err(Rigid::FreeAcross {
                label: p.label.clone(),
                joint: model.node_name(node),
            });
        }
        if left_out {
            notes.push(format!(
                "attachment point {}: its offset at joint {} (no member that is always a frame \
                 meets it) taken as the joint's translation only — its rotation left out",
                p.label,
                model.node_name(node)
            ));
        }
        patterns.push((node, moments));
    }
    // The places: points with the same unit loads share one.
    let mut places: Vec<Place> = Vec::new();
    let mut place_moments: Vec<[[f64; 3]; 3]> = Vec::new();
    let mut place_of = Vec::with_capacity(points.len());
    for (p, (node, moments)) in points.iter().zip(&patterns) {
        let found = places
            .iter()
            .zip(&place_moments)
            .position(|(q, m)| q.node == *node && m == moments);
        let k = match found {
            Some(k) => k,
            None => {
                // r from r × eₓ = (0, r_z, −r_y) and r × e_y = (−r_z, 0, r_x).
                let offset = [moments[1][2], -moments[0][2], moments[0][1]];
                places.push(Place {
                    node: *node,
                    offset,
                    labels: Vec::new(),
                });
                place_moments.push(*moments);
                places.len() - 1
            }
        };
        places[k].labels.push(p.label.clone());
        place_of.push(k);
    }

    // The state: the reference solution, the tangent's settings.
    let (solution, nonlinear) = reference(model, linearisation, budget)?;
    if solution.stalled || !solution.kept_active.is_empty() {
        notes.push(
            "approximate: the reference solve was accepted with tension-only members kept \
             active, or stopped just above its tolerance"
                .to_owned(),
        );
    }
    if solution.rounded_stabilisation {
        notes.push(
            "the reference solve held its joints at the looser threshold of rounded \
             coordinates"
                .to_owned(),
        );
    }
    let tangent = Tangent::at(model, &solution, nonlinear, false)?;

    // One unit solve per place and direction.
    let run = budget.start();
    let n = places.len();
    let mut columns: Vec<Vec<f64>> = Vec::with_capacity(3 * n);
    for (place, moments) in places.iter().zip(&place_moments) {
        for (k, moment) in moments.iter().enumerate() {
            run.check()
                .map_err(|stop| Rigid::Stopped(stop.to_string()))?;
            let mut x = vec![0.0; tangent.n_eq];
            let mut e = [0.0; 3];
            e[k] = 1.0;
            tangent.put(place.node, e, *moment, &mut x);
            tangent.k.solve(&mut x);
            columns.push(x);
        }
    }
    let mut c = DMatrix::<f64>::zeros(3 * n, 3 * n);
    for (p, (place, moments)) in places.iter().zip(&place_moments).enumerate() {
        for (i, moment) in moments.iter().enumerate() {
            let mut e = [0.0; 3];
            e[i] = 1.0;
            for (col, x) in columns.iter().enumerate() {
                c[(3 * p + i, col)] = tangent.read(place.node, e, *moment, x);
            }
        }
    }
    // Symmetric to round-off (Maxwell–Betti): its mean.
    let c = (&c + c.transpose()) * 0.5;
    // The extra load sets: each place's displacement along each axis.
    let mut extra_moves = Vec::with_capacity(extra.len());
    for loads in extra {
        run.check()
            .map_err(|stop| Rigid::Stopped(stop.to_string()))?;
        let mut x = vec![0.0; tangent.n_eq];
        for l in loads {
            if l.node < tangent.eq.len() {
                tangent.put(l.node, l.force, l.moment, &mut x);
            }
        }
        tangent.k.solve(&mut x);
        extra_moves.push(
            places
                .iter()
                .zip(&place_moments)
                .map(|(place, moments)| {
                    [0, 1, 2].map(|i| {
                        let mut e = [0.0; 3];
                        e[i] = 1.0;
                        tangent.read(place.node, e, moments[i], &x)
                    })
                })
                .collect(),
        );
    }

    // A force at a point carried by a joint held to the ground: refused.
    let ground: Vec<&Stabilisation> = solution
        .stabilised
        .iter()
        .map(|h| &h.at)
        .filter(|h| h.relative.is_empty())
        .collect();
    if !ground.is_empty() {
        let full = Tangent::at(model, &solution, nonlinear, true)?;
        for (col, x) in columns.iter().enumerate() {
            let u = tangent.displacements(x);
            let mut ux = vec![0.0; full.n_eq];
            for (node, d) in u.iter().enumerate() {
                for (k, v) in d.iter().enumerate() {
                    if let Some(q) = full.eq[node][k] {
                        ux[q] = *v;
                    }
                }
            }
            let internal = full.k.mul(&ux);
            // The unit load of this column: at its place's node, force
            // e_k and its offset's moment.
            let (place, k) = (&places[col / 3], col % 3);
            let mut unit = [0.0; 6];
            unit[k] = 1.0;
            unit[3..].copy_from_slice(&place_moments[col / 3][k]);
            for h in &ground {
                let base = if h.rotation { 3 } else { 0 };
                let force: f64 = match h.axis {
                    // A removed direction: what the members put on it,
                    // less the load there (all of it taken by the hold
                    // when only the hold resists it).
                    Some(a) => {
                        let load = if h.node == place.node {
                            unit[base + a]
                        } else {
                            0.0
                        };
                        full.eq[h.node][base + a].map_or(0.0, |q| internal[q]) - load
                    }
                    // A spring: its force.
                    None => {
                        h.stiffness
                            * (0..3)
                                .map(|c| h.direction[c] * u[h.node][base + c])
                                .sum::<f64>()
                    }
                };
                // A moment over the structure's size, as the solver's
                // limit takes it.
                let share = force.abs()
                    / if h.rotation {
                        tangent.size.max(1.0)
                    } else {
                        1.0
                    };
                if share > HELD_LIMIT {
                    return Err(Rigid::HeldToGround {
                        label: place.labels[0].clone(),
                        joint: model.node_name(h.node),
                        dof: h.dof_name(),
                        share,
                    });
                }
            }
        }
    }

    let stiffness = stiffness_of(&c);
    if stiffness.is_none() {
        notes.push(
            "no stiffness matrix: points that cannot move apart (one rigid link, or a fixed \
             joint) — the flexibility only"
                .to_owned(),
        );
    }
    Ok(Condensed {
        labels: points.iter().map(|p| p.label.clone()).collect(),
        place_of,
        places,
        flexibility: c,
        stiffness,
        approximate: solution.stalled || !solution.kept_active.is_empty(),
        notes,
        linearisation: linearisation.words(),
        extra: extra_moves,
    })
}

/// The inverse of the flexibility `c` when it is positive definite and
/// not singular to [`SINGULAR`].
fn stiffness_of(c: &DMatrix<f64>) -> Option<DMatrix<f64>> {
    let eigen = c.clone().symmetric_eigen();
    let (lo, hi) = eigen
        .eigenvalues
        .iter()
        .fold((f64::INFINITY, 0.0f64), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        });
    if !(hi > 0.0 && lo > SINGULAR * hi) {
        return None;
    }
    let k = c.clone().cholesky()?.inverse();
    Some((&k + k.transpose()) * 0.5)
}

/// The reference solution and whether the tangent takes P-Delta.
fn reference(
    model: &Model,
    linearisation: &Linearisation,
    budget: &Budget,
) -> Result<(Solution, bool), Rigid> {
    let (loads, nonlinear) = match linearisation {
        Linearisation::Unloaded => (Loads::default(), false),
        Linearisation::Tangent(loads) => (loads.clone(), model.analysis.nonlinear),
    };
    let options = Options {
        nonlinear,
        stabilise: model.analysis.stabilise,
        ..Options::default()
    };
    let s = solve(model, &loads, &options, budget).map_err(|e| Rigid::Reference(e.to_string()))?;
    Ok((s, nonlinear))
}

/// The tangent stiffness at a solution, factorised (or, `full`, with the
/// directions the stabilisation removed kept as equations, assembled
/// only: to read what the members put on them).
struct Tangent {
    k: Skyline,
    /// Per node, its equations.
    eq: Vec<[Option<usize>; 6]>,
    n_eq: usize,
    /// The structure's size (the diagonal of the box around its joints),
    /// m.
    size: f64,
}

impl Tangent {
    /// The tangent of `model` at `solution` (its slack and bowed members
    /// out, its axial forces with `nonlinear`, its stabilisations), as
    /// the solver assembles it ([`assemble`]). Not `full`: factorised; a
    /// mechanism or a negative pivot refuses the structure.
    fn at(model: &Model, solution: &Solution, nonlinear: bool, full: bool) -> Result<Self, Rigid> {
        let n_nodes = model.nodes.len();
        let out: Vec<bool> = solution
            .members
            .iter()
            .map(|f| f.slack || f.bowed)
            .collect();
        let axial: Vec<f64> = solution.members.iter().map(|f| f.axial).collect();
        let held: Vec<Stabilisation> = solution.stabilised.iter().map(|h| h.at.clone()).collect();
        // As the solver numbers them (crate::solve).
        let bends: Vec<bool> = model
            .members
            .iter()
            .enumerate()
            .map(|(k, m)| m.is_frame() && !out[k])
            .collect();
        let mut has_frame = vec![false; n_nodes];
        let mut connected = vec![false; n_nodes];
        let mut adjacent = vec![Vec::new(); n_nodes];
        for (k, m) in model.members.iter().enumerate() {
            connected[m.i] = true;
            connected[m.j] = true;
            if bends[k] {
                has_frame[m.i] = true;
                has_frame[m.j] = true;
            }
            adjacent[m.i].push(m.j);
            adjacent[m.j].push(m.i);
        }
        for h in held.iter().filter(|h| !h.relative.is_empty()) {
            let nodes: Vec<usize> = std::iter::once(h.node)
                .chain(h.relative.iter().map(|r| r.0))
                .collect();
            for &a in &nodes {
                for &b in &nodes {
                    if a != b && !adjacent[a].contains(&b) {
                        adjacent[a].push(b);
                    }
                }
            }
        }
        let order = reverse_cuthill_mckee(&adjacent);
        let mut removed = vec![[false; 6]; n_nodes];
        if !full {
            for h in &held {
                if let Some(a) = h.axis
                    && h.relative.is_empty()
                {
                    removed[h.node][a + if h.rotation { 3 } else { 0 }] = true;
                }
            }
        }
        let mut eq = vec![[None; 6]; n_nodes];
        let mut dof_of = Vec::new();
        for &n in &order {
            if !connected[n] {
                continue;
            }
            for (d, slot) in eq[n].iter_mut().enumerate() {
                if !model.nodes[n].fixed[d] && !removed[n][d] && (d < 3 || has_frame[n]) {
                    *slot = Some(dof_of.len());
                    dof_of.push((n, d));
                }
            }
        }
        let n_eq = dof_of.len();
        let prepared = prepare(model, &bends, &has_frame, &eq);
        // Profile: each member's equations, and every translation of the
        // joints a relative hold couples (a superset of its terms).
        let mut first: Vec<usize> = (0..n_eq).collect();
        let mut couple = |eqs: &[usize]| {
            if let Some(&low) = eqs.iter().min() {
                for &e in eqs {
                    first[e] = first[e].min(low);
                }
            }
        };
        for p in &prepared {
            let eqs: Vec<usize> = p.eq.iter().flatten().copied().collect();
            couple(&eqs);
        }
        for h in held.iter().filter(|h| !h.relative.is_empty()) {
            let eqs: Vec<usize> = std::iter::once(h.node)
                .chain(h.relative.iter().map(|r| r.0))
                .flat_map(|n| eq[n][..3].iter().flatten().copied().collect::<Vec<_>>())
                .collect();
            couple(&eqs);
        }
        let mut k = Skyline::new(first);
        assemble(
            model,
            &prepared,
            &eq,
            (&axial, &out),
            nonlinear,
            &held,
            &mut k,
        );
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for node in &model.nodes {
            for c in 0..3 {
                lo[c] = lo[c].min(node.point[c]);
                hi[c] = hi[c].max(node.point[c]);
            }
        }
        let size = (0..3).map(|c| (hi[c] - lo[c]).powi(2)).sum::<f64>().sqrt();
        if !full {
            match k.factorize() {
                Ok(0) => {}
                Ok(modes) => return Err(Rigid::Unstable { modes }),
                Err(Pivot::Zero(q)) => {
                    let (n, d) = dof_of[q];
                    return Err(Rigid::Mechanism {
                        joint: model.node_name(n),
                        dof: DOF_NAMES[d],
                    });
                }
            }
        }
        Ok(Self { k, eq, n_eq, size })
    }

    /// Adds the unit load (force `force`, moment `moment`) at `node` to
    /// `x` (the moment where the node has rotations).
    fn put(&self, node: usize, force: [f64; 3], moment: [f64; 3], x: &mut [f64]) {
        for c in 0..3 {
            if let Some(q) = self.eq[node][c] {
                x[q] += force[c];
            }
            if let Some(q) = self.eq[node][3 + c] {
                x[q] += moment[c];
            }
        }
    }

    /// The work of the unit load (force, moment) at `node` on the
    /// displacements `x`: the place's displacement along the force,
    /// u + θ × r.
    fn read(&self, node: usize, force: [f64; 3], moment: [f64; 3], x: &[f64]) -> f64 {
        let mut w = 0.0;
        for c in 0..3 {
            if let Some(q) = self.eq[node][c] {
                w += force[c] * x[q];
            }
            if let Some(q) = self.eq[node][3 + c] {
                w += moment[c] * x[q];
            }
        }
        w
    }

    /// The displacements per node of the equation vector `x`.
    fn displacements(&self, x: &[f64]) -> Vec<[f64; 6]> {
        self.eq
            .iter()
            .map(|e| e.map(|q| q.map_or(0.0, |q| x[q])))
            .collect()
    }
}

/// The members as the solver prepares them for its stiffness
/// ([`Prepared`]; no self-weight: only the stiffness is assembled).
fn prepare(
    model: &Model,
    bends: &[bool],
    has_frame: &[bool],
    eq: &[[Option<usize>; 6]],
) -> Vec<Prepared> {
    model
        .members
        .iter()
        .zip(bends)
        .map(|(m, &bends)| {
            let s = &model.sections[m.section];
            // Rigid end offsets where the joint has rotations (as the
            // solver: left out where only bars meet it).
            let ends = [m.i, m.j].map(|n| has_frame[n]);
            let offsets = [0, 1].map(|e| if ends[e] { m.offsets[e] } else { [0.0; 3] });
            let rigid = (offsets != [[0.0; 3]; 2]).then(|| crate::offsets::rigid(offsets));
            let (l, r) = if rigid.is_some() {
                let p = |n: usize, e: usize| {
                    let q = model.nodes[n].point;
                    [0, 1, 2].map(|c| q[c] + offsets[e][c])
                };
                let (a, b) = (p(m.i, 0), p(m.j, 1));
                (
                    crate::model::norm(crate::model::sub(b, a)),
                    axes_between(a, b, m),
                )
            } else {
                (model.length(m), axes(model, m))
            };
            let mut eqs = [None; 12];
            eqs[..6].copy_from_slice(&eq[m.i]);
            eqs[6..].copy_from_slice(&eq[m.j]);
            Prepared {
                l,
                r,
                t: match &rigid {
                    Some(d) => transformation(&r) * d,
                    None => transformation(&r),
                },
                ke: if bends {
                    frame_elastic(l, s)
                } else {
                    Mat12::zeros()
                },
                ea_l: s.e * s.area / l,
                weight_local: [0.0; 12],
                eq: eqs,
                rigid,
            }
        })
        .collect()
}

/// A pole condensed to its attachment points: the pole check's frame
/// model of the pole ([`crate::pole`], fixed at the ground line), a node
/// at each point's height (m above the ground line, as the pole check
/// places a point: the type's point in its axes), the point's offset
/// from the axis a rigid link — bending both ways and torsion (J = 2I,
/// G = E/2.6, the round section's). `reference`: the loads at the
/// stringing state (the wires' forces at points, unfactored), the
/// tangent taken there with the shaft's own weight and P-Delta; none:
/// the elastic pole, first order.
///
/// Refused ([`Rigid::PoleWithoutElasticity`]) without an elasticity.
/// Said in the notes: fixed at the ground line (no rotation of its
/// embedment), the crossarms rigid, wood's creep left out — all
/// stiffer than the real pole, so smaller deflections: not on the safe
/// side for clearances.
pub fn condense_pole(
    pole: &Pole,
    points: &[(String, [f64; 3])],
    reference: Option<&[PoleLoad]>,
    budget: &Budget,
) -> Result<Condensed, Rigid> {
    condense_pole_with(pole, points, reference, &[], budget)
}

/// The pole's own loads of a case for [`condense_pole_with`]: the wind on
/// its shaft and loads on it (its ice), unfactored.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PoleBody {
    /// The wind pressure on the shaft along where it blows (structure
    /// axes x, y; Pa): q(z) = p · drag · D(z), as the pole check takes it.
    pub wind: [f64; 2],
    /// Loads on the shaft (its ice), as the pole check places them.
    pub loads: Vec<PoleLoad>,
}

/// [`condense_pole`] with, for each of `extra`, each point's
/// displacement under the pole's own loads ([`Condensed::extra`]): the
/// wind on the shaft lumped on the frame model's nodes (each element's
/// q at its middle times its length, half at each end), the loads as
/// the pole check places them.
pub fn condense_pole_with(
    pole: &Pole,
    points: &[(String, [f64; 3])],
    reference: Option<&[PoleLoad]>,
    extra: &[PoleBody],
    budget: &Budget,
) -> Result<Condensed, Rigid> {
    let e = pole.elasticity.ok_or(Rigid::PoleWithoutElasticity)?;
    if points.is_empty() {
        return Err(Rigid::NoPoints);
    }
    let top = pole.height();
    let heights = points
        .iter()
        .map(|p| p.1[2])
        .chain(reference.into_iter().flatten().map(|l| l.point[2]))
        .chain(
            extra
                .iter()
                .flat_map(|b| b.loads.iter().map(|l| l.point[2])),
        );
    let (mut model, z) = crate::pole::frame_model(pole, e, heights);
    for (label, at) in points {
        model.joint_points.push(crate::model::JointPoint {
            label: label.clone(),
            node: crate::pole::node_at(&z, top, at[2]),
            offset: [at[0], at[1], 0.0],
        });
    }
    let linearisation = match reference {
        None => Linearisation::Unloaded,
        Some(loads) => Linearisation::Tangent(Loads {
            nodal: crate::pole::axis_loads(&z, top, loads),
            self_weight: 1.0,
            ..Loads::default()
        }),
    };
    let attachments: Vec<Attachment> = points.iter().map(|p| Attachment::of_type(&p.0)).collect();
    let extra: Vec<Vec<crate::model::NodalLoad>> = extra
        .iter()
        .map(|b| {
            let mut nodal = crate::pole::axis_loads(&z, top, &b.loads);
            if b.wind != [0.0; 2] {
                for k in 0..z.len().saturating_sub(1) {
                    let length = z[k + 1] - z[k];
                    let q = pole.drag * pole.diameter_at(0.5 * (z[k] + z[k + 1])) * length;
                    for node in [k, k + 1] {
                        nodal.push(crate::model::NodalLoad {
                            node,
                            force: [0.5 * q * b.wind[0], 0.5 * q * b.wind[1], 0.0],
                            moment: [0.0; 3],
                        });
                    }
                }
            }
            nodal
        })
        .collect();
    let mut c = condense_with(&model, &attachments, &linearisation, &extra, budget)?;
    c.notes.push(
        "NOT ON THE SAFE SIDE for clearances: the pole fixed at the ground line (its \
         embedment's rotation left out), its crossarms rigid, wood's creep left out — stiffer \
         than the real pole"
            .to_owned(),
    );
    Ok(c)
}

/// The displacement of a place at `offset` from its node (structure
/// axes, m) when the node moves by `node_displacement` (its three
/// translations and three rotations, m and rad): u + θ × r, the rigid
/// link ([`Place::offset`]).
pub fn place_displacement(node_displacement: [f64; 6], offset: [f64; 3]) -> [f64; 3] {
    let d = node_displacement;
    let t = cross([d[3], d[4], d[5]], offset);
    [d[0] + t[0], d[1] + t[1], d[2] + t[2]]
}
