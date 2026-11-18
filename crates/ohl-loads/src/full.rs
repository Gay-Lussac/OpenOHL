// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Calculation level 4 (plan of 2026-11-15, phase 6; in active
//! development): each flexible structure's full non-linear answer in the
//! line's interface loop ([`ohl_line::solve::flexible::FullAnswer`]).
//!
//! Set up once per run with level 3's structures (`prepare_full`, through
//! [`crate::prepare_structure_loads_at`] and [`crate::prepare_check_level`]): per
//! flexible structure its model as the structure check takes it (or its
//! pole's frame model), its points linked as the check links their loads
//! (as the structure check links their loads), and its solve under the wires'
//! forces at the stringing state (with its self-weight) — the state its
//! displacements are measured from (the structures as drawn are the
//! structures as strung), and the first pass's tension-only set.
//!
//! In each pass of a case each structure is solved in full under the
//! wires' forces at its points (turned to its axes) and its own wind and
//! ice of the case (unfactored, as the level-3 deflection takes them),
//! from the last pass's tension-only set
//! ([`ohl_structures_fe::tangent::FullModel::solve`]); its answer is its
//! points' displacement less the stringing state's, and its tangent at the
//! solution condensed, turned to the line's axes. A structure whose solve
//! fails answers as its linear structure (level 3) from then on in that
//! case, and is reported failing there (owner 2026-11-15). The member and
//! code checks are not made here: once, on the converged forces, by the
//! structure check (owner 2026-11-15, speed).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_line::solve::flexible::{
    Answer, FullAnswer, FullProvider, LinearStructure, LoopProgress, StructureResponse,
};
use ohl_structures_fe::condense::{Attachment, PoleBody};
use ohl_structures_fe::model::NodalLoad;
use ohl_structures_fe::tangent::FullModel;
use ohl_weather::WeatherCase;

use crate::LoadCase;
use crate::analysis::Analyses;
use crate::flexibility::{over_cores, placed_link, to_line_axes};

/// A structure's full model, its points' displacement at the stringing
/// state (structure axes, m; on its feet as drawn) and its tension-only
/// set there.
type Strung = (FullModel, Vec<[f64; 3]>, Vec<bool>);

/// A flexible structure set up for level 4.
struct FullStructure {
    /// The placed structure.
    placed: ohl_line::Structure,
    /// Its type.
    t: ohl_structures::StructureType,
    /// Its full model, its displacement at each point under the stringing
    /// state's forces (structure axes, m) and the tension-only set there;
    /// Err: why not (taken linear in every case, said).
    model: Result<Strung, String>,
    /// Its measured foot movements on its model (its supports, or a
    /// pole's base; empty: none) — every case's full solve on them.
    feet: Vec<ohl_structures_fe::Prescribed>,
    /// Per case of the run (as [`FullShared::cases`]), the case as the
    /// structure check takes it here (a wind relative to the line turned
    /// to the structure's axes) and its weather (index).
    cases: Vec<Option<(LoadCase, usize)>>,
    /// The time of its solve at the stringing state (what its first solve
    /// in a case is expected to take).
    strung_time: Duration,
}

/// The progress callback of a level-4 run.
pub type ProgressFn = Arc<dyn Fn(&LoopProgress) + Send + Sync>;

/// Level 4's structures of a run, shared by its cases.
pub struct FullShared {
    /// The models read.
    analyses: Analyses,
    /// Per flexible structure of the run (as its linear structures).
    structures: Vec<FullStructure>,
    /// The run's load cases (their names: the level-3 setup's).
    cases: Vec<LoadCase>,
    /// Their weather.
    weather: Vec<WeatherCase>,
    /// Told where the loops are.
    progress: Mutex<Option<ProgressFn>>,
    /// The time of the setup.
    pub elapsed: Duration,
}

/// [`crate::LoadsSetup::full`]: level 4's structures (equal when the same
/// set-up).
#[derive(Clone)]
pub struct FullSetup(pub Arc<FullShared>);

impl std::fmt::Debug for FullSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FullSetup({} structures, {:.1?})",
            self.0.structures.len(),
            self.0.elapsed
        )
    }
}

impl PartialEq for FullSetup {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl FullSetup {
    /// `f` told where each case's loop is (per pass and per structure).
    pub fn on_progress(&self, f: ProgressFn) {
        *self.0.progress.lock().expect("not poisoned") = Some(f);
    }

    /// The structures without a full model, with why (taken linear in
    /// every case).
    pub fn linear_only(&self) -> Vec<(String, String)> {
        self.0
            .structures
            .iter()
            .filter_map(|s| match &s.model {
                Err(why) => Some((s.placed.id.clone(), why.clone())),
                Ok(_) => None,
            })
            .collect()
    }

