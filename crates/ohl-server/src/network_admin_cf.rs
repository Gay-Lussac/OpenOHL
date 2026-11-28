// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The admin messages of OHL-Server's stages C to F on a network's server
//! (2026-11-28; [`crate::admin`] for the messages and their rules):
//!
//! - **C** (with phase 8): the shared towers (their routes, stale marks,
//!   who may edit them) and a line's twin towers (shown, never merged);
//! - **D** (with phase 9): the Libraries page — each kind's lock,
//!   maintainers, files, entries and releases, the requests, the lines'
//!   pins, the use index; a maintainer's changes through the store's one
//!   function ([`ohl_store_pg::Network::library_command`], audited there);
//!   Update library pins planned as a proposal, then carried out as the
//!   server's own command;
//! - **E**: the Lines page — one line exported as a standalone project
//!   file on the server's exports' thread (a reader connection of its own,
//!   one snapshot), fetched in pieces of [`crate::admin::CHUNK`]; a
//!   project file uploaded in pieces into a folder of the server's own and
//!   imported as a new line (the network import, on the authority: it
//!   needs the writer), then served as every other line;
//! - **F** (with phase 13): a page of the audit, the OIDC identities,
//!   "explain", a network setting changed.
//!
//! Every message is checked by the rights' one function before anything
//! is read or done; every change and refusal is in the network's audit.
//! A file held for a connection lives in a folder of its own (a random
//! name, never a name the app gave as a path) and goes with the
//! connection.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use ohl_session::rights::{Permission, Scope, Target};

use super::exports::ExportTo;
use super::*;
use crate::admin::{
    AUDIT_PAGE_MAX, AdminAsk, AdminReply, AuditPage, AuditRow, CHUNK, ChunkData, EntryRow,
    ExplainView, IdentityRow, ImportView, LibraryAction, LibraryFileView, LibraryKindView,
    LibraryPage, LinePins, LineRow, MAX_TRANSFERS, MAX_UPLOAD, PinKindPlan, PinRow, PinsView,
    RequestRow, SHARED_MAX, SharedDetail, SharedList, SharedRoute, SharedRow, TransferInfo,
    TwinRow, UseRow,
};

/// A file held for one admin connection: a line exported, to fetch; or a
/// project file being uploaded, to import.
pub(crate) struct Transfer {
    client: u64,
    /// Its own folder (in the server's copies folder; removed with it).
    dir: PathBuf,
    path: PathBuf,
    name: String,
    /// Its size (an upload: as announced).
    bytes: u64,
    /// An upload's bytes written so far.
    received: u64,
    upload: bool,
    /// Written whole (an export), or received whole (an upload).
    ready: bool,
    file: Option<std::fs::File>,
}

