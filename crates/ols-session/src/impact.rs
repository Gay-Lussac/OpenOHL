// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Engineering impact of a change between two states ([`crate::versions`]):
//! the sections it touches are solved in one weather case on both sides,
//! and their largest sag, largest tension and smallest ground clearance
//! compared.
//!
//! Touched sections: those changed, and those through changed structures
//! (on either side); when a library file changed, every section. Each side
//! is solved with its own libraries (wires, structure types, insulators;
//! wires missing in a state are taken from a fallback library).
//!
//! Expected cost: one section solve per touched section and side
//! (typically < 1 ms each, at most the finite-element budget); bounded by
//! a number of sections and a time limit ([`ImpactOptions`]), after which
//! the rest is reported as skipped.

use std::time::Duration;

use ols_core::budget::Budget;
use ols_line::Line;
use ols_weather::WeatherCase;

use crate::versions::{Diff, State, resolved_line};
use crate::{Kind, affected_sections};

/// Ground height at a plan point (`None`: no ground there).
pub type Ground<'a> = &'a dyn Fn(f64, f64) -> Option<f64>;

/// How to work out an impact.
pub struct ImpactOptions<'a> {
    /// The weather case solved.
    pub case: &'a WeatherCase,
    /// Every weather case of the project (creep-stretch states refer to
    /// them).
    pub cases: &'a [WeatherCase],
    /// Wires when a state has no wire library of its own.
    pub fallback_cables: Option<&'a ols_cables::Library>,
    /// The ground, for clearances.
    pub ground: Option<Ground<'a>>,
    /// Sections solved at most.
    pub max_sections: usize,
    /// Time limit.
    pub time_limit: Duration,
}

/// What a solved section shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Figures {
    /// Largest sag of its spans, m.
    pub max_sag: f64,
    /// Largest tension of its spans, N (per wire).
    pub max_tension: f64,
    /// Smallest vertical distance from the wire down to the ground, m
    /// (`None`: no ground under it).
    pub min_clearance: Option<f64>,
}

/// One section on both sides.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionImpact {
    /// Section id.
    pub section: String,
    /// Before (`None`: not in the first state, or not solved).
    pub before: Option<Figures>,
    /// After.
    pub after: Option<Figures>,
    /// Why a side was not solved.
    pub errors: Vec<String>,
}

/// The impact of a change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Impact {
    /// The sections solved.
    pub sections: Vec<SectionImpact>,
    /// Touched sections left out (limits reached).
    pub skipped: usize,
    /// Why it stopped early, if it did.
    pub stopped: Option<String>,
}

fn library(state: &State, fallback: Option<&ols_cables::Library>) -> ols_cables::Library {
    let files = state.library_files("cables");
    if files.is_empty() {
        return fallback.cloned().unwrap_or_default();
    }
    let mut lib = ols_cables::Library::default();
    for (name, text) in files {
        // A file that does not load leaves its wires out: the sections on
        // them report it.
        let _ = lib.add_toml(std::path::Path::new(name), text);
    }
    lib
}

fn figures(shape: &ols_line::solve::SectionShape, ground: Option<Ground<'_>>) -> Figures {
    let mut f = Figures {
        max_sag: 0.0,
        max_tension: 0.0,
        min_clearance: None,
    };
    for span in &shape.spans {
        f.max_sag = f.max_sag.max(span.sag);
        f.max_tension = f.max_tension.max(span.max_tension);
        if let Some(g) = ground {
            for p in &span.points {
                if let Some(z) = g(p[0], p[1]) {
                    let c = p[2] - z;
                    f.min_clearance = Some(f.min_clearance.map_or(c, |m: f64| m.min(c)));
                }
            }
        }
    }
    f
}

fn solve(
    line: &Result<Line, String>,
    lib: &ols_cables::Library,
    id: &str,
    opts: &ImpactOptions<'_>,
) -> Result<Option<Figures>, String> {
    let line = line.as_ref().map_err(Clone::clone)?;
    let Some(section) = line.sections.iter().find(|s| s.id == id) else {
        return Ok(None);
    };
    ols_line::solve::solve_one(line, section, lib, opts.case)
        .map(|shape| Some(figures(&shape, opts.ground)))
        .map_err(|e| e.to_string())
}

/// The engineering impact of going from `before` to `after` (`diff`: their
/// differences).
pub fn engineering_impact(
    before: &State,
    after: &State,
    diff: &Diff,
    opts: &ImpactOptions<'_>,
) -> Impact {
    let prepare = |s: &State| {
        resolved_line(s).map(|mut l| {
            l.set_weather_cases(opts.cases);
            l
        })
    };
    let (la, lb) = (prepare(before), prepare(after));
    let changed: Vec<(Kind, String)> = diff.keys();
    let mut touched: Vec<String> = match (&la, &lb) {
        _ if !diff.libraries.is_empty() => {
            let mut all: Vec<String> = [&la, &lb]
                .into_iter()
                .flatten()
                .flat_map(|l| l.sections.iter().map(|s| s.id.clone()))
                .collect();
            all.sort();
            all.dedup();
            all
        }
        (Ok(a), Ok(b)) => affected_sections([a, b], &changed),
        (Ok(one), Err(_)) | (Err(_), Ok(one)) => affected_sections([one, one], &changed),
        (Err(_), Err(_)) => changed
            .iter()
            .filter(|(k, _)| *k == Kind::Section)
            .map(|(_, id)| id.clone())
            .collect(),
    };
    touched.dedup();
    let (liba, libb) = (
        library(before, opts.fallback_cables),
        library(after, opts.fallback_cables),
    );
    let budget = Budget::new()
        .max_iterations(opts.max_sections as u64)
        .time_limit(opts.time_limit);
    let mut run = budget.start();
    let mut out = Impact::default();
    for (k, id) in touched.iter().enumerate() {
        if let Err(stop) = run.step() {
            out.skipped = touched.len() - k;
            out.stopped = Some(stop.to_string());
            break;
        }
        let mut s = SectionImpact {
            section: id.clone(),
            before: None,
            after: None,
            errors: vec![],
        };
        match solve(&la, &liba, id, opts) {
            Ok(f) => s.before = f,
            Err(e) => s.errors.push(format!("before: {e}")),
        }
        match solve(&lb, &libb, id, opts) {
            Ok(f) => s.after = f,
            Err(e) => s.errors.push(format!("after: {e}")),
        }
        out.sections.push(s);
    }
    out
}
