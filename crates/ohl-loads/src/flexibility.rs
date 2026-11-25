// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A placed structure's stiffness at its attachment points, for the
//! line's coupled solve of structures and wires (calculation levels 3 and
//! 4, plan of 2026-11-15, phases 1, 3 and 4; in active development).
//!
//! The structure is the one the structure check analyses
//! ([`Analyses::model_of`]: its type's model, the extension's own when it
//! stands on one that changes it; else its pole, [`Analyses::pole_of`]),
//! its points linked as the check links their loads ([`placed_link`] and
//! [`ohl_structures_fe::Model::attachment_load`], one rule for both), the
//! matrices condensed in the structure's axes
//! ([`ohl_structures_fe::condense`]) and turned to the line's by its
//! orientation (its height adjustment, a translation, changes nothing).
//! A structure with neither a model nor a pole, or one refused, stays
//! rigid with the reason ([`Rigid`]).
//!
//! [`flexible_structures`] sets up every placed structure of a line for
//! calculation level 3 (phase 3: the matrices at the stringing state, kept
//! by their key, condensed over the cores) and [`solve_line_flexible`]
//! runs the line's interface loop ([`ohl_line::solve::flexible`]) in
//! weather cases — an internal entry point for testing (the command
//! line's `--calculation-level 3`). [`prepare_structure_loads`] sets the
//! structures up for the structure loads at level 3 (their own wind and
//! ice per case too; [`crate::structure_loads_at`]).

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_line::solve::flexible::{
    CalculationLevel, FlexibleFailure, FlexibleOptions, FlexibleSolve, LinearStructure,
    ReferenceForces, RigidStructure, Structures, reference_forces,
};
use ohl_structures::StructureType;
use ohl_structures_fe::condense::{
    Attachment, CondenseKey, Condensed, Linearisation, Link, PoleBody, Rigid, condense_full,
    condense_pole_full,
};
use ohl_structures_fe::{Loads, PoleLoad};
use ohl_weather::WeatherCase;

use crate::LoadCase;
use crate::analysis::Analyses;

/// How placed structure `placed` links its attachment point `label`: its
/// own link (a relink, an offset) over the type's, unlinked when it says
/// so (2026-10-21) — the rule of the structure check's loads
/// ([`Analyses::fe`]) and of the condensation.
pub(crate) fn placed_link(placed: Option<&ohl_line::Structure>, label: &str) -> Link {
    match placed.and_then(|s| s.point(label)) {
        Some(p) if p.unlinked => Link::Unlinked,
        Some(p) => match &p.joint {
            Some(joint) => Link::Joint {
                joint: joint.clone(),
                offset: p.offset,
            },
            None => Link::Type,
        },
        None => Link::Type,
    }
}

/// The rotation from a structure's axes (right of the line, ahead, up)
/// to the line's for its orientation (rows of R, v_line = R v_local),
/// from the one convention ([`ohl_line::links::from_structure_axes`]).
pub(crate) fn to_line_axes(orientation_deg: f64) -> [[f64; 3]; 3] {
    let columns = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .map(|e| ohl_line::links::from_structure_axes(orientation_deg, e));
    [0, 1, 2].map(|r| [0, 1, 2].map(|c| columns[c][r]))
}

/// A placed structure condensed to its attachment points.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Flexible {
    /// The memo's key (per type and extension: the orientation is not
    /// in it).
    pub key: CondenseKey,
    /// The matrices in the line's axes.
    pub condensed: Condensed,
    /// Per body case asked, each point's (as asked) deflection under the
    /// structure's own wind and ice of the case, line axes, m.
    pub body: Vec<Vec<[f64; 3]>>,
    /// Each point's (as asked) move by the structure's measured foot
    /// movements, line axes, m (empty: none; [`crate::feet`]).
    pub feet: Vec<[f64; 3]>,
    /// What the foot movements left out, in words.
    pub feet_notes: Vec<String>,
}

/// The distinct non-empty sets of `sets`, and per set its index among
/// them (none: an empty set, no load).
fn distinct<T: PartialEq + IsEmpty>(sets: impl Iterator<Item = T>) -> (Vec<T>, Vec<Option<usize>>) {
    let mut out: Vec<T> = Vec::new();
    let mut of = Vec::new();
    for set in sets {
        if set.is_none_or_empty() {
            of.push(None);
            continue;
        }
        match out.iter().position(|x| *x == set) {
            Some(k) => of.push(Some(k)),
            None => {
                out.push(set);
                of.push(Some(out.len() - 1));
            }
        }
    }
    (out, of)
}

/// A set of loads that may be no load.
trait IsEmpty {
    fn is_none_or_empty(&self) -> bool;
}

impl<T> IsEmpty for Vec<T> {
    fn is_none_or_empty(&self) -> bool {
        self.is_empty()
    }
}

impl IsEmpty for PoleBody {
    fn is_none_or_empty(&self) -> bool {
        self.wind == [0.0; 2] && self.loads.is_empty()
    }
}

impl Analyses {
    /// The stiffness of placed structure `placed` of type `t` at its
    /// attachment points `labels` (those carrying wires), in the line's
    /// axes. `reference`: the wires' forces at points at the reference
    /// state (the stringing state: label, N, the structure's axes,
    /// unfactored) — the tangent there, with the structure's self-weight
    /// when its model takes it; none: the elastic structure (first
    /// order).
    ///
    /// Expected cost: one reference solve and one factorisation of the
    /// structure's tangent plus a substitution per attachment direction
    /// (measured, theory: Structure flexibility); to be kept per
    /// [`Flexible::key`] by its caller ([`Analyses::flexibility_cached`]).
    #[cfg(test)]
    pub(crate) fn flexibility(
        &self,
        t: &StructureType,
        placed: Option<&ohl_line::Structure>,
        labels: &[String],
        reference: Option<&[(String, [f64; 3])]>,
        budget: &Budget,
    ) -> Result<Flexible, Rigid> {
        self.flexibility_cached(t, placed, labels, reference, &[], budget, None)
    }

