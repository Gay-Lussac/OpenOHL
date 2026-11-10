// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Versions › Offline copy: a project line carried between copies of a
//! project as a bundle file — its revisions and their contents
//! ([`ohl_session::Session::export_bundle`],
//! [`ohl_session::Session::import_bundle`]). The file is the one
//! `ohl project bundle` writes and reads (the bundle as JSON), so both
//! exchange files.
//!
//! Both run at once, in the frame (they read and write the project's own
//! session): a bundle is the committed revisions of one line, read or
//! written in well under a second for a line of some hundred structures.
//! How long it took is said in the status line.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;
use ohl_session::versions::{Bundle, short};

use crate::app::Ready;
use crate::model::Model;

/// The file extension offered for bundles (any name is read).
pub const EXTENSION: &str = "json";

/// Versions › Offline copy › Import bundle: the bundle read, the name its
/// line gets, then what the import did.
pub struct ImportWindow {
    /// The file read.
    pub file: PathBuf,
    /// The bundle.
    pub bundle: Bundle,
    /// The project line it is imported as.
    pub as_line: String,
    /// What the import did, or why it was refused.
    pub report: Option<Result<String, String>>,
}

/// Writes the committed revisions of project line `line` to bundle file
/// `path`; in words what was written.
pub fn export_to(model: &Model, line: &str, path: &Path) -> Result<String, String> {
    let started = Instant::now();
    let s = model
        .session()
        .ok_or("a bundle is written from a project (.ohl)")?;
    let bundle = s.export_bundle(line).map_err(|e| e.to_string())?;
    let text = serde_json::to_string(&bundle).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = format!(
        "wrote {}: project line {line}, {} revision(s) up to {} in {:.0?}",
        path.display(),
        bundle.revisions.len(),
        short(&bundle.head),
        started.elapsed()
    );
    if s.has_uncommitted(line).unwrap_or(false) {
        out.push_str("; its uncommitted changes are not in it (commit first to carry them)");
    }
    Ok(out)
}

/// Reads bundle file `path`.
pub fn read(path: &Path) -> Result<Bundle, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: not a bundle ({e})", path.display()))
}

/// The name a bundle's line is imported as by default: its own, or — when
/// that is the line worked on, which an import never moves — a free one.
pub fn default_name(model: &Model, bundle: &Bundle) -> String {
    let Some(s) = model.session() else {
        return bundle.branch.clone();
    };
    if s.current_line() != bundle.branch {
        return bundle.branch.clone();
    }
    let names: Vec<String> = s
        .refs()
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.name)
        .collect();
    (1..)
        .map(|k| format!("{} (imported {k})", bundle.branch))
        .find(|n| !names.contains(n))
        .expect("a free name")
}

/// Imports `bundle` as project line `as_line` (a new line, or an existing
/// one moved forward); in words what it did.
pub fn import(r: &mut Ready, bundle: &Bundle, as_line: &str) -> Result<String, String> {
    let started = Instant::now();
    let name = as_line.trim().to_owned();
    if r.model.remote.is_some() {
        return Err(
            "this project is on a server: a bundle is imported into the server's project file \
             (ohl project bundle), then everyone has it"
                .into(),
        );
    }
    let e = r.model.editing.as_mut().map_err(|why| why.clone())?;
    let existed = e.session.is_line(&name);
    let new = e
        .session
        .import_bundle(&e.user, bundle, Some(&name))
        .map_err(|e| e.to_string())?;
    // What is cached of the version state, and the line when it is shown
    // next to the one worked on, are read again.
    r.model.project_version += 1;
    r.model.refresh_lines(std::slice::from_ref(&name));
    r.view3d.dirty.insert(crate::scene::Layer::OtherLines);
    r.refresh_views();
    Ok(format!(
        "Project line {name} {} revision {} (the bundle of line {}).\n\
         {new} new revision(s) added to the project, {} already in it; read in {:.0?}.\n\n\
         The line you work on ({}) is not changed. To take its changes: Versions › Bring in \
         changes…, from {name}.",
        if existed {
            "moved forward to"
        } else {
            "made at"
        },
        short(&bundle.head),
        bundle.branch,
        bundle.revisions.len() - new,
        started.elapsed(),
        r.model.project_line(),
    ))
}

