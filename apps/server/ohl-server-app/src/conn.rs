// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The connection to the network's server: the app's one boundary with
//! the rest of OpenOHL (ROADMAP O17: the admin app talks to the server's
//! protocol only, never to the database). A join on a thread of its own
//! (it blocks), then admin messages asked and their answers kept in
//! [`Data`], read by the pages each frame. Nothing is written to disk: an
//! invitation's access code and a sign-in's token stay in memory.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;

use ohl_server::admin::{
    AdminAsk, AdminMap, AdminReply, AuditFilter, AuditPage, BackupList, ExplainView, IdentityRow,
    ImportView, InvitationText, LibraryAction, LibraryKindRow, LibraryPage, LineRow, LogLine,
    MapDot, Overview, PinsView, RightsView, SeeAsView, SessionRow, SettingRow, SharedDetail,
    SharedList, TwinRow, UseRow, WhatChanges,
};
use ohl_server::client::{Incoming, Joined, Link, NetJoin};
use ohl_server::{Invitation, NetworkInfo, ServerMsg};
use ohl_session::rights::Permission;
use ohl_session::rights::RightsCommand;

/// Log lines the app keeps (the newest).
pub const LOG_KEPT: usize = 5000;

/// What an answer is for.
#[derive(Debug, Clone, PartialEq)]
// A few waiting at a time: the change kept in place.
#[allow(clippy::large_enum_variant)]
enum Want {
    Overview,
    Sessions,
    Log,
    Backups,
    Settings,
    Rights,
    Map,
    Dots([f64; 4]),
    Route(String, String),
    Libraries,
    SeeAs,
    Preview(RightsCommand),
    /// A change or an action: its answer is said.
    Act(String),
    Invitation,
    // Stages C to F.
    Shared,
    SharedTower,
    Twins(String),
    LibraryPage,
    Uses(String, String),
    Pins,
    Lines,
    /// A line's export being written (then fetched).
    Export,
    /// A piece of it.
    Piece,
    /// An upload's start, a piece of it.
    UploadStart,
    UploadPiece,
    /// The upload imported.
    Import,
    /// A page of the audit (for the CSV export: true).
    Audit(bool),
    Identities,
    Explain,
    /// Its answer said nothing (a transfer ended).
    Quiet,
}

/// A line's file being fetched into a file of this computer.
#[derive(Debug)]
pub struct Download {
    /// The server's token.
    pub token: String,
    /// The line.
    pub line: String,
    /// The file chosen (written as `….part` until whole).
    pub path: PathBuf,
    part: PathBuf,
    file: Option<std::fs::File>,
    /// Its size, bytes (0: being written on the server).
    pub bytes: u64,
    /// Bytes received.
    pub got: u64,
    /// What it holds, in words.
    pub note: String,
}

/// A project file of this computer being sent to be imported.
#[derive(Debug)]
pub struct Upload {
    /// The server's token (none: not started yet).
    pub token: Option<String>,
    /// The file.
    pub path: PathBuf,
    file: std::fs::File,
    /// Its size, bytes.
    pub bytes: u64,
    /// Bytes sent and received by the server.
    pub sent: u64,
    /// The new line's code, its name, the twin distance.
    pub code: String,
    /// See `code`.
    pub name: Option<String>,
    /// See `code`.
    pub tolerance: Option<f64>,
    /// Being imported (the whole file received).
    pub importing: bool,
}

/// The audit's CSV export under way (every page of the filter, oldest
/// last, written as they come).
#[derive(Debug)]
pub struct AuditExport {
    /// The file.
    pub path: PathBuf,
    file: std::fs::File,
    filter: AuditFilter,
    /// Rows written.
    pub rows: u64,
    /// Rows in all.
    pub total: u64,
}

