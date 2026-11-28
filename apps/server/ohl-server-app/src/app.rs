// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The window: the start page (host a network, or connect to a server),
//! then the pages on the left, the page in the middle, what was said at
//! the bottom; the change window ("what changes" before saving) and the
//! closing question in host mode.

use std::time::{Duration, Instant};

use eframe::egui;
use ohl_session::rights::RightsCommand;
use ohl_ui::tool_window as tw;

use crate::conn::{Conn, JoinHow, Joining};
use crate::host::{HostForm, Hosted, Look, Starting};
use crate::map::MapView;

/// The window's title.
pub const APP_NAME: &str = "OHL-Server";

/// The app's settings' name (eframe's storage).
pub const STORAGE_ID: &str = "OHL-Server";

/// A page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The server at a glance.
    Overview,
    /// Backups, migrations, the log, sessions, settings (network admins).
    Operations,
    /// The map.
    Map,
    /// Users and groups.
    Users,
    /// Roles: the permission matrix.
    Roles,
    /// Assignments.
    Assignments,
    /// See as user.
    SeeAs,
    /// Shared towers and twins (stage C).
    Shared,
    /// The country libraries (stage D).
    Libraries,
    /// The lines: export, import (stage E).
    Lines,
    /// The audit (stage F; network admins).
    Audit,
}

impl Page {
    /// Every page, in the order shown.
    pub const ALL: [Page; 11] = [
        Page::Overview,
        Page::Operations,
        Page::Map,
        Page::Shared,
        Page::Lines,
        Page::Libraries,
        Page::Users,
        Page::Roles,
        Page::Assignments,
        Page::SeeAs,
        Page::Audit,
    ];

    /// Its name (also its heading in the user guide).
    pub fn name(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Operations => "Operations",
            Page::Map => "Map",
            Page::Users => "Users and groups",
            Page::Roles => "Roles",
            Page::Assignments => "Assignments",
            Page::SeeAs => "See as user",
            Page::Shared => "Shared towers",
            Page::Libraries => "Libraries",
            Page::Lines => "Lines",
            Page::Audit => "Audit",
        }
    }

    /// For network admins only.
    pub fn admins_only(self) -> bool {
        matches!(self, Page::Operations | Page::Audit)
    }

    /// A page of stages C to F (a server of admin messages 2 or later).
    pub fn stages_cf(self) -> bool {
        matches!(
            self,
            Page::Shared | Page::Libraries | Page::Lines | Page::Audit
        )
    }
}

/// A tab of the Operations page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpTab {
    /// Backups and the restore drill.
    Backups,
    /// The schema and migrations.
    Migrations,
    /// The server's log.
    Log,
    /// Who is connected.
    Sessions,
    /// The server's settings, read-only.
    Settings,
}

impl OpTab {
    /// Every tab.
    pub const ALL: [OpTab; 5] = [
        OpTab::Backups,
        OpTab::Migrations,
        OpTab::Log,
        OpTab::Sessions,
        OpTab::Settings,
    ];

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            OpTab::Backups => "Backups",
            OpTab::Migrations => "Migrations",
            OpTab::Log => "Log",
            OpTab::Sessions => "Sessions",
            OpTab::Settings => "Settings",
        }
    }
}

/// The connect form.
#[derive(Debug, Clone, Default)]
pub struct ConnectForm {
    /// The invitation (`ohl://…`; with a sign-in, its access code is not
    /// needed).
    pub invitation: String,
    /// The user.
    pub user: String,
}