    /// The stiffness of placed structure `placed` of type `t` at its
    /// attachment points `labels`, as `Analyses::flexibility` (the form
    /// without a cache, for tests) says, the condensations (in the
    /// structure's axes) kept in `cache` by their key and taken from it
    /// (the same inputs, the same matrices). `body`: load cases (turned
    /// to the structure's axes where their wind is relative to the line)
    /// and their weather — each point's deflection under the structure's
    /// own wind and ice of each, unfactored, on the same tangent
    /// ([`Flexible::body`], plan §4.6).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn flexibility_cached(
        &self,
        t: &StructureType,
        placed: Option<&ohl_line::Structure>,
        labels: &[String],
        reference: Option<&[(String, [f64; 3])]>,
        body: &[(LoadCase, &WeatherCase)],
        budget: &Budget,
        cache: Option<&CondenseCache>,
    ) -> Result<Flexible, Rigid> {
        let memo = |key: &CondenseKey, make: &dyn Fn() -> Result<Condensed, Rigid>| {
            let Some(cache) = cache else {
                return make();
            };
            if let Some(hit) = cache.lock().expect("not poisoned").get(key) {
                return hit.clone();
            }
            let made = make();
            cache
                .lock()
                .expect("not poisoned")
                .insert(key.clone(), made.clone());
            made
        };
        let extension = placed.and_then(|s| s.extension.as_deref());
        let orientation = placed.map_or(0.0, |s| s.orientation_deg);
        let mut feet_notes = Vec::new();
        let (key, condensed, of_case) = if let Some((model, extended)) = self.model_of(t, extension)
        {
            let model = model.as_ref().map_err(|e| Rigid::ModelNotRead(e.clone()))?;
            // Its design code's plugin not found (or failing): not flexible
            // either — its solve would cap the tension-only members by
            // another rule than its code's (2026-11-18).
            if let Some(e) = self.plugin_error_of(t, extension) {
                return Err(Rigid::ModelNotRead(e.clone()));
            }
            // Its measured foot movements (owner 2026-11-16): the
            // reference state with its feet moved, and each point's move.
            let feet = crate::feet::on_model(model, placed)
                .map_err(|e| Rigid::Reference(format!("foot movements: {e}")))?
                .unwrap_or_default();
            feet_notes.clone_from(&feet.notes);
            let points: Vec<Attachment> = labels
                .iter()
                .map(|l| Attachment {
                    label: l.clone(),
                    link: placed_link(placed, l),
                })
                .collect();
            let linearisation = match reference {
                None => Linearisation::Unloaded,
                Some(forces) => {
                    let mut nodal = Vec::new();
                    for (label, force) in forces {
                        let link = placed_link(placed, label);
                        if link == Link::Unlinked {
                            return Err(Rigid::Unlinked(label.clone()));
                        }
                        let at = model
                            .attachment_load(label, &link, *force)
                            .ok_or_else(|| Rigid::NotInModel(label.clone()))?;
                        nodal.extend(at.loads);
                    }
                    Linearisation::Tangent(Loads {
                        nodal,
                        self_weight: if model.analysis.self_weight { 1.0 } else { 0.0 },
                        prescribed: feet.prescribed.clone(),
                        ..Loads::default()
                    })
                }
            };
            // The structure's own wind and ice of each case, unfactored, at
            // its joints as the check puts them; identical sets once.
            let model_key = (
                t.id.clone(),
                extension.filter(|_| extended).map(str::to_owned),
            );
            let (extra, of_case) = distinct(body.iter().map(|(case, w)| {
                let mut ignored = Vec::new();
                self.body_loads(
                    &model_key,
                    model,
                    t,
                    placed,
                    case,
                    Some(w),
                    false,
                    &mut ignored,
                )
                .nodal
            }));
            let key = CondenseKey::of_model(
                &t.id,
                extension.filter(|_| extended),
                model,
                &points,
                &linearisation,
            )
            .with_extra(&extra, extra.is_empty())
            .with_extra(&("feet", &feet.prescribed), feet.prescribed.is_empty());
            let condensed = memo(&key, &|| {
                condense_full(
                    model,
                    &points,
                    &linearisation,
                    &extra,
                    &feet.prescribed,
                    budget,
                )
            })?;
            (key, condensed, of_case)
        } else if let Some(pole) = self.pole_of(&t.id) {
            let pole = pole.as_ref().map_err(|e| Rigid::ModelNotRead(e.clone()))?;
            // The points as the pole check places them: the type's.
            let points: Vec<(String, [f64; 3])> = labels
                .iter()
                .map(|l| {
                    t.point_local(l, None)
                        .map(|p| (l.clone(), p))
                        .ok_or_else(|| Rigid::NotInModel(l.clone()))
                })
                .collect::<Result<_, _>>()?;
            let wires: Option<Vec<PoleLoad>> = reference.map(|forces| {
                forces
                    .iter()
                    .filter_map(|(l, f)| {
                        Some(PoleLoad {
                            point: t.point_local(l, None)?,
                            force: *f,
                            moment: [0.0; 3],
                        })
                    })
                    .collect()
            });
            // The wind on its shaft and its ice, unfactored, as the pole
            // check takes them.
            let foot = t.foot.map_or(0.0, |f| f.si());
            let (extra, of_case) = distinct(body.iter().map(|(case, w)| {
                let mut b = PoleBody::default();
                let mut iced = 1.0;
                if let Ok(Some(ice)) = case.structure_ice_at(w, placed) {
                    b.loads = crate::analysis::pole_ice(pole, ice, foot).0;
                    let top = pole.diameter_at(pole.height());
                    if top > 0.0 {
                        iced = (top + 2.0 * ice.thickness) / top;
                    }
                }
                let wind = case.structure_wind_at(w, placed);
                let [tx, ty] = wind.towards_xy();
                b.wind = [tx * wind.pressure * iced, ty * wind.pressure * iced];
                b
            }));
            let base = crate::feet::on_pole(placed)
                .map_err(|e| Rigid::Reference(format!("foot movements: {e}")))?
                .unwrap_or([0.0; 6]);
            let key = CondenseKey::of_pole(&t.id, pole, &points, wires.as_deref())
                .with_extra(&extra, extra.is_empty())
                .with_extra(&("feet", base), base == [0.0; 6]);
            let condensed = memo(&key, &|| {
                condense_pole_full(pole, &points, wires.as_deref(), &extra, base, budget)
            })?;
            (key, condensed, of_case)
        } else {
            return Err(Rigid::NoModel);
        };
        let condensed = condensed.turned(to_line_axes(orientation));
        let body = of_case
            .iter()
            .map(|k| {
                k.and_then(|k| condensed.extra_at_points(k))
                    .unwrap_or_else(|| vec![[0.0; 3]; labels.len()])
            })
            .collect();
        let feet = condensed.feet_at_points().unwrap_or_default();
        Ok(Flexible {
            key,
            condensed,
            body,
            feet,
            feet_notes,
        })
    }
}

