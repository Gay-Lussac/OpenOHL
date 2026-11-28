// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The admin messages of a network's server (OHL-Server, the admin app,
//! stages A and B: ROADMAP O17, ADR-0013's amendment of 2026-11-28).
//!
//! An admin app talks to the server's protocol only, never to the
//! database: it joins as any client does (an invitation or an OIDC
//! sign-in, following nothing), then asks [`AdminAsk`]s in
//! [`crate::ClientMsg::Admin`]; each is answered once with
//! [`crate::ServerMsg::Admin`] of the same id. The messages are
//! **additive**: protocol 3 is kept (an older client never sends them, an
//! older server closes the connection on one — the app reads
//! [`crate::NetworkInfo::admin`] first and says the server is too old).
//!
//! Every message is checked by the rights' one function
//! ([`ohl_session::rights::UserRights::allows`]): the server's state,
//! sessions, log, backups and settings by **network admins** only; the
//! rights pages by whoever manages rights somewhere (network admins, line
//! admins on their lines, library maintainers on their libraries), each
//! seeing only what they manage; the map by what the asker may see. Every
//! change (a rights command, an invitation, a disconnection, a backup, a
//! restore drill), every refusal and every "see as" is in the network's
//! append-only audit. Reads that change nothing are not (an app refreshes
//! them every few seconds).
//!
//! "What changes" ([`what_changes`]) is worked out here, on a copy of the
//! rights, without saving: who gains and who loses which permission on
//! which scope, counted.

use std::collections::{BTreeMap, BTreeSet};

use ohl_session::rights::{Assignment, Permission, RightsCommand, RightsData, Subject};
use serde::{Deserialize, Serialize};

/// The admin messages' version a server speaks ([`crate::NetworkInfo::admin`];
/// 0: a server of before them; 1: stages A and B; 2: stages C to F — the
/// shared towers and twins, the Libraries, Lines and Audit pages, the
/// identities, "explain", the network's settings changed).
pub const ADMIN: u32 = 2;

/// The oldest admin messages' version the app works with (stages A and
/// B; the pages of stages C to F need [`ADMIN`] 2).
pub const ADMIN_MIN: u32 = 1;

/// Bytes of a line's file in one [`AdminAsk::Download`] or
/// [`AdminAsk::Upload`] (base64 in the message: under
/// [`crate::LARGE_MESSAGE`]); a big file goes in many, never held whole in
/// memory.
pub const CHUNK: usize = 512 * 1024;

/// The largest line file an admin uploads for an import, bytes.
pub const MAX_UPLOAD: u64 = 8 << 30;

/// Line files being sent or received at once, per connection.
pub const MAX_TRANSFERS: usize = 2;

/// Audit rows one [`AdminAsk::Audit`] answers at most.
pub const AUDIT_PAGE_MAX: usize = 1000;

/// Shared towers one [`AdminAsk::SharedTowers`] lists at most (the count
/// is given in full).
pub const SHARED_MAX: usize = 5000;

/// The log lines a server keeps for its admins (the newest).
pub const LOG_LINES: usize = 4000;

/// Structures one [`AdminAsk::Structures`] answers at most (zoom in for
/// more).
pub const MAX_DOTS: usize = 20_000;

