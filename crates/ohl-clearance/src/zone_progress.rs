// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! How far a zone check has got (owner, 2026-11-04: "make it count"):
//! the step under way of those the run makes, what it has done of what
//! it has to do, and since when — so that a window shows a bar and a
//! time left per step, never "0 s left" while work remains.
//!
//! The steps of a run ([`crate::zone::run_zone_check`]), per calculation
//! of its rules: solving the wind positions (positions solved), making
//! the swing arcs (cases), indexing the wires (rules), reading and
//! checking the points (points of the blocks read, which the point
//! reader counts — [`ZoneProgress::reading`]), checking the ground (wires
//! and swing arcs).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A step of a zone check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Solving the wind positions: positions solved.
    Solving,
    /// Making the swing arcs: arc cases made.
    Arcs,
    /// Indexing the wires (and arcs) of each rule: rules indexed.
    Indexing,
    /// Reading the points and checking each against every rule, wind
    /// position and arc: points of the blocks read (the point reader's).
    Points,
    /// Checking the ground: wires (every position) and the lines of the
    /// swing arcs' boundaries.
    Ground,
}

impl Step {
    const ALL: [Step; 5] = [
        Step::Solving,
        Step::Arcs,
        Step::Indexing,
        Step::Points,
        Step::Ground,
    ];

    fn code(self) -> u8 {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0) as u8 + 1
    }

    fn of(code: u8) -> Option<Step> {
        Self::ALL.get(usize::from(code).checked_sub(1)?).copied()
    }

    /// The step in words ("solving the wind positions").
    pub fn words(self) -> &'static str {
        match self {
            Step::Solving => "solving the wind positions",
            Step::Arcs => "making the swing arcs",
            Step::Indexing => "indexing the wires",
            Step::Points => "reading and checking the points",
            Step::Ground => "checking the ground",
        }
    }

    /// What its counter counts ("positions").
    pub fn unit(self) -> &'static str {
        match self {
            Step::Solving => "positions",
            Step::Arcs => "arc cases",
            Step::Indexing => "rules",
            Step::Points => "points",
            Step::Ground => "wires and arc lines",
        }
    }
}

/// How far a zone check has got, shared with the window showing it.
#[derive(Debug, Default)]
pub struct ZoneProgress {
    step: AtomicU8,
    /// The steps this run makes per calculation, in their order (bits of
    /// [`Step::code`]).
    plan: AtomicU8,
    /// Done of the step under way.
    done: AtomicU64,
    /// To do in the step under way (0: not known yet).
    total: AtomicU64,
    /// The calculation under way (1-based) and their number.
    calc: AtomicUsize,
    calcs: AtomicUsize,
    /// When the step under way began.
    since: Mutex<Option<Instant>>,
    /// Wind positions solved in the solving step (its counter, as
    /// [`crate::zone::solve_states`] counts them).
    pub solved: AtomicUsize,
}

/// What a [`ZoneProgress`] says at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapshot {
    /// The step under way.
    pub step: Step,
    /// Its number among the run's steps (1-based).
    pub number: usize,
    /// The run's steps per calculation.
    pub steps: usize,
    /// The calculation under way (1-based).
    pub calc: usize,
    /// The calculations.
    pub calcs: usize,
    /// Done in the step.
    pub done: u64,
    /// To do in the step (0: not known).
    pub total: u64,
    /// Time since the step began.
    pub elapsed: Duration,
}

impl Snapshot {
    /// The share of the step done (0–1), when known.
    pub fn share(&self) -> Option<f64> {
        (self.total > 0).then(|| (self.done as f64 / self.total as f64).min(1.0))
    }

    /// The step's time left, s: from its pace so far, once 2 % is done;
    /// none when not known — nor once its counter is full while the step
    /// still runs (its last part), never "0 s left" while work remains.
    pub fn left(&self) -> Option<f64> {
        let share = self.share()?;
        (0.02..1.0)
            .contains(&share)
            .then(|| self.elapsed.as_secs_f64() / share * (1.0 - share))
    }

