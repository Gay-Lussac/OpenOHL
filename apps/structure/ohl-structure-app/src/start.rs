// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The start page (OHL-Structure started without a file): **Open a
//! structure…**, the files opened recently (kept in the window's storage)
//! and **New structure…** — an empty level 1 structure (a stick figure
//! with one point), or one built from a pylon template (`.ohlpt`, the
//! line application's templates: `ohl_structures::templates`), written
//! where chosen and opened.

use std::path::{Path, PathBuf};

use eframe::egui;
use ohl_structures::LibraryItem;
use ohl_structures::templates::{PylonTemplate, Role};

use crate::app::{StructureApp, error_label};

/// Key of the recent files in the window's storage: the name from before
/// OpenOHL, kept so that the list is not lost.
pub const RECENT_KEY: &str = "ols-structure-recent"; // storage key: kept

/// How many recent files are kept.
const RECENT_MAX: usize = 10;

/// The example structure in the source tree (the example line's lattice
/// tower, invented values).
pub const EXAMPLE: &str = "examples/synthetic-line/Structures/type-02.ohls";

/// The hover text of "Open the example".
const EXAMPLE_TIP: &str = "A copy of the example structure (the example line's lattice tower, \
     type-02: invented values, level 3) opened from a temporary folder: change it freely, the \
     example itself stays as it is (File › Save as keeps it)";

/// The example structure on this computer, if it is there.
pub fn example_file() -> Option<PathBuf> {
    crate::help::shipped(EXAMPLE).filter(|p| p.is_file())
}

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
    /// The kind of an empty structure: `lattice`, `pole`, `portal` or
    /// `other`.
    pub kind: String,
    /// The id and name last given from the template chosen (replaced when
    /// another is chosen, unless typed over).
    auto: (String, String),
    /// The shipped example cross-sections and insulators copied beside it
    /// (into `Sections/` and `Insulators/`; a file there kept), to start
    /// from.
    pub examples: bool,
}

impl Default for NewStructure {
    fn default() -> Self {
        // The shipped folder, and its name before 2026-10-21.
        let templates = ohl_structures::templates::SHIPPED_FOLDERS
            .iter()
            .flat_map(|f| {
                ohl_core::files::files_with_extension(&ohl_core::files::shipped_library(f), "ohlpt")
            })
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
            kind: "lattice".into(),
            auto: ("new-structure".into(), "New structure".into()),
            examples: true,
        }
    }
}

/// The text of an empty level 1 structure `id` named `name` of `kind`
/// (`lattice`, `pole`, `portal`, `other`): one point where an insulator
/// hangs, 10 m up.
pub fn minimal(id: &str, name: &str, kind: &str) -> String {
    format!(
        "# A new structure (OHL-Structure): add points and strings (Geometry), then its \
         body (Body).\nformat = \"ohl-structure\"\nversion = 2\nid = {id:?}\nname = {name:?}\n\
         kind = {kind:?}\n\n[[point]]\nlabel = \"A1\"\npoint = [\"0 m\", \"0 m\", \"10 m\"]\n"
    )
}

/// The kinds of an empty structure offered: file word and words shown.
const KINDS: [(&str, &str); 4] = [
    ("lattice", "lattice tower"),
    ("pole", "pole"),
    ("portal", "portal"),
    ("other", "other"),
];

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
        Source::Empty => Ok(minimal(id, name, &n.kind)),
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
    ohl_structure_edit::StructureFile::open(path).map(|_| ())
}

