// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Scoped computations (ADR-0013, the country server's phase 10; built
//! 2026-11-20, in active development, not approved yet): a check run on a
//! working set pulls in, by **its own reach** ([`reach`]), what can change
//! its result — from its own line and from other lines — and **says what
//! it pulled in**.
//!
//! - [`clearance`]: clearances by rule, other lines' wires as targets
//!   (crossings among them), the line's own wires beyond the working set
//!   too; the reach is the largest of the wire rules' and grows with what
//!   is found (the voltages met, the highest wire) until it holds.
//! - [`fields`]: the magnetic field with the lines within the field
//!   distance (500 m, owner 2026-11-16) — the wires beyond it left out:
//!   **NOT ON THE SAFE SIDE**, said in every result.
//! - [`loads`]: the loads on shared towers (twins of other lines) with
//!   every line's sections on them, under each line's load cases, the
//!   worst kept.
//!
//! Nothing here reads a store or decides rights: a [`Neighbourhood`]
//! gives what lies near (the network's server: its models in memory and
//! their tile index, each part taken as `Session::drawing_part` takes it —
//! never a whole line or the whole network copied) and the server
//! filters what it reports by the rights.
//!
//! **Safe side**: a scoped result equals the same check on the whole
//! network (every line solved whole, every wire a target), or is more
//! conservative — tested on the synthetic country (its tests, `tests.rs`). What a
//! check could not pull in makes it *not all checked*, never a pass.

pub mod bound;
pub mod clearance;
pub mod electric;
pub mod fields;
pub mod loads;
pub mod reach;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::sync::Arc;

use ohl_core::grid::Bounds;
use ohl_line::electrical::Potential;
use ohl_line::{Line, Point};
use ohl_loads::LoadCase;
use ohl_weather::WeatherCase;
use serde::{Deserialize, Serialize};

pub use bound::SpanChord;
pub use reach::{CheckKind, Reach, reach_of};

/// What kind of object a neighbour is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeighbourKind {
    /// A tension section.
    Section,
    /// A jumper.
    Jumper,
    /// A structure (a twin tower).
    Structure,
}

impl NeighbourKind {
    /// In words.
    pub fn word(self) -> &'static str {
        match self {
            NeighbourKind::Section => "section",
            NeighbourKind::Jumper => "jumper",
            NeighbourKind::Structure => "structure",
        }
    }
}

/// An object pulled in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Neighbour {
    /// Its model (line).
    pub model: String,
    /// What it is.
    pub kind: NeighbourKind,
    /// Its id.
    pub id: String,
}

impl Neighbour {
    /// A neighbour.
    pub fn new(model: &str, kind: NeighbourKind, id: &str) -> Self {
        Self {
            model: model.to_owned(),
            kind,
            id: id.to_owned(),
        }
    }
}

/// What a scoped check pulled in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PulledIn {
    /// Every object read beside the working set (solved, looked at).
    pub read: BTreeSet<Neighbour>,
    /// Of those, the ones within the check's reach: its targets, the
    /// wires on its towers, the lines in its field.
    pub used: BTreeSet<Neighbour>,
}

impl PulledIn {
    /// The models (lines) used.
    pub fn lines(&self) -> BTreeSet<&str> {
        self.used.iter().map(|n| n.model.as_str()).collect()
    }
}

/// A working set as a check takes it: its model and what it checks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkingSet {
    /// The model (line).
    pub model: String,
    /// The sections checked.
    pub sections: Vec<String>,
    /// The jumpers checked.
    pub jumpers: Vec<String>,
    /// The structures checked (structure loads).
    pub structures: Vec<String>,
}

/// A model's cases: its weather cases and its load cases (each line's
/// own library, else the shipped defaults).
#[derive(Debug, Clone, Default)]
pub struct Cases {
    /// Weather cases.
    pub weather: Vec<WeatherCase>,
    /// Load cases.
    pub loads: Vec<LoadCase>,
    /// Where they came from, in words.
    pub source: String,
}

/// The shipped default weather and load cases.
pub fn default_cases() -> Result<Cases, String> {
    let weather = ohl_weather::WeatherCases::parse(include_str!(
        "../../../libraries/weather/default-weather-cases.ohlwc"
    ))?
    .cases;
    Ok(Cases {
        weather,
        loads: ohl_loads::default_load_cases().cases,
        source: "the shipped default cases".into(),
    })
}