/// The entries of Versions › Offline copy.
pub fn menu(ui: &mut egui::Ui, r: &mut Ready, current: &str) {
    if ui
        .button("Export line as bundle…")
        .on_hover_text(format!(
            "Writes the committed revisions of {current} (the line you work on) and their \
             contents to one file, to carry to another copy of the project. Uncommitted changes \
             are not in it: commit first. The project is not changed."
        ))
        .clicked()
    {
        ui.close();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("bundle", &[EXTENSION])
            .set_file_name(format!("{current}.{EXTENSION}"))
            .save_file()
        {
            r.status = Some(match export_to(&r.model, current, &path) {
                Ok(m) => (m, false),
                Err(e) => (e, true),
            });
        }
    }
    let may = if r.model.remote.is_some() {
        Err(
            "On a server the bundle is imported into the server's project file (ohl project bundle)",
        )
    } else if r.model.can_edit().is_err() {
        Err("Read-only: designers and admins import")
    } else {
        Ok(())
    };
    if ui
        .add_enabled(may.is_ok(), egui::Button::new("Import bundle…"))
        .on_hover_text(
            "Reads a bundle file: its revisions are added to the project and its line is made \
             (or moved forward) under a name you choose. The line you work on is not changed: \
             bring its changes in afterwards.",
        )
        .on_disabled_hover_text(may.err().unwrap_or_default())
        .clicked()
    {
        ui.close();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("bundle", &[EXTENSION])
            .pick_file()
        {
            open(r, path);
        }
    }
}

/// Reads bundle file `path` and opens the import window on it.
pub fn open(r: &mut Ready, path: PathBuf) {
    match read(&path) {
        Ok(bundle) => {
            r.bundle_import = Some(Box::new(ImportWindow {
                as_line: default_name(&r.model, &bundle),
                file: path,
                bundle,
                report: None,
            }));
        }
        Err(e) => r.status = Some((e, true)),
    }
}

/// The Import bundle window, when open: what the bundle holds, the name
/// of its line, then the report in a box that scrolls.
pub fn window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut w) = r.bundle_import.take() else {
        return;
    };
    let mut open = true;
    let mut close = false;
    let mut go = false;
    // Back from a refusal to the name (another one may do).
    let mut back = false;
    egui::Window::new("Import bundle")
        .id(egui::Id::new("bundle-import-window"))
        .open(&mut open)
        .collapsible(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(format!(
                "{}: project line {} with {} revision(s), up to {}.",
                w.file.display(),
                w.bundle.branch,
                w.bundle.revisions.len(),
                short(&w.bundle.head)
            ));
            match &w.report {
                None => {
                    ui.horizontal(|ui| {
                        ui.label("as project line").on_hover_text(
                            "The line the bundle's revisions end on here. A new name makes a \
                             new line; the name of a line of this project moves that line \
                             forward (refused when it has revisions the bundle lacks or \
                             uncommitted changes, and for the line you work on).",
                        );
                        ui.text_edit_singleline(&mut w.as_line);
                    });
                    ui.horizontal(|ui| {
                        go = ui
                            .add_enabled(!w.as_line.trim().is_empty(), egui::Button::new("Import"))
                            .clicked();
                        close |= ui.button("Cancel").clicked();
                    });
                }
                Some(report) => {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("bundle-import-report")
                            .max_height(crate::menu::TEXT_BOX_HEIGHT)
                            .auto_shrink([false, true])
                            .show(ui, |ui| match report {
                                Ok(text) => {
                                    ui.add(egui::Label::new(text).wrap());
                                }
                                Err(e) => {
                                    crate::menu::error_label(ui, e.as_str());
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        back = report.is_err() && ui.button("Back").clicked();
                        close |= ui.button("Close").clicked();
                    });
                }
            }
        });
    if back {
        w.report = None;
    }
    if go {
        let name = w.as_line.clone();
        w.report = Some(import(r, &w.bundle, &name));
    }
    if open && !close {
        r.bundle_import = Some(w);
    }
}
