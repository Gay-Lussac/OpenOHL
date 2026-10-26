// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Server mode in the application (ADR-0005, alpha): host the open
//! project, join a server, work together.
//!
//! A joined project is a **replica**: the project file received from the
//! server, opened like any project, kept up to date by applying the edits
//! the server broadcasts, in its order ([`ohl_server::client::replay`]).
//! Edits are requests to the server ([`Model::remote_request`]): the
//! application waits for the answer (a few ms on a network, at most 15 s),
//! applying what others did meanwhile, then re-solves what changed — like
//! a local edit. Others' edits arrive between frames ([`Model::poll_remote`]).
//!
//! Hosting starts a server on the project file in this process and joins
//! it like any client. People (a page) lists who is connected — their
//! role, selection, where they look — and the invitation to give others;
//! admins change roles there. A structure in someone's Move window is
//! theirs (a soft lock) until they close it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_server::client::{Incoming, Joined, Progress};
use ohl_server::{ClientMsg, Invitation, Op, Peer, Presence, ServerMsg};
use ohl_session::{Applied, Role};

use crate::edit::EditError;
use crate::model::{Editing, Model};

/// The connection of a joined project.
pub struct Remote {
    /// The link to the server.
    pub link: ohl_server::client::Link,
    /// The invitation (to give others).
    pub invitation: Invitation,
    /// This user's role.
    pub role: Role,
    /// Who is connected.
    pub peers: Vec<Peer>,
    /// Soft locks: target → user.
    pub locks: BTreeMap<String, String>,
    /// The locks this user asked for.
    held: BTreeSet<String>,
    /// The connection ended (with why): read-only from then on.
    pub closed: Option<String>,
    /// The presence last sent, and when.
    sent: (Presence, Option<Instant>),
    /// This application hosts the server.
    pub hosting: bool,
    /// Hosting: the server's keys (the invitations are made from
    /// it).
    pub secret: Option<ohl_server::Keys>,
    /// The name typed for a personal invitation (People).
    pub invite_name: String,
    /// Hosting with the web client: its address, `https://HOST:PORT/`.
    pub web: Option<String>,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote")
            .field("address", &self.link.address)
            .field("user", &self.link.user)
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

impl Remote {
    /// From a join.
    pub fn new(joined: Joined, invitation: Invitation, hosting: bool) -> Self {
        Self {
            role: joined.role,
            peers: joined.peers,
            locks: joined.locks,
            link: joined.link,
            invitation,
            held: BTreeSet::new(),
            closed: None,
            sent: (Presence::default(), None),
            hosting,
            secret: None,
            invite_name: String::new(),
            web: None,
        }
    }
}

/// Longest wait for the server's answer to an edit (an exit clause).
const ANSWER_TIMEOUT: Duration = Duration::from_secs(15);

/// One `Applied` for several (the sections of all, the last log entry).
fn merge(applied: Vec<Applied>) -> Option<Applied> {
    let mut it = applied.into_iter();
    let mut first = it.next()?;
    for a in it {
        first.seq = a.seq;
        first.objects.extend(a.objects);
        first.points.extend(a.points);
        for s in a.sections {
            if !first.sections.contains(&s) {
                first.sections.push(s);
            }
        }
    }
    Some(first)
}

impl Model {
    /// Joined: edits go to the server from now on.
    pub fn attach(&mut self, remote: Remote) {
        if remote.role == Role::Viewer {
            self.editing = Err(format!("{} is a viewer of this project", remote.link.user));
        }
        self.remote = Some(remote);
    }

    /// The project's session: the editing one, else the one kept for a
    /// read-only replica.
    fn replica(&mut self) -> Option<&mut ohl_session::Session> {
        match &mut self.editing {
            Ok(e) => Some(&mut e.session),
            Err(_) => self.viewer_session.as_mut(),
        }
    }

    /// The user's role here (`None`: a line file, or no role).
    pub fn my_role(&self) -> Option<Role> {
        if let Some(r) = &self.remote {
            return Some(r.role);
        }
        let e = self.editing.as_ref().ok()?;
        e.session.role(&e.user).ok().flatten()
    }