impl Analyses {
    /// The deflected body of placed structure `placed` of type `t` for
    /// drawing (plan of 2026-11-15, phase 5b; display only): on the
    /// tangent the condensation takes (at the stringing state `strung`:
    /// label and force, structure axes, N — as [`Analyses::flexibility_cached`]),
    /// under the change of the wires' forces from there to `forces` (the
    /// same labels, structure axes) and the structure's own wind and ice
    /// of `body` (unfactored) — the linear answer of calculation level 3
    /// at every joint (a model) or along the axis (a pole) — on its
    /// measured foot movements (owner 2026-11-16: their rigid-body motion
    /// and elastic distortion added, the stringing state's tangent taken
    /// with the feet moved, as the condensation takes it). `unloaded`:
    /// the elastic structure, no load — the foot movements alone (the
    /// state without a reference case). Refused as the condensation is.
    ///
    /// Expected cost: one reference solve and one factorisation of the
    /// structure's tangent (as one condensation).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn deflected_body(
        &self,
        t: &StructureType,
        placed: Option<&ohl_line::Structure>,
        strung: &[(String, [f64; 3])],
        forces: &[(String, [f64; 3])],
        body: Option<(&LoadCase, &WeatherCase)>,
        unloaded: bool,
        budget: &Budget,
    ) -> Result<ohl_structures_fe::drawn::BodyDeflection, Rigid> {
        let extension = placed.and_then(|s| s.extension.as_deref());
        let change: Vec<(String, [f64; 3])> = forces
            .iter()
            .map(|(l, f)| {
                let r = strung
                    .iter()
                    .find(|(x, _)| x == l)
                    .map_or([0.0; 3], |(_, r)| *r);
                (l.clone(), [f[0] - r[0], f[1] - r[1], f[2] - r[2]])
            })
            .collect();
        if let Some((model, extended)) = self.model_of(t, extension) {
            let model = model.as_ref().map_err(|e| Rigid::ModelNotRead(e.clone()))?;
            // Its design code's plugin not found (or failing): not flexible
            // either — its solve would cap the tension-only members by
            // another rule than its code's (2026-11-18).
            if let Some(e) = self.plugin_error_of(t, extension) {
                return Err(Rigid::ModelNotRead(e.clone()));
            }
            let nodal_of = |set: &[(String, [f64; 3])]| -> Result<Vec<_>, Rigid> {
                let mut nodal = Vec::new();
                for (label, force) in set {
                    let link = placed_link(placed, label);
                    if link == Link::Unlinked {
                        return Err(Rigid::Unlinked(label.clone()));
                    }
                    let at = model
                        .attachment_load(label, &link, *force)
                        .ok_or_else(|| Rigid::NotInModel(label.clone()))?;
                    nodal.extend(at.loads);
                }
                Ok(nodal)
            };
            let feet = crate::feet::on_model(model, placed)
                .map_err(|e| Rigid::Reference(format!("foot movements: {e}")))?
                .unwrap_or_default();
            let linearisation = if unloaded {
                Linearisation::Unloaded
            } else {
                Linearisation::Tangent(Loads {
                    nodal: nodal_of(strung)?,
                    self_weight: if model.analysis.self_weight { 1.0 } else { 0.0 },
                    prescribed: feet.prescribed.clone(),
                    ..Loads::default()
                })
            };
            let mut loads = nodal_of(&change)?;
            if let Some((case, w)) = body {
                let key = (
                    t.id.clone(),
                    extension.filter(|_| extended).map(str::to_owned),
                );
                let mut ignored = Vec::new();
                loads.extend(
                    self.body_loads(&key, model, t, placed, case, Some(w), false, &mut ignored)
                        .nodal,
                );
            }
            let u = ohl_structures_fe::condense::deflect_full(
                model,
                &linearisation,
                &loads,
                &feet.prescribed,
                budget,
            )?;
            return Ok(ohl_structures_fe::drawn::BodyDeflection {
                joints: model
                    .nodes
                    .iter()
                    .zip(u)
                    .map(|(n, d)| (n.point, [d[0], d[1], d[2]]))
                    .collect(),
                pole: Vec::new(),
            });
        }
        if let Some(pole) = self.pole_of(&t.id) {
            let pole = pole.as_ref().map_err(|e| Rigid::ModelNotRead(e.clone()))?;
            let at = |set: &[(String, [f64; 3])]| -> Vec<PoleLoad> {
                set.iter()
                    .filter_map(|(l, f)| {
                        Some(PoleLoad {
                            point: t.point_local(l, None)?,
                            force: *f,
                            moment: [0.0; 3],
                        })
                    })
                    .collect()
            };
            let mut own = PoleBody::default();
            if let Some((case, w)) = body {
                let foot = t.foot.map_or(0.0, |f| f.si());
                let mut iced = 1.0;
                if let Ok(Some(ice)) = case.structure_ice_at(w, placed) {
                    own.loads = crate::analysis::pole_ice(pole, ice, foot).0;
                    let top = pole.diameter_at(pole.height());
                    if top > 0.0 {
                        iced = (top + 2.0 * ice.thickness) / top;
                    }
                }
                let wind = case.structure_wind_at(w, placed);
                let [tx, ty] = wind.towards_xy();
                own.wind = [tx * wind.pressure * iced, ty * wind.pressure * iced];
            }
            let base = crate::feet::on_pole(placed)
                .map_err(|e| Rigid::Reference(format!("foot movements: {e}")))?
                .unwrap_or([0.0; 6]);
            let axis = ohl_structures_fe::condense::deflect_pole_full(
                pole,
                (!unloaded).then(|| at(strung)).as_deref(),
                &at(&change),
                &own,
                base,
                budget,
            )?;
            return Ok(ohl_structures_fe::drawn::BodyDeflection {
                joints: Vec::new(),
                pole: axis,
            });
        }
        Err(Rigid::NoModel)
    }
}

