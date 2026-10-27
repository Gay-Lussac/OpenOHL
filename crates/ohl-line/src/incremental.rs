// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Incremental solving: keep every section's solution and re-solve only
//! the sections an edit touched.
//!
//! A [`SolvedLine`] holds the result of each section (by id) and of each
//! jumper for one weather case. [`SolvedLine::update`] takes the edited
//! line and the ids of the sections to recalculate (from the session's
//! dirty tracking: changed sections, sections through changed
//! structures), re-solves those, solves new sections, drops deleted ones,
//! and refreshes the jumpers (cheap, and they hang on the solved clamps).
//!
//! Sections are independent finite-element models, so the result is the
//! same as solving everything again — checked by the tests after every
//! kind of edit.

use std::collections::{BTreeMap, HashSet};

use ohl_weather::WeatherCase;
use ohl_wirefe::default_fe_budget;

use crate::jumper::{JumperShape, in_parallel};
use crate::solve::SectionShape;
use crate::{Jumper, Line, LineError, Section, Structure};

/// What an update did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Update {
    /// Sections solved (changed or new).
    pub solved: usize,
    /// Sections dropped (deleted from the line).
    pub dropped: usize,
    /// Sections kept as they were.
    pub kept: usize,
    /// Jumpers solved (the others kept).
    pub jumpers: usize,
}

/// Solutions of a line's sections and jumpers in one weather case.
#[derive(Debug, Clone)]
pub struct SolvedLine {
    case: WeatherCase,
    sections: BTreeMap<String, Result<SectionShape, LineError>>,
    jumpers: BTreeMap<String, Result<JumperShape, LineError>>,
    /// What each jumper was solved from: kept while unchanged.
    jumper_inputs: BTreeMap<String, JumperInputs>,
    /// The jumpers' loads on the clamps the sections were solved with (the
    /// estimate, or their last pull): a section whose jumpers load it
    /// otherwise now is solved again.
    jumper_weights: crate::solve::JumperLoadMap,
    /// Each jumper's last solution with everything it was solved from:
    /// solved again only when one of them changed.
    memo: JumperMemo,
}

/// What the jumpers' inputs are looked up in, made once for all of them:
/// the line's routes and the sections at each wire point.
pub(crate) struct Lookup<'a> {
    /// The wire library the line is solved with.
    library: &'a ohl_cables::Library,
    /// [`Line::alignment_views`].
    views: Vec<crate::route::AlignmentView>,
    /// The sections holding each section point (indices, in line order).
    at_point: std::collections::HashMap<&'a crate::SectionPoint, Vec<usize>>,
    /// The solved shapes the jumpers are solved between, by section (the
    /// first of a section, as a search finds it).
    shape_at: std::collections::HashMap<&'a str, &'a SectionShape>,
}

impl<'a> Lookup<'a> {
    /// Of `line`, solved with `library`, its sections solved as `shapes`.
    pub(crate) fn of(
        line: &'a Line,
        library: &'a ohl_cables::Library,
        shapes: &'a [SectionShape],
    ) -> Self {
        let mut shape_at = std::collections::HashMap::with_capacity(shapes.len());
        for s in shapes {
            shape_at.entry(s.section.as_str()).or_insert(s);
        }
        let mut at_point: std::collections::HashMap<&crate::SectionPoint, Vec<usize>> =
            Default::default();
        for (k, s) in line.sections.iter().enumerate() {
            for p in &s.points {
                let v = at_point.entry(p).or_default();
                if v.last() != Some(&k) {
                    v.push(k);
                }
            }
        }
        Self {
            library,
            views: line.alignment_views(),
            at_point,
            shape_at,
        }
    }
}

