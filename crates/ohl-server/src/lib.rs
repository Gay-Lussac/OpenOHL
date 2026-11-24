// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Server mode (ADR-0005, alpha): a project hosted for several users.
//!
//! The **server** ([`server`]) owns the project's session: it alone
//! validates and applies edits, in one order, and broadcasts each applied
//! operation ([`Op`]) with its log sequence number. A **client**
//! ([`client`]) joins with an [`Invitation`] (address, access code, the
//! server certificate's fingerprint), receives a consistent copy of the
//! project file, and keeps it as a **replica**: it applies the broadcast
//! operations, in order, with the same session code ([`execute`]), so its
//! line, log and sequence numbers stay those of the server — per-user undo
//! and admin rollback mean the same everywhere. A replica that disagrees
//! (another sequence number) is out of sync and must join again.
//!
//! Edits are requests: the server answers after broadcasting what it
//! applied, or refuses (validation, role, an object another user holds).
//! Who may join: a user the project knows only with their **personal**
//! invitation (its code derived from the server's secret and the name);
//! the general invitation lets in only names the project does not know,
//! with the newcomers' role — nobody joins under someone else's name
//! without that person's invitation.
//!
//! Users see each other ([`Presence`]: selection, camera, what they edit)
//! and hold **soft locks** on what they are editing; the server refuses
//! others' commands on a locked object.
//!
//! Transport: QUIC (`quinn`), TLS with a certificate made when the server
//! starts; clients trust it by its fingerprint in the invitation. Messages
//! are JSON, length-prefixed; the project file goes on its own stream.
//!
//! Browsers join through the optional web listener ([`web`]): HTTPS with
//! the same certificate, the web client's files, and a WebSocket speaking
//! the same messages. A web client keeps no replica: instead of the
//! project file it receives a [`Scene`] (the line and its wires solved on
//! the server) whenever the line changes.
//!
//! **A server on a network** (feature `network`, ADR-0013 phase 5:
//! [`network`]) hosts every model of a country server's database instead
//! of one project file: it holds each model's line in memory, writes each
//! command to PostgreSQL, and serves its clients **by interest**
//! ([`stream`]): a client follows whole models (a replica of one), routes
//! or tiles of the network's grid, receives [`Change`]s touching them, tile
//! packages of the context ([`TilePackage`]) and a heartbeat, and catches
//! up after a disconnect from its [`Position`]. **Protocol 3**: clients of
//! protocol 2 (the alpha) are refused with a message saying so; a file
//! server speaks it too, its messages unchanged.

pub mod base64;
pub mod client;
#[cfg(feature = "network")]
#[doc(hidden)]
pub mod fake_provider;
#[cfg(feature = "network")]
pub mod network;
#[cfg(feature = "network")]
pub(crate) mod network_rights;
#[cfg(feature = "offline-checks")]
pub mod offline_checks;
#[cfg(feature = "network")]
pub mod oidc;
#[cfg(feature = "offline-checks")]
pub(crate) mod packager;
#[cfg(feature = "offline-checks")]
pub(crate) mod scoped_report;
pub mod server;
pub mod sign_in;
pub mod stream;
pub mod web;

pub use stream::{
    Change, ContextLabel, ContextRoute, ContextStructure, ContextWire, HeldTile, ModelInfo,
    NetworkInfo, Position, RouteInfo, TileKey, TilePackage,
};

use std::collections::BTreeMap;
use std::fmt;

use ohl_session::collab::Grant;
use ohl_session::versions::Choices;
use ohl_session::working_set::{ExtentSummary, Pick, RouteDetail, Rows, WorkingSetSpec};
use ohl_session::{Applied, Command, Kind, Role, Session, SessionError};
use serde::{Deserialize, Serialize};

/// Version of the protocol; client and server must agree. 3 since
/// 2026-11-19 (a server on a network: the stream by interest, tiles,
/// heartbeats, catch-up; the messages of 2 unchanged).
pub const PROTOCOL: u32 = 3;

/// The refusal of a client speaking another protocol.
pub fn protocol_refusal(protocol: u32) -> String {
    if protocol < PROTOCOL {
        format!(
            "this program speaks protocol {protocol}, the server protocol {PROTOCOL}: update \
             OpenOHL to join (programs of protocol {protocol} cannot join this server)"
        )
    } else {
        format!(
            "this program speaks protocol {protocol}, the server protocol {PROTOCOL}: the \
             server must be updated to let it join"
        )
    }
}

/// ALPN of the protocol (its name of before 2026-10-18, kept: an older
/// client and a newer server still agree on it).
pub const ALPN: &[u8] = b"ols/2";

/// Largest message, bytes (an exit clause against garbage).
pub const MAX_MESSAGE: usize = 64 << 20;

/// Phase 13: a message larger than this takes its size from the
/// server's budget of large messages being read at once
/// ([`server::LARGE_BUDGET`]) before it is read.
pub const LARGE_MESSAGE: usize = 1 << 20;

/// Phase 13: the largest message of a web client after its join (a
/// viewer: interests, catch-ups, edits — no bundles), bytes.
pub const MAX_WEB_MESSAGE: usize = 8 << 20;

/// Largest hello (before the access code is checked), bytes.
pub const MAX_HELLO: usize = 16 << 10;

/// Longest user name, characters.
pub const MAX_NAME: usize = 64;

/// Longest presence selection or lock target, bytes.
pub const MAX_LABEL: usize = 1024;