    /// The user's role changed (by an admin): editing or not.
    fn role_changed(&mut self, role: Role) {
        let Some(r) = &mut self.remote else {
            return;
        };
        r.role = role;
        let user = r.link.user.clone();
        match (role, self.editing.is_ok()) {
            (Role::Viewer, true) => {
                if let Ok(e) = std::mem::replace(
                    &mut self.editing,
                    Err(format!("{user} is a viewer of this project")),
                ) {
                    self.viewer_session = Some(e.session);
                }
            }
            (Role::Designer | Role::Admin, false) => {
                if let Some(session) = self.viewer_session.take() {
                    self.editing = Ok(Editing { session, user });
                }
            }
            _ => {}
        }
    }

    /// Takes one message of the server: an edit is applied to the replica
    /// (its line changes into `applied`; what others did into `others`);
    /// returns the answer to request `id` when it is one.
    fn take(
        &mut self,
        msg: ServerMsg,
        id: Option<u64>,
        applied: &mut Vec<Applied>,
        others: &mut Vec<String>,
    ) -> Option<Result<String, String>> {
        match msg {
            ServerMsg::Applied {
                user,
                op,
                line,
                seq,
            } => {
                let me = self.remote.as_ref().map(|r| r.link.user.clone());
                let session = self.replica()?;
                let result = ohl_server::client::replay(session, &user, &line, &op, seq);
                let on_mine = line == session.current_line();
                match result {
                    // The line worked on changed; another line shown is
                    // read again.
                    Ok(a) if on_mine => applied.extend(a),
                    Ok(_) => self.lines_changed.push(line.clone()),
                    Err(e) => {
                        if let Some(r) = &mut self.remote {
                            r.closed = Some(e);
                        }
                    }
                }
                // The line worked on renamed or deleted by someone else:
                // follow it (renamed) or work on the one the replica is on.
                let now = self.session().map(|s| s.current_line().to_owned());
                if let (Some(r), Some(now)) = (&mut self.remote, now)
                    && r.link.line != now
                {
                    others.push(format!(
                        "{user}: {} — you work on project line {now} now",
                        op.describe()
                    ));
                    r.link.line = now;
                    self.lines_changed.push(String::new());
                }
                if let Op::SetUser { name, role } = &op
                    && me.as_deref() == Some(name.as_str())
                {
                    self.role_changed(*role);
                }
                // The surface of the checks (read when a check needs it).
                if let Op::SetSetting { key, value } = &op
                    && key == crate::surfaces::CHECK_SETTING
                    && *value != self.check_surface
                {
                    self.check_surface = value.clone();
                    self.check_ground = None;
                    self.revisions.ground = crate::revision::fresh();
                }
                // The drawing settings of a project line, changed by
                // someone else (read again: those of the line worked on).
                if let Op::SetSetting { key, .. } = &op
                    && key.starts_with(crate::drawings::SETTING)
                    && me.as_deref() != Some(user.as_str())
                {
                    self.drawings_setting_changed = true;
                }
                if me.as_deref() != Some(user.as_str()) {
                    others.push(format!("{user}: {}", op.describe()));
                }
                None
            }
            ServerMsg::Response { id: i, result } if Some(i) == id => Some(result),
            ServerMsg::Response { .. } => None,
            ServerMsg::Peers { peers } => {
                if let Some(r) = &mut self.remote {
                    r.peers = peers;
                }
                None
            }
            ServerMsg::Locks { locks } => {
                if let Some(r) = &mut self.remote {
                    r.locks = locks;
                }
                None
            }
            ServerMsg::Notice { text } => {
                others.push(text);
                None
            }
            ServerMsg::Welcome { .. } | ServerMsg::Refused { .. } | ServerMsg::Scene(_) => None,
        }
    }

