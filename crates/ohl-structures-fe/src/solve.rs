// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Static solution: first order, or second order (P-Delta) by Newton
//! iterations on the tangent K = Kₑ + K_g(N), tension-only cables that go
//! slack, and the active-set iterations of tension-only members
//! ([`MemberKind::Tension`]), which carry compression up to their own
//! buckling load.

use std::fmt;
use std::time::Duration;

use nalgebra::{Matrix3, SMatrix, Vector3};
use ohl_core::budget::{Budget, Stop};

use crate::element::{
    Mat12, axes, axial_block, frame_elastic, frame_geometric, frame_uniform_load, transformation,
};
use crate::model::{MemberKind, Model, NodalLoad};
use crate::skyline::{Pivot, Skyline, reverse_cuthill_mckee};
use crate::stabilise::{Stabilisation, Stabilised};

/// Names of the degrees of freedom of a node.
pub const DOF_NAMES: [&str; 6] = ["x", "y", "z", "rx", "ry", "rz"];

/// At most this many joints of a mechanism are named.
pub const MECHANISM_JOINTS: usize = 12;

/// A stabilisation held to the ground carrying more than this share of
/// the loads (the larger of their resultant and the largest load at a
/// joint) and more than [`HELD_JOINT_SHARE`] of its own joint's load
/// holds a load in a direction no member holds: a mechanism
/// ([`FeError::Held`]). Otherwise its force is reported
/// ([`Solution::stabilised`]) — the out-of-plane part of the self-weight
/// at a joint of a battered tower face, or the weight at the crossing of
/// a horizontal bracing, for example. A hold **relative** to the ends of
/// the joint's lines ([`crate::Stabilisation::relative`]) is never
/// refused (owner's decision, 2026-10-19): its force stays in the
/// structure, carried by the lines' ends; above these limits it is listed
/// in [`Solution::relative_over`] and flagged in the notes
/// ([`crate::stabilisation_note`]).
pub const HELD_SHARE: f64 = 0.01;

/// See [`HELD_SHARE`]: a quarter of the joint's own load (about 14.5°
/// out of the plane of its members).
pub const HELD_JOINT_SHARE: f64 = 0.25;

/// Default exit clauses: 200 iterations, 30 s. A first-order solve takes
/// one or two iterations; P-Delta converges linearly with the ratio of the
/// axial loads to the buckling load (about 30 iterations at one half).
pub fn default_budget() -> Budget {
    Budget::new()
        .max_iterations(200)
        .time_limit(Duration::from_secs(30))
}

/// Loads of one solve.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loads {
    /// Forces and moments at nodes, structure axes.
    pub nodal: Vec<NodalLoad>,
    /// Factor on the members' self-weight (0: none), on top of each
    /// member's dead-load factor ([`Model::dead_load_factor`]).
    pub self_weight: f64,
}

/// Settings of one solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Geometric non-linearity (P-Delta).
    pub nonlinear: bool,
    /// Converged when the out-of-balance forces are below this share of
    /// the loads.
    pub tolerance: f64,
    /// Joint stabilisation ([`crate::stabilise`]): when the stiffness
    /// matrix is singular (or not positive definite), the joint
    /// directions without stiffness of their own are held and the solve
    /// repeated; false: such a model fails as a mechanism.
    pub stabilise: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            nonlinear: true,
            tolerance: 1e-9,
            stabilise: true,
        }
    }
}

/// Why a solve failed.
#[derive(Debug, Clone, PartialEq)]
pub enum FeError {
    /// The model or the loads are not valid.
    Invalid(String),
    /// A node can move freely: no support, member or spring holds it.
    Mechanism {
        /// Node.
        node: String,
        /// Degree of freedom (x, y, z, rx, ry, rz).
        dof: &'static str,
        /// The joints that move with it (the part of the structure that
        /// moves freely, the node included), in the solver's order; at
        /// most [`MECHANISM_JOINTS`].
        joints: Vec<String>,
        /// How many joints move in all.
        moving: usize,
    },
    /// Joints loaded in a direction no member holds: their stabilisation
    /// ([`crate::stabilise`]) would carry more than [`HELD_SHARE`] of the
    /// loads and [`HELD_JOINT_SHARE`] of their own joint's load.
    Held {
        /// The joints whose stabilisation carries load.
        joints: Vec<String>,
        /// The joint whose stabilisation carries most.
        node: String,
        /// Its direction in words.
        dof: String,
        /// The force (a rotation: moment) it carries, N (N·m).
        force: f64,
        /// Its share of the loads.
        share: f64,
    },
    /// Axial compression beyond the elastic buckling load (the tangent
    /// stiffness has negative eigenvalues).
    Unstable {
        /// Iteration.
        iteration: u64,
        /// Negative pivots (buckling modes passed).
        modes: usize,
    },
    /// The tension-only members do not settle: the active-set iterations
    /// came back to a set they had solved before (a cycle), or took
    /// [`TENSION_PASSES`] solves. [`solve`] does not give this error: it
    /// switches the ties that go round member by member and, as a last
    /// resort, keeps such members active and flags the solution
    /// ([`Solution::kept_active`]); the iterations alone do.
    Unsettled {
        /// The members going taut and slack in turn (at most
        /// [`MECHANISM_JOINTS`] named).
        members: Vec<String>,
        /// How many flip in all.
        flipping: usize,
        /// All of them, by index in the model.
        indices: Vec<usize>,
        /// Solves done.
        passes: u32,
        /// A cycle (else: out of passes).
        cycle: bool,
    },
    /// An exit clause stopped the iterations.
    Stopped {
        /// Which limit.
        stop: Stop,
        /// Out-of-balance force when it stopped, share of the loads.
        residual: f64,
    },
}

impl fmt::Display for FeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeError::Invalid(m) => write!(f, "{m}"),
            FeError::Mechanism {
                node,
                dof,
                joints,
                moving,
            } => {
                write!(f, "mechanism: node {node} is free to move in {dof}")?;
                if *moving > 1 {
                    write!(
                        f,
                        ", with {moving} joints: {}{}",
                        joints.join(", "),
                        if *moving > joints.len() { ", …" } else { "" }
                    )?;
                }
                write!(f, " (add a member, support or spring)")
            }
            FeError::Held {
                joints,
                node,
                dof,
                force,
                share,
            } => write!(
                f,
                "mechanism: joint(s) {} loaded in a direction no member holds — its \
                 stabilisation would carry {:.2} kN at {node} ({dof}), {:.1} % of the loads \
                 (add a member, or move the load to a held joint)",
                joints.join(", "),
                force / 1000.0,
                share * 100.0
            ),
            FeError::Unstable { iteration, modes } => write!(
                f,
                "unstable: axial compression above the elastic buckling load \
                 ({modes} buckling mode(s), iteration {iteration})"
            ),
            FeError::Unsettled {
                members,
                flipping,
                passes,
                cycle,
                ..
            } => write!(
                f,
                "tension-only members do not settle: {}{} go taut and slack in turn ({})",
                members.join(", "),
                if *flipping > members.len() {
                    format!(", … ({flipping} in all)")
                } else {
                    String::new()
                },
                if *cycle {
                    format!("back to a set of slack members solved before, after {passes} solves")
                } else {
                    format!("still changing after {passes} solves")
                }
            ),
            FeError::Stopped { stop, residual } => write!(
                f,
                "not converged: {stop}; out-of-balance forces still {:.2e} of the loads",
                residual
            ),
        }
    }
}

