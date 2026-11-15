// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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

pub mod asce10;
pub mod asce48;
pub mod bolt_rules;
pub mod capacity;
pub mod check;
pub mod condense;
pub mod design;
pub mod drawn;
mod element;
pub mod links;
pub mod member_ends;
pub mod model;
pub mod offsets;
pub mod plates;
pub mod pole;
pub mod sia263;
pub mod sia265;
mod skyline;
pub mod solve;
pub mod stabilise;
mod units;
pub mod wind;

pub use check::{MemberUsage, member_usage};
pub use links::PointLoad;
pub use model::{
    Analysis, CapacityMethod, JointPoint, JointUse, Member, MemberKind, Model, NodalLoad, Node,
    PluginMethod, Section, TensionCompression,
};
pub use offsets::{EccentricEnd, eccentricity_note};
pub use pole::{Pole, PoleCheck, PoleLoad, check_pole, check_pole_factored};
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

/// Why a solve of `model` failed, in words: the error, and when the
/// model's tension-only members carry no compression (`[analysis]
/// tension_compression = "none"`, owner 2026-11-15) and the solve found a
/// part free or unstable, that this is a likely cause — a joint held only
/// by tension-only members in compression is then held by nothing.
pub fn refusal_text(model: &Model, error: &FeError) -> String {
    let text = error.to_string();
    let free = matches!(
        error,
        FeError::Mechanism { .. }
            | FeError::Held { .. }
            | FeError::Implausible { .. }
            | FeError::Unstable { .. }
    );
    if free
        && model.analysis.tension_compression == TensionCompression::None
        && model.members.iter().any(|m| m.kind == MemberKind::Tension)
    {
        format!("{text}; {NO_COMPRESSION_REFUSAL}")
    } else {
        text
    }
}

/// Said after a refusal of a model whose tension-only members carry no
/// compression ([`refusal_text`]).
pub const NO_COMPRESSION_REFUSAL: &str = "the model's tension-only members carry no compression \
     (tension_compression = \"none\"): a joint held only by them when they are shortened is held \
     by nothing";

/// At most this many stabilised directions are named in a note.
const NOTE_NAMES: usize = 12;

/// Said when the model's self-weight is its members' steel alone: no
/// dead-load factor given, none of its own on any member (owner
/// 2026-11-08: bolts, gusset plates and galvanising are then not in the
/// vertical loads — not on the safe side).
pub fn dead_load_note(model: &Model) -> Option<String> {
    let own = model.members.iter().any(|m| m.dead_load_factor.is_some());
    (model.analysis.self_weight
        && !model.analysis.dead_load_given
        && !own
        && !model.members.is_empty())
    .then(|| {
        "NOT ON THE SAFE SIDE: no dead-load factor given — the self-weight is the members' \
             steel only (bolts, gusset plates, galvanising left out); give [analysis] \
             dead_load_factor"
            .to_owned()
    })
}

