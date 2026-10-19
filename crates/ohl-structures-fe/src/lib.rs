// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure analysis (Phase 5): 3D finite elements of lattice towers,
//! frames and guyed structures, and the pole check.
//!
//! - [`model`]: structure models (`.ohlfe`): nodes in the structure's axes
//!   (x right of the line, y ahead, z up), truss, frame (3D
//!   Euler–Bernoulli), cable (tension only, pretension) and tension-only
//!   truss members, sections and materials, supports and springs; nodes
//!   mapped to the structure type's attachments.
//! - [`solve`](mod@solve): static solution, first order or P-Delta (Newton
//!   iterations on Kₑ + K_g with exit clauses), skyline LDLᵀ solver;
//!   displacements, member end forces, reactions; active-set iterations
//!   for tension-only members (compression up to their own buckling
//!   load).
//! - [`stabilise`]: joint stabilisation — the joint directions no member
//!   stiffens (coplanar or collinear trusses, a frame chain free to spin),
//!   held when a model would otherwise be a mechanism.
//! - [`check`]: member usage (tension, compression with Euler buckling,
//!   frame interaction N + M).
//! - [`wind`]: wind on the members with lattice faces, their solidity
//!   ratios and shielding, the factors given as data.
//! - [`pole`]: pole library (`.ohlp`) and the ground-line / taper moment
//!   check with wind on the pole and P-Delta amplification.
//!
//! The theory manual (`docs/theory/structures-fe.md`) gives the
//! formulation and its references.

pub mod check;
pub mod drawn;
mod element;
pub mod model;
pub mod pole;
mod skyline;
pub mod solve;
pub mod stabilise;
mod units;
pub mod wind;

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
    budget: &ohl_core::budget::Budget,
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
    if !solution.relative_over.is_empty() {
        // Relative holds above the refusal limit of a ground hold:
        // carried by their lines, flagged.
        let mut over: Vec<&crate::Stabilised> = solution
            .relative_over
            .iter()
            .filter_map(|&k| held.get(k))
            .collect();
        over.sort_by(|a, b| b.force.abs().total_cmp(&a.force.abs()));
        let names: Vec<String> = over
            .iter()
            .take(NOTE_NAMES)
            .map(|h| {
                format!(
                    "{} {} {:.2} kN",
                    model.nodes[h.at.node].id,
                    h.at.dof_name(),
                    h.force.abs() / 1000.0
                )
            })
            .collect();
        text.push_str(&format!(
            "; LARGE: {} hold(s) relative to their lines take more than {} % of the loads \
             (carried by the lines' ends, not refused — check those joints): {}{}",
            over.len(),
            crate::solve::HELD_SHARE * 100.0,
            names.join(", "),
            if over.len() > NOTE_NAMES { ", …" } else { "" }
        ));
    }
    Some(text)
}

/// The tension-only members of a solution in words (none: the model has
/// none): how many of them went slack and how many are bowed (at their
/// buckling load, [`MemberForces::bowed`]) and how many solves the
/// active-set iterations took, with the first slack ones named and the
/// first bowed ones with their load: `tension-only members: 0 of 2
/// slack, 1 bowed (2 solve(s)); bowed (carry their buckling load): BD
/// 0.83 kN`.
pub fn tension_only_note(model: &Model, solution: &Solution) -> Option<String> {
    if solution.passes == 0 {
        return None;
    }
    let tension: Vec<usize> = (0..model.members.len())
        .filter(|&k| model.members[k].kind == MemberKind::Tension)
        .collect();
    let slack: Vec<&str> = tension
        .iter()
        .filter(|&&k| solution.members[k].slack)
        .map(|&k| model.members[k].id.as_str())
        .collect();
    let bowed: Vec<String> = tension
        .iter()
        .filter(|&&k| solution.members[k].bowed)
        .map(|&k| {
            format!(
                "{} {:.2} kN",
                model.members[k].id,
                -solution.members[k].axial / 1000.0
            )
        })
        .collect();
    let mut text = format!(
        "tension-only members: {} of {} slack{} ({} solve(s))",
        slack.len(),
        tension.len(),
        if bowed.is_empty() {
            String::new()
        } else {
            format!(", {} bowed", bowed.len())
        },
        solution.passes
    );
    if !slack.is_empty() {
        text.push_str(&format!(
            ": {}{}",
            slack
                .iter()
                .take(NOTE_NAMES)
                .copied()
                .collect::<Vec<_>>()
                .join(", "),
            if slack.len() > NOTE_NAMES {
                ", …"
            } else {
                ""
            }
        ));
    }
    if !bowed.is_empty() {
        text.push_str(&format!(
            "; bowed (carry their buckling load): {}{}",
            bowed
                .iter()
                .take(NOTE_NAMES)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            if bowed.len() > NOTE_NAMES {
                ", …"
            } else {
                ""
            }
        ));
    }
    Some(text)
}

/// The tension-only members kept active because the active set did not
/// settle ([`Solution::kept_active`]) in words (none: it settled, or the
/// model has none): `tension-only members kept active because the set did
/// not settle: AC, BD — approximate (last resort)` ([`solve()`]'s last
/// resort).
pub fn kept_active_note(model: &Model, solution: &Solution) -> Option<String> {
    let kept = &solution.kept_active;
    if kept.is_empty() {
        return None;
    }
    let names: Vec<&str> = kept
        .iter()
        .take(NOTE_NAMES)
        .map(|&k| model.members[k].id.as_str())
        .collect();
    Some(format!(
        "tension-only members kept active because the set did not settle: {}{} — \
         approximate (last resort)",
        names.join(", "),
        if kept.len() > NOTE_NAMES {
            format!(", … ({} in all)", kept.len())
        } else {
            String::new()
        }
    ))
}

#[cfg(test)]
mod bending_tests;
#[cfg(test)]
mod held_limit_tests;
#[cfg(test)]
mod member_tests;
#[cfg(test)]
mod pole_extension_tests;
#[cfg(test)]
mod spring_tests;
#[cfg(test)]
mod stabilise_tests;
#[cfg(test)]
mod tension_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wind_tests;
