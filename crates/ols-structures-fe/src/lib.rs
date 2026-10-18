// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure analysis (Phase 5): 3D finite elements of lattice towers,
//! frames and guyed structures, and the pole check.
//!
//! - [`model`]: structure models (`.olsfe`): nodes in the structure's axes
//!   (x right of the line, y ahead, z up), truss, frame (3D
//!   Euler–Bernoulli) and cable (tension only, pretension) members,
//!   sections and materials, supports and springs; nodes mapped to the
//!   structure type's attachments.
//! - [`solve`](mod@solve): static solution, first order or P-Delta (Newton
//!   iterations on Kₑ + K_g with exit clauses), skyline LDLᵀ solver;
//!   displacements, member end forces, reactions.
//! - [`stabilise`]: joint stabilisation — the joint directions no member
//!   stiffens (coplanar or collinear trusses, a frame chain free to spin),
//!   held when a model would otherwise be a mechanism.
//! - [`check`]: member usage (tension, compression with Euler buckling,
//!   frame interaction N + M).
//! - [`pole`]: pole library (`.olsp`) and the ground-line / taper moment
//!   check with wind on the pole and P-Delta amplification.
//!
//! The theory manual (`docs/theory/structures-fe.md`) gives the
//! formulation and its references.

pub mod check;
mod element;
pub mod model;
pub mod pole;
mod skyline;
pub mod solve;
pub mod stabilise;
mod units;

pub use check::{MemberUsage, member_usage};
pub use model::{Analysis, Member, MemberKind, MemberPoint, Model, NodalLoad, Node, Section};
pub use pole::{Pole, PoleCheck, PoleLoad, check_pole};
pub use solve::{FeError, Loads, MemberForces, Options, Reaction, Solution, default_budget, solve};
pub use stabilise::{Stabilisation, Stabilised};

/// Solves a model under `loads` with its own analysis settings and gives
/// the usage of its members (`strength`: the strength factor).
pub fn analyse(
    model: &Model,
    loads: &Loads,
    strength: f64,
    budget: &ols_core::budget::Budget,
) -> Result<(Solution, Vec<MemberUsage>), FeError> {
    let options = Options {
        nonlinear: model.analysis.nonlinear,
        stabilise: model.analysis.stabilise,
        ..Options::default()
    };
    let s = solve(model, loads, &options, budget)?;
    let u = member_usage(model, &s, strength);
    Ok((s, u))
}

/// At most this many stabilised directions are named in a note.
const NOTE_NAMES: usize = 12;

/// The stabilisation of a solution in words (none: nothing was held):
/// how many joint directions at how many joints, the first of them, and
/// the largest force one of them took — with its share of the support
/// reactions, so a stabilisation that carries load shows.
pub fn stabilisation_note(model: &Model, solution: &Solution) -> Option<String> {
    let held = &solution.stabilised;
    if held.is_empty() {
        return None;
    }
    let mut joints: Vec<usize> = held.iter().map(|h| h.at.node).collect();
    joints.dedup();
    let names: Vec<String> = held
        .iter()
        .take(NOTE_NAMES)
        .map(|h| format!("{} {}", model.nodes[h.at.node].id, h.at.dof_name()))
        .collect();
    let mut text = format!(
        "stabilised {} joint direction(s) without stiffness at {} joint(s): {}{}",
        held.len(),
        joints.len(),
        names.join(", "),
        if held.len() > NOTE_NAMES { ", …" } else { "" }
    );
    let largest = held
        .iter()
        .filter(|h| !h.at.rotation)
        .max_by(|a, b| a.force.abs().total_cmp(&b.force.abs()));
    if let Some(h) = largest {
        let support: [f64; 3] =
            [0, 1, 2].map(|k| solution.reactions.iter().map(|r| r.force[k]).sum());
        let total = support.iter().map(|f| f * f).sum::<f64>().sqrt();
        text.push_str(&format!(
            "; largest force held {:.3} kN at {}",
            h.force.abs() / 1000.0,
            model.nodes[h.at.node].id
        ));
        if total > 0.0 {
            text.push_str(&format!(
                " ({:.2} % of the reactions)",
                100.0 * h.force.abs() / total
            ));
        }
    }
    Some(text)
}

#[cfg(test)]
mod pole_extension_tests;
#[cfg(test)]
mod stabilise_tests;
#[cfg(test)]
mod tests;