/// Jumper solutions of one weather case kept between solves (the passes
/// of the jumpers' pull, the updates after edits): a jumper is solved
/// again only when something its solution depends on changed — its
/// inputs ([`JumperInputs`]), the solved shapes of the sections at its
/// ends and of those its taps hang on, the points of the jumpers its taps
/// hang on, the loads on its points, the cables, the line's defaults,
/// solver, creep and bimetallic settings and the weather case. Equal inputs,
/// equal solution: the result is the one a new solve gives.
#[derive(Debug, Default)]
pub(crate) struct JumperMemo {
    kept: std::sync::RwLock<std::collections::HashMap<String, Kept>>,
}

impl Clone for JumperMemo {
    fn clone(&self) -> Self {
        Self {
            kept: std::sync::RwLock::new(self.kept.read().map(|m| m.clone()).unwrap_or_default()),
        }
    }
}

/// A jumper's solution with what it was solved from.
type Kept = (JumperKey, Result<JumperShape, LineError>);

/// Everything a jumper's solution is computed from.
#[derive(Debug, Clone, PartialEq)]
struct JumperKey {
    inputs: JumperInputs,
    shapes: Vec<Option<SectionShape>>,
    taps: Vec<Option<crate::Point>>,
    loads: Vec<(String, [f64; 3])>,
    cables: Vec<Option<ohl_cables::Cable>>,
    global: String,
}

impl JumperMemo {
    /// What every jumper of `line` in `case` depends on alike.
    pub(crate) fn global(line: &Line, case: &WeatherCase) -> String {
        format!(
            "{:?}",
            (
                &line.defaults,
                &line.solver,
                &line.creep_stretch,
                &line.bimetallic,
                &line.condition_cases,
                case
            )
        )
    }

    /// Jumper `jumper` solved as [`crate::jumper::solve_one_jumper_in`]
    /// solves it, or its kept solution when nothing it depends on changed
    /// (`global`: [`Self::global`]; `look`: [`Lookup::of`] the line).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn solve(
        &self,
        line: &Line,
        library: &ohl_cables::Library,
        case: &WeatherCase,
        shapes: &[SectionShape],
        jumper: &Jumper,
        context: &crate::jumper::TapContext,
        global: &str,
        look: &Lookup<'_>,
    ) -> Result<JumperShape, LineError> {
        let taps = || {
            jumper.taps().filter_map(|t| match t {
                crate::Tap::Jumper { jumper, point } => Some(
                    context
                        .points
                        .get(&(jumper.clone(), point.clone()))
                        .copied(),
                ),
                crate::Tap::Span(_) => None,
            })
        };
        let shape_of = |id: &str| look.shape_at.get(id).copied();
        // Kept: compared in place (nothing copied).
        let same = |k: &JumperKey| {
            k.global == global
                && k.loads == context.loads
                && k.inputs.matches(line, jumper, look)
                && k.shapes.len() == k.inputs.sections.len()
                && k.inputs
                    .sections
                    .iter()
                    .zip(&k.shapes)
                    .all(|(s, kept)| kept.as_ref() == shape_of(&s.id))
                && k.taps.iter().copied().eq(taps())
                && k.cables
                    .iter()
                    .map(Option::as_ref)
                    .eq(std::iter::once(&jumper.cable)
                        .chain(k.inputs.sections.iter().map(|s| &s.cable))
                        .map(|c| library.get(c)))
        };
        if let Ok(m) = self.kept.read()
            && let Some((k, r)) = m.get(&jumper.id)
            && same(k)
        {
            return r.clone();
        }
        let inputs = JumperInputs::of(line, jumper, look);
        let key = JumperKey {
            shapes: inputs
                .sections
                .iter()
                .map(|s| shape_of(&s.id).cloned())
                .collect(),
            taps: taps().collect(),
            loads: context.loads.clone(),
            cables: std::iter::once(&jumper.cable)
                .chain(inputs.sections.iter().map(|s| &s.cable))
                .map(|c| library.get(c).cloned())
                .collect(),
            global: global.to_owned(),
            inputs,
        };
        let r = crate::jumper::solve_one_jumper_in(line, library, case, shapes, jumper, context);
        if let Ok(mut m) = self.kept.write() {
            m.insert(jumper.id.clone(), (key, r.clone()));
        }
        r
    }
}