impl std::error::Error for FeError {}

/// Forces of one member.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemberForces {
    /// Axial force, N (tension positive).
    pub axial: f64,
    /// End forces on the member, local axes: node i (Fx, Fy, Fz, Mx, My,
    /// Mz), then node j.
    pub end_forces: [f64; 12],
    /// A cable gone slack, or a tension-only member left out as slack
    /// (its forces 0: it has no buckling load, [`buckling_loads`]).
    pub slack: bool,
    /// A tension-only member **bowed**: shortened beyond its own buckling
    /// load, it carries that load ([`buckling_loads`]; `axial` = −N_cr)
    /// and no more — left out of the stiffness, its force a constant pair
    /// pushing its ends apart.
    pub bowed: bool,
}

/// A support reaction: what holds a joint with a support or a spring.
/// Per direction, a fixed one gives the support's force, a sprung one
/// the spring's force −k·u (one both fixed and sprung: the support's,
/// the spring carrying nothing).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reaction {
    /// Node.
    pub node: usize,
    /// Force on the structure, N.
    pub force: [f64; 3],
    /// Moment on the structure, N·m.
    pub moment: [f64; 3],
}

/// The solution of one load set.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    /// Displacements and rotations per node (m, rad).
    pub displacements: Vec<[f64; 6]>,
    /// Forces per member.
    pub members: Vec<MemberForces>,
    /// Reactions at the nodes with supports or springs (the springs'
    /// forces included): they balance the loads.
    pub reactions: Vec<Reaction>,
    /// Iterations.
    pub iterations: u64,
    /// Out-of-balance forces at the end, share of the loads.
    pub residual: f64,
    /// Whether P-Delta was included.
    pub nonlinear: bool,
    /// The joint directions held by the stabilisation, with the force each
    /// took (empty: the model solved as it is).
    pub stabilised: Vec<Stabilised>,
    /// Solves of the active-set iterations of the tension-only members
    /// ([`MemberKind::Tension`]): 0 for a model without any (solved
    /// once, as always); [`Solution::iterations`] are those of the last.
    pub passes: u32,
    /// The tension-only members kept active, as ordinary truss members,
    /// because the active-set iterations did not settle even member by
    /// member (the **last-resort** fallback, [`solve`]): indices, in member
    /// order; empty when the set settled — always so far on the models
    /// tested. Such a solution is **approximate** — a member kept active
    /// may be compressed beyond its buckling load, which it cannot be —
    /// and is flagged so in the notes ([`crate::kept_active_note`]) and
    /// the usage ([`crate::check::KEPT_ACTIVE`]).
    pub kept_active: Vec<usize>,
    /// The holds relative to the ends of their joints' lines
    /// ([`crate::Stabilisation::relative`]) that take more than
    /// [`HELD_SHARE`] of the loads and [`HELD_JOINT_SHARE`] of their
    /// joint's own load — above the limits at which a hold to the ground
    /// is refused: indices into [`Solution::stabilised`]. Not refused
    /// (their force is carried by the lines' ends), flagged in the notes.
    pub relative_over: Vec<usize>,
    /// The pieces of the members the analysis split at attachment points
    /// along them ([`Model::split`]), each with its own forces, in member
    /// order (empty: none split). Such a member's entry in
    /// [`Solution::members`] gives the forces at its own two joints (its
    /// first piece's end i, its last piece's end j) and the largest axial
    /// force of its pieces; its usage is its worst piece's
    /// ([`crate::check::member_usage`]).
    pub pieces: Vec<Piece>,
}

/// A piece of a member split by the analysis ([`Solution::pieces`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    /// The member (index in the model).
    pub member: usize,
    /// Where it starts and ends, fractions of the member's length from
    /// its first node.
    pub from: f64,
    /// See `from`.
    pub to: f64,
    /// Its forces (end i at `from`, end j at `to`).
    pub forces: MemberForces,
}

/// Per member: what does not change between iterations.
struct Prepared {
    l: f64,
    /// Local axes (rows).
    r: Matrix3<f64>,
    t: Mat12,
    /// Elastic stiffness, local (frames).
    ke: Mat12,
    /// EA/L.
    ea_l: f64,
    /// Consistent self-weight loads, local (frames).
    weight_local: [f64; 12],
    /// Equation numbers of the 12 degrees of freedom.
    eq: [Option<usize>; 12],
}

/// Solves `model` under `loads`.
///
/// When the solve fails as a mechanism or as unstable and
/// `options.stabilise`, the joint directions without stiffness of their
/// own ([`crate::stabilise`]) are held and the solve repeated; the
/// solution lists them ([`Solution::stabilised`]). When there are none,
/// the first error is returned; when the repeated solve fails too, its
/// error. A model that solves as it is solves exactly as without
/// stabilisation (the same operations).
///
/// Expected cost: a skyline LDLᵀ factorisation per iteration (a
/// 1000-member lattice tower: a few milliseconds), one or two iterations
/// first order, a few tens with P-Delta; a stabilised model is solved
/// twice. `budget` bounds the iterations of each solve.
///
/// A model with **tension-only members** ([`MemberKind::Tension`]) is
/// solved by active-set iterations ([`TENSION_PASSES`]): a tension-only
/// member is a truss in tension and in compression up to its own elastic
/// buckling load N_cr ([`buckling_loads`]; 0 without a second moment);
/// shortened beyond, it is **bowed**: left out of the stiffness, it
/// carries N_cr (a constant pair of forces; with N_cr = 0 it is slack).
/// Solved, its members beyond their load bowed and its bowed ones that
/// the structure would load less put back, and solved again, until the
/// set does not change — first order, then (P-Delta) with the geometric
/// stiffness from that set on, each solve converged before the set is
/// updated. Each solve is stabilised as above on the model of its active
/// members, so a joint held only by members gone slack or bowed is held
/// in the directions they left free. A model without tension-only
/// members is solved once, as always.
///
/// When the set does **not settle** ([`FeError::Unsettled`]: a cycle, or
/// [`TENSION_PASSES`] solves), the ties (members on one line through a joint) going round are
/// switched member by member from then on and the iterations repeated
/// (a tie through a joint that something else holds — a spring, a load,
/// another member — is not one rope there). Only when single members go
/// round, or after [`TENSION_PASSES_TOTAL`] solves, the last resort:
/// they are kept active as ordinary truss members and the solution is
/// flagged approximate ([`Solution::kept_active`]). The time limit of
/// `budget` also bounds all the solves together. A model that settles
/// solves exactly as before.
pub fn solve(
    model: &Model,
    loads: &Loads,
    options: &Options,
    budget: &Budget,
) -> Result<Solution, FeError> {
    if let Some(split) = model.split() {
        // Attachment points along frame members: solved on the split
        // model, reported per member.
        let solution = solve_whole(&split.model, loads, options, budget)?;
        return Ok(fold(model, &split, solution));
    }
    solve_whole(model, loads, options, budget)
}

/// [`solve`] of a model as it is (not split).
fn solve_whole(
    model: &Model,
    loads: &Loads,
    options: &Options,
    budget: &Budget,
) -> Result<Solution, FeError> {
    if model.members.iter().any(|m| m.kind == MemberKind::Tension) {
        return solve_tension_only(model, loads, options, budget);
    }
    solve_stabilised(model, loads, options, budget, &[], &[])
}

