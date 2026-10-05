// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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

use ols_session::{Applied, Command, Kind, Role, Session, SessionError};
use serde::{Deserialize, Serialize};

/// Version of the protocol; client and server must agree.
pub const PROTOCOL: u32 = 1;

/// ALPN of the protocol.
pub const ALPN: &[u8] = b"ols/1";

/// Largest message, bytes (an exit clause against garbage).
pub const MAX_MESSAGE: usize = 64 << 20;

/// Largest hello (before the access code is checked), bytes.
pub const MAX_HELLO: usize = 16 << 10;

/// Longest user name, characters.
pub const MAX_NAME: usize = 64;

/// Longest presence selection or lock target, bytes.
pub const MAX_LABEL: usize = 1024;

/// An edit of the project, as requested and as broadcast. Executed by the
/// server and then by every replica, in the server's order ([`execute`]).
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
    /// Save the working line as a project line.
    SaveSnapshot {
        /// Name.
        name: String,
        /// Note.
        note: String,
    },
    /// Delete a project line.
    DeleteSnapshot {
        /// Name.
        name: String,
    },
    /// Set (or remove) a project setting.
    SetSetting {
        /// Key.
        key: String,
        /// Value (`None`: removed).
        value: Option<String>,
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
                Command::MoveStructure { id, .. } | Command::DeleteStructure { id, .. } => {
                    out.push((Kind::Structure, id.clone()));
                }
                Command::DeleteAttachment { structure, .. }
                | Command::RenameAttachment { structure, .. } => {
                    out.push((Kind::Structure, structure.clone()));
                }
                Command::SetStringing { section, .. } => {
                    out.push((Kind::Section, section.clone()));
                }
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
                Command::RenameSystem { .. } => out.push((Kind::Meta, "line".into())),
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
            Op::SaveSnapshot { name, .. } => format!("project line {name} saved"),
            Op::DeleteSnapshot { name } => format!("project line {name} deleted"),
            Op::SetSetting { key, .. } => format!("setting {key}"),
        }
    }
}

/// Executes `op` of `user` on a session: the server's and every
/// replica's code. Returns what changed in the line (`None`: nothing in
/// the line, e.g. a library or a user).
pub fn execute(
    session: &mut Session,
    user: &str,
    op: &Op,
) -> Result<Option<Applied>, SessionError> {
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
        Op::SaveSnapshot { name, note } => {
            session.save_snapshot(user, name, note)?;
            None
        }
        Op::DeleteSnapshot { name } => {
            session.delete_snapshot(user, name)?;
            None
        }
        Op::SetSetting { key, value } => {
            session.require(user, Role::Designer)?;
            session.set_setting(key, value.as_deref())?;
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

/// A soft lock's target: `structure:T2`, `section:S1`…
pub fn lock_target(kind: Kind, id: &str) -> String {
    format!("{}:{id}", kind.as_str())
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
    pub line: ols_line::Line,
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

/// How to join a server: `ols://HOST:PORT/CODE/FINGERPRINT` (the
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
        write!(f, "ols://{}/{}/", self.address, self.code)?;
        for b in self.fingerprint {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Invitation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("not an invitation: {s} (ols://HOST:PORT/CODE/FINGERPRINT)");
        let rest = s.trim().strip_prefix("ols://").ok_or_else(bad)?;
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

/// The access code for `user` (`None`: the general invitation, for users
/// the project does not know): derived from the server's secret, which is
/// never sent.
pub fn access_code(secret: &str, user: Option<&str>) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(secret.as_bytes());
    h.update(b"\n");
    match user {
        Some(u) => h.update(format!("user:{u}").as_bytes()),
        None => h.update(b"*"),
    }
    h.finalize()
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect()
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
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
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
mod tests;
