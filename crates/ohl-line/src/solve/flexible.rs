// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line solved with flexible structures: the interface loop and
//! calculation levels 3 and 4 (plan of 2026-11-15, phases 3–6; in active
//! development). The line's setting is [`crate::SolverOptions::structures`];
//! the structure loads, the wire tensions, the string checks and the
//! loads sent to OHL-Structure take it (through `ohl-loads`), the other
//! checks stay rigid and say so ([`crate::rigid_check_note`]).
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
//!    state ([`reference_forces`]); level 4 ([`FullAnswer`]), each
//!    structure solved in full under F and its own loads, less its solve
//!    under F_ref, C its tangent at that solution condensed — side by
//!    side across the structures, each from the last pass's tension-only
//!    set.
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
//!
//! **Level 4** runs the same loop in two stages ([`solve_line_staged`]):
//! the linear structures first (level 3, to its convergence), then, from
//! that state and without another line solve, the full answers; a
//! structure whose full solve fails is taken linear for the rest of the
//! case and listed failing ([`FlexibleSolve::full_failed`]); a Newton step
//! that cannot be taken (the interface system singular, the step not
//! finite, or its line search spent) falls back to staggered passes
//! relaxed by Aitken's factor (Irons and Tuck 1969). Its passes have
//! their own count ([`INTERFACE_PASSES`]).

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

/// Why the coupled solve sets a reference state aside
/// ([`Line::as_drawn`]): it finds each case's own deflection.
pub const STRUNG: &str = "the coupled solve finds each case's own deflection from the \
    structures as strung";

/// How a line and its structures are computed together (theory:
/// Structure flexibility in the line solve). Levels 1 and 2 are the rigid
/// solves of today (a section's method: ruling span or finite elements);
/// level 4 the structures' full non-linear models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalculationLevel {
    /// Structures rigid (calculation levels 1 and 2; the default).
    Rigid,
    /// Level 3: each flexible structure a linear stiffness at its
    /// attachment points.
    Linear,
    /// Level 4: each flexible structure its full non-linear model in
    /// every pass (P-Delta, tension-only members and guys), from level
    /// 3's solution.
    Full,
}

impl CalculationLevel {
    /// The level of number `n` (2, 3 or 4; 1 is a section's method).
    pub fn from_number(n: u8) -> Result<Self, String> {
        match n {
            2 => Ok(Self::Rigid),
            3 => Ok(Self::Linear),
            4 => Ok(Self::Full),
            1 => Err(
                "calculation level 1 (the ruling span) is a section's method \
                      (its calculation), not a run option; give 2, 3 or 4"
                    .into(),
            ),
            n => Err(format!("calculation level {n}: give 2, 3 or 4")),
        }
    }

    /// Its number.
    pub fn number(self) -> u8 {
        match self {
            Self::Rigid => 2,
            Self::Linear => 3,
            Self::Full => 4,
        }
    }

    /// The structures flexible (level 3 or 4).
    pub fn flexible(self) -> bool {
        self != Self::Rigid
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
/// matrix ([`LinearStructure`]); level 4 its non-linear solve
/// ([`FullAnswer`]).
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
/// (calculation level 3): u = C (F − F_ref) + u_body (+ u_feet, its
/// measured foot movements).
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
    /// The move of each point by the structure's measured foot movements
    /// (line axes, m; owner 2026-11-16, [`crate::feet`]): the structure as
    /// built from its plans and moved at its feet — its rigid-body motion
    /// and elastic distortion, in every case alike. Empty: none.
    pub feet: Vec<Point>,
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
                let mut d = [u[3 * k] + b[0], u[3 * k + 1] + b[1], u[3 * k + 2] + b[2]];
                if let Some(f) = self.feet.get(k) {
                    for (a, b) in d.iter_mut().zip(f) {
                        *a += b;
                    }
                }
                d
            })
            .collect();
        Ok(Answer {
            displacement,
            flexibility: self.flexibility.clone(),
        })
    }
}

/// A structure's full non-linear answer in one case's loop (calculation
/// level 4): [`StructureResponse::answer`] solves it under the forces
/// (its tangent at the solution as the flexibility), from the last
/// answer's tension-only set; when its solve fails it answers as its
/// linear structure (level 3) from then on in this case and says why
/// ([`FullAnswer::failed`]).
pub trait FullAnswer: StructureResponse {
    /// Why its full solve failed in this case (taken linear since); none:
    /// it did not.
    fn failed(&self) -> Option<String>;
    /// Its full solves in this case and their time.
    fn solves(&self) -> (usize, Duration);
    /// The time its next full solve is expected to take (its last one's;
    /// zero: unknown or none): the structures of a pass are started the
    /// longest first, so that no long one is left alone at its end
    /// (2026-11-17).
    fn expected(&self) -> Duration {
        Duration::ZERO
    }
}

/// Where a level-4 loop is ([`FullProvider::progress`]).
#[derive(Debug, Clone, PartialEq)]
pub struct LoopProgress {
    /// The weather case.
    pub case: String,
    /// The line solves so far.
    pub pass: usize,
    /// The level of the answers (3 or 4).
    pub level: u8,
    /// Structures answered in this pass, of `of`.
    pub done: usize,
    /// The structures of the pass.
    pub of: usize,
    /// The largest mismatch of the last pass, m (none: none yet).
    pub mismatch: Option<f64>,
}