/// What the server answered, as the pages show it.
#[derive(Debug, Default)]
pub struct Data {
    /// The network, as the welcome said.
    pub info: Option<NetworkInfo>,
    /// The Overview.
    pub overview: Option<Overview>,
    /// Who is connected.
    pub sessions: Vec<SessionRow>,
    /// The server's log (kept here, the newest [`LOG_KEPT`] lines).
    pub log: Vec<LogLine>,
    /// The last log line asked for.
    pub log_last: u64,
    /// The log's filter the lines were asked with.
    pub log_filter: String,
    /// The backups.
    pub backups: Option<BackupList>,
    /// The settings.
    pub settings: Vec<SettingRow>,
    /// The rights the user manages.
    pub rights: Option<RightsView>,
    /// The map.
    pub map: Option<AdminMap>,
    /// The structures of the area last asked for.
    pub dots: Vec<MapDot>,
    /// That area.
    pub dots_area: Option<[f64; 4]>,
    /// The structures of routes (the assignments' tree), by (line, route).
    pub route_structures: BTreeMap<(String, String), Vec<MapDot>>,
    /// The country libraries.
    pub libraries: Option<Vec<LibraryKindRow>>,
    /// "See as user".
    pub see_as: Option<SeeAsView>,
    /// "What changes" of a change shown before saving.
    pub preview: Option<(RightsCommand, WhatChanges)>,
    /// An invitation made (shown once).
    pub invitation: Option<InvitationText>,
    /// What changes and actions said (text, an error?), newest last.
    pub said: Vec<(String, bool)>,
    /// Long work or changes waiting for their answer (in words).
    pub busy: Vec<String>,
    /// Stage C: the shared towers, the one picked, a line's twins.
    pub shared: Option<SharedList>,
    /// See `shared`.
    pub shared_tower: Option<SharedDetail>,
    /// See `shared`: the line and its twins.
    pub twins: Option<(String, Vec<TwinRow>)>,
    /// Stage D: the Libraries page.
    pub library: Option<LibraryPage>,
    /// Where an entry is used: (kind, entry), the uses.
    pub uses: Option<((String, String), Vec<UseRow>)>,
    /// Update library pins proposed.
    pub pins: Option<PinsView>,
    /// Stage E: the lines.
    pub lines: Option<Vec<LineRow>>,
    /// A line being fetched.
    pub download: Option<Download>,
    /// A file being sent and imported.
    pub upload: Option<Upload>,
    /// The last import's report.
    pub imported: Option<ImportView>,
    /// Stage F: the audit page shown, and the rows it was asked before.
    pub audit: Option<AuditPage>,
    /// See `audit`.
    pub audit_before: Option<i64>,
    /// The audit's CSV export under way.
    pub audit_export: Option<AuditExport>,
    /// The identities mapped.
    pub identities: Option<Vec<IdentityRow>>,
    /// "Explain".
    pub explain: Option<ExplainView>,
}

impl Data {
    /// The server speaks the admin messages of stages C to F.
    pub fn stages_cf(&self) -> bool {
        self.info.as_ref().is_some_and(|i| i.admin >= 2)
            || self.overview.as_ref().is_some_and(|o| o.admin >= 2)
    }

    /// The user is a network admin (the Overview says so).
    pub fn network_admin(&self) -> bool {
        self.overview.as_ref().is_some_and(|o| o.full)
            || self.rights.as_ref().is_some_and(|r| r.network_admin)
    }

    /// The last thing said, if any.
    pub fn last_said(&self) -> Option<&(String, bool)> {
        self.said.last()
    }
}

/// How to join (connect mode, or the app's own server in host mode).
#[derive(Clone)]
pub struct JoinHow {
    /// The invitation (address, the certificate's fingerprint, the access
    /// code — empty with a token).
    pub invitation: Invitation,
    /// The user.
    pub user: String,
    /// An ID token of the network's identity provider (OIDC), in place of
    /// the access code.
    pub id_token: Option<String>,
}

impl std::fmt::Debug for JoinHow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinHow")
            .field("address", &self.invitation.address)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

/// A join being made (on its thread).
pub struct Joining {
    rx: mpsc::Receiver<Result<Joined, String>>,
    how: JoinHow,
}

