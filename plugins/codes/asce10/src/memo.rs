// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What one call works out once: a member's capacities (the same in every
//! load case but for its crossing partner in tension or not). The same
//! function of the same data: the same numbers, kept for the call only
//! ([`start`], [`stop`]) — the core worked them out again for every case
//! (2026-11-18: a tower of 3 000 members in many load cases within the
//! plugins' time limit).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::asce10::Capacity;

type Capacities = HashMap<(String, Option<bool>), Capacity>;

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static CAPACITIES: RefCell<Capacities> = RefCell::new(HashMap::new());
}

/// Keeps what is worked out until [`stop`].
pub fn start() {
    ON.with(|o| o.set(true));
    CAPACITIES.with(|c| c.borrow_mut().clear());
}

/// Forgets it.
pub fn stop() {
    ON.with(|o| o.set(false));
    CAPACITIES.with(|c| c.borrow_mut().clear());
}

/// `f`'s capacities of member `id` with partner `partner`, once per call.
pub fn capacity(id: &str, partner: Option<bool>, f: impl FnOnce() -> Capacity) -> Capacity {
    if !ON.with(Cell::get) {
        return f();
    }
    let key = (id.to_owned(), partner);
    if let Some(c) = CAPACITIES.with(|c| c.borrow().get(&key).cloned()) {
        return c;
    }
    let c = f();
    CAPACITIES.with(|m| m.borrow_mut().insert(key, c.clone()));
    c
}
