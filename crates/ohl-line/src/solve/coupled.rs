// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Sections solved together: the jumpers' pull on what they hang on, the
//! groups coupled by strings, spacers, the coupled solve.

use super::*;

/// Where a jumper's end loads what it hangs on: a section's clamp (its
/// section point), a span point (a tap on a span), or another jumper's
/// point (a tap on a jumper: the jumper's id, the point's label).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoadAt {
    /// A clamp of the sections through a section point.
    Clamp(crate::SectionPoint),
    /// A span point.
    Span(crate::SpanPointRef),
    /// A point of a jumper.
    Jumper(String, String),
}

impl LoadAt {
    /// Where jumper end `end` loads.
    pub fn of(end: &crate::JumperEnd) -> Self {
        match end {
            crate::JumperEnd::Wire(p) => Self::Clamp(p.clone()),
            crate::JumperEnd::Tap {
                tap: crate::Tap::Span(r),
            } => Self::Span(r.clone()),
            crate::JumperEnd::Tap {
                tap: crate::Tap::Jumper { jumper, point },
            } => Self::Jumper(jumper.clone(), point.clone()),
        }
    }

    /// A tap's (not a section clamp).
    pub fn is_tap(&self) -> bool {
        !matches!(self, Self::Clamp(_))
    }
}

/// Whether the jumpers' own forces are put back on what they hang on, in
/// passes: the jumper pull option (clamps), or taps solved with the full
/// loop.
pub fn needs_pull(line: &Line) -> bool {
    line.solver.jumper_loads == crate::JumperLoads::Pull
        || (line.solver.taps == crate::TapLoads::Loop
            && line.jumpers.iter().any(crate::Jumper::on_tap))
}

/// Whether a jumper may be taut between the clamps solved in `results`
/// (its cut length short of their distance, within 0.5 %): its pull is
/// then put back on them whatever the jumper option ([`jumper_pull`]).
pub fn may_be_taut(line: &Line, results: &[Result<SectionShape, LineError>]) -> bool {
    if !line.jumpers.iter().any(|j| j.length.is_some() && !j.rigid) {
        return false;
    }
    // Each section point's clamp: from the first solved section holding
    // it (its first place there), as a search through them would find it.
    let mut sections: std::collections::HashMap<&str, &Section> = Default::default();
    for s in &line.sections {
        sections.entry(s.id.as_str()).or_insert(s);
    }
    let mut clamps: std::collections::HashMap<&crate::SectionPoint, Vector3<f64>> =
        Default::default();
    for s in results.iter().filter_map(|r| r.as_ref().ok()) {
        let Some(sec) = sections.get(s.section.as_str()) else {
            continue;
        };
        for (k, q) in sec.points.iter().enumerate() {
            if sec.points[..k].contains(q) {
                continue;
            }
            if let Some(c) = s.clamps.get(k) {
                clamps.entry(q).or_insert_with(|| v(*c));
            }
        }
    }
    line.jumpers.iter().any(|j| {
        let Some(length) = j.length.filter(|_| !j.rigid) else {
            return false;
        };
        let at = |e: &crate::JumperEnd| e.point().and_then(|p| clamps.get(p).copied());
        match (at(&j.from), at(&j.to)) {
            (Some(a), Some(b)) => (b - a).norm() > 0.995 * length,
            _ => false,
        }
    })
}

/// Jumper loads on what they hang on (the whole bundle's, N).
pub type JumperLoadMap = std::collections::BTreeMap<LoadAt, [f64; 3]>;

/// Every section, the jumpers' loads `jumpers` on their clamps, spread
/// over the cores (sections are independent finite-element models);
/// sections sharing an assembly then together.
pub(super) fn solve_sections_with(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    jumpers: &JumperLoadMap,
) -> Vec<Result<SectionShape, LineError>> {
    let coupling = Coupling {
        jumpers: Some(jumpers.clone()),
        ..Coupling::default()
    };
    let all: Vec<&Section> = line.sections.iter().collect();
    let mut results = crate::jumper::in_parallel(&all, |section| {
        let cable = cable_of(library, section)?;
        solve_section_coupled(line, section, cable, case, budget, &coupling)
    });
    for group in coupled_groups(line) {
        solve_coupled(line, library, case, &group, &mut results, budget, jumpers);
    }
    for group in spacer_groups(line) {
        solve_spacer_group(line, library, case, &group, &mut results, budget, jumpers);
    }
    results
}

