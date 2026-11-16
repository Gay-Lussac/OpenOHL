// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's calculation level in OHL-Design (plan of 2026-11-15, phase
//! 5; in active development): Line › Criteria › Calculation options'
//! **Structures in the wire solve** — rigid (level 2), linear stiffness
//! (level 3), full structures (level 4, not built yet: shown, not
//! offered) — and what each choice costs and changes on this computer
//! (owner: every modelling setting shows its effect on results and its
//! time).
//!
//! The checks that take the level (Wire tensions, String checks, Structure
//! loads and the loads sent to OHL-Structure) record each run here
//! ([`record`]): its level, its time, the time of its rigid solves and the
//! effect it measured against rigid. The last run of each level is kept
//! with the application, for every project ([`load`], [`save`]); the
//! window shows it beside each choice, else an estimate from the last
//! rigid run.

use std::collections::BTreeMap;
use std::sync::Mutex;

use eframe::egui;
use serde::{Deserialize, Serialize};

/// The key in the application's storage.
const KEY: &str = "calculation.level-runs";

/// A run at a calculation level on this computer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LevelRun {
    /// What ran ("Wire tensions, 4 cases").
    pub what: String,
    /// The line's structures and sections ("12 structures, 9 sections").
    pub line: String,
    /// The run's time, s.
    pub seconds: f64,
    /// The time of the rigid solves in it, s (at level 3: the loops'
    /// pass 0 and the structures' setup apart).
    #[serde(default)]
    pub rigid_seconds: Option<f64>,
    /// The structures' setup at level 3, s.
    #[serde(default)]
    pub setup_seconds: Option<f64>,
    /// What it changed against rigid, in words (level 3).
    #[serde(default)]
    pub effect: Option<String>,
    /// The structures it took rigid, with why (level 3).
    #[serde(default)]
    pub rigid: Vec<String>,
}

/// The last run of each level (2, 3), on this computer.
static RUNS: Mutex<BTreeMap<u8, LevelRun>> = Mutex::new(BTreeMap::new());

/// Records `run` as the last at level `level` (2 or 3).
pub fn record(level: u8, run: LevelRun) {
    RUNS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(level, run);
}

/// The last run at level `level` on this computer.
pub fn last(level: u8) -> Option<LevelRun> {
    RUNS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&level)
        .cloned()
}

/// The runs kept with the application.
pub fn load(storage: &dyn eframe::Storage) {
    if let Some(runs) = storage
        .get_string(KEY)
        .and_then(|t| serde_json::from_str::<BTreeMap<u8, LevelRun>>(&t).ok())
    {
        *RUNS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = runs;
    }
}

/// Writes the runs.
pub fn save(storage: &mut dyn eframe::Storage) {
    let runs = RUNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Ok(t) = serde_json::to_string(&runs) {
        storage.set_string(KEY, t);
    }
}

/// "12 structures, 9 sections" of `line`.
pub fn line_size(line: &ohl_line::Line) -> String {
    format!(
        "{} structures, {} sections",
        line.structures.len(),
        line.sections.len()
    )
}

/// The structures of `line` that stay rigid at level 3 whatever the run,
/// with why, read from the files (no model is solved): no type, a type
/// not in the library, a type with neither a structure model nor a pole.
/// A run may take more rigid (a model refused at the stringing state),
/// said in its results.
pub fn rigid_from_files(
    line: &ohl_line::Line,
    types: Option<&ohl_structures::StructureLibrary>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for s in &line.structures {
        if seen.contains(&s.id.as_str()) {
            continue;
        }
        seen.push(&s.id);
        let why = match (&s.structure_type, types) {
            (None, _) => Some("no structure type".to_owned()),
            (Some(t), None) => Some(format!("its type {t} is not in a structure library")),
            (Some(t), Some(lib)) => match lib.get(t) {
                None => Some(format!("its type {t} is not in the structure library")),
                Some(t) if t.model.is_none() && !t.members_designed() && t.pole.is_none() => {
                    Some(format!(
                        "type {} has no structure model (detail level 1 or 2) and no pole",
                        t.id
                    ))
                }
                Some(_) => None,
            },
        };
        if let Some(why) = why {
            out.push(format!("{} ({why})", s.id));
        }
    }
    out
}

