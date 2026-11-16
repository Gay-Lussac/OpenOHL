// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure's full non-linear answer at its attachment points, for
//! calculation level 4 of the line solve (plan of 2026-11-15, phase 6; in
//! active development).
//!
//! In each pass of the line's interface loop a flexible structure is
//! solved under the wires' forces at its attachment points and its own
//! loads ([`FullModel::solve`]): its own settings (P-Delta, stabilisation,
//! tension-only members and guys by their active set), the active set
//! **warm-started** from the last pass's ([`crate::solve::solve_from`]),
//! then
//!
//! - each point's displacement read off the solution as the condensation
//!   reads it (the node's translation, plus its rotation times the offset
//!   where a frame carries the offset's moment:
//!   [`crate::condense::place_displacement`]), and
//! - the **condensed tangent at that solution**
//!   ([`condensed_tangent`]): the tangent stiffness the solver assembles at
//!   the solution — its slack and bowed members out, the geometric
//!   stiffness of its axial forces, its stabilised joints — condensed to
//!   the points exactly as [`crate::condense::condense`] condenses the
//!   tangent at a reference ([`crate::condense::Linearisation`]); the
//!   loop's Newton step takes it.
//!
//! Additive: no solve of before changes (the cold start is
//! [`crate::solve::solve`] itself; a condensation at a linearisation is
//! the same code as before).

use ohl_core::budget::Budget;

use crate::condense::{
    Attachment, Condensed, Link, PoleBody, Rigid, State, condense_state, place_displacement,
};
use crate::model::{JointPoint, Model, NodalLoad};
use crate::pole::{Pole, PoleLoad};
use crate::solve::{Loads, Options, Prescribed, Solution, solve_from};

/// The linearisation in words of a tangent condensed at a case's own
/// solution ([`Condensed::linearisation`]).
pub const AT_SOLUTION: &str = "tangent at the case's own solution: tension-only members and guys \
    as there, P-Delta of its axial forces";

/// `model`'s tangent at `solution` (a solution of it: its slack and bowed
/// members, its axial forces with its own P-Delta setting,
/// [`Solution::nonlinear`], its stabilised joints) condensed to the three
/// translations of each of `points`, as [`crate::condense::condense`]
/// condenses the tangent at a reference — the same mapping of the points,
/// the same refusals (a point unlinked or on no joint, on a joint only
/// bars meet, carried by a joint held to the ground, a tangent that is a
/// mechanism or unstable).
///
/// For a structure that stays linear (first order, no tension-only member
/// switching) it is the unloaded condensation, the same matrices.
///
/// Expected cost: one skyline factorisation of the tangent and a forward
/// and back substitution per attachment direction (as one condensation,
/// without its reference solve).
pub fn condensed_tangent(
    model: &Model,
    points: &[Attachment],
    solution: &Solution,
    budget: &Budget,
) -> Result<Condensed, Rigid> {
    condense_state(model, points, State::At(solution), &[], &[], budget)
}

/// A structure set up for its full solves at its attachment points: its
/// model, its points (linked as the structure check links their loads),
/// its own settings.
#[derive(Debug, Clone, PartialEq)]
pub struct FullModel {
    /// The model (a pole: the pole check's frame model, [`FullModel::of_pole`]).
    pub model: Model,
    /// The attachment points, in the order of the forces given.
    pub points: Vec<Attachment>,
    /// The solve's settings: the model's own P-Delta and stabilisation.
    pub options: Options,
    /// The factor on the members' self-weight in every solve (the model's
    /// own: 1 when it takes it, else 0).
    pub self_weight: f64,
    /// A pole's frame model: its node heights and its height (none: a
    /// structure model).
    pole: Option<(Vec<f64>, f64)>,
}

/// One full solve at the points ([`FullModel::solve`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PointsSolve {
    /// Each point's displacement (as [`FullModel::points`]; structure
    /// axes, m) from the unloaded structure.
    pub displacement: Vec<[f64; 3]>,
    /// The tangent at the solution condensed to the points
    /// ([`condensed_tangent`]; structure axes).
    pub tangent: Condensed,
    /// Per member, left out of the stiffness at the solution (slack or
    /// bowed): the warm start of the next solve.
    pub inactive: Vec<bool>,
    /// Solves of the active-set iterations ([`Solution::passes`]).
    pub passes: u32,
    /// Iterations of the last solve.
    pub iterations: u64,
    /// The solution is approximate (tension-only members kept active as
    /// the last resort, or stopped just above the tolerance).
    pub approximate: bool,
    /// The tension-only members and guys that are slack or bowed.
    pub out: usize,
}

impl FullModel {
    /// A structure model with its points.
    pub fn of_model(model: Model, points: Vec<Attachment>) -> Self {
        let options = Options {
            nonlinear: model.analysis.nonlinear,
            stabilise: model.analysis.stabilise,
            ..Options::default()
        };
        let self_weight = if model.analysis.self_weight { 1.0 } else { 0.0 };
        Self {
            model,
            points,
            options,
            self_weight,
            pole: None,
        }
    }