/// A structure moved as one body and its (zero) body deflection per case.
type OneBody = (LinearStructure, Vec<Vec<ohl_line::Point>>);

/// A structure without a model moved as one body by its foot movements
/// ([`crate::feet::as_one_body`]) for the coupled solve: no flexibility
/// (rigid), each of its points `labels` moved by t + θ × r (turned to
/// the line's axes), with the forces `at` at the stringing state, no
/// body deflection in any of `cases` cases. None: no foot movements.
fn one_body(
    t: &StructureType,
    s: &ohl_line::Structure,
    labels: &[String],
    at: &[(String, ohl_line::Point)],
    cases: usize,
) -> Option<Result<OneBody, String>> {
    let foot = match crate::feet::as_one_body(Some(s)) {
        Ok(Some(f)) => f.clone(),
        Ok(None) => return None,
        Err(e) => return Some(Err(e)),
    };
    let mut feet = Vec::with_capacity(labels.len());
    for l in labels {
        let Some(r) = t.point_local(l, s.extension.as_deref()) else {
            return Some(Err(format!("point {l} not in its type")));
        };
        feet.push(ohl_line::links::from_structure_axes(
            s.orientation_deg,
            foot.rigid_at(r),
        ));
    }
    let n = labels.len();
    Some(Ok((
        LinearStructure {
            structure: s.id.clone(),
            labels: labels.to_vec(),
            flexibility: nalgebra::DMatrix::<f64>::zeros(3 * n, 3 * n),
            reference: at.iter().map(|(_, f)| *f).collect(),
            body: vec![[0.0; 3]; n],
            feet,
            approximate: false,
            notes: vec![
                "rigid (no structure model), moved as one body by its measured foot movements"
                    .to_owned(),
            ],
        },
        vec![vec![[0.0; 3]; n]; cases],
    )))
}

/// Condensations by their key, in the structure's axes (or why refused).
pub(crate) type CondenseCache = Mutex<HashMap<CondenseKey, Result<Condensed, Rigid>>>;

/// A line's structures set up for calculation level 3.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlexibleSetup {
    /// The flexible structures (their matrices in the line's axes), the
    /// rigid ones with why, the notes.
    pub structures: Structures,
    /// The wires' forces at the stringing state.
    pub reference: ReferenceForces,
    /// Condensations made (one per distinct key: type, extension, model,
    /// points and reference forces).
    pub condensed: usize,
    /// The time of the setup (the reference solve included).
    pub elapsed: Duration,
}

/// Each of `items` through `f`, over the cores (in order); `done` told
/// after each item (items done, of), `cancel` stops taking new ones (the
/// items not taken: none).
pub(crate) fn over_cores<T: Sync, R: Send>(
    items: &[T],
    f: impl Fn(&T) -> R + Sync,
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> Vec<Option<R>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(items.len())
        .max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let finished = std::sync::atomic::AtomicUsize::new(0);
    let out: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    let work = || {
        loop {
            if cancel.is_some_and(ohl_core::budget::CancelToken::is_cancelled) {
                return;
            }
            let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(item) = items.get(k) else { return };
            let r = f(item);
            out.lock().expect("not poisoned").push((k, r));
            let n = finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if let Some(d) = done {
                d(n, items.len());
            }
        }
    };
    if threads == 1 {
        work();
    } else {
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(work);
            }
        });
    }
    let mut kept: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    for (k, r) in out.into_inner().expect("not poisoned") {
        kept[k] = Some(r);
    }
    kept
}

/// The structures of `line` for calculation level 3: the wires' forces at
/// the stringing state ([`reference_forces`]); per placed structure its
/// points carrying wires solved by finite elements, condensed at the
/// stringing state (`Analyses::flexibility_cached`: the tangent under its
/// self-weight and those forces), the matrix expanded to its points and
/// turned to the line's axes; a structure with no type, no model, a
/// refusal or no such wire: rigid, with why. `budget`: the line solves';
/// the structures' solves take their own default budget.
///
/// Expected cost: one rigid line solve, then one condensation per placed
/// structure (measured 7 ms for the example's lattice tower, 0.5–1.2 s
/// for real towers of 2000 members; theory: Structure flexibility), over
/// the cores.
pub fn flexible_structures(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    budget: &Budget,
) -> FlexibleSetup {
    let analyses = Analyses::load(types, sections);
    flexible_with(line, library, types, &analyses, budget)
}

/// [`flexible_structures`] with the models read (`analyses`).
pub(crate) fn flexible_with(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    analyses: &Analyses,
    budget: &Budget,
) -> FlexibleSetup {
    setup_with(line, library, types, analyses, budget, &[], &[], None, None).0
}