    /// An edit of the project: to the server when joined, else applied
    /// here (the same code). Libraries, users, project lines and settings
    /// too.
    pub fn project_op(&mut self, what: &str, op: Op) -> Result<String, EditError> {
        // A library file written: what reads the libraries reads again.
        if matches!(op, Op::PutLibrary { .. } | Op::DeleteLibrary { .. }) {
            self.revisions.libraries += 1;
        }
        if self.remote.is_some() {
            return self.remote_request(what, op);
        }
        let started = Instant::now();
        let e = self
            .editing
            .as_mut()
            .map_err(|why| EditError::ReadOnly(why.clone()))?;
        let user = e.user.clone();
        let line = e.session.current_line().to_owned();
        match ohl_server::execute(&mut e.session, &user, &line, &op)? {
            Some(applied) => self.sync(what, applied, started),
            None => Ok(what.to_owned()),
        }
    }

    /// Sends an edit to the server and waits for its answer (what others
    /// did meanwhile is applied first); then re-solves what changed.
    pub fn remote_request(&mut self, what: &str, op: Op) -> Result<String, EditError> {
        let started = Instant::now();
        let r = self
            .remote
            .as_mut()
            .ok_or_else(|| EditError::ReadOnly("not joined".into()))?;
        if let Some(why) = &r.closed {
            return Err(EditError::ReadOnly(format!("disconnected: {why}")));
        }
        let id = r.link.request(op).map_err(EditError::Refused)?;
        let mut applied = Vec::new();
        let mut others = Vec::new();
        let answer = loop {
            if started.elapsed() > ANSWER_TIMEOUT {
                break Err(format!(
                    "no answer from the server in {} s",
                    ANSWER_TIMEOUT.as_secs()
                ));
            }
            let Some(r) = &self.remote else {
                break Err("not joined".into());
            };
            match r.link.recv_timeout(Duration::from_millis(50)) {
                Some(Incoming::Msg(m)) => {
                    if let Some(a) = self.take(m, Some(id), &mut applied, &mut others) {
                        break a;
                    }
                }
                Some(Incoming::Closed(why)) => {
                    if let Some(r) = &mut self.remote {
                        r.closed = Some(why.clone());
                    }
                    break Err(format!("disconnected: {why}"));
                }
                None => {}
            }
        };
        let synced = match merge(applied) {
            Some(a) => Some(self.sync(what, a, started)?),
            None => None,
        };
        match answer {
            Ok(done) => Ok(synced.unwrap_or(done)),
            Err(e) => Err(EditError::Refused(e)),
        }
    }

    /// Applies what others did since the last frame; a report of it
    /// (error: disconnected or out of sync) when anything came.
    pub fn poll_remote(&mut self) -> Option<(String, bool)> {
        let r = self.remote.as_ref()?;
        let was_closed = r.closed.is_some();
        let mut msgs = Vec::new();
        while let Some(m) = r.link.try_recv() {
            let end = matches!(m, Incoming::Closed(_));
            msgs.push(m);
            if end || msgs.len() >= 1000 {
                break;
            }
        }
        if msgs.is_empty() {
            return None;
        }
        let started = Instant::now();
        let mut applied = Vec::new();
        let mut others = Vec::new();
        for m in msgs {
            match m {
                Incoming::Msg(m) => {
                    self.take(m, None, &mut applied, &mut others);
                }
                Incoming::Closed(why) => {
                    if let Some(r) = &mut self.remote
                        && r.closed.is_none()
                    {
                        r.closed = Some(why);
                    }
                }
            }
        }
        let what = others.join("; ");
        let synced = merge(applied).map(|a| self.sync(&what, a, started));
        let r = self.remote.as_ref()?;
        if let (false, Some(why)) = (was_closed, &r.closed) {
            return Some((format!("disconnected from the server: {why}"), true));
        }
        match synced {
            Some(Ok(report)) => Some((report, false)),
            Some(Err(e)) => Some((e.to_string(), true)),
            None if !what.is_empty() => Some((what, false)),
            None => None,
        }
    }

    /// Who holds `target` if not this user.
    pub fn locked_by_other(&self, target: &str) -> Option<String> {
        let r = self.remote.as_ref()?;
        r.locks.get(target).filter(|u| **u != r.link.user).cloned()
    }