/// An OIDC sign-in under way (its own thread: the browser's answer).
pub(crate) struct SignIn {
    pub(crate) rx: std::sync::mpsc::Receiver<Result<String, String>>,
    pub(crate) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// A window open on top of a page.
#[derive(Debug, Clone, PartialEq)]
// One at a time: the change kept in place.
#[allow(clippy::large_enum_variant)]
pub enum Window {
    /// A rights change: "what changes" shown, saved on the action.
    Change(RightsCommand),
    /// An invitation to make (the user's name, display name, days).
    Invite {
        /// Name.
        name: String,
        /// Display name.
        display: String,
        /// Days valid.
        days: u32,
    },
    /// A connection to let go (the connection, the user, the reason).
    Disconnect {
        /// The connection.
        client: u64,
        /// Its user.
        user: String,
        /// Why.
        reason: String,
    },
    /// A maintainer's library change (its note or tag filled in here).
    Library(ohl_server::admin::LibraryAction),
    /// Update library pins of a line: the proposal shown, carried out on
    /// the action.
    Pins(String),
    /// A project file to import (the file, the code, the name, the twin
    /// distance).
    Import {
        /// The file.
        path: std::path::PathBuf,
        /// The new line's code.
        code: String,
        /// Its name (empty: the file's).
        name: String,
        /// Twins within, m.
        tolerance: f64,
    },
    /// A network setting changed.
    Setting {
        /// The setting.
        key: String,
        /// Its new value.
        value: String,
    },
}

/// The app.
pub struct App {
    /// The page shown.
    pub page: Page,
    /// The Operations page's tab.
    pub op_tab: OpTab,
    /// The host form.
    pub host_form: HostForm,
    /// The connect form.
    pub connect_form: ConnectForm,
    pub(crate) look: Option<Result<Look, String>>,
    pub(crate) starting: Option<Starting>,
    pub(crate) joining: Option<Joining>,
    pub(crate) sign_in: Option<SignIn>,
    pub(crate) id_token: Option<String>,
    /// The server this app hosts (host mode).
    pub hosted: Option<Hosted>,
    /// The connection.
    pub conn: Option<Conn>,
    pub(crate) last_join: Option<JoinHow>,
    /// Why the last start or join failed.
    pub error: Option<String>,
    /// The map.
    pub map: MapView,
    /// The window open on top.
    pub window: Option<Window>,
    /// The closing question (host mode).
    pub closing: bool,
    allowed_close: bool,
    pub(crate) refreshed: std::collections::HashMap<&'static str, Instant>,
    /// What the pages keep between frames.
    pub pages: crate::pages::PageState,
    applied_style: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// A new app (the start page).
    pub fn new() -> Self {
        Self {
            page: Page::Overview,
            op_tab: OpTab::Backups,
            host_form: HostForm::default(),
            connect_form: ConnectForm::default(),
            look: None,
            starting: None,
            joining: None,
            sign_in: None,
            id_token: None,
            hosted: None,
            conn: None,
            last_join: None,
            error: None,
            map: MapView::default(),
            window: None,
            closing: false,
            allowed_close: false,
            refreshed: std::collections::HashMap::new(),
            pages: crate::pages::PageState::default(),
            applied_style: false,
        }
    }

    /// Joins with `how` (connect mode, or the app's own server).
    pub fn join(&mut self, how: JoinHow) {
        self.error = None;
        self.conn = None;
        self.last_join = Some(how.clone());
        self.joining = Some(Joining::start(how));
    }

    /// Starts hosting with the host form.
    pub fn host(&mut self) {
        self.error = None;
        self.starting = Some(Starting::start(self.host_form.clone()));
    }

    /// Asks before closing (host mode: the server stops with the app).
    pub fn request_close(&mut self) {
        if self.hosted.is_some() && !self.allowed_close {
            self.closing = true;
            if let Some(c) = &mut self.conn {
                c.ask_sessions();
            }
        }
    }

    /// Whether `page` is open to the user (network admins: every page;
    /// others: not Operations nor Audit; the pages of stages C to F on a
    /// server that speaks them).
    pub fn page_open(&self, page: Page) -> bool {
        let Some(c) = &self.conn else {
            return !page.admins_only() && !page.stages_cf();
        };
        (!page.admins_only() || c.data.network_admin()) && (!page.stages_cf() || c.data.stages_cf())
    }

    /// True once `key` was last done `every` ago (or never): done now.
    pub(crate) fn due(&mut self, key: &'static str, every: Duration) -> bool {
        let now = Instant::now();
        match self.refreshed.get(key) {
            Some(t) if now.duration_since(*t) < every => false,
            _ => {
                self.refreshed.insert(key, now);
                true
            }
        }
    }

    /// Forgets when `key` was done (asked again next frame).
    pub(crate) fn again(&mut self, key: &'static str) {
        self.refreshed.remove(key);
    }