impl Joining {
    /// Starts joining (following nothing: an admin app).
    pub fn start(how: JoinHow) -> Joining {
        let (tx, rx) = mpsc::channel();
        let h = how.clone();
        std::thread::Builder::new()
            .name("ohl-server-app-join".into())
            .spawn(move || {
                let join = NetJoin {
                    id_token: h.id_token.clone(),
                    ..NetJoin::default()
                };
                let r =
                    ohl_server::client::join_network(&h.invitation, &h.user, &join, &mut |_| {});
                let _ = tx.send(r);
            })
            .expect("a thread");
        Joining { rx, how }
    }

    /// The connection once joined; `Some(Err)` when refused.
    pub fn poll(&self) -> Option<Result<Conn, String>> {
        match self.rx.try_recv() {
            Ok(Ok(j)) => Some(Ok(Conn::new(j.link, self.how.clone()))),
            Ok(Err(e)) => Some(Err(e)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the join stopped".into())),
        }
    }
}

/// The connection: asks admin messages and keeps their answers.
pub struct Conn {
    link: Link,
    how: JoinHow,
    waiting: HashMap<u64, Want>,
    /// What the server answered.
    pub data: Data,
    /// The connection ended: why.
    pub closed: Option<String>,
    /// The server said its user's rights changed (it lets the connection
    /// go: the app joins again).
    pub rights_changed: bool,
}

impl Conn {
    fn new(link: Link, how: JoinHow) -> Conn {
        Conn {
            link,
            how,
            waiting: HashMap::new(),
            data: Data::default(),
            closed: None,
            rights_changed: false,
        }
    }

    /// The user signed in.
    pub fn user(&self) -> &str {
        &self.how.user
    }

    /// The server's address.
    pub fn address(&self) -> &str {
        &self.how.invitation.address
    }

    /// The invitation of `code` for this server (its address and
    /// certificate): what an admin passes on to a user.
    pub fn invitation_of(&self, code: &str) -> Invitation {
        Invitation {
            code: code.to_owned(),
            ..self.how.invitation.clone()
        }
    }

    fn send(&mut self, ask: AdminAsk, want: Want) {
        if self.closed.is_some() {
            return;
        }
        match self.link.admin(ask) {
            Ok(id) => {
                if let Want::Act(what) = &want {
                    self.data.busy.push(what.clone());
                }
                self.waiting.insert(id, want);
            }
            Err(e) => self.closed = Some(e),
        }
    }

    /// Whether an answer of this kind is awaited.
    fn awaits(&self, want: &Want) -> bool {
        self.waiting.values().any(|w| w == want)
    }

    /// Asks for the Overview (unless asked already).
    pub fn ask_overview(&mut self) {
        if !self.awaits(&Want::Overview) {
            self.send(AdminAsk::Overview, Want::Overview);
        }
    }

    /// Asks for the sessions.
    pub fn ask_sessions(&mut self) {
        if !self.awaits(&Want::Sessions) {
            self.send(AdminAsk::Sessions, Want::Sessions);
        }
    }

    /// Asks for the log after what is held, with `filter` (a new filter
    /// starts again).
    pub fn ask_log(&mut self, filter: &str) {
        if filter != self.data.log_filter {
            self.data.log.clear();
            self.data.log_last = 0;
            self.data.log_filter = filter.to_owned();
        }
        if !self.awaits(&Want::Log) {
            self.send(
                AdminAsk::Log {
                    after: self.data.log_last,
                    filter: filter.to_owned(),
                },
                Want::Log,
            );
        }
    }

    /// Asks for the backups.
    pub fn ask_backups(&mut self) {
        if !self.awaits(&Want::Backups) {
            self.send(AdminAsk::Backups, Want::Backups);
        }
    }

    /// Asks for the settings.
    pub fn ask_settings(&mut self) {
        if !self.awaits(&Want::Settings) {
            self.send(AdminAsk::Settings, Want::Settings);
        }
    }

    /// Asks for the rights.
    pub fn ask_rights(&mut self) {
        if !self.awaits(&Want::Rights) {
            self.send(AdminAsk::Rights, Want::Rights);
        }
    }

