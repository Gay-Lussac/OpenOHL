// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line solved with flexible structures: the interface loop and
//! calculation level 3 (plan of 2026-11-15, phase 3; in active
//! development — an internal entry point and a command-line flag for
//! testing, no setting, no check uses it yet: every check still takes
//! the rigid solve).
//!
//! Sections and structures keep their own solvers; they meet only at the
//! attachment points of the flexible structures. The unknowns are those
//! points' displacements **u** (three each, line axes, measured from the
//! structures as strung); rigid is u = 0.
//!
//! 1. **Pass 0 is the rigid solve** ([`super::solve_line_with`], bit for
//!    bit): it gives the forces **F** the wires put on the points (the
//!    whole bundle's, unfactored) and the wires' stiffness there
//!    **K_w** (each section's, [`super::SupportStiffness`], summed per
//!    point).
//! 2. **The structures answer** ([`StructureResponse`]): level 3,
//!    u_s = C (F − F_ref) (+ the body loads' deflection, none yet), with C
//!    a structure's flexibility at its points (condensed from its model at
//!    the stringing state) and F_ref the wires' forces at the stringing
//!    state ([`reference_forces`]).
//! 3. **Mismatch** r = u − u_s.
//! 4. **Converged** when every point's mismatch is at most
//!    [`DISPLACEMENT_TOLERANCE`] and every point's force changed by at most
//!    [`FORCE_TOLERANCE`] (or [`RELATIVE_FORCE_TOLERANCE`] of itself) since
//!    the last accepted pass (pass 0: the change K_w r a move by the
//!    mismatch would make).
//! 5. **Newton step** in the flexibility form, which needs no inverse of C:
//!    (I + C K_w) Δu = −r (from r(u) = u − C (F(u) − F_ref) and
//!    dF/du = −K_w). A pass whose mismatch grew takes half the step again
//!    (at most [`LINE_SEARCH_HALVINGS`] times).
//! 6. **Exit clauses**: [`INTERFACE_PASSES`] passes (line solves), a time
//!    limit ([`INTERFACE_TIME_LIMIT`], checked before each pass with the
//!    last pass's time), the budget's cancel token, each section's own
//!    budget. A case that does not converge **fails** with its reason
//!    (owner 2026-11-15) — never the last pass returned as if converged;
//!    the rigid solve is kept with the failure, said rigid.
//!
//! Structures without a model, or refused, stay rigid and are listed
//! ([`RigidStructure`]); sections solved by the ruling span are rigid (no
//! forces of theirs at the points, none moved).

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use nalgebra::{DMatrix, DVector};

use super::*;
use crate::condition::Method;

/// Passes at most (line solves, line-search halvings included; exit
/// clause: level 3 expected in 2–5).
pub const INTERFACE_PASSES: usize = 20;

/// The largest mismatch between a point's displacement and its
/// structure's answer at convergence, m (0.1 mm).
pub const DISPLACEMENT_TOLERANCE: f64 = 1e-4;

/// The largest change of a point's force since the last pass at
/// convergence, N (or [`RELATIVE_FORCE_TOLERANCE`] of the force, the
/// larger).
pub const FORCE_TOLERANCE: f64 = 1.0;

/// [`FORCE_TOLERANCE`] relative to the force.
pub const RELATIVE_FORCE_TOLERANCE: f64 = 1e-6;

/// Times a step whose mismatch grew is halved before it is taken anyway.
pub const LINE_SEARCH_HALVINGS: u32 = 3;

/// Time limit of one case's loop (a guard far above any real case, like
/// the other budgets: a real line's rigid solve takes about a second).
pub const INTERFACE_TIME_LIMIT: Duration = Duration::from_secs(600);

/// Passes the time estimate counts on (level 3 expects 2–5).
pub const EXPECTED_PASSES: u32 = 4;

