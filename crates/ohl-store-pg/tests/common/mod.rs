// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The throwaway PostgreSQL of the tests: the crate's own (feature
//! `testing`, `src/testing.rs`), included here so the store's tests need
//! no feature — one cluster per test program, a database per test,
//! stopped and deleted with the last test, also after a failure.

#[path = "../../src/testing.rs"]
mod testing;

pub use testing::Cluster;
