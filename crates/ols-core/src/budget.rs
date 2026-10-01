// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exit clauses for iterative and long-running work.
//!
//! Every iterative solver and long-running task in OLS CAD must stop on its
//! own: when it converges, when it reaches its iteration limit, when its time
//! limit expires, or when the user cancels. It must never loop forever, and
//! when it stops early it must say *why* (DESIGN_GOALS §1).
//!
//! ```
//! use std::time::Duration;
//! use ols_core::budget::{Budget, Stop};
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

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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

    /// Starts tracking a run against this budget.
    pub fn start(&self) -> Run<'_> {
        Run {
            budget: self,
            started: Instant::now(),
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
    started: Instant,
    iterations: u64,
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

    /// Time since the run started.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
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
