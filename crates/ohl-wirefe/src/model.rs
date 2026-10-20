// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Wire-system model and Newton–Raphson solver.
//!
//! Nodes are points in 3D (attachment points, insulator clamps, load points,
//! yoke plates); **fixed** nodes are supports. Elements:
//!
//! - [`CableElement`]: exact elastic catenary (one per span or span piece);
//! - [`TrussElement`]: straight bar — insulator strings, links, V-string
//!   legs (optionally tension-only, so a compressed leg goes slack).
//!
//! Equilibrium of the free nodes is solved by Newton–Raphson on their
//! positions with a backtracking line search. Exit clauses: an iteration and
//! time budget for the structure, and per element; when it stops early the
//! error names the node with the largest unbalanced force.

use std::fmt;
use std::time::Duration;

use nalgebra::{DMatrix, DVector, Matrix3, Vector3};
use ohl_core::budget::{Budget, Stop};

use crate::catenary_element::{
    CableProperties, ElementError, ElementState, default_element_budget,
};

/// A node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    /// Position (initial guess for free nodes), m. z is up.
    pub position: Vector3<f64>,
    /// Fixed (support) or free.
    pub fixed: bool,
    /// External point load on the node, N.
    pub load: Vector3<f64>,
}

impl Node {
    /// Fixed support at `position`.
    pub fn fixed(position: Vector3<f64>) -> Self {
        Self {
            position,
            fixed: true,
            load: Vector3::zeros(),
        }
    }

    /// Free node starting at `position`.
    pub fn free(position: Vector3<f64>) -> Self {
        Self {
            position,
            fixed: false,
            load: Vector3::zeros(),
        }
    }
}

/// A cable span (or span piece) between two nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CableElement {
    /// Start node index.
    pub i: usize,
    /// End node index.
    pub j: usize,
    /// Properties (unstressed length, EA, load per unstressed length).
    pub properties: CableProperties,
}

/// A straight bar between two nodes (insulator, link, V-string leg).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrussElement {
    /// Start node index.
    pub i: usize,
    /// End node index.
    pub j: usize,
    /// Unstressed length, m.
    pub unstressed_length: f64,
    /// Axial stiffness, N.
    pub axial_stiffness: f64,
    /// Carries no compression (goes slack) when true.
    pub tension_only: bool,
    /// Total weight, N, lumped half on each end node.
    pub weight: f64,
}

/// A wire-system model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    /// Nodes.
    pub nodes: Vec<Node>,
    /// Cable elements.
    pub cables: Vec<CableElement>,
    /// Truss elements.
    pub trusses: Vec<TrussElement>,
}

/// Solved state of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    /// Final node positions, m.
    pub positions: Vec<Vector3<f64>>,
    /// Cable element states, in model order.
    pub cables: Vec<ElementState>,
    /// Truss axial forces, N (tension positive), in model order.
    pub truss_forces: Vec<f64>,
    /// Reaction forces on fixed nodes (force the support exerts), N; zero for free nodes.
    pub reactions: Vec<Vector3<f64>>,
    /// Newton iterations used.
    pub iterations: u64,
    /// Largest unbalanced force on a free node at the end, N.
    pub residual: f64,
}

/// Errors of the wire-system solver.
#[derive(Debug, Clone, PartialEq)]
pub enum FeError {
    /// Invalid model (bad indices, no free node, …).
    InvalidModel(String),
    /// An element could not be evaluated.
    Element {
        /// Cable element index.
        element: usize,
        /// The element error.
        error: ElementError,
    },
    /// Newton did not converge.
    NotConverged {
        /// Why it stopped.
        stop: Stop,
        /// Largest unbalanced force, N.
        residual: f64,
        /// Node with the largest unbalanced force.
        worst_node: usize,
    },
    /// The stiffness matrix is singular (a free node is not held in some
    /// direction, e.g. a node connected only to slack legs).
    Singular {
        /// Newton iteration.
        iteration: u64,
    },
}

impl fmt::Display for FeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModel(m) => write!(f, "invalid wire model: {m}"),
            Self::Element { element, error } => write!(f, "cable element {element}: {error}"),
            Self::NotConverged {
                stop,
                residual,
                worst_node,
            } => write!(
                f,
                "wire system did not converge: {stop}; largest unbalanced force {residual:.3e} N at node {worst_node}"
            ),
            Self::Singular { iteration } => write!(
                f,
                "stiffness matrix singular at Newton iteration {iteration}: a free node is not restrained \
                 in some direction (e.g. only connected to slack legs)"
            ),
        }
    }
}

impl std::error::Error for FeError {}

/// Default exit clauses for a structure solve: Newton usually converges in
/// 5–20 iterations; 100 iterations cover difficult cases (broken wire)
/// with margin. The time limit only guards against a solve that never ends:
/// set far above any real solve (120 s), it does not stop slow machines.
pub fn default_fe_budget() -> Budget {
    Budget::new()
        .max_iterations(100)
        .time_limit(Duration::from_secs(120))
}

