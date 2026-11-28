// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The pages: Overview, Operations (backups, migrations, the log,
//! sessions, settings), the map, and the rights pages (users and groups,
//! roles, assignments, see as user). Each shows what the server answered
//! for the user's rights (the server filters; a page a user may not use is
//! not shown); every rights change goes through the change window, which
//! shows "what changes" before it is saved.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use eframe::egui;
use ohl_server::admin::{UserChange, what_changes_words};
use ohl_session::rights::{Permission, RightsCommand, Scope, Subject};
use ohl_ui::tool_window as tw;

use crate::app::{App, OpTab, Page, Window};
use crate::map::{ColourBy, Highlight, Picked, VOLTAGE_LEGEND, voltage_colour};

/// The height of a long list's scroll box, points (windows stay
/// closable).
pub const LIST_HEIGHT: f32 = 320.0;

/// What the pages keep between frames.
#[derive(Debug, Clone, Default)]
pub struct PageState {
    /// The log's filter.
    pub log_filter: String,
    /// The new assignment: who (`user` or `group`), their name.
    pub subject_group: bool,
    /// See `subject_group`.
    pub subject: String,
    /// The role given.
    pub role: String,
    /// A deny.
    pub deny: bool,
    /// A note.
    pub note: String,
    /// The scope picked.
    pub scope: Scope,
    /// The assignment selected (shown on the map).
    pub selected: Option<i64>,
    /// A new group's name.
    pub new_group: String,
    /// The user to add to a group, by group.
    pub member: BTreeMap<String, String>,
    /// Roles being edited: their permissions.
    pub role_edits: BTreeMap<String, BTreeSet<Permission>>,
    /// A new role's name.
    pub new_role: String,
    /// "See as": the user.
    pub see_as: String,
    /// "See as" shown on the map.
    pub see_as_on_map: bool,
    /// The change whose "what changes" was asked.
    pub previewed: Option<RightsCommand>,
}