/// Passes of the jumpers' pull (exit clause; the clamps settle in two or
/// three).
pub const JUMPER_PULL_PASSES: usize = 8;

/// Passes at most with a taut jumper: its pull on the clamps relaxed
/// ([`jumper_pull`]).
pub const TAUT_PULL_PASSES: usize = 40;

/// The jumpers' own pull on the clamps ([`crate::JumperLoads::Pull`]):
/// the jumpers solved between the solved clamps, the force each exerts
/// on its clamps ([`crate::jumper::JumperShape::end_forces`]) put on the
/// sections in place of the estimate, the sections whose clamps it
/// changes solved again (with their groups), and so on until it changes
/// less than 0.5 N (or 1e-4 of itself). A clamp whose jumpers did not all
/// solve keeps the estimate. `used`: the loads the sections in `results`
/// were solved with; returns those of the last pass.
///
/// Expected cost: per pass, the jumpers and the sections at them solved
/// again (two or three passes: about twice to three times the time of the
/// sections at jumpers; the others are not touched).
pub fn jumper_pull(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    estimate: &JumperLoadMap,
    used: JumperLoadMap,
    results: &mut [Result<SectionShape, LineError>],
) -> JumperLoadMap {
    jumper_pull_with(line, library, case, budget, estimate, used, results, None)
}

