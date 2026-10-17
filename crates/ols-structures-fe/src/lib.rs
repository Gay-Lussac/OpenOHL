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
mod units;

pub use check::{MemberUsage, member_usage};
pub use model::{Analysis, Member, MemberKind, MemberPoint, Model, NodalLoad, Node, Section};
pub use pole::{Pole, PoleCheck, PoleLoad, check_pole};
pub use solve::{FeError, Loads, MemberForces, Options, Reaction, Solution, default_budget, solve};

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
        ..Options::default()
    };
    let s = solve(model, loads, &options, budget)?;
    let u = member_usage(model, &s, strength);
    Ok((s, u))
}

#[cfg(test)]
mod pole_extension_tests;
#[cfg(test)]
mod tests;