/// An edit of the project, as requested and as broadcast, on the project
/// line its user works on ([`ohl_session::lines`]). Executed by the server
/// and then by every replica, in the server's order ([`execute`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// A session command.
    Do {
        /// The command.
        command: Command,
    },
    /// Undo the user's last command.
    Undo,
    /// Redo the user's last undo.
    Redo,
    /// Admin: reverse one log entry.
    Rollback {
        /// The entry.
        seq: i64,
    },
    /// Admin: back to the state just after log entry `seq`.
    RollbackAfter {
        /// The entry.
        seq: i64,
    },
    /// Admin (or the server for a newcomer): give a user a role.
    SetUser {
        /// User.
        name: String,
        /// Role.
        role: Role,
    },
    /// Keep a library file in the project.
    PutLibrary {
        /// Kind (`cables`, `structures`…).
        kind: String,
        /// File name.
        name: String,
        /// Text.
        text: String,
    },
    /// Remove a library file from the project.
    DeleteLibrary {
        /// Kind.
        kind: String,
        /// File name.
        name: String,
    },
    /// Start a project line from another (or from a revision or tag).
    CreateLine {
        /// Name.
        name: String,
        /// Line, revision or tag it starts from.
        from: String,
    },
    /// Delete a project line (its creator or an admin; the main line an
    /// admin).
    DeleteLine {
        /// Name.
        name: String,
    },
    /// Rename a project line (as deleting).
    RenameLine {
        /// Name.
        from: String,
        /// New name.
        to: String,
    },
    /// Describe a project line (its creator or an admin; the main line an
    /// admin).
    SetLineDescription {
        /// The line.
        line: String,
        /// Its description.
        description: String,
    },
    /// Admin: roll a project line back to a revision (recorded as a new
    /// revision; nothing is lost).
    RollBackLine {
        /// The line.
        line: String,
        /// Revision, branch or tag.
        revision: String,
    },
    /// Bring the changes of another project line into this one.
    BringIn {
        /// The line whose changes come in.
        from: String,
        /// Conflicts settled.
        #[serde(default)]
        choices: Choices,
    },
    /// Set (or remove) a project setting.
    SetSetting {
        /// Key.
        key: String,
        /// Value (`None`: removed).
        value: Option<String>,
    },
    /// Record the working state as a revision on the current branch.
    Commit {
        /// Message.
        message: String,
    },
    /// A **branch over a working set** (a network's server, phase 11):
    /// project line `name` started from project line `from`, working on
    /// `routes` — the rules of shared towers apply to its commands with
    /// them as the working set, its changes reach the other routes when it
    /// is merged — shared with `team` (users who work on it beside its
    /// maker). It holds only what it changes.
    CreateWorkingSetBranch {
        /// Name.
        name: String,
        /// The project line it starts from (or a revision of the model, by
        /// its whole id: phase 12).
        from: String,
        /// The routes it works on.
        routes: Vec<String>,
        /// The users it is shared with.
        #[serde(default)]
        team: Vec<String>,
    },
    /// Make a branch (at a revision, default the head's).
    CreateBranch {
        /// Name.
        name: String,
        /// Branch, tag or revision.
        #[serde(default)]
        at: Option<String>,
    },
    /// Name a revision for good (default the head's).
    Tag {
        /// Name.
        name: String,
        /// Branch, tag or revision.
        #[serde(default)]
        at: Option<String>,
        /// Note.
        #[serde(default)]
        note: String,
    },
    /// Admin: protect a branch (or lift the protection).
    ProtectBranch {
        /// Branch.
        name: String,
        /// Protected.
        protected: bool,
    },
    /// Delete a branch or tag (refused without `confirm`, listing what
    /// goes with it).
    DeleteRef {
        /// Name.
        name: String,
        /// Confirms.
        #[serde(default)]
        confirm: bool,
    },
    /// Merge a branch (tag, revision) into the current branch.
    Merge {
        /// Branch, tag or revision.
        other: String,
        /// Message of the merge revision.
        #[serde(default)]
        message: Option<String>,
        /// Conflicts settled.
        #[serde(default)]
        choices: Choices,
    },
    /// A revision undoing another's changes.
    Revert {
        /// The revision.
        revision: String,
        /// Message.
        #[serde(default)]
        message: Option<String>,
        /// Conflicts settled.
        #[serde(default)]
        choices: Choices,
    },
    /// Submit a branch for review.
    SubmitReview {
        /// Branch.
        branch: String,
        /// Branch to merge it into.
        target: String,
        /// Reviewers.
        #[serde(default)]
        reviewers: Vec<String>,
        /// Note.
        #[serde(default)]
        note: String,
    },
    /// A reviewer approves or rejects a review.
    DecideReview {
        /// Review.
        id: i64,
        /// Approve (else reject).
        approve: bool,
        /// Comment.
        #[serde(default)]
        comment: String,
    },
    /// Merge an approved review into its target (the current branch).
    MergeReview {
        /// Review.
        id: i64,
        /// Message.
        #[serde(default)]
        message: Option<String>,
        /// Conflicts settled.
        #[serde(default)]
        choices: Choices,
    },
    /// Admin: limit a user's (or role's) edits.
    Grant {
        /// The grant.
        grant: Grant,
    },
    /// Admin: remove a grant.
    Revoke {
        /// Grant id.
        id: i64,
    },
    /// **Update library pins** (a network's server, phase 9): the line's
    /// library files moved to the network's releases — every kind
    /// released (`kinds` empty) or those named, to the latest or to
    /// `release` (one kind). The server carries it out as
    /// [`Op::LibraryPins`].
    UpdateLibraryPins {
        /// The kinds (empty: every kind released).
        #[serde(default)]
        kinds: Vec<String>,
        /// The release (one kind; none: the latest).
        #[serde(default)]
        release: Option<String>,
    },
    /// Update library pins as the network's server carried it out: what
    /// it broadcasts and the replicas replay (never asked by a client).
    LibraryPins {
        /// The pins after (kind → release).
        pins: BTreeMap<String, String>,
        /// The library files changed (kind, name, text — none: removed).
        changes: Vec<(String, String, Option<String>)>,
        /// The routes whose results go out of date, with their notice.
        stale: BTreeMap<String, String>,
        /// The log entry's words.
        note: String,
    },
}

impl Op {
    /// The objects a command touches (for the soft locks); `None`: the
    /// whole line (a line replaced).
    pub fn targets(&self) -> Option<Vec<(Kind, String)>> {
        fn of(c: &Command, out: &mut Vec<(Kind, String)>) -> bool {
            match c {
                Command::Put { kind, id, .. }
                | Command::Delete { kind, id, .. }
                | Command::Rename { kind, id, .. } => out.push((*kind, id.clone())),
                Command::MoveStructure { id, .. }
                | Command::DeleteStructure { id, .. }
                | Command::ReplaceStructure { id, .. } => {
                    out.push((Kind::Structure, id.clone()));
                }
                Command::DeleteAttachment { structure, .. }
                | Command::RenameAttachment { structure, .. }
                | Command::LinkPoint { structure, .. }
                | Command::UnlinkPoint { structure, .. }
                | Command::RelinkPoints { structure, .. } => {
                    out.push((Kind::Structure, structure.clone()));
                }
                Command::SetStringing { section, .. } | Command::SetWindFactor { section, .. } => {
                    out.push((Kind::Section, section.clone()));
                }
                // The point's host (what names the point follows).
                Command::RenamePoint { kind, id, .. } => out.push((*kind, id.clone())),
                Command::TieAlignmentPoint {
                    route, structure, ..
                } => {
                    out.push((Kind::Route, route.clone()));
                    out.push((Kind::Structure, structure.clone()));
                }
                Command::MoveAlignmentPoint { route, .. }
                | Command::InsertAlignmentPoint { route, .. }
                | Command::RemoveAlignmentPoint { route, .. }
                | Command::DetachAlignmentPoint { route, .. }
                | Command::AlignUnderStructures { route } => {
                    out.push((Kind::Route, route.clone()));
                }
                Command::RenameSystem { .. } => out.push((Kind::Meta, "systems".into())),
                // Points of a cloud: no object of the line is locked.
                Command::ChangePoints { cloud, .. } => {
                    out.push((Kind::PointCloud, cloud.clone()));
                }
                Command::Batch { commands } => {
                    for c in commands {
                        if !of(c, out) {
                            return false;
                        }
                    }
                }
                Command::ReplaceLine { .. } => return false,
            }
            true
        }
        match self {
            Op::Do { command } => {
                let mut out = Vec::new();
                of(command, &mut out).then_some(out)
            }
            // The working state replaced at large.
            Op::BringIn { .. }
            | Op::RollBackLine { .. }
            | Op::Merge { .. }
            | Op::Revert { .. }
            | Op::MergeReview { .. } => None,
            _ => Some(Vec::new()),
        }
    }

