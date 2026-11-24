// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Clearances by rule on a working set, its neighbours pulled in
//! (ADR-0013, phase 10): every wire of this line or another that can come
//! within a wire rule's distance of a checked wire is solved in the
//! check's weather case (each with its own line's settings and wires) and
//! taken as a target — other lines' wires by the rules to other lines
//! (their crossings among them), this line's wires beyond the working set
//! by the rules between this line's wires.
//!
//! **The reach** is the largest reach of the wire rules applying to the
//! case ([`ohl_clearance::Rule::reach_in_plan`]) for every potential a
//! section near can have (each system's phases: a superset of the
//! sections' own) and the highest wire there can be (the structures'
//! tops): worked out again with the lines found in the larger area until
//! it holds. The candidates are the sections whose span bounds
//! ([`crate::bound`]) come within the reach of a checked wire; each is
//! solved and its wires within the reach listed as used.
//!
//! **The same results as on the whole network**: the rules are run by one
//! function ([`run_rules`]) on the wires in one order (by model, section,
//! span) — a pair's distances, requirements and counts depend on the two
//! wires only, so every pair within reach comes out as it does with every
//! wire of the network (tested on the synthetic country, `tests`). Rules
//! to the ground and to points are not checked on the network (no terrain
//! or point cloud is served yet) and rules across weather cases neither:
//! said, never passing.

use std::collections::{BTreeMap, BTreeSet};

use ohl_clearance::case::{CaseTargets, Outcome as CaseOutcome, Skipped, check_case};
use ohl_clearance::rules::{Rule, Target};
use ohl_clearance::{Wire, WireResult};
use ohl_line::electrical::Potential;
use ohl_weather::WeatherCase;
use serde::{Deserialize, Serialize};

use crate::bound::{SegmentGrid, SpanChord};
use crate::{Neighbour, NeighbourKind, Neighbourhood, PulledIn, Reach, WorkingSet, solve_wires};

/// A wire: its model, its section (`jumper:ID` for a jumper) and span.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WireId {
    /// The model (line).
    pub model: String,
    /// The section, or `jumper:ID`.
    pub section: String,
    /// The span (0 for a jumper).
    pub span: usize,
}

impl WireId {
    /// The neighbour it belongs to.
    pub fn neighbour(&self) -> Neighbour {
        match self.section.strip_prefix("jumper:") {
            Some(j) => Neighbour::new(&self.model, NeighbourKind::Jumper, j),
            None => Neighbour::new(&self.model, NeighbourKind::Section, &self.section),
        }
    }

    /// "S1 span 2" / "jumper:J1".
    pub fn describe(&self) -> String {
        if self.section.starts_with("jumper:") {
            self.section.clone()
        } else {
            format!("{} span {}", self.section, self.span + 1)
        }
    }
}

/// What one rule found on the working set's wires.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// One result per checked wire (ground and point rules).
    Wires(Vec<(WireId, WireResult)>),
    /// Wire pairs with a checked wire (the other one of this line or of
    /// another).
    Pairs(Vec<(WireId, WireId, WireResult)>),
    /// Not checked, and why (never a pass).
    NotChecked(String),
}

/// One rule's results.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleResult {
    /// Its index in the rules.
    pub rule: usize,
    /// Its name.
    pub name: String,
    /// What it found.
    pub outcome: Outcome,
}

/// A scoped clearance check.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearanceResult {
    /// The weather case.
    pub case: String,
    /// The reach it pulled in by.
    pub reach: Reach,
    /// The rules' results.
    pub rules: Vec<RuleResult>,
    /// What it pulled in.
    pub pulled: PulledIn,
    /// What it could not do: each makes it *not all checked*.
    pub not_all_checked: Vec<String>,
    /// What it assumed (NOT ON THE SAFE SIDE where a bound did not hold).
    pub notes: Vec<String>,
}