/// Where spheres of radii `ra` about `a` and `rb` about `b` meet nearest
/// to `near` (None: they do not meet).
fn meeting(
    a: Vector3<f64>,
    ra: f64,
    b: Vector3<f64>,
    rb: f64,
    near: Vector3<f64>,
) -> Option<Vector3<f64>> {
    let ab = b - a;
    let d = ab.norm();
    if d < 1e-12 || d > ra + rb || d < (ra - rb).abs() {
        return None;
    }
    let u = ab / d;
    let along = (ra * ra - rb * rb + d * d) / (2.0 * d);
    let h = (ra * ra - along * along).max(0.0).sqrt();
    let c = a + u * along;
    let off = (near - c) - u * u.dot(&(near - c));
    let n = off.norm();
    // Newton's point on the line through both centres: any point of the
    // circle (it is symmetric about that line).
    let v = if n > 1e-12 {
        off / n
    } else {
        let any = if u.x.abs() < 0.9 {
            Vector3::x()
        } else {
            Vector3::y()
        };
        let w = any - u * u.dot(&any);
        w / w.norm()
    };
    Some(c + v * h)
}

struct Evaluation {
    /// Net force on every node (elements + external), N.
    forces: Vec<Vector3<f64>>,
    stiffness: DMatrix<f64>,
    cables: Vec<ElementState>,
    truss_forces: Vec<f64>,
}

impl TrussElement {
    /// Axial force and 3×3 tangent stiffness for chord `d`.
    fn evaluate(&self, d: Vector3<f64>) -> (f64, Vector3<f64>, Matrix3<f64>) {
        let length = d.norm();
        let n = d / length;
        let k_axial = self.axial_stiffness / self.unstressed_length;
        let mut force = k_axial * (length - self.unstressed_length);
        let nn = n * n.transpose();
        let mut k = nn * k_axial;
        // Slack only when clearly shorter than the unstressed length: at zero
        // force (the usual starting state) rounding gives a tiny negative force,
        // and treating that as slack left nodes unrestrained (Newton then
        // collapsed a 3 m string to 5 mm in testing). The taut stiffness is the
        // right tangent at the kink.
        if self.tension_only && length < self.unstressed_length * (1.0 - 1e-9) {
            force = 0.0;
            k = nn * (k_axial * 1e-9);
        }
        k += (Matrix3::identity() - nn) * (force.max(0.0) / length);
        (force, n, k)
    }
}

impl Model {
    fn check(&self) -> Result<(), FeError> {
        let n = self.nodes.len();
        let bad = |i: usize, j: usize| i >= n || j >= n || i == j;
        if let Some(e) = self.cables.iter().position(|c| bad(c.i, c.j)) {
            return Err(FeError::InvalidModel(format!(
                "cable element {e} has invalid nodes"
            )));
        }
        if let Some(e) = self.trusses.iter().position(|t| bad(t.i, t.j)) {
            return Err(FeError::InvalidModel(format!(
                "truss element {e} has invalid nodes"
            )));
        }
        if let Some(e) = self
            .trusses
            .iter()
            .position(|t| !(t.unstressed_length > 0.0 && t.axial_stiffness > 0.0))
        {
            return Err(FeError::InvalidModel(format!(
                "truss element {e} needs positive length and stiffness"
            )));
        }
        Ok(())
    }

    fn evaluate(
        &self,
        positions: &[Vector3<f64>],
        dof: &[Option<usize>],
        n_dof: usize,
        guesses: &mut [Option<(f64, f64)>],
    ) -> Result<Evaluation, FeError> {
        let mut forces: Vec<Vector3<f64>> = self.nodes.iter().map(|n| n.load).collect();
        let mut k = DMatrix::zeros(n_dof, n_dof);
        let mut add_block = |a: usize, b: usize, block: &Matrix3<f64>, sign: f64| {
            if let (Some(ra), Some(rb)) = (dof[a], dof[b]) {
                for r in 0..3 {
                    for c in 0..3 {
                        k[(ra + r, rb + c)] += sign * block[(r, c)];
                    }
                }
            }
        };
        let element_budget = default_element_budget();
        let mut cables = Vec::with_capacity(self.cables.len());
        for (e, cable) in self.cables.iter().enumerate() {
            let d = positions[cable.j] - positions[cable.i];
            let state = cable
                .properties
                .solve(d, guesses[e], &element_budget)
                .map_err(|error| FeError::Element { element: e, error })?;
            guesses[e] = Some((state.horizontal, state.v0));
            forces[cable.i] += state.force_i;
            forces[cable.j] += state.force_j;
            add_block(cable.i, cable.i, &state.stiffness, 1.0);
            add_block(cable.j, cable.j, &state.stiffness, 1.0);
            add_block(cable.i, cable.j, &state.stiffness, -1.0);
            add_block(cable.j, cable.i, &state.stiffness, -1.0);
            cables.push(state);
        }
        let mut truss_forces = Vec::with_capacity(self.trusses.len());
        for truss in &self.trusses {
            let (force, n, kt) = truss.evaluate(positions[truss.j] - positions[truss.i]);
            let half_weight = Vector3::new(0.0, 0.0, -truss.weight / 2.0);
            forces[truss.i] += n * force + half_weight;
            forces[truss.j] += -n * force + half_weight;
            add_block(truss.i, truss.i, &kt, 1.0);
            add_block(truss.j, truss.j, &kt, 1.0);
            add_block(truss.i, truss.j, &kt, -1.0);
            add_block(truss.j, truss.i, &kt, -1.0);
            truss_forces.push(force);
        }
        Ok(Evaluation {
            forces,
            stiffness: k,
            cables,
            truss_forces,
        })
    }

