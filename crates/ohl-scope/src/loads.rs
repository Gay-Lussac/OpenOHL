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
use ohl_line::Line;
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
    /// Per case of [`TowerLoads::cases`] (the same order), every line's
    /// loads on it as one load case of the working set's structure: what
    /// its usage check takes ([`tower_usage`], 2026-11-25).
    #[serde(skip)]
    pub combined: Vec<CombinedCase>,
    /// Its usage on the combined loads (none: not checked here — the
    /// neighbourhood left it to its caller, or it could not be checked,
    /// said in the result's NOT ALL CHECKED).
    #[serde(skip)]
    pub usage: Option<TowerUsage>,
}

/// One line's load case on a shared tower with every line's loads on it,
/// as a load case of the working set's structure (2026-11-25): what the
/// usage check takes, the same check as the line's own loads
/// ([`ohl_loads::capacity`]).
///
/// - **The loads**: at each point those [`one_tower`] adds — the loads of
///   every line whose sections hang there, else the case's line's own
///   (its strings only); the wind on the body the case's line's — each
///   moved onto the working set's structure (its id), nothing else
///   changed (the same type: the same points).
/// - **A broken wire in every span**: each line's sections are solved
///   with it (the case taken whole), each break its own load: every
///   combination of one break per line is a variant of its own (the
///   finite-element and pole checks take each, the worst kept; the
///   allowable loads the envelope, the lines' envelopes added: larger by
///   size than any combination's). More than [`MAX_COMBINATIONS`]: the
///   first of them, NOT ALL CHECKED (said).
/// - **Missing wires** of any line at its twin: the check incomplete
///   (never a pass).
#[derive(Debug, Clone, PartialEq)]
pub struct CombinedCase {
    /// The line whose case it is.
    pub line: String,
    /// The case.
    pub case: String,
    /// The tower's loads in it, every line's.
    pub loads: CaseLoads,
    /// The case's weather (the line's own).
    pub weather: Option<ohl_weather::WeatherCase>,
    /// What this case's usage could not take: each makes it NOT ALL
    /// CHECKED.
    pub not_all_checked: Vec<String>,
}

/// Every combination of one break per line, at most (2026-11-25): each a
/// finite-element solve of the tower per case.
pub const MAX_COMBINATIONS: usize = 256;

/// What the usage check of a line's structures reads: its structure types
/// (their capacity, models and poles) and the members' cross-section
/// library, with the folder they were written to (removed when dropped:
/// the models are read from it).
#[derive(Debug, Default)]
pub struct StructureChecks {
    /// The structure types.
    pub types: ohl_structures::StructureLibrary,
    /// The cross-section library.
    pub sections: ohl_sections::SectionLibrary,
    /// The folder holding their files (none: kept elsewhere).
    pub folder: Option<std::path::PathBuf>,
}

impl Drop for StructureChecks {
    fn drop(&mut self) {
        if let Some(f) = &self.folder {
            let _ = std::fs::remove_dir_all(f);
        }
    }
}

/// A shared tower's usage on the combined loads ([`tower_usage`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TowerUsage {
    /// Per case of [`TowerLoads::combined`] (the same order): the line, the
    /// case and the structure's usage in it.
    pub cases: Vec<(String, String, ohl_loads::capacity::StructureUsage)>,
    /// What it could not check: each makes it NOT ALL CHECKED.
    pub not_all_checked: Vec<String>,
}

