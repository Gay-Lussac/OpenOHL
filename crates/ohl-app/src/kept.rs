// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **One value kept by what it was made from** (owner, 2026-11-10: what
//! the views and windows worked out again on every frame). A [`Kept`]
//! holds the last value and its key; asked with the same key it hands the
//! value out again, with another it is made again. The key must name
//! every input of the value (the line's version, the solve's serial, the
//! libraries' revision…): nothing here looks at the value's inputs
//! themselves. Only *when* a value is computed changes, never the value.

use std::sync::Mutex;

/// The last value made and what it was made from.
#[derive(Debug)]
pub struct Kept<K, V>(Mutex<Option<(K, V)>>);

impl<K, V> Default for Kept<K, V> {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

impl<K: PartialEq, V: Clone> Kept<K, V> {
    /// The value for `key`: the one kept when it was made for the same
    /// key, otherwise made now and kept (values are shared ones, `Arc`s:
    /// the copy handed out is a count). `make` runs outside the lock: it
    /// may ask other kept values.
    pub fn get(&self, key: K, make: impl FnOnce() -> V) -> V {
        if let Some((k, v)) = self.lock().as_ref()
            && *k == key
        {
            return v.clone();
        }
        let v = make();
        *self.lock() = Some((key, v.clone()));
        v
    }

    /// Nothing kept any more: made again when next asked.
    pub fn forget(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(K, V)>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Made once per key, again when the key changes or after `forget`.
    #[test]
    fn a_kept_value_is_made_once_per_key() {
        let kept: Kept<(u64, u64), Arc<Vec<u64>>> = Kept::default();
        let mut made = 0;
        let mut ask = |key: (u64, u64)| {
            kept.get(key, || {
                made += 1;
                Arc::new(vec![key.0, key.1])
            })
        };
        assert_eq!(*ask((1, 1)), vec![1, 1]);
        assert_eq!(*ask((1, 1)), vec![1, 1]);
        assert_eq!(*ask((1, 2)), vec![1, 2]);
        assert_eq!(*ask((1, 2)), vec![1, 2]);
        assert_eq!(*ask((1, 1)), vec![1, 1]);
        kept.forget();
        assert_eq!(*ask((1, 1)), vec![1, 1]);
        assert_eq!(made, 4);
    }
}
