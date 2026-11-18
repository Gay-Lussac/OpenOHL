// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **ASCE 10-97** (*Design of Latticed Steel Transmission Structures*,
//! American Society of Civil Engineers, 1998) for OpenOHL's structures —
//! members, joints and the bolt tools' minimum distances — as a plugin
//! (`structure-code-plugin` world): the code OpenOHL's core had as its
//! default member check (`ohl_structures_fe::asce10`, its branches of
//! `check`, `plates` and `bolt_rules`), moved out of the core unchanged
//! (owner 2026-11-17: "yes go ahead with your recommendations"; moved
//! 2026-11-18, the second round after SIA 263:2013). Built, in active
//! development, not approved yet.
//!
//! **Bundled with OpenOHL** and found before the user's plugins; every
//! structure without `[analysis] capacity`, or naming `"asce10-97"`, is
//! checked by it (the default; the old name an alias of its method
//! `asce10-97`, nothing rewritten).
//!
//! - `member-check-full` (`check-members-full`): the members, every load
//!   case in one call, with everything the core's check read
//!   ([`check`], [`asce10`]); `member-check`'s `check-members` (the
//!   smaller record) is refused: without the section's and the forces'
//!   details the check would not be ASCE 10-97's.
//! - `joint-check` (and `joint-check-full`): a member's bolts in a gusset
//!   plate or cover; a weld between two plates is not checked (ASCE 10-97
//!   gives no weld resistance) ([`joints`]).
//! - `bolt-distances` (and `bolt-distances-full`): ASCE 10-97 4.5 with the
//!   bolt's force; without an analysis the bolt's own design shear
//!   capacity (4.3.2, [`asce10::bolt_shear`]; third round, 2026-11-18).
//! - **Parameters**: none — ASCE 10-97's design strengths have no factors
//!   to set ([`params`]); a parameter file `Plugins/asce10.toml` that is not
//!   empty is refused, so that a value meant for it is never silently left
//!   out.
//!
//! Build with `cargo build --release --target wasm32-wasip2`; its own
//! tests with `cargo test` (natively).

pub mod asce10;
pub mod check;
pub mod joints;
pub mod memo;
pub mod model;
pub mod params;
#[cfg(test)]
mod tests;
#[cfg(target_arch = "wasm32")]
mod wasm;