    /// In words.
    pub fn describe(&self) -> String {
        match self {
            Op::Do { command } => serde_json::to_value(command)
                .ok()
                .and_then(|v| v.get("op").and_then(|o| o.as_str()).map(str::to_owned))
                .unwrap_or_else(|| "command".into())
                .replace('_', " "),
            Op::Undo => "undo".into(),
            Op::Redo => "redo".into(),
            Op::Rollback { seq } => format!("rollback of entry {seq}"),
            Op::RollbackAfter { seq } => format!("rollback to entry {seq}"),
            Op::SetUser { name, role } => format!("{name} made {}", role.as_str()),
            Op::PutLibrary { kind, name, .. } => format!("library {kind}/{name} saved"),
            Op::DeleteLibrary { kind, name } => format!("library {kind}/{name} removed"),
            Op::CreateLine { name, from } => format!("project line {name} started from {from}"),
            Op::CreateWorkingSetBranch {
                name, from, routes, ..
            } => format!(
                "branch {name} over routes {} started from {from}",
                routes.join(", ")
            ),
            Op::DeleteLine { name } => format!("project line {name} deleted"),
            Op::RenameLine { from, to } => format!("project line {from} renamed {to}"),
            Op::BringIn { from, .. } => format!("changes of {from} brought in"),
            Op::SetLineDescription { line, .. } => format!("project line {line} described"),
            Op::RollBackLine { line, revision } => {
                format!(
                    "project line {line} rolled back to {}",
                    ohl_session::versions::short(revision)
                )
            }
            Op::SetSetting { key, .. } => format!("setting {key}"),
            Op::Commit { message } => format!("commit \"{message}\""),
            Op::CreateBranch { name, .. } => format!("branch {name} made"),
            Op::Tag { name, .. } => format!("tag {name} made"),
            Op::ProtectBranch { name, protected } => format!(
                "branch {name} {}",
                if *protected {
                    "protected"
                } else {
                    "unprotected"
                }
            ),
            Op::DeleteRef { name, .. } => format!("{name} deleted"),
            Op::Merge { other, .. } => format!("{other} merged"),
            Op::Revert { revision, .. } => format!("{revision} reverted"),
            Op::SubmitReview { branch, target, .. } => {
                format!("{branch} submitted for review into {target}")
            }
            Op::DecideReview { id, approve, .. } => format!(
                "review #{id} {}",
                if *approve { "approved" } else { "rejected" }
            ),
            Op::MergeReview { id, .. } => format!("review #{id} merged"),
            Op::Grant { grant } => format!("{} may edit {}", grant.subject, grant.describe()),
            Op::Revoke { id } => format!("grant {id} revoked"),
            Op::UpdateLibraryPins { kinds, .. } => format!(
                "library pins updated ({})",
                if kinds.is_empty() {
                    "every kind".to_owned()
                } else {
                    kinds.join(", ")
                }
            ),
            Op::LibraryPins { pins, .. } => format!(
                "library pins: {}",
                pins.iter()
                    .map(|(k, r)| format!("{k} → {r}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// Executes `op` of `user` on project line `line` of a session: the
/// server's and every replica's code. The session's own line is chosen
/// again after. Returns what changed in that line (`None`: nothing in the
/// line, e.g. a library or a user).
pub fn execute(
    session: &mut Session,
    user: &str,
    line: &str,
    op: &Op,
) -> Result<Option<Applied>, SessionError> {
    session.with_line(line, |s| run(s, user, op))
}

/// [`execute`], with `check` deciding what it changed (on the project line
/// it acted on, as it is after it) — all of it kept, or (a refusal, an
/// error) nothing: a network's server deciding each command's objects by
/// the rights (phase 7).
pub fn execute_checked(
    session: &mut Session,
    user: &str,
    line: &str,
    op: &Op,
    check: impl FnOnce(&mut Session, &Applied) -> Result<(), SessionError>,
) -> Result<Option<Applied>, SessionError> {
    session.atomically(|s| {
        s.with_line(line, |s| {
            let applied = run(s, user, op)?;
            if let Some(a) = &applied {
                check(s, a)?;
            }
            Ok(applied)
        })
    })
}

fn run(session: &mut Session, user: &str, op: &Op) -> Result<Option<Applied>, SessionError> {
    Ok(match op {
        Op::Do { command } => Some(session.apply(user, command.clone())?),
        Op::Undo => Some(session.undo(user)?),
        Op::Redo => Some(session.redo(user)?),
        Op::Rollback { seq } => Some(session.rollback(user, *seq)?),
        Op::RollbackAfter { seq } => session.rollback_after(user, *seq)?,
        Op::SetUser { name, role } => {
            session.set_user(user, name, *role)?;
            None
        }
        Op::PutLibrary { kind, name, text } => {
            session.put_library_file(user, kind, name, text)?;
            None
        }
        Op::DeleteLibrary { kind, name } => {
            session.delete_library_file(user, kind, name)?;
            None
        }
        Op::CreateWorkingSetBranch {
            name,
            from,
            routes,
            team,
        } => {
            let from = from.trim();
            // A project line, or (phase 12: an offline working set brought
            // back) a revision of the model, by its whole id.
            let of_line = session.is_line(from);
            if !of_line && (from.len() != 64 || session.revision(from).is_err()) {
                return Err(SessionError::Refused(format!(
                    "a branch over a working set starts from a project line or a revision, not \"{from}\""
                )));
            }
            if routes.is_empty() {
                return Err(SessionError::Refused(
                    "a working set holds at least one route".into(),
                ));
            }
            let known = &session
                .line_named(if of_line {
                    from
                } else {
                    ohl_session::versions::MAIN
                })?
                .routes;
            if let Some(r) = routes.iter().find(|r| !known.iter().any(|x| &x.id == *r)) {
                return Err(SessionError::Refused(format!("no route {r}")));
            }
            session.create_line(user, name, from)?;
            session.set_branch_scope(
                user,
                name.trim(),
                Some(ohl_session::shared::BranchScope {
                    by: user.to_owned(),
                    routes: routes.iter().cloned().collect(),
                    team: team.clone(),
                }),
            )?;
            None
        }
        Op::CreateLine { name, from } => {
            session.create_line(user, name, from)?;
            None
        }
        Op::DeleteLine { name } => {
            session.delete_line(user, name)?;
            None
        }
        Op::RenameLine { from, to } => {
            session.rename_line(user, from, to)?;
            None
        }
        Op::BringIn { from, choices } => session.bring_in(user, from, choices)?.applied,
        Op::SetLineDescription { line, description } => {
            session.set_line_description(user, line, description)?;
            None
        }
        // What changed in the line worked on (another line: nothing here).
        Op::RollBackLine { line, revision } => {
            let applied = session.roll_back_line(user, line, revision)?;
            if line == session.current_line() {
                applied
            } else {
                None
            }
        }
        // A designer whose edits no grant limits, and an admin only on a
        // protected line; never a path of the sender's computer (security
        // review 2026-10-26, M5).
        Op::SetSetting { key, value } => {
            session.require(user, Role::Designer)?;
            session.require_unrestricted(user)?;
            session.check_line_write(user)?;
            if value.as_deref().is_some_and(ohl_session::is_local_path) {
                return Err(SessionError::Refused(format!(
                    "setting {key}: a path of one computer is not shared (a file name is)"
                )));
            }
            session.set_setting(key, value.as_deref())?;
            None
        }
        Op::Commit { message } => {
            session.commit(user, message)?;
            None
        }
        Op::CreateBranch { name, at } => {
            session.create_branch(user, name, at.as_deref())?;
            None
        }
        Op::Tag { name, at, note } => {
            session.tag(user, name, at.as_deref(), note)?;
            None
        }
        Op::ProtectBranch { name, protected } => {
            session.protect_branch(user, name, *protected)?;
            None
        }
        Op::DeleteRef { name, confirm } => {
            session.delete_ref(user, name, *confirm)?;
            None
        }
        Op::Merge {
            other,
            message,
            choices,
        } => {
            session
                .merge(user, other, message.as_deref(), choices)?
                .applied
        }
        Op::Revert {
            revision,
            message,
            choices,
        } => {
            session
                .revert(user, revision, message.as_deref(), choices)?
                .applied
        }
        Op::SubmitReview {
            branch,
            target,
            reviewers,
            note,
        } => {
            session.submit_review(user, branch, target, reviewers, note)?;
            None
        }
        Op::DecideReview {
            id,
            approve,
            comment,
        } => {
            session.decide_review(user, *id, *approve, comment)?;
            None
        }
        Op::MergeReview {
            id,
            message,
            choices,
        } => {
            session
                .merge_review(user, *id, message.as_deref(), choices)?
                .applied
        }
        Op::Grant { grant } => {
            session.grant(user, grant)?;
            None
        }
        Op::Revoke { id } => {
            session.revoke(user, *id)?;
            None
        }
        Op::UpdateLibraryPins { .. } => {
            return Err(SessionError::Refused(
                "library pins are a network's: its server updates them".into(),
            ));
        }
        Op::LibraryPins {
            changes,
            stale,
            note,
            ..
        } => session.library_pins_update(user, changes, stale, note)?,
    })
}

/// What a user shows the others.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    /// The selection, in words.
    #[serde(default)]
    pub selection: String,
    /// Where the 3D view's eye is (line coordinates).
    #[serde(default)]
    pub camera: Option<[f64; 3]>,
}

/// A user connected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Peer {
    /// Name.
    pub user: String,
    /// Role.
    pub role: Role,
    /// What they show.
    pub presence: Presence,
}

/// A soft lock's target: `main:structure:T2`, `phase 1:section:S1`… (the
/// same object of two project lines is two objects).
pub fn lock_target(line: &str, kind: Kind, id: &str) -> String {
    format!("{line}:{}:{id}", kind.as_str())
}

/// The main line's name, the default of a request's line.
fn main_line() -> String {
    ohl_session::versions::MAIN.to_owned()
}

/// Client → server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// First message.
    Hello {
        /// [`PROTOCOL`].
        protocol: u32,
        /// User name.
        user: String,
        /// The access code of the invitation.
        code: String,
    },
    /// Asked first instead of joining (2026-11-20, the sign-in window):
    /// how the server takes sign-ins, answered with [`ServerMsg::SignIn`],
    /// then the connection closes. A server of before closes it
    /// unanswered (invitations only).
    SignInOptions {
        /// [`PROTOCOL`].
        protocol: u32,
    },
    /// An edit; answered with [`ServerMsg::Response`] of the same id.
    Request {
        /// Chosen by the client.
        id: u64,
        /// The edit.
        op: Op,
        /// The project line it acts on (the user's).
        #[serde(default = "main_line")]
        line: String,
    },
    /// What the user shows the others (sent when it changes).
    Presence(Presence),
    /// Hold a soft lock (answered with [`ServerMsg::Locks`], or a refusal
    /// in [`ServerMsg::Notice`]).
    Lock {
        /// [`lock_target`].
        target: String,
    },
    /// Release a soft lock.
    Unlock {
        /// [`lock_target`].
        target: String,
    },
    /// First message of a client of a network's server (protocol 3; the
    /// hello joins as before): what it follows. Answered as the hello is;
    /// with a model, its copy follows the welcome as a project file does.
    Join {
        /// [`PROTOCOL`].
        protocol: u32,
        /// User name.
        user: String,
        /// The access code of the invitation.
        code: String,
        /// The model to follow whole (its copy is sent, then its changes).
        #[serde(default)]
        model: Option<String>,
        /// Changes after this position first (a catch-up after a
        /// disconnect).
        #[serde(default)]
        since: Option<Position>,
        /// The routes of its working set.
        #[serde(default)]
        routes: Vec<String>,
        /// The tiles it views (with the revision it holds).
        #[serde(default)]
        tiles: Vec<HeldTile>,
        /// It holds the model's copy already (it catches up from
        /// `since`): no copy is sent.
        #[serde(default)]
        held: bool,
        /// Phase 6: the working set it follows as a partial replica (in
        /// place of `model`): its replica is sent as a project file (unless
        /// `held`: then it says what it holds with [`ClientMsg::Hold`]),
        /// then the rows that change ([`ServerMsg::Rows`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_set: Option<WorkingSetSpec>,
        /// Phase 7: an admin sees as this user (what both may see; nothing
        /// is changed so; in the audit).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        see_as: Option<String>,
        /// Phase 13: an ID token of the network's identity provider
        /// (OIDC) in place of the access code (`code` then empty): its
        /// subject signs in as the user a network admin mapped it to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id_token: Option<String>,
    },
    /// The routes and tiles a client of a network follows from now on (in
    /// place of the last ones): tiles new to it come as packages at their
    /// revision (unless it holds that one), then their changes.
    Interest {
        /// The routes of its working set.
        #[serde(default)]
        routes: Vec<String>,
        /// The tiles it views.
        #[serde(default)]
        tiles: Vec<HeldTile>,
    },
    /// Every change of a client's interest after `since` (in a
    /// [`ServerMsg::CatchUp`]).
    Catch {
        /// What it has.
        since: Position,
    },
    /// An edit of a model of a network (answered as a request is).
    RequestOn {
        /// Chosen by the client.
        id: u64,
        /// The model.
        model: String,
        /// The edit.
        op: Op,
        /// The model's project line it acts on.
        #[serde(default = "main_line")]
        line: String,
    },
    /// Phase 6: works out the working set of `picks` on `model`, snapped
    /// out to whole tension sections, to show it before it is confirmed
    /// (answered with [`ServerMsg::Extent`] of the same id; nothing kept).
    Pick {
        /// Chosen by the client.
        id: u64,
        /// The model (line).
        model: String,
        /// What was picked.
        picks: Vec<Pick>,
    },
    /// Phase 6: a client holding a replica follows working set `spec` from
    /// now on (a route added, a part taken away): answered with the rows
    /// that come and go ([`ServerMsg::Rows`]). On the same model.
    SetWorkingSet {
        /// The working set.
        spec: WorkingSetSpec,
    },
    /// Phase 6: a replica joining again (a join with `working_set` and
    /// `held`): what it holds, each object with its hash
    /// ([`ohl_session::working_set::object_hash`]); answered with the rows
    /// that differ.
    Hold {
        /// Kind, id, hash.
        objects: Vec<(Kind, String, u64)>,
    },
    /// Phase 6: saves working set `spec` under its name for this user
    /// (answered with [`ServerMsg::WorkingSets`]).
    SaveWorkingSet {
        /// The working set, named.
        spec: WorkingSetSpec,
    },
    /// Phase 6: deletes this user's working set `name` (answered with
    /// [`ServerMsg::WorkingSets`]).
    DeleteWorkingSet {
        /// Its name.
        name: String,
    },
    /// Phase 6: this user's saved working sets
    /// ([`ServerMsg::WorkingSets`]).
    WorkingSets,
    /// Phase 6: a route's structures with their stations, to pick a part
    /// of it ([`ServerMsg::RouteDetail`]).
    RouteDetail {
        /// The model.
        model: String,
        /// The route.
        route: String,
    },
    /// Phase 10: a check on a working set, its neighbours pulled in by the
    /// check's reach and computed on the server (answered with
    /// [`ServerMsg::CheckResult`] of the same id; later, once, with
    /// [`ServerMsg::CheckStale`] when what it used changed).
    Check {
        /// Chosen by the client.
        id: u64,
        /// The working set (none: the one this connection follows).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_set: Option<WorkingSetSpec>,
        /// The check.
        check: ScopedCheck,
    },
    /// Phase 12: exports working set `working_set` as an offline working
    /// set (a project file with its read-only neighbours, the export
    /// recorded); answered with [`ServerMsg::WorkingSetExported`] of the
    /// same id.
    ExportWorkingSet {
        /// Chosen by the client.
        id: u64,
        /// The working set.
        working_set: WorkingSetSpec,
        /// The checks' reach its neighbours are taken within, m (default
        /// the field distance, 500 m).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reach: Option<f64>,
    },
    /// Phase 12: brings an offline working set's work back (its bundle of
    /// only the changes): applied on a branch over the working set made
    /// from the export's base; answered with [`ServerMsg::BundleImported`]
    /// of the same id.
    ImportBundle {
        /// Chosen by the client.
        id: u64,
        /// The bundle.
        bundle: Box<ohl_session::versions::Bundle>,
        /// The branch's name (default `offline-USER-N-K`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
}

/// What an offline working set's export wrote (phase 12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineExported {
    /// The model.
    pub model: String,
    /// The export's number on the server.
    pub number: i64,
    /// The file's size, bytes.
    pub bytes: u64,
    /// Objects of the working set (editable).
    pub wanted: usize,
    /// Objects of its context and of the line's neighbours (read-only).
    pub context: usize,
    /// Other lines' neighbours: lines, sections.
    pub neighbour_lines: usize,
    /// See `neighbour_lines`.
    pub neighbour_sections: usize,
    /// What its user may not see, left out (counted).
    pub left_out: ohl_session::offline::LeftOut,
    /// The server's base revision.
    pub base: String,
    /// The export's own revision.
    pub export: String,
    /// The main line's changes were recorded first (a revision made).
    pub recorded_first: bool,
    /// Seconds on the server.
    pub seconds: f64,
    /// Phase 14: objects of it, or within its reach, changed on the
    /// network after its content was taken, while it was written on the
    /// exports' thread (each once) — **not in it**: the file is the base
    /// revision's working set, and its work brought back lists them as
    /// conflicts or neighbours changed since. 0 on the authority.
    #[serde(default)]
    pub changed_meanwhile: usize,
    /// Phase 14: such a change, or a line of it changed whole (its
    /// libraries), landed while it was written: newer on the server than
    /// in the file.
    #[serde(default)]
    pub stale: bool,
}

/// An offline working set's file as the server sends it (phase 12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineFile {
    /// What it holds.
    pub report: OfflineExported,
    /// The project file, base64.
    pub file: String,
}

/// What a bundle brought back did (phase 12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BroughtBack {
    /// The model.
    pub model: String,
    /// The branch over the working set made for it.
    pub branch: String,
    /// The offline revisions it carried.
    pub revisions: usize,
    /// Objects put and deleted.
    pub puts: usize,
    /// See `puts`.
    pub deletes: usize,
    /// Objects it changes that changed on the main line since the export
    /// (as the user may see them): conflicts its merge into main asks to
    /// settle — never settled here.
    pub conflicts: Vec<String>,
    /// Read-only neighbours (the context) changed on the server since the
    /// export: checks made offline are out of date.
    pub context_changed: usize,
    /// Seconds on the server.
    pub seconds: f64,
}

/// A check a network's server runs on a working set (phase 10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScopedCheck {
    /// Clearances by rule: the line's own wires beyond the working set and
    /// other lines' wires within the rules' reach as targets.
    Clearances {
        /// The weather case (of the working set's line).
        case: String,
        /// The rules (a rule file, `ohl-clearance-rules`).
        rules: String,
    },
    /// The magnetic field with the lines within the field distance.
    Fields {
        /// The weather case.
        case: String,
        /// The field distance, m (none: 500 m).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        distance: Option<f64>,
        /// The current of every circuit without its own, A.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amps: Option<f64>,
        /// A limit, µT.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit_ut: Option<f64>,
    },
    /// The electric field with the lines within the field distance, solved
    /// together (2026-11-24): the worst over their unknown phase angles,
    /// exact for one other line, a bound for more (said).
    ElectricField {
        /// The weather case.
        case: String,
        /// The field distance, m (none: 500 m).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        distance: Option<f64>,
        /// A limit, kV/m.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit_kv_m: Option<f64>,
    },
    /// The loads on the working set's shared towers (every line's
    /// sections on them, under each line's cases, the worst kept).
    StructureLoads {
        /// The structures (none: every structure of the working set with
        /// a twin in another line).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        structures: Vec<String>,
    },
}

