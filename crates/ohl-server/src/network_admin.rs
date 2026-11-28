// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The admin messages on a network's server (2026-11-28, OHL-Server the
//! admin app, stages A and B; [`crate::admin`] for the messages and their
//! rules). Answered on the authority from what it holds (its clients, its
//! models' lines and places, its rights as read); a rights change through
//! the database's one function ([`ohl_store_pg::Network::rights_command`],
//! audited there); the long work (a backup, a restore drill) on a thread
//! of its own, one at a time, answered through the jobs when done.

use std::sync::atomic::Ordering as AtomicOrdering;

use ohl_session::rights::{Permission, RightsCommand, Scope, Target};

use super::*;
use crate::admin::{
    AdminAsk, AdminMap, AdminReply, BackupList, BackupRow, InvitationText, LibraryFileRow,
    LibraryKindRow, MapDot, MapRoute, Overview, RightsView, RoleRow, SeeAsView, SeeLine,
    SessionRow, SettingRow, UserRow, unix_now,
};

/// The age of the last backup past which the Overview warns, hours (the
/// owner's plan: older than 24 h).
const BACKUP_WARN_H: f64 = 24.0;

/// Points of one alignment on the map at most (a simplified polyline).
const MAP_POINTS: usize = 64;

impl NetHost {
    /// Answers admin message `ask` of `client` (`user`): pushed on its own
    /// queue now, or, for long work, when done; a refusal returned (and
    /// audited).
    pub(super) fn admin(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        ask: AdminAsk,
    ) -> Result<(), String> {
        if !self.clients.contains_key(&client) {
            return Err("not joined (let go): join again".into());
        }
        // An admin seeing as another acts with what both may: no admin
        // message then.
        if self
            .clients
            .get(&client)
            .is_some_and(|c| c.see_as.is_some())
        {
            return Err("a connection seeing as another user asks no admin questions".into());
        }
        let what = format!("admin: {}", ask.describe());
        let me = self.rights_of(user);
        if !me.manages_anything() && maintained_kinds(&self.rights, user).is_empty() {
            let why = format!(
                "{user} manages no rights on this network: OHL-Server is for network admins, line \
                 admins (their lines) and library maintainers (their libraries)"
            );
            self.audit_refused(user, &what, &why);
            return Err(why);
        }
        let network_admin = me.allows(Permission::ManageRights, &Target::Network);
        let admins_only = matches!(
            ask,
            AdminAsk::Sessions
                | AdminAsk::Disconnect { .. }
                | AdminAsk::Log { .. }
                | AdminAsk::Backups
                | AdminAsk::BackupNow
                | AdminAsk::RestoreDrill { .. }
                | AdminAsk::Settings
                | AdminAsk::Invite { .. }
                | AdminAsk::Reissue { .. }
        );
        if admins_only && !network_admin {
            let why = format!(
                "{user} may not ask for the {}: network admins only",
                ask.describe()
            );
            self.audit_refused(user, &what, &why);
            return Err(why);
        }
        let reply = match ask {
            AdminAsk::Overview => AdminReply::Overview(Box::new(self.overview(&me, network_admin))),
            AdminAsk::Sessions => AdminReply::Sessions(self.sessions()),
            AdminAsk::Disconnect {
                client: other,
                reason,
            } => AdminReply::Done(self.disconnect(client, user, other, &reason)?),
            AdminAsk::Log { after, filter } => {
                AdminReply::Log(crate::admin::log_page(after, &filter))
            }
            AdminAsk::Backups => AdminReply::Backups(self.backups()),
            AdminAsk::BackupNow => {
                self.backup_now(client, user, id)?;
                return Ok(());
            }
            AdminAsk::RestoreDrill { backup } => {
                self.restore_drill(client, user, id, &backup)?;
                return Ok(());
            }
            AdminAsk::Settings => AdminReply::Settings(self.settings()),
            AdminAsk::Rights => AdminReply::Rights(Box::new(self.rights_view(&me, network_admin))),
            AdminAsk::Change { command } => {
                let done = self
                    .net
                    .rights_command(user, &command)
                    .map_err(|e| e.to_string())?;
                log(&format!("{user} changed the rights: {done}"));
                // Answered first, then the rights read again (a client
                // whose rights changed is let go: perhaps this one).
                self.push(
                    client,
                    None,
                    None,
                    Some(ServerMsg::Admin {
                        id,
                        result: Ok(AdminReply::Done(done)),
                    }),
                );
                self.refresh_rights();
                return Ok(());
            }
            AdminAsk::Preview { command } => {
                AdminReply::Preview(Box::new(self.preview(&me, network_admin, &command)))
            }
            AdminAsk::Invite {
                name,
                display,
                days,
            } => AdminReply::Invitation(self.invite(user, &name, display, days, false)?),
            AdminAsk::Reissue { name, days } => {
                AdminReply::Invitation(self.invite(user, &name, None, days, true)?)
            }
            AdminAsk::SeeAs { user: who } => {
                AdminReply::SeeAs(Box::new(self.see_as_view(user, &me, &who)?))
            }
            AdminAsk::Map => AdminReply::Map(Box::new(self.admin_map(&me))),
            AdminAsk::Structures { bounds } => AdminReply::Structures(self.dots(&me, bounds)?),
            AdminAsk::RouteStructures { model, route } => {
                AdminReply::Structures(self.route_dots(&me, &model, &route)?)
            }
            AdminAsk::Libraries => AdminReply::Libraries(self.libraries(&me)?),
        };
        self.push(
            client,
            None,
            None,
            Some(ServerMsg::Admin {
                id,
                result: Ok(reply),
            }),
        );
        Ok(())
    }