    /// Holds the soft locks `wanted` (what this user edits now), releasing
    /// the others.
    pub fn hold(&mut self, wanted: BTreeSet<String>) {
        let Some(r) = &mut self.remote else {
            return;
        };
        if r.held == wanted || r.closed.is_some() {
            return;
        }
        for t in r.held.difference(&wanted) {
            let _ = r.link.send(ClientMsg::Unlock { target: t.clone() });
        }
        for t in wanted.difference(&r.held) {
            let _ = r.link.send(ClientMsg::Lock { target: t.clone() });
        }
        r.held = wanted;
    }

    /// Shows the others what this user selects and where they look (at
    /// most twice a second).
    pub fn show_presence(&mut self, presence: Presence) {
        let Some(r) = &mut self.remote else {
            return;
        };
        if r.closed.is_some()
            || r.sent.0 == presence
            || r.sent
                .1
                .is_some_and(|t| t.elapsed() < Duration::from_millis(500))
        {
            return;
        }
        if r.link.send(ClientMsg::Presence(presence.clone())).is_ok() {
            r.sent = (presence, Some(Instant::now()));
        }
    }
}

/// Where joined projects are kept (one file per server address and
/// user, replaced at every join).
pub fn joined_file(dir: &Path, invitation: &Invitation, user: &str) -> PathBuf {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };
    dir.join(format!(
        "{}-{}.ohl",
        clean(&invitation.address),
        clean(user)
    ))
}

/// The default folder of joined projects.
pub fn default_joined_dir() -> PathBuf {
    eframe::storage_dir(crate::STORAGE_ID)
        .map(|d| d.join("joined"))
        .unwrap_or_else(|| std::env::temp_dir().join("ohl-design-joined"))
}

/// A join running in the background.
pub enum JoinEvent {
    /// Bytes of the project received.
    Progress(Progress),
    /// Joined (the project file, the connection), or why not.
    Done(Result<(PathBuf, Box<Remote>), String>),
}

/// Joins in the background; events on the returned channel.
pub fn start_join(
    invitation: Invitation,
    user: String,
    dir: PathBuf,
    hosting: bool,
    ctx: egui::Context,
) -> Receiver<JoinEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let file = joined_file(&dir, &invitation, &user);
        let p = tx.clone();
        let c = ctx.clone();
        let result = ohl_server::client::join(&invitation, &user, &file, &mut |g| {
            let _ = p.send(JoinEvent::Progress(g));
            c.request_repaint();
        });
        let done = result.map(|j| (file, Box::new(Remote::new(j, invitation, hosting))));
        let _ = tx.send(JoinEvent::Done(done));
        ctx.request_repaint();
    });
    rx
}

/// File › Join a server: the invitation and the user name.
#[derive(Default)]
pub struct JoinWindow {
    /// The invitation, as given.
    pub invitation: String,
    /// User name.
    pub user: String,
    /// Why the last attempt failed.
    pub error: Option<String>,
    /// Bytes received so far.
    pub progress: Option<Progress>,
    /// The join running.
    pub running: Option<Receiver<JoinEvent>>,
}