/// A question or an action of an admin app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "snake_case")]
// A few at a time: the rights command kept in place.
#[allow(clippy::large_enum_variant)]
pub enum AdminAsk {
    /// The server at a glance ([`AdminReply::Overview`]).
    Overview,
    /// Who is connected ([`AdminReply::Sessions`]; network admins).
    Sessions,
    /// Lets a connection go, with a reason the user is shown (network
    /// admins; audited).
    Disconnect {
        /// The connection ([`SessionRow::client`]).
        client: u64,
        /// Why (shown to the user, in the audit).
        reason: String,
    },
    /// The server's log after line `after` ([`AdminReply::Log`]; network
    /// admins).
    Log {
        /// The last line the app holds (0: from the oldest kept).
        #[serde(default)]
        after: u64,
        /// Only lines holding this text (any case; empty: every line).
        #[serde(default)]
        filter: String,
    },
    /// The backups in the server's backup folder ([`AdminReply::Backups`];
    /// network admins).
    Backups,
    /// A backup made now into a new folder of the server's backup folder
    /// (network admins; audited; answered when done).
    BackupNow,
    /// The restore drill of a backup of the backup folder: its dump checked
    /// against its manifest, and, with the server's drill database, restored
    /// there and every line compared (network admins; audited; answered when
    /// done).
    RestoreDrill {
        /// The backup's folder name.
        backup: String,
    },
    /// The server's settings, read-only ([`AdminReply::Settings`]; network
    /// admins).
    Settings,
    /// The network's users, groups, roles and assignments the asker
    /// manages ([`AdminReply::Rights`]).
    Rights,
    /// A rights change, saved (the rights' one function decides; audited).
    Change {
        /// The change.
        command: RightsCommand,
    },
    /// What a rights change would change, nothing saved
    /// ([`AdminReply::Preview`]).
    Preview {
        /// The change.
        command: RightsCommand,
    },
    /// A user added (if new) and their personal invitation made (network
    /// admins; audited).
    Invite {
        /// The user's name.
        name: String,
        /// Their display name (default: the name).
        #[serde(default)]
        display: Option<String>,
        /// Valid this many days (none: the server's default).
        #[serde(default)]
        days: Option<u32>,
    },
    /// A user's personal invitation made again: every older one refused
    /// from now on (network admins; audited).
    Reissue {
        /// The user.
        name: String,
        /// Valid this many days (none: the server's default).
        #[serde(default)]
        days: Option<u32>,
    },
    /// What `user` sees, as both the asker and they may (the server's own
    /// filter; audited) ([`AdminReply::SeeAs`]).
    SeeAs {
        /// The user.
        user: String,
    },
    /// The map: every route the asker may see, simplified
    /// ([`AdminReply::Map`]).
    Map,
    /// The structures standing in an area the asker may see
    /// ([`AdminReply::Structures`]; at most [`MAX_DOTS`]).
    Structures {
        /// x0, y0, x1, y1, m.
        bounds: [f64; 4],
    },
    /// A route's structures, in order along it (the assignments' tree).
    RouteStructures {
        /// The line (model).
        model: String,
        /// The route.
        route: String,
    },
    /// The country libraries the asker may see ([`AdminReply::Libraries`]).
    Libraries,
    // Stage C (with phase 8): shared towers and twins.
    /// The shared towers the asker may see: structures on two routes or
    /// more, their routes and stale marks ([`AdminReply::SharedTowers`]).
    SharedTowers,
    /// One shared tower: its routes, their stale marks, and who may edit
    /// it (shown to who manages rights on its line)
    /// ([`AdminReply::SharedTower`]).
    SharedTower {
        /// The line.
        model: String,
        /// The structure.
        id: String,
    },
    /// The twin towers of a line: its structures within a distance of
    /// another line's of the same type — shown, never merged (owner
    /// 2026-11-25) ([`AdminReply::Twins`]).
    Twins {
        /// The line.
        model: String,
        /// Within this distance, m (default the import's, 0.5 m).
        #[serde(default)]
        tolerance: Option<f64>,
    },
    // Stage D (with phase 9): the Libraries page.
    /// The country libraries as their page shows them: each kind's lock,
    /// maintainers, files and entries, releases, withdrawn entries; the
    /// requests; which lines pin which release ([`AdminReply::LibraryPage`]).
    LibraryPage,
    /// Where an entry is used (the use index, as the asker may see it)
    /// ([`AdminReply::Uses`]).
    Uses {
        /// The library kind.
        kind: String,
        /// The entry.
        entry: String,
    },
    /// A maintainer's library change (the store's rights decide; audited).
    Library {
        /// The change.
        action: LibraryAction,
    },
    /// What Update library pins would do to a line, nothing changed
    /// ([`AdminReply::Pins`]).
    PinsPlan {
        /// The line.
        line: String,
        /// Only these kinds (empty: every kind released).
        #[serde(default)]
        kinds: Vec<String>,
    },
    /// Update library pins of a line, as planned (the rights decide; one
    /// logged command, the routes using what changed marked out of date).
    UpdatePins {
        /// The line.
        line: String,
        /// Only these kinds (empty: every kind released).
        #[serde(default)]
        kinds: Vec<String>,
    },
    // Stage E: the Lines page.
    /// The lines the asker may see ([`AdminReply::Lines`]).
    Lines,
    /// One line exported as a standalone project file, written on the
    /// server's exports' thread (export on the whole line; audited); then
    /// fetched with [`AdminAsk::Download`] ([`AdminReply::Transfer`]).
    ExportLine {
        /// The line.
        model: String,
    },
    /// A piece of a file the server holds for this connection
    /// ([`AdminReply::Chunk`]).
    Download {
        /// The transfer.
        token: String,
        /// From this byte.
        offset: u64,
    },
    /// A project file to import starts coming (network admins)
    /// ([`AdminReply::Transfer`]).
    UploadStart {
        /// Its file name (kept in the import's report).
        name: String,
        /// Its size, bytes.
        bytes: u64,
    },
    /// A piece of it, in order ([`AdminReply::Received`]).
    Upload {
        /// The transfer.
        token: String,
        /// Its first byte.
        offset: u64,
        /// The bytes, base64 (at most [`CHUNK`]).
        data: String,
    },
    /// The uploaded file imported as a new line of the network (network
    /// admins; audited) ([`AdminReply::Imported`]).
    ImportLine {
        /// The transfer (the whole file received).
        token: String,
        /// The new line's code.
        code: String,
        /// Its name (default: the line's name in the file).
        #[serde(default)]
        name: Option<String>,
        /// Twin towers within this distance, m (default 0.5 m).
        #[serde(default)]
        tolerance: Option<f64>,
    },
    /// A transfer ended: its file removed from the server.
    TransferDone {
        /// The transfer.
        token: String,
    },
    // Stage F (with phase 13): the Audit page and the rest of phase 13.
    /// A page of the network's audit, newest first (network admins)
    /// ([`AdminReply::Audit`]).
    Audit {
        /// Which rows.
        #[serde(default)]
        filter: AuditFilter,
        /// Rows older than this one (none: from the newest).
        #[serde(default)]
        before: Option<i64>,
        /// Rows at most (0: 200; at most [`AUDIT_PAGE_MAX`]).
        #[serde(default)]
        limit: usize,
    },
    /// The OIDC identities mapped to network users (network admins)
    /// ([`AdminReply::Identities`]).
    Identities,
    /// Why a user may or may not do something (the rights' one function;
    /// audited) ([`AdminReply::Explain`]).
    Explain {
        /// The user.
        user: String,
        /// The permission.
        permission: Permission,
        /// The target's line (none: the network).
        #[serde(default)]
        line: Option<String>,
        /// The target's object kind (none: the line whole).
        #[serde(default)]
        kind: Option<String>,
        /// The target's object.
        #[serde(default)]
        id: Option<String>,
    },
    /// A network setting changed (`ohl network setting`; network admins;
    /// audited).
    SetSetting {
        /// The setting.
        key: String,
        /// Its value.
        value: String,
    },
}