impl ScopedCheck {
    /// Its name in words.
    pub fn name(&self) -> &'static str {
        match self {
            ScopedCheck::Clearances { .. } => "clearances",
            ScopedCheck::Fields { .. } => "magnetic field",
            ScopedCheck::ElectricField { .. } => "electric field",
            ScopedCheck::StructureLoads { .. } => "structure loads",
        }
    }
}

/// One thing a scoped check found, as its user may see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// The rule, profile or load.
    pub what: String,
    /// On what of the working set (a wire's span, a profile, a tower's
    /// point).
    pub on: String,
    /// Against what (another wire, a line's case); a neighbour the user
    /// may not see is never named: "a hidden object of another line".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub against: Option<String>,
    /// The measure (radial, vertical, B, factored transverse, …).
    pub measure: String,
    /// Its value (m, µT, N).
    pub value: f64,
    /// What is required, when a rule or a limit says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<f64>,
    /// Below what is required (or above a limit).
    pub violates: bool,
    /// Where on the working set's wire (none when a neighbour the user
    /// may not see is involved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 3]>,
    /// The result is limited by a neighbour the user may not see.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// What a scoped check pulled in, as its user may see it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PulledReport {
    /// The lines (models) used, those the user may see something of.
    pub lines: Vec<String>,
    /// Their routes the user may see.
    pub routes: Vec<String>,
    /// The objects used the user may see ("line L: section S").
    pub objects: Vec<String>,
    /// Objects used the user may not see: counted, never named.
    pub hidden: usize,
    /// Objects read beside the working set (within its reach or not).
    pub read: usize,
}