/// The solution of a split model ([`Model::split`]) per member of
/// `model`: a split member's forces at its own joints, its pieces in
/// [`Solution::pieces`].
fn fold(model: &Model, split: &crate::links::Split, mut s: Solution) -> Solution {
    let n = model.members.len();
    let all = s.members.clone();
    s.members.truncate(n);
    for m in 0..n {
        let parts: Vec<&(usize, usize, f64, f64)> =
            split.pieces.iter().filter(|p| p.0 == m).collect();
        let (Some(first), Some(last)) = (parts.first(), parts.last()) else {
            continue;
        };
        let mut f = all[first.1];
        f.end_forces[6..].copy_from_slice(&all[last.1].end_forces[6..]);
        f.axial = parts
            .iter()
            .map(|p| all[p.1].axial)
            .max_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(f.axial);
        f.slack = false;
        f.bowed = false;
        s.members[m] = f;
        s.pieces.extend(parts.iter().map(|p| Piece {
            member: m,
            from: p.2,
            to: p.3,
            forces: all[p.1],
        }));
    }
    s
}

/// Solves `model` with the members `inactive` left out (empty: none),
/// each carrying the compression `caps` (N, per member; empty: none) —
/// bowed tension-only members —, stabilised when it fails as a mechanism
/// or as unstable (see [`solve`]).
pub(crate) fn solve_stabilised(
    model: &Model,
    loads: &Loads,
    options: &Options,
    budget: &Budget,
    inactive: &[bool],
    caps: &[f64],
) -> Result<Solution, FeError> {
    match solve_with(model, loads, options, budget, &[], inactive, caps) {
        Err(e @ (FeError::Mechanism { .. } | FeError::Unstable { .. })) if options.stabilise => {
            let held = crate::stabilise::find(model, inactive);
            if held.is_empty() {
                return Err(e);
            }
            solve_with(model, loads, options, budget, &held, inactive, caps)
        }
        other => other,
    }
}

/// Most solves of one run of the active-set iterations of the
/// tension-only members (first order and P-Delta together).
pub const TENSION_PASSES: u32 = 100;

/// Most solves of the active-set iterations in all, over the runs with
/// ties switched member by member ([`solve`]); beyond, a set that goes
/// round is kept active (the last resort).
pub const TENSION_PASSES_TOTAL: u32 = 1000;

/// A tension-only member goes beyond its buckling load (bowed, or slack)
/// when its force is below −(that load + this share of the largest axial
/// force of the solve), and is put back when it would carry less
/// compression than its load less this share: round-off on a member at
/// its load (or without force) does not switch it.
pub const TENSION_TOLERANCE: f64 = 1e-9;

/// The load a tension-only member carries in compression at most, N: its
/// elastic (Euler) buckling load π²·E·I_min/L_k² from its own section and
/// length, L_k its `lk`, else its `k` (default 1: pinned ends) × its
/// length — as the compression check ([`crate::check::euler_load`]); 0
/// without a second moment (I_min = 0: slack in compression, as a rope).
/// 0 for the other kinds.
pub fn buckling_loads(model: &Model) -> Vec<f64> {
    (0..model.members.len())
        .map(|k| {
            if model.members[k].kind == MemberKind::Tension {
                crate::check::euler_load(model, k).unwrap_or(0.0)
            } else {
                0.0
            }
        })
        .collect()
}

/// Per member, the compression its tie ([`chains`]) carries at most: the
/// least buckling load of the tie's members (the force along a tie is
/// one; its weakest member bows first).
pub(crate) fn tie_caps(ties: &[Vec<usize>], own: &[f64]) -> Vec<f64> {
    let mut caps = vec![0.0; own.len()];
    for tie in ties {
        let least = tie.iter().map(|&k| own[k]).fold(f64::INFINITY, f64::min);
        for &k in tie {
            caps[k] = least;
        }
    }
    caps
}

/// [`solve`] of a model with tension-only members: the active-set
/// iterations ([`settle`]) in runs ([`rounds`]), the time limit of
/// `budget` over all of them.
fn solve_tension_only(
    model: &Model,
    loads: &Loads,
    options: &Options,
    budget: &Budget,
) -> Result<Solution, FeError> {
    let own = buckling_loads(model);
    let run = budget.start();
    let (mut s, passes, kept) = rounds(
        model,
        |m, ties| {
            let caps = tie_caps(ties, &own);
            let mut residual = f64::NAN;
            let (s, passes) = settle(m, options.nonlinear, |nonlinear, inactive| {
                run.check()
                    .map_err(|stop| FeError::Stopped { stop, residual })?;
                let opts = Options {
                    nonlinear,
                    ..*options
                };
                let s = solve_stabilised(m, loads, &opts, budget, inactive, &caps)?;
                residual = s.residual;
                let (next, shortening) = next_active_set(m, &s, inactive, ties, &caps);
                Ok((s, next, shortening))
            })?;
            let over = ties_hiding_compression(&s, ties, &own);
            Ok((s, passes, over))
        },
        |m| solve_stabilised(m, loads, options, budget, &[], &[]),
    )?;
    s.passes = passes;
    s.kept_active = kept;
    Ok(s)
}

/// The runs of the active-set iterations of `model`'s tension-only
/// members. `run(model, ties)` settles the model with its tension-only
/// members switched by `ties` and gives its solution, the solves done and
/// the members of ties that hide compression
/// ([`ties_hiding_compression`]); `once(model)` solves a model left
/// without tension-only members.
///
/// The ties start as [`chains`]; a tie that hides compression in a member,
/// or that goes round ([`FeError::Unsettled`]), is split into its members
/// and the iterations run again. Members going round on their own (or
/// after [`TENSION_PASSES_TOTAL`] solves) are the last resort: kept active
/// as ordinary trusses (made trusses in a copy of the model) and the
/// iterations run again on the others. Each run splits a tie or keeps a
/// member, so the runs end. The solution, the solves in all and the
/// members kept active.
pub(crate) fn rounds<S>(
    model: &Model,
    mut run: impl FnMut(&Model, &[Vec<usize>]) -> Result<(S, u32, Vec<usize>), FeError>,
    mut once: impl FnMut(&Model) -> Result<S, FeError>,
) -> Result<(S, u32, Vec<usize>), FeError> {
    let mut ties = chains(model);
    // The model with the members kept active made trusses (none: as it is).
    let mut kept_model: Option<Model> = None;
    let mut kept: Vec<usize> = Vec::new();
    let mut done = 0;
    loop {
        let m = kept_model.as_ref().unwrap_or(model);
        if !m.members.iter().any(|x| x.kind == MemberKind::Tension) {
            // Every tension-only member kept active: one solve.
            return Ok((once(m)?, done + 1, kept));
        }
        match run(m, &ties) {
            Ok((s, passes, over)) => {
                done += passes;
                // A tie hiding compression in a member: member by member.
                if !over.is_empty() && done < TENSION_PASSES_TOTAL {
                    let before = ties.len();
                    ties = split(ties, &over);
                    if ties.len() > before {
                        continue;
                    }
                }
                return Ok((s, done, kept));
            }
            Err(FeError::Unsettled {
                indices, passes, ..
            }) if !indices.is_empty() => {
                done += passes;
                // The ties going round, switched member by member.
                if done < TENSION_PASSES_TOTAL {
                    let before = ties.len();
                    ties = split(ties, &indices);
                    if ties.len() > before {
                        continue;
                    }
                }
                // Last resort: the members going round kept active.
                let copy = kept_model.get_or_insert_with(|| model.clone());
                for &k in &indices {
                    // Kept active: a truss, or a frame when it bends.
                    let m = &mut copy.members[k];
                    m.kind = if m.bending {
                        MemberKind::Frame
                    } else {
                        MemberKind::Truss
                    };
                    m.bending = false;
                    kept.push(k);
                }
                kept.sort_unstable();
                kept.dedup();
                for t in &mut ties {
                    t.retain(|k| !indices.contains(k));
                }
                ties.retain(|t| !t.is_empty());
            }
            Err(e) => return Err(e),
        }
    }
}