/// [`jumper_pull`], the jumpers' solutions kept in `memo` (when given)
/// and taken from it while nothing they depend on changed (the same
/// result).
#[allow(clippy::too_many_arguments)]
pub(crate) fn jumper_pull_with(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    budget: &Budget,
    estimate: &JumperLoadMap,
    mut used: JumperLoadMap,
    results: &mut [Result<SectionShape, LineError>],
    memo: Option<&crate::incremental::JumperMemo>,
) -> JumperLoadMap {
    if line.jumpers.is_empty() {
        return used;
    }
    let groups = coupled_groups(line);
    // Taut jumpers' pull on their clamps, relaxed (a stiff coupling):
    // Aitken's step from the last two residuals.
    let mut omega = 0.3_f64;
    let mut last_r: Option<Vec<Vector3<f64>>> = None;
    let mut taut_seen = false;
    for pass in 0..TAUT_PULL_PASSES {
        if pass >= JUMPER_PULL_PASSES && !taut_seen {
            break;
        }
        let shapes: Vec<SectionShape> = results
            .iter()
            .filter_map(|r| r.as_ref().ok().cloned())
            .collect();
        let solved = crate::jumper::solve_jumpers_memo(line, library, case, &shapes, &used, memo);
        // The pull on each clamp or tap: the sum of its jumpers', where all
        // of them solved; else the estimate. Clamps pulled with the jumper
        // pull option, taps with the full loop.
        let pull_clamps = line.solver.jumper_loads == crate::JumperLoads::Pull;
        let pull_taps = line.solver.taps == crate::TapLoads::Loop;
        let mut next = estimate.clone();
        let mut failed: std::collections::BTreeSet<LoadAt> = Default::default();
        let mut pulled: JumperLoadMap = Default::default();
        // The clamps of taut jumpers: always pulled.
        let mut taut: std::collections::BTreeSet<LoadAt> = Default::default();
        for (j, r) in line.jumpers.iter().zip(&solved) {
            let ends = [LoadAt::of(&j.from), LoadAt::of(&j.to)];
            if let Ok(s) = r
                && s.taut
            {
                taut.extend(ends.iter().cloned());
                taut_seen = true;
            }
            match r {
                Ok(s) => {
                    let chained = crate::jumper::chained_pulls(line, s)
                        .into_iter()
                        .map(|(p, f)| (LoadAt::Clamp(p), f));
                    for (p, f) in ends.into_iter().zip(s.end_forces).chain(chained) {
                        let e = pulled.entry(p).or_insert([0.0; 3]);
                        for c in 0..3 {
                            e[c] += f[c];
                        }
                    }
                }
                Err(_) => failed.extend(ends),
            }
        }
        // Taut clamps relaxed: from the loads used, a step of `omega` of
        // the residual.
        let keys: Vec<LoadAt> = taut
            .iter()
            .filter(|p| !failed.contains(p))
            .cloned()
            .collect();
        let r: Vec<Vector3<f64>> = keys
            .iter()
            .map(|p| {
                let g = v(pulled.get(p).copied().unwrap_or([0.0; 3]));
                g - v(used.get(p).copied().unwrap_or([0.0; 3]))
            })
            .collect();
        if let Some(prev) = &last_r
            && prev.len() == r.len()
        {
            let dr: Vec<Vector3<f64>> = r.iter().zip(prev).map(|(a, b)| a - b).collect();
            let den: f64 = dr.iter().map(|x| x.norm_squared()).sum();
            if den > 0.0 {
                let num: f64 = prev.iter().zip(&dr).map(|(a, b)| a.dot(b)).sum();
                omega = (-omega * num / den).clamp(0.02, 1.0);
            }
        }
        last_r = Some(r.clone());
        for (p, f) in pulled {
            let wanted = if p.is_tap() { pull_taps } else { pull_clamps };
            if let Some(k) = keys.iter().position(|q| *q == p) {
                let u = v(used.get(&p).copied().unwrap_or([0.0; 3])) + r[k] * omega;
                next.insert(p, [u.x, u.y, u.z]);
            } else if wanted && !failed.contains(&p) {
                next.insert(p, f);
            }
        }
        // What changed: the sections whose clamps' or span points' loads
        // did; a jumper's points' (then solved again in the next pass).
        let moved = |p: &LoadAt| {
            let a = v(next.get(p).copied().unwrap_or([0.0; 3]));
            let b = v(used.get(p).copied().unwrap_or([0.0; 3]));
            (a - b).norm() > 0.5_f64.max(1e-4 * a.norm())
        };
        let changed: Vec<&LoadAt> = next
            .keys()
            .chain(used.keys())
            .filter(|p| moved(p))
            .collect();
        let again: Vec<usize> = line
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                changed.iter().any(|p| match p {
                    LoadAt::Clamp(q) => s.points.contains(q),
                    LoadAt::Span(r) => r.section == s.id,
                    LoadAt::Jumper(..) => false,
                })
            })
            .map(|(k, _)| k)
            .collect();
        let on_jumpers = changed.iter().any(|p| matches!(p, LoadAt::Jumper(..)));
        used = next;
        if again.is_empty() {
            if on_jumpers {
                continue;
            }
            return used;
        }
        let coupling = Coupling {
            jumpers: Some(used.clone()),
            ..Coupling::default()
        };
        // Over the cores.
        let solved = crate::jumper::in_parallel(&again, |&k| {
            let section = &line.sections[k];
            cable_of(library, section).and_then(|cable| {
                solve_section_coupled(line, section, cable, case, budget, &coupling)
            })
        });
        for (&k, r) in again.iter().zip(solved) {
            results[k] = r;
        }
        for group in groups
            .iter()
            .filter(|g| g.iter().any(|k| again.contains(k)))
        {
            solve_coupled(line, library, case, group, results, budget, &used);
        }
        for group in spacer_groups(line)
            .iter()
            .filter(|g| g.iter().any(|k| again.contains(k)))
        {
            solve_spacer_group(line, library, case, group, results, budget, &used);
        }
    }
    used
}

pub(crate) fn cable_of<'a>(
    library: &'a ohl_cables::Library,
    section: &Section,
) -> Result<&'a Cable, LineError> {
    library.get(&section.cable).ok_or_else(|| {
        LineError::Invalid(format!(
            "section {}: wire type \"{}\" not in the libraries",
            section.id, section.cable
        ))
    })
}

