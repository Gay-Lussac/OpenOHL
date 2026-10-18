// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The libraries of a structure (File › Libraries, the Libraries panel):
//! section libraries (`.ohlsec`), insulator assemblies (`.ohli`), poles
//! (`.ohlp`), structure models (`.ohlfe`), load-case recipes (`.ohlslc`)
//! and pylon templates (`.ohlpt`) — where the window reads them, what the
//! structure names that none has, **Import** (a file or a folder copied
//! into the folder the window reads, after each file is read and checked)
//! and **Export** (the libraries the structure uses, or every one, written
//! to a folder in the line application's layout: `structures/`,
//! `sections/`, `insulators/`, `pylon-templates/`).
//!
//! The folders follow the structure's: `Sections/` and `Insulators/`
//! beside the structure file, or those given (`--sections`,
//! `--insulators`; started from a project by the line application, the
//! project's working copy, which the line application stores back into
//! the project when files appear there); poles, structure models and
//! recipes beside the structure file (as the line application keeps
//! them, next to the structure types that name them); pylon templates in
//! `PylonTemplates/` beside it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eframe::egui;
use ohl_sections::{SECTION_FILE_EXTENSION, SectionLibrary};
use ohl_structure_loads::{STRUCTURE_LOAD_CASE_FILE_EXTENSION, StructureLoadCases};
use ohl_structures::insulators::INSULATOR_FILE_EXTENSION;
use ohl_structures::templates::{PylonTemplate, TEMPLATE_FILE_EXTENSION};
use ohl_structures::{InsulatorAssembly, LibraryItem, PoleRef, StructureType};

use crate::app::{StructureApp, error_label};

/// A kind of library a structure uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LibKind {
    /// Section libraries: the members' and the pole's sections.
    Sections,
    /// Insulator assemblies: the strings' insulators.
    Insulators,
    /// Poles (`pole = "id"`).
    Poles,
    /// Structure models (`model = "….ohlfe"`).
    Models,
    /// Load-case recipes on their own.
    Recipes,
    /// Pylon templates (a new structure built from one).
    Templates,
}

impl LibKind {
    /// Every kind, in the order of the menus.
    pub const ALL: [LibKind; 6] = [
        LibKind::Sections,
        LibKind::Insulators,
        LibKind::Poles,
        LibKind::Models,
        LibKind::Recipes,
        LibKind::Templates,
    ];

    /// Its name in the menus and the panel.
    pub fn title(self) -> &'static str {
        match self {
            LibKind::Sections => "Section libraries",
            LibKind::Insulators => "Insulator assemblies",
            LibKind::Poles => "Poles",
            LibKind::Models => "Structure models",
            LibKind::Recipes => "Load-case recipes",
            LibKind::Templates => "Pylon templates",
        }
    }

    /// Its files' extension.
    pub fn extension(self) -> &'static str {
        match self {
            LibKind::Sections => SECTION_FILE_EXTENSION,
            LibKind::Insulators => INSULATOR_FILE_EXTENSION,
            LibKind::Poles => "ohlp",
            LibKind::Models => "ohlfe",
            LibKind::Recipes => STRUCTURE_LOAD_CASE_FILE_EXTENSION,
            LibKind::Templates => TEMPLATE_FILE_EXTENSION,
        }
    }

    /// Its folder in an export (the line application's File › Export ›
    /// Libraries layout: poles, models and recipes beside the structures).
    pub fn export_folder(self) -> &'static str {
        match self {
            LibKind::Sections => "sections",
            LibKind::Insulators => "insulators",
            LibKind::Poles | LibKind::Models | LibKind::Recipes => "structures",
            LibKind::Templates => "pylon-templates",
        }
    }
}

/// Where the window reads each library and imports into.
#[derive(Debug, Clone, PartialEq)]
pub struct Folders {
    /// The structure file's folder.
    pub structure: PathBuf,
    /// The section library: a folder (or a file, `--sections FILE`).
    pub sections: PathBuf,
    /// The folders insulator assemblies are read from.
    pub insulators: Vec<PathBuf>,
    /// The folder insulator assemblies are imported into.
    pub insulators_into: PathBuf,
}

