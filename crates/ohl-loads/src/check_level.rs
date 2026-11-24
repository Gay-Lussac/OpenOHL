// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's calculation level in the checks of weather cases (plan of
//! 2026-11-15, phase 5; in active development): the wire tensions and
//! the string checks solve the line at the level of its setting
//! ([`ohl_line::SolverOptions::structures`], or a run's own).
//!
//! At level 3 the structures are set up once per run
//! ([`prepare_check_level`]): the wires' forces at the stringing state
//! (each section by its own method, in its initial condition, in still
//! air at its stringing temperature — the structures as drawn are the
//! structures as strung), each placed structure condensed there, and its
//! points' deflection under its own wind and ice of each weather case
//! (unfactored, as a weather case on its own takes it,
//! [`LoadCase::from_weather`]). Each case is then solved by the interface
//! loop ([`ohl_line::solve::flexible::solve_line_flexible`]), whose pass 0
//! is the rigid solve: a check has both solves of every case and keeps,
//! per value, the worse of the two (owner 2026-11-15: never less safe
//! than rigid). A case whose loop does not converge is checked rigid
//! only, said.
//!
//! At level 4 the setup adds each flexible structure's full model
//! ([`crate::full`]) and the loop goes on from level 3's solution with
//! the structures solved in full in every pass (phase 6).

use std::time::Duration;

use ohl_core::budget::Budget;
use ohl_line::condition::{Calculation, Condition, Method};
use ohl_line::solve::SectionShape;
use ohl_line::solve::flexible::{CalculationLevel, FlexibleOptions, FlexibleSolve};
use ohl_line::{Line, LineError};
use ohl_weather::WeatherCase;

use crate::{LoadCase, LoadsSetup, WEATHER_CASE_PREFIX};

/// A line's structures set up for its checks at a calculation level
/// ([`prepare_check_level`]).
#[derive(Debug, Clone, PartialEq)]
pub struct CheckLevel {
    /// The level asked.
    pub level: CalculationLevel,
    /// The structures at level 3 or 4 (flexible with their matrices and
    /// their own wind and ice per weather case, at level 4 their full
    /// models; rigid with why); none at level 2.
    pub setup: Option<LoadsSetup>,
    /// Each section's method in the setup (by id): a check solving a
    /// section by another method is solved rigid ([`CheckLevel::refuses`]).
    methods: Vec<(String, Method)>,
    /// What the results say of the line's reference state (plan of
    /// 2026-11-15, phase 5b; none: no reference case).
    reference: Option<String>,
}

impl CheckLevel {
    /// Calculation level 2: every case solved rigid alone.
    pub fn rigid() -> Self {
        Self {
            level: CalculationLevel::Rigid,
            setup: None,
            methods: Vec::new(),
            reference: None,
        }
    }

    /// Calculation level 2 on `line`: every case solved rigid alone, the
    /// structures as deflected in its reference case when that state is
    /// in use ([`ohl_line::reference`]), said in the notes.
    pub fn rigid_on(line: &Line) -> Self {
        Self {
            reference: line.reference_note(),
            ..Self::rigid()
        }
    }

    /// Why `line` (a check's line: its rule's calculation, its local
    /// conditions) is not solved at this level (none: it is). The setup's
    /// forces at the stringing state are those of each section by its own
    /// method; a check solving a section by another (a rule's ruling
    /// span on a section of finite elements, or the other way round)
    /// would read a deflection that is not there.
    pub fn refuses(&self, line: &Line) -> Option<String> {
        self.setup.as_ref()?;
        let other: Vec<&str> = line
            .sections
            .iter()
            .filter(|s| {
                self.methods
                    .iter()
                    .find(|(id, _)| *id == s.id)
                    .is_some_and(|(_, m)| *m != s.calculation.method())
            })
            .map(|s| s.id.as_str())
            .collect();
        (!other.is_empty()).then(|| {
            format!(
                "solved rigid (calculation level 2): the rule's calculation solves {} by another \
                 method than the line (its structures' stiffness is set up for the line's own)",
                if other.len() > 3 {
                    format!("{} sections", other.len())
                } else {
                    format!("section(s) {}", other.join(", "))
                }
            )
        })
    }

