// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Performed checks: the library of the checks saved on the
//! current project line ([`ohl_session::reports`]) — filtered by check,
//! user and status; each with whether the line changed since; opened back
//! into its check's window, renamed, noted, given a status, exported,
//! deleted (asked first); reports imported from files.

use std::collections::BTreeSet;

use eframe::egui;
use ohl_session::Command;
use ohl_session::reports::{self, CheckReport, ReportStatus};

use crate::model::Model;

/// A report's line state against now: its name, in words, unchanged.
type Since = (String, String, bool);

/// What the window asks of the application.
#[derive(Default)]
pub struct Out {
    /// A command to apply, with what it does.
    pub apply: Option<(String, Command)>,
    /// A report to open in its check's window.
    pub open: Option<CheckReport>,
}

/// The Performed checks window.
#[derive(Default)]
pub struct CheckLibraryWindow {
    /// To be shown (the app opens its page).
    pub open: bool,
    /// Rows containing this.
    pub filter: String,
    /// Only this check (empty: all).
    pub check: String,
    /// Only this status.
    pub status: Option<ReportStatus>,
    /// A report being renamed: (old name, new name).
    rename: Option<(String, String)>,
    /// A note being typed: (report, text).
    note: Option<(String, String)>,
    /// A deletion waiting for its confirmation.
    delete: Option<String>,
    /// A report read from a file whose name exists: replace it?
    import: Option<CheckReport>,
    /// What the last action did, or why it failed (an error).
    message: Option<(String, bool)>,
    /// Each report's line state against now, for this line version.
    since: Option<(u64, Vec<Since>)>,
}

impl CheckLibraryWindow {
    /// Opens the window.
    pub fn show(&mut self) {
        self.open = true;
        self.since = None;
    }

    /// The page (a dock page: floating first, it may be docked).
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Out {
        let mut out = Out::default();
        self.contents(ui, model, &mut out);
        if out.apply.is_some() {
            self.since = None;
        }
        out
    }