    /// The provider of case `case` (by name; none, or not among the run's
    /// cases: no body loads).
    pub(crate) fn for_case(&self, case: Option<&str>) -> ohl_line::solve::flexible::FullModels {
        let case = case.and_then(|c| self.0.cases.iter().position(|x| x.name == c));
        ohl_line::solve::flexible::FullModels(Arc::new(FullCase {
            shared: self.0.clone(),
            case,
        }))
    }
}

/// One case's provider.
struct FullCase {
    shared: Arc<FullShared>,
    case: Option<usize>,
}

impl FullProvider for FullCase {
    fn responses<'a>(&'a self, linear: &'a [LinearStructure]) -> Vec<Box<dyn FullAnswer + 'a>> {
        linear
            .iter()
            .map(|l| {
                let s = self
                    .shared
                    .structures
                    .iter()
                    .find(|s| s.placed.id == l.structure);
                Box::new(Answering {
                    shared: &self.shared,
                    s,
                    linear: l,
                    case: self.case,
                    state: Mutex::new(State::default()),
                }) as Box<dyn FullAnswer + 'a>
            })
            .collect()
    }

    fn progress(&self, at: &LoopProgress) {
        let f = self.shared.progress.lock().expect("not poisoned").clone();
        if let Some(f) = f {
            f(at);
        }
    }

    fn notes(&self) -> Vec<String> {
        let mut out = vec![
            "level 4: each structure's solve bounded by the structure solver's own exit clauses \
             (200 iterations, 30 s a solve); the member and code checks once, on the converged \
             forces"
                .to_owned(),
        ];
        for s in &self.shared.structures {
            if let Err(why) = &s.model {
                out.push(format!(
                    "{}: no full model at level 4 ({why}) — taken linear (level 3)",
                    s.placed.id
                ));
            }
        }
        let on_feet: Vec<&str> = self
            .shared
            .structures
            .iter()
            .filter(|s| !s.feet.is_empty())
            .map(|s| s.placed.id.as_str())
            .collect();
        if !on_feet.is_empty() {
            out.push(format!(
                "level 4 on the measured foot movements of {}: each case's full solve with the \
                 supports moved, its displacement from the stringing state's solve on the feet as \
                 drawn (as level 3 adds the feet's moves)",
                on_feet.join(", ")
            ));
        }
        out
    }
}

/// A structure's state in one case's loop.
#[derive(Default)]
struct State {
    /// The last solve's tension-only set (none: the stringing state's).
    inactive: Option<Vec<bool>>,
    /// Why its solve failed (answering linear since).
    failed: Option<String>,
    /// Its own wind and ice of the case at its nodes (made at the first
    /// answer).
    body: Option<Vec<NodalLoad>>,
    /// Solves and their time; the last one's.
    solves: usize,
    time: Duration,
    last: Option<Duration>,
}

/// A structure answering in one case's loop.
struct Answering<'a> {
    shared: &'a FullShared,
    s: Option<&'a FullStructure>,
    linear: &'a LinearStructure,
    case: Option<usize>,
    state: Mutex<State>,
}

impl StructureResponse for Answering<'_> {
    fn structure(&self) -> &str {
        &self.linear.structure
    }

    fn labels(&self) -> &[String] {
        &self.linear.labels
    }

    fn reference(&self) -> &[ohl_line::Point] {
        &self.linear.reference
    }

    fn answer(&self, forces: &[ohl_line::Point]) -> Result<Answer, String> {
        let mut st = self.state.lock().expect("not poisoned");
        if st.failed.is_some() {
            return self.linear.answer(forces);
        }
        let Some(s) = self.s else {
            st.failed = Some("not set up at level 4".into());
            return self.linear.answer(forces);
        };
        let (model, strung, strung_set) = match &s.model {
            Ok(m) => m,
            Err(why) => {
                st.failed = Some(why.clone());
                return self.linear.answer(forces);
            }
        };
        if st.body.is_none() {
            st.body = Some(self.shared.body_of(s, model, self.case));
        }
        let orientation = s.placed.orientation_deg;
        let local: Vec<[f64; 3]> = forces
            .iter()
            .map(|f| ohl_line::links::to_structure_axes(orientation, *f))
            .collect();
        let started = Instant::now();
        let start = st.inactive.clone().unwrap_or_else(|| strung_set.clone());
        let solved = model.solve_on(
            &local,
            st.body.as_deref().unwrap_or_default(),
            &s.feet,
            Some(&start),
            &ohl_structures_fe::default_budget(),
        );
        let took = started.elapsed();
        st.solves += 1;
        st.time += took;
        st.last = Some(took);
        match solved {
            Ok(p) => {
                st.inactive = Some(p.inactive);
                let n = self.linear.labels.len();
                let displacement = (0..n)
                    .map(|k| {
                        let d = [0, 1, 2].map(|c| p.displacement[k][c] - strung[k][c]);
                        ohl_line::links::from_structure_axes(orientation, d)
                    })
                    .collect();
                let c = p.tangent.turned(to_line_axes(orientation));
                let mut flexibility = nalgebra::DMatrix::<f64>::zeros(3 * n, 3 * n);
                for a in 0..n {
                    for b in 0..n {
                        for i in 0..3 {
                            for j in 0..3 {
                                flexibility[(3 * a + i, 3 * b + j)] =
                                    c.flexibility[(3 * c.place_of[a] + i, 3 * c.place_of[b] + j)];
                            }
                        }
                    }
                }
                Ok(Answer {
                    displacement,
                    flexibility,
                })
            }
            Err(why) => {
                st.failed = Some(why);
                self.linear.answer(forces)
            }
        }
    }
}