/// The groups of sections (indices) whose assemblies are joined — a
/// section ending on a string that another one's string hangs from too (a
/// TAK) — with more than one section each.
pub fn coupled_groups(line: &Line) -> Vec<Vec<usize>> {
    let n = line.sections.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    // The section ending at each (structure, set).
    let mut at: std::collections::BTreeMap<(&str, u32), Vec<usize>> = Default::default();
    for (k, sec) in line.sections.iter().enumerate() {
        for p in &sec.points {
            at.entry((p.structure.as_str(), p.set)).or_default().push(k);
        }
    }
    // The structures by id (the first of an id, as `Line::structure`
    // finds it).
    let mut by_id: std::collections::HashMap<&str, &crate::Structure> = Default::default();
    for s in &line.structures {
        by_id.entry(s.id.as_str()).or_insert(s);
    }
    for (k, sec) in line.sections.iter().enumerate() {
        for p in &sec.points {
            let Some(s) = by_id.get(p.structure.as_str()).copied() else {
                continue;
            };
            let Some(w) = s.wire_at_set(p.set) else {
                continue;
            };
            let c = s.component(&w.label);
            for &i in &c.wires {
                let Some(set) = s.wires[i].set else { continue };
                for &j in at.get(&(p.structure.as_str(), set)).into_iter().flatten() {
                    let (a, b) = (find(&mut parent, k), find(&mut parent, j));
                    if a != b {
                        parent[a.max(b)] = a.min(b);
                    }
                }
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for k in 0..n {
        let r = find(&mut parent, k);
        groups.entry(r).or_default().push(k);
    }
    groups.into_values().filter(|g| g.len() > 1).collect()
}

/// Passes of the coupling between sections sharing an assembly (exit
/// clause; a TAK settles in a few).
pub const COUPLING_PASSES: usize = 30;

/// The groups of sections (indices) joined by spacers, with more than one
/// section each.
pub fn spacer_groups(line: &Line) -> Vec<Vec<usize>> {
    let n = line.sections.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let index = |id: &str| line.sections.iter().position(|s| s.id == id);
    for s in &line.spacers {
        if let (Some(a), Some(b)) = (index(&s.a.section), index(&s.b.section)) {
            let (x, y) = (find(&mut parent, a), find(&mut parent, b));
            if x != y {
                parent[x.max(y)] = x.min(y);
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for k in 0..n {
        let r = find(&mut parent, k);
        groups.entry(r).or_default().push(k);
    }
    groups.into_values().filter(|g| g.len() > 1).collect()
}

/// Passes of the spacers' coupling (exit clause; a few in practice).
pub const SPACER_PASSES: usize = 40;

/// Solves a group of sections joined by spacers ([`spacer_groups`]): each
/// spacer is held by the section at its end `a`, as a bar to where the
/// section at `b` puts its span point, and pulls that point with the
/// bar's force, half way from the last pass (they overshoot otherwise);
/// the sections are solved in turn until the forces change less than
/// 1 N (or 1e-4 of themselves). A spacer without a length takes the
/// distance between its ends in still air at the sections' stringing
/// temperatures. Each end carries half its weight and the wind on it
/// (across the span, from the weather case's side). `results` holds the
/// sections solved alone and gets the coupled ones; not settling in
/// [`SPACER_PASSES`] passes is an error for the group. Sections also
/// sharing an assembly with another (a TAK) are not supported with
/// spacers yet (an error).
///
/// Expected cost: a few solves of each section of the group.
pub fn solve_spacer_group(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    group: &[usize],
    results: &mut [Result<SectionShape, LineError>],
    budget: &Budget,
    jumpers: &JumperLoadMap,
) {
    let fail = |results: &mut [Result<SectionShape, LineError>], m: String| {
        for &k in group {
            results[k] = Err(LineError::Invalid(m.clone()));
        }
    };
    let in_tak = coupled_groups(line);
    if let Some(&k) = group.iter().find(|k| in_tak.iter().any(|g| g.contains(k))) {
        fail(
            results,
            format!(
                "section {}: spacers on a section sharing an assembly are not supported yet",
                line.sections[k].id
            ),
        );
        return;
    }
    let index = |id: &str| line.sections.iter().position(|s| s.id == id);
    let spacers: Vec<&crate::Spacer> = line
        .spacers
        .iter()
        .filter(|s| index(&s.a.section).is_some_and(|k| group.contains(&k)))
        .collect();
    let point_at =
        |results: &[Result<SectionShape, LineError>], end: &crate::SpanPointRef| -> Option<Point> {
            let k = index(&end.section)?;
            results[k]
                .as_ref()
                .ok()?
                .span_points
                .iter()
                .find(|(l, _)| *l == end.point)
                .map(|(_, p)| *p)
        };
    // Lengths: given, else in still air at stringing.
    let mut lengths = std::collections::BTreeMap::new();
    let mut still: std::collections::BTreeMap<usize, Result<SectionShape, LineError>> =
        Default::default();
    for s in &spacers {
        let length = match s.length {
            Some(l) => l,
            None => {
                let mut at = Vec::new();
                for end in [&s.a, &s.b] {
                    let Some(k) = index(&end.section) else {
                        continue;
                    };
                    let shape = still.entry(k).or_insert_with(|| {
                        let section = &line.sections[k];
                        let calm = WeatherCase::still(
                            "stringing",
                            ohl_core::units::Temperature::from_celsius(
                                section.stringing.temperature(),
                            ),
                        );
                        let coupling = Coupling {
                            jumpers: Some(Default::default()),
                            ..Coupling::default()
                        };
                        cable_of(library, section).and_then(|cable| {
                            solve_section_coupled(line, section, cable, &calm, budget, &coupling)
                        })
                    });
                    if let Some(p) = shape
                        .as_ref()
                        .ok()
                        .and_then(|x| x.span_points.iter().find(|(l, _)| *l == end.point))
                    {
                        at.push(v(p.1));
                    }
                }
                match at.as_slice() {
                    [a, b] => (a - b).norm(),
                    _ => {
                        fail(
                            results,
                            format!("spacer {}: its ends did not solve at stringing", s.id),
                        );
                        return;
                    }
                }
            }
        };
        lengths.insert(s.id.clone(), length);
    }
    // The wind on a spacer, half on each end: across its span.
    let wind_of = |results: &[Result<SectionShape, LineError>], s: &crate::Spacer| -> Point {
        if case.wind_pressure.si() <= 0.0 || s.wind_area <= 0.0 {
            return [0.0; 3];
        }
        let Some(k) = index(&s.a.section) else {
            return [0.0; 3];
        };
        let Some(span) = line.sections[k]
            .span_points
            .iter()
            .find(|p| p.label == s.a.point)
            .map(|p| p.span)
        else {
            return [0.0; 3];
        };
        let Some(shape) = results[k].as_ref().ok() else {
            return [0.0; 3];
        };
        let d = v(shape.clamps[span + 1]) - v(shape.clamps[span]);
        let h = d.xy();
        if h.norm() == 0.0 {
            return [0.0; 3];
        }
        let left = Vector3::new(-h.y, h.x, 0.0) / h.norm();
        // The span's local wind (roadmap O15).
        let local = line.sections[k].wind_factor(span);
        p(left * (0.5 * case.wind_pressure.si() * local * s.wind_area * case.wind_from.to_left()))
    };
    // The forces on the pulled ends, one vector per spacer (in `spacers`'
    // order): fixed point of g(f) — the pulled sections solved under f,
    // then the holding sections with their bars to where those put the
    // ends give the bars' forces — with Aitken's relaxation.
    let n = spacers.len();
    let mut f: Vec<Vector3<f64>> = vec![Vector3::zeros(); n];
    let mut last: Option<Vec<Vector3<f64>>> = None;
    let mut omega = 0.5;
    let solve_with = |results: &mut [Result<SectionShape, LineError>],
                      sections: &[usize],
                      f: &[Vector3<f64>]| {
        let mut couplings: std::collections::BTreeMap<usize, SpacerCoupling> = Default::default();
        for (s, force) in spacers.iter().zip(f) {
            let (Some(a), Some(b)) = (index(&s.a.section), index(&s.b.section)) else {
                continue;
            };
            let wind = wind_of(results, s);
            if let Some(at) = point_at(results, &s.b) {
                couplings.entry(a).or_default().holds.push(SpacerHold {
                    id: s.id.clone(),
                    point: s.a.point.clone(),
                    at,
                    length: lengths[&s.id],
                    stiffness: s.stiffness.unwrap_or(line.defaults.insulator_stiffness),
                    tension_only: s.compression.is_none(),
                    weight: s.weight,
                    wind,
                    compression: s.compression,
                });
            }
            // The other end: the bar's force, half its weight, the wind.
            let pull = force + Vector3::new(0.0, 0.0, -0.5 * s.weight) + v(wind);
            let e = couplings
                .entry(b)
                .or_default()
                .pulls
                .entry(s.b.point.clone())
                .or_insert([0.0; 3]);
            for c in 0..3 {
                e[c] += pull[c];
            }
        }
        let solved = crate::jumper::in_parallel(sections, |&k| {
            let section = &line.sections[k];
            let coupling = Coupling {
                jumpers: Some(jumpers.clone()),
                spacers: couplings.get(&k).cloned().unwrap_or_default(),
                ..Coupling::default()
            };
            cable_of(library, section).and_then(|cable| {
                solve_section_coupled(line, section, cable, case, budget, &coupling)
            })
        });
        for (&k, r) in sections.iter().zip(solved) {
            results[k] = r;
        }
    };
    // The sections pulled, then those holding (a section may be both).
    let pulled: Vec<usize> = group
        .iter()
        .copied()
        .filter(|k| spacers.iter().any(|s| index(&s.b.section) == Some(*k)))
        .collect();
    let holding: Vec<usize> = group
        .iter()
        .copied()
        .filter(|k| spacers.iter().any(|s| index(&s.a.section) == Some(*k)))
        .collect();
    for _ in 0..SPACER_PASSES {
        solve_with(results, &pulled, &f);
        solve_with(results, &holding, &f);
        // g(f): the bars' forces on the pulled ends.
        let mut g = Vec::with_capacity(n);
        for s in &spacers {
            let Some(a) = index(&s.a.section) else { return };
            let Ok(shape) = &results[a] else {
                return;
            };
            let Some(state) = shape.spacers.iter().find(|x| x.id == s.id) else {
                fail(
                    results,
                    format!("spacer {}: not solved on {}", s.id, s.a.section),
                );
                return;
            };
            g.push(v(state.on_other));
        }
        let r: Vec<Vector3<f64>> = g.iter().zip(&f).map(|(g, f)| g - f).collect();
        let size = r.iter().map(Vector3::norm).fold(0.0, f64::max);
        let scale = g.iter().map(Vector3::norm).fold(0.0, f64::max);
        if size < 1.0_f64.max(1e-4 * scale) {
            // Settled: the sections as solved with f ≈ g. The pulled ones
            // once more with g, so both sides agree.
            solve_with(results, &pulled, &g);
            return;
        }
        // Aitken: the step from the last two residuals.
        if let Some(prev) = &last {
            let dr: Vec<Vector3<f64>> = r.iter().zip(prev).map(|(a, b)| a - b).collect();
            let den: f64 = dr.iter().map(|x| x.norm_squared()).sum();
            if den > 0.0 {
                let num: f64 = prev.iter().zip(&dr).map(|(a, b)| a.dot(b)).sum();
                omega = (-omega * num / den).clamp(-1.0, 1.0);
                if omega.abs() < 0.05 {
                    omega = 0.05_f64.copysign(omega);
                }
            }
        }
        for (fi, ri) in f.iter_mut().zip(&r) {
            *fi += ri * omega;
        }
        last = Some(r);
    }
    fail(
        results,
        format!("the spacers did not settle in {SPACER_PASSES} passes"),
    );
}

/// Solves a group of sections sharing assemblies together (Dirichlet–
/// Neumann): one section through a shared assembly solves it, the other
/// sections' wires pulling on their clamps in it; the others hold their
/// clamp where that solution puts it, which gives their pull for the next
/// pass. The stiffest section solves it (`coupling_solvers`): a section
/// held there that pulls much harder for a small move of its clamp than
/// the solving one holds it back (a short taut span against a long
/// section on one swinging string) would make the pulls overshoot in turn.
/// The pulls are relaxed by Aitken's factor from the last two passes
/// (`relaxed`), half way at first. Until the held clamps' pulls change
/// less than 0.1 N (or 1e-6 of themselves). `results` holds the sections'
/// solutions, solved alone, and gets the coupled ones; a section that fails
/// keeps its error, the others then stay as solved alone. Not settling in
/// [`COUPLING_PASSES`] passes is an error for the group, naming the pull
/// still changing.
///
/// Expected cost: a few solves of each section of the group (5–15 passes
/// on the existing lines' TAKs).
pub fn solve_coupled(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    group: &[usize],
    results: &mut [Result<SectionShape, LineError>],
    budget: &Budget,
    jumpers: &JumperLoadMap,
) {
    type Points = std::collections::BTreeMap<crate::SectionPoint, Point>;
    let solvers = coupling_solvers(line, library, case, group, results);
    let pulls_of = |results: &[Result<SectionShape, LineError>]| -> Option<Points> {
        let mut pulls = Points::new();
        for &k in group {
            let shape = results[k].as_ref().ok()?;
            for (p, f) in line.sections[k].points.iter().zip(&shape.pulls) {
                pulls.insert(p.clone(), *f);
            }
        }
        Some(pulls)
    };
    // Where the solved assemblies put the clamps of the group's points
    // (from the section solving each).
    let held_of = |results: &[Result<SectionShape, LineError>]| -> Points {
        let mut held = Points::new();
        for &k in group {
            let Ok(shape) = &results[k] else { continue };
            for (p, nodes) in line.sections[k].points.iter().zip(&shape.nodes) {
                let Some(s) = line.structure(&p.structure) else {
                    continue;
                };
                for (label, at) in nodes {
                    if let Some(set) = s.wire(label).and_then(|w| w.set)
                        && set != p.set
                    {
                        let q = crate::SectionPoint::new(p.structure.clone(), set);
                        if solvers.get(&q).is_none_or(|id| *id == line.sections[k].id) {
                            held.insert(q, *at);
                        }
                    }
                }
            }
        }
        held
    };
    let Some(mut pulls) = pulls_of(results) else {
        return;
    };
    let mut last: Option<(Vec<Vector3<f64>>, f64)> = None;
    let mut still = String::new();
    for _ in 0..COUPLING_PASSES {
        for &k in group {
            let section = &line.sections[k];
            let coupling = Coupling {
                pulls: pulls.clone(),
                held: held_of(results),
                solvers: solvers.clone(),
                jumpers: Some(jumpers.clone()),
                ..Coupling::default()
            };
            results[k] = cable_of(library, section).and_then(|cable| {
                solve_section_coupled(line, section, cable, case, budget, &coupling)
            });
        }
        let Some(next) = pulls_of(results) else {
            return;
        };
        // The pulls another section's solution takes: those on the clamps
        // held there (the others are only results).
        let held = held_of(results);
        let mut keys = Vec::new();
        let mut r = Vec::new();
        let (mut change, mut worst) = (0.0_f64, None);
        for (p, f) in &next {
            if !held.contains_key(p) {
                pulls.insert(p.clone(), *f);
                continue;
            }
            let d = v(*f) - v(pulls.get(p).copied().unwrap_or(*f));
            let c = d.norm() / (0.1 + 1e-6 * v(*f).norm());
            if c > change {
                (change, worst) = (c, Some((p, d.norm())));
            }
            keys.push(p.clone());
            r.push(d);
        }
        if change < 1.0 {
            return;
        }
        if let Some((p, d)) = worst {
            still = format!(
                " (the pull on a clamp of set {} at {} still changing by {d:.0} N)",
                p.set, p.structure
            );
        }
        let omega = relaxed(&mut last, r.clone());
        for (point, d) in keys.into_iter().zip(r) {
            let f = v(pulls.get(&point).copied().unwrap_or([0.0; 3])) + d * omega;
            pulls.insert(point, p(f));
        }
    }
    for &k in group {
        results[k] = Err(LineError::Invalid(format!(
            "section {}: the sections sharing its strings did not settle together in \
             {COUPLING_PASSES} passes{still}",
            line.sections[k].id
        )));
    }
}

/// Aitken's relaxation factor for a fixed-point pass with residual `r`
/// (the pass's result less what it took), from the last pass's residual
/// and factor in `last` (updated): ½ at first, then between 0.02 and 1.
pub(super) fn relaxed(last: &mut Option<(Vec<Vector3<f64>>, f64)>, r: Vec<Vector3<f64>>) -> f64 {
    let mut omega = 0.5;
    if let Some((prev, w)) = last.as_ref() {
        omega = *w;
        if prev.len() == r.len() {
            let dr: Vec<Vector3<f64>> = r.iter().zip(prev).map(|(a, b)| a - b).collect();
            let den: f64 = dr.iter().map(Vector3::norm_squared).sum();
            if den > 0.0 {
                let num: f64 = prev.iter().zip(&dr).map(|(a, b)| a.dot(b)).sum();
                omega = (-w * num / den).clamp(0.02, 1.0);
            }
        }
    }
    *last = Some((r, omega));
    omega
}

/// The section solving each assembly shared in `group` (its id), by the
/// section points on it: the stiffest along the line at its end there, of
/// those solved alone in `results` — the least compliance
/// \\( \sum_i L_i / EA + w^2 L_i^3 / (12 H_i^3) \\) over its spans (span
/// lengths \\( L_i \\), horizontal tensions \\( H_i \\), the case's load per
/// metre \\( w \\): the ruling span's), the first in the line's order of
/// equals. An assembly with a section not solved alone keeps the first
/// section through it.
pub(super) fn coupling_solvers(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    group: &[usize],
    results: &[Result<SectionShape, LineError>],
) -> std::collections::BTreeMap<crate::SectionPoint, String> {
    let compliance = |k: usize| -> Option<f64> {
        let shape = results[k].as_ref().ok()?;
        let cable = cable_of(library, &line.sections[k]).ok()?;
        let ea = cable.axial_stiffness().si();
        let w = load_magnitude(case, cable);
        let c: f64 = shape
            .spans
            .iter()
            .filter_map(|s| {
                let (a, b) = (s.points.first()?, s.points.last()?);
                let l = (v(*b) - v(*a)).xy().norm();
                let h = s.horizontal_tension.max(1.0);
                Some(l / ea + w * w * l.powi(3) / (12.0 * h.powi(3)))
            })
            .sum();
        c.is_finite().then_some(c)
    };
    let mut out = std::collections::BTreeMap::new();
    // The group's points by assembly: (structure, its sets).
    let mut assemblies: std::collections::BTreeMap<(&str, Vec<u32>), Vec<usize>> =
        Default::default();
    for &k in group {
        for p in &line.sections[k].points {
            let Some(s) = line.structure(&p.structure) else {
                continue;
            };
            let Some(w) = s.wire_at_set(p.set) else {
                continue;
            };
            let mut sets: Vec<u32> = s
                .component(&w.label)
                .wires
                .iter()
                .filter_map(|&i| s.wires[i].set)
                .collect();
            sets.sort_unstable();
            sets.dedup();
            if sets.len() > 1 {
                let e = assemblies.entry((p.structure.as_str(), sets)).or_default();
                if !e.contains(&k) {
                    e.push(k);
                }
            }
        }
    }
    for ((structure, sets), sections) in assemblies {
        let mut through: Vec<usize> = sections;
        through.sort_unstable();
        let mut best: Option<(usize, f64)> = None;
        for &k in &through {
            let Some(c) = compliance(k) else {
                best = None;
                break;
            };
            if best.is_none_or(|(_, b)| c < b) {
                best = Some((k, c));
            }
        }
        let Some((k, _)) = best else { continue };
        for set in sets {
            out.insert(
                crate::SectionPoint::new(structure.to_owned(), set),
                line.sections[k].id.clone(),
            );
        }
    }
    out
}