/// The full structures of a line for one case (calculation level 4):
/// fresh answers per solve (their warm starts and failures are the
/// case's), in the order of the linear structures they fall back on.
pub trait FullProvider: Send + Sync {
    /// The answers for one solve, one per structure of `linear` (in its
    /// order).
    fn responses<'a>(&'a self, linear: &'a [LinearStructure]) -> Vec<Box<dyn FullAnswer + 'a>>;
    /// Told after each structure's answer at level 4 (a progress bar);
    /// nothing by default.
    fn progress(&self, _at: &LoopProgress) {}
    /// What it assumes and leaves out, in words (said with the level's
    /// notes).
    fn notes(&self) -> Vec<String> {
        Vec::new()
    }
}

/// [`Structures::full`]: a shared [`FullProvider`] (equal when the same
/// one).
#[derive(Clone)]
pub struct FullModels(pub std::sync::Arc<dyn FullProvider>);

impl std::fmt::Debug for FullModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FullModels")
    }
}

impl PartialEq for FullModels {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
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
    /// The flexible structures' own wind and ice of the case are in their
    /// deflection ([`LinearStructure::body`]; the structure loads and the
    /// checks at the line's level, plan §4.6); false: left out, said NOT
    /// ON THE SAFE SIDE for leeward clearances.
    pub body_loads: bool,
    /// Calculation level 4: the flexible structures' full models (none:
    /// level 3, the linear structures alone).
    pub full: Option<FullModels>,
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
    // On the structures as strung: a reference state's lean is not the
    // stringing's ([`crate::reference`]).
    let drawn = line.as_drawn(STRUNG);
    let line = &*drawn;
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
                            for (x, f) in e.iter_mut().zip(s.force) {
                                *x += f;
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
    /// The level of the structures' answers (3 or 4).
    pub level: u8,
    /// A line solve (false: the full answers taken at level 3's
    /// solution, no line solve).
    pub solved: bool,
    /// A staggered pass (level 4's fallback when the Newton step could not
    /// be taken), relaxed by this factor.
    pub relaxed: Option<f64>,
}

/// A case solved at calculation level 3 or 4.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexibleSolve {
    /// The weather case.
    pub case: String,
    /// The level (3 or 4).
    pub level: CalculationLevel,
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
    /// Level 4: every point at level 3's solution, where the full
    /// answers started (empty at level 3).
    pub level3: Vec<PointResult>,
    /// Level 4: the structures whose full solve failed in this case, with
    /// why — taken linear (level 3) for the rest of it, reported failing.
    pub full_failed: Vec<RigidStructure>,
    /// Level 4: the structures' full solves and their time (summed over
    /// the structures; they run side by side).
    pub structure_solves: (usize, Duration),
}

impl FlexibleSolve {
    /// The line solves made (the passes that solved the line).
    pub fn line_solves(&self) -> usize {
        self.passes.iter().filter(|p| p.solved).count()
    }

    /// Level 4: the largest change of a point's move and of a point's
    /// force from level 3's solution, in words; none at level 3.
    pub fn change_from_level3(&self) -> Option<String> {
        if self.level3.is_empty() {
            return None;
        }
        let (mut moved, mut at_m) = (0.0_f64, String::new());
        let (mut force, mut at_f) = (0.0_f64, String::new());
        for (a, b) in self.level3.iter().zip(&self.points) {
            let d = (0..3)
                .map(|k| (b.displacement[k] - a.displacement[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            if d > moved {
                (moved, at_m) = (d, format!(" ({} {})", b.structure, b.label));
            }
            if let (Some(x), Some(y)) = (a.flexible, b.flexible) {
                let size = x.iter().map(|v| v * v).sum::<f64>().sqrt();
                let d = (0..3).map(|k| (y[k] - x[k]).powi(2)).sum::<f64>().sqrt();
                let share = if size > 1.0 { d / size } else { 0.0 };
                if share > force {
                    (force, at_f) = (share, format!(" ({} {})", b.structure, b.label));
                }
            }
        }
        Some(format!(
            "against level 3: the points' moves differ by up to {:.1} mm{at_m}, their forces by \
             up to {:.2} %{at_f}",
            moved * 1e3,
            force * 100.0,
        ))
    }
}

/// A case whose loop did not converge (or could not go on): it fails,
/// with why; the rigid solve stays, said rigid.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexibleFailure {
    /// The weather case.
    pub case: String,
    /// The level asked (3 or 4).
    pub level: u8,
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
            "case \"{}\": calculation level {} failed: {} (the rigid results stay, said rigid)",
            self.case, self.level, self.reason
        )
    }
}

