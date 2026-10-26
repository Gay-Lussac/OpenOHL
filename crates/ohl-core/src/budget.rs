// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Exit clauses for iterative and long-running work.
//!
//! Every iterative solver and long-running task in OpenOHL must stop on its
//! own: when it converges, when it reaches its iteration limit, when its time
//! limit expires, or when the user cancels. It must never loop forever, and
//! when it stops early it must say *why* (DESIGN_GOALS §1).
//!
//! ```
//! use std::time::Duration;
//! use ohl_core::budget::{Budget, Stop};
//!
//! // Newton iteration for √2 with an exit clause.
//! let budget = Budget::new().max_iterations(50).time_limit(Duration::from_millis(100));
//! let mut run = budget.start();
//! let mut x: f64 = 1.0;
//! loop {
//!     run.step()?;
//!     let next = x - (x * x - 2.0) / (2.0 * x);
//!     if (next - x).abs() < 1e-14 {
//!         break;
//!     }
//!     x = next;
//! }
//! assert!((x - 2f64.sqrt()).abs() < 1e-12);
//! assert!(run.iterations() < 10);
//! # Ok::<(), Stop>(())
//! ```
//!
//! # The clock
//!
//! Reading the system clock costs about as much as one iteration of a
//! catenary element (~0.1 µs on macOS; profiled 2026-10-26, half of a
//! wire solve's time went into it). So a [`Run`] reads it only when its
//! budget has a time limit, and then not on every [`Run::step`]: while any
//! timed run is alive, a background thread (`ohl-budget-clock`) reads the
//! clock once every [`CLOCK_TICK`] into a shared counter, and the runs
//! read that counter — one atomic load, as for the cancel token. A time
//! limit is therefore noticed within about a millisecond of expiring (the
//! tick, plus the operating system's delay in waking the thread), early or
//! late; the iterations, the iteration limit and the cancel token are
//! exact, and no result depends on the clock. The thread sleeps (parked)
//! while no timed run is alive. Where no thread can be started, the runs
//! read the clock themselves.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often the shared clock of the time limits is read (see [the
/// clock](self#the-clock)): a time limit is noticed within about this of
/// expiring.
pub const CLOCK_TICK: Duration = Duration::from_millis(1);

/// The shared coarse clock of the timed runs (see [the
/// clock](self#the-clock)).
mod clock {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::thread::Thread;
    use std::time::Instant;

    static EPOCH: OnceLock<Instant> = OnceLock::new();
    /// Microseconds since `EPOCH`, as last read.
    static NOW: AtomicU64 = AtomicU64::new(0);
    /// Timed runs alive.
    static USERS: AtomicUsize = AtomicUsize::new(0);
    /// The thread keeping `NOW` (none: it could not be started).
    static TICKER: OnceLock<Option<Thread>> = OnceLock::new();

    /// The clock read now, µs since `EPOCH` (kept in `NOW` too).
    fn precise() -> u64 {
        let epoch = *EPOCH.get_or_init(Instant::now);
        let now = u64::try_from(epoch.elapsed().as_micros()).unwrap_or(u64::MAX);
        NOW.fetch_max(now, Ordering::Relaxed);
        now
    }

    fn ticker() -> Option<&'static Thread> {
        TICKER
            .get_or_init(|| {
                std::thread::Builder::new()
                    .name("ohl-budget-clock".into())
                    .spawn(|| {
                        loop {
                            // Parked while no timed run is alive (an
                            // unpark before the park makes it return).
                            if USERS.load(Ordering::Acquire) == 0 {
                                std::thread::park();
                                continue;
                            }
                            precise();
                            std::thread::sleep(super::CLOCK_TICK);
                        }
                    })
                    .ok()
                    .map(|h| h.thread().clone())
            })
            .as_ref()
    }

    /// A timed run starts: its start, µs.
    pub(super) fn enter() -> u64 {
        let first = USERS.fetch_add(1, Ordering::AcqRel) == 0;
        match ticker() {
            // The ticker runs: its reading is at most a tick old.
            Some(_) if !first => NOW.load(Ordering::Relaxed),
            Some(t) => {
                let now = precise();
                t.unpark();
                now
            }
            None => precise(),
        }
    }

    /// A timed run ends.
    pub(super) fn leave() {
        USERS.fetch_sub(1, Ordering::AcqRel);
    }

    /// The time now, µs (while a timed run is alive).
    pub(super) fn now() -> u64 {
        match ticker() {
            Some(_) => NOW.load(Ordering::Relaxed),
            None => precise(),
        }
    }
}