impl Folders {
    /// For the structure file `path`, the section library and insulator
    /// folder given (none: the defaults beside the file).
    pub fn new(path: &Path, sections: Option<&Path>, insulators: Option<&Path>) -> Self {
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let (insulators, insulators_into) = match insulators {
            Some(d) => (vec![d.to_path_buf()], d.to_path_buf()),
            None => (
                vec![dir.clone(), dir.join("Insulators")],
                dir.join("Insulators"),
            ),
        };
        Self {
            sections: sections.map_or_else(|| dir.join("Sections"), Path::to_path_buf),
            structure: dir,
            insulators,
            insulators_into,
        }
    }

    /// The folders `kind` is read from.
    pub fn read(&self, kind: LibKind) -> Vec<PathBuf> {
        match kind {
            LibKind::Sections => vec![self.sections.clone()],
            LibKind::Insulators => self.insulators.clone(),
            LibKind::Poles | LibKind::Models | LibKind::Recipes => vec![self.structure.clone()],
            LibKind::Templates => vec![self.structure.join("PylonTemplates")],
        }
    }

    /// The folder `kind` is imported into, or why there is none.
    pub fn import_into(&self, kind: LibKind) -> Result<PathBuf, String> {
        match kind {
            LibKind::Sections if self.sections.is_file() => Err(format!(
                "the section library given is a file ({}): start with --sections FOLDER to \
                 import into a folder",
                self.sections.display()
            )),
            LibKind::Insulators => Ok(self.insulators_into.clone()),
            _ => Ok(self.read(kind).remove(0)),
        }
    }
}

/// The library files and ids a structure names.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct References {
    /// Section ids of members, extension members and the pole that the
    /// file's own `[[section]]` tables do not define.
    pub sections: BTreeSet<String>,
    /// Insulator assembly ids of strings and attachments.
    pub insulators: BTreeSet<String>,
    /// The pole file named (`pole = "…"`).
    pub pole: Option<String>,
    /// The structure model named (`model = "….ohlfe"`).
    pub model: Option<String>,
    /// The pylon template it was built from (`[template] id`).
    pub template: Option<String>,
}

/// What `t` names from the libraries.
pub fn references(t: &StructureType) -> References {
    let own: BTreeSet<&str> = t
        .sections
        .iter()
        .filter_map(|s| s.get("id").and_then(|v| v.as_str()))
        .collect();
    let mut r = References::default();
    let members = t
        .members
        .iter()
        .chain(t.extensions.iter().flat_map(|e| e.members.iter()));
    for s in members.filter_map(|m| m.section.as_deref()) {
        if !own.contains(s) {
            r.sections.insert(s.to_owned());
        }
    }
    match &t.pole {
        Some(PoleRef::File(name)) => r.pole = Some(name.clone()),
        Some(PoleRef::Inline(p)) => {
            if let Some(s) = p.section.as_deref().filter(|s| !own.contains(s)) {
                r.sections.insert(s.to_owned());
            }
        }
        None => {}
    }
    let strings = t.strings.iter().filter_map(|s| s.insulator.clone());
    let attachments = t.attachments.iter().filter_map(|a| a.insulator.clone());
    r.insulators = strings.chain(attachments).collect();
    r.model = t.model.clone();
    r.template = t
        .template
        .as_ref()
        .and_then(|tp| tp.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    r
}

/// A library file found.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its kind.
    pub kind: LibKind,
    /// The file.
    pub path: PathBuf,
    /// The ids it holds (a section library: its sections; others: its
    /// own id or name).
    pub ids: Vec<String>,
    /// Why it does not read.
    pub error: Option<String>,
}

/// The libraries found and what is missing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    /// Every library file found, by kind.
    pub entries: Vec<Entry>,
    /// What the structure names.
    pub references: References,
    /// What it names that no library has, in words.
    pub missing: Vec<String>,
}

