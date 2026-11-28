// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OHL-Server, the admin app of OpenOHL (ROADMAP O17, stages A and B,
//! 2026-11-28; built, in active development, not approved yet).
//!
//! A desktop app for the admins of a network's server, in two modes:
//! **host** ([`host`]: the server inside the app, for a small office or a
//! test; closing asks first) and **connect** (to a headless `ohl network
//! serve`, the production way). Its pages ([`pages`]): Overview,
//! Operations (backups, migrations, the log, sessions, settings), a
//! simplified read-only [`map`], and the rights pages (users and groups,
//! roles, assignments, see as user).
//!
//! It is kept apart from the rest of OpenOHL behind one boundary,
//! [`conn`]: the server's protocol and its admin messages
//! (`ohl_server::admin`). It never reads or writes the database itself;
//! host mode starts the server program in this process, which does.

pub mod app;
pub mod conn;
pub mod help;
pub mod host;
pub mod map;
pub mod pages;
pub mod pages_cf;

pub use app::App;