/// The wire rules (between wires) applying to `case`, checked in it.
pub fn wire_rules<'a>(rules: &'a [Rule], case: &str) -> Vec<&'a Rule> {
    rules
        .iter()
        .filter(|r| ohl_clearance::case::applies(r, case) && r.case_pairs.is_empty())
        .filter(|r| matches!(r.target, Target::SameLine { .. } | Target::OtherLines))
        .collect()
}

/// The largest plan reach of `rules` for the potentials met and up to
/// height `top`.
pub fn rules_reach(
    rules: &[&Rule],
    own: &[Option<Potential>],
    others: &[Option<Potential>],
    top: f64,
) -> f64 {
    rules
        .iter()
        .map(|r| r.reach_in_plan(own, others, top))
        .fold(0.0, f64::max)
}

fn distinct(p: impl IntoIterator<Item = Option<Potential>>) -> Vec<Option<Potential>> {
    let mut out: Vec<Option<Potential>> = Vec::new();
    for x in p {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

/// Runs `rules` in `case` on the line's wires `line` (the checked ones
/// and this line's others) against the other lines' wires `others`, and
/// keeps the results of the wires `checked` says are checked — the one
/// function for a scoped run and for the whole network's. The wires are
/// taken in one order (by model, section, span), so that a pair is
/// measured the same way in both; the pairs are looked for at least
/// `min_reach` m apart ([`CaseTargets::min_reach`]).
pub fn run_rules(
    rules: &[Rule],
    case: &str,
    line: &[(WireId, Wire)],
    others: &[(WireId, Wire)],
    checked: &dyn Fn(&WireId) -> bool,
    min_reach: f64,
) -> Result<Vec<RuleResult>, String> {
    let mut line: Vec<&(WireId, Wire)> = line.iter().collect();
    line.sort_by(|a, b| a.0.cmp(&b.0));
    let mut others: Vec<&(WireId, Wire)> = others.iter().collect();
    others.sort_by(|a, b| a.0.cmp(&b.0));
    let wires: Vec<Wire> = line.iter().map(|(_, w)| w.clone()).collect();
    let other_wires: Vec<Wire> = others.iter().map(|(_, w)| w.clone()).collect();
    let voltages = distinct(wires.iter().map(|w| w.potential));
    let other_voltages = distinct(other_wires.iter().map(|w| w.potential));
    let found = check_case(
        rules,
        case,
        &wires,
        CaseTargets {
            voltages: &voltages,
            other_voltages: &other_voltages,
            ground: None,
            surfaces: &[],
            point_classes: &[],
            points: None,
            other_wires: &other_wires,
            min_reach,
        },
    )?;
    let mut out = Vec::new();
    for c in found {
        let rule = &rules[c.rule];
        let outcome = match c.outcome {
            CaseOutcome::Skipped(Skipped::NoOtherLines) => Outcome::Pairs(Vec::new()),
            CaseOutcome::Skipped(Skipped::AcrossCases) => Outcome::NotChecked(
                "a rule across weather cases: not checked on the network yet".into(),
            ),
            CaseOutcome::Skipped(Skipped::NoGround) => Outcome::NotChecked(
                "no ground on the network (terrain is not served yet): not checked".into(),
            ),
            CaseOutcome::Skipped(_) => Outcome::NotChecked(
                "no points on the network (point clouds are not served yet): not checked".into(),
            ),
            CaseOutcome::Wires(results) => Outcome::Wires(
                results
                    .into_iter()
                    .enumerate()
                    .filter(|(k, _)| checked(&line[*k].0))
                    .map(|(k, r)| (line[k].0.clone(), r))
                    .collect(),
            ),
            CaseOutcome::Pairs(pairs) => {
                let to_others = matches!(rule.target, Target::OtherLines);
                Outcome::Pairs(
                    pairs
                        .into_iter()
                        .filter_map(|p| {
                            let a = &line[p.a].0;
                            let b = if to_others {
                                &others[p.b].0
                            } else {
                                &line[p.b].0
                            };
                            (checked(a) || (!to_others && checked(b)))
                                .then(|| (a.clone(), b.clone(), p.result))
                        })
                        .collect(),
                )
            }
        };
        out.push(RuleResult {
            rule: c.rule,
            name: rule.name.clone(),
            outcome,
        });
    }
    Ok(out)
}

/// How far a scoped run looks for pairs: its reach and the longest piece
/// of a checked wire (pairs are looked for from three points of each
/// piece: with that much more, every pair within the reach is found
/// whatever the pieces' length — never less than on the whole network).
pub fn search_reach(reach: f64, checked: &[(WireId, Wire)]) -> f64 {
    let piece = checked
        .iter()
        .flat_map(|(_, w)| w.points.windows(2))
        .map(|s| (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]))
        .fold(0.0, f64::max);
    if reach > 0.0 {
        reach + piece + 1.0
    } else {
        0.0
    }
}

/// The clearances of working set `ws` by `rules` in weather case `case`,
/// its neighbours pulled in from `net` (see the module's notes).
///
/// Expected cost: the working set's sections solved, then the candidates
/// near it (each section solved once, on what solving it alone reads),
/// then the rules on what was found.
pub fn clearances(
    net: &mut dyn Neighbourhood,
    ws: &WorkingSet,
    rules: &[Rule],
    case: &WeatherCase,
) -> Result<ClearanceResult, String> {
    let mut not_all = Vec::new();
    let mut notes = Vec::new();
    let ws_sections: BTreeSet<String> = ws.sections.iter().cloned().collect();
    let ws_jumpers: BTreeSet<String> = ws.jumpers.iter().cloned().collect();
    let library = net.library(&ws.model)?;
    let part = net.part(&ws.model, &[], &ws.sections)?;
    let (own_wires, failed) = solve_wires(&part, &library, case, &ws_sections, Some(&ws_jumpers));
    not_all.extend(failed.into_iter().map(|f| format!("{f}: not checked")));
    for j in &ws_jumpers {
        if !part.jumpers.iter().any(|x| &x.id == j) {
            not_all.push(format!(
                "jumper {j}: not in its sections' part: not checked"
            ));
        }
    }
    let own: Vec<(WireId, Wire)> = own_wires
        .into_iter()
        .map(|w| {
            (
                WireId {
                    model: ws.model.clone(),
                    section: w.section.clone(),
                    span: w.span,
                },
                w,
            )
        })
        .collect();
    let active = wire_rules(rules, &case.name);
    let mut pulled = PulledIn::default();
    let mut same: Vec<(WireId, Wire)> = Vec::new();
    let mut others: Vec<(WireId, Wire)> = Vec::new();
    let mut reach_m = 0.0;
    if !active.is_empty() && !own.is_empty() {
        let mut grid = SegmentGrid::new(50.0);
        for (_, w) in &own {
            grid.add_polyline(&w.points);
        }
        let bounds = grid
            .bounds()
            .ok_or("the working set's wires have no place")?;
        let own_pot = net.potentials(&ws.model);
        let top_own = ohl_clearance::checks::top_of(own.iter().map(|(_, w)| w));
        let margin = net.max_margin();
        let mut r = rules_reach(&active, &own_pot, &[], top_own);
        // The reach for the lines found in the area it reaches, again
        // until it holds (it only grows).
        let (candidates, chords) = loop {
            let area = bounds.grown(r + 2.0 * margin);
            let mut by_model: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (m, s) in net.sections_in(&area) {
                if m == ws.model && ws_sections.contains(&s) {
                    continue;
                }
                by_model.entry(m).or_default().push(s);
            }
            let mut other_pot: Vec<Option<Potential>> = Vec::new();
            let mut top = top_own;
            let mut chords: BTreeMap<(String, String), Vec<SpanChord>> = BTreeMap::new();
            for (m, secs) in &mut by_model {
                secs.sort();
                secs.dedup();
                if *m != ws.model {
                    other_pot.extend(net.potentials(m));
                }
                for (s, c) in net.chords(m, secs) {
                    top = c.iter().map(|c| c.top).fold(top, f64::max);
                    chords.insert((m.clone(), s), c);
                }
            }
            let other_pot = distinct(other_pot);
            let r2 = rules_reach(&active, &own_pot, &other_pot, top);
            if r2 <= r {
                break (by_model, chords);
            }
            r = r2;
        };
        reach_m = r;
        // The candidates: a span whose bound comes within the reach.
        let mut keep: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for ((m, s), cs) in &chords {
            if cs
                .iter()
                .any(|c| grid.nearest(c.mid(), c.mid(), r + c.margin).is_some())
            {
                keep.entry(m.clone()).or_default().insert(s.clone());
            }
        }
        // A section listed in the area whose chords are not known (not
        // there any more) is said.
        for (m, secs) in &candidates {
            for s in secs {
                if !chords.contains_key(&(m.clone(), s.clone())) {
                    not_all.push(format!(
                        "line {m}: section {s} not read: its wires not taken as targets"
                    ));
                }
            }
        }
        for (m, secs) in keep {
            let list: Vec<String> = secs.iter().cloned().collect();
            let solved = net
                .part(&m, &[], &list)
                .and_then(|part| net.library(&m).map(|lib| (part, lib)));
            let (part, lib) = match solved {
                Ok(x) => x,
                Err(e) => {
                    not_all.push(format!(
                        "line {m}: {} section(s) near not read ({e}): not taken as targets",
                        list.len()
                    ));
                    continue;
                }
            };
            let (wires, failed) = solve_wires(&part, &lib, case, &secs, None);
            for f in failed {
                not_all.push(format!(
                    "line {m}: {f}: its wires not taken as targets — not all checked"
                ));
            }
            for w in wires {
                let id = WireId {
                    model: m.clone(),
                    section: w.section.clone(),
                    span: w.span,
                };
                if m == ws.model {
                    // The working set's own wires are checked, not targets.
                    let jumper = w.section.strip_prefix("jumper:");
                    if jumper.is_some_and(|j| ws_jumpers.contains(j))
                        || ws_sections.contains(&w.section)
                    {
                        continue;
                    }
                }
                if let Some(c) = chords
                    .get(&(m.clone(), w.section.clone()))
                    .and_then(|cs| cs.iter().find(|c| c.span == w.span))
                    && !c.holds(&w.points)
                {
                    notes.push(format!(
                        "NOT ON THE SAFE SIDE: line {m}, {}: the wire lies outside its \
                         assumed bound (a wire at most {} times its ends' distance): a wire \
                         farther away may have been left out, so a distance can be \
                         smaller than shown; widen the working set or check that line \
                         in its own model",
                        id.describe(),
                        crate::bound::WIRE_LENGTH_FACTOR
                    ));
                }
                let n = id.neighbour();
                let near = w.points.windows(2).any(|s| {
                    grid.nearest([s[0][0], s[0][1]], [s[1][0], s[1][1]], r)
                        .is_some()
                }) || (w.points.len() == 1
                    && grid
                        .nearest(
                            [w.points[0][0], w.points[0][1]],
                            [w.points[0][0], w.points[0][1]],
                            r,
                        )
                        .is_some());
                pulled.read.insert(n.clone());
                if near {
                    pulled.used.insert(n);
                }
                if m == ws.model {
                    same.push((id, w));
                } else {
                    others.push((id, w));
                }
            }
        }
    }
    let mut line = own.clone();
    line.extend(same);
    let checked = |id: &WireId| {
        id.model == ws.model
            && match id.section.strip_prefix("jumper:") {
                Some(j) => ws_jumpers.contains(j),
                None => ws_sections.contains(&id.section),
            }
    };
    let results = run_rules(
        rules,
        &case.name,
        &line,
        &others,
        &checked,
        search_reach(reach_m, &own),
    )?;
    notes.sort();
    notes.dedup();
    Ok(ClearanceResult {
        case: case.name.clone(),
        reach: if reach_m > 0.0 {
            Reach::Wires { metres: reach_m }
        } else {
            Reach::Own
        },
        rules: results,
        pulled,
        not_all_checked: not_all,
        notes,
    })
}