/// What a jumper's solution depends on: itself and its cable's data, the
/// sections at its ends, the structures it hangs on, and the line's
/// direction there (its wind side).
#[derive(Debug, Clone, PartialEq)]
struct JumperInputs {
    jumper: Jumper,
    /// Its cable's data (a library edit changes it).
    cable: Option<ohl_cables::Cable>,
    sections: Vec<Section>,
    structures: Vec<Structure>,
    direction: Option<[f64; 2]>,
}

impl JumperInputs {
    /// Its inputs, looked up in `look` (made once for every jumper).
    fn of(line: &Line, jumper: &Jumper, look: &Lookup<'_>) -> Self {
        Self {
            jumper: jumper.clone(),
            cable: look.library.get(&jumper.cable).cloned(),
            sections: Self::sections_of(line, jumper, look).cloned().collect(),
            // (Without their stick drawing: for views only.)
            structures: Self::structures_of(line, jumper)
                .map(|s| Structure {
                    members: Vec::new(),
                    ..s.clone()
                })
                .collect(),
            direction: Self::direction_of(line, jumper, look),
        }
    }

    /// Whether these are `jumper`'s inputs in `line` now (as [`Self::of`]
    /// would make them, compared without copying them).
    fn matches(&self, line: &Line, jumper: &Jumper, look: &Lookup<'_>) -> bool {
        self.jumper == *jumper
            && self.cable.as_ref() == look.library.get(&jumper.cable)
            && self
                .sections
                .iter()
                .eq(Self::sections_of(line, jumper, look))
            && self.structures.len() == Self::structures_of(line, jumper).count()
            && self
                .structures
                .iter()
                .zip(Self::structures_of(line, jumper))
                .all(|(a, b)| same_but_drawing(a, b))
            && self.direction == Self::direction_of(line, jumper, look)
    }

    /// The sections at its ends and those its taps hang on, in line
    /// order.
    fn sections_of<'a>(
        line: &'a Line,
        jumper: &'a Jumper,
        look: &Lookup<'_>,
    ) -> impl Iterator<Item = &'a Section> {
        let mut at: std::collections::BTreeSet<usize> = jumper
            .points()
            .filter_map(|p| look.at_point.get(p))
            .flatten()
            .copied()
            .collect();
        // The sections its taps hang on (and so the jumpers too): every
        // section of that id, as a filter by id finds them.
        let hosts: Vec<String> = jumper
            .taps()
            .filter_map(|t| line.tap_section(t, 0).map(|s| s.id.clone()))
            .collect();
        if !hosts.is_empty() {
            at.extend(
                line.sections
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| hosts.contains(&s.id))
                    .map(|(k, _)| k),
            );
        }
        at.into_iter().map(|k| &line.sections[k])
    }

    /// The structures it hangs on.
    fn structures_of<'a>(
        line: &'a Line,
        jumper: &'a Jumper,
    ) -> impl Iterator<Item = &'a Structure> {
        line.structures.iter().filter(move |s| {
            jumper.points().any(|p| p.structure == s.id)
                || jumper
                    .supports()
                    .iter()
                    .any(|c| c.structure_id(jumper) == s.id)
        })
    }

    /// The line's direction at its middle.
    fn direction_of(line: &Line, jumper: &Jumper, look: &Lookup<'_>) -> Option<[f64; 2]> {
        match (
            jumper.from.point().and_then(|p| line.wire_point(p)),
            jumper.to.point().and_then(|p| line.wire_point(p)),
        ) {
            (Some(a), Some(b)) => line.direction_at_in(
                &look.views,
                [
                    0.5 * (a.position[0] + b.position[0]),
                    0.5 * (a.position[1] + b.position[1]),
                ],
            ),
            _ => None,
        }
    }
}

