// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The loads on shared towers (ADR-0013, phase 10; owner 2026-11-16:
//! "shared towers checked under each line's cases, the worst kept").
//!
//! A tower carrying wires of several lines is, on a network, one
//! structure in each line's model — **twins**: of the same type, within
//! [`crate::reach::TWIN_TOLERANCE`] in plan (the network's import lists
//! them so). Each line's sections hang on its own twin's points. For a
//! structure of the working set with twins:
//!
//! - **under each line's load cases** (its own library's, else the
//!   shipped defaults): every line's sections on the tower solved in that
//!   case — each with its own line's settings and wires, what solving them
//!   alone reads (`Session::drawing_part`) — and the loads at its twin's
//!   points taken;
//! - **one tower's loads**: at a point holding one line's sections, that
//!   line's load there; at a point holding none, the case's line's own
//!   twin's (its strings only); at a point holding several lines'
//!   sections, their loads added (each line's strings there counted
//!   again: more load, conservative, said); the wind on the body once,
//!   the case's line's;
//! - **the worst kept**: per point and component the largest factored
//!   load by size, with the line and case giving it.
//!
//! Each line's own structure groups decide which of its cases its twin
//! takes; the other lines' sections are solved in every case the twin
//! takes (their own groups set aside: the tower is the case's line's).
//! Solved rigid (calculation level 2): a line set to level 3 or 4 has
//! its own loads there larger where the structures' flexibility adds —
//! NOT ON THE SAFE SIDE, said with the tower.

use std::collections::BTreeMap;

use ohl_core::grid::Bounds;
use ohl_line::solve::flexible::CalculationLevel;
use ohl_loads::{AttachmentLoad, CaseLoads};
use serde::{Deserialize, Serialize};

use crate::reach::TWIN_TOLERANCE;
use crate::{Neighbour, NeighbourKind, Neighbourhood, PulledIn, Reach, WorkingSet};

/// A point's load in one case, the lines' added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointLoad {
    /// The point's label (empty: the wind on the body).
    pub point: String,
    /// The lines whose sections hang there (none: strings only).
    pub lines: Vec<String>,
    /// The sections hanging there (each line's).
    pub sections: Vec<String>,
    /// Unfactored, in the line's axes (x east, y north, z up), N.
    pub force: [f64; 3],
    /// Unfactored, in the structure's axes (transverse, longitudinal,
    /// vertical), N.
    pub local: [f64; 3],
    /// Factored, N.
    pub factored: [f64; 3],
}

/// One load case of one line on the tower.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseOnTower {
    /// The line whose case it is.
    pub line: String,
    /// The case.
    pub case: String,
    /// Its loads per point.
    pub points: Vec<PointLoad>,
}

/// The worst of a point over every line's cases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Worst {
    /// The point's label.
    pub point: String,
    /// Per component (transverse, longitudinal, vertical) the largest
    /// factored load by size, N.
    pub factored: [f64; 3],
    /// Per component, the line and case giving it.
    pub governing: [(String, String); 3],
}

/// A tower of the working set and its loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TowerLoads {
    /// The working set's structure.
    pub structure: String,
    /// Its twins in other lines.
    pub twins: Vec<Neighbour>,
    /// Every line's cases on it.
    pub cases: Vec<CaseOnTower>,
    /// The worst per point.
    pub worst: Vec<Worst>,
    /// What it assumed (NOT ON THE SAFE SIDE where it is not).
    pub notes: Vec<String>,
}

/// The loads on a working set's structures, shared towers with every
/// line's sections.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LoadsResult {
    /// Per structure.
    pub towers: Vec<TowerLoads>,
    /// What it pulled in.
    pub pulled: PulledIn,
    /// What it could not do: each makes it *not all checked*.
    pub not_all_checked: Vec<String>,
}

impl LoadsResult {
    /// The reach.
    pub fn reach(&self) -> Reach {
        Reach::Towers {
            tolerance: TWIN_TOLERANCE,
        }
    }
}

/// One line's loads at its twin in a case (none: the case failed).
pub type TwinCase = (String, String, Result<CaseLoads, String>);