/// How a line and its structures are computed together (theory:
/// Structure flexibility in the line solve). Levels 1 and 2 are the rigid
/// solves of today (a section's method: ruling span or finite elements);
/// level 4 is not built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalculationLevel {
    /// Structures rigid (calculation levels 1 and 2; the default).
    Rigid,
    /// Level 3: each flexible structure a linear stiffness at its
    /// attachment points.
    Linear,
}

impl CalculationLevel {
    /// The level of number `n` (2 or 3; 1 is a section's method, 4 not
    /// built yet).
    pub fn from_number(n: u8) -> Result<Self, String> {
        match n {
            2 => Ok(Self::Rigid),
            3 => Ok(Self::Linear),
            1 => Err(
                "calculation level 1 (the ruling span) is a section's method \
                      (its calculation), not a run option; give 2 or 3"
                    .into(),
            ),
            4 => Err("calculation level 4 (full structures) is not built yet; give 2 or 3".into()),
            n => Err(format!("calculation level {n}: give 2 or 3")),
        }
    }

    /// Its number.
    pub fn number(self) -> u8 {
        match self {
            Self::Rigid => 2,
            Self::Linear => 3,
        }
    }
}

/// A structure's answer to the wires' forces ([`StructureResponse`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// Each point's displacement (in the order of
    /// [`StructureResponse::labels`]; line axes, m) from the structure as
    /// strung.
    pub displacement: Vec<Point>,
    /// The flexibility at that state, m/N: 3 × points square, line axes
    /// (row 3a + i, column 3b + j: point a's move along i under a unit
    /// force at b along j).
    pub flexibility: DMatrix<f64>,
}

/// What a flexible structure answers in the loop: level 3 a fixed linear
/// matrix ([`LinearStructure`]); level 4 (not built) its non-linear solve.
pub trait StructureResponse: Sync {
    /// The placed structure's id.
    fn structure(&self) -> &str;
    /// Its attachment points (labels).
    fn labels(&self) -> &[String];
    /// The wires' forces on its points at the stringing state (as
    /// `labels`; line axes, N): the state its displacements are measured
    /// from.
    fn reference(&self) -> &[Point];
    /// Its displacements under `forces` on its points (as `labels`; line
    /// axes, N, the whole bundles', unfactored). Err: why it cannot answer
    /// (the case then fails).
    fn answer(&self, forces: &[Point]) -> Result<Answer, String>;
}

/// A structure as a linear stiffness at its attachment points
/// (calculation level 3): u = C (F − F_ref) + u_body.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearStructure {
    /// The placed structure's id.
    pub structure: String,
    /// Its attachment points carrying wires solved by finite elements.
    pub labels: Vec<String>,
    /// Flexibility C, m/N: 3 × labels square, line axes (points on one
    /// joint share their rows).
    pub flexibility: DMatrix<f64>,
    /// F_ref: the wires' forces at the stringing state on each point
    /// (line axes, N).
    pub reference: Vec<Point>,
    /// The deflection of each point under the structure's own wind and
    /// ice of the case (line axes, m): zero — the body loads come with
    /// the structure loads (plan phase 4).
    pub body: Vec<Point>,
    /// The matrix is approximate (the reference solve was; said in
    /// `notes`).
    pub approximate: bool,
    /// What it leaves out or assumes, in words.
    pub notes: Vec<String>,
}

impl StructureResponse for LinearStructure {
    fn structure(&self) -> &str {
        &self.structure
    }

    fn labels(&self) -> &[String] {
        &self.labels
    }

    fn reference(&self) -> &[Point] {
        &self.reference
    }

    fn answer(&self, forces: &[Point]) -> Result<Answer, String> {
        let n = self.labels.len();
        if forces.len() != n || self.flexibility.nrows() != 3 * n || self.reference.len() != n {
            return Err(format!(
                "{}: {} points, {} forces, a {}-row matrix",
                self.structure,
                n,
                forces.len(),
                self.flexibility.nrows()
            ));
        }
        let mut df = DVector::<f64>::zeros(3 * n);
        for (k, (f, r)) in forces.iter().zip(&self.reference).enumerate() {
            for c in 0..3 {
                df[3 * k + c] = f[c] - r[c];
            }
        }
        let u = &self.flexibility * df;
        let displacement = (0..n)
            .map(|k| {
                let b = self.body.get(k).copied().unwrap_or([0.0; 3]);
                [u[3 * k] + b[0], u[3 * k + 1] + b[1], u[3 * k + 2] + b[2]]
            })
            .collect();
        Ok(Answer {
            displacement,
            flexibility: self.flexibility.clone(),
        })
    }
}