/// The time and effect shown beside choice `choice` for `line`: the last
/// run at its level on this computer, else an estimate from the last
/// rigid run.
pub fn beside(choice: ohl_line::StructuresInSolve) -> String {
    use ohl_line::StructuresInSolve as S;
    let passes = f64::from(ohl_line::solve::flexible::EXPECTED_PASSES);
    match choice {
        S::Rigid => match last(2) {
            Some(r) => format!(
                "Last run here: {} in {:.1} s ({}).",
                r.what, r.seconds, r.line
            ),
            None => "Time: one line solve per case (no run measured here yet).".to_owned(),
        },
        S::Linear => match (last(3), last(2)) {
            (Some(r), _) => {
                let mut t = format!(
                    "Last run here: {} in {:.1} s ({}{}{}).",
                    r.what,
                    r.seconds,
                    r.line,
                    r.setup_seconds.map_or(String::new(), |s| format!(
                        "; structures set up in {s:.1} s"
                    )),
                    r.rigid_seconds
                        .filter(|s| *s > 0.0)
                        .map_or(String::new(), |s| format!(
                            "; {:.1}× its rigid solves",
                            r.seconds / s
                        )),
                );
                if let Some(e) = &r.effect {
                    t.push_str(&format!(" Effect measured: {e}."));
                }
                t
            }
            (None, Some(r)) => format!(
                "Not run here yet. Estimate from the last rigid run ({} in {:.1} s): about \
                 {:.0} s — {passes:.0} line solves per case instead of one, plus one \
                 condensation per structure first. Its effect is shown after a run.",
                r.what,
                r.seconds,
                r.seconds * passes
            ),
            (None, None) => format!(
                "Not run here yet: about {passes:.0} line solves per case instead of one (2–5 \
                 measured), plus one condensation per structure first (7 ms for the example's \
                 lattice tower, 0.5–1.2 s for real towers of 2000 members, over the cores). Its \
                 effect is shown after a run."
            ),
        },
        S::Full => "Not built yet (calculation level 4: the structures' full non-linear models \
                    in every pass; planned after level 3)."
            .to_owned(),
    }
}

/// The (?) help of the choice: what each level does, the structures that
/// stay rigid and why, what the checks take.
pub fn help(line: &ohl_line::Line, types: Option<&ohl_structures::StructureLibrary>) -> String {
    let rigid = rigid_from_files(line, types);
    let mut t = String::from(
        "Rigid (level 2): every structure point holds its wires where it is drawn. Linear \
         stiffness (level 3, in active development): each structure with a model (a full \
         design, a separate .ohlfe model, or a pole with an elasticity) answers the wires' \
         forces with a deflection — a linear stiffness at its attachment points condensed at \
         the stringing state, its own wind and ice of the case in it — and the line is solved \
         again until the wires and the structures agree (2–5 line solves per case). Never less \
         safe than rigid: the checks that take it keep the worse of the rigid and the level-3 \
         solve. Taken: Wire tensions, String checks, Structure loads and the loads sent to \
         OHL-Structure. Still rigid (said in their results): the clearances, the views, the \
         sagging tables, internal clearances (until the structure body is drawn deflected) and \
         the other checks. Full structures (level 4) are not built yet.\n\nStay rigid at level \
         3: ",
    );
    if rigid.is_empty() {
        t.push_str("none from the files (a run may still refuse a model, said in its results)");
    } else {
        t.push_str(&rigid.join("; "));
        t.push_str(
            " — a structure without a computable model cannot deflect; sections solved by the \
             ruling span stay rigid too",
        );
    }
    if let Some(r) = last(3).filter(|r| !r.rigid.is_empty()) {
        t.push_str(&format!(
            ".\n\nTaken rigid by the last level-3 run here: {}",
            r.rigid.join("; ")
        ));
    }
    t
}