/// The notes of every level-3 result (`body`: the structures' own wind
/// and ice of the case in their deflection, [`Structures::body_loads`]).
pub fn level_three_notes(body: bool) -> Vec<String> {
    let mut notes: Vec<String> = vec![
        "calculation level 3 (in active development): each flexible structure a linear \
         stiffness at its attachment points, linearised at the stringing state (tension-only \
         members and guys as at that state); displacements from the structures as strung"
            .into(),
        "interaction at unfactored loads; a load case's factors go on the resulting forces".into(),
        "NOT ON THE SAFE SIDE for the structures if taken from here: the flexible loads may be \
         lower than rigid — the structure loads at level 3 keep, per point, the larger of rigid \
         and flexible"
            .into(),
        format!(
            "NOT ON THE SAFE SIDE for clearances: the deflection already present at stringing, \
             {}rigid foundations in the models, linearisation stiffer than the case's state \
             where members would go slack — all make the points move less than the real \
             structures, so a clearance can be smaller than shown; level 4 follows slack \
             members, keep a margin for the rest (not modelled yet)",
            if body {
                ""
            } else {
                "the structures' own wind and ice (left out of their deflection here), "
            }
        ),
        "the wires' stiffness at the points is approximate (sheaves' sharing, stress-strain \
         secant, loads per stretched metre, jumpers' pull, shared assemblies): it costs passes, \
         never accuracy"
            .into(),
    ];
    if body {
        notes.insert(
            1,
            "each flexible structure's own wind and ice of the case in its deflection \
             (unfactored, on the same linear stiffness)"
                .into(),
        );
    }
    notes
}

/// The notes of every level-4 result (`body`: the structures' own wind
/// and ice of the case in their solves, [`Structures::body_loads`]).
pub fn level_four_notes(body: bool) -> Vec<String> {
    let mut notes: Vec<String> = vec![
        "calculation level 4 (in active development): each flexible structure solved in full in \
         every pass (its own P-Delta, stabilisation, tension-only members and guys by their \
         active set) under the wires' forces at its attachment points, its displacement from its \
         solve under the stringing state's forces (the structures as drawn are the structures as \
         strung); started from level 3's solution, the Newton step on each structure's tangent \
         at its solution"
            .into(),
        "interaction at unfactored loads; a load case's factors go on the resulting forces".into(),
        "NOT ON THE SAFE SIDE for the structures if taken from here: the flexible loads may be \
         lower than rigid — the structure loads at level 4 keep, per point, the larger of rigid \
         and flexible"
            .into(),
        format!(
            "NOT ON THE SAFE SIDE for clearances: the deflection already present at stringing, \
             {}rigid foundations in the models, P-Delta on the drawn geometry (no large \
             displacements: a very flexible structure's large rotations are not followed) — the \
             points may move less than the real structures, so a clearance can be smaller than \
             shown; keep a margin there (these parts are not modelled yet)",
            if body {
                ""
            } else {
                "the structures' own wind and ice (left out of their solves here), "
            }
        ),
        "a structure whose full solve fails in a case is taken linear (level 3) there and \
         reported failing in it; a case whose loop does not converge fails"
            .into(),
        "the wires' stiffness at the points is approximate (sheaves' sharing, stress-strain \
         secant, loads per stretched metre, jumpers' pull, shared assemblies): it costs passes, \
         never accuracy"
            .into(),
    ];
    if body {
        notes.insert(
            1,
            "each flexible structure's own wind and ice of the case in its solves (unfactored)"
                .into(),
        );
    }
    notes
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

/// [`solve_line_coupled`] with level 3's linear structures — level 4 when
/// `structures` has their full models ([`Structures::full`]:
/// [`solve_line_staged`]).
pub fn solve_line_flexible(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    structures: &Structures,
    options: &FlexibleOptions,
) -> Result<FlexibleSolve, FlexibleFailure> {
    solve_line_flexible_broken(line, library, case, budget, structures, options, &[])
}

/// [`solve_line_flexible`] with the wires of `broken` broken (per
/// section, its spans: [`super::solve_line_breaks`]) in every pass: a
/// broken wire as a whole-line solve, the structures next to the break
/// held back by the other wires on them (plan §6.2). Pass 0 is
/// [`super::solve_line_breaks`] bit for bit. Err: as there (a section not
/// in the line), the case failed in pass 0.
pub fn solve_line_flexible_broken(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    structures: &Structures,
    options: &FlexibleOptions,
    broken: &[(&str, Vec<Break>)],
) -> Result<FlexibleSolve, FlexibleFailure> {
    let responses: Vec<&dyn StructureResponse> = structures
        .flexible
        .iter()
        .map(|s| s as &dyn StructureResponse)
        .collect();
    let mut notes = match &structures.full {
        Some(full) => {
            let mut n = level_four_notes(structures.body_loads);
            n.extend(full.0.notes());
            n
        }
        None => level_three_notes(structures.body_loads),
    };
    notes.extend(structures.notes.iter().cloned());
    for s in &structures.flexible {
        for n in &s.notes {
            notes.push(format!("{}: {n}", s.structure));
        }
    }
    let answers = structures
        .full
        .as_ref()
        .map(|f| f.0.responses(&structures.flexible));
    let full: Option<Vec<&dyn FullAnswer>> = answers
        .as_ref()
        .map(|a| a.iter().map(|b| b.as_ref()).collect());
    let stage = full.as_deref().map(|answers| FullStage {
        answers,
        provider: structures.full.as_ref().map(|f| &*f.0),
    });
    solve_line_staged(
        line,
        library,
        case,
        budget,
        &responses,
        stage,
        &structures.rigid,
        notes,
        options,
        broken,
    )
}

/// The line solved in `case` with the structures `responses` flexible
/// (module docs): the interface loop. `rigid`: the structures taken rigid
/// (listed in the result), `notes`: said with the result.
///
/// Expected cost: one rigid line solve per pass (2–5 passes; the
/// estimate after pass 0 is in the result) plus a banded elimination of
/// the interface system (3 × flexible points square, within its band)
/// per pass.
#[allow(clippy::too_many_arguments)]
pub fn solve_line_coupled(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    responses: &[&dyn StructureResponse],
    rigid: &[RigidStructure],
    notes: Vec<String>,
    options: &FlexibleOptions,
    broken: &[(&str, Vec<Break>)],
) -> Result<FlexibleSolve, FlexibleFailure> {
    solve_line_staged(
        line, library, case, budget, responses, None, rigid, notes, options, broken,
    )
}

/// Calculation level 4's stage of the loop ([`solve_line_staged`]): the
/// full answers, one per linear structure (in its order), and who is told
/// the progress.
#[derive(Clone, Copy)]
pub struct FullStage<'a> {
    /// The full answers.
    pub answers: &'a [&'a dyn FullAnswer],
    /// Told the progress (none: nobody).
    pub provider: Option<&'a dyn FullProvider>,
}