/// The setup of [`flexible_with`] with, per structure and per case of
/// `cases` (their weather in `weather`), each point's deflection under
/// the structure's own wind and ice of the case (unfactored), as
/// [`LoadsSetup::body`] keeps them; `done` told per structure, `cancel`
/// stops it (the structures not reached: rigid, said).
#[allow(clippy::too_many_arguments)]
fn setup_with(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    analyses: &Analyses,
    budget: &Budget,
    cases: &[LoadCase],
    weather: &[WeatherCase],
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> (FlexibleSetup, Vec<Vec<Vec<ohl_line::Point>>>) {
    let started = Instant::now();
    let reference = reference_forces(line, library, budget);
    let mut points: BTreeMap<&str, Vec<(String, ohl_line::Point)>> = BTreeMap::new();
    for ((structure, label), f) in &reference.forces {
        points
            .entry(structure.as_str())
            .or_default()
            .push((label.clone(), *f));
    }
    // Each structure once (the first of an id, as `Line::structure`).
    let mut placed: Vec<&ohl_line::Structure> = Vec::new();
    for s in &line.structures {
        if !placed.iter().any(|p| p.id == s.id) {
            placed.push(s);
        }
    }
    let cache: CondenseCache = Mutex::new(HashMap::new());
    let structure_budget = ohl_structures_fe::default_budget();
    let rigid = |s: &ohl_line::Structure, reason: String| RigidStructure {
        structure: s.id.clone(),
        reason,
    };
    type Answer = Option<Result<(LinearStructure, Vec<Vec<ohl_line::Point>>), RigidStructure>>;
    let answers = over_cores(
        &placed,
        |&s| -> Answer {
            let Some(at) = points.get(s.id.as_str()) else {
                // No wire solved by finite elements on it: nothing to move.
                return None;
            };
            let Some(type_id) = &s.structure_type else {
                return Some(Err(rigid(s, "no structure type".into())));
            };
            let Some(t) = types.get(type_id) else {
                return Some(Err(rigid(
                    s,
                    format!("its type {type_id} is not in the structure library"),
                )));
            };
            let labels: Vec<String> = at.iter().map(|(l, _)| l.clone()).collect();
            let local: Vec<(String, [f64; 3])> = at
                .iter()
                .map(|(l, f)| {
                    (
                        l.clone(),
                        ohl_line::links::to_structure_axes(s.orientation_deg, *f),
                    )
                })
                .collect();
            // Each case as the structure check takes it here: a wind
            // relative to the line turned to the structure's axes.
            let body: Vec<(LoadCase, &WeatherCase)> = cases
                .iter()
                .filter_map(|c| {
                    let w = weather.iter().find(|w| w.name == c.weather)?;
                    Some((
                        c.in_structure_axes(w, line, s).unwrap_or_else(|| c.clone()),
                        w,
                    ))
                })
                .collect();
            let flexible = match analyses.flexibility_cached(
                t,
                Some(s),
                &labels,
                Some(&local),
                &body,
                &structure_budget,
                Some(&cache),
            ) {
                Ok(f) => f,
                // No model, moved as one body by its foot movements: rigid
                // (no flexibility), its points moved (owner 2026-11-16).
                Err(Rigid::NoModel) => {
                    return Some(match one_body(t, s, &labels, at, cases.len()) {
                        Some(Ok(f)) => Ok(f),
                        Some(Err(why)) => Err(rigid(
                            s,
                            format!(
                                "{}; NOT ON THE SAFE SIDE — its foot movements not taken: {why}; \
                                 a settlement can lean it further than solved, so its \
                                 clearances and loads can be worse; fix this to take them",
                                Rigid::NoModel
                            ),
                        )),
                        None => Err(rigid(s, Rigid::NoModel.to_string())),
                    });
                }
                Err(e) => return Some(Err(rigid(s, e.to_string()))),
            };
            let c = &flexible.condensed;
            // Per point its place's rows (points on one joint share them).
            let place: Vec<usize> = labels
                .iter()
                .map(|l| {
                    c.labels
                        .iter()
                        .position(|x| x == l)
                        .map_or(0, |k| c.place_of[k])
                })
                .collect();
            let n = labels.len();
            let mut matrix = nalgebra::DMatrix::<f64>::zeros(3 * n, 3 * n);
            for a in 0..n {
                for b in 0..n {
                    for i in 0..3 {
                        for j in 0..3 {
                            matrix[(3 * a + i, 3 * b + j)] =
                                c.flexibility[(3 * place[a] + i, 3 * place[b] + j)];
                        }
                    }
                }
            }
            let mut notes = c.notes.clone();
            notes.push(c.linearisation.to_owned());
            notes.extend(flexible.feet_notes.iter().cloned());
            if !flexible.feet.is_empty() {
                notes.push(
                    "on its measured foot movements: its points moved by them (the rigid-body \
                     motion and the elastic distortion), in every case alike"
                        .to_owned(),
                );
            }
            // Per case of `cases` (a case whose weather is unknown: none).
            let mut by_case = Vec::with_capacity(cases.len());
            let mut k = 0;
            for c in cases {
                if weather.iter().any(|w| w.name == c.weather) {
                    by_case.push(flexible.body[k].clone());
                    k += 1;
                } else {
                    by_case.push(vec![[0.0; 3]; n]);
                }
            }
            Some(Ok((
                LinearStructure {
                    structure: s.id.clone(),
                    labels,
                    flexibility: matrix,
                    reference: at.iter().map(|(_, f)| *f).collect(),
                    body: vec![[0.0; 3]; n],
                    feet: flexible.feet.clone(),
                    approximate: c.approximate,
                    notes,
                },
                by_case,
            )))
        },
        done,
        cancel,
    );
    let mut structures = Structures::default();
    let mut body = Vec::new();
    for (s, a) in placed.iter().zip(answers) {
        match a {
            Some(Some(Ok((f, b)))) => {
                structures.flexible.push(f);
                body.push(b);
            }
            Some(Some(Err(r))) => structures.rigid.push(r),
            Some(None) => {}
            None => structures
                .rigid
                .push(rigid(s, "stopped before its turn".into())),
        }
    }
    for f in &reference.failed {
        structures.notes.push(format!(
            "stringing state: section {f} — its points' reference forces missing"
        ));
    }
    let condensed = cache.lock().expect("not poisoned").len();
    (
        FlexibleSetup {
            structures,
            reference,
            condensed,
            elapsed: started.elapsed(),
        },
        body,
    )
}

/// A line's structures set up for its structure loads at calculation
/// level 3 ([`prepare_structure_loads`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadsSetup {
    /// The structures (flexible with their matrices, rigid with why), the
    /// stringing state's forces, the condensations made, the time.
    pub setup: FlexibleSetup,
    /// Per load case (by name, wind directions expanded): per flexible
    /// structure (as `setup.structures.flexible`) each point's
    /// deflection under the structure's own wind and ice of the case,
    /// unfactored, line axes, m.
    pub body: BTreeMap<String, Vec<Vec<ohl_line::Point>>>,
    /// Calculation level 4: the flexible structures' full models
    /// ([`crate::full`]; none: level 3).
    pub full: Option<crate::full::FullSetup>,
}