/// Shared flag used to cancel running work from another thread (e.g. the GUI).
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// New, not-cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation of every task holding this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Limits for one task. Without limits set, only cancellation can stop it,
/// so solvers must always set at least an iteration limit.
#[derive(Debug, Clone, Default)]
pub struct Budget {
    max_iterations: Option<u64>,
    time_limit: Option<Duration>,
    cancel: Option<CancelToken>,
}

impl Budget {
    /// Budget without limits.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stops after this many iterations.
    pub fn max_iterations(mut self, n: u64) -> Self {
        self.max_iterations = Some(n);
        self
    }

    /// Stops after this much wall-clock time.
    pub fn time_limit(mut self, limit: Duration) -> Self {
        self.time_limit = Some(limit);
        self
    }

    /// Stops when the token is cancelled.
    pub fn cancel_token(mut self, token: CancelToken) -> Self {
        self.cancel = Some(token);
        self
    }

    /// Its iteration limit, if any (e.g. as part of a memo's key: a result
    /// found within a larger limit may not be found within a smaller one).
    pub fn iteration_limit(&self) -> Option<u64> {
        self.max_iterations
    }

    /// The token that cancels it, if any (to share with sub-tasks).
    pub fn token(&self) -> Option<&CancelToken> {
        self.cancel.as_ref()
    }

    /// Starts tracking a run against this budget. Without a time limit no
    /// clock is read (see [the clock](self#the-clock)).
    pub fn start(&self) -> Run<'_> {
        Run {
            budget: self,
            started: self.time_limit.map(|_| clock::enter()),
            iterations: 0,
        }
    }
}

/// Why a task stopped before finishing.
#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    /// The iteration limit was reached.
    IterationLimit {
        /// Iterations performed.
        iterations: u64,
        /// Time spent.
        elapsed: Duration,
    },
    /// The time limit expired.
    TimeLimit {
        /// The limit.
        limit: Duration,
        /// Iterations performed before stopping.
        iterations: u64,
    },
    /// The user (or the system) cancelled the task.
    Cancelled {
        /// Iterations performed before stopping.
        iterations: u64,
    },
}

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // A run without a time limit does not measure its time.
            Self::IterationLimit {
                iterations,
                elapsed,
            } if elapsed.is_zero() => {
                write!(
                    f,
                    "stopped after reaching the limit of {iterations} iterations"
                )
            }
            Self::IterationLimit {
                iterations,
                elapsed,
            } => {
                write!(
                    f,
                    "stopped after reaching the limit of {iterations} iterations ({elapsed:.2?})"
                )
            }
            Self::TimeLimit { limit, iterations } => {
                write!(
                    f,
                    "stopped after the time limit of {limit:.2?} ({iterations} iterations done)"
                )
            }
            Self::Cancelled { iterations } => write!(f, "cancelled after {iterations} iterations"),
        }
    }
}

impl std::error::Error for Stop {}

/// A task running against a [`Budget`].
#[derive(Debug)]
pub struct Run<'a> {
    budget: &'a Budget,
    /// Its start on the shared clock, µs (with a time limit only).
    started: Option<u64>,
    iterations: u64,
}

impl Drop for Run<'_> {
    fn drop(&mut self) {
        if self.started.is_some() {
            clock::leave();
        }
    }
}