impl Listing {
    /// The files of `kind`.
    pub fn of(&self, kind: LibKind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == kind)
    }

    /// Whether a file of `kind` that reads has `id`.
    pub fn has(&self, kind: LibKind, id: &str) -> bool {
        self.of(kind)
            .any(|e| e.error.is_none() && e.ids.iter().any(|i| i == id))
    }
}

/// Reads library file `path` of `kind` (`text`: its content; `sections`:
/// the section library a structure model is read with): its ids, or why it
/// does not read.
pub fn check_file(
    kind: LibKind,
    path: &Path,
    text: &str,
    sections: &SectionLibrary,
) -> Result<Vec<String>, String> {
    match kind {
        LibKind::Sections => SectionLibrary::parse(text)
            .map(|l| l.sections.into_iter().map(|s| s.id).collect())
            .map_err(|e| format!("{}: {e}", path.display())),
        LibKind::Insulators => InsulatorAssembly::parse(path, text)
            .map(|a| vec![a.id])
            .map_err(|e| e.to_string()),
        LibKind::Poles => ohl_structure_link::pole_file_id(text)
            .map(|id| vec![id])
            .map_err(|e| format!("{}: {e}", path.display())),
        LibKind::Models => ohl_structure_link::model_file_name(text, sections)
            .map(|n| vec![n])
            .map_err(|e| format!("{}: {e}", path.display())),
        LibKind::Recipes => StructureLoadCases::parse(text)
            .map(|s| s.cases.into_iter().map(|c| c.name).collect())
            .map_err(|e| format!("{}: {e}", path.display())),
        LibKind::Templates => PylonTemplate::parse(path, text)
            .map(|t| vec![t.id])
            .map_err(|e| e.to_string()),
    }
}

/// The files of `kind` at `at` (a file, or the files of a folder).
fn files_of(at: &Path, kind: LibKind) -> Vec<PathBuf> {
    if at.is_file() {
        return vec![at.to_path_buf()];
    }
    ohl_core::files::files_with_extension(at, kind.extension())
}

/// The libraries in `folders` (each file read and checked) and what the
/// structure `t` names that none has.
pub fn scan(folders: &Folders, t: Option<&StructureType>) -> Listing {
    let mut entries = Vec::new();
    let sections = ohl_structure_link::section_library(&folders.sections).unwrap_or_default();
    for kind in LibKind::ALL {
        for dir in folders.read(kind) {
            for path in files_of(&dir, kind) {
                let (ids, error) = match std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))
                    .and_then(|text| check_file(kind, &path, &text, &sections))
                {
                    Ok(ids) => (ids, None),
                    Err(e) => (Vec::new(), Some(e)),
                };
                entries.push(Entry {
                    kind,
                    path,
                    ids,
                    error,
                });
            }
        }
    }
    let mut listing = Listing {
        entries,
        ..Listing::default()
    };
    if let Some(t) = t {
        let r = references(t);
        let missing_sections: Vec<&String> = r
            .sections
            .iter()
            .filter(|s| !listing.has(LibKind::Sections, s))
            .collect();
        if !missing_sections.is_empty() {
            listing.missing.push(format!(
                "section(s) in no library: {} (looked in {})",
                join(&missing_sections),
                folders.sections.display()
            ));
        }
        let missing_insulators: Vec<&String> = r
            .insulators
            .iter()
            .filter(|s| !listing.has(LibKind::Insulators, s))
            .collect();
        if !missing_insulators.is_empty() {
            listing.missing.push(format!(
                "insulator assembly(ies) in no library: {}",
                join(&missing_insulators)
            ));
        }
        if let Some(name) = &r.pole {
            let found =
                crate::pole_panel::pole_file(&folders.structure, name).is_some_and(|p| p.is_file());
            if !found {
                listing.missing.push(format!(
                    "pole \"{name}\": no .ohlp file of that name or id beside the structure"
                ));
            }
        }
        if let Some(m) = &r.model
            && !folders.structure.join(m).is_file()
        {
            listing.missing.push(format!(
                "structure model \"{m}\": not beside the structure file"
            ));
        }
        listing.references = r;
    }
    listing
}