/// The shipped example libraries copied beside a new structure in `dir`:
/// the cross-sections into `Sections/`, the insulator assemblies into
/// `Insulators/` (a file of the same name there kept).
pub fn copy_examples(dir: &Path) -> Result<(), String> {
    for (kind, into, ext) in [
        ("sections", "Sections", "ohlsec"),
        ("insulators", "Insulators", "ohli"),
    ] {
        let from = ohl_core::files::shipped_library(kind);
        let Ok(entries) = std::fs::read_dir(&from) else {
            continue;
        };
        let to = dir.join(into);
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some(ext) {
                continue;
            }
            let Some(name) = p.file_name() else {
                continue;
            };
            let target = to.join(name);
            if target.exists() {
                continue;
            }
            std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            std::fs::copy(&p, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

impl StructureApp {
    /// The start page: open, new, the example, plugins; the files opened
    /// recently (the last first; one not there any more marked, and
    /// removable).
    pub(crate) fn start_page(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<PathBuf> = None;
        let mut example = false;
        let mut forget: Option<PathBuf> = None;
        let big = egui::vec2(220.0, 34.0);
        ui.vertical_centered(|ui| {
            ui.add_space(48.0);
            // The OpenOHL mark above the name.
            ui.add(crate::icons::mark(ui.ctx(), 64.0));
            ui.heading("OHL-Structure");
            ui.label("A structure file (.ohls): its points and strings, its body, its design.");
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 2.0 * big.x - 12.0).max(0.0) / 2.0);
                if ui
                    .add(egui::Button::new("Open a structure…").min_size(big))
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Structure", &["ohls"])
                        .pick_file()
                {
                    open = Some(p);
                }
                if ui
                    .add(egui::Button::new("New structure…").min_size(big))
                    .on_hover_text(
                        "An empty level 1 structure, or one built from a structure template, \
                         written where you choose",
                    )
                    .clicked()
                {
                    self.new_structure = Some(NewStructure::default());
                }
            });
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 2.0 * big.x - 12.0).max(0.0) / 2.0);
                let there = example_file().is_some();
                if ui
                    .add_enabled(there, egui::Button::new("Open the example").min_size(big))
                    .on_hover_text(EXAMPLE_TIP)
                    .on_disabled_hover_text(format!(
                        "The example ({EXAMPLE}) is not on this computer"
                    ))
                    .clicked()
                {
                    example = true;
                }
                if ui
                    .add(egui::Button::new("Plugins…").min_size(big))
                    .on_hover_text("The plugin manager (shared with OHL-Design)")
                    .clicked()
                {
                    self.plugins.open_manager();
                }
            });
            if let Some(e) = &self.open_error {
                error_label(ui, e.as_str());
            }
            ui.add_space(20.0);
            if !self.recent.files.is_empty() {
                ui.strong("Recent files");
                egui::ScrollArea::vertical()
                    .id_salt("recent")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for f in &self.recent.files {
                            let name = f
                                .file_name()
                                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                            let folder = f
                                .parent()
                                .and_then(Path::file_name)
                                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                            let there = f.is_file();
                            ui.horizontal(|ui| {
                                ui.add_space((ui.available_width() - 2.0 * big.x).max(0.0) / 2.0);
                                let text = if there {
                                    egui::RichText::new(&name).strong()
                                } else {
                                    egui::RichText::new(format!("{name} (missing)")).weak()
                                };
                                let r = ui
                                    .add_enabled(
                                        there,
                                        egui::Button::new(text)
                                            .min_size(egui::vec2(big.x, 26.0))
                                            .right_text(egui::RichText::new(folder).weak().small()),
                                    )
                                    .on_hover_text(f.display().to_string())
                                    .on_disabled_hover_text(format!(
                                        "{}: not there any more",
                                        f.display()
                                    ));
                                if r.clicked() {
                                    open = Some(f.clone());
                                }
                                if ui
                                    .small_button("\u{00d7}")
                                    .on_hover_text("Remove it from this list (the file stays)")
                                    .clicked()
                                {
                                    forget = Some(f.clone());
                                }
                            });
                        }
                    });
            }
        });
        if let Some(f) = forget {
            self.recent.files.retain(|x| *x != f);
        }
        if example {
            self.open_example();
        }
        if let Some(p) = open {
            self.open_chosen(&p);
        }
    }

    /// Opens a copy of the example structure (in a new folder of the
    /// system's temporary folder: the example stays as it is; File › Save
    /// as keeps it elsewhere). Not added to the recent files.
    pub fn open_example(&mut self) {
        let Some(from) = example_file() else {
            self.open_error = Some(format!("the example ({EXAMPLE}) is not on this computer"));
            return;
        };
        let mut k = 1;
        let dir = loop {
            let d = std::env::temp_dir().join(format!("ohl-structure-example-{k}"));
            if !d.exists() {
                break d;
            }
            k += 1;
        };
        let to = dir.join(from.file_name().unwrap_or_default());
        let copied = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::copy(&from, &to))
            .map_err(|e| format!("{}: {e}", to.display()));
        if let Err(e) = copied {
            self.open_error = Some(e);
            return;
        }
        // Its insulator assemblies beside it, as in the example line.
        if let Some(lib) = from
            .parent()
            .and_then(Path::parent)
            .map(|d| d.join("Insulators"))
            && let Ok(entries) = std::fs::read_dir(&lib)
        {
            let into = dir.join("Insulators");
            let _ = std::fs::create_dir_all(&into);
            for e in entries.flatten() {
                let _ = std::fs::copy(e.path(), into.join(e.file_name()));
            }
        }
        self.open_chosen(&to);
        let canonical = std::fs::canonicalize(&to).unwrap_or(to);
        self.recent.files.retain(|f| *f != canonical);
        if self.file.is_some() {
            self.message = Some((
                "the example structure (a copy): invented values; File › Save as to keep it".into(),
                false,
            ));
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
                if matches!(n.source, Source::Empty) {
                    ui.horizontal(|ui| {
                        ui.label("kind");
                        for (k, words) in KINDS {
                            ui.radio_value(&mut n.kind, k.to_owned(), words);
                        }
                    });
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
                if ui.button("Another template file (.ohlpt)…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Structure template", &["ohlpt"])
                        .pick_file()
                {
                    chosen = Some(Source::Template(p, Role::Suspension));
                }
                if let Some(s) = chosen {
                    n.source = s;
                }
                // The id and name from the template and the role (unless
                // typed over): `delta-1c-suspension`, the template's name.
                if let Source::Template(p, role) = &n.source
                    && (n.id.trim(), n.name.trim()) == (n.auto.0.as_str(), n.auto.1.as_str())
                {
                    let (tid, tname) = n
                        .templates
                        .iter()
                        .find(|(q, ..)| q == p)
                        .map(|(_, i, m)| (i.clone(), m.clone()))
                        .unwrap_or_else(|| {
                            let stem = p
                                .file_stem()
                                .map_or(String::new(), |s| s.to_string_lossy().into_owned());
                            (stem.clone(), stem)
                        });
                    let role = match role {
                        Role::Suspension => "suspension",
                        Role::Strain => "strain",
                    };
                    n.auto = (format!("{tid}-{role}"), format!("{tname} ({role})"));
                    (n.id, n.name) = n.auto.clone();
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
                             application's structure builder sets others).",
                        )
                        .small()
                        .weak(),
                    );
                }
                ui.separator();
                ui.checkbox(
                    &mut n.examples,
                    "Copy the example cross-sections and insulators beside it (to start from)",
                )
                .on_hover_text(
                    "The ones shipped with OpenOHL, into Sections/ and Insulators/ beside the new \
                     file (a file there is kept); change or replace them later (File › Libraries)",
                );
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
                                .add_filter("Structure", &["ohls"])
                                .set_file_name(format!("{}.ohls", n.id.trim()))
                                .save_file()
                                .map(|p| p.with_extension("ohls"));
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
        if n.examples
            && let Some(dir) = path.parent()
        {
            copy_examples(dir)?;
        }
        self.open_chosen(path);
        self.message = Some((format!("new structure {}", path.display()), false));
        // The next step in front: the body (a lattice tower's generators,
        // a pole's outline).
        let kind = self.file.as_ref().map(|f| f.structure().kind);
        match kind {
            Some(ohl_structures::structures::StructureKind::Pole) => {
                self.focus(crate::layout::Panel::Pole);
            }
            Some(ohl_structures::structures::StructureKind::Lattice) => {
                self.focus(crate::layout::Panel::Generators);
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The examples copied beside a new structure: the cross-sections and
    /// the insulators, a file already there kept.
    #[test]
    fn the_examples_are_copied_beside_it() {
        let dir = std::env::temp_dir().join(format!("ohl-new-examples-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("Sections")).unwrap();
        let mine = dir.join("Sections/example-sections.ohlsec");
        std::fs::write(&mine, "# mine").unwrap();
        copy_examples(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), "# mine");
        assert!(dir.join("Insulators/example-suspension.ohli").is_file());
        std::fs::remove_file(&mine).unwrap();
        copy_examples(&dir).unwrap();
        assert!(std::fs::read_to_string(&mine).unwrap().contains("section"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Recent files: the last first, once, at most ten.
    #[test]
    fn recent_files() {
        let mut r = Recent::default();
        for k in 0..12 {
            r.add(Path::new(&format!("/nowhere/s{k}.ohls")));
        }
        r.add(Path::new("/nowhere/s5.ohls"));
        assert_eq!(r.files.len(), RECENT_MAX);
        assert_eq!(r.files[0], PathBuf::from("/nowhere/s5.ohls"));
        assert_eq!(r.files.iter().filter(|f| f.ends_with("s5.ohls")).count(), 1);
    }

    /// An empty structure and one of each shipped template read (level 1);
    /// a bad id is refused; a file there is not replaced.
    #[test]
    fn new_structures_read() {
        let dir = std::env::temp_dir().join(format!("ohl-structure-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut n = NewStructure::default();
        assert!(!n.templates.is_empty(), "the shipped templates");
        create(&dir.join("e.ohls"), &n).unwrap();
        assert!(
            create(&dir.join("e.ohls"), &n)
                .unwrap_err()
                .contains("exists")
        );
        let f = ohl_structure_edit::StructureFile::open(&dir.join("e.ohls")).unwrap();
        assert_eq!(f.structure().detail().level(), 1);
        for (k, (p, _, _)) in n.templates.clone().iter().enumerate() {
            n.source = Source::Template(p.clone(), Role::Suspension);
            n.id = format!("t{k}");
            create(&dir.join(format!("t{k}.ohls")), &n).unwrap();
        }
        n.id = "a b".into();
        assert!(text_of(&n).unwrap_err().contains("letters"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