/// `ties` with those of more than one member that hold one of `members`
/// split into ties of one member each.
fn split(ties: Vec<Vec<usize>>, members: &[usize]) -> Vec<Vec<usize>> {
    ties.into_iter()
        .flat_map(|t| {
            if t.len() > 1 && t.iter().any(|k| members.contains(k)) {
                t.into_iter().map(|k| vec![k]).collect()
            } else {
                vec![t]
            }
        })
        .collect()
}

/// A member of an active tie compressed beyond its own buckling load
/// `own` by more than this share of the largest axial force of the solve
/// splits its tie (see [`solve`]).
pub const TIE_SPLIT_TOLERANCE: f64 = 1e-6;

/// The members of active ties (of more than one member) in solution `s`
/// compressed beyond their own buckling load `own` (by more than
/// [`TIE_SPLIT_TOLERANCE`]): a tie is one rope only through a joint that
/// nothing else holds along it; where a load, a spring or another member
/// does, its halves carry different forces and one of them may be
/// compressed while the tie, end to end, is not.
fn ties_hiding_compression(s: &Solution, ties: &[Vec<usize>], own: &[f64]) -> Vec<usize> {
    let largest = s.members.iter().map(|f| f.axial.abs()).fold(0.0, f64::max);
    let tolerance = TIE_SPLIT_TOLERANCE * largest;
    ties.iter()
        .filter(|t| t.len() > 1)
        .flatten()
        .copied()
        .filter(|&k| {
            let f = &s.members[k];
            !f.slack && !f.bowed && f.axial < -own[k] - tolerance
        })
        .collect()
}

/// The active-set iterations: `step(nonlinear, inactive)` solves the
/// model with the members `inactive` left out (slack or bowed) and gives
/// its solution, the set to leave out next and per member how far it is
/// shortened beyond its load (a strain; 0: not); repeated until the set
/// does not change, first order, then (`nonlinear`) with P-Delta from the
/// set found — a compressed tension-only member's geometric softening in
/// the first solves would not be real.
///
/// Members left out together may leave a part free to move that one of
/// them alone would hold — both diagonals of an X panel shortened by its
/// legs under a heavy vertical load, of which one is stretched as soon as
/// the panel sways. A solve that fails as a mechanism (or holding load,
/// or buckling) after members were left out is therefore tried again
/// with only the more shortened half of them left out, down to one; one
/// member whose leaving fails the solve gives that error (the structure
/// needs it beyond its buckling load).
///
/// A set solved before in the same phase (a cycle) or [`TENSION_PASSES`]
/// solves: [`FeError::Unsettled`] ([`solve`] then switches the ties going
/// round member by member, or keeps the members active). The last
/// solution and the solves done.
pub(crate) fn settle<S>(
    model: &Model,
    nonlinear: bool,
    mut step: impl FnMut(bool, &[bool]) -> Result<(S, Vec<bool>, Vec<f64>), FeError>,
) -> Result<(S, u32), FeError> {
    let mut inactive = vec![false; model.members.len()];
    let mut passes = 0;
    let phases: &[bool] = if nonlinear { &[false, true] } else { &[false] };
    for (phase, &nonlinear) in phases.iter().enumerate() {
        let mut seen: Vec<Vec<bool>> = Vec::new();
        // The last set solved and its members' shortening.
        let mut solved: Option<(Vec<bool>, Vec<f64>)> = None;
        loop {
            passes += 1;
            let (s, next, shortening) = match step(nonlinear, &inactive) {
                Ok(x) => x,
                Err(
                    e @ (FeError::Mechanism { .. }
                    | FeError::Held { .. }
                    | FeError::Unstable { .. }),
                ) => {
                    let Some((base, shortening)) = &solved else {
                        return Err(e);
                    };
                    let mut newly: Vec<usize> = (0..inactive.len())
                        .filter(|&k| inactive[k] && !base[k])
                        .collect();
                    if newly.len() <= 1 || passes >= TENSION_PASSES {
                        return Err(e);
                    }
                    // The more compressed half stays out.
                    newly.sort_by(|&a, &b| shortening[b].total_cmp(&shortening[a]));
                    for &k in &newly[newly.len().div_ceil(2)..] {
                        inactive[k] = false;
                    }
                    continue;
                }
                Err(e) => return Err(e),
            };
            if next == inactive {
                if phase + 1 == phases.len() {
                    return Ok((s, passes));
                }
                break;
            }
            solved = Some((inactive.clone(), shortening));
            seen.push(std::mem::replace(&mut inactive, next));
            let cycle = seen.iter().position(|x| *x == inactive);
            if cycle.is_some() || passes >= TENSION_PASSES {
                // The members that change within the cycle (or in all).
                let from = &seen[cycle.unwrap_or(0)..];
                let indices: Vec<usize> = (0..model.members.len())
                    .filter(|&k| from.iter().any(|x| x[k] != inactive[k]))
                    .collect();
                return Err(FeError::Unsettled {
                    members: indices
                        .iter()
                        .take(MECHANISM_JOINTS)
                        .map(|&k| model.members[k].id.clone())
                        .collect(),
                    flipping: indices.len(),
                    indices,
                    passes,
                    cycle: cycle.is_some(),
                });
            }
        }
    }
    unreachable!("the last phase returns")
}

/// The tension-only members to leave out after solution `s` of the model
/// with `inactive` left out (slack or bowed): an active one compressed
/// beyond its tie's buckling load `caps` (and [`TENSION_TOLERANCE`]) goes,
/// a left-out one that would carry less compression than that load (with
/// `caps` 0: be stretched) comes back.
///
/// A left-out member's stretch is that of its ends, except at a joint
/// direction the solve held ([`Solution::stabilised`]): a joint held only
/// by members gone slack has no position of its own there. It is placed
/// where the left-out members at it would be in balance (their elastic
/// energy least: the limit of giving them a vanishing stiffness), so the
/// two halves of a slack diagonal through a held joint stretch alike, by
/// half of the diagonal's own stretch.
///
/// Tension-only members on one line through a joint (`ties`, from
/// [`chains`]) are one tie — a diagonal in two halves through its
/// crossing with the other: it goes or comes back as a whole, by its
/// stretch end to end (the sum of its members' stretches, an active one's
/// N·L/EA) over its flexibility Σ L/EA, a force. A half alone would hang
/// from the crossing joint.
///
/// Also per member its shortening strain beyond the load (its tie's
/// compression less `caps`, over its EA) when its tie is active and
/// compressed beyond it, else 0 ([`settle`]).
pub(crate) fn next_active_set(
    model: &Model,
    s: &Solution,
    inactive: &[bool],
    ties: &[Vec<usize>],
    caps: &[f64],
) -> (Vec<bool>, Vec<f64>) {
    let largest = s.members.iter().map(|f| f.axial.abs()).fold(0.0, f64::max);
    let tolerance = TENSION_TOLERANCE * largest;
    let stretch = slack_stretch(model, s, inactive);
    let mut shortening = vec![0.0; model.members.len()];
    let ea = |k: usize| {
        let sec = &model.sections[model.members[k].section];
        sec.e * sec.area
    };
    let mut next = vec![false; model.members.len()];
    for tie in ties {
        // Its stretch end to end and its flexibility.
        let (mut long, mut flex) = (0.0, 0.0);
        for &k in tie {
            let f = model.length(&model.members[k]) / ea(k);
            flex += f;
            long += if inactive[k] {
                stretch[k]
            } else {
                s.members[k].axial * f
            };
        }
        let force = long / flex;
        // The tie's buckling load (0: none, slack below zero).
        let cap = caps.get(tie[0]).copied().unwrap_or(0.0);
        let out = if tie.iter().all(|&k| inactive[k]) {
            force <= tolerance - cap
        } else {
            force < -tolerance - cap
        };
        for &k in tie {
            next[k] = out;
            if !inactive[k] {
                shortening[k] = ((-force - cap) / ea(k)).max(0.0);
            }
        }
    }
    (next, shortening)
}