/// A scoped check's result (phase 10), filtered by the user's rights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopedReport {
    /// The check.
    pub check: String,
    /// The working set's model.
    pub model: String,
    /// Its reach, in words.
    pub reach: String,
    /// Every part checked, or "NOT ALL CHECKED".
    pub status: String,
    /// What it found (each violation, and per rule or profile the
    /// lowest or highest).
    pub findings: Vec<Finding>,
    /// Results in all (pairs, points, loads).
    pub results: usize,
    /// Of them, below what is required.
    pub violations: usize,
    /// What it pulled in.
    pub pulled: PulledReport,
    /// What it could not do (each makes it not all checked).
    pub not_all_checked: Vec<String>,
    /// What it assumed (NOT ON THE SAFE SIDE where it is not).
    pub notes: Vec<String>,
    /// Seconds on the server.
    pub seconds: f64,
}

/// Rows a partial replica takes to follow its model (phase 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowsMsg {
    /// The model.
    pub model: String,
    /// The stream's position the replica holds once they are taken.
    pub at: Position,
    /// Who made the change (none: the working set changed, or a replica
    /// joining again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// The change in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what: Option<String>,
    /// The rows.
    pub rows: Rows,
    /// The working set followed from now on, when it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<WorkingSetSpec>,
}

/// A working set saved (phase 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSet {
    /// Its name, model and picks.
    pub spec: WorkingSetSpec,
    /// The routes its extent was on when saved.
    pub routes: Vec<String>,
    /// When it was saved.
    pub time: String,
}