    /// Puts the free end of every bar back on the sphere it can swing on
    /// about its other end, keeping the length change Newton intended.
    ///
    /// A Newton step moves a swinging insulator clamp along a straight line
    /// tangent to its arc; with a nearly rigid string that fake stretch
    /// (δ²/2L) creates huge spurious forces and stalls the iteration (seen in
    /// testing: 100 iterations without convergence). Projecting removes the
    /// curvature error while keeping Newton's radial (length) update.
    ///
    /// Each bar swings its end farther from the supports about the nearer
    /// (equally far: its second node about its first, a string's bottom
    /// about its top). Nodes are placed from the supports outwards, each
    /// once the nodes it hangs from are: on its one bar's sphere, or where
    /// its two bars' spheres meet nearest to Newton's point (a V's tip —
    /// also one closing chains of strings), or, with more bars, by a few
    /// alternating passes.
    fn project_links(&self, old: &[Vector3<f64>], trial: &mut [Vector3<f64>]) {
        let depth = self.truss_depths();
        // Intended lengths from Newton's (unprojected) step, computed ONCE.
        // Recomputing them from already-projected points in every pass
        // shrank each string by a factor cos θ per pass — a bug found by
        // tracing: −6.5 µm on a 3 m string, thousands of newtons of fake force.
        let mut links: Vec<(usize, usize, f64, bool, f64)> = Vec::new();
        for t in &self.trusses {
            let (pivot, free) = match (self.nodes[t.i].fixed, self.nodes[t.j].fixed) {
                (true, false) => (t.i, t.j),
                (false, true) => (t.j, t.i),
                (false, false) => match (depth[t.i], depth[t.j]) {
                    (Some(a), Some(b)) if b < a => (t.j, t.i),
                    (Some(_), Some(_)) => (t.i, t.j),
                    _ => continue,
                },
                (true, true) => continue,
            };
            let old_chord = old[free] - old[pivot];
            let old_len = old_chord.norm();
            if old_len == 0.0 {
                continue;
            }
            // The length change Newton intended: the ends' relative move
            // along the bar (the pivot may move too).
            let moved = (trial[free] - old[free]) - (trial[pivot] - old[pivot]);
            let intended = old_len + (old_chord / old_len).dot(&moved);
            let slack = t.tension_only && intended > (trial[free] - trial[pivot]).norm();
            links.push((pivot, free, intended, slack, t.unstressed_length));
        }
        // The nodes to place, each with its bars (pivot, length); a slack
        // leg does not hold its node — but a node all of whose bars are
        // slack would hang on nothing (a joint of a chain of strings, both
        // its strings slack after a step): it drops until its bar nearest
        // the supports is taut again (Newton cannot bring it back: slack
        // bars give it no stiffness and no move changes its force).
        let mut held: std::collections::BTreeMap<usize, Vec<(usize, f64)>> =
            std::collections::BTreeMap::new();
        let mut loose: std::collections::BTreeMap<usize, (usize, f64)> =
            std::collections::BTreeMap::new();
        for &(pivot, free, intended, slack, length) in &links {
            let entry = held.entry(free).or_default();
            if !slack {
                entry.push((pivot, intended));
            } else {
                let nearer = loose
                    .get(&free)
                    .is_none_or(|(p, _)| depth[pivot] < depth[*p]);
                if nearer {
                    loose.insert(free, (pivot, intended.max(length)));
                }
            }
        }
        for (node, bar) in loose {
            if let Some(bars) = held.get_mut(&node)
                && bars.is_empty()
            {
                bars.push(bar);
            }
        }
        // From the supports outwards: a node once every node it hangs from
        // is placed (a loop: the rest in order of depth).
        let mut order: Vec<usize> = Vec::with_capacity(held.len());
        let mut placed: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        loop {
            let ready: Vec<usize> = held
                .iter()
                .filter(|(n, bars)| {
                    !placed.contains(n)
                        && bars
                            .iter()
                            .all(|(p, _)| !held.contains_key(p) || placed.contains(p))
                })
                .map(|(n, _)| *n)
                .collect();
            if ready.is_empty() {
                let mut rest: Vec<usize> = held
                    .keys()
                    .copied()
                    .filter(|n| !placed.contains(n))
                    .collect();
                rest.sort_by_key(|&n| depth[n]);
                order.extend(rest);
                break;
            }
            for n in ready {
                placed.insert(n);
                order.push(n);
            }
        }
        let sphere = |trial: &mut [Vector3<f64>], node: usize, pivot: usize, length: f64| {
            let chord = trial[node] - trial[pivot];
            let len = chord.norm();
            if len > 0.0 {
                trial[node] = trial[pivot] + chord * (length / len);
            }
        };
        for node in order {
            let bars = &held[&node];
            match bars[..] {
                [] => {}
                [(p, l)] => sphere(trial, node, p, l),
                [(p, lp), (q, lq)] => match meeting(trial[p], lp, trial[q], lq, trial[node]) {
                    Some(at) => trial[node] = at,
                    None => sphere(trial, node, p, lp),
                },
                _ => {
                    for _ in 0..5 {
                        for &(p, l) in bars {
                            sphere(trial, node, p, l);
                        }
                    }
                }
            }
        }
    }