    fn poll(&mut self) {
        if let Some(s) = &self.starting
            && let Some(r) = s.poll()
        {
            self.starting = None;
            match r {
                Ok((hosted, how)) => {
                    self.hosted = Some(hosted);
                    self.join(how);
                }
                Err(e) => self.error = Some(format!("the server did not start: {e}")),
            }
        }
        if let Some(j) = &self.joining
            && let Some(r) = j.poll()
        {
            self.joining = None;
            match r {
                Ok(mut c) => {
                    c.ask_overview();
                    c.ask_rights();
                    c.ask_map();
                    self.refreshed.clear();
                    self.conn = Some(c);
                }
                Err(e) => self.error = Some(format!("not joined: {e}")),
            }
        }
        if let Some(s) = &self.sign_in
            && let Ok(r) = s.rx.try_recv()
        {
            self.sign_in = None;
            match r {
                Ok(token) => {
                    self.id_token = Some(token);
                    self.connect_now();
                }
                Err(e) => self.error = Some(format!("not signed in: {e}")),
            }
        }
        if let Some(c) = &mut self.conn {
            c.poll();
            // What an import said outlives the connection.
            if let Some(v) = c.data.imported.take() {
                self.pages.imported = Some(v);
            }
        }
        // A change of the user's own rights lets the connection go (the
        // server's rule): joined again at once, on the same page.
        let rights_changed = self
            .conn
            .as_ref()
            .filter(|c| c.closed.is_some())
            .is_some_and(|c| {
                c.rights_changed
                    || c.closed
                        .as_deref()
                        .is_some_and(|why| why.contains("rights on this network changed"))
            });
        if rights_changed
            && self.joining.is_none()
            && let Some(how) = self.last_join.clone()
            && self.due("rejoin", Duration::from_secs(10))
        {
            let page = self.page;
            self.join(how);
            self.page = page;
        }
    }

    /// Joins with the connect form (the token of a sign-in if there is
    /// one).
    pub(crate) fn connect_now(&mut self) {
        let f = &self.connect_form;
        let invitation = match f.invitation.trim().parse::<ohl_server::Invitation>() {
            Ok(i) => i,
            Err(e) => {
                self.error = Some(format!("the invitation: {e}"));
                return;
            }
        };
        if f.user.trim().is_empty() {
            self.error = Some("the user who signs in".into());
            return;
        }
        let how = JoinHow {
            invitation,
            user: f.user.trim().to_owned(),
            id_token: self.id_token.take(),
        };
        self.join(how);
    }

