// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Design › Sections & materials › **Section library**: the sections of
//! the section library the structure reads (`--sections`, else
//! `Sections/` beside the file; started from the line application, the
//! project line's library, which it keeps in the project when a file
//! there changes) typed here — new, edit, duplicate, delete — with their
//! properties computed as they are typed (by the section library itself)
//! and what is wrong said. Each change is written to its `.ohlsec` file
//! at once and is one step of the window's undo and redo, in order with
//! the structure file's edits ([`LibraryEdit::at`]). Members pick the
//! sections in Design › Member sections and Groups.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use ohl_sections::{SECTION_FILE_EXTENSION, Section, SectionLibrary};
use ohl_structure_edit::section_library::{
    self as edit, MATERIAL_KEYS, MaterialDraft, SHAPES, SectionDraft,
};
use toml_edit::DocumentMut;

use crate::app::{StructureApp, error_label};

/// How often the library folder is looked at again (files written by the
/// line application, another window).
const RESCAN: Duration = Duration::from_secs(1);

/// File name of the library file new sections go into when the folder
/// has none.
pub const NEW_FILE: &str = "sections.ohlsec";

/// One change of a library file, for undo and redo.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEdit {
    /// In words.
    pub what: String,
    /// The file.
    pub path: PathBuf,
    /// Its text before (none: it did not exist).
    pub before: Option<String>,
    /// Its text after.
    pub after: String,
    /// How many structure-file edits could be undone when it was made (or
    /// undone): its place in the window's history.
    pub at: usize,
}

/// A section being typed: the file it goes to, the section it replaces
/// (none: a new one), the fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Editing {
    /// The library file.
    pub file: PathBuf,
    /// The section it replaces there.
    pub old: Option<String>,
    /// The fields as typed.
    pub draft: SectionDraft,
}

/// A file of the section library.
#[derive(Debug, Clone)]
pub struct LibraryFile {
    /// Where.
    pub path: PathBuf,
    /// Its text.
    pub text: String,
    /// What it reads as, or why not.
    pub read: Result<SectionLibrary, String>,
}

impl LibraryFile {
    fn doc(&self) -> Option<DocumentMut> {
        self.text.parse().ok()
    }

    /// The file's default material.
    fn default_material(&self) -> Option<String> {
        self.doc()?
            .get("default_material")?
            .as_str()
            .map(str::to_owned)
    }
}

/// The Section library sheet's state.
#[derive(Debug, Default)]
pub struct SectionEditor {
    /// The section being typed.
    pub editing: Option<Editing>,
    /// The file new sections go into (none: the first, else
    /// [`NEW_FILE`]).
    pub into: Option<PathBuf>,
    /// A deletion waiting for its confirmation: file and id.
    pub confirm_delete: Option<(PathBuf, String)>,
    /// The material being typed: its file, the material it replaces
    /// (none: a new one), the fields.
    pub material: Option<(PathBuf, Option<String>, MaterialDraft)>,
    /// A material's deletion waiting for its confirmation: file and id.
    pub confirm_material_delete: Option<(PathBuf, String)>,
    /// The files read, and when.
    files: Option<(Instant, PathBuf, Vec<LibraryFile>)>,
    undo: Vec<LibraryEdit>,
    redo: Vec<LibraryEdit>,
}

/// The `.ohlsec` files at `at` (a folder: its files in name order; a
/// file: itself).
pub fn library_files(at: &Path) -> Vec<LibraryFile> {
    let paths = if at.is_file() {
        vec![at.to_path_buf()]
    } else {
        ohl_core::files::files_with_extension(at, SECTION_FILE_EXTENSION)
    };
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let read = SectionLibrary::parse(&text);
            LibraryFile { path, text, read }
        })
        .collect()
}