/// A library maintainer's change from the admin app (the maintainers'
/// commands of `ohl network library`; files are changed and entries asked
/// for in OHL-Design, not here).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum LibraryAction {
    /// Lock or unlock a kind, or one file of it.
    Lock {
        /// The kind.
        kind: String,
        /// One file (none: the kind).
        #[serde(default)]
        file: Option<String>,
        /// Locked.
        locked: bool,
        /// Why.
        #[serde(default)]
        note: String,
    },
    /// A request granted.
    Grant {
        /// The request.
        id: i64,
        /// A note.
        #[serde(default)]
        note: String,
    },
    /// A request refused, with a reason.
    Refuse {
        /// The request.
        id: i64,
        /// Why (required).
        note: String,
    },
    /// The head of a kind released under a tag.
    Release {
        /// The kind.
        kind: String,
        /// The tag.
        tag: String,
        /// Notes.
        #[serde(default)]
        notes: String,
    },
    /// An entry withdrawn (kept for who uses it; no new use).
    Withdraw {
        /// The kind.
        kind: String,
        /// The file.
        file: String,
        /// The entry.
        entry: String,
        /// Why.
        #[serde(default)]
        note: String,
    },
    /// A withdrawn entry back.
    Restore {
        /// The kind.
        kind: String,
        /// The file.
        file: String,
        /// The entry.
        entry: String,
    },
}

impl LibraryAction {
    /// In words.
    pub fn describe(&self) -> String {
        match self {
            LibraryAction::Lock {
                kind, file, locked, ..
            } => format!(
                "{} {}{}",
                if *locked { "lock" } else { "unlock" },
                kind,
                file.as_deref().map_or(String::new(), |f| format!("/{f}"))
            ),
            LibraryAction::Grant { id, .. } => format!("grant request #{id}"),
            LibraryAction::Refuse { id, .. } => format!("refuse request #{id}"),
            LibraryAction::Release { kind, tag, .. } => format!("release {kind} as {tag}"),
            LibraryAction::Withdraw { kind, entry, .. } => format!("withdraw {kind} {entry}"),
            LibraryAction::Restore { kind, entry, .. } => format!("restore {kind} {entry}"),
        }
    }
}

impl AdminAsk {
    /// In words (the log, refusals).
    pub fn describe(&self) -> String {
        let v = serde_json::to_value(self).unwrap_or_default();
        v.get("ask")
            .and_then(|a| a.as_str())
            .unwrap_or("admin")
            .replace('_', " ")
    }
}

/// An admin message's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", content = "data", rename_all = "snake_case")]
pub enum AdminReply {
    /// [`AdminAsk::Overview`].
    Overview(Box<Overview>),
    /// [`AdminAsk::Sessions`].
    Sessions(Vec<SessionRow>),
    /// A change done, in words.
    Done(String),
    /// [`AdminAsk::Log`].
    Log(LogPage),
    /// [`AdminAsk::Backups`].
    Backups(BackupList),
    /// [`AdminAsk::Settings`].
    Settings(Vec<SettingRow>),
    /// [`AdminAsk::Rights`].
    Rights(Box<RightsView>),
    /// [`AdminAsk::Preview`].
    Preview(Box<WhatChanges>),
    /// [`AdminAsk::Invite`], [`AdminAsk::Reissue`].
    Invitation(InvitationText),
    /// [`AdminAsk::SeeAs`].
    SeeAs(Box<SeeAsView>),
    /// [`AdminAsk::Map`].
    Map(Box<AdminMap>),
    /// [`AdminAsk::Structures`], [`AdminAsk::RouteStructures`].
    Structures(Vec<MapDot>),
    /// [`AdminAsk::Libraries`].
    Libraries(Vec<LibraryKindRow>),
    /// [`AdminAsk::SharedTowers`].
    SharedTowers(SharedList),
    /// [`AdminAsk::SharedTower`].
    SharedTower(Box<SharedDetail>),
    /// [`AdminAsk::Twins`].
    Twins(Vec<TwinRow>),
    /// [`AdminAsk::LibraryPage`].
    LibraryPage(Box<LibraryPage>),
    /// [`AdminAsk::Uses`].
    Uses(Vec<UseRow>),
    /// [`AdminAsk::PinsPlan`].
    Pins(Box<PinsView>),
    /// [`AdminAsk::Lines`].
    Lines(Vec<LineRow>),
    /// [`AdminAsk::ExportLine`], [`AdminAsk::UploadStart`].
    Transfer(TransferInfo),
    /// [`AdminAsk::Download`].
    Chunk(ChunkData),
    /// [`AdminAsk::Upload`]: the bytes received so far.
    Received(u64),
    /// [`AdminAsk::ImportLine`].
    Imported(Box<ImportView>),
    /// [`AdminAsk::Audit`].
    Audit(AuditPage),
    /// [`AdminAsk::Identities`].
    Identities(Vec<IdentityRow>),
    /// [`AdminAsk::Explain`].
    Explain(ExplainView),
}

/// A route of a shared tower.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedRoute {
    /// Its id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its results are out of date: why (phase 8: a shared tower moved, a
    /// library updated, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<String>,
}

/// A shared tower: a structure on two routes or more.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedRow {
    /// Its line.
    pub model: String,
    /// Its id.
    pub id: String,
    /// Where (plan, m).
    pub at: [f64; 2],
    /// Its routes (those the asker may see).
    pub routes: Vec<SharedRoute>,
}

/// The shared towers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SharedList {
    /// The towers (at most [`SHARED_MAX`]).
    pub rows: Vec<SharedRow>,
    /// Shared towers the asker may see, in all.
    pub total: usize,
    /// Of them, on a route whose results are out of date.
    pub stale: usize,
}

