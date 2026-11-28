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
/// 0: a server of before them).
pub const ADMIN: u32 = 1;

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