/// A structure taken rigid, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct RigidStructure {
    /// The placed structure's id.
    pub structure: String,
    /// Why, in words ("no structure model (detail level 1 or 2)").
    pub reason: String,
}

/// The structures of a line for the loop.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Structures {
    /// The flexible ones.
    pub flexible: Vec<LinearStructure>,
    /// The rigid ones, with why.
    pub rigid: Vec<RigidStructure>,
    /// Notes of the setup (the reference state's sections that failed …).
    pub notes: Vec<String>,
}

/// Exit clauses and tolerances of the loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexibleOptions {
    /// Passes at most ([`INTERFACE_PASSES`]).
    pub passes: usize,
    /// Time limit of a case ([`INTERFACE_TIME_LIMIT`]).
    pub time_limit: Duration,
    /// [`DISPLACEMENT_TOLERANCE`], m.
    pub displacement_tolerance: f64,
    /// [`FORCE_TOLERANCE`], N.
    pub force_tolerance: f64,
    /// [`RELATIVE_FORCE_TOLERANCE`].
    pub relative_force_tolerance: f64,
    /// [`LINE_SEARCH_HALVINGS`].
    pub halvings: u32,
}

impl Default for FlexibleOptions {
    fn default() -> Self {
        Self {
            passes: INTERFACE_PASSES,
            time_limit: INTERFACE_TIME_LIMIT,
            displacement_tolerance: DISPLACEMENT_TOLERANCE,
            force_tolerance: FORCE_TOLERANCE,
            relative_force_tolerance: RELATIVE_FORCE_TOLERANCE,
            halvings: LINE_SEARCH_HALVINGS,
        }
    }
}

/// The wires' forces at the stringing state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReferenceForces {
    /// The force on each structure point (by structure and label; line
    /// axes, N, the whole bundles'), of the sections solved by finite
    /// elements.
    pub forces: BTreeMap<(String, String), Point>,
    /// The sections that did not solve there, with why.
    pub failed: Vec<String>,
    /// The sections solved by the ruling span (no forces at points: rigid
    /// in the loop).
    pub ruling_span: Vec<String>,
    /// How long it took.
    pub elapsed: Duration,
}

/// The wires' forces on the structure points at the stringing state:
/// every section in still air at its stringing temperature, the whole
/// line solved as in any case (jumpers, shared assemblies), once per
/// distinct stringing temperature, each section's forces from the solve
/// at its own — the state the structures are drawn in (geometry as
/// strung, owner 2026-11-15). A case equal to it gives u = 0 exactly.
///
/// Expected cost: one rigid line solve per distinct stringing temperature
/// (usually one).
pub fn reference_forces(
    line: &Line,
    library: &ohl_cables::Library,
    budget: &Budget,
) -> ReferenceForces {
    let started = Instant::now();
    let mut temperatures: Vec<f64> = Vec::new();
    for s in &line.sections {
        let t = s.stringing.temperature();
        if !temperatures.iter().any(|x| x.to_bits() == t.to_bits()) {
            temperatures.push(t);
        }
    }
    let mut out = ReferenceForces::default();
    for t in temperatures {
        let case = WeatherCase::still("stringing", ohl_core::units::Temperature::from_celsius(t));
        let results = solve_line_with(line, library, &case, budget);
        for (section, r) in line.sections.iter().zip(results) {
            if section.stringing.temperature().to_bits() != t.to_bits() {
                continue;
            }
            if section.calculation.method() == Method::Rs {
                out.ruling_span.push(section.id.clone());
                continue;
            }
            match r {
                Ok(shape) => {
                    for (k, supports) in shape.attachment_loads.iter().enumerate() {
                        for s in supports {
                            let e = out
                                .forces
                                .entry((section.points[k].structure.clone(), s.label.clone()))
                                .or_insert([0.0; 3]);
                            for c in 0..3 {
                                e[c] += s.force[c];
                            }
                        }
                    }
                }
                Err(e) => out.failed.push(format!("{}: {e}", section.id)),
            }
        }
    }
    out.elapsed = started.elapsed();
    out
}

