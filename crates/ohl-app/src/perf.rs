// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Frame phases timed (the frame benchmark, docs/development/performance.md):
//! [`span`] around a phase adds its time to a per-thread table while
//! timing is on ([`enable`]); off (the default) a span costs one atomic
//! load. Display only: nothing here changes what is drawn or computed.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static ON: AtomicBool = AtomicBool::new(false);

thread_local! {
    static TABLE: RefCell<Vec<(&'static str, Duration, u32)>> = const { RefCell::new(Vec::new()) };
}

/// Timing on or off (off by default).
#[cfg_attr(not(test), allow(dead_code))]
pub fn enable(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// A phase being timed: its time is added when it is dropped.
pub struct Span(Option<(&'static str, Instant)>);

/// Times the phase `name` until the returned guard is dropped.
pub fn span(name: &'static str) -> Span {
    Span(ON.load(Ordering::Relaxed).then(|| (name, Instant::now())))
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, at)) = self.0.take() {
            let d = at.elapsed();
            TABLE.with(|t| {
                let mut t = t.borrow_mut();
                match t.iter_mut().find(|(n, _, _)| *n == name) {
                    Some(e) => {
                        e.1 += d;
                        e.2 += 1;
                    }
                    None => t.push((name, d, 1)),
                }
            });
        }
    }
}

/// The phases timed since the last call (name, total time, times), in
/// the order first seen; the table emptied.
#[cfg_attr(not(test), allow(dead_code))]
pub fn take() -> Vec<(&'static str, Duration, u32)> {
    TABLE.with(|t| std::mem::take(&mut *t.borrow_mut()))
}