/// Whether structures `a` and `b` are the same but for their stick
/// drawing ([`Structure::members`], for views only: no solution reads
/// it). Every other field compared (a field added must be added here).
fn same_but_drawing(a: &Structure, b: &Structure) -> bool {
    let Structure {
        id,
        position,
        orientation_deg,
        structure_type,
        extension,
        description,
        height_adjust,
        points,
        strings,
        wires,
        omit,
        omit_sets,
        members: _,
        routes,
        // (Read by the structure checks only, after the solve.)
        weather_groups: _,
        // (Data only: nothing solved reads it.)
        soil_resistivity_ohm_m: _,
        fields,
    } = a;
    *id == b.id
        && *position == b.position
        && *orientation_deg == b.orientation_deg
        && *structure_type == b.structure_type
        && *extension == b.extension
        && *description == b.description
        && *height_adjust == b.height_adjust
        && *points == b.points
        && *strings == b.strings
        && *wires == b.wires
        && *omit == b.omit
        && *omit_sets == b.omit_sets
        && *routes == b.routes
        && *fields == b.fields
}

fn solve_one(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    id: &str,
    jumpers: &crate::solve::JumperLoadMap,
) -> Result<SectionShape, LineError> {
    let section = line
        .sections
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| LineError::Invalid(format!("no section {id}")))?;
    let cable = crate::solve::cable_of(library, section)?;
    // The jumpers hanging on its clamps, with their own cables.
    let coupling = crate::solve::Coupling {
        jumpers: Some(jumpers.clone()),
        ..Default::default()
    };
    crate::solve::solve_section_coupled(line, section, cable, case, &default_fe_budget(), &coupling)
}

impl SolvedLine {
    /// Nothing solved yet, in `case`.
    pub fn empty(case: &WeatherCase) -> Self {
        Self {
            case: case.clone(),
            sections: BTreeMap::new(),
            jumpers: BTreeMap::new(),
            jumper_inputs: BTreeMap::new(),
            jumper_weights: BTreeMap::new(),
            memo: JumperMemo::default(),
        }
    }

    /// Solves every section and jumper of `line`.
    pub fn solve(line: &Line, library: &ohl_cables::Library, case: &WeatherCase) -> Self {
        let mut solved = Self {
            case: case.clone(),
            sections: BTreeMap::new(),
            jumpers: BTreeMap::new(),
            jumper_inputs: BTreeMap::new(),
            jumper_weights: BTreeMap::new(),
            memo: JumperMemo::default(),
        };
        let all: Vec<String> = line.sections.iter().map(|s| s.id.clone()).collect();
        solved.update(line, library, &all);
        solved
    }

    /// [`SolvedLine::solve`] taken from a memo of whole solves when the
    /// same line (every field), library and case were solved so before
    /// (performance review, 2026-10-26: the checks solving their cases
    /// again — internal clearances, phase distances). The same solution
    /// bit for bit; never a case updated edit by edit.
    pub fn solve_memo(line: &Line, library: &ohl_cables::Library, case: &WeatherCase) -> Self {
        crate::solve::live_memo(line, library, case)
    }

    /// Every section's result, by id.
    pub(crate) fn section_results(&self) -> impl Iterator<Item = &Result<SectionShape, LineError>> {
        self.sections.values()
    }

    /// The weather case.
    pub fn case(&self) -> &WeatherCase {
        &self.case
    }