/// A property row: name, value in its unit, what it is.
fn rows(s: &Section, density: Option<f64>) -> Vec<(&'static str, String, &'static str)> {
    let p = s.properties();
    let cm2 = |v: f64| format!("{:.3} cm²", v * 1e4);
    let cm4 = |v: f64| format!("{:.4} cm⁴", v * 1e8);
    let cm3 = |v: Option<f64>| v.map_or("—".to_owned(), |v| format!("{:.4} cm³", v * 1e6));
    let mm = |v: f64| format!("{:.2} mm", v * 1e3);
    let mut out = vec![
        ("A", cm2(p.area), "area"),
        ("Iy", cm4(p.iy), "second moment about y (along the width)"),
        ("Iz", cm4(p.iz), "second moment about z (along the height)"),
        ("Iu", cm4(p.iu), "major principal second moment"),
        ("Iv", cm4(p.iv), "minor principal second moment"),
        (
            "It",
            p.it.map_or("—".to_owned(), cm4),
            "torsion constant (see the cross-section library page for its approximations)",
        ),
        ("Wel,y", cm3(p.wel_y), "elastic modulus about y"),
        ("Wel,z", cm3(p.wel_z), "elastic modulus about z"),
        ("Wpl,y", cm3(p.wpl_y), "plastic modulus about y"),
        ("Wpl,z", cm3(p.wpl_z), "plastic modulus about z"),
        ("iy", mm(p.gyration_y), "radius of gyration about y"),
        ("iz", mm(p.gyration_z), "radius of gyration about z"),
        (
            "iv",
            mm(p.gyration_v),
            "radius of gyration about v (the smallest)",
        ),
        (
            "centroid",
            format!("{} / {}", mm(p.centroid[0]), mm(p.centroid[1])),
            "y / z from the lower left of the outline",
        ),
    ];
    out.push((
        "weight",
        density.map_or("— (no material density)".to_owned(), |rho| {
            format!("{:.3} kg/m", p.area * rho)
        }),
        "per metre: area × the material's density",
    ));
    out
}

impl SectionEditor {
    /// The library files at `at`, read again when a second passed or
    /// `at` changed.
    pub fn files(&mut self, at: &Path) -> &[LibraryFile] {
        let stale = self
            .files
            .as_ref()
            .is_none_or(|(when, p, _)| p != at || when.elapsed() >= RESCAN);
        if stale {
            self.files = Some((Instant::now(), at.to_path_buf(), library_files(at)));
        }
        self.files.as_ref().map_or(&[][..], |f| f.2.as_slice())
    }

    /// Read the files again at the next look.
    pub fn forget_files(&mut self) {
        self.files = None;
    }

    /// Whether the next undo is a library edit (`steps`: the structure
    /// file's undo steps now).
    pub fn undo_first(&self, steps: Option<usize>) -> bool {
        self.undo
            .last()
            .is_some_and(|e| steps.is_none_or(|s| e.at >= s))
    }

    /// Whether the next redo is a library edit (`steps`, `redo_steps`: the
    /// structure file's undo and redo steps now). A library edit no longer
    /// reachable (the structure file went another way) is dropped.
    pub fn redo_first(&mut self, steps: Option<usize>, redo_steps: usize) -> bool {
        let Some(e) = self.redo.last() else {
            return false;
        };
        let Some(u) = steps else {
            return true;
        };
        if u > e.at || u + redo_steps < e.at {
            self.redo.clear();
            return false;
        }
        u == e.at
    }

    /// What undo of the library would take back.
    pub fn undo_name(&self) -> Option<String> {
        self.undo
            .last()
            .map(|e| format!("cross-section library: {}", e.what))
    }

    /// What redo of the library would make again.
    pub fn redo_name(&self) -> Option<String> {
        self.redo
            .last()
            .map(|e| format!("cross-section library: {}", e.what))
    }
}

