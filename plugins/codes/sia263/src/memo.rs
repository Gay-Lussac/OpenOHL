// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What one call works out once (2026-11-18: a tower of 2 400 members in
//! four load cases took more than the plugins' 5 s): a member's
//! capacities (the same in every case but for its crossing partner) and a
//! class 4 angle's effective section (the same for every member of that
//! profile and steel). The same functions of the same data: the same
//! numbers, kept for the call only ([`start`], [`stop`]).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::shared::Capacity;
use crate::sia263::Stability;

/// By member, its crossing partner in tension or not, and the key of its
/// lengths worked out from the bracing (0: none).
type Capacities = HashMap<(String, Option<bool>, u64), (Capacity, Option<Stability>)>;
type Effective = HashMap<(String, u64, u64), Option<([f64; 2], f64)>>;

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static CAPACITIES: RefCell<Capacities> = RefCell::new(HashMap::new());
    static EFFECTIVE: RefCell<Effective> = RefCell::new(HashMap::new());
}

/// Keeps what is worked out until [`stop`].
pub fn start() {
    ON.with(|o| o.set(true));
    CAPACITIES.with(|c| c.borrow_mut().clear());
    EFFECTIVE.with(|c| c.borrow_mut().clear());
}

/// Forgets it.
pub fn stop() {
    ON.with(|o| o.set(false));
    CAPACITIES.with(|c| c.borrow_mut().clear());
    EFFECTIVE.with(|c| c.borrow_mut().clear());
}

/// `f`'s capacities of member `id` with partner `partner` and lengths
/// `lengths` (their key), once per call.
pub fn capacity(
    id: &str,
    partner: Option<bool>,
    lengths: u64,
    f: impl FnOnce() -> (Capacity, Option<Stability>),
) -> (Capacity, Option<Stability>) {
    if !ON.with(Cell::get) {
        return f();
    }
    let key = (id.to_owned(), partner, lengths);
    if let Some(c) = CAPACITIES.with(|c| c.borrow().get(&key).cloned()) {
        return c;
    }
    let c = f();
    CAPACITIES.with(|m| m.borrow_mut().insert(key, c.clone()));
    c
}

/// `f`'s effective angle of profile `id` at yield `fy` (its gross minor
/// modulus `w` told apart too), once per call.
pub fn effective(
    id: &str,
    fy: f64,
    w: f64,
    f: impl FnOnce() -> Option<([f64; 2], f64)>,
) -> Option<([f64; 2], f64)> {
    if !ON.with(Cell::get) {
        return f();
    }
    let key = (id.to_owned(), fy.to_bits(), w.to_bits());
    if let Some(c) = EFFECTIVE.with(|c| c.borrow().get(&key).copied()) {
        return c;
    }
    let c = f();
    EFFECTIVE.with(|m| m.borrow_mut().insert(key, c));
    c
}