impl Drop for Transfer {
    fn drop(&mut self) {
        self.file = None;
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A line's export as the exports' thread writes it.
#[derive(Debug, Clone)]
pub(crate) struct ModelExport {
    pub(super) url: String,
    pub(super) code: String,
    pub(super) path: PathBuf,
    pub(super) user: String,
    pub(super) token: String,
}

/// Writes export `m` (on the exports' thread): a reader connection of its
/// own, the store's export (one snapshot, refused when the file exists).
pub(super) fn write_model(m: &ModelExport) -> Result<ohl_store_pg::export::ExportReport, String> {
    let net = Network::connect(&m.url).map_err(|e| e.to_string())?;
    ohl_store_pg::export::export(&net, &m.code, &m.path, &m.user).map_err(|e| e.to_string())
}

/// A file name an admin gave, kept to safe characters (a name, never a
/// path); a project file's extension required.
fn safe_name(name: &str) -> Result<String, String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
        .take(120)
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_owned();
    if clean.is_empty() || !clean.to_ascii_lowercase().ends_with(".ohl") {
        return Err(format!("\"{name}\": a project file (.ohl)"));
    }
    Ok(clean)
}

/// A route's stale mark in words.
fn stale_words(s: &ohl_session::shared::Stale) -> String {
    s.notices
        .last()
        .map_or_else(|| format!("{} change(s)", s.count), |n| n.what.clone())
}

impl NetHost {
    /// Answers the admin messages of stages C to F (`me`: the asker's
    /// rights; the gate of [`NetHost::admin`] passed): the answer to push
    /// now, or none when it comes later (an export) — a refusal returned.
    pub(super) fn admin_cf(
        &mut self,
        client: u64,
        user: &str,
        id: u64,
        me: &Arc<UserRights>,
        network_admin: bool,
        ask: AdminAsk,
    ) -> Result<Option<AdminReply>, String> {
        Ok(Some(match ask {
            AdminAsk::SharedTowers => AdminReply::SharedTowers(self.shared_towers(me)),
            AdminAsk::SharedTower { model, id: tower } => AdminReply::SharedTower(Box::new(
                self.shared_tower(me, network_admin, &model, &tower)?,
            )),
            AdminAsk::Twins { model, tolerance } => {
                AdminReply::Twins(self.twins(me, &model, tolerance)?)
            }
            AdminAsk::LibraryPage => {
                AdminReply::LibraryPage(Box::new(self.library_page(me, network_admin)?))
            }
            AdminAsk::Uses { kind, entry } => AdminReply::Uses(
                self.net
                    .uses(me, &kind, &entry)
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|u| UseRow {
                        line: u.line,
                        branch: u.branch,
                        object_kind: u.object_kind,
                        object: u.object,
                        routes: u.routes,
                    })
                    .collect(),
            ),
            AdminAsk::Library { action } => AdminReply::Done(self.library_action(user, action)?),
            AdminAsk::PinsPlan { line, kinds } => {
                AdminReply::Pins(Box::new(self.pins_plan(me, &line, &kinds)?))
            }
            AdminAsk::UpdatePins { line, kinds } => {
                AdminReply::Done(self.update_pins(user, me, &line, &kinds)?)
            }
            AdminAsk::Lines => AdminReply::Lines(self.lines_rows(me)),
            AdminAsk::ExportLine { model } => {
                self.export_line(client, id, user, me, &model)?;
                return Ok(None);
            }
            AdminAsk::Download { token, offset } => {
                AdminReply::Chunk(self.download(client, &token, offset)?)
            }
            AdminAsk::UploadStart { name, bytes } => {
                AdminReply::Transfer(self.upload_start(client, user, &name, bytes)?)
            }
            AdminAsk::Upload {
                token,
                offset,
                data,
            } => AdminReply::Received(self.upload(client, &token, offset, &data)?),
            AdminAsk::ImportLine {
                token,
                code,
                name,
                tolerance,
            } => {
                let view = self.import_line(client, user, &token, &code, name, tolerance)?;
                // Answered first, then the rights read again (the file's users
                // joined the line: a client whose rights changed is let go —
                // perhaps this one).
                self.push(
                    client,
                    None,
                    None,
                    Some(ServerMsg::Admin {
                        id,
                        result: Ok(AdminReply::Imported(Box::new(view))),
                    }),
                );
                self.refresh_rights();
                return Ok(None);
            }
            AdminAsk::TransferDone { token } => {
                let mine = self
                    .transfers
                    .get(&token)
                    .is_some_and(|t| t.client == client);
                if mine {
                    self.transfers.remove(&token);
                }
                AdminReply::Done("the file is gone from the server".into())
            }
            AdminAsk::Audit {
                filter,
                before,
                limit,
            } => AdminReply::Audit(self.audit_read(&filter, before, limit)?),
            AdminAsk::Identities => AdminReply::Identities(
                self.net
                    .identities()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|[issuer, subject, user]| IdentityRow {
                        issuer,
                        subject,
                        user,
                    })
                    .collect(),
            ),
            AdminAsk::Explain {
                user: who,
                permission,
                line,
                kind,
                id: object,
            } => AdminReply::Explain(self.explain(user, me, &who, permission, line, kind, object)?),
            AdminAsk::SetSetting { key, value } => {
                self.net
                    .set_setting(user, &key, &value)
                    .map_err(|e| e.to_string())?;
                log(&format!("{user} set the network's {key} to {value}"));
                AdminReply::Done(format!("{key} = {value}"))
            }
            _ => return Err("not an admin message of stages C to F (a bug)".into()),
        }))
    }

    /// Lets the files of client `client` go (it left).
    pub(super) fn drop_transfers(&mut self, client: u64) {
        self.transfers.retain(|_, t| t.client != client);
    }

    /// The shared towers `me` may see (at most [`SHARED_MAX`] listed).
    fn shared_towers(&mut self, me: &UserRights) -> SharedList {
        let all = me.sees_everything();
        let mut out = SharedList::default();
        for (code, m) in &self.models {
            if !all && !me.sees_some_of_line(code) {
                continue;
            }
            let line = m.session.line();
            let stale = m.session.stale_routes().unwrap_or_default();
            let names: HashMap<&str, &str> = line
                .routes
                .iter()
                .map(|r| (r.id.as_str(), r.name.as_str()))
                .collect();
            for s in &line.structures {
                let place = m.places.get(Kind::Structure, &s.id);
                let Some(p) = place else { continue };
                if p.routes.len() < 2 {
                    continue;
                }
                if !all && !nr::sees(me, code, Kind::Structure, &s.id, place) {
                    continue;
                }
                let routes: Vec<SharedRoute> = p
                    .routes
                    .iter()
                    .filter(|r| {
                        all || nr::sees(me, code, Kind::Route, r, m.places.get(Kind::Route, r))
                    })
                    .map(|r| SharedRoute {
                        id: r.clone(),
                        name: names
                            .get(r.as_str())
                            .copied()
                            .unwrap_or_default()
                            .to_owned(),
                        stale: stale.get(r).map(stale_words),
                    })
                    .collect();
                out.total += 1;
                out.stale += usize::from(routes.iter().any(|r| r.stale.is_some()));
                if out.rows.len() < SHARED_MAX {
                    out.rows.push(SharedRow {
                        model: code.to_string(),
                        id: s.id.clone(),
                        at: [s.position[0], s.position[1]],
                        routes,
                    });
                }
            }
        }
        out
    }

    /// One shared tower, with who may edit it (to who manages rights on
    /// its line).
    fn shared_tower(
        &mut self,
        me: &UserRights,
        network_admin: bool,
        model: &str,
        tower: &str,
    ) -> Result<SharedDetail, String> {
        let all = me.sees_everything();
        let Some(m) = self.models.get(model) else {
            return Err(super::admin::HIDDEN.into());
        };
        let place = m.places.get(Kind::Structure, tower);
        let line = m.session.line();
        let Some(s) = line.structures.iter().find(|s| s.id == tower) else {
            return Err(super::admin::HIDDEN.into());
        };
        if !all && !nr::sees(me, model, Kind::Structure, tower, place) {
            return Err(super::admin::HIDDEN.into());
        }
        let stale = m.session.stale_routes().unwrap_or_default();
        let routes = place
            .map(|p| p.routes.to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|r| all || nr::sees(me, model, Kind::Route, r, m.places.get(Kind::Route, r)))
            .map(|r| SharedRoute {
                name: line
                    .routes
                    .iter()
                    .find(|x| x.id == r)
                    .map(|x| x.name.clone())
                    .unwrap_or_default(),
                stale: stale.get(&r).map(stale_words),
                id: r,
            })
            .collect();
        let row = SharedRow {
            model: model.to_owned(),
            id: tower.to_owned(),
            at: [s.position[0], s.position[1]],
            routes,
        };
        let target_place = place.map(|p| nr::Place {
            bounds: p.bounds,
            routes: p.routes.clone(),
        });
        let editors = (network_admin || me.manages(&Scope::line(model))).then(|| {
            let users: Vec<String> = self
                .rights
                .users
                .iter()
                .filter(|(_, active)| **active)
                .map(|(u, _)| u.clone())
                .collect();
            let target = nr::target(model, Kind::Structure, tower, target_place.as_ref());
            users
                .into_iter()
                .filter(|u| self.rights_of(u).allows(Permission::Edit, &target))
                .collect()
        });
        Ok(SharedDetail { row, editors })
    }

    /// The twin towers of line `model` the asker may see (theirs and the
    /// other line's structure).
    fn twins(
        &mut self,
        me: &UserRights,
        model: &str,
        tolerance: Option<f64>,
    ) -> Result<Vec<TwinRow>, String> {
        let all = me.sees_everything();
        if !self.models.contains_key(model) || (!all && !me.sees_some_of_line(model)) {
            return Err(super::admin::HIDDEN.into());
        }
        let tol = tolerance.unwrap_or(ohl_store_pg::import::TWIN_TOLERANCE);
        if !(tol.is_finite() && (0.0..=50.0).contains(&tol)) {
            return Err("a twin distance of 0 to 50 m".into());
        }
        let twins =
            ohl_store_pg::import::twins(&self.net, model, tol).map_err(|e| e.to_string())?;
        let seen = |code: &str, id: &str| {
            all || self.models.get(code).is_some_and(|m| {
                nr::sees(
                    me,
                    code,
                    Kind::Structure,
                    id,
                    m.places.get(Kind::Structure, id),
                )
            })
        };
        Ok(twins
            .into_iter()
            .filter(|t| seen(model, &t.structure) && seen(&t.other_line, &t.other))
            .map(|t| TwinRow {
                line: model.to_owned(),
                structure: t.structure,
                other_line: t.other_line,
                other: t.other,
                distance_m: t.distance_m,
                structure_type: t.structure_type,
            })
            .collect())
    }

    /// The Libraries page as `me` may see it.
    fn library_page(
        &mut self,
        me: &UserRights,
        network_admin: bool,
    ) -> Result<LibraryPage, String> {
        let e = |e: ohl_store_pg::NetError| e.to_string();
        let maintained = super::admin::maintained_kinds(&self.rights, me.user());
        let mut page = LibraryPage::default();
        for k in self.net.library_kinds(me).map_err(e)? {
            let files = self
                .net
                .library_files(me, &k.kind)
                .map_err(e)?
                .into_iter()
                .map(|f| LibraryFileView {
                    file: f.file,
                    locked: f.locked,
                    entries: f
                        .entries
                        .into_iter()
                        .map(|(id, version, release, withdrawn)| EntryRow {
                            id,
                            version,
                            release,
                            withdrawn,
                        })
                        .collect(),
                })
                .collect();
            page.kinds.push(LibraryKindView {
                maintain: network_admin || maintained.contains(&k.kind) || maintained.contains("*"),
                kind: k.kind,
                locked: k.locked,
                maintainers: k.maintainers,
                releases: k.releases,
                files,
            });
        }
        page.requests = self
            .net
            .library_requests(me)
            .map_err(e)?
            .into_iter()
            .map(|r| RequestRow {
                id: r.id,
                kind: r.kind,
                file: r.file,
                entry: r.entry,
                state: r.state,
                user: r.user,
                line: r.line,
                reason: r.reason,
                maintainer: r.maintainer,
                note: r.note,
                created: r.created,
                decided: r.decided,
                released: r.released,
            })
            .collect();
        page.lines = self
            .net
            .library_status(me, None)
            .map_err(e)?
            .into_iter()
            .map(|l| LinePins {
                line: l.line,
                built: l.built,
                kinds: l
                    .kinds
                    .into_iter()
                    .map(|k| PinRow {
                        kind: k.kind,
                        pinned: k.pinned,
                        latest: k.latest,
                        behind: k.behind,
                        withdrawn_used: k.withdrawn_used,
                    })
                    .collect(),
            })
            .collect();
        Ok(page)
    }

    /// A maintainer's change: the store's one function decides and
    /// audits it; the server reads the libraries again.
    fn library_action(&mut self, user: &str, action: LibraryAction) -> Result<String, String> {
        use ohl_store_pg::library::LibraryCommand as C;
        let words = action.describe();
        let command = match action {
            LibraryAction::Lock {
                kind,
                file,
                locked,
                note,
            } => C::Lock {
                kind,
                file,
                locked,
                note,
            },
            LibraryAction::Grant { id, note } => C::Grant { id, note },
            LibraryAction::Refuse { id, note } => {
                if note.trim().is_empty() {
                    return Err("say why the request is refused (its requester is shown it)".into());
                }
                C::Refuse { id, note }
            }
            LibraryAction::Release { kind, tag, notes } => C::Release { kind, tag, notes },
            LibraryAction::Withdraw {
                kind,
                file,
                entry,
                note,
            } => C::WithdrawEntry {
                kind,
                file,
                entry,
                note,
            },
            LibraryAction::Restore { kind, file, entry } => C::RestoreEntry { kind, file, entry },
        };
        let done = self
            .net
            .library_command(user, &command)
            .map_err(|e| e.to_string())?;
        log(&format!("{user}: library, {words}: {}", done.text));
        self.refresh_rights();
        Ok(done.text)
    }

    /// What Update library pins would do to line `line` (nothing changed),
    /// and whether the asker may carry it out.
    fn pins_plan(
        &mut self,
        me: &UserRights,
        line: &str,
        kinds: &[String],
    ) -> Result<PinsView, String> {
        if !me.sees_some_of_line(line) {
            return Err(super::admin::HIDDEN.into());
        }
        let Some(m) = self.models.get_mut(line) else {
            return Err(super::admin::HIDDEN.into());
        };
        let plan = ohl_store_pg::library::plan_pins(
            &self.net,
            &mut m.session,
            line,
            (!kinds.is_empty()).then_some(kinds),
            None,
        )
        .map_err(|e| e.to_string())?;
        Ok(PinsView {
            line: line.to_owned(),
            built: plan.built,
            moves: plan.moves(),
            refusal: ohl_store_pg::library::check_pin_rights(me, &plan).err(),
            text: plan.describe(),
            kinds: plan
                .kinds
                .iter()
                .map(|k| PinKindPlan {
                    kind: k.kind.clone(),
                    from: k.from.clone(),
                    to: k.to.clone(),
                    files: k.changes.len(),
                    changed: k.changed.clone(),
                    used_changed: k.used_changed.clone(),
                    stale_routes: k.stale_routes.clone(),
                    kept: k.kept.clone(),
                    withdrawn_used: k.withdrawn_used.clone(),
                    refused: k.refused.clone(),
                })
                .collect(),
        })
    }

    /// Update library pins of line `line` (planned again now, checked by
    /// the rights, carried out as the server's own command: logged,
    /// broadcast, the routes using what changed marked out of date).
    fn update_pins(
        &mut self,
        user: &str,
        me: &Arc<UserRights>,
        line: &str,
        kinds: &[String],
    ) -> Result<String, String> {
        if !me.sees_some_of_line(line) || !self.models.contains_key(line) {
            return Err(super::admin::HIDDEN.into());
        }
        let main = ohl_session::versions::MAIN;
        let op = self.plan_library_pins(me, line, main, kinds, None)?;
        let note = op.describe();
        self.apply(None, user, line, main, op)
            .map_err(|e| self.sanitise(me, &e))?;
        self.audit_admin(user, "admin.update_pins", &format!("line {line}"), &note);
        log(&format!(
            "{user} updated the library pins of line {line}: {note}"
        ));
        Ok(format!("line {line}: {note}"))
    }

    /// The lines `me` may see something of.
    fn lines_rows(&mut self, me: &UserRights) -> Vec<LineRow> {
        let all = me.sees_everything();
        let mut out = Vec::new();
        for (code, m) in &self.models {
            if !all && !me.sees_some_of_line(code) {
                continue;
            }
            let line = m.session.line();
            out.push(LineRow {
                code: code.to_string(),
                name: m.name.clone(),
                routes: line.routes.len(),
                structures: line.structures.len(),
                sections: line.sections.len(),
                stale: m.session.stale_routes().map(|s| s.len()).unwrap_or(0),
                whole: all || me.sees_whole_line(code),
                may_export: me.allows(Permission::Export, &Target::Line(code)),
            });
        }
        out
    }

    /// The transfers of client `client`, counted.
    fn transfers_of(&self, client: u64) -> usize {
        self.transfers
            .values()
            .filter(|t| t.client == client)
            .count()
    }

    /// A transfer's own folder in the server's copies folder.
    fn transfer_dir(&self) -> Result<(String, PathBuf), String> {
        let token = random_code(24);
        let dir = self.copies.join(format!("admin-{token}"));
        std::fs::create_dir_all(&dir).map_err(|e| format!("the server's folder: {e}"))?;
        Ok((token, dir))
    }

    /// Starts exporting line `model` for client `client`: the export
    /// permission on the whole line, then written on the exports' thread.
    fn export_line(
        &mut self,
        client: u64,
        id: u64,
        user: &str,
        me: &Arc<UserRights>,
        model: &str,
    ) -> Result<(), String> {
        if !self.models.contains_key(model) || !me.sees_some_of_line(model) {
            return Err(super::admin::HIDDEN.into());
        }
        if !me.allows(Permission::Export, &Target::Line(model)) {
            let why =
                format!("{user} may not export line {model}: it needs export on the whole line");
            self.audit_refused(user, &format!("admin: export line {model}"), &why);
            return Err(why);
        }
        if self.transfers_of(client) >= MAX_TRANSFERS {
            return Err(format!(
                "{MAX_TRANSFERS} files are held for this connection already: fetch or cancel one"
            ));
        }
        let (token, dir) = self.transfer_dir()?;
        let name = format!(
            "{}.ohl",
            safe_name(&format!("{model}.ohl"))?.trim_end_matches(".ohl")
        );
        let path = dir.join(&name);
        self.transfers.insert(
            token.clone(),
            Transfer {
                client,
                dir,
                path: path.clone(),
                name,
                bytes: 0,
                received: 0,
                upload: false,
                ready: false,
                file: None,
            },
        );
        log(&format!("{user} exports line {model} from the admin app"));
        let m = ModelExport {
            url: self.options.database.clone(),
            code: model.to_owned(),
            path,
            user: user.to_owned(),
            token,
        };
        if self.exports.on_thread() {
            self.start_model_export(client, id, user, me.clone(), m)
        } else {
            let r = write_model(&m);
            let now = self.client_rights(client);
            self.model_back(ExportTo::Client { client, id }, user, me, now, m, r);
            Ok(())
        }
    }

    /// A line's export back from the exports' thread: kept for its client
    /// to fetch, audited, answered — or nothing, its client gone or its
    /// user's rights changed meanwhile.
    pub(super) fn model_back(
        &mut self,
        to: ExportTo,
        user: &str,
        rights: &Arc<UserRights>,
        now: Option<Arc<UserRights>>,
        m: ModelExport,
        written: Result<ohl_store_pg::export::ExportReport, String>,
    ) {
        let ExportTo::Client { client, id } = to else {
            self.transfers.remove(&m.token);
            return;
        };
        let result = match (written, now) {
            (_, None) => {
                self.transfers.remove(&m.token);
                return;
            }
            (Err(e), Some(_)) => {
                self.transfers.remove(&m.token);
                Err(format!("the export of line {}: {e}", m.code))
            }
            (Ok(_), Some(now)) if !now.same_as(rights) => {
                self.transfers.remove(&m.token);
                let why = "your rights changed while the line was written: nothing exported — \
                           ask again"
                    .to_owned();
                self.audit_refused(user, &format!("admin: export line {}", m.code), &why);
                Err(why)
            }
            (Ok(r), Some(_)) => match self.transfers.get_mut(&m.token) {
                None => return,
                Some(t) => {
                    t.bytes = std::fs::metadata(&t.path).map_or(0, |x| x.len());
                    t.ready = true;
                    let note = format!(
                        "line {}: {} object rows, {} log entries, {} revisions, {} structures \
                         ({:.1} MB, written in {:.1} s)",
                        r.line,
                        r.objects,
                        r.entries,
                        r.revisions,
                        r.structures,
                        t.bytes as f64 / 1e6,
                        r.seconds
                    );
                    let info = TransferInfo {
                        token: m.token.clone(),
                        name: t.name.clone(),
                        bytes: t.bytes,
                        note: note.clone(),
                    };
                    self.audit_admin(
                        user,
                        "admin.export_line",
                        &format!("line {}", m.code),
                        &format!(
                            "{note}; revision {}",
                            r.revision.as_deref().unwrap_or("none")
                        ),
                    );
                    Ok(info)
                }
            },
        };
        if let Err(e) = &result {
            log(&format!("line export of {user}: {e}"));
        }
        let result = result
            .map(AdminReply::Transfer)
            .map_err(|e| self.sanitise(rights, &e));
        self.push(client, None, None, Some(ServerMsg::Admin { id, result }));
    }

    /// A piece of a file held for client `client`.
    fn download(&mut self, client: u64, token: &str, offset: u64) -> Result<ChunkData, String> {
        let t = self
            .transfers
            .get(token)
            .filter(|t| t.client == client && !t.upload && t.ready)
            .ok_or("no such file for this connection (gone, or not written yet)")?;
        if offset > t.bytes {
            return Err(format!("byte {offset} past the file's end ({})", t.bytes));
        }
        let mut f = std::fs::File::open(&t.path).map_err(|e| format!("the file: {e}"))?;
        f.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("the file: {e}"))?;
        let want = CHUNK.min((t.bytes - offset) as usize);
        let mut buf = vec![0u8; want];
        f.read_exact(&mut buf)
            .map_err(|e| format!("the file: {e}"))?;
        Ok(ChunkData {
            offset,
            last: offset + want as u64 >= t.bytes,
            data: crate::base64::encode(&buf),
        })
    }

    /// Starts receiving a project file (network admins: the gate checked).
    fn upload_start(
        &mut self,
        client: u64,
        user: &str,
        name: &str,
        bytes: u64,
    ) -> Result<TransferInfo, String> {
        if bytes == 0 || bytes > MAX_UPLOAD {
            return Err(format!(
                "a project file of 1 byte to {:.0} GB",
                MAX_UPLOAD as f64 / 1e9
            ));
        }
        let name = safe_name(name)?;
        if self.transfers_of(client) >= MAX_TRANSFERS {
            return Err(format!(
                "{MAX_TRANSFERS} files are held for this connection already: fetch or cancel one"
            ));
        }
        let (token, dir) = self.transfer_dir()?;
        let path = dir.join(&name);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("the server's folder: {e}"))?;
        log(&format!(
            "{user} uploads {name} ({:.1} MB) from the admin app",
            bytes as f64 / 1e6
        ));
        self.transfers.insert(
            token.clone(),
            Transfer {
                client,
                dir,
                path,
                name: name.clone(),
                bytes,
                received: 0,
                upload: true,
                ready: false,
                file: Some(file),
            },
        );
        Ok(TransferInfo {
            token,
            name,
            bytes,
            note: "send the file in pieces, in order".into(),
        })
    }

    /// A piece of an upload, in order.
    fn upload(&mut self, client: u64, token: &str, offset: u64, data: &str) -> Result<u64, String> {
        let t = self
            .transfers
            .get_mut(token)
            .filter(|t| t.client == client && t.upload && !t.ready)
            .ok_or("no such upload for this connection")?;
        if offset != t.received {
            return Err(format!("byte {} expected next ({offset} sent)", t.received));
        }
        let bytes = crate::base64::decode(data)?;
        if bytes.len() > CHUNK || t.received + bytes.len() as u64 > t.bytes {
            return Err("a piece too long".into());
        }
        let f = t.file.as_mut().ok_or("the upload's file is closed")?;
        f.write_all(&bytes)
            .map_err(|e| format!("the server's folder: {e}"))?;
        t.received += bytes.len() as u64;
        if t.received == t.bytes {
            if let Some(f) = t.file.take() {
                f.sync_all()
                    .map_err(|e| format!("the server's folder: {e}"))?;
            }
            t.ready = true;
        }
        Ok(t.received)
    }

    /// The uploaded file imported as line `code` (network admins: the
    /// gate checked): the network import on the writer, then served.
    fn import_line(
        &mut self,
        client: u64,
        user: &str,
        token: &str,
        code: &str,
        name: Option<String>,
        tolerance: Option<f64>,
    ) -> Result<ImportView, String> {
        let t = self
            .transfers
            .get(token)
            .filter(|t| t.client == client && t.upload)
            .ok_or("no such upload for this connection")?;
        if !t.ready {
            return Err(format!(
                "the file is not all here yet ({} of {} bytes)",
                t.received, t.bytes
            ));
        }
        let path = t.path.clone();
        let mut o = ohl_store_pg::import::ImportOptions::new(code, user);
        o.name = name.filter(|n| !n.trim().is_empty());
        if let Some(tol) = tolerance {
            o.tolerance = tol;
        }
        let r = ohl_store_pg::import::import(&self.net, &path, &o);
        self.transfers.remove(token);
        let r = r.map_err(|e| e.to_string())?;
        self.add_model(code)?;
        log(&format!(
            "{user} imported line {code} from the admin app ({} renamed, {} twins, {:.1} s)",
            r.renamed.len(),
            r.twins.len(),
            r.seconds
        ));
        self.audit_admin(
            user,
            "admin.import_line",
            &format!("line {code}"),
            &format!(
                "from the admin app: {} object kinds, {} ids renamed, {} twin towers listed",
                r.objects.len(),
                r.renamed.len(),
                r.twins.len()
            ),
        );
        Ok(ImportView {
            text: r.text(),
            line: r.line.clone(),
            name: r.name.clone(),
            file: r.file.clone(),
            objects: r.objects.iter().map(|(k, n)| (k.clone(), *n)).collect(),
            renamed: r
                .renamed
                .iter()
                .map(|x| (x.kind.clone(), x.from.clone(), x.to.clone()))
                .collect(),
            twins: r
                .twins
                .iter()
                .map(|x| TwinRow {
                    line: r.line.clone(),
                    structure: x.structure.clone(),
                    other_line: x.other_line.clone(),
                    other: x.other.clone(),
                    distance_m: x.distance_m,
                    structure_type: x.structure_type.clone(),
                })
                .collect(),
            users: r.users.clone(),
            left_in_file: r.left_in_file.clone(),
            seconds: r.seconds,
        })
    }

    /// A page of the audit (network admins: the gate checked).
    fn audit_read(
        &mut self,
        f: &crate::admin::AuditFilter,
        before: Option<i64>,
        limit: usize,
    ) -> Result<AuditPage, String> {
        let limit = if limit == 0 {
            200
        } else {
            limit.min(AUDIT_PAGE_MAX)
        };
        let q = ohl_store_pg::rights::AuditQuery {
            user: f.user.trim().to_owned(),
            action: f.action.trim().to_owned(),
            target: f.target.trim().to_owned(),
            text: f.text.trim().to_owned(),
            from: f.from.clone(),
            until: f.until.clone(),
        };
        let (mut rows, total) = self
            .net
            .audit_page(&q, before, limit as i64 + 1)
            .map_err(|e| e.to_string())?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        Ok(AuditPage {
            rows: rows
                .into_iter()
                .map(|(id, [time, user, action, target, detail])| AuditRow {
                    id,
                    time,
                    user,
                    action,
                    target,
                    detail,
                })
                .collect(),
            total: total.max(0) as u64,
            more,
        })
    }

    /// Why `who` may or may not (the asker sees the target; audited).
    #[allow(clippy::too_many_arguments)]
    fn explain(
        &mut self,
        user: &str,
        me: &UserRights,
        who: &str,
        p: Permission,
        line: Option<String>,
        kind: Option<String>,
        object: Option<String>,
    ) -> Result<ExplainView, String> {
        if !self.rights.users.contains_key(who) {
            return Err(format!("no user {who} on this network"));
        }
        let kind = match &kind {
            Some(k) => Some(Kind::parse(k).map_err(|e| e.to_string())?),
            None => None,
        };
        let place = match (&line, kind, &object) {
            (Some(l), Some(k), Some(i)) => self.models.get(l.as_str()).and_then(|m| {
                m.places.get(k, i).map(|p| nr::Place {
                    bounds: p.bounds,
                    routes: p.routes.clone(),
                })
            }),
            _ => None,
        };
        let (target, words) = match (&line, kind, &object) {
            (None, _, _) => (Target::Network, "the network".to_owned()),
            (Some(l), None, _) => (Target::Line(l), format!("line {l}")),
            (Some(l), Some(k), Some(i)) => (
                nr::target(l, k, i, place.as_ref()),
                format!("{} {i} of line {l}", k.as_str()),
            ),
            (Some(_), Some(_), None) => return Err("an object kind needs its id".into()),
        };
        // An admin learns only about what they may see themselves.
        if !matches!(target, Target::Network) && !me.allows(Permission::See, &target) {
            return Err(super::admin::HIDDEN.into());
        }
        let them = self.rights_of(who);
        let (may, granted_by, denied_by) = them.explain(p, &target);
        let mut notes = Vec::new();
        if !them.active() {
            notes.push(format!("{who} is not an active user of the network"));
        }
        if p.needs_see() && !them.allows(Permission::See, &target) {
            notes.push(format!(
                "{} needs see, which is not given there",
                p.as_str()
            ));
        }
        self.audit_admin(
            user,
            "explain",
            &format!("user:{who}"),
            &format!("{} on {words} (the admin app)", p.as_str()),
        );
        Ok(ExplainView {
            user: who.to_owned(),
            permission: p,
            target: words,
            may,
            granted_by,
            denied_by,
            notes,
        })
    }
}