/// Server → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// What the server takes to join (the answer to
    /// [`ClientMsg::SignInOptions`]).
    SignIn(sign_in::SignInOptions),
    /// Joined: the project file follows on its own stream (`size` bytes),
    /// as it was after log entry `seq`.
    Welcome {
        /// Project (line) name.
        project: String,
        /// The user's role.
        role: Role,
        /// Log entry the copy is at.
        seq: i64,
        /// Bytes of the copy.
        size: u64,
        /// Users connected (with the newcomer).
        peers: Vec<Peer>,
        /// Soft locks held: target → user.
        locks: BTreeMap<String, String>,
    },
    /// Not joined.
    Refused {
        /// Why.
        reason: String,
    },
    /// An edit applied (in the server's order); `seq`: the log entry it
    /// made, if any.
    Applied {
        /// User who made it.
        user: String,
        /// The edit.
        op: Op,
        /// The project line it acted on.
        #[serde(default = "main_line")]
        line: String,
        /// Log entry made.
        seq: Option<i64>,
    },
    /// The answer to a request (after its [`ServerMsg::Applied`]).
    Response {
        /// The request's id.
        id: u64,
        /// What was done, or why not.
        result: Result<String, String>,
    },
    /// The users connected changed (joined, left, role, presence).
    Peers {
        /// Who.
        peers: Vec<Peer>,
    },
    /// The soft locks changed.
    Locks {
        /// Target → user.
        locks: BTreeMap<String, String>,
    },
    /// Something to tell the user (e.g. a lock refused).
    Notice {
        /// The text.
        text: String,
    },
    /// Web clients only: what to draw, sent after the welcome and again
    /// when the line changed.
    Scene(Box<Scene>),
    /// A network's server: what it holds (after the welcome).
    Network(Box<NetworkInfo>),
    /// A network's server: a change touching the client's interest.
    Change(Box<Change>),
    /// A network's server: a tile of the context at its revision.
    Tile(Box<TilePackage>),
    /// A network's server: these tiles of the client changed too much to
    /// follow: read them again.
    TilesChanged {
        /// The tiles.
        tiles: Vec<TileKey>,
        /// Their new revision.
        at: Position,
        /// Why.
        reason: String,
    },
    /// A network's server: where the stream is (`at`) and the last change
    /// sent to this client (`last`): a client that has not received `last`
    /// missed something and catches up.
    Heartbeat {
        /// The server's position.
        at: Position,
        /// The position of the last change sent to this client.
        #[serde(default)]
        last: Option<Position>,
    },
    /// A network's server: the changes of the client's interest after
    /// `from`, in order, up to `to` (the stream's position then).
    CatchUp {
        /// Asked from.
        from: Position,
        /// Up to.
        to: Position,
        /// The changes.
        changes: Vec<Change>,
        /// More than a catch-up carries: read the model or tiles again.
        #[serde(default)]
        incomplete: bool,
    },
    /// Phase 6: the working set picked, snapped (or why not), for
    /// [`ClientMsg::Pick`] of the same id.
    Extent {
        /// The pick's id.
        id: u64,
        /// The model.
        model: String,
        /// The working set in numbers, or why not.
        result: Result<ExtentSummary, String>,
    },
    /// Phase 6: rows a partial replica takes (after a change of its model,
    /// a change of its working set, or when it joined again).
    Rows(Box<RowsMsg>),
    /// Phase 6: the user's saved working sets.
    WorkingSets {
        /// By name.
        sets: Vec<SavedSet>,
    },
    /// Phase 6: a route's structures with their stations.
    RouteDetail {
        /// The model.
        model: String,
        /// The route, or why not.
        result: Result<RouteDetail, String>,
    },
    /// Phase 10: a scoped check's result, for [`ClientMsg::Check`] of the
    /// same id.
    CheckResult {
        /// The check's id.
        id: u64,
        /// What it found, or why not.
        result: Result<Box<ScopedReport>, String>,
    },
    /// Phase 10: what check `id` used changed: its result is stale (sent
    /// once; check again).
    CheckStale {
        /// The check's id.
        id: u64,
        /// What changed, as the user may see it.
        reason: String,
    },
    /// Phase 12: an offline working set exported, for
    /// [`ClientMsg::ExportWorkingSet`] of the same id.
    WorkingSetExported {
        /// The request's id.
        id: u64,
        /// The file, or why not.
        result: Result<Box<OfflineFile>, String>,
    },
    /// Phase 12: a bundle brought back, for [`ClientMsg::ImportBundle`] of
    /// the same id.
    BundleImported {
        /// The request's id.
        id: u64,
        /// What it did, or why not.
        result: Result<Box<BroughtBack>, String>,
    },
}

/// The line as a web client draws it: solved on the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Log entry the line is at.
    pub seq: i64,
    /// Edits of the line or its libraries since the server started: a
    /// newer scene has a higher one.
    pub revision: u64,
    /// The line (typed structures given their attachments when the
    /// libraries allow).
    pub line: ohl_line::Line,
    /// Weather case the wires are solved in.
    pub case: String,
    /// One per section, in the line's order.
    pub wires: Vec<SceneWire>,
    /// What could not be done (libraries, structure types).
    pub notes: Vec<String>,
}

/// A section's wire, solved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneWire {
    /// Section id.
    pub section: String,
    /// One per span, in order (empty when it did not solve).
    pub spans: Vec<SceneSpan>,
    /// Why it did not solve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A span's wire, solved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneSpan {
    /// Points along the wire, back to ahead (millimetre precision).
    pub points: Vec<[f64; 3]>,
    /// Largest vertical distance below the chord, m.
    pub sag: f64,
    /// Largest tension, N.
    pub max_tension: f64,
}

/// How to join a server: `ohl://HOST:PORT/CODE/FINGERPRINT` (the
/// fingerprint: SHA-256 of the server certificate, hexadecimal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    /// Host name or address, and port.
    pub address: String,
    /// Access code.
    pub code: String,
    /// SHA-256 of the server's certificate.
    pub fingerprint: [u8; 32],
}

impl fmt::Display for Invitation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ohl://{}/{}/", self.address, self.code)?;
        for b in self.fingerprint {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Invitation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("not an invitation: {s} (ohl://HOST:PORT/CODE/FINGERPRINT)");
        let s = s.trim();
        let rest = s.strip_prefix("ohl://").ok_or_else(bad)?;
        let mut parts = rest.split('/');
        let (Some(address), Some(code), Some(fp), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        if address.is_empty() || code.is_empty() || fp.len() != 64 || !fp.is_ascii() {
            return Err(bad());
        }
        let mut fingerprint = [0u8; 32];
        for (k, b) in fingerprint.iter_mut().enumerate() {
            *b = u8::from_str_radix(&fp[2 * k..2 * k + 2], 16).map_err(|_| bad())?;
        }
        Ok(Self {
            address: address.to_owned(),
            code: code.to_owned(),
            fingerprint,
        })
    }
}