/// The structures' answers of a pass: their displacements u_s and C block
/// by block (its rows in the unknowns, its matrix).
type Answered = (DVector<f64>, Vec<(Vec<usize>, DMatrix<f64>)>);

/// The first relaxation factor of level 4's staggered passes (Aitken's
/// then, within [`RELAXATION_LIMITS`]).
pub const RELAXATION_START: f64 = 0.5;

/// The least and the largest relaxation factor of the staggered passes.
pub const RELAXATION_LIMITS: [f64; 2] = [0.05, 1.0];

/// [`solve_line_coupled`] in stages: the linear structures `responses`
/// to their convergence (level 3; with `full` none, exactly
/// [`solve_line_coupled`]), then — level 4 — the full answers of `full`
/// from that state: answered at the same line solve first (no new one),
/// then the loop goes on with them, their tangents in the Newton step,
/// the structures answered side by side; [`INTERFACE_PASSES`] for each
/// stage. A full answer that fails answers linear from then on (listed in
/// [`FlexibleSolve::full_failed`]). Where the Newton step of the full
/// stage cannot be taken (the interface system singular, the step not
/// finite, its line search spent with the mismatch still growing), the
/// passes go on staggered: u ← u − ω r, ω by Aitken's rule
/// (ω_k = −ω_{k−1} r_{k−1}·(r_k − r_{k−1}) / |r_k − r_{k−1}|²,
/// within [`RELAXATION_LIMITS`]) — said in the notes.
///
/// Expected cost: level 3's, then per full pass one line solve and one
/// full solve of each flexible structure (side by side over the cores)
/// with a condensation of its tangent; 2–5 passes expected.
#[allow(clippy::too_many_arguments)]
pub fn solve_line_staged(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    responses: &[&dyn StructureResponse],
    full: Option<FullStage<'_>>,
    rigid: &[RigidStructure],
    mut notes: Vec<String>,
    options: &FlexibleOptions,
    broken: &[(&str, Vec<Break>)],
) -> Result<FlexibleSolve, FlexibleFailure> {
    let started = Instant::now();
    let asked = if full.is_some() { 4 } else { 3 };
    // Each case its own deflection from the structures as strung: a
    // reference state in use is set aside ([`crate::reference`]).
    let drawn = line.as_drawn(STRUNG);
    let line = &*drawn;
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
    // Every pass solves as given: the wire solver itself works about a
    // local origin far from the origin ([`ohl_wirefe::Model::solve`]), so
    // pass 0 and the passes with moved points round alike.
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
            strung: true,
            stiffness: true,
            ..Coupling::default()
        };
        if broken.is_empty() {
            Ok(solve_line_based(line, library, case, budget, &base))
        } else {
            solve_line_breaks_based(line, library, case, broken, budget, &base)
        }
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
    let cancelled = || {
        budget
            .token()
            .is_some_and(ohl_core::budget::CancelToken::is_cancelled)
    };
    // The structures' answers to the forces `f` (the full ones at level
    // 4, side by side), C block by block (its rows in the unknowns, its
    // matrix). Err: a structure that did not answer, in words.
    let answer_all = |at_full: bool,
                      f: &DVector<f64>,
                      pass: usize,
                      mismatch: Option<f64>|
     -> Result<Answered, String> {
        let ids_of = |r: usize| -> Vec<Option<usize>> {
            let s = responses[r];
            s.labels()
                .iter()
                .map(|l| index.get(&(s.structure().to_owned(), l.clone())).copied())
                .collect()
        };
        let forces_of = |r: usize, ids: &[Option<usize>]| -> Vec<Point> {
            ids.iter()
                .enumerate()
                .map(|(k, i)| match i {
                    Some(i) if owner[*i] == (r, k) => [f[3 * i], f[3 * i + 1], f[3 * i + 2]],
                    _ => responses[r].reference()[k],
                })
                .collect()
        };
        let answers: Vec<Result<Answer, String>> = match full.filter(|_| at_full) {
            Some(stage) => {
                let done = std::sync::atomic::AtomicUsize::new(0);
                let of = stage.answers.len();
                // The longest first (else in their order); each answer is
                // its own, so the order changes none.
                let mut items: Vec<usize> = (0..of).collect();
                items.sort_by_key(|&r| std::cmp::Reverse(stage.answers[r].expected()));
                let answered = crate::jumper::in_parallel_each(&items, |&r| {
                    if cancelled() {
                        return Err("cancelled".to_owned());
                    }
                    let a = stage.answers[r].answer(&forces_of(r, &ids_of(r)));
                    let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if let Some(p) = stage.provider {
                        p.progress(&LoopProgress {
                            case: case.name.clone(),
                            pass: pass + 1,
                            level: 4,
                            done: d,
                            of,
                            mismatch,
                        });
                    }
                    a
                });
                // Back in the structures' order.
                let mut answers: Vec<Option<Result<Answer, String>>> =
                    (0..of).map(|_| None).collect();
                for (&r, a) in items.iter().zip(answered) {
                    answers[r] = Some(a);
                }
                answers
                    .into_iter()
                    .map(|a| a.unwrap_or_else(|| Err("not answered".to_owned())))
                    .collect()
            }
            None => (0..responses.len())
                .map(|r| responses[r].answer(&forces_of(r, &ids_of(r))))
                .collect(),
        };
        let mut us = DVector::<f64>::zeros(n);
        let mut blocks: Vec<(Vec<usize>, DMatrix<f64>)> = Vec::with_capacity(responses.len());
        for (r, answer) in answers.into_iter().enumerate() {
            let s = responses[r];
            let answer = answer.map_err(|e| {
                if e == "cancelled" {
                    e
                } else {
                    format!("structure {} did not answer: {e}", s.structure())
                }
            })?;
            let ids = ids_of(r);
            // Its points the loop owns (a label given twice: once).
            let own: Vec<(usize, usize)> = ids
                .iter()
                .enumerate()
                .filter_map(|(a, ia)| ia.filter(|&ia| owner[ia] == (r, a)).map(|ia| (a, ia)))
                .collect();
            let mut rows = Vec::with_capacity(3 * own.len());
            let mut block = DMatrix::<f64>::zeros(3 * own.len(), 3 * own.len());
            for (p, &(a, ia)) in own.iter().enumerate() {
                for k in 0..3 {
                    us[3 * ia + k] = answer.displacement[a][k];
                    rows.push(3 * ia + k);
                }
                for (q, &(b, _)) in own.iter().enumerate() {
                    for i in 0..3 {
                        for j in 0..3 {
                            block[(3 * p + i, 3 * q + j)] =
                                answer.flexibility[(3 * a + i, 3 * b + j)];
                        }
                    }
                }
            }
            blocks.push((rows, block));
        }
        Ok((us, blocks))
    };
    let mut passes: Vec<PassReport> = Vec::new();
    let mut rigid_results: Vec<Result<SectionShape, LineError>> = Vec::new();
    let mut rigid_forces: Option<(DVector<f64>, Vec<bool>)> = None;
    let mut estimate = Duration::ZERO;
    let fail = |reason: String,
                passes: Vec<PassReport>,
                rigid: Vec<Result<SectionShape, LineError>>| FlexibleFailure {
        case: case.name.clone(),
        level: asked,
        reason,
        passes,
        rigid,
        elapsed: started.elapsed(),
    };
    // Every point at `u` with the forces `f` (`seen`: which have one).
    let point_results = |u: &DVector<f64>,
                         f: &DVector<f64>,
                         seen: &[bool],
                         rigid: Option<&(DVector<f64>, Vec<bool>)>|
     -> Vec<PointResult> {
        (0..np)
            .map(|i| {
                let d = [u[3 * i], u[3 * i + 1], u[3 * i + 2]];
                PointResult {
                    structure: points[i].0.clone(),
                    label: points[i].1.clone(),
                    displacement: d,
                    local: local(i, d),
                    rigid: rigid.and_then(|(f0, seen0)| {
                        seen0
                            .get(i)
                            .copied()
                            .unwrap_or(false)
                            .then(|| [f0[3 * i], f0[3 * i + 1], f0[3 * i + 2]])
                    }),
                    flexible: seen[i].then(|| [f[3 * i], f[3 * i + 1], f[3 * i + 2]]),
                }
            })
            .collect()
    };
    let mut u = DVector::<f64>::zeros(n);
    let mut accepted_u = DVector::<f64>::zeros(n);
    let mut accepted_f: Option<DVector<f64>> = None;
    let mut accepted_residual = f64::INFINITY;
    let mut step = DVector::<f64>::zeros(n);
    let mut halved = 0_u32;
    let mut missing_noted = false;
    // Level 4's stage: on, from which pass; level 3's points there; the
    // staggered fallback and its relaxation.
    let mut at_full = false;
    let mut stage_start = 0_usize;
    let mut level3: Vec<PointResult> = Vec::new();
    let mut staggered = false;
    let mut omega = RELAXATION_START;
    let mut last_r: Option<DVector<f64>> = None;
    for pass in 0.. {
        if cancelled() {
            return Err(fail(
                format!("cancelled after {pass} passes"),
                passes,
                stripped(&rigid_results),
            ));
        }
        if let Some(p) = full.and_then(|s| s.provider) {
            p.progress(&LoopProgress {
                case: case.name.clone(),
                pass: pass + 1,
                level: if at_full { 4 } else { 3 },
                done: 0,
                of: responses.len(),
                mismatch: passes.last().map(|p| p.displacement_residual),
            });
        }
        let pass_started = Instant::now();
        let results = match solve(&u) {
            Ok(r) => r,
            Err(e) => {
                return Err(fail(
                    format!("the broken wires: {e}"),
                    passes,
                    rigid_results,
                ));
            }
        };
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
            // How far the points were moved (the step that led there).
            let (mut most, mut at) = (0.0_f64, None);
            for i in 0..np {
                let x = norm3(&u, i);
                if x > most {
                    (most, at) = (x, Some(i));
                }
            }
            let moved = at.map_or(String::new(), |i| {
                format!(
                    " (the points moved by up to {:.1} mm, most at {} {})",
                    most * 1e3,
                    points[i].0,
                    points[i].1
                )
            });
            return Err(fail(
                format!(
                    "section {} did not solve with its points moved in pass {pass}{moved}: {e}",
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
        // A point no section put a force on: where a section at its
        // structure failed in pass 0, its force as at the stringing state;
        // else no wire holds it in this case (removed): no force.
        let failed_at: std::collections::BTreeSet<&str> = line
            .sections
            .iter()
            .zip(&rigid_results)
            .filter(|(_, r)| r.is_err())
            .flat_map(|(s, _)| s.points.iter().map(|p| p.structure.as_str()))
            .collect();
        for (i, &(r, k)) in owner.iter().enumerate() {
            if !seen[i] {
                if !failed_at.contains(points[i].0.as_str()) {
                    for c in 0..3 {
                        f[3 * i + c] = 0.0;
                    }
                    continue;
                }
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
        // The answers to this line solve: once, or (level 4) again with
        // the full answers where level 3 has converged.
        let mut solved_now = true;
        let (r, blocks, residual, worst, grew) = loop {
            let answered = Instant::now();
            let (us, blocks) = match answer_all(
                at_full,
                &f,
                pass,
                passes.last().map(|p| p.displacement_residual),
            ) {
                Ok(x) => x,
                Err(e) if e == "cancelled" => {
                    return Err(fail(
                        format!("cancelled in pass {}", pass + 1),
                        passes,
                        rigid_results,
                    ));
                }
                Err(e) => return Err(fail(e, passes, rigid_results)),
            };
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
            let pass_time = if solved_now {
                pass_started.elapsed()
            } else {
                answered.elapsed()
            };
            passes.push(PassReport {
                displacement_residual: residual,
                force_residual,
                worst: worst.map(|i| points[i].clone()),
                halved: false,
                elapsed: pass_time,
                level: if at_full { 4 } else { 3 },
                solved: solved_now,
                relaxed: None,
            });
            if pass == 0 && solved_now {
                estimate = pass_time * EXPECTED_PASSES;
            }
            if residual <= options.displacement_tolerance && force_ok {
                if !at_full && full.is_some() {
                    // Level 3 converged: the full answers from here, at
                    // this line solve.
                    level3 = point_results(&u, &f, &seen, rigid_forces.as_ref());
                    at_full = true;
                    stage_start = pass;
                    accepted_residual = f64::INFINITY;
                    halved = 0;
                    solved_now = false;
                    continue;
                }
                // Converged.
                let point_results = point_results(&u, &f, &seen, rigid_forces.as_ref());
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
                let mut full_failed = Vec::new();
                let mut structure_solves = (0, Duration::ZERO);
                if let Some(stage) = full {
                    for a in stage.answers {
                        if let Some(why) = a.failed() {
                            notes.push(format!(
                                "{} failed at calculation level 4 in case \"{}\": {why} — taken \
                                 linear (level 3) for the rest of the case; reported failing in \
                                 it",
                                a.structure(),
                                case.name
                            ));
                            full_failed.push(RigidStructure {
                                structure: a.structure().to_owned(),
                                reason: why,
                            });
                        }
                        let (k, t) = a.solves();
                        structure_solves.0 += k;
                        structure_solves.1 += t;
                    }
                }
                return Ok(FlexibleSolve {
                    case: case.name.clone(),
                    level: if full.is_some() {
                        CalculationLevel::Full
                    } else {
                        CalculationLevel::Linear
                    },
                    sections,
                    rigid: rigid_results,
                    points: point_results,
                    spans,
                    passes,
                    rigid_structures: rigid.to_vec(),
                    notes,
                    elapsed: started.elapsed(),
                    estimate,
                    level3,
                    full_failed,
                    structure_solves,
                });
            }
            break (r, blocks, residual, worst, grew);
        };
        let pass_time = pass_started.elapsed();
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
        if pass + 1 - stage_start >= options.passes {
            return Err(fail(
                format!(
                    "not converged in {} passes{}{}",
                    options.passes,
                    if at_full { " at level 4" } else { "" },
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
        if grew && at_full && !staggered {
            staggered = true;
            notes.push(format!(
                "level 4, case \"{}\": the Newton step did not lower the mismatch after {} \
                 halvings (pass {}): staggered passes from there, relaxed by Aitken's factor",
                case.name,
                options.halvings,
                pass + 1
            ));
        }
        accepted_u = u.clone();
        accepted_f = Some(f);
        accepted_residual = residual;
        halved = 0;
        let delta = if staggered {
            None
        } else {
            // Newton: (I + C K_w) Δu = −r, C block by block.
            let mut a = DMatrix::<f64>::identity(n, n);
            for (rows, block) in &blocks {
                add_block_product(&mut a, block, &kw, rows);
            }
            match solve_banded(a, -&r) {
                Some(delta) if delta.iter().all(|x| x.is_finite()) => Some(delta),
                failed if !at_full => {
                    let why = if failed.is_none() {
                        format!("the interface system is singular in pass {pass}")
                    } else {
                        format!("the Newton step is not finite in pass {pass}")
                    };
                    return Err(fail(
                        if failed.is_none() {
                            format!("{why}{}", worst_text(&passes))
                        } else {
                            why
                        },
                        passes,
                        rigid_results,
                    ));
                }
                _ => {
                    staggered = true;
                    notes.push(format!(
                        "level 4, case \"{}\": the Newton step could not be taken in pass {} \
                         (the interface system singular, or the step not finite): staggered \
                         passes from there, relaxed by Aitken's factor",
                        case.name,
                        pass + 1
                    ));
                    None
                }
            }
        };
        step = match delta {
            Some(delta) => delta,
            None => {
                // Staggered: u − ω r, ω by Aitken's rule.
                if let Some(prev) = &last_r {
                    let dr = &r - prev;
                    let den = dr.norm_squared();
                    if den > 0.0 {
                        omega = (-omega * prev.dot(&dr) / den)
                            .clamp(RELAXATION_LIMITS[0], RELAXATION_LIMITS[1]);
                    }
                }
                last_r = Some(r.clone());
                if let Some(last) = passes.last_mut() {
                    last.relaxed = Some(omega);
                }
                -&r * omega
            }
        };
        u = &accepted_u + &step;
    }
    unreachable!("the loop returns")
}

/// Adds C·K_w[rows, ..] (`block` C, the rows `rows` of K_w) to the rows
/// `rows` of `a` — each entry of `a` gets it from one block only.
///
/// Only K_w's columns with an entry on `rows` are multiplied (2026-11-17:
/// a structure's points are tied by wires to a few others; the product of
/// C with the whole width cost a fifth of a second per pass on a line of
/// 4 400 unknowns): a column of K_w that is zero there gives a zero column
/// of the product, which adds nothing to `a` (each entry of `a` is 1 or 0
/// before its block's term: x + (±0) = x). The product of the columns
/// alone is the whole product's columns bit for bit (the matrix product
/// forms each column from its own column, the same sums whatever the
/// width — `the_block_product_is_the_whole_one`), taken the same way: at
/// least [`GEMM_SMALL`] + 1 columns where the whole product has more rows
/// than that (the matrix library multiplies small matrices otherwise), and
/// the whole width when C has an entry that is not finite (∞·0 is not 0).
fn add_block_product(
    a: &mut DMatrix<f64>,
    block: &DMatrix<f64>,
    kw: &DMatrix<f64>,
    rows: &[usize],
) {
    let n = kw.ncols();
    let mut cols: Vec<usize> = if block.iter().all(|x| x.is_finite()) {
        (0..n)
            .filter(|&col| {
                let c = kw.column(col);
                rows.iter().any(|&row| c[row] != 0.0)
            })
            .collect()
    } else {
        (0..n).collect()
    };
    if rows.len() > GEMM_SMALL && cols.len() <= GEMM_SMALL {
        // As many columns as the whole product's path needs (zero ones:
        // they add nothing).
        let more: Vec<usize> = (0..n)
            .filter(|c| !cols.contains(c))
            .take(GEMM_SMALL + 1 - cols.len())
            .collect();
        cols.extend(more);
        cols.sort_unstable();
    }
    if cols.is_empty() {
        return;
    }
    let mut kr = DMatrix::<f64>::zeros(rows.len(), cols.len());
    for (k, &col) in cols.iter().enumerate() {
        let from = kw.column(col);
        let mut to = kr.column_mut(k);
        for (i, &row) in rows.iter().enumerate() {
            to[i] = from[row];
        }
    }
    let product = block * kr;
    for (k, &col) in cols.iter().enumerate() {
        let from = product.column(k);
        let mut to = a.column_mut(col);
        for (i, &row) in rows.iter().enumerate() {
            to[row] += from[i];
        }
    }
}

/// The largest dimension at which the matrix library (nalgebra 0.35's
/// `gemm`) multiplies two matrices by its own loops rather than by the
/// blocked product; [`add_block_product`] keeps each product on the path
/// the whole one takes.
const GEMM_SMALL: usize = 5;

/// Solves A x = b by Gaussian elimination with partial pivoting within
/// A's band (its lower and upper bandwidths found from its entries: the
/// interface system couples only the points of structures a section
/// reaches, banded along the line), the dense LU when the band is wide.
/// None: singular. Cost n · kl · (kl + ku) against the dense n³.
fn solve_banded(mut a: DMatrix<f64>, mut b: DVector<f64>) -> Option<DVector<f64>> {
    let n = a.nrows();
    let (mut kl, mut ku) = (0_usize, 0_usize);
    for j in 0..n {
        for i in 0..n {
            if a[(i, j)] != 0.0 {
                if i > j {
                    kl = kl.max(i - j);
                } else {
                    ku = ku.max(j - i);
                }
            }
        }
    }
    if 3 * (kl + ku) >= n {
        return a.lu().solve(&b);
    }
    // With row exchanges the upper band grows to kl + ku.
    let upper = kl + ku;
    let mut factors = vec![0.0; kl + 1];
    for j in 0..n {
        let last = (j + kl).min(n - 1);
        let (p, size) = (j..=last)
            .map(|i| (i, a[(i, j)].abs()))
            .fold((j, -1.0), |best, x| if x.1 > best.1 { x } else { best });
        // Zero, or NaN (a NaN pivot is never taken).
        if size <= 0.0 || !size.is_finite() {
            return None;
        }
        let end = (j + upper).min(n - 1);
        if p != j {
            for c in j..=end {
                a.swap((p, c), (j, c));
            }
            b.swap_rows(p, j);
        }
        let pivot = a[(j, j)];
        for i in j + 1..=last {
            factors[i - j] = a[(i, j)] / pivot;
            a[(i, j)] = 0.0;
            b[i] -= factors[i - j] * b[j];
        }
        for c in j + 1..=end {
            let ajc = a[(j, c)];
            if ajc == 0.0 {
                continue;
            }
            // Down the column (its entries one after the other in memory:
            // the same operations, side by side).
            let mut column = a.column_mut(c);
            for (x, f) in column.as_mut_slice()[j + 1..=last]
                .iter_mut()
                .zip(&factors[1..])
            {
                *x -= f * ajc;
            }
        }
    }
    let mut x = DVector::<f64>::zeros(n);
    for j in (0..n).rev() {
        let end = (j + upper).min(n - 1);
        let mut s = b[j];
        for c in j + 1..=end {
            s -= a[(j, c)] * x[c];
        }
        x[j] = s / a[(j, j)];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod band_tests {
    use super::*;

    /// The banded elimination gives the dense LU's answer on a banded
    /// unsymmetric matrix that needs row exchanges (a zero on the
    /// diagonal), and refuses a singular one.
    #[test]
    fn the_banded_solve_is_the_dense_one() {
        let n = 40;
        let mut a = DMatrix::<f64>::zeros(n, n);
        for i in 0..n {
            for j in i.saturating_sub(2)..(i + 4).min(n) {
                let x = ((i * 7 + j * 13) % 11) as f64 - 5.0;
                a[(i, j)] = x + if i == j { 0.5 } else { 0.0 };
            }
        }
        a[(3, 3)] = 0.0;
        let b = DVector::from_fn(n, |i, _| (i as f64).sin());
        let dense = a.clone().lu().solve(&b).unwrap();
        let banded = solve_banded(a.clone(), b.clone()).unwrap();
        assert!(
            (&dense - &banded).amax() < 1e-9 * dense.amax(),
            "{dense} {banded}"
        );
        let mut singular = a;
        for i in 0..n {
            singular[(i, 5)] = 0.0;
        }
        assert!(solve_banded(singular, b).is_none());
    }

    /// The interface matrix built from the columns of K_w a block reaches
    /// ([`add_block_product`]) is the one built with the whole width (the
    /// way before 2026-11-17), bit for bit: blocks of 3 to 102 rows,
    /// together or apart, K_w reaching none, a few (fewer than the matrix
    /// library's small size, and more) or many columns, a block with an
    /// entry that is not finite.
    #[test]
    fn the_block_product_is_the_whole_one() {
        let n = 240;
        let mut seed = 0x853c_49e6_748f_ea9b_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        };
        for (case, (size, reach)) in [
            (3, 2),
            (3, 40),
            (6, 0),
            (6, 3),
            (6, 6),
            (9, 5),
            (54, 4),
            (54, 60),
            (102, 7),
            (102, 200),
        ]
        .into_iter()
        .enumerate()
        {
            let start = (case * 17) % (n - size);
            let rows: Vec<usize> = if case % 2 == 0 {
                (start..start + size).collect()
            } else {
                (0..size).map(|k| (k * 7 + case) % n).collect()
            };
            let mut block = DMatrix::<f64>::from_fn(size, size, |_, _| next());
            if case == 9 {
                block[(1, 2)] = f64::INFINITY;
            }
            let mut kw = DMatrix::<f64>::zeros(n, n);
            for c in 0..reach {
                let col = (c * 13 + case) % n;
                for &row in &rows {
                    kw[(row, col)] = next();
                }
            }
            // The whole width, row by row.
            let mut whole = DMatrix::<f64>::identity(n, n);
            let mut kr = DMatrix::<f64>::zeros(rows.len(), n);
            for (k, &row) in rows.iter().enumerate() {
                kr.row_mut(k).copy_from(&kw.row(row));
            }
            let product = &block * kr;
            for (k, &row) in rows.iter().enumerate() {
                for col in 0..n {
                    whole[(row, col)] += product[(k, col)];
                }
            }
            let mut cut = DMatrix::<f64>::identity(n, n);
            add_block_product(&mut cut, &block, &kw, &rows);
            let bits = |m: &DMatrix<f64>| m.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&cut), bits(&whole), "case {case}");
        }
    }
}