    /// `line` solved in weather case `case` at this level: the rigid solve
    /// alone (level 2, or refused: [`CheckLevel::refuses`]), or the
    /// interface loop with the structures flexible, its pass 0 the rigid
    /// solve (level 3; level 4 from level 3's solution with the structures'
    /// full models). `budget`: each section's.
    ///
    /// Expected cost: level 2 one line solve; level 3 2–5 (the loop's
    /// passes); level 4 2–5 more, each with a full solve per flexible
    /// structure (side by side).
    pub fn solve(
        &self,
        line: &Line,
        library: &ohl_cables::Library,
        case: &WeatherCase,
        budget: &Budget,
    ) -> LevelSolve {
        let rigid = |why: Option<String>| LevelSolve {
            case: case.name.clone(),
            rigid: ohl_line::solve::solve_line_with(line, library, case, budget),
            level: why.map(Err),
        };
        let Some(setup) = &self.setup else {
            return rigid(None);
        };
        // Each case its own deflection from the structures as strung (its
        // rigid solve too): a reference state in use set aside.
        let drawn = line.as_drawn(ohl_line::solve::flexible::STRUNG);
        let line = &*drawn;
        let rigid = |why: Option<String>| LevelSolve {
            case: case.name.clone(),
            rigid: ohl_line::solve::solve_line_with(line, library, case, budget),
            level: why.map(Err),
        };
        if let Some(why) = self.refuses(line) {
            return rigid(Some(why));
        }
        let structures = setup.structures_for(&format!("{WEATHER_CASE_PREFIX}{}", case.name));
        match ohl_line::solve::flexible::solve_line_flexible(
            line,
            library,
            case,
            budget,
            &structures,
            &FlexibleOptions::default(),
        ) {
            Ok(s) => LevelSolve {
                case: case.name.clone(),
                rigid: s.rigid.clone(),
                level: Some(Ok(s)),
            },
            Err(f) if f.rigid.is_empty() => rigid(Some(f.to_string())),
            Err(f) => LevelSolve {
                case: case.name.clone(),
                rigid: f.rigid.clone(),
                level: Some(Err(f.to_string())),
            },
        }
    }