/// A point of a flexible structure as solved.
#[derive(Debug, Clone, PartialEq)]
pub struct PointResult {
    /// The structure.
    pub structure: String,
    /// The point's label.
    pub label: String,
    /// Its displacement from the structure as strung, line axes, m.
    pub displacement: Point,
    /// The same in the structure's axes (right of the line, ahead, up), m.
    pub local: Point,
    /// The wires' force on it in the rigid solve (line axes, N, the whole
    /// bundles', unfactored; none: no section solved by finite elements
    /// put one there).
    pub rigid: Option<Point>,
    /// The same in the flexible solve.
    pub flexible: Option<Point>,
}

/// A span of a section, rigid and flexible.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanChange {
    /// The section.
    pub section: String,
    /// The span (0 = the section's first).
    pub span: usize,
    /// Horizontal tension, rigid and flexible, N.
    pub tension: [f64; 2],
    /// Sag below the chord, rigid and flexible, m.
    pub sag: [f64; 2],
    /// Height of the lowest point, rigid and flexible, m.
    pub low: [f64; 2],
}

/// One pass of the loop.
#[derive(Debug, Clone, PartialEq)]
pub struct PassReport {
    /// The largest mismatch of a point, m.
    pub displacement_residual: f64,
    /// The largest change of a point's force since the last accepted pass
    /// (pass 0: of K_w r), N.
    pub force_residual: f64,
    /// Where the mismatch is largest (structure, label).
    pub worst: Option<(String, String)>,
    /// A step taken by half again (its mismatch grew).
    pub halved: bool,
    /// The pass's time.
    pub elapsed: Duration,
}

/// A case solved at calculation level 3.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexibleSolve {
    /// The weather case.
    pub case: String,
    /// The sections as solved with the structures flexible, in the line's
    /// order.
    pub sections: Vec<Result<SectionShape, LineError>>,
    /// The rigid solve (pass 0), the same as [`super::solve_line_with`].
    pub rigid: Vec<Result<SectionShape, LineError>>,
    /// Every point of a flexible structure.
    pub points: Vec<PointResult>,
    /// Every span solved both ways.
    pub spans: Vec<SpanChange>,
    /// The passes.
    pub passes: Vec<PassReport>,
    /// The structures taken rigid, with why.
    pub rigid_structures: Vec<RigidStructure>,
    /// What it assumes and leaves out.
    pub notes: Vec<String>,
    /// Its time.
    pub elapsed: Duration,
    /// Its time as estimated after pass 0 (pass 0's time ×
    /// [`EXPECTED_PASSES`]).
    pub estimate: Duration,
}

/// A case whose loop did not converge (or could not go on): it fails,
/// with why; the rigid solve stays, said rigid.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexibleFailure {
    /// The weather case.
    pub case: String,
    /// Why, in words (the worst point and the trend where it applies).
    pub reason: String,
    /// The passes made.
    pub passes: Vec<PassReport>,
    /// The rigid solve (pass 0; empty when it was not reached).
    pub rigid: Vec<Result<SectionShape, LineError>>,
    /// Its time.
    pub elapsed: Duration,
}

impl std::fmt::Display for FlexibleFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "case \"{}\": calculation level 3 failed: {} (the rigid results stay, said rigid)",
            self.case, self.reason
        )
    }
}