    fn audit_admin(&self, user: &str, action: &str, target: &str, detail: &str) {
        if let Err(e) = self.net.audit(user, action, target, detail) {
            log(&format!("the audit: {e}"));
        }
    }

    /// The last backup recorded: its time and age, hours.
    fn last_backup(&self) -> (Option<String>, Option<f64>) {
        let Some(v) = self.net.last_backup().ok().flatten() else {
            return (None, None);
        };
        let time = v.get("time").and_then(|t| t.as_str()).map(str::to_owned);
        let age = v
            .get("epoch")
            .and_then(serde_json::Value::as_f64)
            .map(|e| ((unix_now() as f64 - e) / 3600.0).max(0.0));
        (time, age)
    }

    fn overview(&mut self, me: &UserRights, full: bool) -> Overview {
        let mut o = Overview {
            full,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: crate::PROTOCOL,
            admin: crate::admin::ADMIN,
            uptime_s: self.born.elapsed().as_secs(),
            network: self.name.clone(),
            program_schema: ohl_store_pg::schema::SCHEMA,
            backup_max_age_h: ohl_store_pg::backup::DEFAULT_MAX_AGE_HOURS,
            ..Overview::default()
        };
        for (code, m) in &self.models {
            if !full && !me.sees_some_of_line(code) {
                continue;
            }
            o.models += 1;
            let line = m.session.line();
            o.routes += line.routes.len();
            o.structures += line.structures.len();
            if full {
                o.stale_routes += m.session.stale_routes().map(|s| s.len()).unwrap_or(0);
            }
        }
        if !full {
            o.connections = self.clients.len();
            return o;
        }
        o.schema = self.net.schema().unwrap_or(0);
        o.connected = self.common.peers.keys().cloned().collect();
        o.connections = self.clients.len();
        o.working_sets = self.clients.values().filter(|c| c.ws.is_some()).count();
        o.queue_waiting = self.heavy.len();
        o.check_running = self.check_running;
        o.export_running = self.exports.client_running();
        if let Some(h) = self
            .net
            .setting("backup_max_age_hours")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|h| h.is_finite() && *h > 0.0)
        {
            o.backup_max_age_h = h;
        }
        (o.last_backup, o.backup_age_h) = self.last_backup();
        match o.backup_age_h {
            None => o
                .warnings
                .push("no backup is recorded: make one (Operations › Backups)".into()),
            Some(h) if h > BACKUP_WARN_H => o.warnings.push(format!(
                "the last backup is {h:.1} h old (older than {BACKUP_WARN_H:.0} h)"
            )),
            _ => {}
        }
        if o.schema < o.program_schema {
            o.warnings.push(format!(
                "migrations pending: the network's schema is {}, this server's {} (a migration \
                 needs a backup younger than {:.0} h)",
                o.schema, o.program_schema, o.backup_max_age_h
            ));
        }
        if o.stale_routes > 0 {
            o.warnings.push(format!(
                "{} route(s) with results out of date (a shared tower moved, a library \
                 updated, neighbours changed)",
                o.stale_routes
            ));
        }
        o
    }

    fn sessions(&self) -> Vec<SessionRow> {
        let mut out: Vec<SessionRow> = self
            .clients
            .iter()
            .map(|(id, c)| {
                let mut follows = Vec::new();
                if let Some(m) = &c.model {
                    follows.push(format!("line {m}"));
                }
                if let Some(ws) = &c.ws {
                    follows.push(format!(
                        "a working set of line {} ({} route(s))",
                        ws.spec.model,
                        ws.routes.len()
                    ));
                }
                if !c.routes.is_empty() {
                    follows.push(format!("{} route(s)", c.routes.len()));
                }
                if !c.tiles.is_empty() {
                    follows.push(format!("{} tile(s)", c.tiles.len()));
                }
                SessionRow {
                    client: *id,
                    user: c.user.clone(),
                    since: c.since,
                    follows: if follows.is_empty() {
                        "nothing (an admin app, or joining)".into()
                    } else {
                        follows.join(", ")
                    },
                    see_as: c.see_as.clone(),
                }
            })
            .collect();
        out.sort_by_key(|r| (r.since, r.client));
        out
    }

    fn disconnect(
        &mut self,
        me: u64,
        user: &str,
        other: u64,
        reason: &str,
    ) -> Result<String, String> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err("say why (the user is shown it)".into());
        }
        if other == me {
            return Err("that is this connection: close the app instead".into());
        }
        let Some(who) = self.clients.get(&other).map(|c| c.user.clone()) else {
            return Err(format!("no connection {other} (gone already)"));
        };
        let reason: String = reason.chars().take(500).collect();
        self.let_go(
            other,
            &format!("a network admin let this connection go: {reason}"),
        );
        self.audit_admin(
            user,
            "admin.disconnect",
            &format!("user:{who}"),
            &format!("connection {other}: {reason}"),
        );
        log(&format!(
            "{user} let {who}'s connection {other} go: {reason}"
        ));
        Ok(format!("{who}'s connection let go"))
    }

    fn backups(&self) -> BackupList {
        let mut list = BackupList {
            folder: self.options.backup_dir.is_some(),
            drill_database: self.options.drill_database.is_some(),
            ..BackupList::default()
        };
        (list.last, list.age_h) = self.last_backup();
        if let Some(dir) = &self.options.backup_dir
            && let Ok(entries) = std::fs::read_dir(dir)
        {
            for e in entries.flatten() {
                let path = e.path();
                if !path.join(ohl_store_pg::backup::MANIFEST).is_file() {
                    continue;
                }
                if let Ok(m) = ohl_store_pg::backup::read_manifest(&path) {
                    list.rows.push(BackupRow {
                        name: e.file_name().to_string_lossy().into_owned(),
                        time: m.time,
                        schema: m.schema,
                        bytes: m.bytes,
                        user: m.user,
                        models: m.models.len(),
                    });
                }
            }
        }
        list.rows
            .sort_by(|a, b| b.time.cmp(&a.time).then(b.name.cmp(&a.name)));
        list
    }

    /// Takes the admins' one long work (refused while one runs).
    fn take_admin_work(&self) -> Result<(), String> {
        if self.admin_busy.swap(true, AtomicOrdering::SeqCst) {
            return Err("a backup or a restore drill runs already: wait for it".into());
        }
        Ok(())
    }

    fn backup_now(&mut self, client: u64, user: &str, id: u64) -> Result<(), String> {
        let Some(dir) = self.options.backup_dir.clone() else {
            return Err(
                "this server has no backup folder: start it with --backup-dir FOLDER, or back up \
                 with ohl network backup beside it"
                    .into(),
            );
        };
        self.take_admin_work()?;
        let name = format!("backup-{}", crate::admin::utc_stamp(unix_now()));
        let folder = dir.join(&name);
        self.audit_admin(user, "admin.backup", &name, "started from the admin app");
        log(&format!("{user} started backup {name}"));
        let (url, user, jobs, busy) = (
            self.options.database.clone(),
            user.to_owned(),
            self.admin_jobs.clone(),
            self.admin_busy.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("ohl-admin-work".into())
            .spawn(move || {
                let result = ohl_store_pg::backup::backup(&url, &user, &folder)
                    .map(|r| {
                        AdminReply::Done(format!(
                            "backup {name}: {} line(s), {:.1} MB, in {:.1} s",
                            r.manifest.models.len(),
                            r.manifest.bytes as f64 / 1e6,
                            r.seconds
                        ))
                    })
                    .map_err(|e| format!("the backup failed: {e}"));
                log(&match &result {
                    Ok(_) => format!("backup {name} done"),
                    Err(e) => e.clone(),
                });
                busy.store(false, AtomicOrdering::SeqCst);
                let _ = jobs.send(Job::AdminDone { client, id, result });
            });
        if let Err(e) = spawned {
            self.admin_busy.store(false, AtomicOrdering::SeqCst);
            return Err(e.to_string());
        }
        Ok(())
    }

    fn restore_drill(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        backup: &str,
    ) -> Result<(), String> {
        let Some(dir) = self.options.backup_dir.clone() else {
            return Err("this server has no backup folder (--backup-dir)".into());
        };
        // A folder of the backup folder, by name only.
        if backup.is_empty()
            || backup.contains(['/', '\\'])
            || backup.starts_with('.')
            || backup.chars().any(char::is_control)
        {
            return Err(format!("\"{backup}\": a backup's folder name"));
        }
        let folder = dir.join(backup);
        if !folder.join(ohl_store_pg::backup::MANIFEST).is_file() {
            return Err(format!("no backup {backup} in the backup folder"));
        }
        let drill = self.options.drill_database.clone();
        if drill.as_deref() == Some(self.options.database.as_str()) {
            return Err("the drill database is the network's own: refused".into());
        }
        self.take_admin_work()?;
        self.audit_admin(
            user,
            "admin.restore_drill",
            backup,
            if drill.is_some() {
                "started: restored into the drill database and compared"
            } else {
                "started: the dump checked against its manifest"
            },
        );
        log(&format!("{user} started the restore drill of {backup}"));
        let (user, jobs, busy, backup) = (
            user.to_owned(),
            self.admin_jobs.clone(),
            self.admin_busy.clone(),
            backup.to_owned(),
        );
        let spawned = std::thread::Builder::new()
            .name("ohl-admin-work".into())
            .spawn(move || {
                let result = drill_work(&folder, drill.as_deref(), &user, &backup);
                log(&match &result {
                    Ok(_) => format!("restore drill of {backup} passed"),
                    Err(e) => format!("restore drill of {backup}: {e}"),
                });
                busy.store(false, AtomicOrdering::SeqCst);
                let _ = jobs.send(Job::AdminDone {
                    client,
                    id,
                    result: result.map(AdminReply::Done),
                });
            });
        if let Err(e) = spawned {
            self.admin_busy.store(false, AtomicOrdering::SeqCst);
            return Err(e.to_string());
        }
        Ok(())
    }

    fn settings(&self) -> Vec<SettingRow> {
        let o = &self.options;
        let start = |key: &str, value: String, note: &str| SettingRow {
            key: key.into(),
            value,
            source: "start".into(),
            note: note.into(),
        };
        let database = match ohl_store_pg::connect::Target::parse(&o.database) {
            Ok(t) => format!(
                "{} / {} (TLS {})",
                t.hosts().join(", "),
                t.config.get_dbname().unwrap_or("?"),
                t.mode.as_str()
            ),
            Err(_) => "(not shown)".into(),
        };
        let mut rows = vec![
            start(
                "database",
                database,
                "the network's database: host(s) / name (the password never shown)",
            ),
            start(
                "readers",
                o.readers.to_string(),
                "reader connections at most",
            ),
            start(
                "packagers",
                o.packagers.to_string(),
                "threads making tile packages",
            ),
            start(
                "solve_threads",
                o.solve_threads.to_string(),
                "threads solving one package's sections side by side",
            ),
            start(
                "packages_mb",
                o.packages_mb.to_string(),
                "tile packages kept, MB",
            ),
            start(
                "shapes_mb",
                o.shapes_mb.to_string(),
                "solved section shapes kept, MB",
            ),
            start(
                "heartbeat_ms",
                o.heartbeat.as_millis().to_string(),
                "a heartbeat this often, ms",
            ),
            start(
                "max_tiles",
                o.max_tiles.to_string(),
                "tiles one client views at once",
            ),
            start(
                "idle_solve",
                o.idle_solve.to_string(),
                "the context solved ahead while the server is idle",
            ),
            start(
                "tiles_per_second",
                format!("{}", o.tiles_per_second),
                "new tiles made for one client a second",
            ),
            start(
                "max_heavy_queue",
                o.max_heavy_queue.to_string(),
                "checks, exports and bundles waiting at most",
            ),
            start(
                "invite_days",
                o.invite_days
                    .map_or("until reissued".into(), |d| d.to_string()),
                "the days a personal invitation is valid",
            ),
            start(
                "oidc",
                o.oidc
                    .as_ref()
                    .map_or("off".into(), |c| c.issuer().to_owned()),
                "OIDC sign-in: the identity provider's issuer",
            ),
            start(
                "check_worker",
                o.check_worker.to_string(),
                "checks on a worker thread of their own",
            ),
            start(
                "export_worker",
                o.export_worker.to_string(),
                "offline working sets written on a thread of their own",
            ),
            start(
                "backup_dir",
                o.backup_dir
                    .as_ref()
                    .map_or("none".into(), |d| d.display().to_string()),
                "the folder of the backups made from the admin app",
            ),
            start(
                "drill_database",
                if o.drill_database.is_some() {
                    "set".into()
                } else {
                    "none".into()
                },
                "an empty database the restore drill restores into (never shown)",
            ),
        ];
        for (key, note) in ohl_store_pg::backup::SETTINGS {
            rows.push(SettingRow {
                key: (*key).into(),
                value: self
                    .net
                    .setting(key)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "(default)".into()),
                source: "network".into(),
                note: (*note).into(),
            });
        }
        rows
    }

    fn library_kind_names(&self, me: &UserRights) -> Vec<String> {
        self.net
            .library_kinds(me)
            .map(|k| k.into_iter().map(|k| k.kind).collect())
            .unwrap_or_default()
    }

    fn rights_view(&mut self, me: &UserRights, network_admin: bool) -> RightsView {
        let d = &self.rights;
        let admins: BTreeSet<String> = d.network_admins().into_iter().collect();
        let users = d
            .users
            .iter()
            .map(|(name, active)| UserRow {
                name: name.clone(),
                active: *active,
                groups: d.groups_of(name).into_iter().map(str::to_owned).collect(),
                network_admin: admins.contains(name),
                connected: self.common.peers.contains_key(name),
            })
            .collect();
        let groups = d
            .groups
            .iter()
            .map(|(g, m)| (g.clone(), m.iter().cloned().collect()))
            .collect();
        let roles = d
            .roles
            .iter()
            .map(|(name, (p, shipped))| RoleRow {
                name: name.clone(),
                permissions: p.list(),
                shipped: *shipped,
            })
            .collect();
        let assignments = d
            .assignments
            .iter()
            .filter(|a| network_admin || me.manages(&a.scope))
            .cloned()
            .collect();
        let manages_lines = self
            .models
            .keys()
            .filter(|c| network_admin || me.manages(&Scope::line(c)))
            .cloned()
            .collect();
        let rev = d.rev;
        let maintained = maintained_kinds(&self.rights, me.user());
        let manages_libraries = self
            .library_kind_names(me)
            .into_iter()
            .filter(|k| {
                network_admin
                    || maintained.contains(k)
                    || maintained.contains("*")
                    || me.manages(&Scope {
                        library_kind: Some(k.clone()),
                        ..Scope::default()
                    })
            })
            .collect();
        RightsView {
            me: me.user().to_owned(),
            network_admin,
            manages_lines,
            manages_libraries,
            users,
            groups,
            roles,
            assignments,
            rev,
        }
    }

    fn preview(
        &self,
        me: &UserRights,
        network_admin: bool,
        command: &RightsCommand,
    ) -> crate::admin::WhatChanges {
        let mut w = crate::admin::what_changes(&self.rights, command);
        let admins_only = !matches!(
            command,
            RightsCommand::Assign { .. } | RightsCommand::Unassign { .. }
        );
        if !network_admin {
            if admins_only {
                w.notes
                    .push("network admins only: saving it will be refused".into());
            }
            match command {
                RightsCommand::Assign { deny: true, .. } => w
                    .notes
                    .push("a deny is set by network admins only: saving it will be refused".into()),
                RightsCommand::Assign { scope, .. } if !me.manages(scope) => w.notes.push(
                    "you do not manage rights on that scope: saving it will be refused".into(),
                ),
                RightsCommand::Unassign { id } => {
                    if let Some(a) = self.rights.assignments.iter().find(|a| a.id == *id)
                        && !me.manages(&a.scope)
                    {
                        w.notes.push(
                            "you do not manage rights on that assignment's scope: saving it \
                             will be refused"
                                .into(),
                        );
                    }
                }
                _ => {}
            }
        }
        w
    }

    fn invite(
        &mut self,
        user: &str,
        name: &str,
        display: Option<String>,
        days: Option<u32>,
        reissue: bool,
    ) -> Result<InvitationText, String> {
        if let Some(d) = days
            && !(1..=3650).contains(&d)
        {
            return Err("an invitation is valid 1 to 3650 days".into());
        }
        let done = if reissue {
            self.net
                .rights_command(
                    user,
                    &RightsCommand::ReissueInvitation {
                        name: name.to_owned(),
                    },
                )
                .map_err(|e| e.to_string())?
        } else if !self.rights.users.contains_key(name) {
            self.net
                .rights_command(
                    user,
                    &RightsCommand::AddUser {
                        name: name.to_owned(),
                        display,
                    },
                )
                .map_err(|e| e.to_string())?
        } else {
            format!("{name} is a user already")
        };
        self.refresh_rights();
        let code = self.personal_code(name, days)?;
        let until = days
            .or(self.options.invite_days)
            .map(|d| crate::sign_in::day_text(crate::utc_day(i64::from(d))));
        self.audit_admin(
            user,
            if reissue {
                "admin.reissue_invitation"
            } else {
                "admin.invite"
            },
            &format!("user:{name}"),
            &format!(
                "{done}; valid until {}",
                until.as_deref().unwrap_or("reissued")
            ),
        );
        log(&format!(
            "{user} {} the personal invitation of {name}",
            if reissue { "reissued" } else { "made" }
        ));
        Ok(InvitationText {
            user: name.to_owned(),
            code,
            until,
        })
    }

    fn see_as_view(&mut self, user: &str, me: &UserRights, who: &str) -> Result<SeeAsView, String> {
        if !self.rights.users.contains_key(who) {
            return Err(format!("no user {who} on this network"));
        }
        let them = self.rights_of(who);
        let both = me.clone().and((*them).clone());
        self.audit_admin(user, "see_as", &format!("user:{who}"), "the admin app");
        let mut out = SeeAsView {
            user: who.to_owned(),
            ..SeeAsView::default()
        };
        let all = both.sees_everything();
        for (code, m) in &self.models {
            if !both.sees_some_of_line(code) {
                continue;
            }
            let whole = both.sees_whole_line(code);
            let mut counts: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
            for ((kind, id), place) in m.places.iter() {
                let c = counts.entry(kind.as_str()).or_default();
                c.1 += 1;
                if all || whole || nr::sees(&both, code, *kind, id, Some(place)) {
                    c.0 += 1;
                }
            }
            for r in &m.session.line().routes {
                if all
                    || whole
                    || nr::sees(
                        &both,
                        code,
                        Kind::Route,
                        &r.id,
                        m.places.get(Kind::Route, &r.id),
                    )
                {
                    out.routes.push(r.id.clone());
                }
            }
            out.lines.push(SeeLine {
                line: code.clone(),
                whole,
                counts: counts
                    .into_iter()
                    .map(|(k, (s, a))| (k.to_owned(), s, a))
                    .collect(),
            });
        }
        Ok(out)
    }

    fn admin_map(&mut self, me: &UserRights) -> AdminMap {
        let all = me.sees_everything();
        let mut map = AdminMap {
            crs: self.crs.clone(),
            ..AdminMap::default()
        };
        // Users with a working set, by route.
        let mut in_use: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for c in self.clients.values() {
            if let Some(ws) = &c.ws {
                for r in &ws.routes {
                    in_use.entry(r.clone()).or_default().insert(c.user.clone());
                }
            }
        }
        // Soft locks by line (a lock's target starts with its project
        // line; the models' own main line named by the model's code is
        // not in it — counted on the line whose objects it names).
        let mut bounds: Option<Bounds> = None;
        let mut grow = |p: [f64; 2]| {
            let b = Bounds::new(p, p);
            bounds = Some(bounds.map_or(b, |x| x.union(&b)));
        };
        for (code, m) in &self.models {
            if !all && !me.sees_some_of_line(code) {
                continue;
            }
            map.lines.push((code.to_string(), m.name.clone()));
            let line = m.session.line();
            let voltage = line
                .systems
                .iter()
                .map(|s| s.voltage_kv)
                .filter(|v| v.is_finite())
                .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
            let owner = line
                .systems
                .iter()
                .map(|s| s.owner.trim())
                .find(|o| !o.is_empty())
                .unwrap_or_default()
                .to_owned();
            let stale = m.session.stale_routes().unwrap_or_default();
            let points: HashMap<&str, [f64; 2]> = line
                .alignment_points
                .iter()
                .map(|p| (p.id.as_str(), [p.position[0], p.position[1]]))
                .collect();
            let mut of_route: BTreeMap<&str, Vec<Vec<[f64; 2]>>> = BTreeMap::new();
            for a in &line.alignments {
                let pts: Vec<[f64; 2]> = a
                    .points
                    .iter()
                    .filter_map(|p| points.get(p.as_str()).copied())
                    .collect();
                of_route
                    .entry(a.route.as_str())
                    .or_default()
                    .push(simplify(&pts, MAP_POINTS));
            }
            let mut structures_of: BTreeMap<String, usize> = BTreeMap::new();
            for s in &line.structures {
                let routes = m
                    .places
                    .get(Kind::Structure, &s.id)
                    .map(|p| p.routes.clone());
                if let Some(rs) = &routes {
                    for r in rs.iter() {
                        *structures_of.entry(r.clone()).or_default() += 1;
                    }
                    if rs.len() > 1
                        && (all
                            || nr::sees(
                                me,
                                code,
                                Kind::Structure,
                                &s.id,
                                m.places.get(Kind::Structure, &s.id),
                            ))
                    {
                        map.shared.push([s.position[0], s.position[1]]);
                    }
                }
            }
            let locks = self
                .common
                .locks
                .keys()
                .filter(|t| {
                    t.split(':')
                        .nth(2)
                        .is_some_and(|id| m.places.get(Kind::Structure, id).is_some())
                })
                .count();
            for r in &line.routes {
                if !all
                    && !nr::sees(
                        me,
                        code,
                        Kind::Route,
                        &r.id,
                        m.places.get(Kind::Route, &r.id),
                    )
                {
                    continue;
                }
                let lines = of_route.remove(r.id.as_str()).unwrap_or_default();
                for l in &lines {
                    for p in l {
                        grow(*p);
                    }
                }
                map.routes.push(MapRoute {
                    id: r.id.clone(),
                    model: code.to_string(),
                    name: r.name.clone(),
                    voltage_kv: voltage,
                    owner: owner.clone(),
                    lines,
                    structures: structures_of.get(&r.id).copied().unwrap_or(0),
                    stale: stale.get(&r.id).map(|s| {
                        s.notices
                            .last()
                            .map_or_else(|| format!("{} change(s)", s.count), |n| n.what.clone())
                    }),
                    in_use: in_use
                        .get(&r.id)
                        .map(|u| u.iter().cloned().collect())
                        .unwrap_or_default(),
                    locks,
                });
            }
        }
        map.bounds = bounds.map(|b| [b.min[0], b.min[1], b.max[0], b.max[1]]);
        map
    }

    fn dots(&mut self, me: &UserRights, b: [f64; 4]) -> Result<Vec<MapDot>, String> {
        if !b.iter().all(|v| v.is_finite()) || b[2] < b[0] || b[3] < b[1] {
            return Err("an area x0, y0, x1, y1 with x0 ≤ x1 and y0 ≤ y1".into());
        }
        let all = me.sees_everything();
        let mut out = Vec::new();
        for (code, m) in &self.models {
            if !all && !me.sees_some_of_line(code) {
                continue;
            }
            for s in &m.session.line().structures {
                let [x, y] = [s.position[0], s.position[1]];
                if x < b[0] || x > b[2] || y < b[1] || y > b[3] {
                    continue;
                }
                let place = m.places.get(Kind::Structure, &s.id);
                if !all && !nr::sees(me, code, Kind::Structure, &s.id, place) {
                    continue;
                }
                if out.len() >= crate::admin::MAX_DOTS {
                    return Ok(out);
                }
                out.push(MapDot {
                    model: code.to_string(),
                    id: s.id.clone(),
                    at: [x, y],
                    shared: place.is_some_and(|p| p.routes.len() > 1),
                });
            }
        }
        Ok(out)
    }

    fn route_dots(
        &mut self,
        me: &UserRights,
        model: &str,
        route: &str,
    ) -> Result<Vec<MapDot>, String> {
        let all = me.sees_everything();
        let Some(m) = self.models.get(model) else {
            return Err(format!("no line {model}"));
        };
        if !all
            && !nr::sees(
                me,
                model,
                Kind::Route,
                route,
                m.places.get(Kind::Route, route),
            )
        {
            return Err(HIDDEN.into());
        }
        let line = m.session.line();
        let at: HashMap<&str, [f64; 2]> = line
            .structures
            .iter()
            .map(|s| (s.id.as_str(), [s.position[0], s.position[1]]))
            .collect();
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        // In order along the route's alignments (their points tied to
        // structures), then the route's other structures.
        let tied = line
            .alignments
            .iter()
            .filter(|a| a.route == route)
            .flat_map(|a| a.points.iter())
            .filter_map(|p| line.alignment_points.iter().find(|q| &q.id == p))
            .filter_map(|p| p.structure.clone());
        let others = m
            .places
            .iter()
            .filter(|((k, _), p)| *k == Kind::Structure && p.routes.iter().any(|r| r == route))
            .map(|((_, id), _)| id.clone());
        for id in tied.chain(others.collect::<Vec<_>>()) {
            if !seen.insert(id.clone()) {
                continue;
            }
            let place = m.places.get(Kind::Structure, &id);
            if !all && !nr::sees(me, model, Kind::Structure, &id, place) {
                continue;
            }
            if let Some(p) = at.get(id.as_str()) {
                out.push(MapDot {
                    model: model.to_owned(),
                    shared: place.is_some_and(|p| p.routes.len() > 1),
                    id,
                    at: *p,
                });
            }
        }
        Ok(out)
    }

    fn libraries(&mut self, me: &UserRights) -> Result<Vec<LibraryKindRow>, String> {
        let kinds = self.net.library_kinds(me).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for k in kinds {
            let files = self
                .net
                .library_files(me, &k.kind)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|f| LibraryFileRow {
                    file: f.file,
                    locked: f.locked,
                    entries: f.entries.into_iter().map(|e| e.0).collect(),
                })
                .collect();
            out.push(LibraryKindRow {
                kind: k.kind,
                locked: k.locked,
                maintainers: k.maintainers,
                files,
            });
        }
        Ok(out)
    }
}