/// The section ids members may take: the section library's (looked up
/// first), then the structure file's own `[[section]]` tables.
pub fn section_choices(library: &SectionLibrary, doc: &DocumentMut) -> Vec<String> {
    let mut ids: Vec<String> = library.sections.iter().map(|s| s.id.clone()).collect();
    for id in ohl_structure_edit::item_ids(doc, "section") {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Writes `text` to `path` (its folder made).
fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

impl StructureApp {
    /// Where the section library is (a folder, or the file given).
    pub fn section_library_at(&self) -> Option<PathBuf> {
        Some(self.folders()?.sections)
    }

    /// The file new sections go into: the one chosen, else the first of
    /// the library, else [`NEW_FILE`] in its folder (made at the first
    /// save).
    pub fn section_target(&mut self) -> Option<PathBuf> {
        let at = self.section_library_at()?;
        let files: Vec<PathBuf> = self
            .section_editor
            .files(&at)
            .iter()
            .map(|f| f.path.clone())
            .collect();
        if let Some(p) = &self.section_editor.into
            && files.contains(p)
        {
            return Some(p.clone());
        }
        Some(files.into_iter().next().unwrap_or_else(|| {
            if at.is_file() {
                at.clone()
            } else {
                at.join(NEW_FILE)
            }
        }))
    }

    /// A library edit made: `path`'s text was `before`, is `after`.
    fn library_edited(&mut self, what: &str, path: &Path, before: Option<String>, after: String) {
        let at = self.file.as_ref().map_or(0, |f| f.undo_steps());
        let e = &mut self.section_editor;
        e.undo.push(LibraryEdit {
            what: what.to_owned(),
            path: path.to_path_buf(),
            before,
            after,
            at,
        });
        e.redo.clear();
        e.forget_files();
        self.reload_libraries();
    }

    /// The section being typed written into its file (one undo step); the
    /// library read again. What was done, or why not.
    pub fn save_section(&mut self, ed: &Editing) -> Result<String, String> {
        let check = self.check_draft(ed);
        let section = check.section.map_err(|e| format!("not saved: {e}"))?;
        let before = std::fs::read_to_string(&ed.file).ok();
        let mut doc: DocumentMut = match &before {
            Some(t) => t
                .parse()
                .map_err(|e| format!("{}: not TOML: {e}", ed.file.display()))?,
            None => edit::new_library_text("Project cross-sections")
                .parse()
                .map_err(|e| format!("{e}"))?,
        };
        edit::put_section(&mut doc, ed.old.as_deref(), &ed.draft)?;
        let after = doc.to_string();
        // The whole file must still read.
        SectionLibrary::parse(&after).map_err(|e| format!("not saved: {e}"))?;
        write(&ed.file, &after)?;
        let what = match &ed.old {
            Some(old) if *old != section.id => {
                format!("cross-section {old} renamed {}", section.id)
            }
            Some(_) => format!("section {}", section.id),
            None => format!("new cross-section {}", section.id),
        };
        self.library_edited(&what, &ed.file, before, after);
        Ok(format!(
            "{what} saved in {}",
            ed.file
                .file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned())
        ))
    }

    /// Section `id` removed from library file `path` (one undo step).
    pub fn delete_section(&mut self, path: &Path, id: &str) -> Result<String, String> {
        let before =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut doc: DocumentMut = before
            .parse()
            .map_err(|e| format!("{}: not TOML: {e}", path.display()))?;
        edit::remove_section(&mut doc, id)?;
        let after = doc.to_string();
        write(path, &after)?;
        self.library_edited(
            &format!("delete cross-section {id}"),
            path,
            Some(before),
            after,
        );
        Ok(format!("cross-section {id} deleted"))
    }

    /// A copy of section `id` of library file `path` to type (a free id).
    pub fn duplicate_section(&mut self, path: &Path, id: &str) -> Option<Editing> {
        let at = self.section_library_at()?;
        let files = self.section_editor.files(&at);
        let file = files.iter().find(|f| f.path == path)?;
        let mut draft = SectionDraft::read(&file.doc()?, id)?;
        let taken: Vec<String> = files
            .iter()
            .filter_map(|f| f.read.as_ref().ok())
            .flat_map(|l| l.sections.iter().map(|s| s.id.clone()))
            .collect();
        draft.id = edit::copy_id(taken.iter().map(String::as_str), id);
        if !draft.name.is_empty() {
            draft.name = format!("{} (copy)", draft.name);
        }
        Some(Editing {
            file: path.to_path_buf(),
            old: None,
            draft,
        })
    }

    /// The draft checked: the section as the library reads it (or what
    /// is wrong), and warnings that do not stop a save.
    pub fn check_draft(&mut self, ed: &Editing) -> DraftCheck {
        let Some(at) = self.section_library_at() else {
            return DraftCheck {
                section: Err("no structure open".into()),
                warnings: Vec::new(),
                density: None,
            };
        };
        let files = self.section_editor.files(&at).to_vec();
        let file = files.iter().find(|f| f.path == ed.file);
        let base = file.and_then(LibraryFile::doc).and_then(|d| {
            ed.old
                .as_deref()
                .and_then(|o| edit::section_table_of(&d, o).cloned())
        });
        let mut section = ed.draft.section(base.as_ref());
        // An id used by another section of the library.
        if let Ok(s) = &section {
            for f in &files {
                let Ok(lib) = &f.read else { continue };
                let mine = f.path == ed.file;
                if lib
                    .sections
                    .iter()
                    .any(|o| o.id == s.id && !(mine && ed.old.as_deref() == Some(o.id.as_str())))
                {
                    section = Err(format!(
                        "id \"{}\" is taken by a cross-section of {}",
                        s.id,
                        f.path
                            .file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                    ));
                    break;
                }
            }
        }
        let material = if ed.draft.material.trim().is_empty() {
            file.and_then(LibraryFile::default_material)
        } else {
            Some(ed.draft.material.trim().to_owned())
        };
        let mut warnings = Vec::new();
        let lib = &self.loads.sections;
        let density = match &material {
            None => {
                warnings.push(
                    "no material (and the file has no default_material): the finite-element \
                     model refuses members of this cross-section"
                        .to_owned(),
                );
                None
            }
            Some(m) => match lib.material(m) {
                Some(mat) => Some(mat.density),
                None => {
                    warnings.push(format!(
                        "no material \"{m}\" in the cross-section library: add its [[material]] to a \
                         .ohlsec file of the library"
                    ));
                    None
                }
            },
        };
        DraftCheck {
            section,
            warnings,
            density,
        }
    }

    /// Undo of the last library edit: its file's text put back. Refused
    /// when the file changed since.
    pub(crate) fn undo_library(&mut self) -> Result<String, String> {
        let e = self.section_editor.undo.pop().ok_or("nothing to undo")?;
        let now = std::fs::read_to_string(&e.path).ok();
        if now.as_deref() != Some(e.after.as_str()) {
            return Err(format!(
                "{} changed since ({}): not undone",
                e.path.display(),
                e.what
            ));
        }
        match &e.before {
            Some(t) => write(&e.path, t)?,
            None => {
                std::fs::remove_file(&e.path).map_err(|x| format!("{}: {x}", e.path.display()))?
            }
        }
        let what = e.what.clone();
        let at = self.file.as_ref().map_or(0, |f| f.undo_steps());
        self.section_editor.redo.push(LibraryEdit { at, ..e });
        self.section_editor.editing = None;
        self.section_editor.forget_files();
        self.reload_libraries();
        Ok(format!("undone: cross-section library: {what}"))
    }

    /// Redo of the last library edit undone.
    pub(crate) fn redo_library(&mut self) -> Result<String, String> {
        let e = self.section_editor.redo.pop().ok_or("nothing to redo")?;
        let now = std::fs::read_to_string(&e.path).ok();
        if now != e.before {
            return Err(format!(
                "{} changed since ({}): not made again",
                e.path.display(),
                e.what
            ));
        }
        write(&e.path, &e.after)?;
        let what = e.what.clone();
        let at = self.file.as_ref().map_or(0, |f| f.undo_steps());
        self.section_editor.undo.push(LibraryEdit { at, ..e });
        self.section_editor.editing = None;
        self.section_editor.forget_files();
        self.reload_libraries();
        Ok(format!("made again: cross-section library: {what}"))
    }

    /// Design › Sections & materials › Section library.
    pub(crate) fn section_library_sheet(&mut self, ui: &mut egui::Ui) {
        let Some(at) = self.section_library_at() else {
            return;
        };
        let files = self.section_editor.files(&at).to_vec();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Cross-section library: {}", at.display()))
                .on_hover_text(
                    "The library the structure's members take their cross-sections from \
                     (--sections, else Sections/ beside the file; started from the line \
                     application, the project line's library). Changes are written at once \
                     and undone with Edit › Undo.",
                );
        });
        self.wind_summary(ui);
        for f in &files {
            if let Err(e) = &f.read {
                error_label(ui, format!("{}: {e}", f.path.display()));
            }
        }
        if let Some(target) = self.section_target() {
            ui.horizontal(|ui| {
                if ui
                    .button("+ New cross-section")
                    .on_hover_text("Type a new cross-section of the library")
                    .clicked()
                {
                    self.section_editor.editing = Some(Editing {
                        file: target.clone(),
                        old: None,
                        draft: SectionDraft::new("", SHAPES[0].name),
                    });
                }
                if files.len() > 1 {
                    ui.label("new cross-sections go into");
                    let mut chosen = target.clone();
                    egui::ComboBox::from_id_salt("section-into")
                        .selected_text(
                            chosen
                                .file_name()
                                .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                        )
                        .show_ui(ui, |ui| {
                            for f in &files {
                                let name = f
                                    .path
                                    .file_name()
                                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                                ui.selectable_value(&mut chosen, f.path.clone(), name);
                            }
                        });
                    if chosen != target {
                        self.section_editor.into = Some(chosen);
                    }
                } else if files.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "no library file yet: the first cross-section makes {}",
                            target.display()
                        ))
                        .weak(),
                    );
                }
            });
        }
        if let Some(ed) = self.section_editor.editing.clone() {
            egui::Frame::group(ui.style()).show(ui, |ui| self.section_form(ui, ed));
        }
        self.section_list(ui, &files);
        ui.separator();
        self.material_list(ui, &files);
    }

    /// The section library's wind factors, in one line.
    fn wind_summary(&self, ui: &mut egui::Ui) {
        let text = match &self.loads.sections.wind {
            None => "Wind factors for structures: none given — the member wind takes the \
                     model's generic drag factor and no shielding"
                .to_owned(),
            Some(w) => format!(
                "Wind factors for structures from {}: drag for {} shape(s){}{}",
                w.source_name(),
                w.drag.len(),
                if w.face_drag.is_some() {
                    ", face drag by solidity"
                } else {
                    ""
                },
                if w.shielding.is_some() {
                    ", shielding"
                } else {
                    ""
                }
            ),
        };
        ui.label(egui::RichText::new(text).weak()).on_hover_text(
            "The [wind] table of the cross-section library: from a design-code plugin \
             (Load cases › Load cases from a design code…, files written) or typed in a \
             .ohlsec file. Used by the finite-element check's member wind in the line \
             application and by ohl structures fe.",
        );
    }

    /// The form of the section being typed.
    fn section_form(&mut self, ui: &mut egui::Ui, mut ed: Editing) {
        ui.strong(match &ed.old {
            Some(old) => format!("Cross-section {old}"),
            None => "New cross-section".to_owned(),
        });
        let materials: Vec<String> = self
            .loads
            .sections
            .materials
            .iter()
            .map(|m| m.id.clone())
            .collect();
        egui::Grid::new("section-form")
            .num_columns(2)
            .show(ui, |ui| {
                let text = form_field;
                text(
                    ui,
                    "id",
                    "What members name: letters, digits, - _ .",
                    &mut ed.draft.id,
                    "e.g. L-60x6",
                );
                text(ui, "name", "Display name", &mut ed.draft.name, "");
                text(
                    ui,
                    "description",
                    "Free text",
                    &mut ed.draft.description,
                    "",
                );
                ui.label("shape")
                    .on_hover_text("The shapes the cross-section library computes");
                let title = edit::shape_kind(&ed.draft.shape)
                    .map_or(ed.draft.shape.clone(), |s| s.title.to_owned());
                egui::ComboBox::from_id_salt("section-shape")
                    .selected_text(title)
                    .show_ui(ui, |ui| {
                        for s in SHAPES {
                            ui.selectable_value(&mut ed.draft.shape, s.name.to_owned(), s.title);
                        }
                    });
                ui.end_row();
                ui.label("material").on_hover_text(
                    "A material of the cross-section library (empty: the file's default_material)",
                );
                egui::ComboBox::from_id_salt("section-material")
                    .selected_text(if ed.draft.material.is_empty() {
                        "(the file's default)".to_owned()
                    } else {
                        ed.draft.material.clone()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut ed.draft.material,
                            String::new(),
                            "(the file's default)",
                        );
                        for m in &materials {
                            ui.selectable_value(&mut ed.draft.material, m.clone(), m);
                        }
                    });
                ui.end_row();
                if let Some(kind) = edit::shape_kind(&ed.draft.shape) {
                    for (k, what) in kind.dims {
                        let v = ed.draft.dims.entry((*k).to_owned()).or_default();
                        let hint = if *k == "sides" { "e.g. 8" } else { "e.g. 6 mm" };
                        text(ui, k, what, v, hint);
                    }
                    for (k, what) in kind.given {
                        let v = ed.draft.given.entry((*k).to_owned()).or_default();
                        text(ui, k, what, v, "with its unit");
                    }
                }
            });
        let check = self.check_draft(&ed);
        match &check.section {
            Ok(s) => {
                egui::CollapsingHeader::new("Properties (computed as typed)")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("section-properties")
                            .striped(true)
                            .show(ui, |ui| {
                                for (k, v, tip) in rows(s, check.density) {
                                    ui.label(k).on_hover_text(tip);
                                    ui.monospace(v);
                                    ui.end_row();
                                }
                            });
                    });
            }
            Err(e) => error_label(ui, e.as_str()),
        }
        for w in &check.warnings {
            ui.colored_label(ui.visuals().warn_fg_color, w);
        }
        let mut close = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(check.section.is_ok(), egui::Button::new("Save"))
                .on_hover_text("Write it into the library file (one undo step)")
                .on_disabled_hover_text("Fix what is said above first")
                .clicked()
            {
                let r = self.save_section(&ed);
                if r.is_ok() {
                    close = true;
                }
                self.message = Some(match r {
                    Ok(m) => (m, false),
                    Err(e) => (e, true),
                });
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
            ui.label(
                egui::RichText::new(format!(
                    "into {}",
                    ed.file
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                ))
                .weak(),
            );
        });
        self.section_editor.editing = if close { None } else { Some(ed) };
    }

    /// The sections of the library, with their tools.
    fn section_list(&mut self, ui: &mut egui::Ui, files: &[LibraryFile]) {
        let used: Vec<String> = self
            .file
            .as_ref()
            .map(|f| {
                f.structure()
                    .members
                    .iter()
                    .filter_map(|m| m.section.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut action: Option<(PathBuf, String, char)> = None;
        egui::Grid::new("library-sections")
            .striped(true)
            .show(ui, |ui| {
                for h in [
                    "cross-section",
                    "shape",
                    "name",
                    "material",
                    "A",
                    "file",
                    "used",
                    "",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for f in files {
                    let Ok(lib) = &f.read else { continue };
                    let file = f
                        .path
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    for s in &lib.sections {
                        ui.label(&s.id);
                        ui.label(s.shape.as_str());
                        ui.label(&s.name);
                        ui.label(s.material.as_deref().unwrap_or("—"));
                        ui.label(format!("{:.2} cm²", s.properties().area * 1e4));
                        ui.label(&file);
                        let n = used.iter().filter(|u| **u == s.id).count();
                        ui.label(if n == 0 {
                            String::new()
                        } else {
                            format!("{n} member(s)")
                        });
                        ui.horizontal(|ui| {
                            if ui.small_button("✏").on_hover_text("Edit").clicked() {
                                action = Some((f.path.clone(), s.id.clone(), 'e'));
                            }
                            if ui
                                .small_button("copy")
                                .on_hover_text("Duplicate: a copy to type")
                                .clicked()
                            {
                                action = Some((f.path.clone(), s.id.clone(), 'c'));
                            }
                            if ui.small_button("🗑").on_hover_text("Delete…").clicked() {
                                action = Some((f.path.clone(), s.id.clone(), 'd'));
                            }
                        });
                        ui.end_row();
                    }
                }
            });
        if files
            .iter()
            .all(|f| f.read.as_ref().map_or(true, |l| l.sections.is_empty()))
        {
            ui.label(
                egui::RichText::new("No cross-section in the library yet: + New cross-section.")
                    .weak(),
            );
        }
        match action {
            Some((path, id, 'e')) => {
                let draft = files
                    .iter()
                    .find(|f| f.path == path)
                    .and_then(LibraryFile::doc)
                    .and_then(|d| SectionDraft::read(&d, &id));
                if let Some(draft) = draft {
                    self.section_editor.editing = Some(Editing {
                        file: path,
                        old: Some(id),
                        draft,
                    });
                }
            }
            Some((path, id, 'c')) => {
                self.section_editor.editing = self.duplicate_section(&path, &id);
            }
            Some((path, id, _)) => self.section_editor.confirm_delete = Some((path, id)),
            None => {}
        }
        if let Some((path, id)) = self.section_editor.confirm_delete.clone() {
            let n = used.iter().filter(|u| **u == id).count();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(if n > 0 {
                    format!(
                        "Delete cross-section {id}? {n} member(s) of this structure use it (and maybe \
                         other structures of the line): they would have no cross-section."
                    )
                } else {
                    format!("Delete cross-section {id} from the library?")
                });
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        self.message = Some(match self.delete_section(&path, &id) {
                            Ok(m) => (m, false),
                            Err(e) => (e, true),
                        });
                        self.section_editor.confirm_delete = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.section_editor.confirm_delete = None;
                    }
                });
            });
        }
    }
}