impl JoinWindow {
    /// The window; `Some` once joined (the project file, the connection).
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        dir: &Path,
    ) -> Option<(PathBuf, Box<Remote>)> {
        let mut joined = None;
        if let Some(rx) = &self.running {
            while let Ok(e) = rx.try_recv() {
                match e {
                    JoinEvent::Progress(p) => self.progress = Some(p),
                    JoinEvent::Done(Ok(j)) => joined = Some(j),
                    JoinEvent::Done(Err(e)) => self.error = Some(e),
                }
            }
            if joined.is_some() || self.error.is_some() {
                self.running = None;
            }
        }
        egui::Window::new("Join a server")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label("Invitation (from the person hosting the project)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.invitation)
                        .desired_width(480.0)
                        .hint_text("ohl://host:port/code/fingerprint"),
                );
                ui.horizontal(|ui| {
                    ui.label("User name");
                    ui.text_edit_singleline(&mut self.user);
                });
                ui.label(
                    egui::RichText::new(
                        "The project is copied to this computer, then kept up to date with \
                         everyone's edits. Users the project does not know join as viewers \
                         (an admin can make them designers).",
                    )
                    .small()
                    .weak(),
                );
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                if self.running.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        match self.progress {
                            Some(p) => ui.label(format!(
                                "receiving the project: {:.1} of {:.1} MB",
                                p.received as f64 / 1e6,
                                p.total as f64 / 1e6
                            )),
                            None => ui.label("connecting…"),
                        };
                    });
                } else if ui.button("Join").clicked() {
                    match self.invitation.parse::<Invitation>() {
                        Ok(inv) if !self.user.trim().is_empty() => {
                            self.error = None;
                            self.progress = None;
                            self.running = Some(start_join(
                                inv,
                                self.user.trim().to_owned(),
                                dir.to_owned(),
                                false,
                                ctx.clone(),
                            ));
                        }
                        Ok(_) => self.error = Some("give a user name".into()),
                        Err(e) => self.error = Some(e),
                    }
                }
            });
        if self.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        joined
    }
}

/// File › Host this project: the port and who may join.
pub struct HostWindow {
    /// Address to listen on (default: every network of this computer).
    pub bind: std::net::IpAddr,
    /// UDP port.
    pub port: u16,
    /// Role of users the project does not know (`None`: refused).
    pub newcomers: Option<Role>,
    /// Also serve the web client (browsers join with a link).
    pub web: bool,
    /// TCP port of the web client.
    pub web_port: u16,
    /// Connections at once.
    pub max_connections: usize,
    /// Connections of one address not joined yet (this computer's too).
    pub max_per_address: usize,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

impl Default for HostWindow {
    fn default() -> Self {
        Self {
            bind: std::net::IpAddr::from([0, 0, 0, 0]),
            port: 4433,
            newcomers: Some(Role::Viewer),
            web: false,
            web_port: 8443,
            max_connections: ohl_server::server::MAX_QUIC,
            max_per_address: ohl_server::server::MAX_PENDING,
            error: None,
        }
    }
}

impl HostWindow {
    /// The window; true when Start was clicked.
    pub fn ui(&mut self, ctx: &egui::Context, open: &mut bool) -> bool {
        let mut start = false;
        egui::Window::new("Host this project")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Others join with the invitation shown in People once the server runs. \
                         The project file stays here: every edit, from anyone, is written to it. \
                         The port must be reachable (UDP) from the others' computers.",
                    )
                    .small()
                    .weak(),
                );
                egui::Grid::new("host").show(ui, |ui| {
                    ui.label("Port (UDP)");
                    ui.add(egui::DragValue::new(&mut self.port).range(0..=65535))
                        .on_hover_text("0: any free port (shown in the invitation)");
                    ui.end_row();
                    ui.label("Users the project does not know");
                    egui::ComboBox::from_id_salt("newcomers")
                        .selected_text(match self.newcomers {
                            Some(Role::Viewer) => "join as viewers",
                            Some(Role::Designer) => "join as designers",
                            Some(Role::Admin) => "join as admins",
                            None => "are refused",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.newcomers,
                                Some(Role::Viewer),
                                "join as viewers",
                            );
                            ui.selectable_value(
                                &mut self.newcomers,
                                Some(Role::Designer),
                                "join as designers",
                            );
                            ui.selectable_value(&mut self.newcomers, None, "are refused");
                        });
                    ui.end_row();
                    ui.checkbox(&mut self.web, "Web client").on_hover_text(
                        "Browsers join with a link (People): HTTPS with this server's \
                             self-signed certificate, so browsers warn once",
                    );
                    ui.add_enabled(
                        self.web,
                        egui::DragValue::new(&mut self.web_port).range(0..=65535),
                    )
                    .on_hover_text("TCP port of the web client (0: any free port)");
                    ui.end_row();
                    ui.label("Connections at most");
                    ui.add(egui::DragValue::new(&mut self.max_connections).range(1..=4096))
                        .on_hover_text("Connections at once; more wait for a free one");
                    ui.end_row();
                    ui.label("Per address at most");
                    ui.add(egui::DragValue::new(&mut self.max_per_address).range(1..=1024))
                        .on_hover_text(
                            "Connections of one address not joined yet, this computer's own \
                             too (behind a reverse proxy every user comes from it: raise this)",
                        );
                    ui.end_row();
                });
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                start = ui.button("Start hosting").clicked();
            });
        start
    }
}