    /// Asks for the map.
    pub fn ask_map(&mut self) {
        if !self.awaits(&Want::Map) {
            self.send(AdminAsk::Map, Want::Map);
        }
    }

    /// Asks for the structures of area `b` (unless that area is held or
    /// asked).
    pub fn ask_dots(&mut self, b: [f64; 4]) {
        if self.data.dots_area == Some(b) || self.awaits(&Want::Dots(b)) {
            return;
        }
        self.send(AdminAsk::Structures { bounds: b }, Want::Dots(b));
    }

    /// Asks for a route's structures (the tree).
    pub fn ask_route(&mut self, model: &str, route: &str) {
        let key = (model.to_owned(), route.to_owned());
        if self.data.route_structures.contains_key(&key)
            || self.awaits(&Want::Route(key.0.clone(), key.1.clone()))
        {
            return;
        }
        self.send(
            AdminAsk::RouteStructures {
                model: key.0.clone(),
                route: key.1.clone(),
            },
            Want::Route(key.0, key.1),
        );
    }

    /// Asks for the country libraries.
    pub fn ask_libraries(&mut self) {
        if !self.awaits(&Want::Libraries) {
            self.send(AdminAsk::Libraries, Want::Libraries);
        }
    }

    /// Asks what `user` sees.
    pub fn ask_see_as(&mut self, user: &str) {
        self.send(
            AdminAsk::SeeAs {
                user: user.to_owned(),
            },
            Want::SeeAs,
        );
    }

    /// Asks what a change would change (shown before it is saved).
    pub fn ask_preview(&mut self, command: RightsCommand) {
        self.data.preview = None;
        self.send(
            AdminAsk::Preview {
                command: command.clone(),
            },
            Want::Preview(command),
        );
    }

    /// Saves a rights change (the server's one function decides).
    pub fn change(&mut self, command: RightsCommand) {
        let what = format!("the change ({})", command.action());
        self.send(AdminAsk::Change { command }, Want::Act(what));
    }

    /// Invites a user (added if new), or reissues their invitation.
    pub fn invite(&mut self, name: &str, display: Option<String>, days: Option<u32>, again: bool) {
        self.data.invitation = None;
        let ask = if again {
            AdminAsk::Reissue {
                name: name.to_owned(),
                days,
            }
        } else {
            AdminAsk::Invite {
                name: name.to_owned(),
                display,
                days,
            }
        };
        self.send(ask, Want::Invitation);
    }

    /// Lets a connection go, with a reason.
    pub fn disconnect(&mut self, client: u64, reason: &str) {
        self.send(
            AdminAsk::Disconnect {
                client,
                reason: reason.to_owned(),
            },
            Want::Act("letting the connection go".into()),
        );
    }

    /// A backup made now (in the server's backup folder).
    pub fn backup_now(&mut self) {
        self.send(AdminAsk::BackupNow, Want::Act("the backup".into()));
    }

    /// The restore drill of backup `name`.
    pub fn restore_drill(&mut self, name: &str) {
        self.send(
            AdminAsk::RestoreDrill {
                backup: name.to_owned(),
            },
            Want::Act(format!("the restore drill of {name}")),
        );
    }

    /// Takes what came; true when anything did (the pages are drawn
    /// again).
    pub fn poll(&mut self) -> bool {
        let mut any = false;
        while let Some(m) = self.link.try_recv() {
            any = true;
            match m {
                Incoming::Closed(why) => {
                    self.closed = Some(why);
                    break;
                }
                Incoming::Msg(msg) => self.take(msg),
            }
        }
        any
    }

