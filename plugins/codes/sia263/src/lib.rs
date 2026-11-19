// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **SIA 263:2013** (*Construction en acier / Stahlbau*) for OpenOHL's
//! structures — members, joints and the bolt tools' minimum distances —
//! as a plugin (`structure-code-plugin` world): the code OpenOHL's core
//! had (`ohl_structures_fe::sia263`, its branches of `check`, `plates`
//! and `bolt_rules`), moved out of the core unchanged (owner 2026-11-17:
//! "yes go ahead with your recommendations"; moved 2026-11-18). Built, in
//! active development, not approved yet.
//!
//! **Bundled with OpenOHL** and found before the user's plugins; files
//! naming `[analysis] capacity = "sia263-2013"` are checked by it (the old
//! name an alias of its method `sia263-2013`, nothing rewritten).
//!
//! - `member-check-full` (`check-members-full`): the members, every load
//!   case in one call, with everything the core's check read
//!   ([`check`], [`sia263`]); `member-check`'s `check-members` (the
//!   smaller record) is refused: without the section's and the forces'
//!   details the check would not be SIA 263's.
//! - `joint-check`: a member's bolts in a gusset plate or cover, a weld
//!   between two plates ([`joints`]).
//! - `joint-check-groups` (2026-11-19): a joint's bolt groups under the
//!   members' forces together, each bolt by Table 16 and Table 17
//!   ([`groups`]).
//! - `bolt-distances` (and `bolt-distances-full`): SIA 263 Table 17 by
//!   bolt size; without an analysis the force per bolt shown with them is
//!   the bolt's own design shear resistance by Table 16 over γ_M2
//!   ([`sia263::bolt_shear`]; third round, 2026-11-18).
//! - **Parameters** (`Plugins/sia263.toml` beside a structure): γ_M1, γ_M2,
//!   f_uE ([`params`]); without a file, the standard's.
//!
//! Build with `cargo build --release --target wasm32-wasip2`; its own
//! tests with `cargo test` (natively).

pub mod bends;
pub mod bracing;
pub mod check;
pub mod groups;
pub mod joints;
pub mod lengths;
pub mod memo;
pub mod model;
pub mod params;
pub mod shared;
pub mod sia263;
#[cfg(test)]
mod tests;
#[cfg(target_arch = "wasm32")]
mod wasm;