/// The notes of every level-3 result.
pub fn level_three_notes() -> Vec<String> {
    vec![
        "calculation level 3 (in active development): each flexible structure a linear \
         stiffness at its attachment points, linearised at the stringing state (tension-only \
         members and guys as at that state); displacements from the structures as strung"
            .into(),
        "interaction at unfactored loads; a load case's factors go on the resulting forces".into(),
        "NOT ON THE SAFE SIDE for the structures if taken from here: the flexible loads may be \
         lower than rigid — every structure check still uses the rigid solve (the larger of \
         rigid and flexible comes with the structure checks)"
            .into(),
        "NOT ON THE SAFE SIDE for clearances: the deflection already present at stringing, the \
         structures' own wind and ice (not in their deflection yet), rigid foundations in the \
         models, linearisation stiffer than the case's state where members would go slack — \
         all make the points move less than the real structures"
            .into(),
        "the wires' stiffness at the points is approximate (sheaves' sharing, stress-strain \
         secant, loads per stretched metre, jumpers' pull, shared assemblies): it costs passes, \
         never accuracy"
            .into(),
    ]
}

/// Without the support stiffness (asked for the loop only).
fn stripped(results: &[Result<SectionShape, LineError>]) -> Vec<Result<SectionShape, LineError>> {
    results
        .iter()
        .map(|r| {
            r.clone().map(|mut s| {
                s.support_stiffness = None;
                s
            })
        })
        .collect()
}

/// [`solve_line_coupled`] with level 3's linear structures.
pub fn solve_line_flexible(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    structures: &Structures,
    options: &FlexibleOptions,
) -> Result<FlexibleSolve, FlexibleFailure> {
    let responses: Vec<&dyn StructureResponse> = structures
        .flexible
        .iter()
        .map(|s| s as &dyn StructureResponse)
        .collect();
    let mut notes = level_three_notes();
    notes.extend(structures.notes.iter().cloned());
    for s in &structures.flexible {
        for n in &s.notes {
            notes.push(format!("{}: {n}", s.structure));
        }
    }
    solve_line_coupled(
        line,
        library,
        case,
        budget,
        &responses,
        &structures.rigid,
        notes,
        options,
    )
}