    /// Each node's distance in bars from a fixed node (None: not reached
    /// through bars).
    fn truss_depths(&self) -> Vec<Option<usize>> {
        let mut depth: Vec<Option<usize>> =
            self.nodes.iter().map(|n| n.fixed.then_some(0)).collect();
        // Few bars: relax until nothing changes (at most one pass per bar).
        for _ in 0..=self.trusses.len() {
            let mut changed = false;
            for t in &self.trusses {
                for (a, b) in [(t.i, t.j), (t.j, t.i)] {
                    if let Some(d) = depth[a]
                        && depth[b].is_none_or(|e| e > d + 1)
                    {
                        depth[b] = Some(d + 1);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        depth
    }

    /// Solves the equilibrium of the free nodes.
    ///
    /// Expected cost: each Newton iteration evaluates every element (~1 µs)
    /// and solves a dense system of 3 × (free nodes); a 10-span section with
    /// suspension strings converges in well under 10 ms.
    pub fn solve(&self, budget: &Budget) -> Result<Solution, FeError> {
        match self.solve_direct(budget) {
            Ok(solution) => Ok(solution),
            Err(
                error @ (FeError::NotConverged { .. }
                | FeError::Element { .. }
                | FeError::Singular { .. }),
            ) if self
                .nodes
                .iter()
                .any(|n| !n.fixed && n.load != Vector3::zeros())
                || self
                    .cables
                    .iter()
                    .any(|c| c.properties.load.x != 0.0 || c.properties.load.y != 0.0) =>
            {
                self.solve_by_load_steps(budget)
                    .or_else(|_| self.solve_by_relaxation(budget))
                    .map_err(|_| error)
            }
            Err(error @ (FeError::NotConverged { .. } | FeError::Singular { .. })) => {
                self.solve_by_relaxation(budget).map_err(|_| error)
            }
            Err(error) => Err(error),
        }
    }

    /// Last resort for strings hanging from strings (chains of hinged
    /// joints that Newton can fold or stall on): dynamic relaxation — each
    /// free node a mass moving under its unbalanced force (masses from the
    /// stiffness, so every step is stable), the motion stopped at each peak
    /// of kinetic energy — until near equilibrium, then Newton from there.
    /// Only for models with such strings (others: the error stands).
    /// Exit clauses: 50 000 steps or 5 s, and the budget's time limit.
    fn solve_by_relaxation(&self, budget: &Budget) -> Result<Solution, FeError> {
        let chained = self
            .trusses
            .iter()
            .any(|t| !self.nodes[t.i].fixed && !self.nodes[t.j].fixed);
        if !chained {
            return Err(FeError::InvalidModel("no chained strings".into()));
        }
        let started = std::time::Instant::now();
        let run = budget.start();
        let mut dof = vec![None; self.nodes.len()];
        let mut n_dof = 0;
        for (i, node) in self.nodes.iter().enumerate() {
            if !node.fixed {
                dof[i] = Some(n_dof);
                n_dof += 3;
            }
        }
        let mut positions: Vec<Vector3<f64>> = self.nodes.iter().map(|n| n.position).collect();
        let mut velocity = vec![Vector3::zeros(); self.nodes.len()];
        // Masses only grow: a string going slack would otherwise leave its
        // joint light, and the next taut step throws it far.
        let mut masses = vec![1.0_f64; self.nodes.len()];
        let mut guesses = vec![None; self.cables.len()];
        let mut energy = 0.0_f64;
        let load_scale = self
            .nodes
            .iter()
            .map(|n| n.load.norm())
            .chain(self.trusses.iter().map(|t| t.weight))
            .chain(
                self.cables
                    .iter()
                    .map(|c| c.properties.load.norm() * c.properties.unstressed_length),
            )
            .fold(1.0_f64, f64::max);
        for step in 0..50_000 {
            if step % 256 == 0 {
                run.check().map_err(|stop| FeError::NotConverged {
                    stop,
                    residual: f64::NAN,
                    worst_node: 0,
                })?;
                if started.elapsed() > Duration::from_secs(5) {
                    break;
                }
            }
            let eval = self.evaluate(&positions, &dof, n_dof, &mut guesses)?;
            let mut worst = 0.0_f64;
            let mut next_energy = 0.0;
            for (i, f) in eval.forces.iter().enumerate() {
                let Some(r) = dof[i] else { continue };
                worst = worst.max(f.norm());
                // A mass for which a unit time step is stable: the row sum
                // of the stiffness (Gershgorin), at least a small floor.
                let row: f64 = (0..3)
                    .map(|k| {
                        (0..n_dof)
                            .map(|c| eval.stiffness[(r + k, c)].abs())
                            .sum::<f64>()
                    })
                    .fold(0.0, f64::max);
                masses[i] = masses[i].max(row);
                let mass = masses[i];
                velocity[i] += f / mass;
                next_energy += mass * velocity[i].norm_squared();
            }
            if worst < 1e-3 * load_scale {
                break;
            }
            // Past a peak of kinetic energy: stop the motion there.
            if next_energy < energy {
                velocity.iter_mut().for_each(|v| *v = Vector3::zeros());
                energy = 0.0;
                continue;
            }
            energy = next_energy;
            for (i, v) in velocity.iter().enumerate() {
                if dof[i].is_some() {
                    positions[i] += v;
                }
            }
        }
        let mut model = self.clone();
        for (node, p) in model.nodes.iter_mut().zip(&positions) {
            node.position = *p;
        }
        let result = model.solve_direct(budget);
        if ohl_core::env::var_os("OHL_FE_TRACE").is_some() {
            let eval = self.evaluate(&positions, &dof, n_dof, &mut guesses)?;
            let worst = eval
                .forces
                .iter()
                .enumerate()
                .filter(|(i, _)| dof[*i].is_some())
                .map(|(_, f)| f.norm())
                .fold(0.0, f64::max);
            eprintln!(
                "relaxation: {:.0?}, largest unbalanced force {worst:.3e} N (target {:.3e}); then Newton: {}",
                started.elapsed(),
                1e-3 * load_scale,
                match &result {
                    Ok(s) => format!("solved in {} iterations", s.iterations),
                    Err(e) => e.to_string(),
                }
            );
        }
        result
    }

    /// Fallback for hard cases (e.g. many heavy point loads far from the
    /// starting shape — a span with 23 alternating 359 N / 30 N markers did
    /// not converge directly; strings chained between free joints swung
    /// by wind): solve the cables under their vertical load alone, then
    /// ramp the point loads and the sideways (wind) part of the cables'
    /// load up together, each step starting from the previous solution,
    /// halving the step after a failure. Exit clauses: at most 40 steps,
    /// step ≥ 1/256, and the budget's time limit for the whole procedure.
    fn solve_by_load_steps(&self, budget: &Budget) -> Result<Solution, FeError> {
        let run = budget.start();
        let full: Vec<Vector3<f64>> = self.nodes.iter().map(|n| n.load).collect();
        let cable_full: Vec<Vector3<f64>> = self.cables.iter().map(|c| c.properties.load).collect();
        let mut model = self.clone();
        let set_loads = |model: &mut Model, factor: f64| {
            for (node, load) in model.nodes.iter_mut().zip(&full) {
                node.load = load * factor;
            }
            for (cable, load) in model.cables.iter_mut().zip(&cable_full) {
                cable.properties.load = Vector3::new(load.x * factor, load.y * factor, load.z);
            }
        };
        set_loads(&mut model, 0.0);
        let mut solution = model.solve_direct(budget)?;
        let (mut done, mut step) = (0.0_f64, 0.25_f64);
        for _ in 0..40 {
            run.check().map_err(|stop| FeError::NotConverged {
                stop,
                residual: f64::NAN,
                worst_node: 0,
            })?;
            let target = (done + step).min(1.0);
            for (node, p) in model.nodes.iter_mut().zip(&solution.positions) {
                node.position = *p;
            }
            set_loads(&mut model, target);
            let result = model.solve_direct(budget);
            if ohl_core::env::var_os("OHL_FE_TRACE").is_some() {
                eprintln!(
                    "load steps: {target:.4} of the loads: {}",
                    match &result {
                        Ok(s) => format!("solved in {} iterations", s.iterations),
                        Err(e) => e.to_string(),
                    }
                );
            }
            match result {
                Ok(next) => {
                    solution = next;
                    done = target;
                    if done >= 1.0 {
                        return Ok(solution);
                    }
                    step = (step * 2.0).min(0.5);
                }
                Err(error) => {
                    step /= 2.0;
                    if step < 1.0 / 256.0 {
                        return Err(error);
                    }
                }
            }
        }
        Err(FeError::NotConverged {
            stop: Stop::IterationLimit {
                iterations: 40,
                elapsed: run.elapsed(),
            },
            residual: f64::NAN,
            worst_node: 0,
        })
    }

    /// Newton–Raphson from the nodes' current positions.
    fn solve_direct(&self, budget: &Budget) -> Result<Solution, FeError> {
        self.check()?;
        let mut dof = vec![None; self.nodes.len()];
        let mut n_dof = 0;
        for (i, node) in self.nodes.iter().enumerate() {
            if !node.fixed {
                dof[i] = Some(n_dof);
                n_dof += 3;
            }
        }
        let mut positions: Vec<Vector3<f64>> = self.nodes.iter().map(|n| n.position).collect();
        let mut guesses = vec![None; self.cables.len()];
        let load_scale = self
            .cables
            .iter()
            .map(|c| c.properties.load.norm() * c.properties.unstressed_length)
            .chain(self.nodes.iter().map(|n| n.load.norm()))
            .chain(self.trusses.iter().map(|t| t.weight))
            .fold(1.0_f64, f64::max);

        let free_residual = |eval: &Evaluation| -> (f64, usize) {
            let mut worst = (0.0, 0);
            for (i, f) in eval.forces.iter().enumerate() {
                // NaN counts as infinite: never converged.
                let norm = if f.norm().is_nan() {
                    f64::INFINITY
                } else {
                    f.norm()
                };
                if dof[i].is_some() && norm > worst.0 {
                    worst = (norm, i);
                }
            }
            worst
        };
        // The unbalanced force of all free nodes together (root of the sum
        // of squares): a step that swings a chain of strings as a whole
        // lowers it while one joint's force may still grow.
        let total_residual = |eval: &Evaluation| -> f64 {
            let s: f64 = eval
                .forces
                .iter()
                .enumerate()
                .filter(|(i, _)| dof[*i].is_some())
                .map(|(_, f)| f.norm_squared())
                .sum();
            if s.is_nan() { f64::INFINITY } else { s.sqrt() }
        };

        // Each joint's largest move per step, where strings hang from
        // strings: a third of its shortest string.
        let chained = self
            .trusses
            .iter()
            .any(|t| !self.nodes[t.i].fixed && !self.nodes[t.j].fixed);
        let mut caps: Vec<Option<f64>> = vec![None; self.nodes.len()];
        if chained {
            for t in &self.trusses {
                for n in [t.i, t.j] {
                    let cap = t.unstressed_length / 3.0;
                    caps[n] = Some(caps[n].map_or(cap, |c: f64| c.min(cap)));
                }
            }
        }

        // Set OHL_FE_TRACE=1 to print the Newton history (diagnostics).
        let trace = ohl_core::env::var_os("OHL_FE_TRACE").is_some();
        let mut run = budget.start();
        let mut eval = self.evaluate(&positions, &dof, n_dof, &mut guesses)?;
        let (mut residual, mut worst_node) = free_residual(&eval);
        let mut total = total_residual(&eval);
        // Convergence tolerance: 1e-9 of the loads, but never below what
        // floating point can resolve. Coordinates carry ~1e-16 relative
        // error, which a stiff bar turns into force noise (k · ε · |x|): with a
        // 3e9 N/m string at 600 m that floor is ~1e-4 N, and a pure relative
        // tolerance (4.8e-6 N) was unreachable in testing.
        let coordinate_scale = positions.iter().map(|p| p.amax()).fold(1.0_f64, f64::max);
        let stiffest = (0..n_dof)
            .map(|d| eval.stiffness[(d, d)].abs())
            .fold(0.0, f64::max);
        // Each cable element is itself solved iteratively, which leaves force
        // noise proportional to EA: converged spans checked against another program's
        // export stalled at 6.3e-6 N (≈ 3e-13·EA, 1e-9 of the load) and were
        // reported as failures. 1e-11·EA covers it with a 30x margin (1e-10
        // was too loose: it cost accuracy in the insulator-swing check).
        let element_floor = 1e-11
            * self
                .cables
                .iter()
                .map(|c| c.properties.axial_stiffness)
                .fold(0.0, f64::max);
        let tolerance = (1e-9 * load_scale)
            .max(64.0 * f64::EPSILON * stiffest * coordinate_scale)
            .max(element_floor);
        // Initial damping: a small fraction of the stiffest diagonal term.
        let mut mu = 1e-4
            * (0..n_dof)
                .map(|d| eval.stiffness[(d, d)].abs())
                .fold(0.0, f64::max);
        while residual > tolerance || residual.is_nan() {
            run.step().map_err(|stop| FeError::NotConverged {
                stop,
                residual,
                worst_node,
            })?;
            let mut rhs = DVector::zeros(n_dof);
            for (i, f) in eval.forces.iter().enumerate() {
                if let Some(r) = dof[i] {
                    rhs.fixed_rows_mut::<3>(r).copy_from(f);
                }
            }
            // Levenberg–Marquardt damping: K + μI. μ is tiny next to stiff bar
            // directions (so a string picks up its load in one step) but keeps
            // steps moderate where the structure is still soft (a string at
            // zero force has no pendulum stiffness yet). μ falls after good
            // steps, so the iteration becomes plain Newton near the solution.
            let mut damped = eval.stiffness.clone();
            for d in 0..n_dof {
                damped[(d, d)] += mu;
            }
            let step = damped
                .lu()
                .solve(&rhs)
                .filter(|s| s.iter().all(|v| v.is_finite()))
                .ok_or(FeError::Singular {
                    iteration: run.iterations(),
                })?;
            // Strings hanging from strings (chains, a V under a V): a joint
            // moves at most a third of its shortest string per step — a
            // longer step can fold a chain the wrong way, which Newton
            // does not undo. (Models without them step as before.)
            let mut factor: f64 = 1.0;
            for (i, cap) in caps.iter().enumerate() {
                if let (Some(cap), Some(r)) = (cap, dof[i]) {
                    let moved = step.fixed_rows::<3>(r).norm();
                    if moved > *cap {
                        factor = factor.min(cap / moved);
                    }
                }
            }
            // Backtracking line search on the unbalanced force.
            loop {
                let mut trial: Vec<Vector3<f64>> = positions
                    .iter()
                    .enumerate()
                    .map(|(i, p)| match dof[i] {
                        Some(r) => p + step.fixed_rows::<3>(r) * factor,
                        None => *p,
                    })
                    .collect();
                self.project_links(&positions, &mut trial);
                let mut trial_guesses = guesses.clone();
                match self.evaluate(&trial, &dof, n_dof, &mut trial_guesses) {
                    Ok(trial_eval) => {
                        let (r, w) = free_residual(&trial_eval);
                        let tr = total_residual(&trial_eval);
                        if r < residual || tr < 0.9 * total || factor < 1e-6 {
                            if trace {
                                eprintln!(
                                    "newton {:>3}: residual {r:.3e} N at node {w}, step factor {factor}, |step| {:.3e} m, mu {mu:.2e}",
                                    run.iterations(),
                                    step.norm()
                                );
                            }
                            mu = if factor == 1.0 { mu * 0.2 } else { mu * 4.0 };
                            total = tr;
                            (positions, eval, guesses, residual, worst_node) =
                                (trial, trial_eval, trial_guesses, r, w);
                            break;
                        }
                    }
                    Err(error) if factor < 1e-6 => return Err(error),
                    Err(_) => {}
                }
                factor *= 0.5;
                run.check().map_err(|stop| FeError::NotConverged {
                    stop,
                    residual,
                    worst_node,
                })?;
            }
        }

        let reactions = eval
            .forces
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if dof[i].is_none() {
                    -f
                } else {
                    Vector3::zeros()
                }
            })
            .collect();
        Ok(Solution {
            positions,
            cables: eval.cables,
            truss_forces: eval.truss_forces,
            reactions,
            iterations: run.iterations(),
            residual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 15.97;
    const EA: f64 = 34.64e6;

    fn cable(i: usize, j: usize, l0: f64, load: Vector3<f64>) -> CableElement {
        CableElement {
            i,
            j,
            properties: CableProperties {
                unstressed_length: l0,
                axial_stiffness: EA,
                load,
            },
        }
    }

    fn gravity() -> Vector3<f64> {
        Vector3::new(0.0, 0.0, -W)
    }

    #[test]
    fn single_span_between_supports_needs_no_iteration() {
        let model = Model {
            nodes: vec![
                Node::fixed(Vector3::zeros()),
                Node::fixed(Vector3::new(300.0, 0.0, 0.0)),
            ],
            cables: vec![cable(0, 1, 300.4, gravity())],
            trusses: vec![],
        };
        let s = model.solve(&default_fe_budget()).unwrap();
        assert_eq!(s.iterations, 0);
        // Reactions carry the cable weight.
        let total: Vector3<f64> = s.reactions.iter().sum();
        assert!((total.z - W * 300.4).abs() < 1e-6, "{total}");
    }

    /// An infinite line of identical level spans and suspension strings with
    /// transverse wind: every clamp swings the same way, every span is
    /// symmetric and sends exactly half its load to each clamp, so
    /// tan φ = q_y L0 / (q_z L0 + W_ins / 2)  (rigid string, weight lumped).
    ///
    /// Emulated with one clamp and two spans whose far ends are moved to the
    /// clamp's swung position until the geometry stops changing. (With fixed
    /// far ends the spans are not symmetric once the clamp rises, and the
    /// formula is only approximate: 0.400 vs 0.433 rad — found in testing.)
    #[test]
    fn insulator_swing_matches_closed_form() {
        let (span, l0, ins_len, ins_weight) = (300.0, 300.4, 3.0, 800.0);
        let q = Vector3::new(0.0, 8.0, -W);
        let mut model = Model {
            nodes: vec![
                Node::fixed(Vector3::new(0.0, 0.0, 30.0 - ins_len)),
                Node::fixed(Vector3::new(span, 0.0, 30.0)),
                Node::free(Vector3::new(span, 0.0, 30.0 - ins_len)),
                Node::fixed(Vector3::new(2.0 * span, 0.0, 30.0 - ins_len)),
            ],
            cables: vec![cable(0, 2, l0, q), cable(2, 3, l0, q)],
            trusses: vec![TrussElement {
                i: 1,
                j: 2,
                unstressed_length: ins_len,
                // Realistic composite string: a much stiffer value (1e10 N) raises
                // the floating-point floor of the residual to ~0.03 N (~5 µrad).
                axial_stiffness: 5e7,
                tension_only: true,
                weight: ins_weight,
            }],
        };
        let mut swing = Vector3::zeros();
        let mut angle = f64::NAN;
        // Fixed-point loop with its own exit clause: each pass shrinks the
        // error ~13x; stop when the swing angle is stable (≤ 30 passes).
        for _ in 0..30 {
            let s = model.solve(&default_fe_budget()).unwrap();
            let clamp = s.positions[2];
            swing = clamp - Vector3::new(span, 0.0, 30.0);
            model.nodes[0].position = Vector3::new(0.0, clamp.y, clamp.z);
            model.nodes[3].position = Vector3::new(2.0 * span, clamp.y, clamp.z);
            model.nodes[2].position = clamp;
            let new_angle = swing.y.atan2(-swing.z);
            if (new_angle - angle).abs() < 1e-12 {
                break;
            }
            angle = new_angle;
        }
        // Solver force floor ~1.4e-4 N on ~5.2 kN ≈ 3e-8 rad: 1e-7 rad tolerance.
        let measured = swing.y.atan2(-swing.z);
        let expected = (q.y * l0).atan2(W * l0 + ins_weight / 2.0);
        assert!(
            (measured - expected).abs() < 1e-7,
            "measured {measured}, expected {expected}"
        );
        // The clamp stays in the vertical plane of the structure (symmetry).
        assert!(swing.x.abs() < 1e-9);
    }

    #[test]
    fn broken_span_swings_insulator_toward_remaining_span() {
        // Same structure, right span removed: the clamp swings back.
        let model = Model {
            nodes: vec![
                Node::fixed(Vector3::new(0.0, 0.0, 27.0)),
                Node::fixed(Vector3::new(300.0, 0.0, 30.0)),
                Node::free(Vector3::new(300.0, 0.0, 27.0)),
            ],
            cables: vec![cable(0, 2, 300.4, gravity())],
            trusses: vec![TrussElement {
                i: 1,
                j: 2,
                unstressed_length: 3.0,
                axial_stiffness: 1e10,
                tension_only: true,
                weight: 800.0,
            }],
        };
        let s = model.solve(&default_fe_budget()).unwrap();
        let swing = s.positions[2] - s.positions[1];
        assert!(
            swing.x < -1.0,
            "clamp should swing toward the remaining span: {swing}"
        );
        assert!((swing.norm() - 3.0).abs() < 1e-3);
        // With the clamp pulled back the remaining span slackens.
        assert!(s.cables[0].horizontal < 28_000.0);
    }

    /// 23 heavy point loads from a crude straight-line start (the case that
    /// failed against another program's export): solved, possibly by load stepping.
    #[test]
    fn many_heavy_point_loads_converge() {
        let (span, l0, n) = (560.0, 561.8, 23);
        let mut model = Model {
            nodes: vec![Node::fixed(Vector3::zeros())],
            cables: vec![],
            trusses: vec![],
        };
        for k in 1..=n {
            let x = span * k as f64 / (n + 1) as f64;
            let mut node = Node::free(Vector3::new(x, 0.0, 0.0));
            node.load = Vector3::new(0.0, 0.0, if k % 2 == 1 { -359.0 } else { -30.0 });
            model.nodes.push(node);
        }
        model.nodes.push(Node::fixed(Vector3::new(span, 0.0, 0.0)));
        for k in 0..=n {
            model.cables.push(cable(
                k,
                k + 1,
                l0 / (n + 1) as f64,
                Vector3::new(0.0, 0.0, -10.47),
            ));
        }
        let s = model.solve(&default_fe_budget()).unwrap();
        // Vertical equilibrium: reactions carry cable weight + point loads.
        let total: f64 = s.reactions.iter().map(|r| r.z).sum();
        let expected = 10.47 * l0 + 12.0 * 359.0 + 11.0 * 30.0;
        assert!((total - expected).abs() < 1e-3, "{total} vs {expected}");
    }

    #[test]
    fn reports_non_convergence_with_worst_node() {
        let model = Model {
            nodes: vec![
                Node::fixed(Vector3::zeros()),
                Node::free(Vector3::new(150.0, 0.0, 0.0)),
                Node::fixed(Vector3::new(300.0, 0.0, 0.0)),
            ],
            cables: vec![cable(0, 1, 150.2, gravity()), cable(1, 2, 150.2, gravity())],
            trusses: vec![],
        };
        let mut model = model;
        model.nodes[1].load = Vector3::new(0.0, 0.0, -5_000.0);
        let error = model.solve(&Budget::new().max_iterations(1)).unwrap_err();
        assert!(
            matches!(error, FeError::NotConverged { worst_node: 1, .. }),
            "{error}"
        );
        assert!(model.solve(&default_fe_budget()).is_ok());
    }
}

#[cfg(test)]
mod jacobian_check {
    use super::*;

    /// The assembled stiffness must equal −d(forces)/d(positions) of the
    /// whole model (finite differences), at a swung, loaded state.
    #[test]
    fn assembled_stiffness_matches_finite_differences() {
        let q = Vector3::new(0.0, 8.0, -15.97);
        let props = CableProperties {
            unstressed_length: 300.4,
            axial_stiffness: 34.64e6,
            load: q,
        };
        let swung =
            Vector3::new(300.0, 0.0, 30.0) + Vector3::new(0.1, -0.9, -2.85).normalize() * 3.0003;
        let model = Model {
            nodes: vec![
                Node::fixed(Vector3::new(0.0, 0.0, 27.0)),
                Node::fixed(Vector3::new(300.0, 0.0, 30.0)),
                Node::free(swung),
                Node::fixed(Vector3::new(600.0, 0.0, 27.0)),
            ],
            cables: vec![
                CableElement {
                    i: 0,
                    j: 2,
                    properties: props,
                },
                CableElement {
                    i: 2,
                    j: 3,
                    properties: props,
                },
            ],
            trusses: vec![TrussElement {
                i: 1,
                j: 2,
                unstressed_length: 3.0,
                axial_stiffness: 1e7,
                tension_only: true,
                weight: 800.0,
            }],
        };
        let dof = vec![None, None, Some(0), None];
        let positions: Vec<Vector3<f64>> = model.nodes.iter().map(|n| n.position).collect();
        let base = model.evaluate(&positions, &dof, 3, &mut [None; 2]).unwrap();
        for axis in 0..3 {
            let eps = 1e-6;
            let mut moved = positions.clone();
            moved[2][axis] += eps;
            let e = model.evaluate(&moved, &dof, 3, &mut [None; 2]).unwrap();
            let numeric = -(e.forces[2] - base.forces[2]) / eps;
            for k in 0..3 {
                let analytic = base.stiffness[(k, axis)];
                assert!(
                    (numeric[k] - analytic).abs() <= 1e-4 * base.stiffness.norm(),
                    "d f[{k}] / d x[{axis}]: numeric {:.6e}, analytic {:.6e}",
                    numeric[k],
                    analytic
                );
            }
        }
    }
}