    /// A pole with its points (label, the type's point in the pole's axes,
    /// m), as [`crate::condense::condense_pole`] models it: the pole
    /// check's frame model above the ground line (fixed there; P-Delta
    /// and its shaft's own weight), a node at each point's height and at
    /// each of `heights` (where loads on the shaft go: its ice), the
    /// points' offsets from the axis rigid links. Refused without an
    /// elasticity.
    pub fn of_pole(
        pole: &Pole,
        points: &[(String, [f64; 3])],
        heights: impl IntoIterator<Item = f64>,
    ) -> Result<Self, Rigid> {
        let e = pole.elasticity.ok_or(Rigid::PoleWithoutElasticity)?;
        if points.is_empty() {
            return Err(Rigid::NoPoints);
        }
        let top = pole.height();
        let (mut model, z) =
            crate::pole::frame_model(pole, e, points.iter().map(|p| p.1[2]).chain(heights));
        for (label, at) in points {
            model.joint_points.push(JointPoint {
                label: label.clone(),
                node: crate::pole::node_at(&z, top, at[2]),
                offset: [at[0], at[1], 0.0],
            });
        }
        let attachments = points.iter().map(|p| Attachment::of_type(&p.0)).collect();
        let mut full = Self::of_model(model, attachments);
        full.pole = Some((z, top));
        Ok(full)
    }

    /// A pole's own loads of a case on its frame model (the wind on its
    /// shaft lumped on the nodes, the loads on it as the pole check
    /// places them), as [`crate::condense::condense_pole_with`] takes
    /// them; none for a structure model.
    pub fn pole_body(&self, pole: &Pole, body: &PoleBody) -> Vec<NodalLoad> {
        match &self.pole {
            Some((z, _)) => crate::condense::pole_body_loads(pole, z, body),
            None => Vec::new(),
        }
    }

    /// Loads at a pole's points given by position (its frame model's
    /// nodes at their heights, the offsets' moments); none for a
    /// structure model.
    pub fn pole_loads(&self, loads: &[PoleLoad]) -> Vec<NodalLoad> {
        match &self.pole {
            Some((z, top)) => crate::pole::axis_loads(z, *top, loads),
            None => Vec::new(),
        }
    }

    /// A pole's base moved by `base` (translations m, rotations rad,
    /// structure axes: its measured foot movement) as prescribed on its
    /// frame model's ground node, as [`crate::condense::condense_pole_full`]
    /// takes it; none for a structure model or a zero movement.
    pub fn pole_feet(&self, base: [f64; 6]) -> Vec<Prescribed> {
        if self.pole.is_none() || base == [0.0; 6] {
            return Vec::new();
        }
        vec![Prescribed {
            node: 0,
            displacement: base,
        }]
    }

    /// [`FullModel::solve_on`] on the structure's feet as drawn.
    ///
    /// Expected cost: as [`FullModel::solve_on`].
    pub fn solve(
        &self,
        forces: &[[f64; 3]],
        extra: &[NodalLoad],
        start: Option<&[bool]>,
        budget: &Budget,
    ) -> Result<PointsSolve, String> {
        self.solve_on(forces, extra, &[], start, budget)
    }

    /// The structure solved under `forces` at its points (as
    /// [`FullModel::points`]; structure axes, N) and the loads `extra` at
    /// its nodes (its own wind and ice of a case), with its self-weight,
    /// its supports moved by `feet` (its measured foot movements,
    /// [`Prescribed`]; empty: as drawn — the solve and the tangent then
    /// with the feet moved), its own settings, the active set of the
    /// tension-only members started from `start` (the last pass's
    /// [`PointsSolve::inactive`];
    /// none: cold, as [`crate::solve::solve`]): each point's displacement
    /// and the tangent at the solution condensed to the points. Err: why,
    /// in words (the solve failed — unstable, a mechanism, implausible —,
    /// or its tangent is refused).
    ///
    /// Expected cost: one solve of the structure (first order: one or two
    /// factorisations; P-Delta a few tens; the active-set iterations
    /// fewer from a warm start) and one condensation of its tangent.
    pub fn solve_on(
        &self,
        forces: &[[f64; 3]],
        extra: &[NodalLoad],
        feet: &[Prescribed],
        start: Option<&[bool]>,
        budget: &Budget,
    ) -> Result<PointsSolve, String> {
        if forces.len() != self.points.len() {
            return Err(format!(
                "{} forces for {} points",
                forces.len(),
                self.points.len()
            ));
        }
        let mut nodal: Vec<NodalLoad> = extra.to_vec();
        for (p, f) in self.points.iter().zip(forces) {
            if p.link == Link::Unlinked {
                return Err(Rigid::Unlinked(p.label.clone()).to_string());
            }
            let at = self
                .model
                .attachment_load(&p.label, &p.link, *f)
                .ok_or_else(|| Rigid::NotInModel(p.label.clone()).to_string())?;
            nodal.extend(at.loads);
        }
        let loads = Loads {
            nodal,
            self_weight: self.self_weight,
            prescribed: feet.to_vec(),
            ..Loads::default()
        };
        let s = solve_from(&self.model, &loads, &self.options, budget, start)
            .map_err(|e| format!("did not solve under the loads with the line: {e}"))?;
        let tangent = condensed_tangent(&self.model, &self.points, &s, budget)
            .map_err(|e| format!("its tangent under the loads with the line: {}", at_case(&e)))?;
        let displacement = tangent
            .place_of
            .iter()
            .map(|&p| {
                let place = &tangent.places[p];
                place_displacement(s.displacements[place.node], place.offset)
            })
            .collect();
        let inactive: Vec<bool> = s.members.iter().map(|f| f.slack || f.bowed).collect();
        Ok(PointsSolve {
            displacement,
            out: inactive.iter().filter(|x| **x).count(),
            inactive,
            passes: s.passes,
            iterations: s.iterations,
            approximate: s.stalled || !s.kept_active.is_empty(),
            tangent,
        })
    }
}

/// A refusal of the tangent in a case's words (the condensation's say "at
/// the reference").
fn at_case(e: &Rigid) -> String {
    match e {
        Rigid::Mechanism { joint, dof } => format!("a mechanism: joint {joint} free in {dof}"),
        Rigid::Unstable { modes } => format!("unstable ({modes} buckling mode(s))"),
        other => other.to_string(),
    }
}