impl LoadsSetup {
    /// The structures for load case `case` (its name): each flexible one
    /// with its own wind and ice of the case in its deflection (level 4:
    /// in its full solves too).
    pub fn structures_for(&self, case: &str) -> Structures {
        let mut out = self.setup.structures.clone();
        out.body_loads = true;
        if let Some(body) = self.body.get(case) {
            for (s, b) in out.flexible.iter_mut().zip(body) {
                s.body.clone_from(b);
            }
        }
        out.full = self.full.as_ref().map(|f| f.for_case(Some(case)));
        out
    }

    /// The calculation level set up (3, or 4 with the full models).
    pub fn level(&self) -> CalculationLevel {
        if self.full.is_some() {
            CalculationLevel::Full
        } else {
            CalculationLevel::Linear
        }
    }
}

/// `line`'s structures set up for its structure loads at calculation
/// level 3 in load cases `cases` (wind directions expanded here as
/// [`crate::structure_loads_at`] expands them; their weather in
/// `weather`): the wires' forces at the stringing state — every section
/// by finite elements in its initial condition (the structures as
/// strung, owner 2026-11-15), in still air at its stringing temperature —
/// then per placed structure its condensation at that state and its
/// points' deflection under its own wind and ice of each case
/// (unfactored, the check's member wind and ice, or the pole check's
/// wind and ice on the shaft; one more substitution per distinct case),
/// the structures over the cores, `done` told after each (structures
/// done, of), `cancel` stopping it. Kept for every case of the run
/// ([`LoadsSetup::structures_for`]).
///
/// Expected cost: one rigid line solve, then one condensation per placed
/// structure (7 ms for the example's lattice tower, 0.5–1.2 s for real
/// towers of 2000 members), over the cores.
#[allow(clippy::too_many_arguments)]
pub fn prepare_structure_loads(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
    cases: &[LoadCase],
    budget: &Budget,
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> LoadsSetup {
    prepare_structure_loads_at(
        line,
        library,
        types,
        sections,
        weather,
        cases,
        CalculationLevel::Linear,
        budget,
        done,
        cancel,
    )
}

/// [`prepare_structure_loads`] at calculation level `level`: 3 as there;
/// 4 with each flexible structure's full model besides
/// ([`crate::full`]: its solve under the stringing state's forces, its
/// cases in its axes), `done` told per structure again. Level 2: as 3
/// (the caller does not take it).
///
/// Expected cost: level 3's, and at level 4 one full solve per flexible
/// structure more.
#[allow(clippy::too_many_arguments)]
pub fn prepare_structure_loads_at(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
    cases: &[LoadCase],
    level: CalculationLevel,
    budget: &Budget,
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> LoadsSetup {
    let cases = crate::expand_directions(cases);
    let mut strung = line.clone();
    strung.set_weather_cases(weather);
    let initial = ohl_line::condition::Calculation::ALL
        .into_iter()
        .find(|c| {
            c.condition() == ohl_line::condition::Condition::Initial
                && c.method() == ohl_line::condition::Method::Fe
        })
        .expect("an initial finite-element calculation");
    for s in &mut strung.sections {
        s.calculation = initial;
    }
    prepare_strung(
        &strung, library, types, sections, weather, &cases, level, budget, done, cancel,
    )
}

/// [`prepare_structure_loads`] on `strung`, the line as its stringing
/// state is solved (its weather cases set, each section's calculation the
/// one the reference takes), in the load cases `cases` (their wind
/// directions expanded), at `level` (4: the full models besides).
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_strung(
    strung: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    weather: &[WeatherCase],
    cases: &[LoadCase],
    level: CalculationLevel,
    budget: &Budget,
    done: Option<&(dyn Fn(usize, usize) + Sync)>,
    cancel: Option<&ohl_core::budget::CancelToken>,
) -> LoadsSetup {
    let analyses = Analyses::load(types, sections);
    let (setup, body) = setup_with(
        strung, library, types, &analyses, budget, cases, weather, done, cancel,
    );
    let full = (level == CalculationLevel::Full).then(|| {
        crate::full::prepare_full(
            strung,
            types,
            analyses,
            &setup.structures.flexible,
            cases,
            weather,
            done,
            cancel,
        )
    });
    let mut by_case: BTreeMap<String, Vec<Vec<ohl_line::Point>>> = BTreeMap::new();
    for (k, c) in cases.iter().enumerate() {
        by_case.insert(
            c.name.clone(),
            body.iter().map(|per_case| per_case[k].clone()).collect(),
        );
    }
    LoadsSetup {
        setup,
        body: by_case,
        full,
    }
}

/// A line's run at a calculation level in weather cases.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexibleRun {
    /// The level.
    pub level: CalculationLevel,
    /// The structures as set up (none flexible at level 2).
    pub setup: FlexibleSetup,
    /// Each case: solved, or failed with why (the rigid solve kept).
    pub cases: Vec<Result<FlexibleSolve, FlexibleFailure>>,
}

/// `line` solved in `cases` at calculation level `level` — level 3: its
/// structures set up ([`flexible_structures`]) and each case's interface
/// loop ([`ohl_line::solve::flexible::solve_line_flexible`], its default
/// exit clauses); level 4: besides, each flexible structure's full model
/// ([`crate::full`]) from level 3's solution; level 2: the rigid solve of
/// each case (pass 0 alone). Unfactored, weather cases, the structures'
/// own wind and ice not in their deflection (the structure loads at
/// levels 3 and 4: [`prepare_structure_loads_at`],
/// [`crate::structure_loads_at`]).
///
/// Expected cost: the setup, then per case 2–5 rigid line solves (level
/// 4: then 2–5 more, each with one full solve per flexible structure;
/// measured: theory, Structure flexibility).
pub fn solve_line_flexible(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    types: &ohl_structures::StructureLibrary,
    sections: &ohl_sections::SectionLibrary,
    cases: &[WeatherCase],
    level: CalculationLevel,
    budget: &Budget,
) -> FlexibleRun {
    let analyses = Analyses::load(types, sections);
    let setup = match level {
        CalculationLevel::Rigid => FlexibleSetup::default(),
        CalculationLevel::Linear | CalculationLevel::Full => {
            flexible_with(line, library, types, &analyses, budget)
        }
    };
    let mut structures = setup.structures.clone();
    if level == CalculationLevel::Full {
        let full = crate::full::prepare_full(
            line,
            types,
            analyses,
            &setup.structures.flexible,
            &[],
            &[],
            None,
            None,
        );
        structures.full = Some(full.for_case(None));
    }
    let options = FlexibleOptions::default();
    let cases = cases
        .iter()
        .map(|case| {
            ohl_line::solve::flexible::solve_line_flexible(
                line,
                library,
                case,
                budget,
                &structures,
                &options,
            )
        })
        .collect();
    FlexibleRun {
        level,
        setup,
        cases,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use ohl_structures_fe::condense::condense;

    use super::*;

    fn ex() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line")
    }

    fn setup() -> (ohl_structures::StructureLibrary, ohl_line::Line, Analyses) {
        let types = ohl_structures::StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
        let line = ohl_line::Line::load(&ex().join("synthetic-line-typed.ohl.json"))
            .unwrap()
            .resolve(&types, &ohl_structures::InsulatorLibrary::default())
            .unwrap();
        let analyses = Analyses::load(&types, &ohl_sections::SectionLibrary::default());
        (types, line, analyses)
    }

    fn labels(s: &ohl_line::Structure) -> Vec<String> {
        s.strings.iter().map(|st| st.top.clone()).collect()
    }

    /// Two structures of one type (the example's type-02, a full design)
    /// turned differently: one key (the condensation is the type's), the
    /// matrices the structure-axes condensation turned by each one's
    /// orientation; at a reference with the wires' weight, softer or as
    /// stiff — never refused.
    #[test]
    fn placed_structures_are_their_types_condensation_turned() {
        let (types, line, analyses) = setup();
        let budget = ohl_structures_fe::default_budget();
        let t = types.get("type-02").unwrap();
        let t2 = line.structure("T2").unwrap();
        let t4 = line.structure("T4").unwrap();
        let points = labels(t2);
        assert!(!points.is_empty());
        let a = analyses
            .flexibility(t, Some(t2), &points, None, &budget)
            .unwrap();
        let b = analyses
            .flexibility(t, Some(t4), &labels(t4), None, &budget)
            .unwrap();
        assert_eq!(a.key, b.key);
        assert_ne!(a.condensed.flexibility, b.condensed.flexibility);
        let model = analyses.model("type-02", None).unwrap();
        let local = condense(
            model,
            &points
                .iter()
                .map(|l| Attachment::of_type(l))
                .collect::<Vec<_>>(),
            &Linearisation::Unloaded,
            &budget,
        )
        .unwrap();
        let turned = local.turned(to_line_axes(t4.orientation_deg));
        assert!(
            (&turned.flexibility - &b.condensed.flexibility).abs().max()
                < 1e-12 * turned.flexibility.abs().max()
        );
        // A force across the line in T4's axes moves its points across
        // T4 (turned with it): the same as the local matrix gives.
        let f_local = [1e3, 0.0, 0.0];
        let f_line = ohl_line::links::from_structure_axes(t4.orientation_deg, f_local);
        let u_line = b.condensed.deflection(&[(points[0].as_str(), f_line)])[0];
        let u_local = local.deflection(&[(points[0].as_str(), f_local)])[0];
        let back = ohl_line::links::to_structure_axes(t4.orientation_deg, u_line);
        for k in 0..3 {
            assert!((back[k] - u_local[k]).abs() < 1e-12, "{back:?} {u_local:?}");
        }
        // At a reference: the wires' weight on the points, the
        // self-weight.
        let wires: Vec<(String, [f64; 3])> = points
            .iter()
            .map(|l| (l.clone(), [0.0, 0.0, -20e3]))
            .collect();
        let at_ref = analyses
            .flexibility(t, Some(t2), &points, Some(&wires), &budget)
            .unwrap();
        assert_ne!(at_ref.key, a.key);
        assert!(at_ref.condensed.stiffness.is_some());
    }

    /// A relink of the placed structure moves its point's place (the
    /// structure check's rule); an unlinked point, a type without a
    /// model, a pole: as the structure check takes them.
    #[test]
    fn links_poles_and_types_without_a_model() {
        let (types, line, analyses) = setup();
        let budget = ohl_structures_fe::default_budget();
        let t = types.get("type-02").unwrap();
        let mut t2 = line.structure("T2").unwrap().clone();
        let points = labels(&t2);
        let model = analyses.model("type-02", None).unwrap();
        let other = model
            .nodes
            .iter()
            .position(|n| n.id == "A2")
            .expect("joint A2");
        for p in t2.points.iter_mut().filter(|p| p.label == points[0]) {
            p.joint = Some("A2".into());
            p.offset = [0.0, 0.3, 0.0];
        }
        let relinked = analyses
            .flexibility(t, Some(&t2), &points, None, &budget)
            .unwrap();
        let place = &relinked.condensed.places[relinked.condensed.place_of[0]];
        assert_eq!(place.node, other);
        // A2 is a lattice arm's tip (trusses only): the offset's rotation
        // left out, said (as the check leaves its moment out).
        assert_eq!(place.offset.map(f64::abs), [0.0; 3]);
        assert!(
            relinked
                .condensed
                .notes
                .iter()
                .any(|n| n.contains("rotation left out")),
            "{:?}",
            relinked.condensed.notes
        );
        for p in t2.points.iter_mut().filter(|p| p.label == points[0]) {
            p.unlinked = true;
        }
        assert_eq!(
            analyses.flexibility(t, Some(&t2), &points, None, &budget),
            Err(Rigid::Unlinked(points[0].clone()))
        );
        // type-01: no model, no pole.
        let t1 = line.structure("T1").unwrap();
        assert_eq!(
            analyses.flexibility(
                types.get("type-01").unwrap(),
                Some(t1),
                &labels(t1),
                None,
                &budget
            ),
            Err(Rigid::NoModel)
        );
        // type-05: a steel pole with an elasticity.
        let t6 = line.structure("T6").unwrap();
        let pole = analyses
            .flexibility(
                types.get("type-05").unwrap(),
                Some(t6),
                &labels(t6),
                None,
                &budget,
            )
            .unwrap();
        assert!(pole.condensed.stiffness.is_some());
        assert!(
            pole.condensed
                .notes
                .iter()
                .any(|n| n.contains("NOT ON THE SAFE SIDE")),
            "{:?}",
            pole.condensed.notes
        );
    }

    /// The example line at calculation level 3: its lattice towers (T2,
    /// T4, a model) and its steel pole (T6) flexible, every other
    /// structure rigid with why; in the stringing state nothing moves; in
    /// cold, iced and windy cases the loop converges in a few passes, each
    /// point where its structure puts it under the wires' forces
    /// (u = C (F − F_ref) to 0.1 mm), the rigid solve kept bit for bit;
    /// cold, the spans at the pole slacken (tension below rigid).
    #[test]
    fn the_example_line_at_level_three() {
        let (types, line, _) = setup();
        let library = ohl_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap();
        let sections = ohl_sections::SectionLibrary::default();
        let budget = ohl_wirefe::default_fe_budget();
        let still =
            |t: f64| WeatherCase::still("case", ohl_core::units::Temperature::from_celsius(t));
        let mut iced = still(-5.0);
        iced.ice_thickness = ohl_core::units::Length::from_millimeters(15.0);
        let mut windy = still(5.0);
        windy.wind_pressure = ohl_core::units::Stress::from_pascals(450.0);
        let cases = [still(15.0), still(-20.0), iced, windy];
        let run = solve_line_flexible(
            &line,
            &library,
            &types,
            &sections,
            &cases,
            CalculationLevel::Linear,
            &budget,
        );
        let flexible: Vec<&str> = run
            .setup
            .structures
            .flexible
            .iter()
            .map(|s| s.structure.as_str())
            .collect();
        assert_eq!(
            flexible,
            ["T2", "T4", "T6"],
            "{:?}",
            run.setup.structures.rigid
        );
        let rigid: Vec<&str> = run
            .setup
            .structures
            .rigid
            .iter()
            .map(|s| s.structure.as_str())
            .collect();
        assert_eq!(rigid, ["T1", "T3", "T5", "T7"]);
        assert!(
            run.setup.structures.rigid[0]
                .reason
                .contains("no structure model"),
            "{:?}",
            run.setup.structures.rigid
        );
        for (case, r) in cases.iter().zip(&run.cases) {
            let solved = r.as_ref().unwrap_or_else(|e| panic!("{e}"));
            let plain = ohl_line::solve::solve_line_with(&line, &library, case, &budget);
            assert_eq!(format!("{plain:?}"), format!("{:?}", solved.rigid));
            assert!(solved.passes.len() <= 6, "{:?}", solved.passes);
            for s in &run.setup.structures.flexible {
                let forces: Vec<ohl_line::Point> = s
                    .labels
                    .iter()
                    .map(|l| {
                        solved
                            .points
                            .iter()
                            .find(|p| p.structure == s.structure && p.label == *l)
                            .and_then(|p| p.flexible)
                            .unwrap()
                    })
                    .collect();
                let answer =
                    ohl_line::solve::flexible::StructureResponse::answer(s, &forces).unwrap();
                for (l, u) in s.labels.iter().zip(&answer.displacement) {
                    let p = solved
                        .points
                        .iter()
                        .find(|p| p.structure == s.structure && p.label == *l)
                        .unwrap();
                    let d: f64 = (0..3)
                        .map(|c| (p.displacement[c] - u[c]).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    assert!(d < 1e-4, "{} {l}: {:?} against {u:?}", s.structure, p);
                }
            }
        }
        // The stringing state: one pass, nothing moves.
        let at_stringing = run.cases[0].as_ref().unwrap();
        assert_eq!(at_stringing.passes.len(), 1);
        assert!(
            at_stringing
                .points
                .iter()
                .all(|p| p.displacement == [0.0; 3])
        );
        // Cold, still air: the suspension structures see only the small
        // change of the unbalance — every point moves less than the
        // tolerance, the rigid solve is the answer (pass 0).
        assert_eq!(run.cases[1].as_ref().unwrap().passes.len(), 1);
        // Iced: every point moves down (by about a millimetre).
        let iced = run.cases[2].as_ref().unwrap();
        assert!(
            iced.points.iter().all(|p| p.local[2] < 0.0),
            "{:?}",
            iced.points
        );
        // Wind from the right: every point moves leeward (to the left of
        // the line), the pole's top by decimetres, the lattice towers'
        // points by centimetres.
        let windy = run.cases[3].as_ref().unwrap();
        for p in &windy.points {
            let least = if p.structure == "T6" { 0.1 } else { 0.005 };
            assert!(p.local[0] < -least, "{p:?}");
        }
        assert!(windy.passes.len() <= 4, "{:?}", windy.passes);
    }
}