/// The tower's loads in one case of line `own`: `twins` gives, per line,
/// (model, its twin's id, its loads in that case). See the module's notes.
/// `None`: the case's line's twin takes no such case.
pub fn one_tower(
    own: &str,
    twins: &[(String, String, &CaseLoads)],
    notes: &mut Vec<String>,
) -> Option<Vec<PointLoad>> {
    let at = |s: &str, l: &CaseLoads| -> Vec<AttachmentLoad> {
        l.loads
            .iter()
            .filter(|a| a.structure == s)
            .cloned()
            .collect()
    };
    let (_, own_id, own_loads) = twins.iter().find(|t| t.0 == own)?;
    let own_at = at(own_id, own_loads);
    if own_at.is_empty() {
        return None;
    }
    // Per point: the loads of each line holding sections there.
    let mut points: BTreeMap<String, Vec<(String, AttachmentLoad)>> = BTreeMap::new();
    for (m, s, l) in twins {
        for a in at(s, l) {
            if !a.sections.is_empty() {
                points
                    .entry(a.point.clone())
                    .or_default()
                    .push((m.clone(), a));
            }
        }
    }
    let mut out = Vec::new();
    let mut labels: Vec<String> = own_at.iter().map(|a| a.point.clone()).collect();
    labels.extend(points.keys().cloned());
    labels.sort();
    labels.dedup();
    for label in labels {
        let (lines, loads): (Vec<String>, Vec<AttachmentLoad>) = match points.get(&label) {
            Some(held) if label.is_empty() => {
                // The body's wind: the case's line's own.
                let _ = held;
                (
                    Vec::new(),
                    own_at
                        .iter()
                        .filter(|a| a.point.is_empty())
                        .cloned()
                        .collect(),
                )
            }
            Some(held) => held.iter().cloned().unzip(),
            None => (
                Vec::new(),
                own_at
                    .iter()
                    .filter(|a| a.point == label)
                    .cloned()
                    .collect(),
            ),
        };
        if loads.is_empty() {
            continue;
        }
        let mut lines = lines;
        lines.sort();
        lines.dedup();
        if lines.len() > 1 {
            notes.push(format!(
                "point {label}: sections of {} lines added, each line's strings there counted \
                 again (more load: conservative)",
                lines.len()
            ));
        }
        let mut sections: Vec<String> = loads.iter().flat_map(|a| a.sections.clone()).collect();
        sections.sort();
        sections.dedup();
        let mut local = [0.0; 3];
        let mut factored = [0.0; 3];
        let mut force = [0.0; 3];
        for a in &loads {
            for k in 0..3 {
                local[k] += a.local[k];
                factored[k] += a.factored[k];
                force[k] += a.force[k];
            }
        }
        out.push(PointLoad {
            point: label,
            lines,
            sections,
            force,
            local,
            factored,
        });
    }
    Some(out)
}

/// The worst per point over `cases` (per component the largest by size).
pub fn worst(cases: &[CaseOnTower]) -> Vec<Worst> {
    let mut by_point: BTreeMap<String, Worst> = BTreeMap::new();
    for c in cases {
        for p in &c.points {
            let w = by_point.entry(p.point.clone()).or_insert_with(|| Worst {
                point: p.point.clone(),
                factored: [0.0; 3],
                governing: Default::default(),
            });
            for k in 0..3 {
                if p.factored[k].abs() > w.factored[k].abs() || w.governing[k].1.is_empty() {
                    w.factored[k] = p.factored[k];
                    w.governing[k] = (c.line.clone(), c.case.clone());
                }
            }
        }
    }
    by_point.into_values().collect()
}

/// The loads on the structures of working set `ws` (its `structures`),
/// each shared tower with every line's sections on it, under each line's
/// cases, the worst kept (see the module's notes).
///
/// Expected cost: per tower, per line on it, its sections there solved
/// once per load case of every line on it.
pub fn shared_tower_loads(
    net: &mut dyn Neighbourhood,
    ws: &WorkingSet,
    budget: &ohl_core::budget::Budget,
) -> Result<LoadsResult, String> {
    let mut out = LoadsResult::default();
    for s in &ws.structures {
        let Some((pos, kind)) = net.structure(&ws.model, s) else {
            out.not_all_checked
                .push(format!("structure {s}: not on its line: not checked"));
            continue;
        };
        let area = Bounds::new([pos[0], pos[1]], [pos[0], pos[1]]).grown(TWIN_TOLERANCE);
        let mut group: Vec<(String, String)> = vec![(ws.model.clone(), s.clone())];
        for (m, t) in net.structures_in(&area) {
            if m == ws.model {
                continue;
            }
            let Some((p, k)) = net.structure(&m, &t) else {
                continue;
            };
            if (p[0] - pos[0]).hypot(p[1] - pos[1]) <= TWIN_TOLERANCE && k == kind {
                group.push((m, t));
            }
        }
        group.sort();
        group.dedup();
        let twins: Vec<Neighbour> = group
            .iter()
            .filter(|(m, _)| *m != ws.model)
            .map(|(m, t)| Neighbour::new(m, NeighbourKind::Structure, t))
            .collect();
        let tower = tower_loads(net, &ws.model, s, &group, budget, &mut out)?;
        for t in &twins {
            out.pulled.read.insert(t.clone());
            out.pulled.used.insert(t.clone());
        }
        out.towers.push(TowerLoads { twins, ..tower });
    }
    out.not_all_checked.sort();
    out.not_all_checked.dedup();
    Ok(out)
}