/// The general access code (for users the project does not know),
/// derived from the server's general secret: the host's `--code`, or a
/// random one. Twelve hexadecimal digits, as before 2026-10-26 (a link
/// made with `--code` stays valid). Everyone holding the general
/// invitation knows this code; nothing else is derived from that secret
/// (security review 2026-10-26, H2).
pub fn general_code(secret: &str) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(secret.as_bytes());
    h.update(b"\n*");
    hex(&h.finalize()[..6])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// HMAC-SHA-256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&sha2::Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |x: u8| block.map(|b| b ^ x);
    let mut inner = sha2::Sha256::new();
    inner.update(pad(0x36));
    inner.update(message);
    let mut outer = sha2::Sha256::new();
    outer.update(pad(0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// The keys the access codes are made from: the general secret (the
/// host's `--code`, or random) gives only the general code; the personal
/// codes come from a random key of 256 bits that is never derived from
/// the general secret, so knowing (or guessing) the general invitation
/// tells nothing about anybody's personal one (security review
/// 2026-10-26, H2).
#[derive(Clone)]
pub struct Keys {
    general: String,
    personal: [u8; 32],
}

impl fmt::Debug for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Keys(..)")
    }
}

impl Keys {
    /// Keys of a general secret and a personal key.
    pub fn new(general: &str, personal: [u8; 32]) -> Self {
        Self {
            general: general.to_owned(),
            personal,
        }
    }

    /// Random keys (for one run of a server).
    pub fn random() -> Self {
        let mut personal = [0u8; 32];
        getrandom::fill(&mut personal).expect("system randomness");
        Self::new(&random_code(16), personal)
    }

    /// The access code for `user` (`None`: the general invitation).
    /// Personal codes: HMAC-SHA-256 of the personal key over the name,
    /// 128 bits in hexadecimal.
    pub fn code(&self, user: Option<&str>) -> String {
        match user {
            None => general_code(&self.general),
            Some(u) => hex(&hmac_sha256(&self.personal, format!("user:{u}").as_bytes())[..16]),
        }
    }

    /// Phase 13: `user`'s personal access code of `generation` (each
    /// reissue adds one: older codes refused), valid to the end of day
    /// `until` (UTC, `YYYYMMDD`; none: no end). Generation 0 without an
    /// end is the code of before ([`Keys::code`]): invitations given out
    /// stay valid until reissued. Otherwise `G.UNTIL.MAC` (`UNTIL` 0 for
    /// none), the MAC over the name, the generation and the end — neither
    /// can be changed without the key.
    pub fn personal_code(&self, user: &str, generation: i32, until: Option<u32>) -> String {
        if generation == 0 && until.is_none() {
            return self.code(Some(user));
        }
        let u = until.unwrap_or(0);
        let mac = hmac_sha256(
            &self.personal,
            format!("user:{user}\ngeneration:{generation}\nuntil:{u}").as_bytes(),
        );
        format!("{generation}.{u}.{}", hex(&mac[..16]))
    }

    /// Phase 13: checks `code` as `user`'s personal access code, whose
    /// current generation is `generation`, on day `today` (`YYYYMMDD`,
    /// UTC). A code that is not one of the key's is [`CodeRefusal::Wrong`]
    /// (nothing else said); one of the key's of an older generation is
    /// withdrawn, one past its day expired.
    pub fn check_personal(
        &self,
        user: &str,
        code: &str,
        generation: i32,
        today: u32,
    ) -> Result<(), CodeRefusal> {
        let mut parts = code.splitn(3, '.');
        let (g, u, rest) = (parts.next(), parts.next(), parts.next());
        let (g, u) = match (g, u, rest) {
            (Some(g), Some(u), Some(_)) => (
                g.parse::<i32>().map_err(|_| CodeRefusal::Wrong)?,
                u.parse::<u32>().map_err(|_| CodeRefusal::Wrong)?,
            ),
            _ => {
                // The code of before: generation 0, no end.
                if !same_code(code, &self.code(Some(user))) {
                    return Err(CodeRefusal::Wrong);
                }
                return if generation == 0 {
                    Ok(())
                } else {
                    Err(CodeRefusal::Withdrawn)
                };
            }
        };
        let until = (u != 0).then_some(u);
        if (g == 0 && until.is_none()) || !same_code(code, &self.personal_code(user, g, until)) {
            return Err(CodeRefusal::Wrong);
        }
        if g != generation {
            return Err(CodeRefusal::Withdrawn);
        }
        match until {
            Some(day) if day < today => Err(CodeRefusal::Expired(day)),
            _ => Ok(()),
        }
    }

    /// The personal key from a key file (64 hexadecimal digits), made
    /// with a fresh random key when the file does not exist (readable and
    /// writable by its owner only).
    pub fn personal_key_file(path: &std::path::Path) -> Result<[u8; 32], String> {
        let parse = |text: &str| -> Option<[u8; 32]> {
            let t = text.trim();
            if t.len() != 64 || !t.is_ascii() {
                return None;
            }
            let mut key = [0u8; 32];
            for (k, b) in key.iter_mut().enumerate() {
                *b = u8::from_str_radix(&t[2 * k..2 * k + 2], 16).ok()?;
            }
            Some(key)
        };
        match std::fs::read_to_string(path) {
            Ok(text) => parse(&text).ok_or_else(|| {
                format!(
                    "{}: not a server key file (64 hexadecimal digits)",
                    path.display()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut key = [0u8; 32];
                getrandom::fill(&mut key).map_err(|e| e.to_string())?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                use std::io::Write;
                options
                    .open(path)
                    .and_then(|mut f| writeln!(f, "{}", hex(&key)))
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(key)
            }
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

/// Phase 13: why a personal access code is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeRefusal {
    /// Not a code of the server's key for this user.
    Wrong,
    /// A code of an older generation (the invitation was reissued).
    Withdrawn,
    /// Valid to the end of this day (`YYYYMMDD`, UTC), passed.
    Expired(u32),
}

/// Phase 13: the day `days` days after today, `YYYYMMDD` (UTC).
pub fn utc_day(days_from_today: i64) -> u32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    ymd(now.div_euclid(86_400) + days_from_today)
}

/// The date of day `z` after 1970-01-01 as `YYYYMMDD` (the proleptic
/// Gregorian calendar; H. Hinnant's civil-from-days).
pub fn ymd(z: i64) -> u32 {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y * 10_000 + m * 100 + d) as u32
}

/// Makes a new directory only its owner may enter (0700 on Unix), with a
/// random name, under the temporary folder: the copies of a project made
/// for the clients are kept there (security review 2026-10-26, L6).
pub fn private_temp_dir(prefix: &str) -> Result<std::path::PathBuf, String> {
    let mut last = String::new();
    for _ in 0..8 {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", random_code(12)));
        // Never a folder that exists already (`create`, not `create_all`).
        match private_builder().create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) => last = format!("{}: {e}", dir.display()),
        }
    }
    Err(last)
}

fn private_builder() -> std::fs::DirBuilder {
    #[allow(unused_mut)]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
}

/// Makes folder `path` (its parent must exist) only its owner may enter,
/// unless it exists.
pub(crate) fn private_dir(path: &std::path::Path) -> std::io::Result<()> {
    match private_builder().create(path) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        r => r,
    }
}

/// A token bucket: `rate` per second, at most `burst` at once.
#[derive(Debug, Clone)]
pub(crate) struct Bucket {
    tokens: f64,
    rate: f64,
    burst: f64,
    last: std::time::Instant,
}

impl Bucket {
    pub(crate) fn new(rate: f64, burst: f64) -> Self {
        Self {
            tokens: burst,
            rate,
            burst,
            last: std::time::Instant::now(),
        }
    }