/// The line solved in `case` with the structures `responses` flexible
/// (module docs): the interface loop. `rigid`: the structures taken rigid
/// (listed in the result), `notes`: said with the result.
///
/// Expected cost: one rigid line solve per pass (2–5 passes; the
/// estimate after pass 0 is in the result) plus a dense LU of the
/// interface system (3 × flexible points square) per pass.
#[allow(clippy::too_many_arguments)]
pub fn solve_line_coupled(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    responses: &[&dyn StructureResponse],
    rigid: &[RigidStructure],
    mut notes: Vec<String>,
    options: &FlexibleOptions,
) -> Result<FlexibleSolve, FlexibleFailure> {
    let started = Instant::now();
    // The points: per structure its labels, in order.
    let mut points: Vec<(String, String)> = Vec::new();
    let mut owner: Vec<(usize, usize)> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    for (r, s) in responses.iter().enumerate() {
        for (k, label) in s.labels().iter().enumerate() {
            let key = (s.structure().to_owned(), label.clone());
            if index.contains_key(&key) {
                continue;
            }
            index.insert(key.clone(), points.len());
            points.push(key);
            owner.push((r, k));
        }
    }
    let np = points.len();
    let n = 3 * np;
    // Ruling-span sections at flexible structures: rigid, said.
    let flexible_ids: std::collections::BTreeSet<&str> =
        responses.iter().map(|s| s.structure()).collect();
    for s in &line.sections {
        if s.calculation.method() == Method::Rs
            && s.points
                .iter()
                .any(|p| flexible_ids.contains(p.structure.as_str()))
        {
            notes.push(format!(
                "section {} solved by the ruling span: rigid — its points not moved and its \
                 forces' change not in the structures' deflection",
                s.id
            ));
        }
    }
    let solve = |u: &DVector<f64>| {
        let mut moved = PointMoves::new();
        for (i, key) in points.iter().enumerate() {
            let d = [u[3 * i], u[3 * i + 1], u[3 * i + 2]];
            if d != [0.0; 3] {
                moved.insert(key.clone(), d);
            }
        }
        let base = Coupling {
            moved,
            stiffness: true,
            ..Coupling::default()
        };
        solve_line_based(line, library, case, budget, &base)
    };
    // The forces at the points, which points have one, K_w.
    let gather = |results: &[Result<SectionShape, LineError>]| {
        let mut f = DVector::<f64>::zeros(n);
        let mut seen = vec![false; np];
        let mut kw = DMatrix::<f64>::zeros(n, n);
        for (section, r) in line.sections.iter().zip(results) {
            let Ok(shape) = r else { continue };
            for (k, supports) in shape.attachment_loads.iter().enumerate() {
                let Some(point) = section.points.get(k) else {
                    continue;
                };
                for s in supports {
                    if let Some(&i) = index.get(&(point.structure.clone(), s.label.clone())) {
                        for c in 0..3 {
                            f[3 * i + c] += s.force[c];
                        }
                        seen[i] = true;
                    }
                }
            }
            if let Some(k) = &shape.support_stiffness {
                let at: Vec<Option<usize>> = k
                    .points
                    .iter()
                    .map(|p| index.get(&(p.structure.clone(), p.label.clone())).copied())
                    .collect();
                for (a, ia) in at.iter().enumerate() {
                    let Some(ia) = ia else { continue };
                    for (b, ib) in at.iter().enumerate() {
                        let Some(ib) = ib else { continue };
                        for r in 0..3 {
                            for c in 0..3 {
                                kw[(3 * ia + r, 3 * ib + c)] += k.matrix[(3 * a + r, 3 * b + c)];
                            }
                        }
                    }
                }
            }
        }
        (f, seen, kw)
    };
    let local = |i: usize, w: Point| -> Point {
        let orientation = line
            .structure(&points[i].0)
            .map_or(0.0, |s| s.orientation_deg);
        crate::links::to_structure_axes(orientation, w)
    };
    let norm3 = |x: &DVector<f64>, i: usize| -> f64 {
        (x[3 * i].powi(2) + x[3 * i + 1].powi(2) + x[3 * i + 2].powi(2)).sqrt()
    };
    let mut passes: Vec<PassReport> = Vec::new();
    let mut rigid_results: Vec<Result<SectionShape, LineError>> = Vec::new();
    let mut rigid_forces: Option<(DVector<f64>, Vec<bool>)> = None;
    let mut estimate = Duration::ZERO;
    let fail = |reason: String,
                passes: Vec<PassReport>,
                rigid: Vec<Result<SectionShape, LineError>>| FlexibleFailure {
        case: case.name.clone(),
        reason,
        passes,
        rigid,
        elapsed: started.elapsed(),
    };
    let mut u = DVector::<f64>::zeros(n);
    let mut accepted_u = DVector::<f64>::zeros(n);
    let mut accepted_f: Option<DVector<f64>> = None;
    let mut accepted_residual = f64::INFINITY;
    let mut step = DVector::<f64>::zeros(n);
    let mut halved = 0_u32;
    let mut missing_noted = false;
    for pass in 0.. {
        if budget
            .token()
            .is_some_and(ohl_core::budget::CancelToken::is_cancelled)
        {
            return Err(fail(
                format!("cancelled after {pass} passes"),
                passes,
                stripped(&rigid_results),
            ));
        }
        let pass_started = Instant::now();
        let results = solve(&u);
        if pass == 0 {
            rigid_results = stripped(&results);
            for (s, r) in line.sections.iter().zip(&results) {
                if let Err(e) = r {
                    notes.push(format!(
                        "section {} did not solve (rigid either): {e}; its points' forces taken \
                         at the stringing state in the structures' deflection",
                        s.id
                    ));
                }
            }
        } else if let Some((s, e)) = line
            .sections
            .iter()
            .zip(&results)
            .zip(&rigid_results)
            .find_map(|((s, r), r0)| match (r, r0) {
                (Err(e), Ok(_)) => Some((s, e)),
                _ => None,
            })
        {
            return Err(fail(
                format!(
                    "section {} did not solve with its points moved in pass {pass}: {e}",
                    s.id
                ),
                passes,
                rigid_results,
            ));
        }
        let (mut f, seen, kw) = gather(&results);
        if pass == 0 {
            rigid_forces = Some((f.clone(), seen.clone()));
        }
        // A point no section put a force on (its section failed in pass 0):
        // its force as at the stringing state.
        for (i, &(r, k)) in owner.iter().enumerate() {
            if !seen[i] {
                let reference = responses[r].reference()[k];
                for c in 0..3 {
                    f[3 * i + c] = reference[c];
                }
                if !missing_noted {
                    missing_noted = true;
                    notes.push(format!(
                        "{} {}: no force from a section solved by finite elements; taken as \
                         at the stringing state",
                        points[i].0, points[i].1
                    ));
                }
            }
        }
        // The structures' answers, C block by block.
        let mut us = DVector::<f64>::zeros(n);
        let mut c = DMatrix::<f64>::zeros(n, n);
        for (r, s) in responses.iter().enumerate() {
            let ids: Vec<Option<usize>> = s
                .labels()
                .iter()
                .map(|l| index.get(&(s.structure().to_owned(), l.clone())).copied())
                .collect();
            let forces: Vec<Point> = ids
                .iter()
                .enumerate()
                .map(|(k, i)| match i {
                    Some(i) if owner[*i] == (r, k) => [f[3 * i], f[3 * i + 1], f[3 * i + 2]],
                    _ => s.reference()[k],
                })
                .collect();
            let answer = match s.answer(&forces) {
                Ok(a) => a,
                Err(e) => {
                    return Err(fail(
                        format!("structure {} did not answer: {e}", s.structure()),
                        passes,
                        rigid_results,
                    ));
                }
            };
            for (a, ia) in ids.iter().enumerate() {
                let Some(ia) = *ia else { continue };
                if owner[ia] != (r, a) {
                    continue;
                }
                for k in 0..3 {
                    us[3 * ia + k] = answer.displacement[a][k];
                }
                for (b, ib) in ids.iter().enumerate() {
                    let Some(ib) = *ib else { continue };
                    if owner[ib] != (r, b) {
                        continue;
                    }
                    for i in 0..3 {
                        for j in 0..3 {
                            c[(3 * ia + i, 3 * ib + j)] =
                                answer.flexibility[(3 * a + i, 3 * b + j)];
                        }
                    }
                }
            }
        }
        let r = &u - &us;
        let (mut residual, mut worst) = (0.0_f64, None);
        for i in 0..np {
            let x = norm3(&r, i);
            if x > residual {
                (residual, worst) = (x, Some(i));
            }
        }
        // The forces' change since the last accepted pass (pass 0: the
        // change K_w r a move by the mismatch would make).
        let change = match &accepted_f {
            Some(prev) => &f - prev,
            None => &kw * &r,
        };
        let mut force_residual = 0.0_f64;
        let mut force_ok = true;
        for i in 0..np {
            let d = norm3(&change, i);
            force_residual = force_residual.max(d);
            if d > options
                .force_tolerance
                .max(options.relative_force_tolerance * norm3(&f, i))
            {
                force_ok = false;
            }
        }
        let grew = pass > 0 && residual > accepted_residual;
        let pass_time = pass_started.elapsed();
        passes.push(PassReport {
            displacement_residual: residual,
            force_residual,
            worst: worst.map(|i| points[i].clone()),
            halved: false,
            elapsed: pass_time,
        });
        if pass == 0 {
            estimate = pass_time * EXPECTED_PASSES;
        }
        if residual <= options.displacement_tolerance && force_ok {
            // Converged.
            let (f0, seen0) = rigid_forces.take().unwrap_or_default();
            let point_results = (0..np)
                .map(|i| {
                    let d = [u[3 * i], u[3 * i + 1], u[3 * i + 2]];
                    PointResult {
                        structure: points[i].0.clone(),
                        label: points[i].1.clone(),
                        displacement: d,
                        local: local(i, d),
                        rigid: seen0
                            .get(i)
                            .copied()
                            .unwrap_or(false)
                            .then(|| [f0[3 * i], f0[3 * i + 1], f0[3 * i + 2]]),
                        flexible: seen[i].then(|| [f[3 * i], f[3 * i + 1], f[3 * i + 2]]),
                    }
                })
                .collect();
            let sections = stripped(&results);
            let mut spans = Vec::new();
            for (a, b) in rigid_results.iter().zip(&sections) {
                let (Ok(a), Ok(b)) = (a, b) else { continue };
                for (s, (x, y)) in a.spans.iter().zip(&b.spans).enumerate() {
                    spans.push(SpanChange {
                        section: a.section.clone(),
                        span: s,
                        tension: [x.horizontal_tension, y.horizontal_tension],
                        sag: [x.sag, y.sag],
                        low: [x.low_point[2], y.low_point[2]],
                    });
                }
            }
            return Ok(FlexibleSolve {
                case: case.name.clone(),
                sections,
                rigid: rigid_results,
                points: point_results,
                spans,
                passes,
                rigid_structures: rigid.to_vec(),
                notes,
                elapsed: started.elapsed(),
                estimate,
            });
        }
        // Exit clauses.
        let worst_text = |passes: &[PassReport]| -> String {
            let Some(i) = worst else {
                return String::new();
            };
            let d = local(i, [r[3 * i], r[3 * i + 1], r[3 * i + 2]]);
            let axis = (0..3)
                .max_by(|&a, &b| d[a].abs().total_cmp(&d[b].abs()))
                .unwrap_or(0);
            let trend = match passes {
                [.., a, b] if a.displacement_residual > 0.0 => {
                    let q = b.displacement_residual / a.displacement_residual;
                    if q < 1.0 {
                        format!(", decreasing {q:.2}× a pass")
                    } else {
                        format!(", growing {q:.2}× a pass")
                    }
                }
                _ => String::new(),
            };
            format!(
                "; mismatch still {:.1} mm at {} {} ({}){trend}",
                residual * 1e3,
                points[i].0,
                points[i].1,
                ["across the line", "along the line", "vertical"][axis]
            )
        };
        if pass + 1 >= options.passes {
            return Err(fail(
                format!(
                    "not converged in {} passes{}",
                    options.passes,
                    worst_text(&passes)
                ),
                passes,
                rigid_results,
            ));
        }
        if started.elapsed() + pass_time > options.time_limit {
            return Err(fail(
                format!(
                    "stopped by the time limit ({:.0?}) after {} passes{}",
                    options.time_limit,
                    pass + 1,
                    worst_text(&passes)
                ),
                passes,
                rigid_results,
            ));
        }
        // A step whose mismatch grew: half of it again.
        if grew && halved < options.halvings {
            halved += 1;
            step *= 0.5;
            u = &accepted_u + &step;
            if let Some(last) = passes.last_mut() {
                last.halved = true;
            }
            continue;
        }
        accepted_u = u.clone();
        accepted_f = Some(f);
        accepted_residual = residual;
        halved = 0;
        // Newton: (I + C K_w) Δu = −r.
        let a = DMatrix::<f64>::identity(n, n) + &c * &kw;
        let Some(delta) = a.lu().solve(&(-&r)) else {
            return Err(fail(
                format!(
                    "the interface system is singular in pass {pass}{}",
                    worst_text(&passes)
                ),
                passes,
                rigid_results,
            ));
        };
        if delta.iter().any(|x| !x.is_finite()) {
            return Err(fail(
                format!("the Newton step is not finite in pass {pass}"),
                passes,
                rigid_results,
            ));
        }
        step = delta;
        u = &accepted_u + &step;
    }
    unreachable!("the loop returns")
}
