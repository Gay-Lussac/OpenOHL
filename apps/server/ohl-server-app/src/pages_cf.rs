// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The pages of stages C to F (2026-11-28): Shared towers (with the
//! twins), Lines (export, import), Libraries (locks, maintainers,
//! requests, releases, withdrawn entries, the use index, Update library
//! pins), Audit (filtered, paged, to CSV); the identities on the Users
//! page, "explain" on the See as page, a network setting changed; their
//! windows. Each shows what the server answered for the user's rights;
//! every change goes through a window and is in the server's audit.

use std::time::Duration;

use eframe::egui;
use ohl_server::admin::LibraryAction;
use ohl_session::rights::{Permission, RightsCommand};
use ohl_ui::tool_window as tw;

use crate::app::{App, Page, Window};
use crate::pages::LIST_HEIGHT;

/// Audit rows on one page.
pub const AUDIT_ROWS: usize = 200;

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

/// A line's code from a file name (letters, digits, `-`, `_`; upper case).
pub fn code_of(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|s| {
            s.to_string_lossy()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                .take(32)
                .collect::<String>()
                .to_ascii_uppercase()
        })
        .unwrap_or_default()
}

impl App {
    /// Shared towers (stage C, with phase 8): towers on two routes or
    /// more, their routes and stale marks, who may edit one; a line's
    /// twin towers (shown, never merged).
    pub(crate) fn shared_page(&mut self, ui: &mut egui::Ui) {
        if self.due("shared", Duration::from_secs(30))
            && let Some(c) = &mut self.conn
        {
            c.ask_shared();
            c.ask_lines();
        }
        Self::page_intro_cf(
            ui,
            Page::Shared,
            "Towers carrying two routes or more: their routes, which of them have results out of \
             date (a shared tower moved, a library updated), and who may edit a tower.",
            "Read-only. Twin towers — a line's tower within a distance of another line's of the \
             same type — are shown, never merged (twins stay as they are).",
        );
        let mut on_map = None;
        let Some(c) = &mut self.conn else { return };
        let Some(s) = c.data.shared.clone() else {
            ui.spinner();
            return;
        };
        ui.label(format!(
            "{} shared tower(s) you may see, {} on a route whose results are out of date{}.",
            s.total,
            s.stale,
            if s.rows.len() < s.total {
                format!(" (the first {} listed)", s.rows.len())
            } else {
                String::new()
            }
        ));
        let mut detail = None;
        egui::ScrollArea::vertical()
            .id_salt("shared")
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                egui::Grid::new("shared-grid")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Line", "Tower", "Routes", ""] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in &s.rows {
                            ui.label(&r.model);
                            ui.label(&r.id);
                            ui.vertical(|ui| {
                                for rt in &r.routes {
                                    match &rt.stale {
                                        Some(why) => {
                                            ohl_ui::messages::error_label(
                                                ui,
                                                egui::RichText::new(format!(
                                                    "{} {} — out of date: {why}",
                                                    rt.id, rt.name
                                                )),
                                            );
                                        }
                                        None => {
                                            ui.label(format!("{} {}", rt.id, rt.name));
                                        }
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.small_button("Who may edit").clicked() {
                                    detail = Some((r.model.clone(), r.id.clone()));
                                }
                                if ui.small_button("On the map").clicked() {
                                    on_map = Some(r.at);
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        if let Some((m, id)) = detail {
            c.ask_shared_tower(&m, &id);
        }
        if let Some(d) = &c.data.shared_tower {
            ui.add_space(4.0);
            ui.strong(format!("Tower {} of line {}", d.row.id, d.row.model));
            ui.label(match &d.editors {
                Some(e) if e.is_empty() => "Nobody may edit it.".to_owned(),
                Some(e) => format!("Who may edit it: {}.", e.join(", ")),
                None => "Who may edit it is shown to who manages rights on its line.".to_owned(),
            });
        }
        ui.separator();
        tw::heading(
            ui,
            "Twin towers",
            "A line's towers within the distance of another line's tower of the same type (the \
             import's test): likely one tower in two models. Shown, never merged.",
        );
        let lines: Vec<String> = c
            .data
            .lines
            .as_ref()
            .map(|l| l.iter().map(|l| l.code.clone()).collect())
            .unwrap_or_default();
        let p = &mut self.pages;
        if p.twin_tolerance <= 0.0 {
            p.twin_tolerance = 0.5;
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("twins-line")
                .selected_text(if p.twins_line.is_empty() {
                    "line…"
                } else {
                    p.twins_line.as_str()
                })
                .show_ui(ui, |ui| {
                    for l in lines {
                        ui.selectable_value(&mut p.twins_line, l.clone(), l);
                    }
                });
            tw::label(ui, "within", "The plan distance, m (the import's: 0.5 m)");
            ohl_ui::style::field(
                ui,
                egui::DragValue::new(&mut p.twin_tolerance)
                    .range(0.01..=50.0)
                    .speed(0.1)
                    .suffix(" m"),
            );
            if ui
                .add_enabled(!p.twins_line.is_empty(), egui::Button::new("Find twins"))
                .clicked()
            {
                c.ask_twins(&p.twins_line.clone(), Some(p.twin_tolerance));
            }
        });
        if let Some((line, twins)) = &c.data.twins {
            ui.label(format!("Line {line}: {} twin tower(s).", twins.len()));
            egui::ScrollArea::vertical()
                .id_salt("twins")
                .max_height(LIST_HEIGHT / 2.0)
                .show(ui, |ui| {
                    egui::Grid::new("twins-grid")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["Tower", "Other line's tower", "Distance", "Type"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for t in twins {
                                ui.label(&t.structure);
                                ui.label(format!("{} {}", t.other_line, t.other));
                                ui.label(format!("{:.2} m", t.distance_m));
                                ui.label(t.structure_type.as_deref().unwrap_or("point by point"));
                                ui.end_row();
                            }
                        });
                });
        }
        if let Some(at) = on_map {
            self.map.centre_on(at);
            self.page = Page::Map;
        }
    }

    fn page_intro_cf(ui: &mut egui::Ui, page: Page, what: &str, more: &str) {
        ui.heading(page.name());
        tw::intro(ui, what, more, page.name(), || {
            crate::help::open_section("user-guide/ohl-server", &tw::anchor(page.name()))
        });
    }

    /// Lines (stage E): the lines the user may see; one exported as a
    /// standalone project file (written on the server, fetched in
    /// pieces); a project file imported as a new line (network admins).
    pub(crate) fn lines_page(&mut self, ui: &mut egui::Ui) {
        if self.due("lines", Duration::from_secs(30))
            && let Some(c) = &mut self.conn
        {
            c.ask_lines();
        }
        Self::page_intro_cf(
            ui,
            Page::Lines,
            "The network's lines you may see. Export one as a standalone project file (it opens \
             in OHL-Design alone); import a project file as a new line (network admins).",
            "An export needs export on the whole line and is in the audit; it is written on the \
             server's exports' thread and fetched in pieces. An import renames ids another line \
             has and lists twin towers (never merged); it is in the audit.",
        );
        let mut open = None;
        let mut export = None;
        let admin = self.conn.as_ref().is_some_and(|c| c.data.network_admin());
        let Some(c) = &mut self.conn else { return };
        let busy = c.data.download.is_some() || c.data.upload.is_some();
        ui.horizontal(|ui| {
            if admin
                && ui
                    .add_enabled(!busy, egui::Button::new("Import a line…"))
                    .on_hover_text("A project file (.ohl) from this computer, as a new line")
                    .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("OpenOHL project", &["ohl"])
                    .pick_file()
            {
                open = Some(Window::Import {
                    code: code_of(&path),
                    path,
                    name: String::new(),
                    tolerance: 0.5,
                });
            }
        });
        if let Some(d) = &c.data.download {
            ui.horizontal(|ui| {
                if d.bytes == 0 {
                    ui.spinner();
                    ui.label(format!("line {}: written on the server…", d.line));
                } else {
                    ui.add(
                        egui::ProgressBar::new(d.got as f32 / d.bytes.max(1) as f32)
                            .desired_width(240.0)
                            .text(format!("line {}: {} of {}", d.line, mb(d.got), mb(d.bytes))),
                    );
                }
            });
        }
        if let Some(u) = &c.data.upload {
            ui.horizontal(|ui| {
                if u.importing {
                    ui.spinner();
                    ui.label(format!("importing line {} on the server…", u.code));
                } else {
                    ui.add(
                        egui::ProgressBar::new(u.sent as f32 / u.bytes.max(1) as f32)
                            .desired_width(240.0)
                            .text(format!("sending: {} of {}", mb(u.sent), mb(u.bytes))),
                    );
                }
            });
        }
        let Some(lines) = c.data.lines.clone() else {
            ui.spinner();
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("lines")
            .max_height(LIST_HEIGHT)
            .show(ui, |ui| {
                egui::Grid::new("lines-grid")
                    .num_columns(7)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in [
                            "Line",
                            "Name",
                            "Routes",
                            "Structures",
                            "Sections",
                            "Out of date",
                            "",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for l in &lines {
                            ui.label(format!(
                                "{}{}",
                                l.code,
                                if l.whole { "" } else { " (part)" }
                            ));
                            ui.label(&l.name);
                            ui.label(l.routes.to_string());
                            ui.label(l.structures.to_string());
                            ui.label(l.sections.to_string());
                            ui.label(l.stale.to_string());
                            let r = ui
                                .add_enabled(l.may_export && !busy, egui::Button::new("Export…"))
                                .on_disabled_hover_text(if l.may_export {
                                    "A file is being sent or fetched"
                                } else {
                                    "Export on the whole line is needed"
                                });
                            if r.clicked() {
                                export = Some(l.code.clone());
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(code) = export
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("OpenOHL project", &["ohl"])
                .set_file_name(format!("{code}.ohl"))
                .save_file()
        {
            c.export_line(&code, path);
        }
        if let Some(v) = self.pages.imported.clone() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(format!(
                    "Line {} imported from {}: {} id(s) renamed, {} twin tower(s) listed, in {:.1} s",
                    v.line,
                    v.file,
                    v.renamed.len(),
                    v.twins.len(),
                    v.seconds
                ));
                if ui.small_button("Close the report").clicked() {
                    self.pages.imported = None;
                }
            });
            if let Some(v) = &self.pages.imported {
                egui::ScrollArea::vertical()
                    .id_salt("import-report")
                    .max_height(LIST_HEIGHT / 2.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&v.text).monospace());
                    });
            }
        }
        if open.is_some() {
            self.window = open;
        }
    }

    /// Libraries (stage D, with phase 9).
    pub(crate) fn libraries_page(&mut self, ui: &mut egui::Ui) {
        if self.due("library-page", Duration::from_secs(60))
            && let Some(c) = &mut self.conn
        {
            c.ask_library_page();
        }
        Self::page_intro_cf(
            ui,
            Page::Libraries,
            "The network's libraries: each kind's lock and maintainers, its files and entries, its \
             releases and withdrawn entries; the requests; which lines pin which release.",
            "Maintainers grant or refuse requests (with a reason), release, withdraw and lock — on \
             their libraries only; the server decides each change and keeps it in the audit. \
             Update library pins is shown as a proposal first.",
        );
        let mut open = None;
        let Some(c) = &mut self.conn else { return };
        let Some(page) = c.data.library.clone() else {
            ui.spinner();
            return;
        };
        let mut uses = None;
        let mut lock = None;
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            tw::heading(
                ui,
                "Kinds",
                "Each library kind you may see: locked or not, its maintainers, releases, files and \
                 entries (version, the release it came in, withdrawn).",
            );
            egui::ScrollArea::vertical()
                .id_salt("lib-kinds")
                .max_height(LIST_HEIGHT + 80.0)
                .show(ui, |ui| {
                    for k in &page.kinds {
                        egui::CollapsingHeader::new(format!(
                            "{}{}",
                            k.kind,
                            if k.locked { " (locked)" } else { "" }
                        ))
                        .id_salt(("lib-kind", &k.kind))
                        .default_open(page.kinds.len() == 1)
                        .show(ui, |ui| {
                            ui.label(format!(
                                "Maintainers: {}",
                                if k.maintainers.is_empty() {
                                    "none".to_owned()
                                } else {
                                    k.maintainers.join(", ")
                                }
                            ));
                            ui.label(format!(
                                "Releases: {}",
                                if k.releases.is_empty() {
                                    "none".to_owned()
                                } else {
                                    k.releases.join(", ")
                                }
                            ));
                            if k.maintain {
                                ui.horizontal(|ui| {
                                    if ui
                                        .small_button(if k.locked { "Unlock…" } else { "Lock…" })
                                        .clicked()
                                    {
                                        lock = Some(LibraryAction::Lock {
                                            kind: k.kind.clone(),
                                            file: None,
                                            locked: !k.locked,
                                            note: String::new(),
                                        });
                                    }
                                    if ui.small_button("Release…").clicked() {
                                        lock = Some(LibraryAction::Release {
                                            kind: k.kind.clone(),
                                            tag: String::new(),
                                            notes: String::new(),
                                        });
                                    }
                                });
                            }
                            for f in &k.files {
                                egui::CollapsingHeader::new(format!(
                                    "{}{} — {} entr{}",
                                    f.file,
                                    if f.locked { " (locked)" } else { "" },
                                    f.entries.len(),
                                    if f.entries.len() == 1 { "y" } else { "ies" }
                                ))
                                .id_salt(("lib-file", &k.kind, &f.file))
                                .show(ui, |ui| {
                                    for e in &f.entries {
                                        ui.horizontal(|ui| {
                                            let text = format!(
                                                "{} {} — {}{}",
                                                e.id,
                                                e.version,
                                                e.release.as_deref().unwrap_or("not released"),
                                                if e.withdrawn { " — withdrawn" } else { "" }
                                            );
                                            if e.withdrawn {
                                                ui.weak(text);
                                            } else {
                                                ui.label(text);
                                            }
                                            if ui.small_button("Uses").clicked() {
                                                uses = Some((k.kind.clone(), e.id.clone()));
                                            }
                                            if k.maintain {
                                                let action = if e.withdrawn {
                                                    ("Restore…", LibraryAction::Restore {
                                                        kind: k.kind.clone(),
                                                        file: f.file.clone(),
                                                        entry: e.id.clone(),
                                                    })
                                                } else {
                                                    ("Withdraw…", LibraryAction::Withdraw {
                                                        kind: k.kind.clone(),
                                                        file: f.file.clone(),
                                                        entry: e.id.clone(),
                                                        note: String::new(),
                                                    })
                                                };
                                                if ui.small_button(action.0).clicked() {
                                                    lock = Some(action.1);
                                                }
                                            }
                                        });
                                    }
                                });
                            }
                        });
                    }
                });
            let ui = &mut cols[1];
            tw::heading(
                ui,
                "Requests",
                "Entries asked for: yours, and those of the libraries you maintain. A maintainer \
                 who did not ask grants or refuses (a refusal says why).",
            );
            egui::ScrollArea::vertical()
                .id_salt("lib-requests")
                .max_height(LIST_HEIGHT / 2.0)
                .show(ui, |ui| {
                    if page.requests.is_empty() {
                        ui.weak("No request.");
                    }
                    for r in &page.requests {
                        let maintain = page
                            .kinds
                            .iter()
                            .any(|k| k.kind == r.kind && k.maintain);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "#{} {} {} into {} — {} by {}{}",
                                r.id,
                                r.kind,
                                r.entry,
                                r.file,
                                r.state,
                                r.user,
                                r.line
                                    .as_deref()
                                    .map_or(String::new(), |l| format!(" for line {l}"))
                            ));
                            if r.state == "requested" && maintain {
                                if ui.small_button("Grant…").clicked() {
                                    lock = Some(LibraryAction::Grant {
                                        id: r.id,
                                        note: String::new(),
                                    });
                                }
                                if ui.small_button("Refuse…").clicked() {
                                    lock = Some(LibraryAction::Refuse {
                                        id: r.id,
                                        note: String::new(),
                                    });
                                }
                            }
                        });
                        if !r.reason.is_empty() {
                            ui.weak(format!("    {}", r.reason));
                        }
                        if let Some(n) = &r.note {
                            ui.weak(format!(
                                "    {}: {n}",
                                r.maintainer.as_deref().unwrap_or("decided")
                            ));
                        }
                    }
                });
            tw::heading(
                ui,
                "Which lines pin which release",
                "Each line's pin per kind against the latest release; built lines stay pinned \
                 (moved only by who may review the whole line), designs follow the latest when \
                 their pins are updated.",
            );
            egui::ScrollArea::vertical()
                .id_salt("lib-pins")
                .max_height(LIST_HEIGHT / 2.0)
                .show(ui, |ui| {
                    egui::Grid::new("lib-pins-grid")
                        .num_columns(3)
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["Line", "Pins", ""] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for l in &page.lines {
                                ui.label(format!(
                                    "{}{}",
                                    l.line,
                                    if l.built { " (built)" } else { "" }
                                ));
                                ui.label(
                                    l.kinds
                                        .iter()
                                        .map(|k| {
                                            format!(
                                                "{} {}{}{}",
                                                k.kind,
                                                k.pinned.as_deref().unwrap_or("own files"),
                                                if k.behind > 0 {
                                                    format!(" ({} behind {})", k.behind, k.latest.as_deref().unwrap_or("?"))
                                                } else {
                                                    String::new()
                                                },
                                                if k.withdrawn_used.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!("; withdrawn used: {}", k.withdrawn_used.join(", "))
                                                }
                                            )
                                        })
                                        .collect::<Vec<_>>()
                                        .join("; "),
                                );
                                if ui.small_button("Update library pins…").clicked() {
                                    open = Some(Window::Pins(l.line.clone()));
                                }
                                ui.end_row();
                            }
                        });
                });
            if let Some(((kind, entry), list)) = &c.data.uses {
                ui.strong(format!("{kind} {entry}: {} use(s) you may see", list.len()));
                egui::ScrollArea::vertical()
                    .id_salt("lib-uses")
                    .max_height(LIST_HEIGHT / 3.0)
                    .show(ui, |ui| {
                        for u in list {
                            ui.label(format!(
                                "{} {} {} {}{}",
                                u.line,
                                u.branch,
                                u.object_kind,
                                u.object,
                                if u.routes.is_empty() {
                                    String::new()
                                } else {
                                    format!(" (routes {})", u.routes.join(", "))
                                }
                            ));
                        }
                    });
            }
        });
        if let Some((k, e)) = uses {
            c.ask_uses(&k, &e);
        }
        if let Some(a) = lock {
            open = Some(Window::Library(a));
        }
        if open.is_some() {
            self.window = open;
        }
    }

    /// Audit (stage F, with phase 13; network admins): the network's
    /// append-only audit, filtered, paged, to CSV.
    pub(crate) fn audit_page(&mut self, ui: &mut egui::Ui) {
        if self.pages.audit_asked.is_none()
            && let Some(c) = &mut self.conn
        {
            c.ask_audit(&self.pages.audit_filter, None, AUDIT_ROWS);
            self.pages.audit_asked = Some(Vec::new());
        }
        Self::page_intro_cf(
            ui,
            Page::Audit,
            "The network's audit: every rights change, refusal, export, import, library change, \
             backup and \"see as\", newest first. Append-only: nobody changes or deletes a row.",
            "Filter by who (the user), what (the action's start: admin., library., refused…), the \
             scope (text in the target: line L001, user:bob) and when (UTC). Export writes every \
             row of the filter to a CSV file of this computer.",
        );
        let Some(c) = &mut self.conn else { return };
        let f = &mut self.pages.audit_filter;
        let mut apply = false;
        egui::Grid::new("audit-filter")
            .num_columns(6)
            .show(ui, |ui| {
                let mut field = |ui: &mut egui::Ui, label: &str, help: &str, v: &mut String| {
                    tw::label(ui, label, help);
                    apply |= ui
                        .add(egui::TextEdit::singleline(v).desired_width(140.0))
                        .lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                };
                field(ui, "Who", "The user, exactly", &mut f.user);
                field(
                    ui,
                    "What",
                    "The action's start: admin., library., refused, see_as…",
                    &mut f.action,
                );
                field(
                    ui,
                    "Scope",
                    "Text in the target: line L001, user:bob, cables…",
                    &mut f.target,
                );
                ui.end_row();
                field(ui, "Text", "Text in the detail", &mut f.text);
                field(
                    ui,
                    "From",
                    "UTC: YYYY-MM-DD or YYYY-MM-DD HH:MM",
                    &mut f.from,
                );
                field(
                    ui,
                    "Until",
                    "UTC: YYYY-MM-DD (to its end) or YYYY-MM-DD HH:MM",
                    &mut f.until,
                );
                ui.end_row();
            });
        let page = c.data.audit.clone();
        let mut ask: Option<Option<i64>> = None;
        let stack = self.pages.audit_asked.get_or_insert_with(Vec::new);
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() || apply {
                stack.clear();
                ask = Some(None);
            }
            if ui
                .add_enabled(!stack.is_empty(), egui::Button::new("Newer"))
                .clicked()
            {
                stack.pop();
                ask = Some(stack.last().copied());
            }
            let older = page
                .as_ref()
                .filter(|p| p.more)
                .and_then(|p| p.rows.last())
                .map(|r| r.id);
            if ui
                .add_enabled(older.is_some(), egui::Button::new("Older"))
                .clicked()
                && let Some(id) = older
            {
                stack.push(id);
                ask = Some(Some(id));
            }
            let exporting = c.data.audit_export.is_some();
            if ui
                .add_enabled(!exporting, egui::Button::new("Export to CSV…"))
                .on_hover_text("Every row of the filter, to a CSV file of this computer")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("audit.csv")
                    .save_file()
            {
                c.export_audit(&self.pages.audit_filter, path);
            }
            if let Some(x) = &c.data.audit_export {
                ui.spinner();
                ui.label(format!("{} of {} rows written", x.rows, x.total));
            }
        });
        if let Some(before) = ask {
            c.ask_audit(&self.pages.audit_filter, before, AUDIT_ROWS);
        }
        let Some(page) = page else {
            ui.spinner();
            return;
        };
        ui.label(format!(
            "{} row(s) match; {} shown{}.",
            page.total,
            page.rows.len(),
            match (page.rows.first(), page.rows.last()) {
                (Some(a), Some(b)) => format!(" (#{} to #{})", a.id, b.id),
                _ => String::new(),
            }
        ));
        egui::ScrollArea::both()
            .id_salt("audit")
            .max_height(LIST_HEIGHT + 120.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("audit-grid")
                    .num_columns(6)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["#", "Time (UTC)", "Who", "What", "On", "Detail"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in &page.rows {
                            ui.label(r.id.to_string());
                            ui.label(&r.time);
                            ui.label(&r.user);
                            ui.label(&r.action);
                            ui.label(&r.target);
                            let d: String = r.detail.chars().take(160).collect();
                            ui.label(d).on_hover_text(&r.detail);
                            ui.end_row();
                        }
                    });
            });
    }

    /// The identities (stage F): OIDC subjects and the network users they
    /// sign in as (network admins; the Users page).
    pub(crate) fn identities_section(&mut self, ui: &mut egui::Ui) {
        let Some(c) = &mut self.conn else { return };
        if !c.data.network_admin() || !c.data.stages_cf() {
            return;
        }
        if c.data.identities.is_none() {
            c.ask_identities();
        }
        ui.separator();
        tw::heading(
            ui,
            "Identities",
            "The organisation's identity provider (OIDC): each subject signs in as the network \
             user it is mapped to; an unmapped subject is refused. Network admins only, in the \
             audit.",
        );
        let mut open = None;
        let users: Vec<String> = c
            .data
            .rights
            .as_ref()
            .map(|r| r.users.iter().map(|u| u.name.clone()).collect())
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt("identities")
            .max_height(LIST_HEIGHT / 2.0)
            .show(ui, |ui| {
                egui::Grid::new("identities-grid")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for h in ["Issuer", "Subject", "User", ""] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for i in c.data.identities.iter().flatten() {
                            ui.label(&i.issuer);
                            ui.label(&i.subject);
                            ui.label(&i.user);
                            if ui.small_button("Unmap…").clicked() {
                                open = Some(Window::Change(RightsCommand::UnmapIdentity {
                                    issuer: i.issuer.clone(),
                                    subject: i.subject.clone(),
                                }));
                            }
                            ui.end_row();
                        }
                    });
            });
        let p = &mut self.pages;
        ui.horizontal(|ui| {
            tw::label(ui, "Issuer", "As the server's --oidc-issuer");
            ui.add(egui::TextEdit::singleline(&mut p.id_issuer).desired_width(200.0));
            tw::label(ui, "Subject", "The ID token's sub");
            ui.add(egui::TextEdit::singleline(&mut p.id_subject).desired_width(140.0));
            egui::ComboBox::from_id_salt("identity-user")
                .selected_text(if p.id_user.is_empty() {
                    "user…"
                } else {
                    p.id_user.as_str()
                })
                .show_ui(ui, |ui| {
                    for u in users {
                        ui.selectable_value(&mut p.id_user, u.clone(), u);
                    }
                });
            let ok = !p.id_issuer.trim().is_empty()
                && !p.id_subject.trim().is_empty()
                && !p.id_user.is_empty();
            if ui.add_enabled(ok, egui::Button::new("Map…")).clicked() {
                open = Some(Window::Change(RightsCommand::MapIdentity {
                    issuer: p.id_issuer.trim().to_owned(),
                    subject: p.id_subject.trim().to_owned(),
                    user: p.id_user.clone(),
                }));
            }
        });
        if open.is_some() {
            self.window = open;
        }
    }

    /// "Explain" (stage F, the See as page): why a user may or may not.
    pub(crate) fn explain_section(&mut self, ui: &mut egui::Ui) {
        let Some(c) = &mut self.conn else { return };
        if !c.data.stages_cf() {
            return;
        }
        ui.separator();
        tw::heading(
            ui,
            "Explain",
            "Why the user picked above may or may not do something: the assignments granting and \
             denying it there (the server's own function; within what you may see; in the audit).",
        );
        let lines: Vec<String> = c
            .data
            .map
            .as_ref()
            .map(|m| m.lines.iter().map(|l| l.0.clone()).collect())
            .unwrap_or_default();
        let p = &mut self.pages;
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("explain-permission")
                .selected_text(p.explain_permission.as_str())
                .show_ui(ui, |ui| {
                    for q in Permission::ALL {
                        ui.selectable_value(&mut p.explain_permission, q, q.as_str());
                    }
                });
            ui.label("on");
            egui::ComboBox::from_id_salt("explain-line")
                .selected_text(if p.explain_line.is_empty() {
                    "the network"
                } else {
                    p.explain_line.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.explain_line, String::new(), "the network");
                    for l in lines {
                        ui.selectable_value(&mut p.explain_line, l.clone(), l);
                    }
                });
            tw::label(
                ui,
                "Kind",
                "An object kind (structure, section…; empty: the line whole)",
            );
            ui.add(egui::TextEdit::singleline(&mut p.explain_kind).desired_width(90.0));
            tw::label(ui, "Id", "The object's id");
            ui.add(egui::TextEdit::singleline(&mut p.explain_id).desired_width(90.0));
            let ok = !p.see_as.is_empty();
            if ui
                .add_enabled(ok, egui::Button::new("Explain"))
                .on_disabled_hover_text("Pick a user above first")
                .clicked()
            {
                let opt = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
                c.ask_explain(
                    &p.see_as,
                    p.explain_permission,
                    opt(&p.explain_line),
                    opt(&p.explain_kind),
                    opt(&p.explain_id),
                );
            }
        });
        if let Some(e) = &c.data.explain {
            let fmt = |v: &[i64]| {
                if v.is_empty() {
                    "none".to_owned()
                } else {
                    v.iter()
                        .map(|i| format!("#{i}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            };
            ui.strong(format!(
                "{} {} {} on {}.",
                e.user,
                if e.may { "may" } else { "may not" },
                e.permission.as_str(),
                e.target
            ));
            ui.label(format!(
                "Granted by {}; denied by {}.",
                fmt(&e.granted_by),
                fmt(&e.denied_by)
            ));
            for n in &e.notes {
                ui.weak(n);
            }
        }
    }

    /// The windows of stages C to F (a library change, Update library
    /// pins, an import, a setting); true when `w` is one of them.
    pub(crate) fn cf_window(&mut self, ctx: &egui::Context, w: &Window) -> bool {
        if !matches!(
            w,
            Window::Library(_) | Window::Pins(_) | Window::Import { .. } | Window::Setting { .. }
        ) {
            return false;
        }
        let Some(c) = &mut self.conn else {
            self.window = None;
            return true;
        };
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut open = true;
        let mut close = escape;
        let mut keep = None;
        let help = |section: &'static str| {
            move || crate::help::open_section("user-guide/ohl-server", section)
        };
        match w.clone() {
            Window::Library(mut a) => {
                egui::Window::new("Library change")
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        tw::intro(
                            ui,
                            &format!("To {}.", a.describe()),
                            "The server decides it (maintainers of the kind; a grant by a \
                             maintainer who did not ask) and keeps it in the audit.",
                            "library",
                            help("libraries"),
                        );
                        let ok = match &mut a {
                            LibraryAction::Refuse { note, .. } => {
                                tw::label(ui, "Why", "Required: its requester is shown it");
                                ui.text_edit_singleline(note);
                                !note.trim().is_empty()
                            }
                            LibraryAction::Release { tag, notes, .. } => {
                                tw::label(ui, "Tag", "The release's name (R2, 2026-12…)");
                                ui.text_edit_singleline(tag);
                                tw::label(ui, "Notes", "What changed");
                                ui.text_edit_singleline(notes);
                                !tag.trim().is_empty()
                            }
                            LibraryAction::Grant { note, .. }
                            | LibraryAction::Withdraw { note, .. }
                            | LibraryAction::Lock { note, .. } => {
                                tw::label(ui, "Note", "Kept in the audit");
                                ui.text_edit_singleline(note);
                                true
                            }
                            LibraryAction::Restore { .. } => true,
                        };
                        let label = match &a {
                            LibraryAction::Lock { locked: true, .. } => "Lock",
                            LibraryAction::Lock { .. } => "Unlock",
                            LibraryAction::Grant { .. } => "Grant",
                            LibraryAction::Refuse { .. } => "Refuse",
                            LibraryAction::Release { .. } => "Release",
                            LibraryAction::Withdraw { .. } => "Withdraw",
                            LibraryAction::Restore { .. } => "Restore",
                        };
                        match tw::buttons(
                            ui,
                            label,
                            ok,
                            Some("Fill in what is asked"),
                            tw::Keys::Escape,
                        ) {
                            Some(tw::Pressed::Action) => {
                                c.library(a.clone());
                                close = true;
                            }
                            Some(tw::Pressed::Cancel) => close = true,
                            None => {}
                        }
                    });
                keep = Some(Window::Library(a));
            }
            Window::Pins(line) => {
                if c.data.pins.as_ref().is_none_or(|p| p.line != line) && !self.pages.pins_asked {
                    c.ask_pins(&line);
                    self.pages.pins_asked = true;
                }
                egui::Window::new("Update library pins")
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        tw::intro(
                            ui,
                            &format!("Line {line}'s library files moved to the latest releases."),
                            "A proposal first: what changes, nothing saved. Carried out, it is one \
                             logged command of the line; the routes using what changed are marked \
                             out of date.",
                            "pins",
                            help("libraries"),
                        );
                        let plan = c.data.pins.as_ref().filter(|p| p.line == line);
                        match plan {
                            None => {
                                ui.spinner();
                            }
                            Some(p) => {
                                egui::ScrollArea::vertical()
                                    .max_height(240.0)
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(&p.text).monospace());
                                    });
                                if !p.moves {
                                    ui.label("Nothing to update.");
                                }
                                if let Some(why) = &p.refusal {
                                    ohl_ui::messages::error_label(
                                        ui,
                                        egui::RichText::new(format!("You may not: {why}")),
                                    );
                                }
                            }
                        }
                        let ok = plan.is_some_and(|p| p.moves && p.refusal.is_none());
                        match tw::buttons(
                            ui,
                            "Update the pins",
                            ok,
                            Some("Nothing to update, or not yours to update"),
                            tw::Keys::Escape,
                        ) {
                            Some(tw::Pressed::Action) => {
                                c.update_pins(&line);
                                close = true;
                            }
                            Some(tw::Pressed::Cancel) => close = true,
                            None => {}
                        }
                    });
            }
            Window::Import {
                path,
                mut code,
                mut name,
                mut tolerance,
            } => {
                egui::Window::new("Import a line")
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        tw::intro(
                            ui,
                            &format!(
                                "{} sent to the server and imported as a new line.",
                                path.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned())
                            ),
                            "Its current state comes in (its history stays in the file); ids another \
                             line has are renamed CODE-id; twin towers are listed, never merged. \
                             Network admins only; in the audit.",
                            "import",
                            help("lines"),
                        );
                        egui::Grid::new("import").num_columns(2).show(ui, |ui| {
                            tw::label(ui, "Code", "The new line's code (letters, digits, - and _)");
                            ui.text_edit_singleline(&mut code);
                            ui.end_row();
                            tw::label(ui, "Name", "Optional (default: the file's line's name)");
                            ui.text_edit_singleline(&mut name);
                            ui.end_row();
                            tw::label(ui, "Twins within", "m (default 0.5 m)");
                            ohl_ui::style::field(
                                ui,
                                egui::DragValue::new(&mut tolerance)
                                    .range(0.0..=50.0)
                                    .speed(0.1)
                                    .suffix(" m"),
                            );
                            ui.end_row();
                        });
                        let ok = !code.trim().is_empty();
                        match tw::buttons(ui, "Import", ok, Some("A code first"), tw::Keys::Escape) {
                            Some(tw::Pressed::Action) => {
                                let n = name.trim();
                                c.import_line(
                                    path.clone(),
                                    code.trim(),
                                    (!n.is_empty()).then(|| n.to_owned()),
                                    Some(tolerance),
                                );
                                close = true;
                            }
                            Some(tw::Pressed::Cancel) => close = true,
                            None => {}
                        }
                    });
                keep = Some(Window::Import {
                    path,
                    code,
                    name,
                    tolerance,
                });
            }
            Window::Setting { key, mut value } => {
                egui::Window::new("Change a setting")
                    .collapsible(false)
                    .open(&mut open)
                    .show(ctx, |ui| {
                        tw::intro(
                            ui,
                            &format!("The network's {key} changed."),
                            "A network setting (ohl network setting): kept in the database, in the \
                             audit. Network admins only.",
                            "setting",
                            help("settings"),
                        );
                        tw::label(ui, "Value", "Its new value");
                        ui.text_edit_singleline(&mut value);
                        let ok = !value.trim().is_empty();
                        match tw::buttons(ui, "Change", ok, Some("A value first"), tw::Keys::Escape)
                        {
                            Some(tw::Pressed::Action) => {
                                c.set_setting(&key, value.trim());
                                close = true;
                            }
                            Some(tw::Pressed::Cancel) => close = true,
                            None => {}
                        }
                    });
                keep = Some(Window::Setting { key, value });
            }
            _ => {}
        }
        if close || !open {
            self.window = None;
            self.pages.pins_asked = false;
        } else if let Some(k) = keep {
            self.window = Some(k);
        }
        true
    }
}