    /// Takes one token, if there is one.
    pub(crate) fn take(&mut self) -> bool {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.rate).min(self.burst);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Characters that look like another (Cyrillic and Greek letters drawn
/// as Latin ones), mapped to the Latin letter: a name's skeleton.
const LOOK_ALIKES: &[(char, char)] = &[
    ('а', 'a'),
    ('в', 'b'),
    ('е', 'e'),
    ('ё', 'e'),
    ('і', 'i'),
    ('ј', 'j'),
    ('к', 'k'),
    ('м', 'm'),
    ('н', 'h'),
    ('о', 'o'),
    ('р', 'p'),
    ('с', 'c'),
    ('т', 't'),
    ('у', 'y'),
    ('х', 'x'),
    ('ѕ', 's'),
    ('ԁ', 'd'),
    ('ԛ', 'q'),
    ('ԝ', 'w'),
    ('α', 'a'),
    ('β', 'b'),
    ('ε', 'e'),
    ('ζ', 'z'),
    ('η', 'h'),
    ('ι', 'i'),
    ('κ', 'k'),
    ('μ', 'm'),
    ('ν', 'v'),
    ('ο', 'o'),
    ('ρ', 'p'),
    ('τ', 't'),
    ('υ', 'u'),
    ('χ', 'x'),
    ('ı', 'i'),
    ('ℓ', 'l'),
    ('0', 'o'),
    ('1', 'l'),
    ('|', 'l'),
];

/// A name as it looks: lower case, full-width forms as ASCII,
/// Cyrillic and Greek look-alikes as Latin letters, `0` as `o`, `1`
/// and `i` as `l`, spaces and the like dropped. Two names of one
/// skeleton are told apart only by the eye's mistakes.
pub fn name_skeleton(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        // Full-width ASCII forms (U+FF01–FF5E).
        let c = match c as u32 {
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFF01 + 0x21).unwrap_or(c),
            _ => c,
        };
        for c in c.to_lowercase() {
            let c = LOOK_ALIKES
                .iter()
                .find(|(from, _)| *from == c)
                .map_or(c, |(_, to)| *to);
            let c = if c == 'i' { 'l' } else { c };
            if c.is_whitespace() || matches!(c, '_' | '-' | '.') {
                continue;
            }
            out.push(c);
        }
    }
    out
}

/// Invisible characters (format characters, bidirectional controls):
/// never in a name.
fn invisible(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD | 0x034F | 0x061C | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180F
            | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F
            | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFFB | 0xE0000..=0xE0FFF
    )
}

/// SHA-256 of a certificate (DER).
pub fn fingerprint(der: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(der).into()
}

/// Writes a message: length (u32, big endian), JSON.
pub async fn write_msg<T: Serialize>(
    stream: &mut quinn::SendStream,
    msg: &T,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
    write_encoded(stream, &bytes).await
}

/// Writes a message already encoded (its JSON): length (u32, big endian),
/// the bytes. A broadcast is encoded once and written to every client so.
pub async fn write_encoded(stream: &mut quinn::SendStream, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_MESSAGE {
        return Err(format!("message of {} bytes: too large", bytes.len()));
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stream.write_all(bytes).await.map_err(|e| e.to_string())
}

/// Reads a message (`None`: the stream ended).
pub async fn read_msg<T: for<'de> Deserialize<'de>>(
    stream: &mut quinn::RecvStream,
) -> Result<Option<T>, String> {
    read_msg_max(stream, MAX_MESSAGE).await
}

/// Phase 13: `len` bytes (in KiB) taken from `budget`, waited for
/// `wait` at most.
pub(crate) async fn take_budget(
    len: usize,
    budget: &std::sync::Arc<tokio::sync::Semaphore>,
    wait: std::time::Duration,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    let kib = u32::try_from(len.div_ceil(1024)).unwrap_or(u32::MAX);
    tokio::time::timeout(wait, budget.clone().acquire_many_owned(kib))
        .await
        .map_err(|_| {
            format!(
                "a message of {} MB: the server is reading too many large messages: try again",
                len >> 20
            )
        })?
        .map_err(|_| "server stopped".to_owned())
}

/// Reads a message of at most `max` bytes (`None`: the stream ended).
pub async fn read_msg_max<T: for<'de> Deserialize<'de>>(
    stream: &mut quinn::RecvStream,
    max: usize,
) -> Result<Option<T>, String> {
    read_msg_budget(stream, max, None).await
}

/// [`read_msg_max`], a message larger than [`LARGE_MESSAGE`] first
/// taking its size (KiB) from `budget` (phase 13: the server's budget of
/// large messages read at once; waited for 30 s at most, then the
/// connection is closed).
pub async fn read_msg_budget<T: for<'de> Deserialize<'de>>(
    stream: &mut quinn::RecvStream,
    max: usize,
    budget: Option<&std::sync::Arc<tokio::sync::Semaphore>>,
) -> Result<Option<T>, String> {
    let mut len = [0u8; 4];
    match stream.read_exact(&mut len).await {
        Ok(()) => {}
        Err(quinn::ReadExactError::FinishedEarly(0)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(format!("message of {len} bytes: too large"));
    }
    let _permit = match budget {
        Some(b) if len > LARGE_MESSAGE => {
            Some(take_budget(len, b, std::time::Duration::from_secs(30)).await?)
        }
        _ => None,
    };
    // Grown as the bytes arrive, never allocated from the announced
    // length alone (security review 2026-10-26, M4).
    let mut buf = Vec::with_capacity(len.min(64 << 10));
    let mut piece = vec![0u8; len.min(64 << 10)];
    while buf.len() < len {
        let want = (len - buf.len()).min(piece.len());
        match stream.read(&mut piece[..want]).await {
            Ok(Some(n)) => buf.extend_from_slice(&piece[..n]),
            Ok(None) => return Err("the stream ended in a message".into()),
            Err(e) => return Err(e.to_string()),
        }
    }
    serde_json::from_slice(&buf)
        .map(Some)
        .map_err(|e| format!("bad message: {e}"))
}

/// Compares access codes in a time that does not depend on where they
/// differ.
pub fn same_code(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Checks a user name: not empty, no surrounding spaces, no control
/// characters, at most [`MAX_NAME`] characters.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("no user name".into());
    }
    if name.trim() != name || name.chars().any(char::is_control) {
        return Err("a user name without surrounding spaces or control characters".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("a user name of at most {MAX_NAME} characters"));
    }
    if name.chars().any(invisible) {
        return Err("a user name without invisible characters".into());
    }
    Ok(())
}

/// Refuses a new name that looks like a user's name (`users`) without
/// being it: the same [`name_skeleton`] — `Аdmin` (Cyrillic A) beside
/// `admin`, `b0b` beside `bob` (security review 2026-10-26, L8).
pub fn check_look_alike<'a>(
    name: &str,
    users: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let skeleton = name_skeleton(name);
    for u in users {
        if u != name && name_skeleton(u) == skeleton {
            return Err(format!(
                "{name} looks like the name of user {u}: choose another name"
            ));
        }
    }
    Ok(())
}

/// Random letters and digits (access codes).
pub fn random_code(n: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).expect("system randomness");
    bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect()
}

#[cfg(test)]
mod hardening_tests;
#[cfg(all(test, feature = "network"))]
mod library_tests;
#[cfg(all(test, feature = "network"))]
mod network_tests;
#[cfg(all(test, feature = "network"))]
mod offline_tests;
#[cfg(test)]
mod points_tests;
#[cfg(all(test, feature = "network"))]
mod rights_tests;
#[cfg(all(test, feature = "network"))]
mod scoped_tests;
#[cfg(test)]
mod security_tests;
#[cfg(all(test, feature = "network"))]
mod sign_in_tests;
#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "network"))]
mod working_set_tests;