fn tower_loads(
    net: &mut dyn Neighbourhood,
    own_model: &str,
    own_id: &str,
    group: &[(String, String)],
    budget: &ohl_core::budget::Budget,
    out: &mut LoadsResult,
) -> Result<TowerLoads, String> {
    let mut notes = Vec::new();
    // Each line's part at its twin: its sections there and what solving
    // them reads.
    let mut parts = Vec::new();
    for (m, t) in group {
        let sections = net.sections_at(m, t);
        let part = match net.part(m, std::slice::from_ref(t), &sections) {
            Ok(p) => p,
            Err(e) => {
                out.not_all_checked.push(format!(
                    "structure {own_id}: line {m}'s sections on its twin not read ({e}): \
                     its loads missing"
                ));
                continue;
            }
        };
        if part.solver.structures.level() != CalculationLevel::Rigid {
            notes.push(format!(
                "NOT ON THE SAFE SIDE: line {m} is set to calculation level 3 or 4 — the \
                 shared tower's loads are solved rigid (level 2) here; that line's own \
                 loads add the structures' flexibility"
            ));
        }
        if m != own_model || t != own_id {
            for sec in &sections {
                out.pulled
                    .read
                    .insert(Neighbour::new(m, NeighbourKind::Section, sec));
                out.pulled
                    .used
                    .insert(Neighbour::new(m, NeighbourKind::Section, sec));
            }
        }
        let library = net.library(m)?;
        parts.push((m.clone(), t.clone(), part, library));
    }
    let mut cases_out = Vec::new();
    for (x, _) in group {
        let cases = match net.cases(x) {
            Ok(c) => c,
            Err(e) => {
                out.not_all_checked.push(format!(
                    "structure {own_id}: line {x}'s load cases not read ({e}): its cases not \
                     checked"
                ));
                continue;
            }
        };
        let known: Vec<String> = cases.loads.iter().map(|c| c.name.clone()).collect();
        // Every line's loads at its twin in line x's cases.
        let mut per_line: Vec<(String, String, Vec<Result<CaseLoads, String>>)> = Vec::new();
        for (m, t, part, library) in &parts {
            let solved = if m == x {
                ohl_loads::structure_loads_each(
                    part,
                    library,
                    &cases.weather,
                    &cases.loads,
                    &known,
                    budget,
                )
            } else {
                // The tower is line x's: its cases taken whole.
                let mut part = part.clone();
                part.structure_groups.clear();
                ohl_loads::structure_loads_each(
                    &part,
                    library,
                    &cases.weather,
                    &cases.loads,
                    &known,
                    budget,
                )
            };
            per_line.push((m.clone(), t.clone(), solved));
        }
        for (k, case) in cases.loads.iter().enumerate() {
            let mut twins: Vec<(String, String, &CaseLoads)> = Vec::new();
            let mut ok = true;
            for (m, t, solved) in &per_line {
                match solved.get(k) {
                    Some(Ok(l)) => {
                        for miss in &l.missing {
                            if miss.structure == *t {
                                out.not_all_checked.push(format!(
                                    "structure {own_id}, line {x}'s case {}: line {m}'s {} \
                                     {} did not solve — its forces missing",
                                    case.name,
                                    miss.kind.as_str(),
                                    miss.id
                                ));
                            }
                        }
                        twins.push((m.clone(), t.clone(), l));
                    }
                    Some(Err(e)) => {
                        ok = false;
                        out.not_all_checked.push(format!(
                            "structure {own_id}, line {x}'s case {}: line {m}: {e}",
                            case.name
                        ));
                    }
                    None => ok = false,
                }
            }
            if !ok {
                continue;
            }
            if let Some(points) = one_tower(x, &twins, &mut notes) {
                cases_out.push(CaseOnTower {
                    line: x.clone(),
                    case: case.name.clone(),
                    points,
                });
            }
        }
    }
    notes.sort();
    notes.dedup();
    Ok(TowerLoads {
        structure: own_id.to_owned(),
        twins: Vec::new(),
        worst: worst(&cases_out),
        cases: cases_out,
        notes,
    })
}