/// The library kinds `user` maintains (an assignment of theirs, not a
/// deny, whose role may review and lock, on a library scope: as the
/// store names maintainers; `*`: every kind) — answer 3 of 2026-11-28: a
/// library maintainer opens the admin app for their libraries.
fn maintained_kinds(d: &RightsData, user: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if !d.active(user) {
        return out;
    }
    for a in d.assignments_of(user) {
        let Some((p, _)) = a.role.as_ref().and_then(|r| d.roles.get(r)) else {
            continue;
        };
        let s = &a.scope;
        let library = s.library_kind.is_some() || s.kind.as_deref() == Some("library");
        if !a.deny && library && p.contains(Permission::Review) && p.contains(Permission::Lock) {
            out.insert(s.library_kind.clone().unwrap_or_else(|| "*".into()));
        }
    }
    out
}

/// The refusal of something the asker may not see (no name given back).
const HIDDEN: &str = ohl_session::rights::HIDDEN_REFUSAL;

/// A polyline simplified to at most `max` points (every n-th, the ends
/// kept): the map is a sketch, never measured on.
fn simplify(pts: &[[f64; 2]], max: usize) -> Vec<[f64; 2]> {
    if pts.len() <= max || max < 2 {
        return pts.to_vec();
    }
    let step = (pts.len() - 1) as f64 / (max - 1) as f64;
    (0..max)
        .map(|i| pts[((i as f64 * step).round() as usize).min(pts.len() - 1)])
        .collect()
}