    /// Brings the solutions up to date with `line` after an edit:
    /// re-solves the sections in `dirty`, solves sections not solved yet,
    /// drops sections no longer in the line, and re-solves the jumpers
    /// whose inputs changed (the jumper, the sections at its ends — or one
    /// of them re-solved —, the structures it hangs on).
    ///
    /// Expected cost: one section solve (typically < 1 ms) per dirty or new
    /// section, plus the jumpers at them (a jumper cut to its sag: its end
    /// sections solved again in its design case, a few ms).
    pub fn update(
        &mut self,
        line: &Line,
        library: &ohl_cables::Library,
        dirty: &[String],
    ) -> Update {
        let mut update = Update::default();
        let before = self.sections.len();
        let ids: HashSet<&str> = line.sections.iter().map(|s| s.id.as_str()).collect();
        let dirty_set: HashSet<&str> = dirty.iter().map(String::as_str).collect();
        self.sections.retain(|id, _| ids.contains(id.as_str()));
        update.dropped = before - self.sections.len();
        let mut resolved: HashSet<&str> = HashSet::new();
        // The jumpers' load on the clamps now: a section whose jumpers
        // load it otherwise than when it was solved (their weight, the
        // calculation options) is solved again. Pulling: the last pulls
        // where there are some (the passes below settle them again).
        let estimate = crate::jumper::end_loads(line, &self.case, |id| library.get(id));
        let pull = crate::solve::needs_pull(line);
        let mut weights = estimate.clone();
        if pull {
            for (p, f) in &self.jumper_weights {
                if estimate.contains_key(p) {
                    weights.insert(p.clone(), *f);
                }
            }
        }
        let reweighed: HashSet<&str> = line
            .sections
            .iter()
            .filter(|section| {
                let differs =
                    |k: &crate::solve::LoadAt| weights.get(k) != self.jumper_weights.get(k);
                section
                    .points
                    .iter()
                    .any(|p| differs(&crate::solve::LoadAt::Clamp(p.clone())))
                    || weights.keys().chain(self.jumper_weights.keys()).any(|k| {
                        matches!(k, crate::solve::LoadAt::Span(r) if r.section == section.id)
                            && differs(k)
                    })
            })
            .map(|s| s.id.as_str())
            .collect();
        for section in &line.sections {
            if dirty_set.contains(section.id.as_str())
                || !self.sections.contains_key(&section.id)
                || reweighed.contains(section.id.as_str())
            {
                let result = solve_one(line, library, &self.case, &section.id, &weights);
                self.sections.insert(section.id.clone(), result);
                resolved.insert(&section.id);
                update.solved += 1;
            } else {
                update.kept += 1;
            }
        }
        // Sections sharing an assembly with one solved again: solved
        // together, from each solved alone (as a full solve starts, so the
        // result does not depend on the edits before).
        let coupled = crate::solve::coupled_groups(line);
        for group in &coupled {
            if !group
                .iter()
                .any(|&k| resolved.contains(line.sections[k].id.as_str()))
            {
                continue;
            }
            for &k in group {
                let id = &line.sections[k].id;
                if !resolved.contains(id.as_str()) {
                    let r = solve_one(line, library, &self.case, id, &weights);
                    self.sections.insert(id.clone(), r);
                }
            }
            let mut results: Vec<Result<SectionShape, LineError>> = line
                .sections
                .iter()
                .map(|s| {
                    self.sections
                        .get(&s.id)
                        .cloned()
                        .unwrap_or_else(|| Err(LineError::Invalid("not solved".into())))
                })
                .collect();
            crate::solve::solve_coupled(
                line,
                library,
                &self.case,
                group,
                &mut results,
                &default_fe_budget(),
                &weights,
            );
            for &k in group {
                let id = &line.sections[k].id;
                self.sections.insert(id.clone(), results[k].clone());
                resolved.insert(id);
            }
        }
        // Sections joined by spacers with one solved again: solved together,
        // from each solved alone where it is in no assembly shared (else
        // as its assembly's group left it, as in a full solve).
        let in_coupled: HashSet<usize> = coupled.iter().flatten().copied().collect();
        for group in crate::solve::spacer_groups(line) {
            if !group
                .iter()
                .any(|&k| resolved.contains(line.sections[k].id.as_str()))
            {
                continue;
            }
            for &k in &group {
                let id = &line.sections[k].id;
                if !resolved.contains(id.as_str()) && !in_coupled.contains(&k) {
                    let r = solve_one(line, library, &self.case, id, &weights);
                    self.sections.insert(id.clone(), r);
                }
            }
            let mut results: Vec<Result<SectionShape, LineError>> = line
                .sections
                .iter()
                .map(|s| {
                    self.sections
                        .get(&s.id)
                        .cloned()
                        .unwrap_or_else(|| Err(LineError::Invalid("not solved".into())))
                })
                .collect();
            crate::solve::solve_spacer_group(
                line,
                library,
                &self.case,
                &group,
                &mut results,
                &default_fe_budget(),
                &weights,
            );
            for &k in &group {
                let id = &line.sections[k].id;
                self.sections.insert(id.clone(), results[k].clone());
                resolved.insert(id);
            }
        }
        // The jumpers' own pull, settled again on the sections (with the
        // option, or a jumper taut between its clamps).
        let mut results: Vec<Result<SectionShape, LineError>> = line
            .sections
            .iter()
            .map(|s| {
                self.sections
                    .get(&s.id)
                    .cloned()
                    .unwrap_or_else(|| Err(LineError::Invalid("not solved".into())))
            })
            .collect();
        if pull || crate::solve::may_be_taut(line, &results) {
            let before = results.clone();
            weights = crate::solve::jumper_pull_with(
                line,
                library,
                &self.case,
                &default_fe_budget(),
                &estimate,
                weights,
                &mut results,
                Some(&self.memo),
            );
            for (k, (now, was)) in results.into_iter().zip(before).enumerate() {
                if now != was {
                    let id = &line.sections[k].id;
                    self.sections.insert(id.clone(), now);
                    if resolved.insert(id) {
                        update.solved += 1;
                        update.kept = update.kept.saturating_sub(1);
                    }
                }
            }
        }
        self.jumper_weights = weights;
        let shapes = self.shapes(line);
        let mut jumpers = BTreeMap::new();
        let mut inputs = BTreeMap::new();
        let mut stale: Vec<&Jumper> = Vec::new();
        let look = Lookup::of(line, library, &shapes);
        let mut before = std::mem::take(&mut self.jumper_inputs);
        for j in &line.jumpers {
            // The inputs kept when they are still these (no copy made).
            let (now, same) = match before.remove(&j.id) {
                Some(b) if b.matches(line, j, &look) => (b, true),
                _ => (JumperInputs::of(line, j, &look), false),
            };
            let kept = same
                && !now
                    .sections
                    .iter()
                    .any(|s| resolved.contains(s.id.as_str()));
            match self.jumpers.remove(&j.id) {
                Some(r) if kept => {
                    jumpers.insert(j.id.clone(), r);
                }
                _ => stale.push(j),
            }
            inputs.insert(j.id.clone(), now);
        }
        // Those to solve again, over the cores; with taps every jumper, in
        // their order (a jumper on another's point after it, with the
        // loads it puts there).
        let case = &self.case;
        let taps = line.jumpers.iter().any(crate::Jumper::on_tap);
        if taps {
            let all = crate::jumper::solve_jumpers_memo(
                line,
                library,
                case,
                &shapes,
                &self.jumper_weights,
                Some(&self.memo),
            );
            update.jumpers = all.len();
            jumpers = line.jumpers.iter().map(|j| j.id.clone()).zip(all).collect();
        } else {
            let global = JumperMemo::global(line, case);
            let memo = &self.memo;
            let solved = in_parallel(&stale, |j| {
                memo.solve(
                    line,
                    library,
                    case,
                    &shapes,
                    j,
                    &crate::jumper::TapContext::default(),
                    &global,
                    &look,
                )
            });
            update.jumpers = stale.len();
            for (j, r) in stale.iter().zip(solved) {
                jumpers.insert(j.id.clone(), r);
            }
        }
        self.jumpers = jumpers;
        self.jumper_inputs = inputs;
        // The kept solutions of jumpers no longer in the line forgotten.
        if let Ok(mut m) = self.memo.kept.write()
            && m.len() > line.jumpers.len()
        {
            let ids: HashSet<&str> = line.jumpers.iter().map(|j| j.id.as_str()).collect();
            m.retain(|id, _| ids.contains(id.as_str()));
        }
        update
    }

