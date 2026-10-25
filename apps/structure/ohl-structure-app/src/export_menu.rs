// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! File › Export: the structure as IFC (IFC4 with the analysis model, IFC4
//! members, IFC2x3 for steel-detailing programs), SAF (analysis programs)
//! or CSV tables
//! (`ohl-structure-export`). By default with the window's section library
//! and its load table (the Load table panel: a placed structure's line
//! values and the values typed); another library or a load table (CSV)
//! can be chosen here.

use std::path::{Path, PathBuf};

use eframe::egui;
use ohl_structure_export::Format;
use ohl_structure_loads::LoadTable;

use crate::app::StructureApp;

/// What the exports take besides the file.
#[derive(Debug, Clone, Default)]
pub struct ExportSettings {
    /// Section library chosen (a file or folder); none: the window's
    /// (`--sections`, else `Sections/` beside the file).
    pub sections: Option<PathBuf>,
    /// Load table chosen (CSV); none: the Load table panel's table.
    pub loads: Option<PathBuf>,
}

impl StructureApp {
    /// The Export submenu of the File menu.
    pub fn export_menu(&mut self, ui: &mut egui::Ui) {
        let has = self.file.is_some();
        ui.menu_button("Export", |ui| {
            for f in Format::ALL {
                if ui
                    .add_enabled(has, egui::Button::new(f.description()))
                    .on_disabled_hover_text("Open a structure first")
                    .clicked()
                {
                    ui.close();
                    self.export_to(f);
                }
            }
            // File-format plugins write the line's export model, not a
            // structure: listed, not applicable (yet).
            for name in self.plugins.io_plugins() {
                ui.add_enabled(false, egui::Button::new(format!("{name} (plugin)")))
                    .on_disabled_hover_text(
                        "A file-format plugin: it takes the line's export model (its \
                         structures, wires and cases), not a structure — use it in the line \
                         application (File › Export)",
                    );
            }
            ui.separator();
            let sections = match &self.export.sections {
                Some(p) => p.display().to_string(),
                None => "the window's (as the load panels)".into(),
            };
            ui.label(format!("Cross-section library: {sections}"));
            ui.horizontal(|ui| {
                if ui.button("Choose library…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter(
                            "Cross-section library",
                            &ohl_core::files::open_extensions("ohlsec"),
                        )
                        .pick_file()
                {
                    self.export.sections = Some(p);
                }
                if self.export.sections.is_some() && ui.button("The window's").clicked() {
                    self.export.sections = None;
                }
            });
            let loads = match &self.export.loads {
                Some(p) => p.display().to_string(),
                None => "the Load table panel's table".into(),
            };
            ui.label(format!("Loads: {loads}"));
            ui.horizontal(|ui| {
                if ui.button("Choose load table…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Load table", &["csv"])
                        .pick_file()
                {
                    self.export.loads = Some(p);
                }
                if self.export.loads.is_some() && ui.button("The panel's table").clicked() {
                    self.export.loads = None;
                }
            });
        });
    }

    /// Asks where, then exports in `format`.
    fn export_to(&mut self, format: Format) {
        let stem = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(Path::file_stem)
            .map_or("structure".into(), |n| n.to_string_lossy().into_owned());
        let out = match format.extension() {
            Some(ext) => {
                let suffix = match format {
                    Format::Ifc2x3 => "-ifc2x3",
                    Format::IfcMembers => "-members",
                    _ => "",
                };
                rfd::FileDialog::new()
                    .add_filter(format.as_str(), &[ext])
                    .set_file_name(format!("{stem}{suffix}.{ext}"))
                    .save_file()
                    .map(|p| p.with_extension(ext))
            }
            None => rfd::FileDialog::new().pick_folder(),
        };
        if let Some(out) = out {
            self.message = Some(match self.export(format, &out) {
                Ok(m) => (m, false),
                Err(e) => (format!("export: {e}"), true),
            });
        }
    }

    /// Exports the structure as it is now (unsaved edits and typed loads
    /// included) in `format` to `out`; what was written, or why not.
    pub fn export(&mut self, format: Format, out: &Path) -> Result<String, String> {
        let f = self.file.as_ref().ok_or("no structure open")?;
        let library = match &self.export.sections {
            Some(p) => ohl_structure_export::load_sections(std::slice::from_ref(p))?.0,
            None => self.loads.sections.clone(),
        };
        let s = match &self.export.loads {
            Some(p) => {
                let table = std::fs::read_to_string(p)
                    .map_err(|e| e.to_string())
                    .and_then(|t| LoadTable::from_csv(&t))
                    .map_err(|e| format!("{}: {e}", p.display()))?;
                ohl_structure_export::structure(&f.text(), &library, Some(&table))?
            }
            None => {
                self.loads.refresh(f);
                let recipes = self.loads.recipes().map(<[_]>::to_vec).unwrap_or_default();
                ohl_structure_link::export_structure(
                    &f.text(),
                    &library,
                    Some(self.loads.table()),
                    &recipes,
                )?
            }
        };
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let written = ohl_structure_export::write(&s, format, out, time)?;
        let mut m = format!(
            "exported {} members, {} load cases to {}",
            s.members.len(),
            s.load_cases.len(),
            written
                .first()
                .and_then(|p| if format == Format::Csv {
                    p.parent()
                } else {
                    Some(p)
                })
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
        if !s.notes.is_empty() {
            m.push_str(&format!(" ({})", s.notes.join("; ")));
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example level 3 structure exported in every format from the
    /// window's state.
    #[test]
    fn every_format_from_the_window() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../examples/synthetic-line/Structures/type-02.ohls");
        let mut app = StructureApp::new(None);
        app.open(&file);
        assert!(app.file.is_some(), "{:?}", app.open_error);
        let dir =
            std::env::temp_dir().join(format!("ohl-structure-export-app-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in Format::ALL {
            let out = match f.extension() {
                Some(e) => dir.join(format!("{}.{e}", f.as_str())),
                None => dir.join("csv"),
            };
            let m = app.export(f, &out).unwrap();
            assert!(m.starts_with("exported 178 members, 5 load cases"), "{m}");
            assert!(out.exists());
        }
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(StructureApp::new(None).export(Format::Saf, &dir).is_err());
    }
}