/// "a, b, c" (at most 12, then "… (n more)").
fn join(ids: &[&String]) -> String {
    let mut s: Vec<String> = ids.iter().take(12).map(|i| (*i).clone()).collect();
    if ids.len() > 12 {
        s.push(format!("… ({} more)", ids.len() - 12));
    }
    s.join(", ")
}

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imported {
    /// The files written.
    pub written: Vec<PathBuf>,
    /// How many of them replaced a file of the same name.
    pub replaced: usize,
}

/// Imports library file or folder `from` of `kind` into the folder the
/// window reads it from. Every file is read and checked first; a file that
/// does not read, or that has an id another file there has already, stops
/// the import with nothing copied (each problem on a line of its own). A
/// file of the same name is replaced.
pub fn import(kind: LibKind, from: &Path, folders: &Folders) -> Result<Imported, String> {
    let into = folders.import_into(kind)?;
    let files = if from.is_dir() {
        files_of(from, kind)
    } else if ohl_core::files::has_extension(from, kind.extension()) {
        vec![from.to_path_buf()]
    } else {
        return Err(format!(
            "{}: not a .{} file ({})",
            from.display(),
            kind.extension(),
            kind.title()
        ));
    };
    if files.is_empty() {
        return Err(format!(
            "{}: no .{} file in this folder",
            from.display(),
            kind.extension()
        ));
    }
    let sections = ohl_structure_link::section_library(&folders.sections).unwrap_or_default();
    let mut problems = Vec::new();
    let mut new_ids: Vec<(String, PathBuf)> = Vec::new();
    for f in &files {
        match std::fs::read_to_string(f)
            .map_err(|e| format!("{}: {e}", f.display()))
            .and_then(|text| check_file(kind, f, &text, &sections))
        {
            Ok(ids) => new_ids.extend(ids.into_iter().map(|i| (i, f.clone()))),
            Err(e) => problems.push(e),
        }
    }
    // Ids are unique in a folder for sections and insulators (the readers
    // refuse two files with one id); a file replaced does not count.
    if matches!(kind, LibKind::Sections | LibKind::Insulators) {
        let names: BTreeSet<_> = files.iter().filter_map(|f| f.file_name()).collect();
        let mut seen: Vec<(String, PathBuf)> = Vec::new();
        for there in files_of(&into, kind) {
            if there.file_name().is_some_and(|n| names.contains(n)) {
                continue;
            }
            if let Ok(ids) = std::fs::read_to_string(&there)
                .map_err(|e| e.to_string())
                .and_then(|text| check_file(kind, &there, &text, &sections))
            {
                seen.extend(ids.into_iter().map(|i| (i, there.clone())));
            }
        }
        for (id, f) in &new_ids {
            if let Some((_, other)) = seen.iter().find(|(i, _)| i == id) {
                problems.push(format!(
                    "{}: \"{id}\" is also in {}",
                    f.display(),
                    other.display()
                ));
            } else {
                seen.push((id.clone(), f.clone()));
            }
        }
    }
    if !problems.is_empty() {
        return Err(format!(
            "nothing imported, {} problem(s):\n{}",
            problems.len(),
            problems.join("\n")
        ));
    }
    std::fs::create_dir_all(&into).map_err(|e| format!("{}: {e}", into.display()))?;
    let mut done = Imported::default();
    for f in files {
        let Some(name) = f.file_name() else {
            continue;
        };
        let to = into.join(name);
        if same_file(&f, &to) {
            continue;
        }
        if to.exists() {
            done.replaced += 1;
        }
        std::fs::copy(&f, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        done.written.push(to);
    }
    Ok(done)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Which libraries an export writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// What the structure uses: the sections it names (with their
    /// materials), its insulators, its pole and model, the template it was
    /// built from.
    Used,
    /// Every library file the window reads.
    All,
}

/// A section library's text with only the sections `keep` and the
/// materials they use (their own, or the file's `default_material`);
/// none kept: `None`. Comments and the order of what stays are kept.
pub fn sections_subset(text: &str, keep: &BTreeSet<String>) -> Result<Option<String>, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    let default = doc
        .get("default_material")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let mut materials = BTreeSet::new();
    let mut kept = 0;
    if let Some(sections) = doc
        .get_mut("section")
        .and_then(|s| s.as_array_of_tables_mut())
    {
        sections.retain(|s| {
            let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let yes = keep.contains(id);
            if yes {
                kept += 1;
                if let Some(m) = s
                    .get("material")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .or(default.clone())
                {
                    materials.insert(m);
                }
            }
            yes
        });
    }
    if kept == 0 {
        return Ok(None);
    }
    if let Some(ms) = doc
        .get_mut("material")
        .and_then(|s| s.as_array_of_tables_mut())
    {
        ms.retain(|m| {
            m.get("id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| materials.contains(id))
        });
    }
    Ok(Some(doc.to_string()))
}

/// Writes the structure (`text`, its file name `name`) and the libraries
/// `which` of `listing` into `out`, a folder per kind as the line
/// application's File › Export › Libraries does. The files written.
pub fn export(
    listing: &Listing,
    name: &str,
    text: &str,
    which: Which,
    out: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut written = Vec::new();
    let mut write = |folder: &str, file: &str, body: &str| -> Result<(), String> {
        let dir = out.join(folder);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let to = dir.join(file);
        std::fs::write(&to, body).map_err(|e| format!("{}: {e}", to.display()))?;
        written.push(to);
        Ok(())
    };
    write("structures", name, text)?;
    let r = &listing.references;
    for e in listing.entries.iter().filter(|e| e.error.is_none()) {
        let Some(file) = e.path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let read =
            || std::fs::read_to_string(&e.path).map_err(|x| format!("{}: {x}", e.path.display()));
        let body = match (which, e.kind) {
            (Which::All, _) => Some(read()?),
            (Which::Used, LibKind::Sections) => sections_subset(&read()?, &r.sections)
                .map_err(|x| format!("{}: {x}", e.path.display()))?,
            (Which::Used, LibKind::Insulators) => e
                .ids
                .iter()
                .any(|i| r.insulators.contains(i))
                .then(read)
                .transpose()?,
            (Which::Used, LibKind::Poles) => r
                .pole
                .as_ref()
                .is_some_and(|p| e.ids.contains(p) || *p == file)
                .then(read)
                .transpose()?,
            (Which::Used, LibKind::Models) => r
                .model
                .as_ref()
                .is_some_and(|m| Path::new(m).file_name() == e.path.file_name())
                .then(read)
                .transpose()?,
            (Which::Used, LibKind::Templates) => r
                .template
                .as_ref()
                .is_some_and(|t| e.ids.contains(t))
                .then(read)
                .transpose()?,
            // The structure's recipes are in its file.
            (Which::Used, LibKind::Recipes) => None,
        };
        if let Some(body) = body {
            write(e.kind.export_folder(), &file, &body)?;
        }
    }
    Ok(written)
}

/// The Libraries panel's state.
#[derive(Debug, Default)]
pub struct LibrariesState {
    /// What was found, and for which file text and folders.
    pub listing: Option<(u64, Listing)>,
    /// A report to show in a window: title, text, error.
    pub report: Option<(String, String, bool)>,
}

impl StructureApp {
    /// The folders of the structure open.
    pub fn folders(&self) -> Option<Folders> {
        let path = self.file.as_ref()?.path()?;
        Some(Folders::new(
            path,
            self.sections_path.as_deref(),
            self.insulators_dir.as_deref(),
        ))
    }

    /// The libraries found (read again when the file or the folders
    /// changed, or after an import).
    pub fn listing(&mut self) -> Option<&Listing> {
        let folders = self.folders()?;
        let f = self.file.as_ref()?;
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            f.text().hash(&mut h);
            format!("{folders:?}").hash(&mut h);
            h.finish()
        };
        if self.libs.listing.as_ref().map(|l| l.0) != Some(key) {
            let listing = scan(&folders, Some(f.structure()));
            self.libs.listing = Some((key, listing));
        }
        self.libs.listing.as_ref().map(|l| &l.1)
    }

    /// File › Libraries › Import: `from` (a file or folder) of `kind`;
    /// the libraries read again. What was done, or why not (also in a
    /// window when long).
    pub fn import_library(&mut self, kind: LibKind, from: &Path) -> Result<Imported, String> {
        let Some(folders) = self.folders() else {
            return Err("open or save a structure first: its folder takes the libraries".into());
        };
        let r = import(kind, from, &folders);
        match &r {
            Ok(done) => {
                self.reload_libraries();
                let into = folders.import_into(kind).unwrap_or_default();
                self.message = Some((
                    format!(
                        "imported {} {} file(s) into {}{}",
                        done.written.len(),
                        kind.title().to_lowercase(),
                        into.display(),
                        if done.replaced > 0 {
                            format!(" ({} replaced)", done.replaced)
                        } else {
                            String::new()
                        }
                    ),
                    false,
                ));
            }
            Err(e) => {
                self.message = Some((format!("import: {}", e.lines().next().unwrap_or("")), true));
                self.libs.report = Some((format!("Import {}", kind.title()), e.clone(), true));
            }
        }
        r
    }

    /// File › Libraries › Export: the structure and its libraries (`which`)
    /// into folder `out`.
    pub fn export_libraries(&mut self, which: Which, out: &Path) -> Result<usize, String> {
        let (name, text) = {
            let f = self.file.as_ref().ok_or("no structure open")?;
            let name = f.path().and_then(Path::file_name).map_or_else(
                || format!("{}.ohls", f.structure().id),
                |n| n.to_string_lossy().into_owned(),
            );
            (name, f.text())
        };
        let listing = self.listing().cloned().unwrap_or_default();
        let r = export(&listing, &name, &text, which, out);
        self.message = Some(match &r {
            Ok(w) => (
                format!(
                    "exported the structure and {} library file(s) to {}",
                    w.len() - 1,
                    out.display()
                ),
                false,
            ),
            Err(e) => (format!("export of the libraries: {e}"), true),
        });
        r.map(|w| w.len())
    }

    /// The insulator assemblies and the section library read again (after
    /// an import, or Reload).
    pub fn reload_libraries(&mut self) {
        let Some(path) = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .map(Path::to_path_buf)
        else {
            return;
        };
        self.load_libraries(&path);
        self.libs.listing = None;
    }

    /// File › Libraries: Import per kind (a file or a folder), Export, the
    /// panel.
    pub fn libraries_menu(&mut self, ui: &mut egui::Ui) {
        let has = self.file.as_ref().is_some_and(|f| f.path().is_some());
        ui.menu_button("Libraries", |ui| {
            for kind in LibKind::ALL {
                ui.add_enabled_ui(has, |ui| {
                    ui.menu_button(format!("Import {}", kind.title().to_lowercase()), |ui| {
                        let ext = kind.extension();
                        if ui
                            .button(format!("A file (.{ext})…"))
                            .on_hover_text("Copy one file into the folder the window reads")
                            .clicked()
                        {
                            ui.close();
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter(kind.title(), &ohl_core::files::open_extensions(ext))
                                .pick_file()
                            {
                                let _ = self.import_library(kind, &p);
                            }
                        }
                        if ui
                            .button("The files of a folder…")
                            .on_hover_text(format!(
                                "Copy every .{ext} file of a folder into the folder the window reads"
                            ))
                            .clicked()
                        {
                            ui.close();
                            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                                let _ = self.import_library(kind, &p);
                            }
                        }
                        if let Some(into) = self.folders().map(|f| f.import_into(kind)) {
                            ui.label(
                                egui::RichText::new(match into {
                                    Ok(d) => format!("into {}", d.display()),
                                    Err(e) => e,
                                })
                                .small()
                                .weak(),
                            );
                        }
                    })
                    .response
                    .on_disabled_hover_text("Open (or save) a structure file first");
                });
            }
            ui.separator();
            for (which, label, tip) in [
                (
                    Which::Used,
                    "Export what it uses…",
                    "The structure, the sections it names (with their materials), its insulators, \
                     pole, model and template, into a folder (structures/, sections/, insulators/…)",
                ),
                (
                    Which::All,
                    "Export every library…",
                    "The structure and every library file the window reads, into a folder",
                ),
            ] {
                if ui
                    .add_enabled(has, egui::Button::new(label))
                    .on_hover_text(tip)
                    .clicked()
                {
                    ui.close();
                    if let Some(out) = rfd::FileDialog::new().pick_folder() {
                        let _ = self.export_libraries(which, &out);
                    }
                }
            }
            ui.separator();
            if ui
                .button("Libraries panel…")
                .on_hover_text(crate::layout::Panel::Libraries.tip())
                .clicked()
            {
                ui.close();
                self.focus(crate::layout::Panel::Libraries);
            }
        });
    }

    /// The Libraries panel: per kind, the files read and where from; what
    /// the structure names that none has; Reload.
    pub fn libraries_ui(&mut self, ui: &mut egui::Ui) {
        let Some(folders) = self.folders() else {
            ui.label("Save the structure first: its folder holds its libraries.");
            return;
        };
        if ui
            .button("⟳ Reload")
            .on_hover_text("Read the library folders again")
            .clicked()
        {
            self.reload_libraries();
        }
        let Some(listing) = self.listing().cloned() else {
            return;
        };
        if listing.missing.is_empty() {
            ui.colored_label(
                crate::app::ok_color(ui),
                "Every library the structure names is there.",
            );
        } else {
            ui.strong("The structure names, but no library has:");
            error_label(ui, listing.missing.join("\n"));
        }
        let mut take: Option<PathBuf> = None;
        for kind in LibKind::ALL {
            let entries: Vec<&Entry> = listing.of(kind).collect();
            let into = folders
                .read(kind)
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            egui::CollapsingHeader::new(format!("{} ({})", kind.title(), entries.len()))
                // Opened when files come (its state is kept per "has files").
                .id_salt(("libraries", kind, entries.is_empty()))
                .default_open(!entries.is_empty())
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(format!("from {into}")).small().weak());
                    for e in entries {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                e.path
                                    .file_name()
                                    .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                            )
                            .on_hover_text(e.path.display().to_string());
                            if e.error.is_none() {
                                let used = match kind {
                                    LibKind::Sections => e
                                        .ids
                                        .iter()
                                        .filter(|i| listing.references.sections.contains(*i))
                                        .count(),
                                    LibKind::Insulators => e
                                        .ids
                                        .iter()
                                        .filter(|i| listing.references.insulators.contains(*i))
                                        .count(),
                                    _ => 0,
                                };
                                let mut text = join(&e.ids.iter().collect::<Vec<_>>());
                                if used > 0 {
                                    text = format!("{text} — {used} used");
                                }
                                ui.label(egui::RichText::new(text).weak());
                                if kind == LibKind::Recipes
                                    && ui
                                        .small_button("Take its cases")
                                        .on_hover_text(
                                            "Its load cases into the structure file (one with \
                                             the name of one there replaces it; one undo step)",
                                        )
                                        .clicked()
                                {
                                    take = Some(e.path.clone());
                                }
                            }
                        });
                        if let Some(err) = &e.error {
                            error_label(ui, err.as_str());
                        }
                    }
                });
        }
        if let Some(p) = take
            && let Some(f) = &mut self.file
        {
            self.message = Some(match crate::load_cases_tab::import_recipes(f, &p) {
                Ok((a, r)) => (
                    format!(
                        "took {a} new and {r} replaced load case(s) from {}",
                        p.display()
                    ),
                    false,
                ),
                Err(e) => (e, true),
            });
        }
    }

    /// An import's (or another library action's) report, in a window of
    /// its own (closable; long text scrolls).
    pub(crate) fn libraries_window(&mut self, ctx: &egui::Context) {
        let Some((title, text, error)) = self.libs.report.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new(title)
            .id(egui::Id::new("library-report"))
            .collapsible(false)
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                if error {
                    error_label(ui, text.as_str());
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| ui.label(text.as_str()));
                }
            });
        if !open {
            self.libs.report = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// A folder of its own with a structure naming library sections and an
    /// insulator assembly.
    fn structure(tag: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("ohl-structure-libs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.ohls");
        std::fs::write(
            &path,
            r#"format = "ohl-structure"
version = 2
id = "t"
name = "T"
kind = "other"

[[point]]
label = "A1"
point = ["0 m", "0 m", "10 m"]

[[wire]]
label = "W1"
set = 1

[[string]]
label = "S1"
top = "A1"
wire = "W1"
insulator = "example-220-i"

[[node]]
id = "F"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "T"
point = ["0 m", "0 m", "10 m"]

[[member]]
id = "M1"
nodes = ["F", "T"]
section = "EA-61x7"

[[member]]
id = "M2"
nodes = ["F", "T"]
section = "NOT-THERE"
"#,
        )
        .unwrap();
        (dir, path)
    }

    /// A section library imported is listed and resolves the structure's
    /// sections; the one it names that no library has stays missing; a
    /// file that does not read, or names an id another has, imports
    /// nothing; export writes only the sections used, with their
    /// material.
    #[test]
    fn import_list_missing_and_export() {
        let (dir, path) = structure("a");
        let t = ohl_structures::structures::StructureType::parse(
            &path,
            &std::fs::read_to_string(&path).unwrap(),
        )
        .unwrap();
        let folders = Folders::new(&path, None, None);
        let before = scan(&folders, Some(&t));
        assert!(
            before.missing.iter().any(|m| m.contains("EA-61x7")),
            "{:?}",
            before.missing
        );
        assert!(before.missing.iter().any(|m| m.contains("example-220-i")));
        let lib = root().join("libraries/sections/example-sections.ohlsec");
        let done = import(LibKind::Sections, &lib, &folders).unwrap();
        assert_eq!(
            done.written,
            vec![dir.join("Sections/example-sections.ohlsec")]
        );
        import(
            LibKind::Insulators,
            &root().join("libraries/insulators"),
            &folders,
        )
        .unwrap();
        let after = scan(&folders, Some(&t));
        assert!(after.has(LibKind::Sections, "EA-61x7"));
        assert!(after.has(LibKind::Insulators, "example-220-i"));
        assert_eq!(after.missing.len(), 1, "{:?}", after.missing);
        assert!(after.missing[0].contains("NOT-THERE"));
        // The same ids in another file: refused, nothing copied.
        let twin = dir.join("twin.ohlsec");
        std::fs::copy(&lib, &twin).unwrap();
        let e = import(LibKind::Sections, &twin, &folders).unwrap_err();
        assert!(
            e.starts_with("nothing imported") && e.contains("is also in"),
            "{e}"
        );
        assert!(!dir.join("Sections/twin.ohlsec").exists());
        // A typo: refused with the file's error.
        let typo = dir.join("typo.ohlsec");
        std::fs::write(
            &typo,
            "format = \"ohl-sections\"\nversion = 1\nnmae = \"x\"\n",
        )
        .unwrap();
        let e = import(LibKind::Sections, &typo, &folders).unwrap_err();
        assert!(e.contains("typo.ohlsec") && e.contains("nmae"), "{e}");
        // The wrong kind of file.
        assert!(import(LibKind::Poles, &typo, &folders).is_err());
        // Export what it uses: the structure, one section, its material.
        let out = dir.join("out");
        let text = std::fs::read_to_string(&path).unwrap();
        let w = export(&after, "t.ohls", &text, Which::Used, &out).unwrap();
        assert!(w.contains(&out.join("structures/t.ohls")));
        let s = std::fs::read_to_string(out.join("sections/example-sections.ohlsec")).unwrap();
        let read = SectionLibrary::parse(&s).unwrap();
        assert_eq!(
            read.sections
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["EA-61x7"]
        );
        assert_eq!(read.materials.len(), 1);
        assert!(read.missing_materials().is_empty());
        let insulators: Vec<_> = std::fs::read_dir(out.join("insulators")).unwrap().collect();
        assert_eq!(insulators.len(), 1);
        // Every library: all three assemblies.
        let all = dir.join("all");
        export(&after, "t.ohls", &text, Which::All, &all).unwrap();
        assert_eq!(
            std::fs::read_dir(all.join("insulators")).unwrap().count(),
            3
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