/// One shared tower in full.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedDetail {
    /// The tower.
    pub row: SharedRow,
    /// Who may edit it (active users; none: not shown — the asker does
    /// not manage rights on its line).
    pub editors: Option<Vec<String>>,
}

/// A twin tower: a structure of a line within a distance of another
/// line's of the same type (shown, never merged).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwinRow {
    /// The line.
    pub line: String,
    /// Its structure.
    pub structure: String,
    /// The other line.
    pub other_line: String,
    /// Its structure.
    pub other: String,
    /// Plan distance, m.
    pub distance_m: f64,
    /// Their structure type (none: modelled point by point).
    pub structure_type: Option<String>,
}

/// A library entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryRow {
    /// Its id.
    pub id: String,
    /// Its version (short).
    pub version: String,
    /// The release it first came in with this version (none: not
    /// released yet — a draft of the head).
    pub release: Option<String>,
    /// Withdrawn.
    pub withdrawn: bool,
}

/// A file of a library kind's head.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryFileView {
    /// Its name.
    pub file: String,
    /// Locked on its own.
    pub locked: bool,
    /// Its entries the asker may see.
    pub entries: Vec<EntryRow>,
}

/// A library kind as the Libraries page shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryKindView {
    /// The kind.
    pub kind: String,
    /// Locked.
    pub locked: bool,
    /// Its maintainers (`user:…`, `group:…`).
    pub maintainers: Vec<String>,
    /// Its releases, oldest first.
    pub releases: Vec<String>,
    /// Its files.
    pub files: Vec<LibraryFileView>,
    /// The asker maintains it (or is a network admin): its maintainers'
    /// buttons are shown (the store still decides each change).
    pub maintain: bool,
}

/// A request for a library addition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestRow {
    /// Its number.
    pub id: i64,
    /// The kind.
    pub kind: String,
    /// The file it goes into.
    pub file: String,
    /// The entry.
    pub entry: String,
    /// draft, requested, granted, refused, withdrawn.
    pub state: String,
    /// Who asked.
    pub user: String,
    /// The line it is for.
    pub line: Option<String>,
    /// Why.
    pub reason: String,
    /// Who decided.
    pub maintainer: Option<String>,
    /// The decision's note.
    pub note: Option<String>,
    /// When asked.
    pub created: String,
    /// When decided.
    pub decided: Option<String>,
    /// The release it came in.
    pub released: Option<String>,
}

/// A line's pin of one kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinRow {
    /// The kind.
    pub kind: String,
    /// Its pin (none: the line's own files).
    pub pinned: Option<String>,
    /// The latest release.
    pub latest: Option<String>,
    /// Releases since its pin.
    pub behind: usize,
    /// Withdrawn entries it uses.
    pub withdrawn_used: Vec<String>,
}

/// A line's library pins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinePins {
    /// The line.
    pub line: String,
    /// Built (pinned; moved only by who may review the whole line).
    pub built: bool,
    /// Per kind.
    pub kinds: Vec<PinRow>,
}

/// The Libraries page.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LibraryPage {
    /// The kinds the asker may see.
    pub kinds: Vec<LibraryKindView>,
    /// The requests the asker may see (their own, those of what they
    /// maintain).
    pub requests: Vec<RequestRow>,
    /// The lines' pins (which lines pin which release).
    pub lines: Vec<LinePins>,
}

/// A use of a library entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UseRow {
    /// The line.
    pub line: String,
    /// The branch.
    pub branch: String,
    /// The object's kind (`library`: the line holds the file).
    pub object_kind: String,
    /// The object.
    pub object: String,
    /// Its routes the asker may see.
    pub routes: Vec<String>,
}

/// What Update library pins does to one kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinKindPlan {
    /// The kind.
    pub kind: String,
    /// From (none: the line's own files).
    pub from: Option<String>,
    /// To.
    pub to: String,
    /// Library files changed.
    pub files: usize,
    /// Entries whose version changed.
    pub changed: Vec<String>,
    /// Of them, used by the line.
    pub used_changed: Vec<String>,
    /// Routes whose results go out of date.
    pub stale_routes: Vec<String>,
    /// The line's own files kept.
    pub kept: Vec<String>,
    /// Withdrawn entries the line uses.
    pub withdrawn_used: Vec<String>,
    /// Why it does not move.
    pub refused: Option<String>,
}

/// What Update library pins would do (a proposal: nothing changed).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PinsView {
    /// The line.
    pub line: String,
    /// Built.
    pub built: bool,
    /// Per kind.
    pub kinds: Vec<PinKindPlan>,
    /// Anything moves.
    pub moves: bool,
    /// The asker may not carry it out: why.
    pub refusal: Option<String>,
    /// In words.
    pub text: String,
}

/// A line on the Lines page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineRow {
    /// Its code.
    pub code: String,
    /// Its name.
    pub name: String,
    /// Routes, structures, sections held.
    pub routes: usize,
    /// See `routes`.
    pub structures: usize,
    /// See `routes`.
    pub sections: usize,
    /// Routes whose results are out of date.
    pub stale: usize,
    /// The asker sees it whole.
    pub whole: bool,
    /// The asker may export it (export on the whole line).
    pub may_export: bool,
}

/// A file the server holds for one connection (an export to fetch, an
/// upload to import).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferInfo {
    /// Its token (random; this connection's only).
    pub token: String,
    /// Its file name.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
    /// What it holds, in words.
    pub note: String,
}

/// A piece of a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkData {
    /// Its first byte.
    pub offset: u64,
    /// The bytes, base64.
    pub data: String,
    /// The file's last piece.
    pub last: bool,
}

