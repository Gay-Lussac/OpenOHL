// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Memos of solves (performance review of 2026-10-26, owner-approved):
//! work done once and taken again while nothing it depends on changed —
//! the same results bit for bit, as the same deterministic code on the
//! same inputs gives them.
//!
//! - **The weather-independent parts of a section solve** (review item 5):
//!   its holders with the strain strings' estimated weights and its
//!   stringing reference, the stringing state of a cable with stress-strain
//!   curves, and the final state after creep or after load. Every case and
//!   every wind state of a run solves them again otherwise.
//!
//! Keys: 128-bit fingerprints of the inputs' `Debug` text — every field,
//! the numbers exactly (shortest round-trip form) — so a change of any
//! input (a line edit, a library edit, another weather case, another
//! iteration limit) is another key: an old entry is never taken for it.
//! Failed solves are not kept (a stop on a time limit or a cancel depends
//! on the moment). Memory is bounded: the least recently used entries go
//! first ([`PARTS_LIMIT`]).

use std::collections::HashMap;
use std::fmt::{Debug, Write as _};
use std::hash::{DefaultHasher, Hasher};
use std::sync::{Arc, Mutex};

use ohl_cables::Cable;
use ohl_core::budget::Budget;

use super::{FinalState, Holder, Reference, estimate_strain_weights, holders_of, reference};
use crate::condition::Condition;
use crate::{Line, LineError, Section};

/// A 128-bit fingerprint of values' `Debug` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Key(u64, u64);

/// Feeds `Debug` text into two independent hashes.
struct Fingerprint(DefaultHasher, DefaultHasher);

impl std::fmt::Write for Fingerprint {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        self.1.write(s.as_bytes());
        Ok(())
    }
}

/// The fingerprint of `parts` (each one's `Debug` text, separated).
pub(crate) fn key(parts: &[&dyn Debug]) -> Key {
    let mut f = Fingerprint(DefaultHasher::new(), DefaultHasher::new());
    // The second hash differs from the first from the start.
    f.1.write_u8(0xA5);
    for p in parts {
        // Writing into a hash never fails.
        let _ = write!(f, "{p:?}\u{1f}");
    }
    Key(f.0.finish(), f.1.finish())
}

/// Entries at most per memo of a solve's parts.
pub const PARTS_LIMIT: usize = 4096;

/// A least-recently-used store.
struct Lru<V> {
    map: HashMap<Key, (u64, usize, V)>,
    clock: u64,
    bytes: usize,
}

impl<V: Clone> Lru<V> {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            clock: 0,
            bytes: 0,
        }
    }

    fn get(&mut self, k: &Key) -> Option<V> {
        self.clock += 1;
        let clock = self.clock;
        self.map.get_mut(k).map(|e| {
            e.0 = clock;
            e.2.clone()
        })
    }

    /// Keeps `v` (about `bytes` of memory); then drops the least recently
    /// used entries while there are more than `entries` or `max_bytes`.
    fn put(&mut self, k: Key, v: V, bytes: usize, entries: usize, max_bytes: usize) {
        self.clock += 1;
        if let Some((_, b, _)) = self.map.insert(k, (self.clock, bytes, v)) {
            self.bytes -= b;
        }
        self.bytes += bytes;
        while self.map.len() > entries || (self.bytes > max_bytes && self.map.len() > 1) {
            let Some(oldest) = self.map.iter().min_by_key(|(_, e)| e.0).map(|(k, _)| *k) else {
                break;
            };
            if let Some((_, b, _)) = self.map.remove(&oldest) {
                self.bytes -= b;
            }
        }
    }
}

/// A section's holders (the strain strings' weights estimated) and its
/// stringing reference: what every case of it starts from.
pub(super) struct Prepared {
    pub(super) layout: Vec<Holder>,
    pub(super) weights: Vec<Option<f64>>,
    pub(super) reference: Reference,
    /// Its memo key (the stringing state of curves builds on it).
    pub(super) key: Key,
}

type Parts<V> = Mutex<Option<Lru<V>>>;

static PREPARED: Parts<Arc<Prepared>> = Mutex::new(None);
static FINAL: Parts<FinalState> = Mutex::new(None);
static STRINGING: Parts<Arc<Vec<f64>>> = Mutex::new(None);

fn recall<V: Clone>(memo: &Parts<V>, k: &Key) -> Option<V> {
    memo.lock()
        .ok()
        .and_then(|mut m| m.get_or_insert_with(Lru::new).get(k))
}

fn keep<V: Clone>(memo: &Parts<V>, k: Key, v: V) {
    if let Ok(mut m) = memo.lock() {
        m.get_or_insert_with(Lru::new)
            .put(k, v, 1, PARTS_LIMIT, usize::MAX);
    }
}