/// The radio of the choice in the Calculation options window, each choice
/// with its time and effect beside it ([`beside`]); full structures
/// greyed (not built yet).
pub fn ui(
    ui: &mut egui::Ui,
    choice: &mut ohl_line::StructuresInSolve,
    line: &ohl_line::Line,
    types: Option<&ohl_structures::StructureLibrary>,
) {
    use ohl_line::StructuresInSolve as S;
    ui.horizontal(|ui| {
        ui.strong("Structures in the wire solve");
        crate::check_help::question(ui, &help(line, types));
    });
    ui.label(
        "How the structures hold the wires in the checks that take the line's calculation \
         level (Wire tensions, String checks, Structure loads, the loads sent to OHL-Structure).",
    );
    for (c, text, hover) in [
        (
            S::Rigid,
            "Rigid (default, calculation level 2)",
            "Every structure point a fixed point of the wire model, as drawn",
        ),
        (
            S::Linear,
            "Linear stiffness (calculation level 3)",
            "Each structure with a model a linear stiffness at its attachment points (condensed \
             at the stringing state, its own wind and ice in its deflection); the checks keep \
             the worse of rigid and level 3",
        ),
        (
            S::Full,
            "Full structures (calculation level 4) — not built yet",
            "The structures' full non-linear models in every pass: not built yet (planned after \
             level 3)",
        ),
    ] {
        ui.add_enabled_ui(c != S::Full, |ui| {
            ui.radio_value(choice, c, text).on_hover_text(hover);
        });
        ui.indent(("level-beside", c.as_str()), |ui| {
            ui.label(egui::RichText::new(beside(c)).small().weak());
        });
    }
}

/// What a check needs to solve at the line's calculation level, taken
/// from the model when a run starts ([`LevelInputs::of`]).
#[derive(Debug, Clone)]
pub struct LevelInputs {
    /// The level (the line's setting).
    pub level: ohl_line::solve::flexible::CalculationLevel,
    /// The structure types (level 3).
    pub types: ohl_structures::StructureLibrary,
    /// The structure models' sections (level 3).
    pub sections: ohl_sections::SectionLibrary,
}

impl LevelInputs {
    /// The line's level and, at level 3, the libraries the structures'
    /// stiffness is set up from. Err: why the check cannot run (full
    /// structures, not built yet; a section library that does not read).
    pub fn of(model: &crate::model::Model) -> Result<Self, String> {
        let level = model.line.solver.structures.level()?;
        let linear = level == ohl_line::solve::flexible::CalculationLevel::Linear;
        Ok(Self {
            level,
            types: if linear {
                model
                    .types
                    .as_ref()
                    .map(|(t, _)| t.clone())
                    .unwrap_or_default()
            } else {
                ohl_structures::StructureLibrary::default()
            },
            sections: if linear {
                model
                    .section_library()
                    .map_err(|e| format!("cross-section library: {e}"))?
            } else {
                ohl_sections::SectionLibrary::default()
            },
        })
    }

    /// The structures of `line` set up for the weather cases `weather`
    /// (nothing at level 2; [`ohl_loads::prepare_check_level`]).
    pub fn prepare(
        &self,
        line: &ohl_line::Line,
        library: &ohl_cables::Library,
        weather: &[ohl_weather::WeatherCase],
    ) -> ohl_loads::CheckLevel {
        ohl_loads::prepare_check_level(
            line,
            library,
            &self.types,
            &self.sections,
            weather,
            self.level,
            &ohl_loads::default_budget(),
            None,
            None,
        )
    }
}

/// Records a check's run ([`record`]): `what` ran on `line` at `level` in
/// `took`, its level-3 solves `tally`, the change against rigid it
/// measured `change`.
pub fn record_run(
    level: &ohl_loads::CheckLevel,
    what: String,
    line: &ohl_line::Line,
    took: std::time::Duration,
    tally: &ohl_loads::LevelTally,
    change: Option<String>,
) {
    let mut run = LevelRun {
        what,
        line: line_size(line),
        seconds: took.as_secs_f64(),
        rigid_seconds: Some(took.as_secs_f64()),
        ..LevelRun::default()
    };
    if let Some(setup) = &level.setup {
        let s = &setup.setup;
        run.setup_seconds = Some(s.elapsed.as_secs_f64());
        run.rigid_seconds = Some(tally.rigid_elapsed.as_secs_f64());
        run.effect = change.or_else(|| {
            tally
                .moved
                .as_ref()
                .map(|(d, at)| format!("largest move {:.1} mm at {at}", d * 1e3))
        });
        if let Some((d, at)) = &tally.moved
            && run.effect.as_deref().is_some_and(|e| !e.contains("move"))
        {
            run.effect = run
                .effect
                .map(|e| format!("{e}; largest move {:.1} mm at {at}", d * 1e3));
        }
        run.rigid = s
            .structures
            .rigid
            .iter()
            .map(|r| format!("{} ({})", r.structure, r.reason))
            .collect();
    }
    record(level.level.number(), run);
}