/// What an import did (the network import's report).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportView {
    /// The new line.
    pub line: String,
    /// Its name.
    pub name: String,
    /// The file's name.
    pub file: String,
    /// Objects imported, by kind.
    pub objects: Vec<(String, usize)>,
    /// Ids renamed (kind, in the file, in the network).
    pub renamed: Vec<(String, String, String)>,
    /// Twin towers listed (never merged).
    pub twins: Vec<TwinRow>,
    /// Users of the file (name, role on the line).
    pub users: Vec<(String, String)>,
    /// What stayed in the file.
    pub left_in_file: Vec<String>,
    /// The report in words (as `ohl network import` prints it).
    pub text: String,
    /// Seconds on the server.
    pub seconds: f64,
}

/// Which audit rows (every field: empty, no constraint).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFilter {
    /// Who (the user, exactly).
    #[serde(default)]
    pub user: String,
    /// What (the action, from its start: `admin.`, `library.`, …).
    #[serde(default)]
    pub action: String,
    /// The scope (the target holding this text: `line L001`, `user:bob`).
    #[serde(default)]
    pub target: String,
    /// Any text of its detail.
    #[serde(default)]
    pub text: String,
    /// From (UTC, `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`).
    #[serde(default)]
    pub from: String,
    /// Until (UTC, as `from`; a day: to its end).
    #[serde(default)]
    pub until: String,
}

/// An audit row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRow {
    /// Its number.
    pub id: i64,
    /// When (UTC).
    pub time: String,
    /// Who.
    pub user: String,
    /// What.
    pub action: String,
    /// On what.
    pub target: String,
    /// The detail.
    pub detail: String,
}

/// A page of the audit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditPage {
    /// The rows, newest first.
    pub rows: Vec<AuditRow>,
    /// Rows matching the filter, in all.
    pub total: u64,
    /// More rows older than the last (ask with `before` its number).
    pub more: bool,
}

/// An identity of the network's identity provider and the user it signs
/// in as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityRow {
    /// The issuer.
    pub issuer: String,
    /// The subject.
    pub subject: String,
    /// The network user.
    pub user: String,
}

/// Why a user may or may not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplainView {
    /// The user.
    pub user: String,
    /// The permission.
    pub permission: Permission,
    /// The target, in words.
    pub target: String,
    /// They may.
    pub may: bool,
    /// The assignments granting it there.
    pub granted_by: Vec<i64>,
    /// The assignments denying it there.
    pub denied_by: Vec<i64>,
    /// More, in words (not active; see not given).
    pub notes: Vec<String>,
}

/// Audit rows as CSV (a header, then a row each; fields quoted where
/// needed, a formula's leading `=`, `+`, `-` or `@` kept as text).
pub fn audit_csv(rows: &[AuditRow], header: bool) -> String {
    fn field(s: &str) -> String {
        // A spreadsheet takes `=…` as a formula: kept as text.
        let s = if s.starts_with(['=', '+', '-', '@']) {
            format!("'{s}")
        } else {
            s.to_owned()
        };
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s
        }
    }
    let mut out = String::new();
    if header {
        out.push_str("id,time,user,action,target,detail\n");
    }
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            r.id,
            field(&r.time),
            field(&r.user),
            field(&r.action),
            field(&r.target),
            field(&r.detail)
        ));
    }
    out
}

/// The server at a glance.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    /// The asker is a network admin: everything below is filled in (else
    /// only the version, the protocol, the uptime and counts of what they
    /// manage).
    pub full: bool,
    /// The server program's version.
    pub version: String,
    /// The network's schema (the database's).
    pub schema: i32,
    /// The schema this server program writes.
    pub program_schema: i32,
    /// The protocol.
    pub protocol: u32,
    /// The admin messages' version.
    pub admin: u32,
    /// Seconds since the server started.
    pub uptime_s: u64,
    /// The network's name.
    pub network: String,
    /// Lines (models), routes and structures held.
    pub models: usize,
    /// See `models`.
    pub routes: usize,
    /// See `models`.
    pub structures: usize,
    /// The users connected (network admins; others: empty).
    pub connected: Vec<String>,
    /// Connections open.
    pub connections: usize,
    /// Connections following a working set.
    pub working_sets: usize,
    /// Checks, exports and bundles waiting.
    pub queue_waiting: usize,
    /// A check runs on the checks' worker.
    pub check_running: bool,
    /// An export or a bundle is being made on the exports' thread.
    pub export_running: bool,
    /// The last backup's time (UTC), if one is recorded.
    pub last_backup: Option<String>,
    /// Its age, hours.
    pub backup_age_h: Option<f64>,
    /// The age a migration accepts, hours (the network's setting).
    pub backup_max_age_h: f64,
    /// Routes whose results are out of date.
    pub stale_routes: usize,
    /// What needs a look: a backup older than 24 h, migrations pending,
    /// stale results.
    pub warnings: Vec<String>,
}

/// A connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRow {
    /// The connection's number (for [`AdminAsk::Disconnect`]).
    pub client: u64,
    /// The user.
    pub user: String,
    /// Connected since (Unix time, s).
    pub since: u64,
    /// What it follows, in words.
    pub follows: String,
    /// An admin seeing as this user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub see_as: Option<String>,
}

/// A line of the server's log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogLine {
    /// Its number (counted from the program's start).
    pub n: u64,
    /// When (Unix time, s).
    pub time: u64,
    /// The text.
    pub text: String,
}

/// Lines of the server's log.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LogPage {
    /// The lines (oldest first).
    pub lines: Vec<LogLine>,
    /// The last line's number of the log (filtered or not).
    pub last: u64,
}