impl StructureApp {
    /// The material being typed written into its file (one undo step);
    /// the library read again. What was done, or why not.
    pub fn save_material(
        &mut self,
        path: &Path,
        old: Option<&str>,
        draft: &MaterialDraft,
    ) -> Result<String, String> {
        let before = std::fs::read_to_string(path).ok();
        let mut doc: DocumentMut = match &before {
            Some(t) => t
                .parse()
                .map_err(|e| format!("{}: not TOML: {e}", path.display()))?,
            None => edit::new_library_text("Project cross-sections")
                .parse()
                .map_err(|e| format!("{e}"))?,
        };
        let base = old.and_then(|o| edit::material_table_of(&doc, o)).cloned();
        draft
            .material(base.as_ref())
            .map_err(|e| format!("not saved: {e}"))?;
        edit::put_material(&mut doc, old, draft)?;
        let after = doc.to_string();
        SectionLibrary::parse(&after).map_err(|e| format!("not saved: {e}"))?;
        write(path, &after)?;
        let id = draft.id.trim();
        let what = match old {
            Some(o) if o != id => format!("material {o} renamed {id}"),
            Some(_) => format!("material {id}"),
            None => format!("new material {id}"),
        };
        self.library_edited(&what, path, before, after);
        Ok(format!("{what} saved"))
    }

