// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The start page (OHL-Structure started without a file): **Open a
//! structure…**, the files opened recently (kept in the window's storage)
//! and **New structure…** — an empty level 1 structure (a stick figure
//! with one point), or one built from a pylon template (`.olspt`, the
//! line application's templates: `ols_structures::templates`), written
//! where chosen and opened.

use std::path::{Path, PathBuf};

use eframe::egui;
use ols_structures::LibraryItem;
use ols_structures::templates::{PylonTemplate, Role};

use crate::app::{StructureApp, error_label};

/// Key of the recent files in the window's storage.
pub const RECENT_KEY: &str = "ols-structure-recent";

/// How many recent files are kept.
const RECENT_MAX: usize = 10;

/// The structure files opened recently, the last first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recent {
    /// The files.
    pub files: Vec<PathBuf>,
}

impl Recent {
    /// As kept in `storage` (none: none).
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        Self {
            files: storage
                .and_then(|s| eframe::get_value::<Vec<PathBuf>>(s, RECENT_KEY))
                .unwrap_or_default(),
        }
    }

    /// Kept in `storage`.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, RECENT_KEY, &self.files);
    }

    /// `path` opened: first in the list (once).
    pub fn add(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.files.retain(|f| *f != path);
        self.files.insert(0, path);
        self.files.truncate(RECENT_MAX);
    }
}

/// What a new structure is built from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// One point where an insulator hangs: a level 1 stick figure to start
    /// from.
    Empty,
    /// A pylon template (its file), built for a role with its default
    /// dimensions.
    Template(PathBuf, Role),
}

/// The New structure window.
#[derive(Debug, Clone, PartialEq)]
pub struct NewStructure {
    /// The structure's id (its file's name too).
    pub id: String,
    /// Its name.
    pub name: String,
    /// What it is built from.
    pub source: Source,
    /// The templates offered: file, id and name (the shipped ones).
    pub templates: Vec<(PathBuf, String, String)>,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

impl Default for NewStructure {
    fn default() -> Self {
        let dir = ols_core::files::shipped_library("pylon-templates");
        let templates = ols_core::files::files_with_extension(&dir, "olspt")
            .into_iter()
            .filter_map(|p| {
                let text = std::fs::read_to_string(&p).ok()?;
                let t = PylonTemplate::parse(&p, &text).ok()?;
                Some((p, t.id, t.name))
            })
            .collect();
        Self {
            id: "new-structure".into(),
            name: "New structure".into(),
            source: Source::Empty,
            templates,
            error: None,
        }
    }
}

/// The text of an empty level 1 structure `id` named `name`: one point
/// where an insulator hangs, 10 m up.
pub fn minimal(id: &str, name: &str) -> String {
    format!(
        "# A new structure (OHL-Structure): add points and insulators (Geometry), then its \
         body (Body).\nformat = \"ols-structure\"\nversion = 2\nid = {id:?}\nname = {name:?}\n\
         kind = \"other\"\n\n[[point]]\nlabel = \"A1\"\npoint = [\"0 m\", \"0 m\", \"10 m\"]\n"
    )
}

/// The text of the new structure `n`.
pub fn text_of(n: &NewStructure) -> Result<String, String> {
    let id = n.id.trim();
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if !ok {
        return Err(format!(
            "id \"{id}\": use letters, digits, - _ and . (it names the file too)"
        ));
    }
    let name = if n.name.trim().is_empty() {
        id
    } else {
        n.name.trim()
    };
    match &n.source {
        Source::Empty => Ok(minimal(id, name)),
        Source::Template(path, role) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let t = PylonTemplate::parse(path, &text).map_err(|e| e.to_string())?;
            t.build(&t.defaults(*role), *role, id, name)
                .map_err(|e| format!("template {}: {e}", t.id))
        }
    }
}

/// Writes the new structure `n` to `path` (a file there is not replaced)
/// and checks that it reads.
pub fn create(path: &Path, n: &NewStructure) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} exists already", path.display()));
    }
    let text = text_of(n)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    ols_structure_edit::StructureFile::open(path).map(|_| ())
}