    fn take(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Network(info) => {
                if info.admin < ohl_server::admin::ADMIN_MIN {
                    self.closed = Some(
                        "this server is older than OHL-Server's admin messages: update the \
                         server (ohl network serve) to the program of this app"
                            .into(),
                    );
                }
                self.data.info = Some(*info);
            }
            ServerMsg::Notice { text } => {
                if text.contains("rights on this network changed") {
                    self.rights_changed = true;
                }
                self.data.said.push((text, true));
            }
            ServerMsg::Refused { reason } => self.closed = Some(reason),
            ServerMsg::Admin { id, result } => {
                let Some(want) = self.waiting.remove(&id) else {
                    return;
                };
                if let Want::Act(what) = &want {
                    self.data.busy.retain(|b| b != what);
                }
                match result {
                    Ok(r) => self.answer(want, r),
                    Err(e) => {
                        match want {
                            Want::Preview(_) => self.data.preview = None,
                            Want::Export | Want::Piece => self.end_download(),
                            Want::UploadStart | Want::UploadPiece | Want::Import => {
                                self.data.upload = None;
                            }
                            Want::Audit(true) => self.data.audit_export = None,
                            _ => {}
                        }
                        self.data.said.push((e, true));
                    }
                }
            }
            _ => {}
        }
    }

    fn answer(&mut self, want: Want, reply: AdminReply) {
        let d = &mut self.data;
        match (want, reply) {
            (_, AdminReply::SharedTowers(s)) => d.shared = Some(s),
            (_, AdminReply::SharedTower(s)) => d.shared_tower = Some(*s),
            (Want::Twins(line), AdminReply::Twins(t)) => d.twins = Some((line, t)),
            (_, AdminReply::Twins(_)) => {}
            (_, AdminReply::LibraryPage(p)) => d.library = Some(*p),
            (Want::Uses(k, e), AdminReply::Uses(u)) => d.uses = Some(((k, e), u)),
            (_, AdminReply::Uses(_)) => {}
            (_, AdminReply::Pins(p)) => d.pins = Some(*p),
            (_, AdminReply::Lines(l)) => d.lines = Some(l),
            (Want::Export, AdminReply::Transfer(t)) => self.export_ready(t),
            (Want::UploadStart, AdminReply::Transfer(t)) => {
                if let Some(u) = &mut self.data.upload {
                    u.token = Some(t.token);
                }
                self.send_piece();
            }
            (_, AdminReply::Transfer(_)) => {}
            (_, AdminReply::Chunk(c)) => self.take_chunk(c),
            (_, AdminReply::Received(n)) => {
                if let Some(u) = &mut self.data.upload {
                    u.sent = n;
                }
                self.send_piece();
            }
            (_, AdminReply::Imported(v)) => {
                let line = v.line.clone();
                d.upload = None;
                d.said.push((format!("line {line} imported"), false));
                d.imported = Some(*v);
                self.waiting
                    .retain(|_, w| *w != Want::Lines && *w != Want::Map);
                self.ask_lines();
                self.ask_map();
            }
            (Want::Audit(true), AdminReply::Audit(page)) => self.take_audit_export(page),
            (_, AdminReply::Audit(page)) => d.audit = Some(page),
            (_, AdminReply::Identities(i)) => d.identities = Some(i),
            (_, AdminReply::Explain(e)) => d.explain = Some(e),
            (_, AdminReply::Overview(o)) => d.overview = Some(*o),
            (_, AdminReply::Sessions(s)) => d.sessions = s,
            (_, AdminReply::Log(page)) => {
                d.log_last = page.last;
                d.log.extend(page.lines);
                if d.log.len() > LOG_KEPT {
                    let cut = d.log.len() - LOG_KEPT;
                    d.log.drain(..cut);
                }
            }
            (_, AdminReply::Backups(b)) => d.backups = Some(b),
            (_, AdminReply::Settings(s)) => d.settings = s,
            (_, AdminReply::Rights(r)) => d.rights = Some(*r),
            (_, AdminReply::Map(m)) => d.map = Some(*m),
            (Want::Dots(b), AdminReply::Structures(s)) => {
                d.dots = s;
                d.dots_area = Some(b);
            }
            (Want::Route(m, r), AdminReply::Structures(s)) => {
                d.route_structures.insert((m, r), s);
            }
            (_, AdminReply::Structures(_)) => {}
            (_, AdminReply::Libraries(l)) => d.libraries = Some(l),
            (_, AdminReply::SeeAs(s)) => d.see_as = Some(*s),
            (Want::Preview(c), AdminReply::Preview(w)) => d.preview = Some((c, *w)),
            (_, AdminReply::Preview(_)) => {}
            (_, AdminReply::Invitation(i)) => {
                d.invitation = Some(i);
                // The user invited is listed.
                self.waiting.retain(|_, w| *w != Want::Rights);
                self.ask_rights();
            }
            (Want::Quiet, AdminReply::Done(_)) => {}
            (want, AdminReply::Done(text)) => {
                d.said.push((text, false));
                // What a change changes is shown again.
                if let Want::Act(what) = &want {
                    let setting = what.starts_with("setting ");
                    let admin = self.data.network_admin();
                    let cf = self.data.stages_cf();
                    self.waiting.retain(|_, w| {
                        !matches!(
                            w,
                            Want::Rights | Want::Map | Want::LibraryPage | Want::Lines
                        )
                    });
                    self.ask_rights();
                    self.ask_map();
                    if admin {
                        self.ask_backups();
                        if setting {
                            self.waiting.retain(|_, w| *w != Want::Settings);
                            self.ask_settings();
                        }
                        if cf {
                            self.ask_identities();
                        }
                    }
                    if cf {
                        self.ask_library_page();
                        self.ask_lines();
                        self.data.pins = None;
                    }
                }
            }
        }
    }
}