    /// The step in words: "step 4 of 5: reading and checking the points
    /// (calculation 1 of 2)".
    pub fn words(&self) -> String {
        let calc = if self.calcs > 1 {
            format!(" (calculation {} of {})", self.calc, self.calcs)
        } else {
            String::new()
        };
        format!(
            "step {} of {}: {}{calc}",
            self.number,
            self.steps,
            self.step.words()
        )
    }
}

impl ZoneProgress {
    /// A new progress, nothing begun.
    pub fn new() -> Self {
        Self::default()
    }

    /// The steps the run makes per calculation, and the number of
    /// calculations.
    pub fn plan(&self, steps: &[Step], calcs: usize) {
        let bits = steps.iter().fold(0u8, |b, s| b | 1 << s.code());
        self.plan.store(bits, Ordering::Relaxed);
        self.calcs.store(calcs.max(1), Ordering::Relaxed);
    }

    /// Calculation `k` (0-based) begins.
    pub fn calculation(&self, k: usize) {
        self.calc.store(k + 1, Ordering::Relaxed);
    }

    /// Step `step` begins with `total` to do (0: not known yet).
    pub fn begin(&self, step: Step, total: u64) {
        self.done.store(0, Ordering::Relaxed);
        self.solved.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        *self.since.lock().expect("not poisoned") = Some(Instant::now());
        self.step.store(step.code(), Ordering::Relaxed);
    }

    /// `n` more done in the step under way.
    pub fn add(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }

    /// The step's counters set (a point reader's: the points of the
    /// blocks listed and of those read).
    pub fn reading(&self, done: u64, total: u64) {
        self.total.store(total, Ordering::Relaxed);
        self.done.store(done, Ordering::Relaxed);
    }

    /// What it says now (none before the first step).
    pub fn snapshot(&self) -> Option<Snapshot> {
        let step = Step::of(self.step.load(Ordering::Relaxed))?;
        let since = *self.since.lock().expect("not poisoned");
        let plan = self.plan.load(Ordering::Relaxed);
        let planned: Vec<Step> = Step::ALL
            .into_iter()
            .filter(|s| plan & (1 << s.code()) != 0)
            .collect();
        let (number, steps) = match planned.iter().position(|s| *s == step) {
            Some(k) => (k + 1, planned.len()),
            None => (step.code() as usize, Step::ALL.len()),
        };
        Some(Snapshot {
            step,
            number,
            steps,
            calc: self.calc.load(Ordering::Relaxed).max(1),
            calcs: self.calcs.load(Ordering::Relaxed).max(1),
            done: match step {
                Step::Solving => self.solved.load(Ordering::Relaxed) as u64,
                _ => self.done.load(Ordering::Relaxed),
            },
            total: self.total.load(Ordering::Relaxed),
            elapsed: since.map_or(Duration::ZERO, |s| s.elapsed()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_count_and_never_say_nothing_left_while_running() {
        let p = ZoneProgress::new();
        assert!(p.snapshot().is_none());
        p.plan(
            &[Step::Solving, Step::Indexing, Step::Points, Step::Ground],
            2,
        );
        p.calculation(1);
        p.begin(Step::Points, 0);
        let s = p.snapshot().unwrap();
        assert_eq!((s.number, s.steps, s.calc, s.calcs), (3, 4, 2, 2));
        assert_eq!(s.share(), None);
        assert_eq!(s.left(), None);
        assert_eq!(
            s.words(),
            "step 3 of 4: reading and checking the points (calculation 2 of 2)"
        );
        p.reading(50, 100);
        let s = p.snapshot().unwrap();
        assert_eq!(s.share(), Some(0.5));
        assert!(s.left().is_some());
        // Everything read: the step's last part runs on, no time said.
        p.reading(100, 100);
        assert_eq!(p.snapshot().unwrap().left(), None);
        // The next step starts its own counter.
        p.begin(Step::Ground, 40);
        p.add(10);
        let s = p.snapshot().unwrap();
        assert_eq!(
            (s.step, s.number, s.done, s.total),
            (Step::Ground, 4, 10, 40)
        );
    }
}