/// The restore drill's work (off the authority): the dump checked
/// against its manifest; with a drill database, restored there and every
/// line and the network's tables compared ([`ohl_store_pg::backup::restore`]).
fn drill_work(
    folder: &std::path::Path,
    drill: Option<&str>,
    user: &str,
    backup: &str,
) -> Result<String, String> {
    use sha2::Digest;
    use std::io::Read;
    let manifest = ohl_store_pg::backup::read_manifest(folder).map_err(|e| e.to_string())?;
    let dump = folder.join(ohl_store_pg::backup::DUMP);
    let mut f = std::fs::File::open(&dump).map_err(|e| format!("the dump: {e}"))?;
    let mut hash = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut bytes = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("the dump: {e}"))?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        hash.update(&buf[..n]);
    }
    let sha: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if sha != manifest.sha256 || bytes != manifest.bytes {
        return Err(format!(
            "the restore drill failed: the dump of {backup} is not the one its manifest names \
             (changed or damaged)"
        ));
    }
    let Some(url) = drill else {
        return Ok(format!(
            "the dump of {backup} matches its manifest ({:.1} MB, SHA-256 checked). The full \
             drill (restored and compared line by line) needs a drill database: start the server \
             with --drill-database",
            bytes as f64 / 1e6
        ));
    };
    let r = ohl_store_pg::backup::restore(url, user, folder)
        .map_err(|e| format!("the restore drill failed: {e}"))?;
    Ok(format!(
        "restore drill of {backup} passed: {} line(s) and the network's tables restored and \
         compared with the manifest in {:.1} s (the drill database now holds it: empty it before \
         the next drill)",
        r.models, r.seconds
    ))
}