/// Stages C to F.
impl Conn {
    /// Asks for the shared towers.
    pub fn ask_shared(&mut self) {
        if !self.awaits(&Want::Shared) {
            self.send(AdminAsk::SharedTowers, Want::Shared);
        }
    }

    /// Asks for one shared tower (who may edit it).
    pub fn ask_shared_tower(&mut self, model: &str, id: &str) {
        self.data.shared_tower = None;
        self.send(
            AdminAsk::SharedTower {
                model: model.to_owned(),
                id: id.to_owned(),
            },
            Want::SharedTower,
        );
    }

    /// Asks for a line's twin towers.
    pub fn ask_twins(&mut self, model: &str, tolerance: Option<f64>) {
        self.data.twins = None;
        self.send(
            AdminAsk::Twins {
                model: model.to_owned(),
                tolerance,
            },
            Want::Twins(model.to_owned()),
        );
    }

    /// Asks for the Libraries page.
    pub fn ask_library_page(&mut self) {
        if !self.awaits(&Want::LibraryPage) {
            self.send(AdminAsk::LibraryPage, Want::LibraryPage);
        }
    }

    /// Asks where an entry is used.
    pub fn ask_uses(&mut self, kind: &str, entry: &str) {
        self.data.uses = None;
        self.send(
            AdminAsk::Uses {
                kind: kind.to_owned(),
                entry: entry.to_owned(),
            },
            Want::Uses(kind.to_owned(), entry.to_owned()),
        );
    }

    /// A maintainer's library change.
    pub fn library(&mut self, action: LibraryAction) {
        let what = format!("the library change ({})", action.describe());
        self.send(AdminAsk::Library { action }, Want::Act(what));
    }

    /// Asks what Update library pins would do to `line`.
    pub fn ask_pins(&mut self, line: &str) {
        self.data.pins = None;
        self.send(
            AdminAsk::PinsPlan {
                line: line.to_owned(),
                kinds: Vec::new(),
            },
            Want::Pins,
        );
    }

    /// Update library pins of `line`.
    pub fn update_pins(&mut self, line: &str) {
        self.send(
            AdminAsk::UpdatePins {
                line: line.to_owned(),
                kinds: Vec::new(),
            },
            Want::Act(format!("updating the library pins of line {line}")),
        );
    }

    /// Asks for the lines.
    pub fn ask_lines(&mut self) {
        if !self.awaits(&Want::Lines) {
            self.send(AdminAsk::Lines, Want::Lines);
        }
    }