    /// The header of a check's results at this level (plan §6.3): the
    /// level, the reference state and the linearisation, the structures
    /// taken rigid with why, the cases solved, their passes and time, the
    /// largest move, the cases that failed, the rule of the worse of rigid
    /// and flexible, and what is left out (NOT ON THE SAFE SIDE where it
    /// is). `tally`: the check's solves; `change`: the check's largest
    /// change of its governing value against rigid, in words. Empty at
    /// level 2.
    pub fn notes(&self, tally: &LevelTally, change: Option<String>) -> Vec<String> {
        let Some(setup) = &self.setup else {
            // Level 2: what the reference state does (none set: nothing).
            return self.reference.iter().cloned().collect();
        };
        let s = &setup.setup.structures;
        let n = self.level.number();
        let mut out: Vec<String> = self
            .reference
            .iter()
            .map(|_| {
                format!(
                    "the line's reference case not used at calculation level {n}: each case \
                     finds its own deflection from the structures as strung (the reference \
                     state is the coupled solve of its case; its rigid solve as drawn)"
                )
            })
            .collect();
        out.push(
            "calculation level 3 (in active development): each structure with a model (or a pole \
             with an elasticity) a linear stiffness at its attachment points, linearised at the \
             stringing state (tension-only members and guys as at that state), its own wind and \
             ice of the case in its deflection (unfactored); reference: the structures as drawn \
             are the structures as strung (displacements from the stringing state)"
                .to_owned(),
        );
        if self.level == CalculationLevel::Full {
            out.push(
                "calculation level 4 (in active development): from level 3's solution, each \
                 flexible structure solved in full in every pass (its own P-Delta, tension-only \
                 members and guys, stabilisation; its own wind and ice of the case, unfactored), \
                 the structures side by side; a structure whose full solve fails in a case taken \
                 linear (level 3) there and said"
                    .to_owned(),
            );
            if let Some(full) = &setup.full {
                for (id, why) in full.linear_only() {
                    out.push(format!(
                        "{id}: no full model ({why}) — taken linear (level 3)"
                    ));
                }
            }
        }
        out.push(format!(
            "never less safe than rigid (owner 2026-11-15): every value is checked on the rigid \
             solve and on the level-{n} solve of its case, the worse kept"
        ));
        let mut rigid: Vec<String> = s
            .rigid
            .iter()
            .take(10)
            .map(|r| format!("{} ({})", r.structure, r.reason))
            .collect();
        if s.rigid.len() > 10 {
            rigid.push(format!("and {} more", s.rigid.len() - 10));
        }
        out.push(format!(
            "{} structure(s) flexible{}",
            s.flexible.len(),
            if rigid.is_empty() {
                String::new()
            } else {
                format!("; taken rigid: {}", rigid.join("; "))
            }
        ));
        let rs = &setup.setup.reference.ruling_span;
        if !rs.is_empty() {
            out.push(format!(
                "sections solved by the ruling span rigid at level 3 (their forces still load the \
                 structures): {}",
                rs.join(", ")
            ));
        }
        out.extend(s.notes.iter().cloned());
        out.push(format!(
            "level {n}: set up in {:.1?} ({} condensations{}); {} case(s) solved at level {n} in \
             {:.1?}, at most {} pass(es){}",
            setup.setup.elapsed,
            setup.setup.condensed,
            setup.full.as_ref().map_or(String::new(), |f| format!(
                ", the full models in {:.1?}",
                f.0.elapsed
            )),
            tally.cases,
            tally.elapsed,
            tally.passes,
            tally
                .moved
                .as_ref()
                .map_or(String::new(), |(d, at)| format!(
                    "; largest move {:.1} mm at {at}",
                    d * 1e3
                ))
        ));
        if let Some(c) = change {
            // The checks' envelopes name the coupled solve "level 3".
            let c = if self.level == CalculationLevel::Full {
                c.replace("level 3", "level 4")
            } else {
                c
            };
            out.push(format!("change against rigid: {c}"));
        }
        if let Some(c) = &tally.from_level3 {
            out.push(format!("level 4 {c}"));
        }
        for f in &tally.full_failed {
            out.push(f.clone());
        }
        for f in &tally.failed {
            out.push(format!("{f} — that case checked on the rigid solve only"));
        }
        out.push(if self.level == CalculationLevel::Full {
            "NOT ON THE SAFE SIDE where a check takes the level-4 solve: the deflection already \
             present at stringing left out, rigid foundations in the models, P-Delta on the drawn \
             geometry (no large displacements) — the points may move less than the real \
             structures, so a clearance can be smaller than shown; keep a margin there (these \
             parts are not modelled yet)"
                .to_owned()
        } else {
            "NOT ON THE SAFE SIDE where a check takes the level-3 solve: the deflection already \
             present at stringing left out, rigid foundations in the models, the linearisation \
             stiffer than the case's state where members would go slack — the points move less \
             than the real structures, so a clearance can be smaller than shown; level 4 \
             follows slack members, keep a margin for the rest (not modelled yet)"
                .to_owned()
        });
        out
    }
}