/// A backup of the backup folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupRow {
    /// Its folder's name.
    pub name: String,
    /// The time of its data (UTC).
    pub time: String,
    /// Its network schema.
    pub schema: i32,
    /// The dump's size, bytes.
    pub bytes: u64,
    /// Who made it.
    pub user: String,
    /// Lines in it.
    pub models: usize,
}

/// The backups.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BackupList {
    /// The server has a backup folder (else backups are made with `ohl
    /// network backup` beside it).
    pub folder: bool,
    /// The server has a drill database (the full restore drill).
    pub drill_database: bool,
    /// The backups, newest first.
    pub rows: Vec<BackupRow>,
    /// The last backup recorded in the network (UTC), and its age, hours.
    pub last: Option<String>,
    /// See `last`.
    pub age_h: Option<f64>,
}

/// A setting, read-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingRow {
    /// Its name.
    pub key: String,
    /// Its value.
    pub value: String,
    /// Where it comes from: `start` (the server's start options: its
    /// command line or configuration — changed by starting it again) or
    /// `network` (the network's own setting, `ohl network setting`).
    pub source: String,
    /// What it is.
    pub note: String,
}

/// A user, as the rights pages show them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserRow {
    /// Name.
    pub name: String,
    /// Not disabled.
    pub active: bool,
    /// Groups they are a member of.
    pub groups: Vec<String>,
    /// A network admin.
    pub network_admin: bool,
    /// Connected now.
    pub connected: bool,
}

/// A role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleRow {
    /// Name.
    pub name: String,
    /// Its permissions.
    pub permissions: Vec<Permission>,
    /// Shipped (read-only).
    pub shipped: bool,
}

/// The network's rights the asker manages.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RightsView {
    /// The asker.
    pub me: String,
    /// The asker is a network admin (users, groups, roles and denies are
    /// theirs to change).
    pub network_admin: bool,
    /// The lines the asker manages rights on (a network admin: every
    /// line).
    pub manages_lines: Vec<String>,
    /// The library kinds the asker manages rights on.
    pub manages_libraries: Vec<String>,
    /// Users (every user: an assignment names one).
    pub users: Vec<UserRow>,
    /// Groups and their members.
    pub groups: Vec<(String, Vec<String>)>,
    /// Roles (every role: an assignment names one).
    pub roles: Vec<RoleRow>,
    /// The assignments whose scope the asker manages.
    pub assignments: Vec<Assignment>,
    /// The rights' revision.
    pub rev: i64,
}

/// One user's change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserChange {
    /// The user.
    pub user: String,
    /// What they gain: `permission on scope`.
    pub gained: Vec<String>,
    /// What they lose.
    pub lost: Vec<String>,
}

/// What a rights change would change.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WhatChanges {
    /// Each user whose rights change.
    pub users: Vec<UserChange>,
    /// Users gaining something.
    pub gaining: usize,
    /// Users losing something.
    pub losing: usize,
    /// Permissions gained, in all (permission × scope × user).
    pub gained: usize,
    /// Permissions lost, in all.
    pub lost: usize,
    /// Anything else the change does, in words.
    pub notes: Vec<String>,
}

/// A personal invitation's access code, for the admin to pass on (the app
/// puts it into the invitation it joined with — the same server's address
/// and certificate; shown once, kept in memory only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvitationText {
    /// The user.
    pub user: String,
    /// The access code.
    pub code: String,
    /// The last day it is valid (none: until reissued).
    pub until: Option<String>,
}

/// What a user sees of one line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeeLine {
    /// The line.
    pub line: String,
    /// The whole line.
    pub whole: bool,
    /// Objects seen and in all, by kind.
    pub counts: Vec<(String, usize, usize)>,
}

/// What a user sees ("see as": what both they and the asker may).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeeAsView {
    /// The user.
    pub user: String,
    /// The lines with something seen.
    pub lines: Vec<SeeLine>,
    /// The routes seen (by id).
    pub routes: Vec<String>,
}

/// A route on the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapRoute {
    /// Its id (unique in the network).
    pub id: String,
    /// Its line (model).
    pub model: String,
    /// Its name.
    pub name: String,
    /// The highest nominal voltage of its line's systems, kV.
    pub voltage_kv: Option<f64>,
    /// Its line's systems' owner (the first given; empty: none).
    pub owner: String,
    /// Its alignments' polylines, simplified (plan, m).
    pub lines: Vec<Vec<[f64; 2]>>,
    /// Structures on it.
    pub structures: usize,
    /// Its results are out of date: why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<String>,
    /// Users with a working set on it now.
    pub in_use: Vec<String>,
    /// Soft locks held on its line.
    pub locks: usize,
}

/// The map.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AdminMap {
    /// The network's coordinate system.
    pub crs: String,
    /// Plan bounds of what is shown (x0, y0, x1, y1).
    pub bounds: Option<[f64; 4]>,
    /// The lines (models): code, name.
    pub lines: Vec<(String, String)>,
    /// The routes.
    pub routes: Vec<MapRoute>,
    /// Shared towers (two routes or more): where.
    pub shared: Vec<[f64; 2]>,
}

/// A structure on the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapDot {
    /// Its line.
    pub model: String,
    /// Its id.
    pub id: String,
    /// Where (plan, m).
    pub at: [f64; 2],
    /// On two routes or more.
    pub shared: bool,
}

/// A library file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryFileRow {
    /// Its name.
    pub file: String,
    /// Locked.
    pub locked: bool,
    /// Its entries (ids) the asker may see.
    pub entries: Vec<String>,
}