    /// Exports line `line` into file `path` of this computer (written on
    /// the server, then fetched in pieces).
    pub fn export_line(&mut self, line: &str, path: PathBuf) {
        if self.data.download.is_some() {
            self.data
                .said
                .push(("a line is being fetched already".into(), true));
            return;
        }
        let mut part = path.clone().into_os_string();
        part.push(".part");
        self.data.download = Some(Download {
            token: String::new(),
            line: line.to_owned(),
            path,
            part: part.into(),
            file: None,
            bytes: 0,
            got: 0,
            note: String::new(),
        });
        self.send(
            AdminAsk::ExportLine {
                model: line.to_owned(),
            },
            Want::Export,
        );
    }

    fn export_ready(&mut self, t: ohl_server::admin::TransferInfo) {
        let Some(d) = &mut self.data.download else {
            return;
        };
        let file = std::fs::File::create(&d.part);
        match file {
            Ok(f) => {
                d.file = Some(f);
                d.token = t.token;
                d.bytes = t.bytes;
                d.note = t.note;
                let token = d.token.clone();
                self.send(AdminAsk::Download { token, offset: 0 }, Want::Piece);
            }
            Err(e) => {
                let what = format!("{}: {e}", d.part.display());
                self.send(AdminAsk::TransferDone { token: t.token }, Want::Quiet);
                self.end_download();
                self.data.said.push((what, true));
            }
        }
    }

    fn take_chunk(&mut self, c: ohl_server::admin::ChunkData) {
        let Some(d) = &mut self.data.download else {
            return;
        };
        let written = ohl_server::base64::decode(&c.data).and_then(|b| {
            if c.offset != d.got {
                return Err("a piece out of order".into());
            }
            d.file
                .as_mut()
                .ok_or_else(|| "the file is closed".to_owned())?
                .write_all(&b)
                .map_err(|e| format!("{}: {e}", d.part.display()))?;
            d.got += b.len() as u64;
            Ok(())
        });
        if let Err(e) = written {
            self.end_download();
            self.data.said.push((e, true));
            return;
        }
        let (token, got) = (d.token.clone(), d.got);
        if !c.last {
            self.send(AdminAsk::Download { token, offset: got }, Want::Piece);
            return;
        }
        let finished = d
            .file
            .take()
            .map_or(Ok(()), |f| f.sync_all())
            .and_then(|()| std::fs::rename(&d.part, &d.path));
        let said = match finished {
            Ok(()) => (
                format!(
                    "line {} written to {}: {}",
                    d.line,
                    d.path.display(),
                    d.note
                ),
                false,
            ),
            Err(e) => (format!("{}: {e}", d.path.display()), true),
        };
        self.data.download = None;
        self.data.said.push(said);
        self.send(AdminAsk::TransferDone { token }, Want::Quiet);
    }

    /// The line being fetched stopped (its part file removed).
    pub fn end_download(&mut self) {
        if let Some(mut d) = self.data.download.take() {
            d.file = None;
            let _ = std::fs::remove_file(&d.part);
            if !d.token.is_empty() {
                self.send(AdminAsk::TransferDone { token: d.token }, Want::Quiet);
            }
        }
    }

    /// Sends project file `path` and imports it as line `code`.
    pub fn import_line(
        &mut self,
        path: PathBuf,
        code: &str,
        name: Option<String>,
        tolerance: Option<f64>,
    ) {
        if self.data.upload.is_some() {
            self.data
                .said
                .push(("a file is being sent already".into(), true));
            return;
        }
        let opened = std::fs::File::open(&path).and_then(|f| Ok((f.metadata()?.len(), f)));
        let (bytes, file) = match opened {
            Ok(x) => x,
            Err(e) => {
                self.data
                    .said
                    .push((format!("{}: {e}", path.display()), true));
                return;
            }
        };
        let name_of = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.data.imported = None;
        self.data.upload = Some(Upload {
            token: None,
            path,
            file,
            bytes,
            sent: 0,
            code: code.to_owned(),
            name,
            tolerance,
            importing: false,
        });
        self.send(
            AdminAsk::UploadStart {
                name: name_of,
                bytes,
            },
            Want::UploadStart,
        );
    }