/// `line`'s structures set up for its checks in the weather cases
/// `weather` at calculation level `level`: nothing at level 2; at level 3
/// the wires' forces at the stringing state — each section by its own
/// method in its initial condition, in still air at its stringing
/// temperature — then per placed structure its condensation there and its
/// points' deflection under its own wind and ice of each weather case
/// (unfactored, [`LoadCase::from_weather`]); the structures over the
/// cores, `done` told after each (structures done, of), `cancel` stopping
/// it. Kept for every case of the run ([`CheckLevel::solve`]).
///
/// Expected cost: one rigid line solve, then one condensation per placed
/// structure (7 ms for the example's lattice tower, 0.5–1.2 s for real
/// towers of 2000 members), over the cores.
#[allow(clippy::too_many_arguments)]
pub fn prepare_check_level(
    line: &Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
    level: CalculationLevel,
    budget: &Budget,
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> CheckLevel {
    if level == CalculationLevel::Rigid {
        return CheckLevel::rigid_on(line);
    }
    let mut strung = line.clone();
    for s in &mut strung.sections {
        s.calculation = Calculation::of(Condition::Initial, s.calculation.method());
    }
    let cases: Vec<LoadCase> = weather
        .iter()
        .map(|w| LoadCase::from_weather(&w.name))
        .collect();
    let setup = crate::flexibility::prepare_strung(
        &strung, library, types, sections, weather, &cases, level, budget, done, cancel,
    );
    CheckLevel {
        level,
        setup: Some(setup),
        methods: line
            .sections
            .iter()
            .map(|s| (s.id.clone(), s.calculation.method()))
            .collect(),
        reference: line.reference.as_ref().map(|r| r.case.clone()),
    }
}

/// A weather case solved at a calculation level ([`CheckLevel::solve`]).
#[derive(Debug, Clone, PartialEq)]
pub struct LevelSolve {
    /// The weather case.
    pub case: String,
    /// The rigid solve, every section (the loop's pass 0 at level 3).
    pub rigid: Vec<Result<SectionShape, LineError>>,
    /// The level-3 solve; Err: why the case has none (its loop did not
    /// converge, or the check's calculation is not the line's); none at
    /// level 2.
    pub level: Option<Result<FlexibleSolve, String>>,
}

impl LevelSolve {
    /// The level-3 solve's sections, when the case has one.
    pub fn flexible(&self) -> Option<&[Result<SectionShape, LineError>]> {
        match &self.level {
            Some(Ok(s)) => Some(&s.sections),
            _ => None,
        }
    }
}

/// What a check's solves at level 3 took ([`CheckLevel::notes`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelTally {
    /// Cases solved at level 3.
    pub cases: usize,
    /// The cases without a level-3 solve, with why.
    pub failed: Vec<String>,
    /// The most passes of a case.
    pub passes: usize,
    /// The loops' time, summed.
    pub elapsed: Duration,
    /// Their rigid solves' time (each loop's pass 0), summed.
    pub rigid_elapsed: Duration,
    /// The largest move of a point (m) and where ("T2 A1 in case X").
    pub moved: Option<(f64, String)>,
    /// Level 4: the largest change from level 3's solution, in words (the
    /// case's with the largest move change).
    pub from_level3: Option<String>,
    /// Level 4: the structures that failed at level 4, per case, in words.
    pub full_failed: Vec<String>,
}

impl LevelTally {
    /// Counts `solve` in.
    pub fn add(&mut self, solve: &LevelSolve) {
        match &solve.level {
            None => {}
            Some(Err(why)) => {
                let line = if why.starts_with("case \"") {
                    why.clone()
                } else {
                    format!("case \"{}\": {why}", solve.case)
                };
                if !self.failed.contains(&line) {
                    self.failed.push(line);
                }
            }
            Some(Ok(s)) => {
                self.cases += 1;
                self.passes = self.passes.max(s.passes.len());
                self.elapsed += s.elapsed;
                self.rigid_elapsed += s.passes.first().map(|p| p.elapsed).unwrap_or_default();
                for p in &s.points {
                    let d = p.displacement.iter().map(|v| v * v).sum::<f64>().sqrt();
                    if self.moved.as_ref().is_none_or(|(m, _)| d > *m) {
                        self.moved =
                            Some((d, format!("{} {} in {}", p.structure, p.label, s.case)));
                    }
                }
                if let Some(c) = s.change_from_level3()
                    && self.from_level3.is_none()
                {
                    self.from_level3 = Some(format!("in case \"{}\" {c}", s.case));
                }
                for f in &s.full_failed {
                    let line = format!(
                        "{} failed at calculation level 4 in case \"{}\": {} — taken linear \
                         (level 3) there",
                        f.structure, s.case, f.reason
                    );
                    if !self.full_failed.contains(&line) {
                        self.full_failed.push(line);
                    }
                }
            }
        }
    }
}