/// A library kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryKindRow {
    /// The kind (`cables`, …).
    pub kind: String,
    /// Locked.
    pub locked: bool,
    /// Its maintainers (`user:…`, `group:…`).
    pub maintainers: Vec<String>,
    /// Its files.
    pub files: Vec<LibraryFileRow>,
}

/// Unix time `t` as `YYYYMMDD-HHMMSS` (UTC; a backup folder's name).
pub fn utc_stamp(t: u64) -> String {
    let d = crate::ymd((t / 86_400) as i64);
    let s = t % 86_400;
    format!("{d}-{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Unix time `t` as `YYYY-MM-DD HH:MM` (UTC).
pub fn utc_text(t: u64) -> String {
    let d = crate::ymd((t / 86_400) as i64);
    let s = t % 86_400;
    format!(
        "{} {:02}:{:02} UTC",
        crate::sign_in::day_text(d),
        s / 3600,
        s / 60 % 60
    )
}

/// Now, Unix time, s.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The server's log as its admins read it: the newest [`LOG_LINES`] lines
/// of this program (what the server writes to its standard error, nothing
/// more: no secret is ever logged), numbered.
struct Kept {
    next: u64,
    lines: std::collections::VecDeque<LogLine>,
}

static KEPT: std::sync::Mutex<Kept> = std::sync::Mutex::new(Kept {
    next: 1,
    lines: std::collections::VecDeque::new(),
});

/// Keeps a line of the server's log.
pub(crate) fn keep_log(text: &str) {
    let mut k = KEPT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let n = k.next;
    k.next += 1;
    if k.lines.len() >= LOG_LINES {
        k.lines.pop_front();
    }
    // A line is at most 2000 characters (a long refusal cut).
    let text: String = text.chars().take(2000).collect();
    k.lines.push_back(LogLine {
        n,
        time: unix_now(),
        text,
    });
}

/// The kept log lines after `after` holding `filter` (any case), at most
/// 1000 (the oldest of them first).
pub fn log_page(after: u64, filter: &str) -> LogPage {
    let k = KEPT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let f = filter.to_lowercase();
    let lines: Vec<LogLine> = k
        .lines
        .iter()
        .filter(|l| l.n > after && (f.is_empty() || l.text.to_lowercase().contains(&f)))
        .take(1000)
        .cloned()
        .collect();
    LogPage {
        // Cut at 1000: the last line given; else every kept line was seen.
        last: match lines.last() {
            Some(l) if lines.len() == 1000 => l.n,
            _ => after.max(k.next - 1),
        },
        lines,
    }
}

/// The change applied to a copy of the rights (for "what changes"; the
/// server's own function checks and saves it). Commands that change no
/// rights (invitations, identities) leave the copy as it is.
pub fn apply_copy(data: &RightsData, command: &RightsCommand) -> RightsData {
    let mut d = data.clone();
    match command {
        RightsCommand::Bootstrap { user } => {
            d.users.insert(user.clone(), true);
            let id = d.assignments.iter().map(|a| a.id).max().unwrap_or(0) + 1;
            d.assignments.push(Assignment {
                id,
                subject: Subject::User(user.clone()),
                role: Some("network_admin".into()),
                deny: false,
                scope: ohl_session::rights::Scope::network(),
                by: user.clone(),
                note: String::new(),
            });
        }
        RightsCommand::AddUser { name, .. } => {
            d.users.entry(name.clone()).or_insert(true);
        }
        RightsCommand::SetActive { name, active } => {
            if let Some(a) = d.users.get_mut(name) {
                *a = *active;
            }
        }
        RightsCommand::AddGroup { name, .. } => {
            d.groups.entry(name.clone()).or_default();
        }
        RightsCommand::DeleteGroup { name } => {
            d.groups.remove(name);
            d.assignments
                .retain(|a| a.subject != Subject::Group(name.clone()));
        }
        RightsCommand::AddMember { group, user } => {
            if let Some(m) = d.groups.get_mut(group) {
                m.insert(user.clone());
            }
        }
        RightsCommand::RemoveMember { group, user } => {
            if let Some(m) = d.groups.get_mut(group) {
                m.remove(user);
            }
        }
        RightsCommand::PutRole {
            name, permissions, ..
        } => {
            let shipped = d.roles.get(name).is_some_and(|(_, s)| *s);
            if !shipped {
                d.roles.insert(
                    name.clone(),
                    (ohl_session::rights::Perms::of(permissions), false),
                );
            }
        }
        RightsCommand::DeleteRole { name } => {
            let used = d.assignments.iter().any(|a| a.role.as_ref() == Some(name));
            if !used && !d.roles.get(name).is_some_and(|(_, s)| *s) {
                d.roles.remove(name);
            }
        }
        RightsCommand::Assign {
            subject,
            role,
            deny,
            scope,
            note,
        } => {
            let id = d.assignments.iter().map(|a| a.id).max().unwrap_or(0) + 1;
            d.assignments.push(Assignment {
                id,
                subject: subject.clone(),
                role: role.clone(),
                deny: *deny,
                scope: scope.clone(),
                by: String::new(),
                note: note.clone(),
            });
        }
        RightsCommand::Unassign { id } => d.assignments.retain(|a| a.id != *id),
        RightsCommand::ReissueInvitation { .. }
        | RightsCommand::MapIdentity { .. }
        | RightsCommand::UnmapIdentity { .. } => {}
    }
    d
}

/// What `user` holds by their assignments: `permission on scope`, granted
/// (`+`) and denied (`-`); nothing when they are not active.
fn holdings(d: &RightsData, user: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if !d.active(user) {
        return out;
    }
    for a in d.assignments_of(user) {
        let perms = match &a.role {
            Some(r) => match d.roles.get(r) {
                Some((p, _)) => p.list(),
                None => continue,
            },
            None => Permission::ALL.to_vec(),
        };
        let scope = a.scope.describe();
        for p in perms {
            out.insert(format!(
                "{}{} on {scope}",
                if a.deny { "denied " } else { "" },
                p.as_str()
            ));
        }
    }
    out
}

/// What `command` changes of `data`: who gains and who loses which
/// permission on which scope (a deny gained is listed as gained `denied
/// …`), counted. Nothing is saved; the server's own function still decides
/// when it is saved.
pub fn what_changes(data: &RightsData, command: &RightsCommand) -> WhatChanges {
    let after = apply_copy(data, command);
    let users: BTreeSet<&String> = data.users.keys().chain(after.users.keys()).collect();
    let mut out = WhatChanges::default();
    for u in users {
        let (b, a) = (holdings(data, u), holdings(&after, u));
        let gained: Vec<String> = a.difference(&b).cloned().collect();
        let lost: Vec<String> = b.difference(&a).cloned().collect();
        if gained.is_empty() && lost.is_empty() {
            continue;
        }
        out.gaining += usize::from(!gained.is_empty());
        out.losing += usize::from(!lost.is_empty());
        out.gained += gained.len();
        out.lost += lost.len();
        out.users.push(UserChange {
            user: u.clone(),
            gained,
            lost,
        });
    }
    match command {
        RightsCommand::AddUser { name, .. } if !data.users.contains_key(name) => out.notes.push(
            format!("{name} becomes a user of the network (no rights yet)"),
        ),
        RightsCommand::ReissueInvitation { name } => out.notes.push(format!(
            "every older invitation of {name} is refused from now on; {name} is let go if \
             connected"
        )),
        RightsCommand::MapIdentity { user, .. } => {
            out.notes.push(format!("the identity signs in as {user}"));
        }
        RightsCommand::UnmapIdentity { .. } => {
            out.notes.push("the identity no longer signs in".into());
        }
        RightsCommand::PutRole { name, .. } if data.roles.get(name).is_some_and(|(_, s)| *s) => {
            out.notes
                .push(format!("{name} is a shipped role: it cannot be changed"));
        }
        _ => {}
    }
    if out.users.is_empty() && out.notes.is_empty() {
        out.notes.push("nobody's rights change".into());
    }
    out
}

/// Counts of what a [`WhatChanges`] says, in one sentence.
pub fn what_changes_words(w: &WhatChanges) -> String {
    format!(
        "{} user(s) gain {} permission(s), {} user(s) lose {}",
        w.gaining, w.gained, w.losing, w.lost
    )
}

/// The assignments of `data` grouped by subject (the rights pages).
pub fn by_subject(data: &[Assignment]) -> BTreeMap<String, Vec<&Assignment>> {
    let mut out: BTreeMap<String, Vec<&Assignment>> = BTreeMap::new();
    for a in data {
        out.entry(a.subject.to_string()).or_default().push(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_session::rights::Scope;

    fn data() -> RightsData {
        let mut d = RightsData::shipped();
        for u in ["admin", "bob", "vic"] {
            d.users.insert(u.into(), true);
        }
        d.assignments.push(Assignment {
            id: 1,
            subject: Subject::User("admin".into()),
            role: Some("network_admin".into()),
            deny: false,
            scope: Scope::network(),
            by: "admin".into(),
            note: String::new(),
        });
        d.groups.insert("east".into(), ["bob".to_owned()].into());
        d
    }

    #[test]
    fn an_assignment_to_a_group_counts_its_members() {
        let d = data();
        let w = what_changes(
            &d,
            &RightsCommand::Assign {
                subject: Subject::Group("east".into()),
                role: Some("viewer".into()),
                deny: false,
                scope: Scope::line("L001"),
                note: String::new(),
            },
        );
        assert_eq!(w.gaining, 1);
        assert_eq!(w.losing, 0);
        assert_eq!(w.users[0].user, "bob");
        assert!(w.users[0].gained.iter().all(|g| g.ends_with("line L001")));
        assert!(w.gained >= 1);
    }

    #[test]
    fn disabling_loses_everything_and_unassigning_undoes() {
        let d = data();
        let w = what_changes(
            &d,
            &RightsCommand::SetActive {
                name: "admin".into(),
                active: false,
            },
        );
        assert_eq!(w.losing, 1);
        assert_eq!(w.lost, Permission::ALL.len());
        let w = what_changes(&d, &RightsCommand::Unassign { id: 1 });
        assert_eq!(w.users[0].user, "admin");
        let w = what_changes(
            &d,
            &RightsCommand::AddMember {
                group: "east".into(),
                user: "vic".into(),
            },
        );
        // East holds nothing: nobody gains.
        assert!(w.users.is_empty());
        assert_eq!(w.notes, vec!["nobody's rights change".to_owned()]);
    }

    #[test]
    fn the_messages_go_and_come_back() {
        let ask = AdminAsk::Change {
            command: RightsCommand::Unassign { id: 3 },
        };
        let t = serde_json::to_string(&ask).unwrap();
        assert_eq!(serde_json::from_str::<AdminAsk>(&t).unwrap(), ask);
        assert_eq!(ask.describe(), "change");
        keep_log("a line for the admins");
        let p = log_page(0, "FOR THE ADMINS");
        assert!(p.lines.iter().any(|l| l.text == "a line for the admins"));
        let r = AdminReply::Done("ok".into());
        let t = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<AdminReply>(&t).unwrap(), r);
    }
}