/// The tension-only members by tie: those that meet at a joint on one line
/// (directions within 0.06°) are one tie, the others one each; in member
/// order.
pub(crate) fn chains(model: &Model) -> Vec<Vec<usize>> {
    let tension: Vec<usize> = (0..model.members.len())
        .filter(|&k| model.members[k].kind == MemberKind::Tension)
        .collect();
    let unit = |k: usize| {
        let m = &model.members[k];
        let d = crate::model::sub(model.nodes[m.j].point, model.nodes[m.i].point);
        let l = crate::model::norm(d);
        [d[0] / l, d[1] / l, d[2] / l]
    };
    let mut at_node: Vec<Vec<usize>> = vec![Vec::new(); model.nodes.len()];
    for &k in &tension {
        at_node[model.members[k].i].push(k);
        at_node[model.members[k].j].push(k);
    }
    let mut group: Vec<usize> = (0..model.members.len()).collect();
    fn root(group: &mut [usize], mut x: usize) -> usize {
        while group[x] != x {
            group[x] = group[group[x]];
            x = group[x];
        }
        x
    }
    for members in &at_node {
        for (a, &p) in members.iter().enumerate() {
            for &q in &members[a + 1..] {
                if crate::model::norm(crate::model::cross(unit(p), unit(q))) <= 1e-3 {
                    let (x, y) = (root(&mut group, p), root(&mut group, q));
                    group[x.max(y)] = x.min(y);
                }
            }
        }
    }
    let mut ties: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for &k in &tension {
        let r = root(&mut group, k);
        ties.entry(r).or_default().push(k);
    }
    ties.into_values().collect()
}

/// The stretch (m) of each left-out member of `inactive` in solution `s`
/// (0 for the others), its ends' held directions placed where the
/// left-out members are in balance ([`next_active_set`]).
fn slack_stretch(model: &Model, s: &Solution, inactive: &[bool]) -> Vec<f64> {
    let unit = |k: usize| {
        let m = &model.members[k];
        let d = crate::model::sub(model.nodes[m.j].point, model.nodes[m.i].point);
        let l = crate::model::norm(d);
        Vector3::new(d[0] / l, d[1] / l, d[2] / l)
    };
    let ea_l = |k: usize| {
        let m = &model.members[k];
        let sec = &model.sections[m.section];
        sec.e * sec.area / model.length(m)
    };
    let left_out: Vec<usize> = (0..model.members.len()).filter(|&k| inactive[k]).collect();
    // The held translations at the ends of left-out members: the unknowns.
    let mut touched = vec![false; model.nodes.len()];
    for &k in &left_out {
        touched[model.members[k].i] = true;
        touched[model.members[k].j] = true;
    }
    let unknowns: Vec<(usize, Vector3<f64>)> = s
        .stabilised
        .iter()
        .filter(|h| !h.at.rotation && touched[h.at.node])
        .map(|h| (h.at.node, Vector3::from(h.at.direction)))
        .collect();
    let mut by_node: Vec<Vec<usize>> = vec![Vec::new(); model.nodes.len()];
    for (u, (n, _)) in unknowns.iter().enumerate() {
        by_node[*n].push(u);
    }
    // Per left-out member: its stretch with the joints as solved, and its
    // terms in the unknowns (∂stretch/∂a).
    let mut base = vec![0.0; model.members.len()];
    let mut terms: Vec<Vec<(usize, f64)>> = vec![Vec::new(); model.members.len()];
    for &k in &left_out {
        let m = &model.members[k];
        let e = unit(k);
        let d = &s.displacements;
        let du = Vector3::new(
            d[m.j][0] - d[m.i][0],
            d[m.j][1] - d[m.i][1],
            d[m.j][2] - d[m.i][2],
        );
        base[k] = e.dot(&du);
        for &u in &by_node[m.j] {
            terms[k].push((u, e.dot(&unknowns[u].1)));
        }
        for &u in &by_node[m.i] {
            terms[k].push((u, -e.dot(&unknowns[u].1)));
        }
    }
    if !unknowns.is_empty() {
        // Least Σ (EA/L)·stretch²: (Σ k g gᵀ) a = −Σ k c g, by groups of
        // unknowns joined through left-out members.
        let mut group: Vec<usize> = (0..unknowns.len()).collect();
        fn root(group: &mut [usize], mut x: usize) -> usize {
            while group[x] != x {
                group[x] = group[group[x]];
                x = group[x];
            }
            x
        }
        for &k in &left_out {
            for w in terms[k].windows(2) {
                let (a, b) = (root(&mut group, w[0].0), root(&mut group, w[1].0));
                group[a] = b;
            }
        }
        let mut members_of: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for &k in &left_out {
            if let Some(&(u, _)) = terms[k].first() {
                let r = root(&mut group, u);
                members_of.entry(r).or_default().push(k);
            }
        }
        let mut a = vec![0.0; unknowns.len()];
        for (r, members) in members_of {
            let ids: Vec<usize> = (0..unknowns.len())
                .filter(|&u| root(&mut group, u) == r)
                .collect();
            let place = |u: usize| ids.iter().position(|&x| x == u).unwrap_or(0);
            let n = ids.len();
            let mut kk = nalgebra::DMatrix::<f64>::zeros(n, n);
            let mut rhs = nalgebra::DVector::<f64>::zeros(n);
            for &k in &members {
                let w = ea_l(k);
                for &(u, g) in &terms[k] {
                    rhs[place(u)] -= w * base[k] * g;
                    for &(v, h) in &terms[k] {
                        kk[(place(u), place(v))] += w * g * h;
                    }
                }
            }
            // A direction no left-out member reaches stays where it is
            // (a tiny regularisation keeps the matrix definite).
            let big = (0..n).map(|i| kk[(i, i)]).fold(0.0, f64::max);
            if big <= 0.0 {
                continue;
            }
            for i in 0..n {
                kk[(i, i)] += 1e-12 * big;
            }
            if let Some(c) = kk.cholesky() {
                let x = c.solve(&rhs);
                for (i, &u) in ids.iter().enumerate() {
                    a[u] = x[i];
                }
            }
        }
        for &k in &left_out {
            for &(u, g) in &terms[k] {
                base[k] += g * a[u];
            }
        }
    }
    base
}