impl FullAnswer for Answering<'_> {
    fn failed(&self) -> Option<String> {
        self.state.lock().expect("not poisoned").failed.clone()
    }

    fn solves(&self) -> (usize, Duration) {
        let st = self.state.lock().expect("not poisoned");
        (st.solves, st.time)
    }

    fn expected(&self) -> Duration {
        let st = self.state.lock().expect("not poisoned");
        if st.failed.is_some() {
            return Duration::ZERO;
        }
        st.last
            .or_else(|| self.s.map(|s| s.strung_time))
            .unwrap_or_default()
    }
}

impl FullShared {
    /// Structure `s`'s own wind and ice of case `case` at its model's
    /// nodes, unfactored (none: no case).
    fn body_of(&self, s: &FullStructure, model: &FullModel, case: Option<usize>) -> Vec<NodalLoad> {
        let Some((c, w)) = case.and_then(|k| s.cases.get(k)?.as_ref()) else {
            return Vec::new();
        };
        let w = &self.weather[*w];
        let extension = s.placed.extension.as_deref();
        if let Some((Ok(m), extended)) = self.analyses.model_of(&s.t, extension) {
            let key = (
                s.t.id.clone(),
                extension.filter(|_| extended).map(str::to_owned),
            );
            let mut ignored = Vec::new();
            return self
                .analyses
                .body_loads(
                    &key,
                    m,
                    &s.t,
                    Some(&s.placed),
                    c,
                    Some(w),
                    false,
                    &mut ignored,
                )
                .nodal;
        }
        if let Some(Ok(pole)) = self.analyses.pole_of(&s.t.id) {
            return model.pole_body(pole, &pole_body(pole, &s.t, &s.placed, c, w));
        }
        Vec::new()
    }
}

/// A pole's own wind and ice of a case, unfactored, as the pole check
/// takes them (and the level-3 condensation).
fn pole_body(
    pole: &ohl_structures_fe::Pole,
    t: &ohl_structures::StructureType,
    placed: &ohl_line::Structure,
    case: &LoadCase,
    w: &WeatherCase,
) -> PoleBody {
    let foot = t.foot.map_or(0.0, |f| f.si());
    let mut b = PoleBody::default();
    let mut iced = 1.0;
    if let Ok(Some(ice)) = case.structure_ice_at(w, Some(placed)) {
        b.loads = crate::analysis::pole_ice(pole, ice, foot).0;
        let top = pole.diameter_at(pole.height());
        if top > 0.0 {
            iced = (top + 2.0 * ice.thickness) / top;
        }
    }
    let wind = case.structure_wind_at(w, Some(placed));
    let [tx, ty] = wind.towards_xy();
    b.wind = [tx * wind.pressure * iced, ty * wind.pressure * iced];
    b
}