    /// Solved sections in line order (failed ones left out).
    pub fn shapes(&self, line: &Line) -> Vec<SectionShape> {
        line.sections
            .iter()
            .filter_map(|s| self.sections.get(&s.id)?.as_ref().ok().cloned())
            .collect()
    }

    /// The result of every section, in line order.
    pub fn results<'a>(
        &'a self,
        line: &'a Line,
    ) -> Vec<(&'a str, &'a Result<SectionShape, LineError>)> {
        line.sections
            .iter()
            .filter_map(|s| self.sections.get(&s.id).map(|r| (s.id.as_str(), r)))
            .collect()
    }

    /// The result of every jumper, in line order.
    pub fn jumper_results<'a>(
        &'a self,
        line: &'a Line,
    ) -> Vec<(&'a str, &'a Result<JumperShape, LineError>)> {
        line.jumpers
            .iter()
            .filter_map(|j| self.jumpers.get(&j.id).map(|r| (j.id.as_str(), r)))
            .collect()
    }

    /// Solved jumpers in line order (failed ones left out).
    pub fn jumper_shapes(&self, line: &Line) -> Vec<JumperShape> {
        line.jumpers
            .iter()
            .filter_map(|j| self.jumpers.get(&j.id)?.as_ref().ok().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jumper::solve_jumpers;
    use crate::solve::solve_line;
    use ohl_core::units::Temperature;
    use std::path::Path;

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    fn assert_same_as_full(line: &Line, solved: &SolvedLine) {
        let lib = library();
        let full: Vec<SectionShape> = solve_line(line, &lib, solved.case())
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(solved.shapes(line), full);
        let jumpers: Vec<JumperShape> = solve_jumpers(line, &lib, solved.case(), &full)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert_eq!(solved.jumper_shapes(line), jumpers);
    }

    /// The line's defaults the jumpers are solved with (their stiffness
    /// factor and exit angle) changed, every section said dirty as the
    /// session says them after such an edit: the jumper solved again, as
    /// a full solve gives it (its cut and its kept solution are keyed on
    /// the defaults too).
    #[test]
    fn a_change_of_the_defaults_is_solved_again() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut line = crate::tests::example();
        line.jumpers[0].sag = Some(1.5);
        line.jumpers[0].length = None;
        let mut solved = SolvedLine::solve(&line, &lib, &case);
        assert_same_as_full(&line, &solved);
        let before = solved.jumper_shapes(&line);
        line.defaults.jumper_exit_angle_deg += 25.0;
        line.defaults.jumper_stiffness_factor *= 3.0;
        let all: Vec<String> = line.sections.iter().map(|s| s.id.clone()).collect();
        solved.update(&line, &lib, &all);
        assert_same_as_full(&line, &solved);
        assert_ne!(
            solved.jumper_shapes(&line),
            before,
            "the defaults change it"
        );
    }

    #[test]
    fn updates_match_a_full_solve() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut line = crate::tests::example();
        let mut solved = SolvedLine::solve(&line, &lib, &case);
        assert_same_as_full(&line, &solved);

        // Move T6 (branch only): only S3 is dirty.
        let t6 = line.structures.iter_mut().find(|s| s.id == "T6").unwrap();
        *t6 = t6.moved_to([t6.position[0] + 3.0, t6.position[1], t6.position[2]]);
        let u = solved.update(&line, &lib, &["S3".to_owned()]);
        assert_eq!((u.solved, u.kept, u.dropped, u.jumpers), (1, 2, 0, 1));
        assert_same_as_full(&line, &solved);

        // Change S1's stringing.
        line.sections[0].stringing = crate::Stringing::Tension {
            horizontal_tension: 23_000.0,
            temperature: 15.0,
        };
        let u = solved.update(&line, &lib, &["S1".to_owned()]);
        assert_eq!((u.solved, u.kept), (1, 2));
        assert_same_as_full(&line, &solved);

        // The jumper (S3 to S1 at T3) is kept when neither its sections,
        // its structure nor itself changed — S2 alone, or nothing.
        assert_eq!(solved.update(&line, &lib, &[]).jumpers, 0);
        let u = solved.update(&line, &lib, &["S2".to_owned()]);
        assert_eq!((u.solved, u.jumpers), (1, 0));
        assert_same_as_full(&line, &solved);
        // Itself changed: solved again, and the two sections it hangs its
        // weight on (no section said dirty).
        line.jumpers[0].sag = Some(2.0);
        let u = solved.update(&line, &lib, &[]);
        assert_eq!((u.solved, u.jumpers), (2, 1));
        assert_same_as_full(&line, &solved);
        // Its structure changed (a string at T3 heavier), no section said
        // dirty: solved again.
        let t3 = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        t3.strings[0].weight += 100.0;
        assert_eq!(solved.update(&line, &lib, &[]).jumpers, 1);
        // (Back as it was: its sections were not solved again.)
        let t3 = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        t3.strings[0].weight -= 100.0;
        assert_eq!(solved.update(&line, &lib, &[]).jumpers, 1);
        assert_same_as_full(&line, &solved);

        // Delete S2, add a copy of S3 under a new id: dropped, solved.
        line.sections.remove(1);
        let mut extra = line.sections[1].clone();
        extra.id = "S3b".into();
        line.sections.push(extra);
        let u = solved.update(&line, &lib, &[]);
        assert_eq!((u.solved, u.kept, u.dropped), (1, 2, 1));
        assert_same_as_full(&line, &solved);
    }

    /// The jumpers' own pull (Criteria › Calculation options): solved from
    /// scratch, the same as a full solve; switched on, and after an edit,
    /// the clamps where a full solve puts them (the passes settle to
    /// within their tolerance from where they start).
    #[test]
    fn updates_with_the_jumpers_pull_match_a_full_solve() {
        let lib = library();
        let case = WeatherCase::still("15", Temperature::from_celsius(15.0));
        let mut line = crate::tests::example();
        line.solver.jumper_loads = crate::JumperLoads::Pull;
        let solved = SolvedLine::solve(&line, &lib, &case);
        assert_same_as_full(&line, &solved);
        let close = |line: &Line, solved: &SolvedLine| {
            let full: Vec<SectionShape> = solve_line(line, &lib, &case)
                .into_iter()
                .map(Result::unwrap)
                .collect();
            for (a, b) in solved.shapes(line).iter().zip(&full) {
                for (p, q) in a.clamps.iter().zip(&b.clamps) {
                    let d = (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt();
                    assert!(d < 2e-3, "{} {p:?} {q:?}", a.section);
                }
            }
        };
        // Switched on from the estimate (no section said dirty).
        let mut estimate = line.clone();
        estimate.solver.jumper_loads = crate::JumperLoads::Estimate;
        let mut solved = SolvedLine::solve(&estimate, &lib, &case);
        let u = solved.update(&line, &lib, &[]);
        assert!(u.solved >= 1, "the sections at the jumper: {u:?}");
        close(&line, &solved);
        // An edit.
        line.sections[0].stringing = crate::Stringing::Tension {
            horizontal_tension: 23_000.0,
            temperature: 15.0,
        };
        solved.update(&line, &lib, &["S1".to_owned()]);
        close(&line, &solved);
    }
}