/// Solves `model` under `loads` with the joint directions `held` and the
/// members `inactive` left out (tension-only members gone slack or bowed;
/// empty: none), each carrying the compression `caps` (N; empty or 0:
/// none, slack) as a constant pair of forces along it.
fn solve_with(
    model: &Model,
    loads: &Loads,
    options: &Options,
    budget: &Budget,
    held: &[Stabilisation],
    inactive: &[bool],
    caps: &[f64],
) -> Result<Solution, FeError> {
    model.check().map_err(FeError::Invalid)?;
    let n_nodes = model.nodes.len();
    // Members with a frame's stiffness in this solve: frames, and the
    // tension-only members that bend while they are in the model.
    let bends: Vec<bool> = model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| m.is_frame() && !inactive.get(k).copied().unwrap_or(false))
        .collect();
    // Rotations exist at nodes with a frame member; nodes without a
    // member have no equations.
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
    // A hold relative to other joints couples them all.
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
    // Degrees of freedom the stabilisation removes.
    let mut removed = vec![[false; 6]; n_nodes];
    for h in held {
        if let Some(a) = h.axis
            && h.relative.is_empty()
        {
            removed[h.node][a + if h.rotation { 3 } else { 0 }] = true;
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

    // Loads: nodal, then the self-weight.
    let mut f_ext = vec![[0.0; 6]; n_nodes];
    for l in &loads.nodal {
        if l.node >= n_nodes {
            return Err(FeError::Invalid("load at a node out of range".into()));
        }
        let node = &model.nodes[l.node];
        if !connected[l.node] {
            return Err(FeError::Invalid(format!(
                "load at node {}, which has no member",
                node.id
            )));
        }
        if !has_frame[l.node] && l.moment.iter().any(|m| *m != 0.0) {
            return Err(FeError::Invalid(format!(
                "moment at node {}, which has no frame member to take it",
                node.id
            )));
        }
        for k in 0..3 {
            f_ext[l.node][k] += l.force[k];
            f_ext[l.node][3 + k] += l.moment[k];
        }
    }
    let prepared: Vec<Prepared> = model
        .members
        .iter()
        .zip(&bends)
        .map(|(m, &bends)| {
            let s = &model.sections[m.section];
            let l = model.length(m);
            let r = axes(model, m);
            let mut eqs = [None; 12];
            eqs[..6].copy_from_slice(&eq[m.i]);
            eqs[6..].copy_from_slice(&eq[m.j]);
            let w = s.weight() * model.dead_load_factor(m) * loads.self_weight;
            let weight_local = if bends {
                frame_uniform_load(l, r * Vector3::new(0.0, 0.0, -w))
            } else {
                [0.0; 12]
            };
            Prepared {
                l,
                r,
                t: transformation(&r),
                ke: if bends {
                    frame_elastic(l, s)
                } else {
                    Mat12::zeros()
                },
                ea_l: s.e * s.area / l,
                weight_local,
                eq: eqs,
            }
        })
        .collect();
    for ((m, p), &bends) in model.members.iter().zip(&prepared).zip(&bends) {
        if bends {
            let g = p.t.transpose() * SMatrix::<f64, 12, 1>::from_column_slice(&p.weight_local);
            for k in 0..6 {
                f_ext[m.i][k] += g[k];
                f_ext[m.j][k] += g[6 + k];
            }
        } else {
            let w = model.sections[m.section].weight()
                * model.dead_load_factor(m)
                * loads.self_weight
                * p.l
                / 2.0;
            f_ext[m.i][2] -= w;
            f_ext[m.j][2] -= w;
        }
    }
    let mut f = vec![0.0; n_eq];
    for (k, (n, d)) in dof_of.iter().enumerate() {
        f[k] = f_ext[*n][*d];
    }

    // Profile.
    let mut first: Vec<usize> = (0..n_eq).collect();
    for p in &prepared {
        let eqs: Vec<usize> = p.eq.iter().flatten().copied().collect();
        if let Some(&low) = eqs.iter().min() {
            for &e in &eqs {
                first[e] = first[e].min(low);
            }
        }
    }
    for h in held {
        let eqs: Vec<usize> = relative_terms(h, &eq).iter().map(|t| t.0).collect();
        if let Some(&low) = eqs.iter().min() {
            for &e in &eqs {
                first[e] = first[e].min(low);
            }
        }
    }
    let mut k = Skyline::new(first);

    // Iterations.
    let mut u = vec![0.0; n_eq];
    let mut axial: Vec<f64> = model.members.iter().map(|m| m.pretension).collect();
    // Slack cables, and the tension-only members left out.
    let mut slack = vec![false; model.members.len()];
    for (s, &out) in slack.iter_mut().zip(inactive) {
        *s = out;
    }
    // The compression each bowed member carries (0: none).
    let mut bowed = vec![0.0; model.members.len()];
    for (k, &c) in caps.iter().enumerate() {
        if slack.get(k).copied().unwrap_or(false) && c > 0.0 {
            bowed[k] = c;
            axial[k] = -c;
        }
    }
    let mut slack_solved: Option<Vec<bool>> = None;
    let scale = f
        .iter()
        .map(|x| x * x)
        .sum::<f64>()
        .sqrt()
        .max(
            model
                .members
                .iter()
                .map(|m| m.pretension)
                .fold(0.0, f64::max),
        )
        .max(1e-9);
    let mut run = budget.start();
    let mut residual;
    loop {
        // Tangent and out-of-balance forces at the current state.
        assemble(
            model,
            &prepared,
            &eq,
            (&axial, &slack),
            options.nonlinear,
            held,
            &mut k,
        );
        let mut r = f.clone();
        constant_forces(model, &prepared, &slack, &bowed, &mut r);
        let ku = k.mul(&u);
        for (a, b) in r.iter_mut().zip(&ku) {
            *a -= b;
        }
        residual = r.iter().map(|x| x * x).sum::<f64>().sqrt() / scale;
        let settled = slack_solved.as_ref() == Some(&slack);
        if settled && residual <= options.tolerance {
            break;
        }
        if let Err(stop) = run.step() {
            return Err(FeError::Stopped { stop, residual });
        }
        let negative = match k.factorize() {
            Ok(negative) => negative,
            Err(Pivot::Zero(q)) => {
                let (n, d) = dof_of[q];
                // The part that moves, from the matrix again.
                assemble(
                    model,
                    &prepared,
                    &eq,
                    (&axial, &slack),
                    options.nonlinear,
                    held,
                    &mut k,
                );
                let (joints, moving) = moving_joints(model, &dof_of, &k.mechanism());
                return Err(FeError::Mechanism {
                    node: model.nodes[n].id.clone(),
                    dof: DOF_NAMES[d],
                    joints,
                    moving,
                });
            }
        };
        if negative > 0 {
            return Err(FeError::Unstable {
                iteration: run.iterations(),
                modes: negative,
            });
        }
        k.solve(&mut r);
        for (a, b) in u.iter_mut().zip(&r) {
            *a += b;
        }
        slack_solved = Some(slack.clone());
        // Axial forces and slack cables from the new displacements.
        for (idx, (m, p)) in model.members.iter().zip(&prepared).enumerate() {
            let d = local_displacements(p, &u);
            let n = m.pretension + p.ea_l * (d[6] - d[0]);
            if m.kind == MemberKind::Cable {
                slack[idx] = n < 0.0;
                axial[idx] = n.max(0.0);
            } else if slack[idx] {
                // A tension-only member left out: slack, no force, or
                // bowed, its buckling load.
                axial[idx] = -bowed[idx];
            } else {
                axial[idx] = n;
            }
        }
        // First order without cables: one solve is exact.
        if !options.nonlinear && !model.members.iter().any(|m| m.kind == MemberKind::Cable) {
            assemble(model, &prepared, &eq, (&axial, &slack), false, held, &mut k);
            let ku = k.mul(&u);
            let mut r = f.clone();
            constant_forces(model, &prepared, &slack, &bowed, &mut r);
            residual = r
                .iter()
                .zip(&ku)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
                .sqrt()
                / scale;
            break;
        }
    }

    // Results.
    let mut displacements = vec![[0.0; 6]; n_nodes];
    for (q, (n, d)) in dof_of.iter().enumerate() {
        displacements[*n][*d] = u[q];
    }
    let mut internal = vec![[0.0; 6]; n_nodes];
    let members: Vec<MemberForces> = model
        .members
        .iter()
        .zip(&prepared)
        .enumerate()
        .map(|(idx, (m, p))| {
            let d = local_displacements(p, &u);
            let n = axial[idx];
            let mut end = [0.0; 12];
            if bends[idx] {
                let mut kt = p.ke;
                if options.nonlinear {
                    kt += frame_geometric(p.l, n);
                }
                let fl = kt * d;
                for c in 0..12 {
                    end[c] = fl[c] - p.weight_local[c];
                }
                // Internal forces on the nodes (without the self-weight
                // loads, which are in the external loads).
                let g = p.t.transpose() * fl;
                for c in 0..6 {
                    internal[m.i][c] += g[c];
                    internal[m.j][c] += g[6 + c];
                }
            } else {
                end[0] = -n;
                end[6] = n;
                let e = p.r.row(0).transpose();
                let mut fi = -e * n;
                if options.nonlinear && (!slack[idx] || n != 0.0) {
                    // The transverse part of a rotated axial force (a
                    // bowed member's too: its force follows its chord).
                    let dg = global_translation(&displacements, m.j)
                        - global_translation(&displacements, m.i);
                    let block = axial_block(&e, 0.0, n / p.l);
                    fi -= block * dg;
                }
                for c in 0..3 {
                    internal[m.i][c] += fi[c];
                    internal[m.j][c] -= fi[c];
                }
            }
            MemberForces {
                axial: n,
                end_forces: end,
                slack: slack[idx] && bowed[idx] == 0.0,
                bowed: slack[idx] && bowed[idx] > 0.0,
            }
        })
        .collect();
    // What each stabilisation holds: the out-of-balance of its joint along
    // its direction (its spring's force, or the reaction of the degree of
    // freedom it removed); held relative to other joints, its spring's
    // force −k·(c·u) — the same, to the solve's residual, but not changed
    // by the share of another relative hold that ends at the joint.
    let mut held_force = vec![[0.0; 6]; n_nodes];
    let stabilised: Vec<Stabilised> = held
        .iter()
        .map(|h| {
            let base = if h.rotation { 3 } else { 0 };
            let force: f64 = if h.relative.is_empty() {
                (0..3)
                    .map(|c| {
                        h.direction[c] * (internal[h.node][base + c] - f_ext[h.node][base + c])
                    })
                    .sum()
            } else {
                -h.stiffness
                    * relative_terms(h, &eq)
                        .iter()
                        .map(|&(q, v)| v * u[q])
                        .sum::<f64>()
            };
            if h.relative.is_empty() {
                for c in 0..3 {
                    held_force[h.node][base + c] += force * h.direction[c];
                }
            } else {
                // Held relative to other joints: the spring takes it to
                // them (on them, the share `w` of it).
                for &(e, w) in &h.relative {
                    for c in 0..3 {
                        internal[e][base + c] += force * w * h.direction[c];
                    }
                }
            }
            Stabilised {
                at: h.clone(),
                force,
            }
        })
        .collect();
    let relative_over = if held.is_empty() {
        Vec::new()
    } else {
        check_held(model, &f_ext, &stabilised)?
    };
    let reactions = model
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.is_support())
        .map(|(n, _)| {
            // What holds the joint: the forces of its members less its
            // loads. A fixed direction takes them as the support's
            // reaction; a sprung one is in balance with its spring,
            // internal + k·u = load, so the same expression is the
            // spring's force −k·u (to the solve's residual). A direction
            // both fixed and sprung does not move: the support takes it
            // all, the spring nothing.
            let mut r = [0.0; 6];
            for d in 0..6 {
                r[d] = internal[n][d] - f_ext[n][d];
            }
            if !held.is_empty() {
                // The supports' share only, without the stabilisation's.
                for d in 0..6 {
                    r[d] -= held_force[n][d];
                }
            }
            Reaction {
                node: n,
                force: [r[0], r[1], r[2]],
                moment: [r[3], r[4], r[5]],
            }
        })
        .collect();
    Ok(Solution {
        displacements,
        members,
        reactions,
        iterations: run.iterations(),
        residual,
        nonlinear: options.nonlinear,
        stabilised,
        passes: 0,
        kept_active: Vec::new(),
        relative_over,
        pieces: Vec::new(),
    })
}