impl StructureApp {
    /// The start page: open, recent files, new.
    pub(crate) fn start_page(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<PathBuf> = None;
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading("OHL-Structure");
            ui.label("A structure file (.olss): its points and insulators, its body, its design.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 320.0).max(0.0) / 2.0);
                if ui.button("Open a structure…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Structure", &["olss"])
                        .pick_file()
                {
                    open = Some(p);
                }
                if ui
                    .button("New structure…")
                    .on_hover_text(
                        "An empty level 1 structure, or one built from a pylon template, \
                         written where you choose",
                    )
                    .clicked()
                {
                    self.new_structure = Some(NewStructure::default());
                }
                if ui
                    .button("Plugins…")
                    .on_hover_text("The plugin manager (shared with OHL-Design)")
                    .clicked()
                {
                    self.plugins.open_manager();
                }
            });
            if let Some(e) = &self.open_error {
                error_label(ui, e.as_str());
            }
            ui.add_space(16.0);
            if !self.recent.files.is_empty() {
                ui.strong("Recent");
                egui::ScrollArea::vertical()
                    .id_salt("recent")
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for f in &self.recent.files {
                            let name = f
                                .file_name()
                                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                            let there = f.is_file();
                            let r = ui
                                .add_enabled(there, egui::Link::new(name))
                                .on_hover_text(f.display().to_string())
                                .on_disabled_hover_text(format!(
                                    "{}: not there any more",
                                    f.display()
                                ));
                            if r.clicked() {
                                open = Some(f.clone());
                            }
                        }
                    });
            }
        });
        if let Some(p) = open {
            self.open_chosen(&p);
        }
    }

    /// The New structure window: id, name, from (empty or a template),
    /// Create… (asks where).
    pub(crate) fn new_structure_window(&mut self, ctx: &egui::Context) {
        let Some(mut n) = self.new_structure.take() else {
            return;
        };
        let mut open = true;
        let mut create_at: Option<PathBuf> = None;
        egui::Window::new("New structure")
            .collapsible(false)
            .open(&mut open)
            .default_width(460.0)
            .show(ctx, |ui| {
                egui::Grid::new("new-structure")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("id");
                        ui.text_edit_singleline(&mut n.id);
                        ui.end_row();
                        ui.label("name");
                        ui.text_edit_singleline(&mut n.name);
                        ui.end_row();
                    });
                ui.separator();
                let empty = matches!(n.source, Source::Empty);
                if ui
                    .radio(empty, "Empty: one point (level 1), to build from")
                    .clicked()
                {
                    n.source = Source::Empty;
                }
                let mut chosen: Option<Source> = None;
                for (p, id, name) in &n.templates {
                    let on = matches!(&n.source, Source::Template(q, _) if q == p);
                    if ui
                        .radio(on, format!("Template {id}: {name}"))
                        .on_hover_text(p.display().to_string())
                        .clicked()
                    {
                        chosen = Some(Source::Template(p.clone(), Role::Suspension));
                    }
                }
                if ui.button("Another template file (.olspt)…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Pylon template", &["olspt"])
                        .pick_file()
                {
                    chosen = Some(Source::Template(p, Role::Suspension));
                }
                if let Some(s) = chosen {
                    n.source = s;
                }
                if let Source::Template(p, role) = &mut n.source {
                    ui.label(
                        egui::RichText::new(format!("template file {}", p.display()))
                            .small()
                            .weak(),
                    );
                    ui.horizontal(|ui| {
                        ui.label("built for");
                        ui.radio_value(role, Role::Suspension, "suspension");
                        ui.radio_value(role, Role::Strain, "strain");
                    });
                    ui.label(
                        egui::RichText::new(
                            "Built with the template's default dimensions (the line \
                             application's pylon builder sets others).",
                        )
                        .small()
                        .weak(),
                    );
                }
                ui.separator();
                if let Some(e) = &n.error {
                    error_label(ui, e.as_str());
                }
                if ui
                    .button("Create…")
                    .on_hover_text("Choose where to write it")
                    .clicked()
                {
                    match text_of(&n) {
                        Err(e) => n.error = Some(e),
                        Ok(_) => {
                            create_at = rfd::FileDialog::new()
                                .add_filter("Structure", &["olss"])
                                .set_file_name(format!("{}.olss", n.id.trim()))
                                .save_file()
                                .map(|p| p.with_extension("olss"));
                        }
                    }
                }
            });
        if let Some(p) = create_at {
            match self.create_structure(&p, &n) {
                Ok(()) => return,
                Err(e) => n.error = Some(e),
            }
        }
        if open {
            self.new_structure = Some(n);
        }
    }

    /// Writes the new structure `n` to `path` and opens it.
    pub fn create_structure(&mut self, path: &Path, n: &NewStructure) -> Result<(), String> {
        if self.dirty() {
            return Err("save or undo the changes of the structure open first".into());
        }
        create(path, n)?;
        self.open_chosen(path);
        self.message = Some((format!("new structure {}", path.display()), false));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recent files: the last first, once, at most ten.
    #[test]
    fn recent_files() {
        let mut r = Recent::default();
        for k in 0..12 {
            r.add(Path::new(&format!("/nowhere/s{k}.olss")));
        }
        r.add(Path::new("/nowhere/s5.olss"));
        assert_eq!(r.files.len(), RECENT_MAX);
        assert_eq!(r.files[0], PathBuf::from("/nowhere/s5.olss"));
        assert_eq!(r.files.iter().filter(|f| f.ends_with("s5.olss")).count(), 1);
    }

    /// An empty structure and one of each shipped template read (level 1);
    /// a bad id is refused; a file there is not replaced.
    #[test]
    fn new_structures_read() {
        let dir = std::env::temp_dir().join(format!("ols-structure-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut n = NewStructure::default();
        assert!(!n.templates.is_empty(), "the shipped templates");
        create(&dir.join("e.olss"), &n).unwrap();
        assert!(
            create(&dir.join("e.olss"), &n)
                .unwrap_err()
                .contains("exists")
        );
        let f = ols_structure_edit::StructureFile::open(&dir.join("e.olss")).unwrap();
        assert_eq!(f.structure().detail().level(), 1);
        for (k, (p, _, _)) in n.templates.clone().iter().enumerate() {
            n.source = Source::Template(p.clone(), Role::Suspension);
            n.id = format!("t{k}");
            create(&dir.join(format!("t{k}.olss")), &n).unwrap();
        }
        n.id = "a b".into();
        assert!(text_of(&n).unwrap_err().contains("letters"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