/// A rights change in words.
pub fn command_words(c: &RightsCommand) -> String {
    match c {
        RightsCommand::Bootstrap { user } => format!("make {user} the first network admin"),
        RightsCommand::AddUser { name, .. } => format!("add user {name}"),
        RightsCommand::SetActive { name, active } => {
            format!("{} {name}", if *active { "enable" } else { "disable" })
        }
        RightsCommand::AddGroup { name, .. } => format!("add group {name}"),
        RightsCommand::DeleteGroup { name } => {
            format!("delete group {name} (with its assignments)")
        }
        RightsCommand::AddMember { group, user } => format!("add {user} to group {group}"),
        RightsCommand::RemoveMember { group, user } => {
            format!("take {user} out of group {group}")
        }
        RightsCommand::PutRole {
            name, permissions, ..
        } => format!(
            "role {name}: {}",
            permissions
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RightsCommand::DeleteRole { name } => format!("delete role {name}"),
        RightsCommand::Assign {
            subject,
            role,
            deny,
            scope,
            ..
        } => format!(
            "{} {subject} {} on {}",
            if *deny { "deny" } else { "give" },
            role.as_deref().unwrap_or("every permission"),
            scope.describe()
        ),
        RightsCommand::Unassign { id } => format!("take assignment #{id} away"),
        RightsCommand::ReissueInvitation { name } => format!("reissue {name}'s invitation"),
        RightsCommand::MapIdentity { user, .. } => format!("map an identity to {user}"),
        RightsCommand::UnmapIdentity { .. } => "unmap an identity".into(),
    }
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

fn hours(h: f64) -> String {
    if h < 1.0 {
        format!("{:.0} min", h * 60.0)
    } else {
        format!("{h:.1} h")
    }
}

fn uptime(s: u64) -> String {
    format!("{} d {:02}:{:02}", s / 86_400, s / 3600 % 24, s / 60 % 60)
}

impl App {
    /// The page shown.
    pub(crate) fn page_ui(&mut self, ui: &mut egui::Ui) {
        if !self.page_open(self.page) {
            self.page = Page::Overview;
        }
        match self.page {
            Page::Overview => self.overview_page(ui),
            Page::Operations => self.operations_page(ui),
            Page::Map => self.map_page(ui),
            Page::Users => self.users_page(ui),
            Page::Roles => self.roles_page(ui),
            Page::Assignments => self.assignments_page(ui),
            Page::SeeAs => self.see_as_page(ui),
        }
    }

    fn page_intro(ui: &mut egui::Ui, page: Page, what: &str, more: &str) {
        ui.heading(page.name());
        tw::intro(ui, what, more, page.name(), || {
            crate::help::open_section("user-guide/ohl-server", &tw::anchor(page.name()))
        });
    }

    fn overview_page(&mut self, ui: &mut egui::Ui) {
        if self.due("overview", Duration::from_secs(5))
            && let Some(c) = &mut self.conn
        {
            c.ask_overview();
        }
        Self::page_intro(
            ui,
            Page::Overview,
            "The server at a glance: its program, its network, who is connected, its queue, the \
             last backup, and what needs a look.",
            "Refreshed every 5 seconds. Line admins and library maintainers see the program and \
             the counts of what they manage only.",
        );
        let Some(o) = self.conn.as_ref().and_then(|c| c.data.overview.clone()) else {
            ui.spinner();
            return;
        };
        if !o.warnings.is_empty() {
            for w in &o.warnings {
                ohl_ui::messages::error_label(ui, egui::RichText::new(format!("⚠ {w}")));
            }
            ui.add_space(6.0);
        }
        egui::Grid::new("overview")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.label(k);
                    ui.label(v);
                    ui.end_row();
                };
                row("Network", o.network.clone());
                row("Server version", o.version.clone());
                row(
                    "Protocol",
                    format!("{} (admin messages {})", o.protocol, o.admin),
                );
                row("Uptime", uptime(o.uptime_s));
                row(
                    "Lines, routes, structures",
                    format!("{}, {}, {}", o.models, o.routes, o.structures),
                );
                row("Connections", o.connections.to_string());
                if o.full {
                    row(
                        "Schema",
                        format!("{} (this server's {})", o.schema, o.program_schema),
                    );
                    row("Users connected", o.connected.join(", "));
                    row("Working sets open", o.working_sets.to_string());
                    row(
                        "Job queue",
                        format!(
                            "{} waiting{}{}",
                            o.queue_waiting,
                            if o.check_running {
                                ", a check running"
                            } else {
                                ""
                            },
                            if o.export_running {
                                ", an export running"
                            } else {
                                ""
                            }
                        ),
                    );
                    row(
                        "Last backup",
                        match (&o.last_backup, o.backup_age_h) {
                            (Some(t), Some(h)) => format!("{t} ({} ago)", hours(h)),
                            _ => "none recorded".into(),
                        },
                    );
                    row(
                        "Results out of date",
                        format!("{} route(s)", o.stale_routes),
                    );
                }
            });
    }

    fn operations_page(&mut self, ui: &mut egui::Ui) {
        Self::page_intro(
            ui,
            Page::Operations,
            "The server's operations: backups and the restore drill, migrations, the log, the \
             sessions, the settings.",
            "Network admins only. Every action is in the network's audit.",
        );
        ui.horizontal(|ui| {
            for t in OpTab::ALL {
                if ui.selectable_label(self.op_tab == t, t.name()).clicked() {
                    self.op_tab = t;
                }
            }
        });
        ui.separator();
        match self.op_tab {
            OpTab::Backups => self.backups_tab(ui),
            OpTab::Migrations => self.migrations_tab(ui),
            OpTab::Log => self.log_tab(ui),
            OpTab::Sessions => self.sessions_tab(ui),
            OpTab::Settings => self.settings_tab(ui),
        }
    }

    fn backups_tab(&mut self, ui: &mut egui::Ui) {
        if self.due("backups", Duration::from_secs(15))
            && let Some(c) = &mut self.conn
        {
            c.ask_backups();
        }
        let Some(c) = &mut self.conn else { return };
        let busy = !c.data.busy.is_empty();
        let Some(b) = c.data.backups.clone() else {
            ui.spinner();
            return;
        };
        tw::heading(
            ui,
            "Backups",
            "PostgreSQL's dump of the whole network in one snapshot, with a manifest of each \
             line's hash; made while the server runs. A migration needs one younger than the \
             network's backup_max_age_hours (default 24 h).",
        );
        ui.label(match (&b.last, b.age_h) {
            (Some(t), Some(h)) => format!("Last backup recorded: {t} ({} ago)", hours(h)),
            _ => "No backup recorded yet.".into(),
        });
        ui.horizontal(|ui| {
            let r = ui
                .add_enabled(b.folder && !busy, egui::Button::new("Back up now"))
                .on_disabled_hover_text(if b.folder {
                    "A backup or a drill runs"
                } else {
                    "The server has no backup folder: start it with --backup-dir"
                });
            if r.clicked() {
                c.backup_now();
            }
            if !b.folder {
                ui.weak("No backup folder on the server (--backup-dir): back up with ohl network backup.");
            }
        });
        if !b.drill_database {
            ui.weak(
                "The restore drill checks the dump against its manifest; the full drill (restored \
                 and compared line by line) needs a drill database (--drill-database).",
            );
        }
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                egui::Grid::new("backups")
                    .num_columns(6)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Backup", "Time (UTC)", "Lines", "Size", "By", ""] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in &b.rows {
                            ui.label(&r.name);
                            ui.label(&r.time);
                            ui.label(r.models.to_string());
                            ui.label(mb(r.bytes));
                            ui.label(&r.user);
                            if ui
                                .add_enabled(!busy, egui::Button::new("Restore drill"))
                                .clicked()
                            {
                                c.restore_drill(&r.name);
                            }
                            ui.end_row();
                        }
                    });
            });
    }

    fn migrations_tab(&mut self, ui: &mut egui::Ui) {
        let hosting = self.hosted.is_some();
        let Some(o) = self.conn.as_ref().and_then(|c| c.data.overview.clone()) else {
            ui.spinner();
            return;
        };
        tw::heading(
            ui,
            "Migrations",
            "A network's schema is brought forward by the server program of a newer version at \
             its start, refused without a backup younger than the network's \
             backup_max_age_hours (or ohl network migrate).",
        );
        ui.label(format!(
            "The network's schema: {}. This server's: {}.",
            o.schema, o.program_schema
        ));
        if o.schema < o.program_schema {
            ohl_ui::messages::error_label(
                ui,
                egui::RichText::new("Migrations pending: back up, then start the server again."),
            );
        } else {
            ui.label("No migration pending: the running server is on its own schema.");
        }
        ui.label(format!(
            "A migration needs a backup younger than {:.0} h; {}.",
            o.backup_max_age_h,
            match o.backup_age_h {
                Some(h) if h <= o.backup_max_age_h =>
                    format!("the last is {} old: it may run", hours(h)),
                Some(h) => format!("the last is {} old: back up first", hours(h)),
                None => "none is recorded: back up first".into(),
            }
        ));
        if hosting {
            ui.weak(
                "Host mode: stop the server, start the app of the newer version and host again; \
                 the server migrates at its start.",
            );
        }
    }

    fn log_tab(&mut self, ui: &mut egui::Ui) {
        let filter = self.pages.log_filter.clone();
        if self.due("log", Duration::from_secs(2))
            && let Some(c) = &mut self.conn
        {
            c.ask_log(&filter);
        }
        ui.horizontal(|ui| {
            tw::label(
                ui,
                "Filter",
                "Only the lines holding this text (any case); the newest 4000 lines are kept by \
                 the server",
            );
            if ui
                .text_edit_singleline(&mut self.pages.log_filter)
                .changed()
            {
                self.again("log");
            }
        });
        let Some(c) = &self.conn else { return };
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT + 120.0)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for l in &c.data.log {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  {}",
                            ohl_server::admin::utc_text(l.time),
                            l.text
                        ))
                        .monospace(),
                    );
                }
            });
    }

    fn sessions_tab(&mut self, ui: &mut egui::Ui) {
        if self.due("sessions", Duration::from_secs(5))
            && let Some(c) = &mut self.conn
        {
            c.ask_sessions();
        }
        let Some(c) = &self.conn else { return };
        let me = c.user().to_owned();
        let rows = c.data.sessions.clone();
        let mut let_go = None;
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                egui::Grid::new("sessions")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["User", "Since", "Follows", "Connection", ""] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in &rows {
                            ui.label(match &r.see_as {
                                Some(s) => format!("{} (seeing as {s})", r.user),
                                None => r.user.clone(),
                            });
                            ui.label(ohl_server::admin::utc_text(r.since));
                            ui.label(&r.follows);
                            ui.label(r.client.to_string());
                            if r.user != me && ui.button("Let go…").clicked() {
                                let_go = Some((r.client, r.user.clone()));
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some((client, user)) = let_go {
            self.window = Some(Window::Disconnect {
                client,
                user,
                reason: String::new(),
            });
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        if self.due("settings", Duration::from_secs(60))
            && let Some(c) = &mut self.conn
        {
            c.ask_settings();
        }
        let Some(c) = &self.conn else { return };
        ui.label(
            "Read-only: \"start\" settings are the server's start options (its command line or \
             configuration — changed by starting it again); \"network\" settings are the \
             network's own (ohl network setting).",
        );
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                egui::Grid::new("settings")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Setting", "Value", "From", "What it is"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for s in &c.data.settings {
                            ui.label(&s.key);
                            ui.label(&s.value);
                            ui.label(&s.source);
                            ui.label(&s.note);
                            ui.end_row();
                        }
                    });
            });
    }

    /// What the map highlights now.
    fn highlight(&self) -> Highlight {
        let mut h = Highlight::default();
        let Some(c) = &self.conn else { return h };
        if let (Some(id), Some(r)) = (self.pages.selected, &c.data.rights)
            && let Some(a) = r.assignments.iter().find(|a| a.id == id)
        {
            let s = &a.scope;
            match (&s.route, &s.line, &s.region) {
                (Some(route), _, _) => {
                    h.routes.insert(route.clone());
                }
                (None, Some(l), _) => {
                    h.lines.insert(l.clone());
                }
                (None, None, Some(poly)) => h.regions.push(poly.clone()),
                (None, None, None) => h.network = s.is_network(),
            }
        }
        if self.pages.see_as_on_map
            && let Some(sa) = &c.data.see_as
        {
            h.only = Some(sa.routes.iter().cloned().collect());
        }
        h
    }

    fn map_page(&mut self, ui: &mut egui::Ui) {
        if self.due("map", Duration::from_secs(30))
            && let Some(c) = &mut self.conn
        {
            c.ask_map();
        }
        ui.horizontal_wrapped(|ui| {
            ui.strong("Map");
            tw::question(
                ui,
                "Read-only and simplified: routes coloured by voltage, owner or state, shared \
                 towers marked, structures as dots past a zoom. Drag to pan, scroll to zoom, click \
                 a route or a structure to pick it (an assignment's scope).",
            );
            ui.separator();
            ui.label("Colour by");
            ui.radio_value(&mut self.map.colour_by, ColourBy::Voltage, "voltage");
            ui.radio_value(&mut self.map.colour_by, ColourBy::Owner, "owner");
            ui.radio_value(&mut self.map.colour_by, ColourBy::State, "state");
            ui.separator();
            ui.checkbox(&mut self.map.assignments, "assignments");
            ui.checkbox(&mut self.map.working_sets, "working sets in use");
            ui.checkbox(&mut self.map.locks, "locks");
            ui.checkbox(&mut self.map.stale, "stale results");
            ui.separator();
            if ui.button("Fit").clicked() {
                self.map.scale = 0.0;
            }
            if ui
                .button("Region outlines…")
                .on_hover_text(
                    "Import outlines from a GeoJSON file (in the network's coordinates); kept \
                     with this app on this computer, never on the server",
                )
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("GeoJSON", &["geojson", "json"])
                    .pick_file()
            {
                self.import_regions(&path);
            }
            if !self.map.regions.is_empty() && ui.button("Clear outlines").clicked() {
                self.map.regions.clear();
            }
        });
        ui.horizontal_wrapped(|ui| match self.map.colour_by {
            ColourBy::Voltage => {
                for (words, kv) in VOLTAGE_LEGEND {
                    ui.colored_label(voltage_colour(kv), format!("━ {words}"));
                }
            }
            ColourBy::Owner => {
                ui.weak("each owner its colour (grey: none given)");
            }
            ColourBy::State => {
                ui.colored_label(crate::map::STALE, "━ results out of date");
                ui.colored_label(crate::map::IN_USE, "━ in use");
                ui.colored_label(crate::map::LOCKED, "━ locks held");
                ui.colored_label(crate::map::QUIET, "━ none of these");
            }
        });
        let picked = self.map.picked.clone();
        if let Some(p) = &picked {
            ui.horizontal(|ui| {
                ui.label(match p {
                    Picked::Route { model, route } => {
                        format!("Picked: route {route} of line {model}")
                    }
                    Picked::Structure { model, id } => {
                        format!("Picked: structure {id} of line {model}")
                    }
                });
                if ui.button("Use as the assignment's scope").clicked() {
                    self.pages.scope = match p {
                        Picked::Route { model, route } => Scope {
                            line: Some(model.clone()),
                            route: Some(route.clone()),
                            ..Scope::default()
                        },
                        Picked::Structure { model, id } => Scope {
                            line: Some(model.clone()),
                            kind: Some("structure".into()),
                            object: Some(id.clone()),
                            ..Scope::default()
                        },
                    };
                    self.page = Page::Assignments;
                }
            });
        }
        if ui
            .small_button("Use the view as a region scope")
            .on_hover_text("The rectangle in view, as an assignment's region")
            .clicked()
            && let Some(r) = self.map.view_region()
        {
            self.pages.scope = Scope {
                region: Some(r),
                ..Scope::default()
            };
            self.page = Page::Assignments;
        }
        let hl = self.highlight();
        let Some(c) = &mut self.conn else { return };
        let Some(map) = c.data.map.clone() else {
            ui.spinner();
            return;
        };
        let want = self.map.ui(ui, &map, &c.data.dots, &hl);
        if let Some(b) = want {
            c.ask_dots(b);
        }
    }

    /// Imports region outlines from a GeoJSON file.
    pub fn import_regions(&mut self, path: &std::path::Path) {
        let r = std::fs::read_to_string(path)
            .map_err(|e| format!("{}: {e}", path.display()))
            .and_then(|t| crate::map::regions_from_geojson(&t));
        let said = match r {
            Ok(r) => {
                let n = r.len();
                self.map.regions = r;
                (format!("{n} region outline(s) imported"), false)
            }
            Err(e) => (format!("the region outlines: {e}"), true),
        };
        if let Some(c) = &mut self.conn {
            c.data.said.push(said);
        }
    }

    fn rights_needed(&mut self) {
        if self.due("rights", Duration::from_secs(30))
            && let Some(c) = &mut self.conn
        {
            c.ask_rights();
        }
    }

    fn users_page(&mut self, ui: &mut egui::Ui) {
        self.rights_needed();
        Self::page_intro(
            ui,
            Page::Users,
            "The network's users and groups: invitations (they expire), disabling, groups and \
             their members.",
            "Users, groups and memberships are changed by network admins only; others see the \
             list (to pick whom to assign). Every change shows what changes before it is saved.",
        );
        let Some(c) = &mut self.conn else { return };
        let Some(r) = c.data.rights.clone() else {
            ui.spinner();
            return;
        };
        let admin = r.network_admin;
        let mut open = None;
        let mut reissue = None;
        if admin && ui.button("Invite a user…").clicked() {
            open = Some(Window::Invite {
                name: String::new(),
                display: String::new(),
                days: ohl_server::network::DEFAULT_INVITE_DAYS,
            });
        }
        if !admin {
            ui.weak("Read-only: network admins change users and groups.");
        }
        tw::heading(
            ui,
            "Users",
            "Everyone the network knows; a disabled user has no rights at all.",
        );
        egui::ScrollArea::vertical()
            .id_salt("users")
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                egui::Grid::new("users")
                    .num_columns(6)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["User", "State", "Groups", "", "", ""] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for u in &r.users {
                            ui.label(&u.name);
                            ui.label(format!(
                                "{}{}{}",
                                if u.active { "active" } else { "disabled" },
                                if u.network_admin {
                                    ", network admin"
                                } else {
                                    ""
                                },
                                if u.connected { ", connected" } else { "" }
                            ));
                            ui.label(u.groups.join(", "));
                            if admin {
                                let words = if u.active { "Disable…" } else { "Enable…" };
                                if ui.button(words).clicked() {
                                    open = Some(Window::Change(RightsCommand::SetActive {
                                        name: u.name.clone(),
                                        active: !u.active,
                                    }));
                                }
                                if ui
                                    .button("Reissue invitation")
                                    .on_hover_text(
                                        "A new personal invitation: every older one is refused \
                                         from now on (the user is let go if connected)",
                                    )
                                    .clicked()
                                {
                                    reissue = Some(u.name.clone());
                                }
                            } else {
                                ui.label("");
                                ui.label("");
                            }
                            ui.label("");
                            ui.end_row();
                        }
                    });
            });
        ui.add_space(8.0);
        tw::heading(
            ui,
            "Groups",
            "A role given to a group reaches every member.",
        );
        let users: Vec<String> = r.users.iter().map(|u| u.name.clone()).collect();
        egui::ScrollArea::vertical()
            .id_salt("groups")
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                for (g, members) in &r.groups {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(g);
                        for m in members {
                            ui.label(m);
                            if admin && ui.small_button("✕").on_hover_text("Take out").clicked() {
                                open = Some(Window::Change(RightsCommand::RemoveMember {
                                    group: g.clone(),
                                    user: m.clone(),
                                }));
                            }
                        }
                        if admin {
                            let pick = self.pages.member.entry(g.clone()).or_default();
                            egui::ComboBox::from_id_salt(("member", g))
                                .selected_text(if pick.is_empty() {
                                    "user…"
                                } else {
                                    pick.as_str()
                                })
                                .show_ui(ui, |ui| {
                                    for u in users.iter().filter(|u| !members.contains(u)) {
                                        ui.selectable_value(pick, u.clone(), u);
                                    }
                                });
                            if ui
                                .add_enabled(!pick.is_empty(), egui::Button::new("Add"))
                                .clicked()
                            {
                                open = Some(Window::Change(RightsCommand::AddMember {
                                    group: g.clone(),
                                    user: pick.clone(),
                                }));
                            }
                            if ui.button("Delete group…").clicked() {
                                open = Some(Window::Change(RightsCommand::DeleteGroup {
                                    name: g.clone(),
                                }));
                            }
                        }
                    });
                }
            });
        if admin {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.pages.new_group);
                let name = self.pages.new_group.trim().to_owned();
                if ui
                    .add_enabled(!name.is_empty(), egui::Button::new("Add group…"))
                    .clicked()
                {
                    open = Some(Window::Change(RightsCommand::AddGroup {
                        name,
                        note: String::new(),
                    }));
                }
            });
        }
        if let Some(name) = reissue {
            c.invite(&name, None, None, true);
        }
        if open.is_some() {
            self.window = open;
        }
    }

    fn roles_page(&mut self, ui: &mut egui::Ui) {
        self.rights_needed();
        Self::page_intro(
            ui,
            Page::Roles,
            "Roles are sets of permissions: rows the roles, columns the permissions. The shipped \
             roles are read-only; the network's own roles are edited here.",
            "Roles are changed by network admins only; a role in use cannot be deleted. A role's \
             change reaches every scope its assignments name: what changes is shown first.",
        );
        let Some(c) = &self.conn else { return };
        let Some(r) = c.data.rights.clone() else {
            ui.spinner();
            return;
        };
        let admin = r.network_admin;
        let mut open = None;
        egui::ScrollArea::both()
            .max_height(LIST_HEIGHT + 120.0)
            .show(ui, |ui| {
                egui::Grid::new("roles")
                    .num_columns(Permission::ALL.len() + 2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Role");
                        for p in Permission::ALL {
                            ui.strong(p.as_str().replace('_', " "));
                        }
                        ui.label("");
                        ui.end_row();
                        for role in &r.roles {
                            ui.label(if role.shipped {
                                format!("{} (shipped)", role.name)
                            } else {
                                role.name.clone()
                            });
                            let editable = admin && !role.shipped;
                            let current: BTreeSet<Permission> =
                                role.permissions.iter().copied().collect();
                            for p in Permission::ALL {
                                let mut on = self
                                    .pages
                                    .role_edits
                                    .get(&role.name)
                                    .map_or(current.contains(&p), |e| e.contains(&p));
                                if ui
                                    .add_enabled(editable, egui::Checkbox::without_text(&mut on))
                                    .changed()
                                {
                                    let e = self
                                        .pages
                                        .role_edits
                                        .entry(role.name.clone())
                                        .or_insert_with(|| current.clone());
                                    if on {
                                        e.insert(p);
                                    } else {
                                        e.remove(&p);
                                    }
                                }
                            }
                            ui.horizontal(|ui| {
                                if let Some(e) = self.pages.role_edits.get(&role.name)
                                    && *e != current
                                    && ui.button("Save…").clicked()
                                {
                                    open = Some(Window::Change(RightsCommand::PutRole {
                                        name: role.name.clone(),
                                        permissions: e.iter().copied().collect(),
                                        note: String::new(),
                                    }));
                                }
                                if editable && ui.button("Delete…").clicked() {
                                    open = Some(Window::Change(RightsCommand::DeleteRole {
                                        name: role.name.clone(),
                                    }));
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        if admin {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.pages.new_role);
                let name = self.pages.new_role.trim().to_owned();
                if ui
                    .add_enabled(!name.is_empty(), egui::Button::new("Add role…"))
                    .on_hover_text("A new role that may see; tick its other permissions after")
                    .clicked()
                {
                    open = Some(Window::Change(RightsCommand::PutRole {
                        name,
                        permissions: vec![Permission::See],
                        note: String::new(),
                    }));
                }
            });
        }
        if open.is_some() {
            self.window = open;
        }
    }

    fn assignments_page(&mut self, ui: &mut egui::Ui) {
        self.rights_needed();
        if self.due("libraries", Duration::from_secs(60))
            && let Some(c) = &mut self.conn
        {
            c.ask_libraries();
            if c.data.map.is_none() {
                c.ask_map();
            }
        }
        Self::page_intro(
            ui,
            Page::Assignments,
            "Who holds which role where: a user or a group, a role, a scope down to a single \
             object or library entry. Default deny; a deny is set by network admins only.",
            "The scope is picked in the tree (network › line › route › structure), in the library \
             list, or on the map (Map › Use as the assignment's scope). What changes is shown \
             before saving. A selected assignment is shown on the map.",
        );
        let Some(c) = &mut self.conn else { return };
        let Some(r) = c.data.rights.clone() else {
            ui.spinner();
            return;
        };
        let mut open = None;
        let mut ask_route: Option<(String, String)> = None;
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            tw::heading(
                ui,
                "Assignments",
                "The assignments whose scope you manage. Click one to show it on the map.",
            );
            egui::ScrollArea::vertical()
                .id_salt("assignments")
                .max_height(LIST_HEIGHT + 120.0)
                .show(ui, |ui| {
                    for (subject, list) in ohl_server::admin::by_subject(&r.assignments) {
                        ui.strong(subject);
                        for a in list {
                            ui.horizontal(|ui| {
                                let text = format!(
                                    "#{} {} {} on {}",
                                    a.id,
                                    if a.deny { "denied" } else { "is" },
                                    a.role.as_deref().unwrap_or("everything"),
                                    a.scope.describe()
                                );
                                if ui
                                    .selectable_label(self.pages.selected == Some(a.id), text)
                                    .clicked()
                                {
                                    self.pages.selected = Some(a.id);
                                }
                                if ui.small_button("Remove…").clicked() {
                                    open =
                                        Some(Window::Change(RightsCommand::Unassign { id: a.id }));
                                }
                            });
                        }
                    }
                });
            let ui = &mut cols[1];
            tw::heading(
                ui,
                "New assignment",
                "Who, which role, where; then what changes.",
            );
            let p = &mut self.pages;
            egui::Grid::new("new-assignment")
                .num_columns(2)
                .show(ui, |ui| {
                    tw::label(ui, "Who", "A user or a group");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut p.subject_group, false, "user");
                        ui.radio_value(&mut p.subject_group, true, "group");
                        let names: Vec<String> = if p.subject_group {
                            r.groups.iter().map(|g| g.0.clone()).collect()
                        } else {
                            r.users.iter().map(|u| u.name.clone()).collect()
                        };
                        egui::ComboBox::from_id_salt("subject")
                            .selected_text(if p.subject.is_empty() {
                                "…"
                            } else {
                                p.subject.as_str()
                            })
                            .show_ui(ui, |ui| {
                                for n in names {
                                    ui.selectable_value(&mut p.subject, n.clone(), n);
                                }
                            });
                    });
                    ui.end_row();
                    tw::label(
                        ui,
                        "Role",
                        "Its permissions are given (or, with deny, denied)",
                    );
                    egui::ComboBox::from_id_salt("role")
                        .selected_text(if p.role.is_empty() {
                            "…"
                        } else {
                            p.role.as_str()
                        })
                        .show_ui(ui, |ui| {
                            for role in &r.roles {
                                ui.selectable_value(&mut p.role, role.name.clone(), &role.name);
                            }
                        });
                    ui.end_row();
                    if r.network_admin {
                        tw::label(
                            ui,
                            "Deny",
                            "Denies the role's permissions there (network admins only)",
                        );
                        ui.checkbox(&mut p.deny, "");
                        ui.end_row();
                    }
                    tw::label(ui, "Note", "Why (kept with the assignment)");
                    ui.text_edit_singleline(&mut p.note);
                    ui.end_row();
                    tw::label(ui, "Scope", "Where: pick below or on the map");
                    ui.label(p.scope.describe());
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                if r.network_admin && ui.button("Whole network").clicked() {
                    p.scope = Scope::network();
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("scope-tree")
                .max_height(LIST_HEIGHT)
                .show(ui, |ui| {
                    egui::CollapsingHeader::new("Lines and routes")
                        .default_open(true)
                        .show(ui, |ui| {
                            let routes = c
                                .data
                                .map
                                .as_ref()
                                .map(|m| m.routes.clone())
                                .unwrap_or_default();
                            for line in &r.manages_lines {
                                let name = c
                                    .data
                                    .map
                                    .as_ref()
                                    .and_then(|m| m.lines.iter().find(|l| &l.0 == line))
                                    .map(|l| l.1.clone())
                                    .unwrap_or_default();
                                egui::CollapsingHeader::new(format!("{line} {name}"))
                                    .id_salt(("line", line))
                                    .show(ui, |ui| {
                                        if ui.small_button("this line").clicked() {
                                            p.scope = Scope::line(line);
                                        }
                                        for rt in routes.iter().filter(|x| &x.model == line) {
                                            let h = egui::CollapsingHeader::new(format!(
                                                "route {} {}",
                                                rt.id, rt.name
                                            ))
                                            .id_salt(("route", &rt.id))
                                            .show(ui, |ui| {
                                                if ui.small_button("this route").clicked() {
                                                    p.scope = Scope {
                                                        line: Some(line.clone()),
                                                        route: Some(rt.id.clone()),
                                                        ..Scope::default()
                                                    };
                                                }
                                                let key = (line.clone(), rt.id.clone());
                                                match c.data.route_structures.get(&key) {
                                                    None => {
                                                        ui.spinner();
                                                    }
                                                    Some(list) => {
                                                        for s in list {
                                                            if ui
                                                                .selectable_label(
                                                                    p.scope.object.as_deref()
                                                                        == Some(s.id.as_str()),
                                                                    format!(
                                                                        "structure {}{}",
                                                                        s.id,
                                                                        if s.shared {
                                                                            " (shared)"
                                                                        } else {
                                                                            ""
                                                                        }
                                                                    ),
                                                                )
                                                                .clicked()
                                                            {
                                                                p.scope = Scope {
                                                                    line: Some(line.clone()),
                                                                    kind: Some("structure".into()),
                                                                    object: Some(s.id.clone()),
                                                                    ..Scope::default()
                                                                };
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                            if h.body_returned.is_some() {
                                                ask_route = Some((line.clone(), rt.id.clone()));
                                            }
                                        }
                                    });
                            }
                        });
                    egui::CollapsingHeader::new("Libraries").show(ui, |ui| {
                        let Some(libs) = &c.data.libraries else {
                            ui.spinner();
                            return;
                        };
                        for k in libs {
                            if !r.network_admin && !r.manages_libraries.contains(&k.kind) {
                                continue;
                            }
                            egui::CollapsingHeader::new(format!(
                                "{}{}",
                                k.kind,
                                if k.locked { " (locked)" } else { "" }
                            ))
                            .id_salt(("kind", &k.kind))
                            .show(ui, |ui| {
                                if ui.small_button("this kind").clicked() {
                                    p.scope = Scope {
                                        library_kind: Some(k.kind.clone()),
                                        ..Scope::default()
                                    };
                                }
                                for f in &k.files {
                                    egui::CollapsingHeader::new(&f.file)
                                        .id_salt(("file", &k.kind, &f.file))
                                        .show(ui, |ui| {
                                            if ui.small_button("this file").clicked() {
                                                p.scope = Scope {
                                                    library_kind: Some(k.kind.clone()),
                                                    library_file: Some(f.file.clone()),
                                                    ..Scope::default()
                                                };
                                            }
                                            for e in &f.entries {
                                                if ui.selectable_label(false, e).clicked() {
                                                    p.scope = Scope {
                                                        library_kind: Some(k.kind.clone()),
                                                        library_file: Some(f.file.clone()),
                                                        library_entry: Some(e.clone()),
                                                        ..Scope::default()
                                                    };
                                                }
                                            }
                                        });
                                }
                            });
                        }
                    });
                });
            let ready = !p.subject.is_empty() && (!p.role.is_empty() || p.deny);
            let r2 = ui
                .add_enabled(
                    ready,
                    egui::Button::new(egui::RichText::new("Show what changes…").strong()),
                )
                .on_disabled_hover_text("Pick who and a role first");
            if r2.clicked() {
                let subject = if p.subject_group {
                    Subject::Group(p.subject.clone())
                } else {
                    Subject::User(p.subject.clone())
                };
                open = Some(Window::Change(RightsCommand::Assign {
                    subject,
                    role: (!p.role.is_empty()).then(|| p.role.clone()),
                    deny: p.deny && r.network_admin,
                    scope: p.scope.clone(),
                    note: p.note.clone(),
                }));
            }
        });
        if let Some((m, rt)) = ask_route {
            c.ask_route(&m, &rt);
        }
        if open.is_some() {
            self.window = open;
        }
    }

    fn see_as_page(&mut self, ui: &mut egui::Ui) {
        self.rights_needed();
        Self::page_intro(
            ui,
            Page::SeeAs,
            "What a user sees: the server's own filter, as both you and they may see (an admin \
             learns nothing they may not see themselves).",
            "Each look is in the network's audit. Tick \"on the map\" to show only the routes \
             they see.",
        );
        let Some(c) = &mut self.conn else { return };
        let users: Vec<String> = c
            .data
            .rights
            .as_ref()
            .map(|r| r.users.iter().map(|u| u.name.clone()).collect())
            .unwrap_or_default();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("see-as")
                .selected_text(if self.pages.see_as.is_empty() {
                    "user…"
                } else {
                    self.pages.see_as.as_str()
                })
                .show_ui(ui, |ui| {
                    for u in users {
                        ui.selectable_value(&mut self.pages.see_as, u.clone(), u);
                    }
                });
            if ui
                .add_enabled(!self.pages.see_as.is_empty(), egui::Button::new("See as"))
                .clicked()
            {
                c.ask_see_as(&self.pages.see_as.clone());
            }
            ui.checkbox(&mut self.pages.see_as_on_map, "on the map");
        });
        let Some(sa) = &c.data.see_as else { return };
        ui.label(format!(
            "{} sees {} line(s), {} route(s):",
            sa.user,
            sa.lines.len(),
            sa.routes.len()
        ));
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                egui::Grid::new("see-as-lines")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for l in &sa.lines {
                            ui.label(format!(
                                "{}{}",
                                l.line,
                                if l.whole { " (whole)" } else { "" }
                            ));
                            ui.label(
                                l.counts
                                    .iter()
                                    .map(|(k, s, a)| format!("{k} {s} of {a}"))
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            );
                            ui.end_row();
                        }
                    });
                // The tree: line › the routes they see.
                let routes = c
                    .data
                    .map
                    .as_ref()
                    .map(|m| m.routes.clone())
                    .unwrap_or_default();
                let seen: BTreeSet<&String> = sa.routes.iter().collect();
                for l in &sa.lines {
                    egui::CollapsingHeader::new(format!("line {}", l.line))
                        .id_salt(("see-as-line", &l.line))
                        .show(ui, |ui| {
                            for r in routes
                                .iter()
                                .filter(|r| r.model == l.line && seen.contains(&r.id))
                            {
                                ui.label(format!("route {} {}", r.id, r.name));
                            }
                        });
                }
            });
    }

    /// The windows on top: the change ("what changes", then saved), an
    /// invitation, letting a connection go, an invitation made.
    pub(crate) fn windows(&mut self, ctx: &egui::Context) {
        self.invitation_window(ctx);
        let Some(w) = self.window.clone() else { return };
        let Some(c) = &mut self.conn else {
            self.window = None;
            return;
        };
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut open = true;
        let mut close = escape;
        match w {
            Window::Change(cmd) => {
                if self.pages.previewed.as_ref() != Some(&cmd) {
                    c.ask_preview(cmd.clone());
                    self.pages.previewed = Some(cmd.clone());
                }
                egui::Window::new("Change the rights")
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        tw::intro(
                            ui,
                            &format!("To {}.", command_words(&cmd)),
                            "What changes is worked out on a copy of the rights, nothing saved; \
                             the server's rights function decides when it is saved, and it is in \
                             the audit.",
                            "change",
                            || crate::help::open_section("user-guide/ohl-server", "what-changes"),
                        );
                        let preview = c.data.preview.as_ref().filter(|(p, _)| *p == cmd);
                        match preview {
                            None => {
                                ui.spinner();
                            }
                            Some((_, w)) => {
                                ui.strong(what_changes_words(w));
                                for n in &w.notes {
                                    ui.label(n);
                                }
                                egui::ScrollArea::vertical()
                                    .max_height(240.0)
                                    .show(ui, |ui| {
                                        for u in &w.users {
                                            user_change(ui, u);
                                        }
                                    });
                            }
                        }
                        let ready = preview.is_some();
                        match tw::buttons(
                            ui,
                            "Save",
                            ready,
                            Some("What changes comes first"),
                            tw::Keys::Escape,
                        ) {
                            Some(tw::Pressed::Action) => {
                                c.change(cmd.clone());
                                if let RightsCommand::PutRole { name, .. } = &cmd {
                                    self.pages.role_edits.remove(name);
                                }
                                close = true;
                            }
                            Some(tw::Pressed::Cancel) => close = true,
                            None => {}
                        }
                    });
            }
            Window::Invite {
                mut name,
                mut display,
                mut days,
            } => {
                egui::Window::new("Invite a user")
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        tw::intro(
                            ui,
                            "A user added to the network (no rights until assigned) and their \
                             personal invitation made, to pass on to them.",
                            "The invitation expires after the days given; reissue it from the \
                             users' list.",
                            "invite",
                            || {
                                crate::help::open_section(
                                    "user-guide/ohl-server",
                                    "users-and-groups",
                                )
                            },
                        );
                        egui::Grid::new("invite").num_columns(2).show(ui, |ui| {
                            tw::label(ui, "Name", "As they sign in");
                            ui.text_edit_singleline(&mut name);
                            ui.end_row();
                            tw::label(ui, "Display name", "Optional");
                            ui.text_edit_singleline(&mut display);
                            ui.end_row();
                            tw::label(ui, "Valid", "Days, 1 to 3650");
                            ohl_ui::style::field(
                                ui,
                                egui::DragValue::new(&mut days).range(1..=3650).suffix(" d"),
                            );
                            ui.end_row();
                        });
                        let ok = !name.trim().is_empty();
                        match tw::buttons(ui, "Invite", ok, Some("A name first"), tw::Keys::Both) {
                            Some(tw::Pressed::Action) => {
                                let d = display.trim();
                                c.invite(
                                    name.trim(),
                                    (!d.is_empty()).then(|| d.to_owned()),
                                    Some(days),
                                    false,
                                );
                                close = true;
                            }
                            Some(tw::Pressed::Cancel) => close = true,
                            None => {}
                        }
                    });
                if !close {
                    self.window = Some(Window::Invite {
                        name,
                        display,
                        days,
                    });
                }
            }
            Window::Disconnect {
                client,
                user,
                mut reason,
            } => {
                egui::Window::new("Let a connection go")
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        tw::intro(
                            ui,
                            &format!("{user}'s connection {client} is let go; they are shown why."),
                            "They may join again at once (disable the user to keep them out). \
                             In the audit.",
                            "disconnect",
                            || crate::help::open_section("user-guide/ohl-server", "sessions"),
                        );
                        tw::label(ui, "Reason", "Shown to the user, kept in the audit");
                        ui.text_edit_singleline(&mut reason);
                        let ok = !reason.trim().is_empty();
                        match tw::buttons(ui, "Let go", ok, Some("Say why first"), tw::Keys::Escape)
                        {
                            Some(tw::Pressed::Action) => {
                                c.disconnect(client, reason.trim());
                                close = true;
                            }
                            Some(tw::Pressed::Cancel) => close = true,
                            None => {}
                        }
                    });
                if !close {
                    self.window = Some(Window::Disconnect {
                        client,
                        user,
                        reason,
                    });
                }
            }
        }
        if close || !open {
            self.window = None;
            self.pages.previewed = None;
            if let Some(c) = &mut self.conn {
                c.data.preview = None;
            }
        }
    }

    fn invitation_window(&mut self, ctx: &egui::Context) {
        let Some(c) = &mut self.conn else { return };
        let Some(inv) = c.data.invitation.clone() else {
            return;
        };
        let text = c.invitation_of(&inv.code).to_string();
        let mut open = true;
        let mut done = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Invitation")
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                tw::intro(
                    ui,
                    &format!(
                        "{}'s personal invitation: pass it on to them by a safe way. It is shown \
                         once and kept nowhere by this app.",
                        inv.user
                    ),
                    "Anyone holding it signs in as that user: treat it like a password.",
                    "invitation",
                    || crate::help::open_section("user-guide/ohl-server", "users-and-groups"),
                );
                let mut shown = text.clone();
                ui.add(egui::TextEdit::singleline(&mut shown).desired_width(520.0));
                ui.label(match &inv.until {
                    Some(d) => format!("Valid until {d} (UTC)."),
                    None => "Valid until reissued.".into(),
                });
                ui.horizontal(|ui| {
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(text.clone());
                    }
                    if ui.button("Close").clicked() {
                        done = true;
                    }
                });
            });
        if done || !open {
            c.data.invitation = None;
        }
    }
}

fn user_change(ui: &mut egui::Ui, u: &UserChange) {
    ui.strong(&u.user);
    for g in &u.gained {
        ui.colored_label(egui::Color32::from_rgb(40, 150, 60), format!("+ {g}"));
    }
    for l in &u.lost {
        ui.colored_label(egui::Color32::from_rgb(200, 50, 50), format!("− {l}"));
    }
}