/// Adds to `r` the constant member forces on the nodes: a cable's
/// pretension pulls its ends together; a bowed tension-only member's
/// compression (`bowed`, N) pushes them apart.
fn constant_forces(
    model: &Model,
    prepared: &[Prepared],
    slack: &[bool],
    bowed: &[f64],
    r: &mut [f64],
) {
    for (idx, (m, p)) in model.members.iter().zip(prepared).enumerate() {
        let n = if m.kind == MemberKind::Cable && !slack[idx] {
            m.pretension
        } else if slack[idx] {
            -bowed[idx]
        } else {
            0.0
        };
        if n != 0.0 {
            let e = p.r.row(0).transpose() * n;
            for c in 0..3 {
                if let Some(q) = p.eq[c] {
                    r[q] += e[c];
                }
                if let Some(q) = p.eq[6 + c] {
                    r[q] -= e[c];
                }
            }
        }
    }
}

/// Refuses a stabilisation held to the ground that carries load: one
/// whose force is above [`HELD_SHARE`] of the loads (the larger of their
/// resultant and the largest load at a joint; a moment over the model's
/// size) and above [`HELD_JOINT_SHARE`] of its joint's own load — a load
/// in a direction no member holds, which the structure does not carry.
/// A hold relative to its lines' ends above the same limits is not
/// refused (its force is carried by the structure: owner's decision,
/// 2026-10-19); their indices in `held` are returned.
fn check_held(
    model: &Model,
    f_ext: &[[f64; 6]],
    held: &[Stabilised],
) -> Result<Vec<usize>, FeError> {
    let mut resultant = [0.0; 3];
    let mut largest: f64 = 0.0;
    for f in f_ext {
        for k in 0..3 {
            resultant[k] += f[k];
        }
        largest = largest.max((f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt());
    }
    let loads = largest.max(
        (resultant[0] * resultant[0] + resultant[1] * resultant[1] + resultant[2] * resultant[2])
            .sqrt(),
    );
    if loads <= 0.0 {
        return Ok(Vec::new());
    }
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for n in &model.nodes {
        for k in 0..3 {
            low[k] = low[k].min(n.point[k]);
            high[k] = high[k].max(n.point[k]);
        }
    }
    let size = (0..3)
        .map(|k| (high[k] - low[k]).powi(2))
        .sum::<f64>()
        .sqrt()
        .max(1.0);
    let share = |h: &Stabilised| h.force.abs() / (loads * if h.at.rotation { size } else { 1.0 });
    let own = |h: &Stabilised| {
        let base = if h.at.rotation { 3 } else { 0 };
        let f = &f_ext[h.at.node][base..base + 3];
        (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt()
    };
    let above = |h: &Stabilised| share(h) > HELD_SHARE && h.force.abs() > HELD_JOINT_SHARE * own(h);
    // Relative holds: carried by their lines' ends, flagged only.
    let relative_over: Vec<usize> = (0..held.len())
        .filter(|&k| !held[k].at.relative.is_empty() && above(&held[k]))
        .collect();
    let over: Vec<&Stabilised> = held
        .iter()
        .filter(|h| h.at.relative.is_empty() && above(h))
        .collect();
    let Some(worst) = over
        .iter()
        .copied()
        .max_by(|a, b| share(a).total_cmp(&share(b)))
    else {
        return Ok(relative_over);
    };
    let mut joints: Vec<String> = Vec::new();
    for h in &over {
        let id = &model.nodes[h.at.node].id;
        if !joints.contains(id) {
            joints.push(id.clone());
        }
    }
    Err(FeError::Held {
        joints,
        node: model.nodes[worst.at.node].id.clone(),
        dof: worst.at.dof_name(),
        force: worst.force.abs(),
        share: share(worst),
    })
}

/// The joints that move in the mechanism `v` (a vector of the equations,
/// [`Skyline::mechanism`]): those translating more than 1/1000 of the
/// largest translation (rotating, when nothing translates), in the
/// solver's order, named up to [`MECHANISM_JOINTS`]; and how many move.
fn moving_joints(model: &Model, dof_of: &[(usize, usize)], v: &[f64]) -> (Vec<String>, usize) {
    let mut size = vec![[0.0f64; 2]; model.nodes.len()];
    let mut order = Vec::new();
    for (q, &(n, d)) in dof_of.iter().enumerate() {
        if v[q] == 0.0 {
            continue;
        }
        if size[n] == [0.0; 2] {
            order.push(n);
        }
        size[n][usize::from(d >= 3)] += v[q] * v[q];
    }
    let kind = usize::from(!order.iter().any(|&n| size[n][0] > 0.0));
    let largest = order.iter().map(|&n| size[n][kind]).fold(0.0, f64::max);
    let moving: Vec<usize> = order
        .into_iter()
        .filter(|&n| largest > 0.0 && size[n][kind] > 1e-6 * largest)
        .collect();
    let names = moving
        .iter()
        .take(MECHANISM_JOINTS)
        .map(|&n| model.nodes[n].id.clone())
        .collect();
    (names, moving.len())
}

fn global_translation(d: &[[f64; 6]], n: usize) -> Vector3<f64> {
    Vector3::new(d[n][0], d[n][1], d[n][2])
}

/// The member's 12 local displacements from the equation vector.
fn local_displacements(p: &Prepared, u: &[f64]) -> SMatrix<f64, 12, 1> {
    let mut g = SMatrix::<f64, 12, 1>::zeros();
    for c in 0..12 {
        if let Some(q) = p.eq[c] {
            g[c] = u[q];
        }
    }
    p.t * g
}

/// The equations and coefficients of a hold relative to other joints
/// (none for one to the ground): the vector c with c·u the joint's motion
/// along its direction less the weighted motion of the others, each
/// equation once.
fn relative_terms(h: &Stabilisation, node_eq: &[[Option<usize>; 6]]) -> Vec<(usize, f64)> {
    let mut c: Vec<(usize, f64)> = Vec::new();
    if h.relative.is_empty() || h.rotation {
        return c;
    }
    for (n, w) in std::iter::once((h.node, 1.0)).chain(h.relative.iter().map(|&(e, w)| (e, -w))) {
        for (eq, dir) in node_eq[n].iter().zip(h.direction) {
            let v = w * dir;
            if let Some(q) = *eq
                && v != 0.0
            {
                match c.iter_mut().find(|t| t.0 == q) {
                    Some(t) => t.1 += v,
                    None => c.push((q, v)),
                }
            }
        }
    }
    c
}

/// Assembles the tangent stiffness at axial forces `axial`.
fn assemble(
    model: &Model,
    prepared: &[Prepared],
    node_eq: &[[Option<usize>; 6]],
    (axial, slack): (&[f64], &[bool]),
    nonlinear: bool,
    held: &[Stabilisation],
    k: &mut Skyline,
) {
    k.clear();
    for (idx, (m, p)) in model.members.iter().zip(prepared).enumerate() {
        // A member that bends in this solve (a frame, or a tension-only
        // member that bends and is in the model): its frame stiffness.
        if m.is_frame() && !slack[idx] {
            let mut kl = p.ke;
            if nonlinear {
                kl += frame_geometric(p.l, axial[idx]);
            }
            let kg = p.t.transpose() * kl * p.t;
            for a in 0..12 {
                let Some(qa) = p.eq[a] else { continue };
                for b in a..12 {
                    if let Some(qb) = p.eq[b] {
                        // Each unordered pair once: the matrix is
                        // stored symmetric.
                        let v = kg[(a, b)];
                        if v != 0.0 {
                            k.add(qa, qb, v);
                        }
                    }
                }
            }
        } else {
            // Trusses, cables, tension-only members (one that bends, left
            // out or bowed: as a truss — no bending of its own).
            let n_l = if nonlinear { axial[idx] / p.l } else { 0.0 };
            // Left out: no stiffness of its own; a bowed member's
            // constant force still turns with its chord (P-Delta).
            let ea_l = if slack[idx] { 0.0 } else { p.ea_l };
            if slack[idx] && n_l == 0.0 {
                continue;
            }
            let e = p.r.row(0).transpose();
            let b = axial_block(&e, ea_l, n_l);
            for (oa, sa) in [(0, 1.0), (6, -1.0)] {
                for (ob, sb) in [(0, 1.0), (6, -1.0)] {
                    for r in 0..3 {
                        for c in 0..3 {
                            let (Some(qa), Some(qb)) = (p.eq[oa + r], p.eq[ob + c]) else {
                                continue;
                            };
                            // Each unordered pair once.
                            if (oa + r) <= (ob + c) {
                                k.add(qa, qb, sa * sb * b[(r, c)]);
                            }
                        }
                    }
                }
            }
        }
    }
    // Springs on the diagonal.
    for (node, eqs) in model.nodes.iter().zip(node_eq) {
        for (spring, q) in node.spring.iter().zip(eqs) {
            if *spring != 0.0
                && let Some(q) = q
            {
                k.add(*q, *q, *spring);
            }
        }
    }
    // Stabilisation springs along skew directions: k·n·nᵀ.
    for h in held
        .iter()
        .filter(|h| h.axis.is_none() && h.relative.is_empty())
    {
        let base = if h.rotation { 3 } else { 0 };
        let eqs = &node_eq[h.node];
        for a in 0..3 {
            for b in a..3 {
                if let (Some(qa), Some(qb)) = (eqs[base + a], eqs[base + b]) {
                    let v = h.stiffness * h.direction[a] * h.direction[b];
                    if v != 0.0 {
                        k.add(qa, qb, v);
                    }
                }
            }
        }
    }
    // Holds relative to other joints: k·c·cᵀ, c the joint's direction less
    // the others' weighted.
    for h in held.iter().filter(|h| !h.relative.is_empty()) {
        let c = relative_terms(h, node_eq);
        for (x, &(qa, va)) in c.iter().enumerate() {
            for &(qb, vb) in &c[x..] {
                let v = h.stiffness * va * vb;
                if v != 0.0 {
                    k.add(qa, qb, v);
                }
            }
        }
    }
}