/// Level 4's structures of a run on `line` (the line its stringing state
/// is solved on), for the flexible structures of level 3's setup
/// `linear` (their stringing state's forces), in load cases `cases`
/// (their weather in `weather`): per structure its full model (its type's
/// model as the structure check takes it, or its pole's frame model), its
/// solve under the stringing state's forces (its self-weight with them),
/// and each case in its axes; over the cores, `done` told per structure,
/// `cancel` stopping it (the structures not reached: taken linear, said).
///
/// Expected cost: one full solve per flexible structure (a lattice tower
/// of 2000 members: some tenths of a second with P-Delta).
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_full(
    line: &ohl_line::Line,
    types: &ohl_structures::StructureLibrary,
    analyses: Analyses,
    linear: &[LinearStructure],
    cases: &[LoadCase],
    weather: &[WeatherCase],
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> FullSetup {
    let started = Instant::now();
    let budget: Budget = ohl_structures_fe::default_budget();
    let made = over_cores(
        linear,
        |l| -> Option<FullStructure> {
            let placed = line.structure(&l.structure)?.clone();
            let t = types.get(placed.structure_type.as_deref()?)?.clone();
            let mut feet = Vec::new();
            let mut strung_time = Duration::ZERO;
            let model = full_model(&analyses, &t, &placed, &l.labels).and_then(|(m, f)| {
                feet = f;
                let local: Vec<[f64; 3]> = l
                    .reference
                    .iter()
                    .map(|f| ohl_line::links::to_structure_axes(placed.orientation_deg, *f))
                    .collect();
                // The stringing state on the feet as drawn: the
                // displacements are measured from the structure as drawn
                // and strung, the feet's moves part of each case's answer
                // (as level 3 adds them).
                let solved = Instant::now();
                let p = m
                    .solve(&local, &[], None, &budget)
                    .map_err(|e| format!("at the stringing state: {e}"))?;
                strung_time = solved.elapsed();
                Ok((m, p.displacement, p.inactive))
            });
            let cases = cases
                .iter()
                .map(|c| {
                    let k = weather.iter().position(|w| w.name == c.weather)?;
                    let local = c
                        .in_structure_axes(&weather[k], line, &placed)
                        .unwrap_or_else(|| c.clone());
                    Some((local, k))
                })
                .collect();
            Some(FullStructure {
                placed,
                t,
                model,
                feet,
                cases,
                strung_time,
            })
        },
        done,
        cancel,
    );
    let structures = linear
        .iter()
        .zip(made)
        .filter_map(|(l, m)| match m {
            Some(Some(s)) => Some(s),
            Some(None) => None,
            None => {
                let placed = line.structure(&l.structure)?.clone();
                let t = types.get(placed.structure_type.as_deref()?)?.clone();
                Some(FullStructure {
                    placed,
                    t,
                    model: Err("stopped before its turn".into()),
                    feet: Vec::new(),
                    cases: Vec::new(),
                    strung_time: Duration::ZERO,
                })
            }
        })
        .collect();
    FullSetup(Arc::new(FullShared {
        analyses,
        structures,
        cases: cases.to_vec(),
        weather: weather.to_vec(),
        progress: Mutex::new(None),
        elapsed: started.elapsed(),
    }))
}

/// The full model of placed structure `placed` of type `t` at points
/// `labels`: its model as the structure check takes it (its extension's
/// own when it changes it), else its pole's frame model; with its
/// measured foot movements on it (`crate::feet`: its supports, a pole's
/// base). Err: why not (a foot that is not a support of the model too).
fn full_model(
    analyses: &Analyses,
    t: &ohl_structures::StructureType,
    placed: &ohl_line::Structure,
    labels: &[String],
) -> Result<(FullModel, Vec<ohl_structures_fe::Prescribed>), String> {
    let extension = placed.extension.as_deref();
    if let Some((model, _)) = analyses.model_of(t, extension) {
        let model = model.as_ref().map_err(Clone::clone)?;
        // Its design code's plugin not found: not solved (2026-11-18).
        if let Some(e) = analyses.plugin_error_of(t, extension) {
            return Err(e.clone());
        }
        let feet = crate::feet::on_model(model, Some(placed))
            .map_err(|e| format!("foot movements: {e}"))?
            .map(|f| f.prescribed)
            .unwrap_or_default();
        let points = labels
            .iter()
            .map(|l| Attachment {
                label: l.clone(),
                link: placed_link(Some(placed), l),
            })
            .collect();
        return Ok((FullModel::of_model(model.clone(), points), feet));
    }
    if let Some(pole) = analyses.pole_of(&t.id) {
        let pole = pole.as_ref().map_err(Clone::clone)?;
        let points: Vec<(String, [f64; 3])> = labels
            .iter()
            .map(|l| {
                t.point_local(l, None)
                    .map(|p| (l.clone(), p))
                    .ok_or_else(|| format!("attachment point {l} not in the type"))
            })
            .collect::<Result<_, _>>()?;
        // The heights its ice goes on (the pole check's pieces).
        let foot = t.foot.map_or(0.0, |f| f.si());
        let h = pole.height();
        let pieces = crate::analysis::POLE_ICE_PIECES;
        let heights = (0..pieces).map(|k| foot + (k as f64 + 0.5) * h / pieces as f64);
        let m = FullModel::of_pole(pole, &points, heights).map_err(|e| e.to_string())?;
        let base = crate::feet::on_pole(Some(placed))
            .map_err(|e| format!("foot movements: {e}"))?
            .unwrap_or([0.0; 6]);
        let feet = m.pole_feet(base);
        return Ok((m, feet));
    }
    Err("no structure model".into())
}

#[cfg(test)]
#[path = "full_tests.rs"]
mod tests;