    /// The OIDC sign-in (the server's identity provider, in the browser):
    /// its token kept in memory only, then the join.
    pub(crate) fn start_sign_in(&mut self) {
        let invitation = match self
            .connect_form
            .invitation
            .trim()
            .parse::<ohl_server::Invitation>()
        {
            Ok(i) => i,
            Err(e) => {
                self.error = Some(format!("the invitation: {e}"));
                return;
            }
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::Builder::new()
            .name("ohl-server-app-sign-in".into())
            .spawn(move || {
                let r = (|| {
                    let offer = ohl_server::sign_in::options(&invitation)?
                        .oidc
                        .ok_or("this server takes invitations only (no sign-in offered)")?;
                    let flow = ohl_server::sign_in::Flow::start(&offer)?;
                    crate::help::open_url(flow.url())?;
                    let signed = flow.finish(Duration::from_secs(300), &|| {
                        stop.load(std::sync::atomic::Ordering::SeqCst)
                    })?;
                    Ok::<String, String>(signed.id_token().to_owned())
                })();
                let _ = tx.send(r);
            })
            .expect("a thread");
        self.sign_in = Some(SignIn { rx, cancel });
    }

    /// Stops the hosted server (the app's own connection goes with it).
    pub fn stop_server(&mut self) {
        self.conn = None;
        if let Some(h) = self.hosted.take() {
            h.server.stop();
        }
    }

    /// One frame (the window's content).
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.applied_style {
            ohl_ui::style::apply(&ctx);
            self.applied_style = true;
        }
        self.poll();
        if ctx.input(|i| i.viewport().close_requested())
            && self.hosted.is_some()
            && !self.allowed_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_close();
        }
        if self.conn.is_some() || self.joining.is_some() || self.starting.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        egui::Panel::top("ohl-server-top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("ohl-server-said").show(ui, |ui| self.said_bar(ui));
        if self.conn.is_none() {
            egui::CentralPanel::default().show(ui, |ui| self.start_page(ui));
        } else {
            egui::Panel::left("ohl-server-pages")
                .resizable(false)
                .show(ui, |ui| self.nav(ui));
            egui::CentralPanel::default().show(ui, |ui| self.page_ui(ui));
        }
        self.windows(&ctx);
        self.closing_window(&ctx);
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong(APP_NAME);
            ui.separator();
            match (&self.conn, &self.hosted) {
                (Some(c), Some(h)) => {
                    ui.label(format!(
                        "Hosting on {} — signed in as {}",
                        h.server.local_addr,
                        c.user()
                    ));
                }
                (Some(c), None) => {
                    ui.label(format!("Connected to {} as {}", c.address(), c.user()));
                }
                (None, Some(h)) => {
                    ui.label(format!("Hosting on {}", h.server.local_addr));
                }
                (None, None) => {
                    ui.label("Not connected");
                }
            }
            if self.hosted.is_some() && ui.button("Stop the server…").clicked() {
                self.request_close();
            }
            if self.hosted.is_none() && self.conn.is_some() && ui.button("Disconnect").clicked() {
                self.conn = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                tw::guide_button(ui, "top", || {
                    crate::help::open_section("user-guide/ohl-server", "ohl-server-the-admin-app")
                });
            });
        });
    }

    fn said_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(c) = &self.conn {
                if let Some(why) = &c.closed {
                    ohl_ui::messages::error_label(
                        ui,
                        egui::RichText::new(format!("The connection ended: {why}")),
                    );
                    return;
                }
                for b in &c.data.busy {
                    ui.spinner();
                    ui.label(format!("{b}…"));
                }
                if let Some((text, error)) = c.data.last_said() {
                    if *error {
                        ohl_ui::messages::error_label(ui, egui::RichText::new(text));
                    } else {
                        ui.label(text);
                    }
                }
            } else if let Some(e) = &self.error {
                ohl_ui::messages::error_label(ui, egui::RichText::new(e));
            } else {
                ui.weak("Host a network, or connect to a server");
            }
        });
        if self.conn.as_ref().is_some_and(|c| c.closed.is_some()) {
            ui.horizontal(|ui| {
                if let Some(how) = self.last_join.clone()
                    && ui.button("Connect again").clicked()
                {
                    self.join(how);
                }
                if ui.button("Back to the start page").clicked() {
                    self.conn = None;
                }
            });
        }
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(140.0);
        ui.add_space(6.0);
        for p in Page::ALL {
            if !self.page_open(p) {
                continue;
            }
            if ui.selectable_label(self.page == p, p.name()).clicked() {
                self.page = p;
            }
        }
        if let Some(o) = self.conn.as_ref().and_then(|c| c.data.overview.as_ref())
            && !o.warnings.is_empty()
        {
            ui.add_space(12.0);
            ohl_ui::messages::error_label(
                ui,
                egui::RichText::new(format!("{} warning(s)", o.warnings.len())).small(),
            );
        }
    }

    fn start_page(&mut self, ui: &mut egui::Ui) {
        let busy = self.starting.is_some() || self.joining.is_some() || self.sign_in.is_some();
        ui.heading(APP_NAME);
        ui.label(
            "The admin app of a network's server: its state, its operations, a simplified map and \
             the rights of its users. It talks to the server only, never to the database.",
        );
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            tw::heading(
                ui,
                "Host a network",
                "The network's server runs inside this app (a small office, a test). Closing the \
                 app stops it (you are asked first). For production, run ohl network serve on \
                 the server and connect to it.",
            );
            let f = &mut self.host_form;
            egui::Grid::new("host-form").num_columns(2).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, label: &str, help: &str, v: &mut String| {
                    tw::label(ui, label, help);
                    ui.text_edit_singleline(v);
                    ui.end_row();
                };
                row(
                    ui,
                    "Database",
                    "The connection string: host=… dbname=… user=… or postgresql://…",
                    &mut f.database,
                );
                row(
                    ui,
                    "Password file",
                    "A file holding the database's password (empty: none, or in the string)",
                    &mut f.password_file,
                );
                row(
                    ui,
                    "TLS",
                    "disable, require, verify-ca or verify-full (empty: the string's, else the \
                     default)",
                    &mut f.tls,
                );
                row(ui, "CA file", "The database's CA (PEM)", &mut f.ca_file);
                row(ui, "Listen on", "Address and port (UDP, QUIC)", &mut f.bind);
                row(
                    ui,
                    "Keys file",
                    "Keeps the personal invitations' key (empty: invitations change at the next \
                     start)",
                    &mut f.keys,
                );
                row(
                    ui,
                    "Backup folder",
                    "Where Operations › Backups writes (empty: no backups from the app)",
                    &mut f.backup_dir,
                );
                row(
                    ui,
                    "Admin",
                    "You: a network admin of this database",
                    &mut f.user,
                );
            });
            ui.checkbox(
                &mut f.bootstrap,
                "First network admin (only while the network has none)",
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("Look at the database"))
                    .clicked()
                {
                    self.look = Some(crate::host::look(&self.host_form));
                }
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(egui::RichText::new("Host").strong()),
                    )
                    .clicked()
                {
                    self.host();
                }
                if self.starting.is_some() {
                    ui.spinner();
                    ui.label("reading every line…");
                }
            });
            match &self.look {
                Some(Ok(l)) => {
                    ui.label(l.describe());
                }
                Some(Err(e)) => {
                    ohl_ui::messages::error_label(ui, egui::RichText::new(e));
                }
                None => {}
            }
            let ui = &mut cols[1];
            tw::heading(
                ui,
                "Connect to a server",
                "A network's server running elsewhere (ohl network serve): your personal \
                 invitation, or a sign-in with the organisation's identity provider when the \
                 server offers one.",
            );
            egui::Grid::new("connect-form")
                .num_columns(2)
                .show(ui, |ui| {
                    tw::label(ui, "Invitation", "ohl://address/code/fingerprint");
                    ui.text_edit_singleline(&mut self.connect_form.invitation);
                    ui.end_row();
                    tw::label(ui, "User", "Your user name on the network");
                    ui.text_edit_singleline(&mut self.connect_form.user);
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(egui::RichText::new("Connect").strong()),
                    )
                    .clicked()
                {
                    self.connect_now();
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Sign in…"))
                    .on_hover_text(
                        "The organisation's identity provider, in your browser (the token kept \
                         in memory only)",
                    )
                    .clicked()
                {
                    self.start_sign_in();
                }
                if self.joining.is_some() {
                    ui.spinner();
                    ui.label("joining…");
                }
                if let Some(s) = &self.sign_in {
                    ui.spinner();
                    ui.label("waiting for the browser…");
                    if ui.button("Cancel").clicked() {
                        s.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            });
        });
    }

    fn closing_window(&mut self, ctx: &egui::Context) {
        if !self.closing {
            return;
        }
        let mut open = true;
        let mut stop = false;
        egui::Window::new("Stop the server")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                tw::intro(
                    ui,
                    "The network's server runs inside this app: closing the app stops it, and \
                     every user connected is let go.",
                    "Users keep their offline work; they join again once a server runs. For a \
                     server that keeps running without this app, use ohl network serve.",
                    "closing",
                    || crate::help::open_section("user-guide/ohl-server", "host-mode"),
                );
                let others: Vec<String> = self
                    .conn
                    .as_ref()
                    .map(|c| {
                        c.data
                            .sessions
                            .iter()
                            .filter(|s| s.user != c.user())
                            .map(|s| format!("{} ({})", s.user, s.follows))
                            .collect()
                    })
                    .unwrap_or_default();
                if others.is_empty() {
                    ui.label("Nobody else is connected.");
                } else {
                    ui.label(format!("{} other connection(s):", others.len()));
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for o in &others {
                                ui.label(o);
                            }
                        });
                }
                match tw::buttons(
                    ui,
                    "Stop the server and close",
                    true,
                    None,
                    tw::Keys::Escape,
                ) {
                    Some(tw::Pressed::Action) => stop = true,
                    Some(tw::Pressed::Cancel) => self.closing = false,
                    None => {}
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.closing = false;
                }
            });
        if !open {
            self.closing = false;
        }
        if stop {
            self.closing = false;
            self.stop_server();
            self.allowed_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// The app's settings kept on this computer (the region outlines).
    pub fn restore(&mut self, storage: Option<&dyn eframe::Storage>) {
        self.map.load(storage);
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.map.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    fn on_exit(&mut self) {
        self.stop_server();
    }
}