impl Run<'_> {
    /// Records one iteration and checks every limit. Call it once per
    /// iteration, *before* doing the work; propagate the error with `?`.
    pub fn step(&mut self) -> Result<(), Stop> {
        if let Some(max) = self.budget.max_iterations
            && self.iterations >= max
        {
            return Err(Stop::IterationLimit {
                iterations: self.iterations,
                elapsed: self.elapsed(),
            });
        }
        self.check()?;
        self.iterations += 1;
        Ok(())
    }

    /// Checks the time limit and cancellation without counting an iteration.
    /// Use inside long steps that are not iterations.
    pub fn check(&self) -> Result<(), Stop> {
        if self
            .budget
            .cancel
            .as_ref()
            .is_some_and(CancelToken::is_cancelled)
        {
            return Err(Stop::Cancelled {
                iterations: self.iterations,
            });
        }
        if let Some(limit) = self.budget.time_limit
            && self.elapsed() >= limit
        {
            return Err(Stop::TimeLimit {
                limit,
                iterations: self.iterations,
            });
        }
        Ok(())
    }

    /// Iterations performed so far.
    pub fn iterations(&self) -> u64 {
        self.iterations
    }

    /// Time since the run started, to within about [`CLOCK_TICK`];
    /// measured only when the budget has a time limit (zero without one).
    pub fn elapsed(&self) -> Duration {
        self.started.map_or(Duration::ZERO, |s| {
            Duration::from_micros(clock::now().saturating_sub(s))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An iteration that never converges must stop at the iteration limit.
    #[test]
    fn iteration_limit_stops_endless_loop() {
        let budget = Budget::new().max_iterations(1000);
        let mut run = budget.start();
        let stop = loop {
            if let Err(stop) = run.step() {
                break stop;
            }
        };
        assert!(
            matches!(
                stop,
                Stop::IterationLimit {
                    iterations: 1000,
                    ..
                }
            ),
            "{stop}"
        );
    }

    #[test]
    fn time_limit_stops_endless_loop() {
        let budget = Budget::new().time_limit(Duration::from_millis(20));
        let mut run = budget.start();
        let stop = loop {
            if let Err(stop) = run.step() {
                break stop;
            }
        };
        assert!(matches!(stop, Stop::TimeLimit { .. }), "{stop}");
        assert!(run.elapsed() < Duration::from_secs(1));
    }

    /// The shared clock (read once a millisecond by its thread) stops a
    /// run at its time limit, not much later; a run without a time limit
    /// does not measure its time.
    #[test]
    fn the_shared_clock_keeps_time_limits() {
        let limit = Duration::from_millis(30);
        let budget = Budget::new().time_limit(limit);
        let started = std::time::Instant::now();
        let mut run = budget.start();
        let stop = loop {
            if let Err(stop) = run.step() {
                break stop;
            }
        };
        let took = started.elapsed();
        assert!(matches!(stop, Stop::TimeLimit { .. }), "{stop}");
        // Early by at most a tick, late by a tick and the thread's waking
        // (generous here: tests run side by side).
        assert!(took + 2 * CLOCK_TICK >= limit, "{took:?}");
        assert!(took < limit + Duration::from_millis(500), "{took:?}");
        let untimed = Budget::new().max_iterations(3);
        let mut run = untimed.start();
        let stop = loop {
            if let Err(stop) = run.step() {
                break stop;
            }
        };
        assert_eq!(run.elapsed(), Duration::ZERO);
        assert_eq!(
            stop.to_string(),
            "stopped after reaching the limit of 3 iterations"
        );
    }

    #[test]
    fn cancellation_from_another_thread() {
        let token = CancelToken::new();
        let budget = Budget::new()
            .cancel_token(token.clone())
            .time_limit(Duration::from_secs(5));
        let worker = std::thread::spawn(move || {
            let mut run = budget.start();
            loop {
                if let Err(stop) = run.step() {
                    return stop;
                }
            }
        });
        std::thread::sleep(Duration::from_millis(10));
        token.cancel();
        let stop = worker.join().unwrap();
        assert!(matches!(stop, Stop::Cancelled { .. }), "{stop}");
    }

    #[test]
    fn explains_why_it_stopped() {
        let stop = Stop::TimeLimit {
            limit: Duration::from_secs(2),
            iterations: 42,
        };
        assert_eq!(
            stop.to_string(),
            "stopped after the time limit of 2.00s (42 iterations done)"
        );
    }
}