/// What lies near a working set, on the network: the server's models and
/// their tile index, or the lines of a test. Every part asked is what
/// solving the sections alone reads (`Session::drawing_part`), structure
/// types resolved — never a whole line.
pub trait Neighbourhood {
    /// The sections of every model with a span or a structure in `area`:
    /// (model, section).
    fn sections_in(&mut self, area: &Bounds) -> Vec<(String, String)>;
    /// The structures of every model standing in `area`: (model, id).
    fn structures_in(&mut self, area: &Bounds) -> Vec<(String, String)>;
    /// A structure's position and type (none: not there).
    fn structure(&mut self, model: &str, id: &str) -> Option<(Point, Option<String>)>;
    /// The sections of `model` with a point on structure `id`.
    fn sections_at(&mut self, model: &str, id: &str) -> Vec<String>;
    /// The span chords of `sections` of `model` (a section not there: none).
    fn chords(&mut self, model: &str, sections: &[String]) -> Vec<(String, Vec<SpanChord>)>;
    /// What solving `sections` of `model` alone reads, with `structures`
    /// (structure types resolved).
    fn part(
        &mut self,
        model: &str,
        structures: &[String],
        sections: &[String],
    ) -> Result<Line, String>;
    /// Every potential a section of `model` can have (each phase of each
    /// of its systems, and earthed): a superset of its sections' own.
    fn potentials(&mut self, model: &str) -> Vec<Option<Potential>>;
    /// The model's wire library.
    fn library(&mut self, model: &str) -> Result<Arc<ohl_cables::Library>, String>;
    /// The model's cases.
    fn cases(&mut self, model: &str) -> Result<Arc<Cases>, String>;
    /// An upper bound of every span's margin on the network
    /// ([`SpanChord::margin`]), m: the candidate search looks this much
    /// further (it may only grow).
    fn max_margin(&mut self) -> f64;
}

/// Every potential a section of `line` can have: each phase of each of
/// its systems, and earthed (`None`).
pub fn system_potentials(line: &Line) -> Vec<Option<Potential>> {
    let mut out: Vec<Option<Potential>> = vec![None];
    for s in &line.systems {
        for k in 0..s.phases.len() {
            let p = Some(Potential {
                nominal_kv: s.voltage_kv,
                kind: s.kind,
                frequency_hz: s.frequency_hz,
                to_earth: s.to_earth(k),
            });
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// A section's wires solved, or why not.
pub(crate) type Solved = (Vec<ohl_clearance::Wire>, Vec<String>);

/// Solves `part` in `case` and gives the wires of `sections` and of every
/// jumper in it (a jumper as `jumper:ID`), with what did not solve.
pub(crate) fn solve_wires(
    part: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    sections: &BTreeSet<String>,
    jumpers: Option<&BTreeSet<String>>,
) -> Solved {
    let mut failed = Vec::new();
    let mut shapes = Vec::new();
    for (k, r) in ohl_line::solve::solve_line(part, library, case)
        .into_iter()
        .enumerate()
    {
        match r {
            Ok(s) => shapes.push(s),
            Err(e) => {
                let id = &part.sections[k].id;
                if sections.contains(id) {
                    failed.push(format!("section {id}: {e}"));
                }
            }
        }
    }
    let mut js = Vec::new();
    for (k, r) in ohl_line::jumper::solve_jumpers(part, library, case, &shapes)
        .into_iter()
        .enumerate()
    {
        let id = &part.jumpers[k].id;
        let wanted = jumpers.is_none_or(|j| j.contains(id));
        match r {
            Ok(j) => js.push(j),
            Err(e) if wanted => failed.push(format!("jumper {id}: {e}")),
            Err(_) => {}
        }
    }
    let wires = ohl_clearance::wires_of(part, &shapes, &js)
        .into_iter()
        .filter(|w| match w.section.strip_prefix("jumper:") {
            Some(j) => jumpers.is_none_or(|set| set.contains(j)),
            None => sections.contains(&w.section),
        })
        .collect();
    (wires, failed)
}
