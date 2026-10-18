// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Static solution: first order, or second order (P-Delta) by Newton
//! iterations on the tangent K = Kₑ + K_g(N), and tension-only cables
//! that go slack.

use std::fmt;
use std::time::Duration;

use nalgebra::{Matrix3, SMatrix, Vector3};
use ols_core::budget::{Budget, Stop};

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

/// A stabilisation carrying more than this share of the loads (the larger
/// of their resultant and the largest load at a joint) and more than
/// [`HELD_JOINT_SHARE`] of its own joint's load holds a load in a
/// direction no member holds: a mechanism ([`FeError::Held`]). Otherwise
/// its force is reported ([`Solution::stabilised`]) — the out-of-plane
/// part of the self-weight at a joint of a battered tower face, or the
/// weight at the crossing of a horizontal bracing, for example.
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
    /// A cable gone slack.
    pub slack: bool,
}

/// A support reaction.
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
    /// Reactions at the nodes with supports or springs.
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
pub fn solve(
    model: &Model,
    loads: &Loads,
    options: &Options,
    budget: &Budget,
) -> Result<Solution, FeError> {
    match solve_with(model, loads, options, budget, &[]) {
        Err(e @ (FeError::Mechanism { .. } | FeError::Unstable { .. })) if options.stabilise => {
            let held = crate::stabilise::find(model);
            if held.is_empty() {
                return Err(e);
            }
            solve_with(model, loads, options, budget, &held)
        }
        other => other,
    }
}

/// Solves `model` under `loads` with the joint directions `held`.
fn solve_with(
    model: &Model,
    loads: &Loads,
    options: &Options,
    budget: &Budget,
    held: &[Stabilisation],
) -> Result<Solution, FeError> {
    model.check().map_err(FeError::Invalid)?;
    let n_nodes = model.nodes.len();
    // Rotations exist at nodes with a frame member; nodes without a
    // member have no equations.
    let mut has_frame = vec![false; n_nodes];
    let mut connected = vec![false; n_nodes];
    let mut adjacent = vec![Vec::new(); n_nodes];
    for m in &model.members {
        connected[m.i] = true;
        connected[m.j] = true;
        if m.kind == MemberKind::Frame {
            has_frame[m.i] = true;
            has_frame[m.j] = true;
        }
        adjacent[m.i].push(m.j);
        adjacent[m.j].push(m.i);
    }
    let order = reverse_cuthill_mckee(&adjacent);
    // Degrees of freedom the stabilisation removes.
    let mut removed = vec![[false; 6]; n_nodes];
    for h in held {
        if let Some(a) = h.axis {
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
        .map(|m| {
            let s = &model.sections[m.section];
            let l = model.length(m);
            let r = axes(model, m);
            let mut eqs = [None; 12];
            eqs[..6].copy_from_slice(&eq[m.i]);
            eqs[6..].copy_from_slice(&eq[m.j]);
            let w = s.weight() * model.dead_load_factor(m) * loads.self_weight;
            let weight_local = if m.kind == MemberKind::Frame {
                frame_uniform_load(l, r * Vector3::new(0.0, 0.0, -w))
            } else {
                [0.0; 12]
            };
            Prepared {
                l,
                r,
                t: transformation(&r),
                ke: if m.kind == MemberKind::Frame {
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
    for (m, p) in model.members.iter().zip(&prepared) {
        if m.kind == MemberKind::Frame {
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
    let mut k = Skyline::new(first);

    // Iterations.
    let mut u = vec![0.0; n_eq];
    let mut axial: Vec<f64> = model.members.iter().map(|m| m.pretension).collect();
    let mut slack = vec![false; model.members.len()];
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
        // Cable pretension pulls the end nodes together.
        for (idx, (m, p)) in model.members.iter().zip(&prepared).enumerate() {
            if m.kind == MemberKind::Cable && !slack[idx] && m.pretension != 0.0 {
                let e = p.r.row(0).transpose() * m.pretension;
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
            } else {
                axial[idx] = n;
            }
        }
        // First order without cables: one solve is exact.
        if !options.nonlinear && !model.members.iter().any(|m| m.kind == MemberKind::Cable) {
            assemble(model, &prepared, &eq, (&axial, &slack), false, held, &mut k);
            let ku = k.mul(&u);
            residual = f
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
            if m.kind == MemberKind::Frame {
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
                if options.nonlinear && !slack[idx] {
                    // The transverse part of a rotated axial force.
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
                slack: slack[idx],
            }
        })
        .collect();
    // What each stabilisation holds: the out-of-balance of its joint along
    // its direction (its spring's force, or the reaction of the degree of
    // freedom it removed).
    let mut held_force = vec![[0.0; 6]; n_nodes];
    let stabilised: Vec<Stabilised> = held
        .iter()
        .map(|h| {
            let base = if h.rotation { 3 } else { 0 };
            let force: f64 = (0..3)
                .map(|c| h.direction[c] * (internal[h.node][base + c] - f_ext[h.node][base + c]))
                .sum();
            for c in 0..3 {
                held_force[h.node][base + c] += force * h.direction[c];
            }
            Stabilised {
                at: h.clone(),
                force,
            }
        })
        .collect();
    if !held.is_empty() {
        check_held(model, &f_ext, &stabilised)?;
    }
    let reactions = model
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.is_support())
        .map(|(n, node)| {
            let mut r = [0.0; 6];
            for d in 0..6 {
                r[d] = internal[n][d] + node.spring[d] * displacements[n][d] - f_ext[n][d];
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
    })
}

/// Refuses a stabilisation that carries load: one whose force is above
/// [`HELD_SHARE`] of the loads (the larger of their resultant and the
/// largest load at a joint; a moment over the model's size) and above
/// [`HELD_JOINT_SHARE`] of its joint's own load — a load in a direction
/// no member holds, which the structure does not carry.
fn check_held(model: &Model, f_ext: &[[f64; 6]], held: &[Stabilised]) -> Result<(), FeError> {
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
        return Ok(());
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
    let over: Vec<&Stabilised> = held
        .iter()
        .filter(|h| share(h) > HELD_SHARE && h.force.abs() > HELD_JOINT_SHARE * own(h))
        .collect();
    let Some(worst) = over
        .iter()
        .copied()
        .max_by(|a, b| share(a).total_cmp(&share(b)))
    else {
        return Ok(());
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
        match m.kind {
            MemberKind::Frame => {
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
            }
            MemberKind::Truss | MemberKind::Cable => {
                if slack[idx] {
                    continue;
                }
                let e = p.r.row(0).transpose();
                let n_l = if nonlinear { axial[idx] / p.l } else { 0.0 };
                let b = axial_block(&e, p.ea_l, n_l);
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
    for h in held.iter().filter(|h| h.axis.is_none()) {
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
}