    /// The upload's next piece, or, all sent, the import.
    fn send_piece(&mut self) {
        let Some(u) = &mut self.data.upload else {
            return;
        };
        let Some(token) = u.token.clone() else {
            return;
        };
        if u.sent >= u.bytes {
            u.importing = true;
            let ask = AdminAsk::ImportLine {
                token,
                code: u.code.clone(),
                name: u.name.clone(),
                tolerance: u.tolerance,
            };
            self.send(ask, Want::Import);
            return;
        }
        let want = ohl_server::admin::CHUNK.min((u.bytes - u.sent) as usize);
        let mut buf = vec![0u8; want];
        if let Err(e) = u.file.read_exact(&mut buf) {
            let what = format!("{}: {e}", u.path.display());
            self.data.upload = None;
            self.send(AdminAsk::TransferDone { token }, Want::Quiet);
            self.data.said.push((what, true));
            return;
        }
        let offset = u.sent;
        self.send(
            AdminAsk::Upload {
                token,
                offset,
                data: ohl_server::base64::encode(&buf),
            },
            Want::UploadPiece,
        );
    }

    /// Asks for a page of the audit (rows older than `before`).
    pub fn ask_audit(&mut self, filter: &AuditFilter, before: Option<i64>, limit: usize) {
        self.data.audit_before = before;
        self.send(
            AdminAsk::Audit {
                filter: filter.clone(),
                before,
                limit,
            },
            Want::Audit(false),
        );
    }

    /// Writes every row of `filter` to CSV file `path` (paged).
    pub fn export_audit(&mut self, filter: &AuditFilter, path: PathBuf) {
        let file = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                self.data
                    .said
                    .push((format!("{}: {e}", path.display()), true));
                return;
            }
        };
        self.data.audit_export = Some(AuditExport {
            path,
            file,
            filter: filter.clone(),
            rows: 0,
            total: 0,
        });
        self.send(
            AdminAsk::Audit {
                filter: filter.clone(),
                before: None,
                limit: ohl_server::admin::AUDIT_PAGE_MAX,
            },
            Want::Audit(true),
        );
    }

    fn take_audit_export(&mut self, page: AuditPage) {
        let Some(x) = &mut self.data.audit_export else {
            return;
        };
        let csv = ohl_server::admin::audit_csv(&page.rows, x.rows == 0);
        if let Err(e) = x.file.write_all(csv.as_bytes()) {
            let what = format!("{}: {e}", x.path.display());
            self.data.audit_export = None;
            self.data.said.push((what, true));
            return;
        }
        x.rows += page.rows.len() as u64;
        x.total = page.total;
        match page.rows.last() {
            Some(last) if page.more => {
                let ask = AdminAsk::Audit {
                    filter: x.filter.clone(),
                    before: Some(last.id),
                    limit: ohl_server::admin::AUDIT_PAGE_MAX,
                };
                self.send(ask, Want::Audit(true));
            }
            _ => {
                let said = format!("{} audit row(s) written to {}", x.rows, x.path.display());
                self.data.audit_export = None;
                self.data.said.push((said, false));
            }
        }
    }

    /// Asks for the identities mapped.
    pub fn ask_identities(&mut self) {
        if !self.awaits(&Want::Identities) {
            self.send(AdminAsk::Identities, Want::Identities);
        }
    }

    /// Asks why `user` may or may not.
    pub fn ask_explain(
        &mut self,
        user: &str,
        permission: Permission,
        line: Option<String>,
        kind: Option<String>,
        id: Option<String>,
    ) {
        self.data.explain = None;
        self.send(
            AdminAsk::Explain {
                user: user.to_owned(),
                permission,
                line,
                kind,
                id,
            },
            Want::Explain,
        );
    }

    /// A network setting changed.
    pub fn set_setting(&mut self, key: &str, value: &str) {
        self.send(
            AdminAsk::SetSetting {
                key: key.to_owned(),
                value: value.to_owned(),
            },
            Want::Act(format!("setting {key}")),
        );
    }
}