impl TowerUsage {
    /// The governing case: the one using most of the structure's capacity
    /// (none: nothing checked).
    pub fn governing(&self) -> Option<&(String, String, ohl_loads::capacity::StructureUsage)> {
        self.cases
            .iter()
            .filter(|c| c.2.governing().is_some())
            .max_by(|a, b| {
                let u = |c: &(String, String, ohl_loads::capacity::StructureUsage)| {
                    c.2.governing().map_or(f64::NEG_INFINITY, |g| g.usage)
                };
                ohl_structures_fe::check::usage_order(u(a), u(b))
            })
    }
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

/// The working set's line's part at the tower and its wire library.
type OwnPart = (Line, std::sync::Arc<ohl_cables::Library>);

/// One line's loads in a case, or why the case failed.
type CaseResult = Result<CaseLoads, String>;

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

/// The loads of `lines` ((line, its loads at its twin)) at the tower as
/// [`one_tower`] adds them, moved onto structure `own_id`: at a point
/// holding sections, the loads with sections of every line there; else
/// (and the body's wind) the case's line `x`'s own.
fn select(own_id: &str, x: &str, lines: &[(String, Vec<AttachmentLoad>)]) -> Vec<AttachmentLoad> {
    let held: std::collections::BTreeSet<&str> = lines
        .iter()
        .flat_map(|(_, l)| l.iter())
        .filter(|a| !a.point.is_empty() && !a.sections.is_empty())
        .map(|a| a.point.as_str())
        .collect();
    let mut out = Vec::new();
    for (m, loads) in lines {
        for a in loads {
            let take = if a.point.is_empty() || !held.contains(a.point.as_str()) {
                m == x
            } else {
                !a.sections.is_empty()
            };
            if take {
                let mut a = a.clone();
                a.structure = own_id.to_owned();
                out.push(a);
            }
        }
    }
    out
}

/// Line `x`'s case `case` on the tower with every line's loads `twins`
/// ((model, its twin, its loads in that case)), as one load case of
/// structure `own_id` ([`CombinedCase`]); none: `x` not among them.
pub fn combine(
    own_id: &str,
    x: &str,
    twins: &[(String, String, &CaseLoads)],
    case: &ohl_loads::LoadCase,
    weather: Option<ohl_weather::WeatherCase>,
) -> Option<CombinedCase> {
    let at = |t: &str, loads: &[AttachmentLoad]| -> Vec<AttachmentLoad> {
        loads.iter().filter(|a| a.structure == t).cloned().collect()
    };
    let own = twins.iter().find(|t| t.0 == x)?;
    // Per line: its envelope at its twin, and each break's.
    let mut envelope: Vec<(String, Vec<AttachmentLoad>)> = Vec::new();
    let mut breaks: Vec<Vec<(Option<usize>, Vec<AttachmentLoad>)>> = Vec::new();
    let mut missing = Vec::new();
    for (m, t, l) in twins {
        envelope.push((m.clone(), at(t, &l.loads)));
        breaks.push(if l.broken_each.is_empty() {
            vec![(None, at(t, &l.loads))]
        } else {
            l.broken_each
                .iter()
                .map(|b| (Some(b.span), at(t, &b.loads)))
                .collect()
        });
        for w in l.missing.iter().filter(|w| w.structure == *t) {
            let mut w = w.clone();
            w.structure = own_id.to_owned();
            missing.push(w);
        }
    }
    let loads = select(own_id, x, &envelope);
    let mut not_all_checked = Vec::new();
    let mut broken_each = Vec::new();
    if twins.iter().any(|(_, _, l)| !l.broken_each.is_empty()) {
        let count = breaks
            .iter()
            .map(Vec::len)
            .fold(1usize, usize::saturating_mul);
        if count > MAX_COMBINATIONS {
            not_all_checked.push(format!(
                "structure {own_id}, line {x}'s case {}: the broken wires of the {} lines on it \
                 make {count} combinations of one break per line; the finite-element and pole \
                 checks take the first {MAX_COMBINATIONS} — a member force combines the load's \
                 components, so a combination left out can load a member more; the allowable \
                 loads take the lines' envelopes added (every combination within them), so a \
                 type checked by allowable loads alone is checked whole",
                case.name,
                twins.len()
            ));
        }
        let x_at = twins.iter().position(|t| t.0 == x).unwrap_or(0);
        // Every combination, the last line's break changing fastest.
        let mut index = vec![0usize; breaks.len()];
        for _ in 0..count.min(MAX_COMBINATIONS) {
            let v: Vec<(String, Vec<AttachmentLoad>)> = twins
                .iter()
                .zip(breaks.iter().zip(&index))
                .map(|((m, _, _), (e, &k))| (m.clone(), e[k].1.clone()))
                .collect();
            // Named by the case's line's break (else the first line's).
            let span = breaks[x_at][index[x_at]]
                .0
                .or_else(|| breaks.iter().zip(&index).find_map(|(e, &k)| e[k].0))
                .unwrap_or(0);
            broken_each.push(ohl_loads::BrokenLoads {
                span,
                loads: select(own_id, x, &v),
            });
            for k in (0..index.len()).rev() {
                index[k] += 1;
                if index[k] < breaks[k].len() {
                    break;
                }
                index[k] = 0;
            }
        }
    }
    Some(CombinedCase {
        line: x.to_owned(),
        case: case.name.clone(),
        loads: CaseLoads {
            case: case.clone(),
            loads,
            failed: Vec::new(),
            missing,
            structure_wind: own.2.structure_wind.clone(),
            broken_each,
            notes: Vec::new(),
            unknown_sets: Vec::new(),
            level: None,
        },
        weather,
        not_all_checked,
    })
}

/// The usage of structure `id` of `part` (its line's part holding it,
/// structure types resolved) on every case of `combined`
/// ([`TowerLoads::combined`]), by **the same check as its line's own
/// loads** ([`ohl_loads::capacity::usage_with_progress`]) with `checks`
/// and its line's wire library `library`. Its line's structure groups are
/// set aside: the cases are those the loads took (each line's cases its
/// twin takes). None: stopped.
///
/// Expected cost: per case the structure's checks once (a finite-element
/// or pole solve per break combination: milliseconds each).
pub fn tower_usage(
    part: &Line,
    id: &str,
    checks: &StructureChecks,
    library: &ohl_cables::Library,
    combined: &[CombinedCase],
    progress: &ohl_loads::capacity::Progress<'_>,
) -> Option<TowerUsage> {
    let mut line = part.clone();
    line.structure_groups.clear();
    // Each case its own weather (two lines may name theirs alike).
    let mut weather = Vec::new();
    let results: Vec<CaseLoads> = combined
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let mut l = c.loads.clone();
            if let Some(w) = &c.weather {
                let mut w = w.clone();
                w.name = format!("{k}\u{1f}{}", w.name);
                l.case.weather = w.name.clone();
                weather.push(w);
            }
            l
        })
        .collect();
    let usage = ohl_loads::capacity::usage_with_progress(
        &line,
        &checks.types,
        &checks.sections,
        library,
        &weather,
        &results,
        progress,
    )?;
    let mut out = TowerUsage::default();
    for (c, u) in combined.iter().zip(usage) {
        out.not_all_checked
            .extend(c.not_all_checked.iter().cloned());
        match u.structures.into_iter().find(|s| s.structure == id) {
            Some(s) => out.cases.push((c.line.clone(), c.case.clone(), s)),
            None => out.not_all_checked.push(format!(
                "structure {id}, line {}'s case {}: no usage computed on the combined loads",
                c.line, c.case
            )),
        }
    }
    out.not_all_checked.dedup();
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
    let checks = if ws.structures.is_empty() {
        None
    } else {
        net.structure_checks(&ws.model)
    };
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
        let (mut tower, own) = tower_loads(net, &ws.model, s, &group, budget, &mut out)?;
        for t in &twins {
            out.pulled.read.insert(t.clone());
            out.pulled.used.insert(t.clone());
        }
        // Its usage on the combined loads, where the neighbourhood checks
        // it (2026-11-25).
        match (checks.as_ref(), own) {
            (None, _) => {}
            (Some(Err(e)), _) => out.not_all_checked.push(format!(
                "structure {s}: its usage on every line's loads not checked: {e} — its loads \
                 there can use more of its capacity than this line's alone; check it where the \
                 structure types are read (OHL-Design's Structure loads window)"
            )),
            (Some(Ok(_)), None) => out.not_all_checked.push(format!(
                "structure {s}: its usage on every line's loads not checked (its own line's \
                 part not read)"
            )),
            (Some(Ok(c)), Some((part, library))) => {
                match tower_usage(
                    &part,
                    s,
                    c,
                    &library,
                    &tower.combined,
                    &ohl_loads::capacity::Progress {
                        threads: 1,
                        ..Default::default()
                    },
                ) {
                    Some(u) => {
                        out.not_all_checked
                            .extend(u.not_all_checked.iter().cloned());
                        tower.usage = Some(u);
                    }
                    None => out
                        .not_all_checked
                        .push(format!("structure {s}: its usage check stopped")),
                }
            }
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
) -> Result<(TowerLoads, Option<OwnPart>), String> {
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
                 shared tower's loads are solved rigid (level 2) here, so they leave out \
                 the extra pull the structures' flexibility adds and can be too small; \
                 check the tower with that line's own loads, at its own level"
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
    // The twins' axes: the loads are added in each one's own (the same
    // type: the same points), right only where they turn alike.
    let own_turn = parts
        .iter()
        .find(|(m, t, _, _)| m == own_model && t == own_id)
        .and_then(|(_, t, p, _)| p.structure(t).map(|s| s.orientation_deg));
    for (m, t, part, _) in &parts {
        let turn = part.structure(t).map(|s| s.orientation_deg);
        if let (Some(a), Some(b)) = (own_turn, turn) {
            let off = ((b - a + 180.0).rem_euclid(360.0) - 180.0).abs();
            if off > TURN_TOLERANCE_DEG {
                out.not_all_checked.push(format!(
                    "structure {own_id}: line {m}'s twin {t} is turned {off:.1}° from it — each \
                     line's loads are in its own twin's axes and at its points by label, added \
                     as if the axes were one: with the twins turned apart a point's label can \
                     name another place and the components other directions, so the loads and \
                     their usage can be wrong either way; give the twins the same orientation \
                     (one tower) in every line"
                ));
            }
        }
    }
    let mut cases_out = Vec::new();
    let mut combined = Vec::new();
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
        let mut per_line: Vec<(String, String, Vec<CaseResult>)> = Vec::new();
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
                let weather = cases
                    .weather
                    .iter()
                    .find(|w| w.name == case.weather)
                    .cloned();
                if let Some(c) = combine(own_id, x, &twins, case, weather) {
                    combined.push(c);
                } else {
                    continue;
                }
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
    let own = parts
        .into_iter()
        .find(|(m, t, _, _)| m == own_model && t == own_id)
        .map(|(_, _, p, l)| (p, l));
    Ok((
        TowerLoads {
            structure: own_id.to_owned(),
            twins: Vec::new(),
            worst: worst(&cases_out),
            cases: cases_out,
            notes,
            combined,
            usage: None,
        },
        own,
    ))
}

/// Twins turned apart by more than this, degrees, are said NOT ALL
/// CHECKED: their loads are added by point label in each one's axes.
pub const TURN_TOLERANCE_DEG: f64 = 0.5;
