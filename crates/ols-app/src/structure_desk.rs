// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A structure type file (`.olss`) opened on its own (`ols-cad
//! pylon.olss`, File › Open): the structure screen of the Structure types
//! page — its attachment points, strings, wire points and sets in tables
//! beside its pylon in 3D — the whole window, without a line. Its body and
//! design: Edit in the structure application…. The insulator assemblies
//! next to the file (and in an `Insulators` folder there) are those its
//! strings may take.

use std::path::{Path, PathBuf};

use eframe::egui;
use eframe::egui_wgpu;
use ols_structures::{InsulatorLibrary, LibraryItem};

use crate::library::{LibraryKind, LibraryPage, PageAction, PageContext};
use crate::model::Sources;

/// A structure type file opened on its own.
pub struct StructureDesk {
    /// The file.
    pub path: PathBuf,
    /// Its editor (the Structure types page, on this file).
    pub page: LibraryPage,
    sources: Sources,
    insulators: InsulatorLibrary,
    /// The structure application started on the file (Edit in the
    /// structure application…).
    pub apps: crate::structure_app::StructureApps,
}

impl StructureDesk {
    /// Opens `path`.
    pub fn open(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // Read once: a file that does not read says why now.
        ols_structures::StructureType::parse(path, &text).map_err(|e| e.to_string())?;
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let sources = Sources {
            file: path.to_path_buf(),
            structures: Some(dir.clone()),
            ..Sources::default()
        };
        let mut insulators = InsulatorLibrary::default();
        for d in [dir.clone(), dir.join("Insulators")] {
            if let Ok(lib) = InsulatorLibrary::load_dir(&d) {
                for a in lib.iter() {
                    let p = lib.path(&a.id).map(Path::to_path_buf).unwrap_or_default();
                    let _ = insulators.insert(&p, a.clone());
                }
            }
        }
        let mut page = LibraryPage::new(LibraryKind::StructureTypes, &sources);
        page.add_file(path);
        page.workspace.on = true;
        page.standalone = true;
        Ok(Self {
            path: path.to_path_buf(),
            page,
            sources,
            insulators,
            apps: Default::default(),
        })
    }

    /// Unsaved changes.
    pub fn dirty(&self) -> bool {
        self.page.dirty()
    }

    /// The window: the editor; ⌘S / Ctrl+S saves.
    pub fn ui(&mut self, ui: &mut egui::Ui, rs: Option<&egui_wgpu::RenderState>, ahead: &mut f64) {
        let save = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        if save && self.page.dirty() {
            match self.page.save() {
                Ok(p) => {
                    self.page.message = Some((format!("saved {}", p.display()), false));
                }
                Err(e) => self.page.message = Some((e, true)),
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Structure type, opened on its own");
                ui.separator();
                ui.label("ahead is at");
                ui.add(egui::DragValue::new(ahead).range(0.0..=359.0).suffix("°"))
                    .on_hover_text(
                        "Angles in plan around the structure turn clockwise seen from above; \
                         which one means ahead along the line (yours, for files opened on their \
                         own; a project has its own: Structures › Angles around structures)",
                    );
                ui.label(
                    egui::RichText::new(format!(
                        "right {:.0}° · back {:.0}° · left {:.0}°",
                        (*ahead + 90.0) % 360.0,
                        (*ahead + 180.0) % 360.0,
                        (*ahead + 270.0) % 360.0
                    ))
                    .weak(),
                );
            });
            ui.separator();
            let cx = PageContext {
                rs,
                line: None,
                insulators: Some(&self.insulators),
                ahead: *ahead,
            };
            // A file saved or opened from the page: this one is shown.
            let action = self.page.ui(ui, &self.sources, cx);
            if let Some(f) = &self.page.file {
                self.path.clone_from(f);
            }
            if let Some(PageAction::EditInStructureApp(path)) = action {
                self.edit_in_app(&path, *ahead);
            }
        });
        self.apps_frame(ui.ctx());
    }

    /// The structure application on `path` (the insulators: those beside
    /// the file, as here).
    pub fn edit_in_app(&mut self, path: &Path, ahead: f64) {
        let id = path
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        self.page.message = Some(match self.apps.launch(path, &id, None, ahead, None, None) {
            Ok(()) => (
                format!(
                    "{id} opened in the structure application: saved there, it is read again here"
                ),
                false,
            ),
            Err(e) => (e, true),
        });
    }

    /// Each frame: the file saved in the structure application read again
    /// (when there are no changes here); its reports shown.
    fn apps_frame(&mut self, ctx: &egui::Context) {
        if self.apps.active() {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            for c in self.apps.poll() {
                if let crate::structure_app::Change::Structure(path, id) = c
                    && path == self.path
                {
                    self.page.message = Some(if self.page.dirty() {
                        (
                            format!(
                                "{id} was saved in the structure application; this page has changes of its own (Discard to see the saved file)"
                            ),
                            true,
                        )
                    } else {
                        self.page.open(&path);
                        (
                            format!("{id} saved in the structure application: read again"),
                            false,
                        )
                    });
                }
            }
        }
        crate::structure_app::report_window(ctx, &mut self.apps);
    }
}