/// [`estimate_strain_weights`] and [`reference`] of `section` (its holders
/// from `line`), from the memo when solved before.
pub(super) fn prepared(
    line: &Line,
    section: &Section,
    cable: &Cable,
    budget: &Budget,
) -> Result<Arc<Prepared>, LineError> {
    let holders = holders_of(line, section)?;
    let k = key(&[section, &holders, cable, &budget.iteration_limit()]);
    if let Some(p) = recall(&PREPARED, &k) {
        return Ok(p);
    }
    let (layout, weights) = estimate_strain_weights(section, holders, cable, budget)?;
    let reference = reference(section, &layout, cable, budget)?;
    let p = Arc::new(Prepared {
        layout,
        weights,
        reference,
        key: k,
    });
    keep(&PREPARED, k, p.clone());
    Ok(p)
}

/// The spans' tensions in the stringing state of a cable with curves
/// (`solve` works them out), for the section of `prepared` solved with the
/// line's calculation options `options` (they are set on its reference).
pub(super) fn stringing(
    prepared: &Prepared,
    options: &dyn Debug,
    solve: impl FnOnce() -> Result<Vec<f64>, LineError>,
) -> Result<Arc<Vec<f64>>, LineError> {
    let k = key(&[&prepared.key, options]);
    if let Some(t) = recall(&STRINGING, &k) {
        return Ok(t);
    }
    let t = Arc::new(solve()?);
    keep(&STRINGING, k, t.clone());
    Ok(t)
}

/// [`super::final_state`], from the memo when worked out before.
pub(super) fn final_state(
    line: &Line,
    section: &Section,
    cable: &Cable,
    condition: Condition,
    budget: &Budget,
) -> Result<FinalState, LineError> {
    let holders = holders_of(line, section)?;
    let k = key(&[
        section,
        &holders,
        cable,
        &line.bimetallic,
        &line.condition_cases,
        &condition,
        &budget.iteration_limit(),
    ]);
    if let Some(f) = recall(&FINAL, &k) {
        return Ok(f);
    }
    let f = super::final_state(line, section, cable, condition, budget)?;
    keep(&FINAL, k, f.clone());
    Ok(f)
}

/// Drops every memo (e.g. to measure a solve from scratch).
pub fn forget_solves() {
    for m in [&PREPARED as &dyn Forget, &FINAL, &STRINGING] {
        m.forget();
    }
}

trait Forget {
    fn forget(&self);
}

impl<V> Forget for Parts<V> {
    fn forget(&self) {
        if let Ok(mut m) = self.lock() {
            *m = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn library() -> ohl_cables::Library {
        ohl_cables::Library::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
        )
        .unwrap()
    }

    /// A key sees every field, the numbers exactly.
    #[test]
    fn keys_see_every_change() {
        let line = crate::tests::example();
        let mut other = line.clone();
        other.structures[1].position[2] += 1e-9;
        assert_ne!(key(&[&line]), key(&[&other]));
        assert_eq!(key(&[&line]), key(&[&line.clone()]));
        let mut cases = line.clone();
        cases.condition_cases.creep = Some(ohl_weather::WeatherCase::still(
            "c",
            ohl_core::units::Temperature::from_celsius(5.0),
        ));
        assert_ne!(key(&[&line]), key(&[&cases]));
    }

    /// A section solved from the memo's parts (in every condition,
    /// across cases) is the section solved from scratch, bit for bit.
    #[test]
    fn solves_from_the_parts_are_the_solves() {
        use crate::condition::Calculation;
        use ohl_core::units::{Stress, Temperature};
        use ohl_weather::WeatherCase;
        let lib = library();
        let budget = ohl_wirefe::default_fe_budget();
        let mut windy = WeatherCase::still("w", Temperature::from_celsius(10.0));
        windy.wind_pressure = Stress::from_si(500.0);
        let cases = [
            WeatherCase::still("c", Temperature::from_celsius(-5.0)),
            WeatherCase::still("h", Temperature::from_celsius(60.0)),
            windy,
        ];
        for calc in [
            Calculation::INITIAL_FE,
            Calculation::CREEP_FE,
            Calculation::MAX_SAG_RS,
        ] {
            let mut line = crate::tests::example();
            line.condition_cases.creep = Some(cases[0].clone());
            line.condition_cases.loads = vec![cases[2].clone()];
            for s in &mut line.sections {
                s.calculation = calc;
            }
            for case in &cases {
                forget_solves();
                let fresh = super::super::solve_line_with(&line, &lib, case, &budget);
                assert!(fresh.iter().all(Result::is_ok), "{fresh:?}");
                let again = super::super::solve_line_with(&line, &lib, case, &budget);
                assert_eq!(format!("{again:?}"), format!("{fresh:?}"));
            }
        }
    }
}