/// What the People page asks for.
pub enum PeopleAction {
    /// Look where a peer looks.
    GoTo([f64; 3]),
    /// An edit (an admin changing a role).
    Op(String, Op),
}

/// The People page: who is connected, the invitation, roles (admins).
pub fn people_ui(ui: &mut egui::Ui, model: &mut Model) -> Option<PeopleAction> {
    let mut go = None;
    let Some(r) = &mut model.remote else {
        ui.label("Not connected to a server. File › Host this project, or File › Join a server.");
        return None;
    };
    let admin = r.role == Role::Admin && r.closed.is_none();
    let me = r.link.user.clone();
    ui.horizontal(|ui| {
        match &r.closed {
            Some(why) => ui.colored_label(
                ui.visuals().error_fg_color,
                format!("disconnected: {why} (read-only)"),
            ),
            None => ui.label(format!(
                "{} {} as {} ({})",
                if r.hosting { "hosting on" } else { "joined" },
                r.link.address,
                me,
                r.role.as_str()
            )),
        };
    });
    let invitation = r.invitation.to_string();
    ui.horizontal(|ui| {
        ui.label("Invitation")
            .on_hover_text("For people the project does not know: they join as newcomers");
        ui.add(
            egui::TextEdit::singleline(&mut invitation.as_str())
                .desired_width(420.0)
                .font(egui::TextStyle::Monospace),
        );
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(invitation.clone());
        }
    });
    let (web, secret) = (r.web.clone(), r.secret.clone());
    let web_link = |user: Option<&str>| {
        let (web, secret) = (web.as_ref()?, secret.as_ref()?);
        let who = user
            .map(|u| format!("user={}&", ohl_server::web::fragment_escape(u)))
            .unwrap_or_default();
        Some(format!("{web}#{who}code={}", secret.code(user)))
    };
    if let Some(link) = web_link(None) {
        ui.horizontal(|ui| {
            ui.label("Web client").on_hover_text(
                "The link for browsers (people the project does not know). Their browser \
                 warns about the certificate once: its fingerprint is the invitation's",
            );
            ui.add(
                egui::TextEdit::singleline(&mut link.as_str())
                    .desired_width(420.0)
                    .font(egui::TextStyle::Monospace),
            );
            if ui.button("Copy").clicked() {
                ui.ctx().copy_text(link.clone());
            }
        });
    }
    if let Some(secret) = r.secret.clone() {
        ui.horizontal(|ui| {
            ui.label("Personal invitation for").on_hover_text(
                "A user of the project joins only with theirs (nobody else can use the name)",
            );
            ui.add(egui::TextEdit::singleline(&mut r.invite_name).desired_width(120.0));
            let name = r.invite_name.trim().to_owned();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("Copy it"))
                .clicked()
            {
                let personal = Invitation {
                    code: secret.code(Some(&name)),
                    ..r.invitation.clone()
                };
                ui.ctx().copy_text(personal.to_string());
            }
            if let Some(link) = web_link(Some(&name))
                && ui
                    .add_enabled(!name.is_empty(), egui::Button::new("Copy web link"))
                    .clicked()
            {
                ui.ctx().copy_text(link);
            }
        });
    }
    let r = &*r;
    ui.separator();
    let mut role_change: Option<(String, Role)> = None;
    egui::Grid::new("people")
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for h in ["user", "role", "selection", "editing", ""] {
                ui.strong(h);
            }
            ui.end_row();
            for p in &r.peers {
                let name = if p.user == me {
                    format!("{} (you)", p.user)
                } else {
                    p.user.clone()
                };
                ui.label(name);
                if admin && p.user != me {
                    let mut role = p.role;
                    egui::ComboBox::from_id_salt(("role", &p.user))
                        .selected_text(role.as_str())
                        .show_ui(ui, |ui| {
                            for r in [Role::Viewer, Role::Designer, Role::Admin] {
                                ui.selectable_value(&mut role, r, r.as_str());
                            }
                        });
                    if role != p.role {
                        role_change = Some((p.user.clone(), role));
                    }
                } else {
                    ui.label(p.role.as_str());
                }
                ui.label(&p.presence.selection);
                let editing: Vec<&str> = r
                    .locks
                    .iter()
                    .filter(|(_, u)| **u == p.user)
                    .map(|(t, _)| t.as_str())
                    .collect();
                ui.label(editing.join(", "));
                if let Some(c) = p.presence.camera
                    && p.user != me
                {
                    if ui
                        .small_button("Go to")
                        .on_hover_text("Look where they look")
                        .clicked()
                    {
                        go = Some(PeopleAction::GoTo(c));
                    }
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    if let Some((name, role)) = role_change {
        let what = format!("{name} made {}", role.as_str());
        return Some(PeopleAction::Op(what, Op::SetUser { name, role }));
    }
    go
}

/// Versions › History: the operation log, newest first; admins roll back.
/// Returns an edit to make (rollback of one entry, or back to one).
pub fn history_ui(ui: &mut egui::Ui, model: &Model) -> Option<(String, Op)> {
    let session = match (&model.editing, &model.viewer_session) {
        (Ok(e), _) => &e.session,
        (Err(_), Some(s)) => s,
        (Err(why), None) => {
            ui.label(format!("No log: {why}"));
            return None;
        }
    };
    let admin = model.my_role() == Some(Role::Admin) && model.can_edit().is_ok();
    let log = match session.log() {
        Ok(l) => l,
        Err(e) => {
            crate::menu::error_label(ui, e.to_string());
            return None;
        }
    };
    let mut out = None;
    ui.label(
        egui::RichText::new(format!(
            "{} entries. Admins: \"Reverse\" undoes one entry (anyone's); \"Back to here\" \
             returns the line to its state just after an entry (one entry, undoable).",
            log.len()
        ))
        .small()
        .weak(),
    );
    egui::ScrollArea::vertical()
        .max_height(480.0)
        .show(ui, |ui| {
            egui::Grid::new("history")
                .striped(true)
                .spacing([10.0, 3.0])
                .show(ui, |ui| {
                    for h in ["#", "time (UTC)", "user", "", "what", ""] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for e in log.iter().rev().take(1000) {
                        ui.label(e.seq.to_string());
                        ui.label(e.time.get(..19).unwrap_or(&e.time));
                        ui.label(&e.user);
                        ui.label(&e.kind);
                        let what = e.note.clone().unwrap_or_else(|| {
                            format!(
                                "{} object(s): {}",
                                e.objects.len(),
                                e.objects
                                    .iter()
                                    .take(4)
                                    .map(|(k, id)| format!("{} {id}", k.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        });
                        let undone = e.undone_by.map(|b| format!(" (reversed by #{b})"));
                        ui.label(format!("{what}{}", undone.unwrap_or_default()));
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(admin, |ui| {
                                if e.undone_by.is_none()
                                    && ui
                                        .small_button("Reverse")
                                        .on_hover_text("Undo this entry")
                                        .clicked()
                                {
                                    out = Some((
                                        format!("entry {} reversed", e.seq),
                                        Op::Rollback { seq: e.seq },
                                    ));
                                }
                                if ui
                                    .small_button("Back to here")
                                    .on_hover_text("The line as it was just after this entry")
                                    .clicked()
                                {
                                    out = Some((
                                        format!("back to entry {}", e.seq),
                                        Op::RollbackAfter { seq: e.seq },
                                    ));
                                }
                            });
                        });
                        ui.end_row();
                    }
                });
        });
    out
}