/// A hold to the ground taking more than this force (N; N m for a
/// rotation) is said not on the safe side: below it, rounding.
pub const GROUND_SAID: f64 = 1.0;

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
        .map(|h| format!("{} {}", model.node_name(h.at.node), h.at.dof_name()))
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
            model.node_name(h.at.node)
        ));
        if total > 0.0 {
            text.push_str(&format!(
                " ({:.2} % of the reactions)",
                100.0 * h.force.abs() / total
            ));
        }
    }
    // Held to the ground: their force goes to the ground, not through the
    // members (owner 2026-11-08: said where it applies).
    let ground: Vec<&crate::Stabilised> = held
        .iter()
        .filter(|h| h.at.relative.is_empty() && h.force.abs() > GROUND_SAID)
        .collect();
    if let Some(h) = ground
        .iter()
        .max_by(|a, b| a.force.abs().total_cmp(&b.force.abs()))
    {
        text.push_str(&format!(
            "; NOT ON THE SAFE SIDE: {} hold(s) to the ground take load the members do not \
             carry (largest {:.3} {} at {}): add a member to hold those joints",
            ground.len(),
            h.force.abs() / 1000.0,
            if h.at.rotation { "kN m" } else { "kN" },
            model.node_name(h.at.node)
        ));
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
                    model.node_name(h.at.node),
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

/// How the solve converged in words, when it is worth saying (none:
/// converged to its tolerance, joints stabilised as usual): iterations
/// stopped by an exit clause with the forces out of balance by less than
/// [`solve::STALL_ACCEPTED`] of the loads, accepted ([`Solution::stalled`]);
/// joints stabilised at the looser threshold of rounded coordinates after
/// a first solve moved them implausibly far
/// ([`Solution::rounded_stabilisation`]).
pub fn convergence_note(solution: &Solution) -> Option<String> {
    let mut parts = Vec::new();
    if solution.stalled {
        parts.push(format!(
            "iterations stopped after {} with the forces out of balance by {:.1e} of the loads \
             (above the tolerance, below {:.0e}): accepted",
            solution.iterations,
            solution.residual,
            solve::STALL_ACCEPTED
        ));
    }
    if solution.rounded_stabilisation {
        parts.push(
            "joints moved implausibly far (a near-mechanism): directions with less than about \
             0.2° of stiffness held, as for coordinates rounded to four digits — check the \
             stabilised joints"
                .to_owned(),
        );
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// The tension-only members of a solution in words (none: the model has
/// none): how many of them went slack and how many are bowed (at their
/// buckling load, [`MemberForces::bowed`]) and how many solves the
/// active-set iterations took, with the first slack ones named and the
/// first bowed ones with their load: `tension-only members (compression up to their buckling load): 0 of 2
/// slack, 1 bowed (2 solve(s)); bowed (carry their buckling load): BD
/// 0.83 kN`. A model whose tension-only members carry no compression
/// (`[analysis] tension_compression = "none"`) says so:
/// `tension-only members (no compression): 1 of 2 slack (2 solve(s)): BD`.
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
    // The law, the default too (owner 2026-11-15).
    let law = model.analysis.tension_compression.words();
    let mut text = format!(
        "tension-only members ({law}): {} of {} slack{} ({} solve(s))",
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
    let held = held_by_shortened(model, solution);
    if !held.is_empty() {
        let names: Vec<String> = held
            .iter()
            .take(NOTE_NAMES)
            .map(|&n| model.node_name(n))
            .collect();
        text.push_str(&format!(
            "; WARNING: {} joint(s) held by the stabilisation where their shortened tension-only \
             members would hold them (within its limits, not refused — check them): {}{}",
            held.len(),
            names.join(", "),
            if held.len() > NOTE_NAMES { ", …" } else { "" }
        ));
    }
    Some(text)
}

/// A tension-only member stiffens a held direction when its axis has at
/// least this component along it (the sine of 1°).
const ALONG_HELD: f64 = 0.017_452_406;

/// The joints of a solution held by the stabilisation in a direction one
/// of their tension-only members would hold if it were not shortened
/// (slack or bowed: out of the stiffness): its axis at least
/// [`ALONG_HELD`] along a held translation (owner 2026-11-15: such a
/// joint is held only by shortened tension-only members, within the
/// stabilisation's limits — not refused, but no longer quiet). In node
/// order.
pub fn held_by_shortened(model: &Model, solution: &Solution) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for h in solution.stabilised.iter().filter(|h| !h.at.rotation) {
        let n = h.at.node;
        if out.contains(&n) {
            continue;
        }
        let d = h.at.direction;
        let shortened = model.members.iter().enumerate().any(|(k, m)| {
            let f = &solution.members[k];
            if m.kind != MemberKind::Tension || !(f.slack || f.bowed) || (m.i != n && m.j != n) {
                return false;
            }
            let (a, b) = (model.nodes[m.i].point, model.nodes[m.j].point);
            let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let l = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt();
            l > 0.0 && (e[0] * d[0] + e[1] * d[1] + e[2] * d[2]).abs() / l >= ALONG_HELD
        });
        if shortened {
            out.push(n);
        }
    }
    out.sort_unstable();
    out
}

/// A joint moving more than this share of its shortest member's length
/// relative to the mean of its members' far ends is said
/// ([`local_displacement_note`]; owner 2026-11-15).
pub const LOCAL_MOVE_SHARE: f64 = 0.1;

/// The joints that moved far relative to their own members, in words
/// (none: none did): a joint whose displacement less the mean of its
/// members' far ends' displacements is more than [`LOCAL_MOVE_SHARE`] of
/// the shortest of its members, the joint itself having moved at least as
/// far as that mean (the joint that moved, not its neighbours, nor a
/// support next to it) — a local near-mechanism (a flat grillage
/// sagging, a joint held almost only in one plane), which the structure's
/// own limit ([`solve::IMPLAUSIBLE_SHARE`] of its size) does not see. A
/// warning, not a refusal: the results there rest on large displacements
/// of a small-displacement analysis (P-Delta at most) — NOT ON THE SAFE
/// SIDE until those joints are checked. Joints named largest share first.
pub fn local_displacement_note(model: &Model, solution: &Solution) -> Option<String> {
    let mut far: Vec<Vec<usize>> = vec![Vec::new(); model.nodes.len()];
    let mut shortest = vec![f64::INFINITY; model.nodes.len()];
    for m in &model.members {
        let l = model.length(m);
        far[m.i].push(m.j);
        far[m.j].push(m.i);
        shortest[m.i] = shortest[m.i].min(l);
        shortest[m.j] = shortest[m.j].min(l);
    }
    let d = &solution.displacements;
    let mut moved: Vec<(usize, f64, f64)> = Vec::new();
    for (n, ends) in far.iter().enumerate() {
        if ends.is_empty() || shortest[n].is_nan() || shortest[n] <= 0.0 || n >= d.len() {
            continue;
        }
        let mut mean = [0.0; 3];
        for &e in ends {
            for (k, m) in mean.iter_mut().enumerate() {
                *m += d[e][k] / ends.len() as f64;
            }
        }
        let r = (0..3)
            .map(|k| (d[n][k] - mean[k]).powi(2))
            .sum::<f64>()
            .sqrt();
        // The joint that moved, not its neighbours: it moved at least as
        // far as the mean of its members' other ends.
        let own = (0..3).map(|k| d[n][k].powi(2)).sum::<f64>().sqrt();
        let around = (0..3).map(|k| mean[k].powi(2)).sum::<f64>().sqrt();
        if r.is_nan() || (r > LOCAL_MOVE_SHARE * shortest[n] && own >= around) {
            moved.push((n, r, shortest[n]));
        }
    }
    if moved.is_empty() {
        return None;
    }
    moved.sort_by(|a, b| check::usage_order(b.1 / b.2, a.1 / a.2));
    let names: Vec<String> = moved
        .iter()
        .take(NOTE_NAMES)
        .map(|(n, r, l)| {
            format!(
                "{} {:.0} mm ({:.0} % of {:.2} m)",
                model.node_name(*n),
                r * 1000.0,
                100.0 * r / l,
                l
            )
        })
        .collect();
    Some(format!(
        "WARNING, NOT ON THE SAFE SIDE until checked: {} joint(s) moved more than {:.0} % of \
         their shortest member relative to their members' other ends (a local near-mechanism, \
         e.g. a flat grillage sagging; large displacements in a small-displacement analysis): \
         {}{}",
        moved.len(),
        LOCAL_MOVE_SHARE * 100.0,
        names.join(", "),
        if moved.len() > NOTE_NAMES {
            ", …"
        } else {
            ""
        }
    ))
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
mod condense_tests;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod held_limit_tests;
#[cfg(test)]
mod member_ends_tests;
#[cfg(test)]
mod member_tests;
#[cfg(test)]
mod offset_tests;
#[cfg(test)]
mod optional_checks_tests;
#[cfg(test)]
mod pole_extension_tests;
#[cfg(test)]
mod sia263_tests;
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