    fn contents(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        let editing = match &model.editing {
            Ok(e) => e,
            Err(e) => {
                ui.label(format!(
                    "Reports are kept in a project (with its line's branch): {e}"
                ));
                return;
            }
        };
        let session = &editing.session;
        let mut all = session.reports();
        all.sort_by(|a, b| b.time.cmp(&a.time));
        ui.label(
            egui::RichText::new(format!(
                "The checks saved on the project line \"{}\": kept with its branch (commits, \
                 merges, bundles), one per name; saving, a status, a note or a deletion is an \
                 edit (undo brings it back).",
                session.current_line()
            ))
            .small()
            .weak(),
        );
        // Each report's line state, computed again when the line changes.
        if self
            .since
            .as_ref()
            .is_none_or(|s| s.0 != model.line_version)
        {
            let now = session.check_state().ok();
            let list = all
                .iter()
                .map(|r| {
                    let (text, same) = match &now {
                        Some(now) => {
                            let c = reports::changed_since(&r.state, now);
                            (c.summary(), c.is_empty())
                        }
                        None => ("—".into(), false),
                    };
                    (r.id.clone(), text, same)
                })
                .collect();
            self.since = Some((model.line_version, list));
        }
        let since = self.since.clone().map(|s| s.1).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filter")
                    .desired_width(160.0),
            );
            let checks: BTreeSet<String> = all.iter().map(|r| r.title.clone()).collect();
            egui::ComboBox::from_id_salt("check-library-check")
                .selected_text(if self.check.is_empty() {
                    "every check"
                } else {
                    self.check.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.check, String::new(), "every check");
                    for c in checks {
                        ui.selectable_value(&mut self.check, c.clone(), c);
                    }
                });
            egui::ComboBox::from_id_salt("check-library-status")
                .selected_text(self.status.map_or("every status", ReportStatus::as_str))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.status, None, "every status");
                    for s in ReportStatus::ALL {
                        ui.selectable_value(&mut self.status, Some(s), s.as_str());
                    }
                });
            if ui
                .button("Import…")
                .on_hover_text("A report exported before (.ohlcr), onto this line")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Check report", &[reports::REPORT_FILE_EXTENSION])
                    .pick_file()
            {
                match std::fs::read_to_string(&p)
                    .map_err(|e| e.to_string())
                    .and_then(|t| CheckReport::parse(&t))
                {
                    Ok(r) if session.report(&r.id).is_some() => self.import = Some(r),
                    Ok(r) => out.apply = Some((format!("imported check report {}", r.id), r.put())),
                    Err(e) => self.message = Some((format!("{}: {e}", p.display()), true)),
                }
            }
        });
        if let Some(r) = self.import.clone() {
            ui.horizontal(|ui| {
                crate::menu::message(
                    ui,
                    format!("A report \"{}\" exists on this line: replace it?", r.id),
                    Some(ui.visuals().warn_fg_color),
                );
                if ui.button("Replace").clicked() {
                    out.apply = Some((format!("imported check report {}", r.id), r.put()));
                    self.import = None;
                }
                if ui.button("Cancel").clicked() {
                    self.import = None;
                }
            });
        }
        if let Some(id) = self.delete.clone() {
            ui.horizontal(|ui| {
                crate::menu::message(
                    ui,
                    format!(
                        "Delete the report \"{id}\"? It takes nothing else with it (undo brings \
                         it back)."
                    ),
                    Some(ui.visuals().warn_fg_color),
                );
                if ui.button("Delete").clicked() {
                    out.apply = Some((format!("deleted check report {id}"), reports::delete(&id)));
                    self.delete = None;
                }
                if ui.button("Cancel").clicked() {
                    self.delete = None;
                }
            });
        }
        if let Some((m, error)) = &self.message {
            if *error {
                crate::menu::error_label(ui, m);
            } else {
                ui.colored_label(crate::menu::ok_color(ui), m);
            }
        }
        let needle = self.filter.trim().to_lowercase();
        let shown: Vec<&CheckReport> = all
            .iter()
            .filter(|r| self.check.is_empty() || r.title == self.check)
            .filter(|r| self.status.is_none_or(|s| r.status == s))
            .filter(|r| {
                needle.is_empty()
                    || [&r.id, &r.title, &r.user, &r.note, &r.summary]
                        .iter()
                        .any(|t| t.to_lowercase().contains(&needle))
            })
            .collect();
        if all.is_empty() {
            ui.label("No check saved on this line yet: a check's window saves its run (Save).");
            return;
        }
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("check-library-rows")
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for h in [
                            "", "name", "check", "when", "by", "status", "line", "result", "note",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in shown {
                            self.row(ui, r, &since, out);
                            ui.end_row();
                        }
                    });
            });
    }

    fn row(&mut self, ui: &mut egui::Ui, r: &CheckReport, since: &[Since], out: &mut Out) {
        ui.horizontal(|ui| {
            if ui
                .button("Open")
                .on_hover_text("In its check's window: its parameters and results")
                .clicked()
            {
                out.open = Some(r.clone());
            }
            if ui.button("Export…").clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Check report", &[reports::REPORT_FILE_EXTENSION])
                    .set_file_name(format!("{}.{}", r.id, reports::REPORT_FILE_EXTENSION))
                    .save_file()
            {
                self.message = Some(match std::fs::write(&p, r.to_text()) {
                    Ok(()) => (format!("exported to {}", p.display()), false),
                    Err(e) => (format!("{}: {e}", p.display()), true),
                });
            }
            if ui.button("Delete…").clicked() {
                self.delete = Some(r.id.clone());
            }
        });
        // Name (renamed in place).
        match &mut self.rename {
            Some((old, new)) if *old == r.id => {
                let e = ui.add(egui::TextEdit::singleline(new).desired_width(180.0));
                if e.lost_focus() {
                    let new = new.trim().to_owned();
                    if !new.is_empty() && new != r.id {
                        let mut renamed = r.clone();
                        renamed.id = new.clone();
                        out.apply = Some((
                            format!("renamed check report {} to {new}", r.id),
                            Command::Batch {
                                commands: vec![reports::delete(&r.id), renamed.put()],
                            },
                        ));
                    }
                    self.rename = None;
                }
            }
            _ => {
                if ui
                    .label(&r.id)
                    .on_hover_text("Double-click to rename")
                    .double_clicked()
                {
                    self.rename = Some((r.id.clone(), r.id.clone()));
                }
            }
        }
        ui.label(&r.title).on_hover_text(match &r.plugin {
            Some(p) => format!("{} — plugin {p}", r.app),
            None => r.app.clone(),
        });
        ui.label(&r.time);
        ui.label(&r.user);
        let mut status = r.status;
        egui::ComboBox::from_id_salt(("check-library-status-of", &r.id))
            .selected_text(status.as_str())
            .width(90.0)
            .show_ui(ui, |ui| {
                for s in ReportStatus::ALL {
                    ui.selectable_value(&mut status, s, s.as_str());
                }
            });
        if status != r.status {
            let mut changed = r.clone();
            changed.status = status;
            out.apply = Some((
                format!("check report {}: {}", r.id, status.as_str()),
                changed.put(),
            ));
        }
        let (text, same) = since
            .iter()
            .find(|s| s.0 == r.id)
            .map_or(("—".into(), false), |s| (s.1.clone(), s.2));
        ui.label(egui::RichText::new(&text).color(if same {
            crate::menu::ok_color(ui)
        } else {
            ui.visuals().warn_fg_color
        }));
        ui.label(&r.summary);
        // The note (typed in place, kept when the field is left).
        match &mut self.note {
            Some((id, text)) if *id == r.id => {
                let e = ui.add(egui::TextEdit::singleline(text).desired_width(200.0));
                if e.lost_focus() {
                    if *text != r.note {
                        let mut noted = r.clone();
                        noted.note = text.trim().to_owned();
                        out.apply = Some((format!("check report {}: note", r.id), noted.put()));
                    }
                    self.note = None;
                }
            }
            _ => {
                let shown = if r.note.is_empty() {
                    "—"
                } else {
                    r.note.as_str()
                };
                if ui
                    .label(shown)
                    .on_hover_text("Double-click to write a note")
                    .double_clicked()
                {
                    self.note = Some((r.id.clone(), r.note.clone()));
                }
            }
        }
    }
}
