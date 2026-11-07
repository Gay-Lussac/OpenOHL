// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Notices: the shared ones ([`ohl_ui::notices`]).

pub use ohl_ui::notices::Notices;

/// How often the window is drawn again while an information fades, s (in
/// the tests' steps of 0.25 s: not at once).
pub const TICK: Option<f64> = Some(if cfg!(test) { 0.3 } else { 0.05 });
