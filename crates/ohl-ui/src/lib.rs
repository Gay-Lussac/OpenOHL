// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The look and the widgets both applications share: OHL-Design
//! (`ohl-app`) and OHL-Structure (`ohl-structure-app`) use this one copy,
//! so a fix in one reaches the other. It holds no model and no engine, and
//! never depends on an application ([plan and rule](https://github.com/Gay-Lussac/OpenOHL/blob/main/docs/development/ui-shared.md)).
//!
//! What differs between the applications stays in theirs: the storage key
//! of the UI scale, OHL-Structure's taller rows and right-aligned value
//! fields, the short sentences OHL-Design makes of an edit's message, each
//! application's tools and icons.

pub mod glyphs;
pub mod icons;
pub mod messages;
pub mod notices;
pub mod rows;
pub mod snap_marker;
pub mod style;
