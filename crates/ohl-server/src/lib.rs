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

pub mod client;
pub mod server;
pub mod web;

use std::collections::BTreeMap;
use std::fmt;

use ohl_session::collab::Grant;
use ohl_session::versions::Choices;
use ohl_session::{Applied, Command, Kind, Role, Session, SessionError};
use serde::{Deserialize, Serialize};

/// Version of the protocol; client and server must agree.
pub const PROTOCOL: u32 = 2;

/// ALPN of the protocol (its name of before 2026-10-18, kept: an older
/// client and a newer server still agree on it).
pub const ALPN: &[u8] = b"ols/2";

/// Largest message, bytes (an exit clause against garbage).
pub const MAX_MESSAGE: usize = 64 << 20;

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
}

/// Server → client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
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
    if bytes.len() > MAX_MESSAGE {
        return Err(format!("message of {} bytes: too large", bytes.len()));
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stream.write_all(&bytes).await.map_err(|e| e.to_string())
}

/// Reads a message (`None`: the stream ended).
pub async fn read_msg<T: for<'de> Deserialize<'de>>(
    stream: &mut quinn::RecvStream,
) -> Result<Option<T>, String> {
    read_msg_max(stream, MAX_MESSAGE).await
}

/// Reads a message of at most `max` bytes (`None`: the stream ended).
pub async fn read_msg_max<T: for<'de> Deserialize<'de>>(
    stream: &mut quinn::RecvStream,
    max: usize,
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
mod points_tests;
#[cfg(test)]
mod security_tests;
#[cfg(test)]
mod tests;
