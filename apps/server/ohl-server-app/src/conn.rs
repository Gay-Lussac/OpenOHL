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
use std::sync::mpsc;

use ohl_server::admin::{
    AdminAsk, AdminMap, AdminReply, BackupList, InvitationText, LibraryKindRow, LogLine, MapDot,
    Overview, RightsView, SeeAsView, SessionRow, SettingRow, WhatChanges,
};
use ohl_server::client::{Incoming, Joined, Link, NetJoin};
use ohl_server::{Invitation, NetworkInfo, ServerMsg};
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
}

/// What the server answered, as the pages show it.
#[derive(Debug, Clone, Default)]
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
}

impl Data {
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
}

impl Conn {
    fn new(link: Link, how: JoinHow) -> Conn {
        Conn {
            link,
            how,
            waiting: HashMap::new(),
            data: Data::default(),
            closed: None,
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
                if info.admin < ohl_server::admin::ADMIN {
                    self.closed = Some(
                        "this server is older than OHL-Server's admin messages: update the \
                         server (ohl network serve) to the program of this app"
                            .into(),
                    );
                }
                self.data.info = Some(*info);
            }
            ServerMsg::Notice { text } => self.data.said.push((text, true)),
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
                        if let Want::Preview(_) = want {
                            self.data.preview = None;
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
            (_, AdminReply::Invitation(i)) => d.invitation = Some(i),
            (want, AdminReply::Done(text)) => {
                d.said.push((text, false));
                // What a change changes is shown again.
                if matches!(want, Want::Act(_)) {
                    let admin = self.data.network_admin();
                    self.waiting
                        .retain(|_, w| *w != Want::Rights && *w != Want::Map);
                    self.ask_rights();
                    self.ask_map();
                    if admin {
                        self.ask_backups();
                    }
                }
            }
        }
    }
}