    /// Material `id` removed from library file `path` (one undo step);
    /// refused while a section of the file names it.
    pub fn delete_material(&mut self, path: &Path, id: &str) -> Result<String, String> {
        let before =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut doc: DocumentMut = before
            .parse()
            .map_err(|e| format!("{}: not TOML: {e}", path.display()))?;
        edit::remove_material(&mut doc, id)?;
        let after = doc.to_string();
        write(path, &after)?;
        self.library_edited(&format!("delete material {id}"), path, Some(before), after);
        Ok(format!("material {id} deleted"))
    }

    /// The materials of the library (roadmap U5, 2026-10-25): listed,
    /// typed (new, edit, duplicate, delete), each change one undo step.
    fn material_list(&mut self, ui: &mut egui::Ui, files: &[LibraryFile]) {
        ui.horizontal(|ui| {
            ui.strong("Materials");
            if let Some(target) = self.section_target()
                && ui
                    .button("+ New material")
                    .on_hover_text("Type a new material of the library")
                    .clicked()
            {
                self.section_editor.material = Some((target, None, MaterialDraft::default()));
            }
        });
        let mut action: Option<(PathBuf, String, char)> = None;
        egui::Grid::new("library-materials")
            .striped(true)
            .show(ui, |ui| {
                for h in [
                    "material", "name", "E", "G", "density", "fy", "fu", "file", "",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for f in files {
                    let Ok(lib) = &f.read else { continue };
                    let file = f
                        .path
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    for m in &lib.materials {
                        ui.label(&m.id);
                        ui.label(&m.name);
                        ui.label(format!("{:.0} GPa", m.elasticity / 1e9));
                        ui.label(format!("{:.1} GPa", m.shear_modulus / 1e9));
                        ui.label(format!("{:.0} kg/m³", m.density));
                        let mpa = |v: Option<f64>| {
                            v.map_or("—".into(), |v| format!("{:.0} MPa", v / 1e6))
                        };
                        ui.label(mpa(m.yield_strength));
                        ui.label(mpa(m.ultimate));
                        ui.label(&file);
                        ui.horizontal(|ui| {
                            if ui.small_button("✏").on_hover_text("Edit").clicked() {
                                action = Some((f.path.clone(), m.id.clone(), 'e'));
                            }
                            if ui
                                .small_button("copy")
                                .on_hover_text("Duplicate: a copy to type")
                                .clicked()
                            {
                                action = Some((f.path.clone(), m.id.clone(), 'c'));
                            }
                            if ui.small_button("🗑").on_hover_text("Delete…").clicked() {
                                action = Some((f.path.clone(), m.id.clone(), 'd'));
                            }
                        });
                        ui.end_row();
                    }
                }
            });
        if files
            .iter()
            .all(|f| f.read.as_ref().map_or(true, |l| l.materials.is_empty()))
        {
            ui.label(
                egui::RichText::new(
                    "No material in the library yet: + New material (the sections take their \
                     weight and stiffness from it).",
                )
                .weak(),
            );
        }
        let read = |path: &PathBuf, id: &str| {
            files
                .iter()
                .find(|f| &f.path == path)
                .and_then(LibraryFile::doc)
                .and_then(|d| MaterialDraft::read(&d, id))
        };
        match action {
            Some((path, id, 'e')) => {
                if let Some(d) = read(&path, &id) {
                    self.section_editor.material = Some((path, Some(id), d));
                }
            }
            Some((path, id, 'c')) => {
                if let Some(mut d) = read(&path, &id) {
                    let taken: Vec<String> = files
                        .iter()
                        .filter_map(|f| f.read.as_ref().ok())
                        .flat_map(|l| l.materials.iter().map(|m| m.id.clone()))
                        .collect();
                    d.id = edit::copy_id(taken.iter().map(String::as_str), &id);
                    if !d.name.is_empty() {
                        d.name = format!("{} (copy)", d.name);
                    }
                    self.section_editor.material = Some((path, None, d));
                }
            }
            Some((path, id, _)) => self.section_editor.confirm_material_delete = Some((path, id)),
            None => {}
        }
        if let Some((path, id)) = self.section_editor.confirm_material_delete.clone() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(format!("Delete material {id} from the library?"));
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        self.message = Some(match self.delete_material(&path, &id) {
                            Ok(m) => (m, false),
                            Err(e) => (e, true),
                        });
                        self.section_editor.confirm_material_delete = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.section_editor.confirm_material_delete = None;
                    }
                });
            });
        }
        if let Some((path, old, mut d)) = self.section_editor.material.clone() {
            let mut close = false;
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong(match &old {
                    Some(o) => format!("Material {o}"),
                    None => "New material".to_owned(),
                });
                egui::Grid::new("material-form")
                    .num_columns(2)
                    .show(ui, |ui| {
                        form_field(
                            ui,
                            "id",
                            "What sections name: letters, digits, - _ .",
                            &mut d.id,
                            "e.g. S355",
                        );
                        form_field(ui, "name", "Display name", &mut d.name, "");
                        form_field(ui, "description", "Free text", &mut d.description, "");
                        for (k, what) in MATERIAL_KEYS {
                            let v = d.values.entry(k.to_owned()).or_default();
                            form_field(ui, k, what, v, "with its unit");
                        }
                        form_field(
                            ui,
                            "partial factors",
                            "As a code check names them: name = value, separated by commas",
                            &mut d.partial_factors,
                            "e.g. gamma_m0 = 1.0",
                        );
                    });
                let base = old.as_ref().and_then(|o| {
                    files
                        .iter()
                        .find(|f| f.path == path)
                        .and_then(LibraryFile::doc)
                        .and_then(|doc| edit::material_table_of(&doc, o).cloned())
                });
                let check = d.material(base.as_ref());
                if let Err(e) = &check {
                    error_label(ui, e.as_str());
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(check.is_ok(), egui::Button::new("Save"))
                        .on_hover_text("Write it into the library file (one undo step)")
                        .on_disabled_hover_text("Fix what is said above first")
                        .clicked()
                    {
                        let r = self.save_material(&path, old.as_deref(), &d);
                        close = r.is_ok();
                        self.message = Some(match r {
                            Ok(m) => (m, false),
                            Err(e) => (e, true),
                        });
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
            self.section_editor.material = if close { None } else { Some((path, old, d)) };
        }
    }
}

/// A row of the form: label (with what it is), the field typed.
fn form_field(ui: &mut egui::Ui, label: &str, tip: &str, v: &mut String, hint: &str) {
    ui.label(label).on_hover_text(tip);
    ui.add(
        egui::TextEdit::singleline(v)
            .desired_width(200.0)
            .hint_text(hint),
    );
    ui.end_row();
}

/// A draft checked ([`StructureApp::check_draft`]).
#[derive(Debug, Clone)]
pub struct DraftCheck {
    /// The section as the library reads it, or what is wrong.
    pub section: Result<Section, String>,
    /// What does not stop a save (a material missing).
    pub warnings: Vec<String>,
    /// The material's density, kg/m³.
    pub density: Option<f64>,
}
