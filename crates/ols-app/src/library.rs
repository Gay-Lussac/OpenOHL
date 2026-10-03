// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Library editors: wires (`.olsw`), insulator assemblies (`.olsi`),
//! structure types (`.olss`) and weather cases (`.olswc`). One editor for
//! all of them: the file is edited as TOML (`toml_edit`, so comments and
//! layout stay), every field as text with its units (`"28.1 mm"`), with
//! the known fields of each kind to add. A change is checked at once with
//! the library's own reader; a file is saved only when it reads, then the
//! model reads its libraries again and solves.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::model::Sources;

/// A kind of library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LibraryKind {
    /// Wires and cables (`.olsw`, several per file).
    Cables,
    /// Insulator assemblies (`.olsi`, one per file).
    Insulators,
    /// Structure types (`.olss`, one per file).
    StructureTypes,
    /// Weather cases (`.olswc`, several per file).
    WeatherCases,
}

/// A field that can be added: key, example value (TOML), help.
type Field = (&'static str, &'static str, &'static str);

const COMPONENT: &[Field] = &[
    ("area", "\"400 mm²\"", "cross-section of this component"),
    ("modulus", "\"55 GPa\"", "elastic (final) modulus"),
    (
        "thermal_expansion",
        "23e-6",
        "linear thermal expansion, 1/K",
    ),
];

const CABLE: &[Field] = &[
    (
        "id",
        "\"new-cable\"",
        "unique id, used by the sections (letters, digits, - _ .)",
    ),
    ("name", "\"New cable\"", "display name"),
    (
        "kind",
        "\"acsr\"",
        "acsr, acss, aac, aaac, acar, accc, opgw, adss, steel or other",
    ),
    ("source", "\"data sheet …\"", "where the values come from"),
    ("diameter", "\"28.1 mm\"", "overall diameter"),
    ("mass_per_length", "\"1.63 kg/m\"", "mass per length"),
    (
        "rated_tensile_strength",
        "\"120 kN\"",
        "rated tensile strength (RTS)",
    ),
    (
        "outer",
        "{ area = \"400 mm²\", modulus = \"55 GPa\", thermal_expansion = 23e-6 }",
        "outer component (the whole cable if homogeneous)",
    ),
    (
        "core",
        "{ area = \"50 mm²\", modulus = \"190 GPa\", thermal_expansion = 11.5e-6 }",
        "core component (bimetallic cables)",
    ),
];

const CASE: &[Field] = &[
    ("name", "\"new case\"", "unique name"),
    ("description", "\"\"", "free text"),
    (
        "tags",
        "[\"clearance\"]",
        "for choosing cases (e.g. clearance, loads, stringing)",
    ),
    ("temperature", "\"15 °C\"", "cable temperature"),
    ("ice_thickness", "\"10 mm\"", "radial ice thickness"),
    ("ice_load", "\"15 N/m\"", "or: ice load per length"),
    (
        "ice_density",
        "\"913 kg/m³\"",
        "ice density (wind diameter)",
    ),
    (
        "wind_pressure",
        "\"400 Pa\"",
        "wind pressure on the projected diameter",
    ),
];

const WEATHER_FILE: &[Field] = &[
    ("format", "\"ols-weather-cases\"", "file format"),
    ("version", "1", "format version"),
    ("name", "\"Weather cases\"", "name of the set"),
];

const INSULATOR: &[Field] = &[
    ("format", "\"ols-insulator\"", "file format"),
    ("version", "1", "format version"),
    (
        "id",
        "\"new-insulator\"",
        "unique id, used by structure types and strings",
    ),
    ("name", "\"New insulator\"", "display name"),
    ("kind", "\"suspension\"", "suspension, v_string or strain"),
    ("source", "\"\"", "where the values come from"),
    (
        "length",
        "\"4.5 m\"",
        "attachment point to clamp (V-string: leg length)",
    ),
    ("weight", "\"1500 N\"", "total weight"),
    (
        "axial_stiffness",
        "\"50 MN\"",
        "axial stiffness of a string or leg",
    ),
    ("wind_area", "\"0.5 m²\"", "projected area for wind"),
    (
        "half_width",
        "\"2 m\"",
        "V-string: centre to each leg attachment",
    ),
];

const STRUCTURE: &[Field] = &[
    ("format", "\"ols-structure\"", "file format"),
    ("version", "1", "format version"),
    (
        "id",
        "\"new-type\"",
        "unique id, used by the line's structures",
    ),
    ("name", "\"New type\"", "display name"),
    ("kind", "\"lattice\"", "lattice, pole, portal or other"),
    ("height", "\"40 m\"", "mast top for the stick drawing"),
    ("source", "\"\"", "where the values come from"),
];

const ATTACHMENT: &[Field] = &[
    ("set", "1", "set (cross-arm) number"),
    ("phase", "1", "phase number in the set"),
    ("label", "\"\"", "label"),
    (
        "point",
        "[\"0 m\", \"0 m\", \"30 m\"]",
        "insulator attachment point: across (right looking ahead), along (ahead), up",
    ),
    (
        "insulator",
        "\"\"",
        "insulator assembly id (or give a string)",
    ),
    ("side", "\"ahead\"", "strain: ahead or back"),
    (
        "string",
        "{ length = \"4 m\", weight = \"1000 N\" }",
        "or: the string here (length, weight, top, bottom labels)",
    ),
];

const EXTENSION: &[Field] = &[
    ("name", "\"+3\"", "name of the extension"),
    ("dz", "\"3 m\"", "height added to every attachment point"),
];

const BODY: &[Field] = &[(
    "points",
    "[[\"0 m\", \"0 m\", \"0 m\"], [\"0 m\", \"0 m\", \"40 m\"]]",
    "polyline in local axes",
)];

impl LibraryKind {
    /// Every kind (the Libraries menu).
    pub const ALL: [LibraryKind; 4] = [
        LibraryKind::Cables,
        LibraryKind::Insulators,
        LibraryKind::StructureTypes,
        LibraryKind::WeatherCases,
    ];

    /// Title of its page.
    pub fn title(self) -> &'static str {
        match self {
            LibraryKind::Cables => "Wires",
            LibraryKind::Insulators => "Insulators",
            LibraryKind::StructureTypes => "Structure types",
            LibraryKind::WeatherCases => "Weather cases",
        }
    }

    /// File extension.
    pub fn extension(self) -> &'static str {
        match self {
            LibraryKind::Cables => "olsw",
            LibraryKind::Insulators => "olsi",
            LibraryKind::StructureTypes => "olss",
            LibraryKind::WeatherCases => "olswc",
        }
    }

    /// Several items per file: the array of tables holding them.
    fn items_key(self) -> Option<&'static str> {
        match self {
            LibraryKind::Cables => Some("cable"),
            LibraryKind::WeatherCases => Some("case"),
            _ => None,
        }
    }

    /// Fields that can be added to a table at `context` (the array key,
    /// or `""` for the file's top level).
    fn fields(self, context: &str) -> &'static [Field] {
        match (self, context) {
            (LibraryKind::Cables, "cable") => CABLE,
            (LibraryKind::Cables, "outer" | "core") => COMPONENT,
            (LibraryKind::WeatherCases, "case") => CASE,
            (LibraryKind::WeatherCases, "") => WEATHER_FILE,
            (LibraryKind::Insulators, "") => INSULATOR,
            (LibraryKind::StructureTypes, "") => STRUCTURE,
            (LibraryKind::StructureTypes, "attachment") => ATTACHMENT,
            (LibraryKind::StructureTypes, "extension") => EXTENSION,
            (LibraryKind::StructureTypes, "body") => BODY,
            _ => &[],
        }
    }

    /// Arrays of tables a table at `context` can get.
    fn arrays(self, context: &str) -> &'static [&'static str] {
        match (self, context) {
            (LibraryKind::StructureTypes, "") => &["attachment", "extension", "body"],
            _ => &[],
        }
    }

    /// Where its files are, as the model uses them.
    pub fn location(self, sources: &Sources) -> PathBuf {
        match self {
            LibraryKind::Cables => crate::model::library_path(sources),
            LibraryKind::Insulators => crate::model::insulators_dir(sources),
            LibraryKind::StructureTypes => crate::model::structures_dir(sources),
            LibraryKind::WeatherCases => sources
                .weather
                .clone()
                .unwrap_or_else(|| crate::model::model_dir(sources)),
        }
    }

    /// Its files: the file, or the files of the directory.
    pub fn files(self, sources: &Sources) -> Vec<PathBuf> {
        let at = self.location(sources);
        let dir = if at.is_dir() {
            at.clone()
        } else {
            at.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|it| {
                it.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|x| x == self.extension()))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        if at.is_file() && !out.contains(&at) {
            out.insert(0, at);
        }
        out
    }

    /// The directory new files go to.
    pub fn new_file_dir(self, sources: &Sources) -> PathBuf {
        let at = self.location(sources);
        if at.is_dir() || at.extension().is_none() {
            at
        } else {
            at.parent().map(Path::to_path_buf).unwrap_or_default()
        }
    }

    /// Checks a file's text with the library's own reader.
    pub fn check(self, path: &Path, text: &str) -> Result<(), String> {
        use ols_structures::LibraryItem;
        match self {
            LibraryKind::Cables => ols_cables::Library::default()
                .add_toml(path, text)
                .map_err(|e| e.to_string()),
            LibraryKind::WeatherCases => ols_weather::WeatherCases::parse(text).map(|_| ()),
            LibraryKind::StructureTypes => ols_structures::StructureType::parse(path, text)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            LibraryKind::Insulators => ols_structures::InsulatorAssembly::parse(path, text)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        }
    }

    /// A new file's text.
    pub fn template(self, id: &str) -> String {
        match self {
            LibraryKind::Cables => format!(
                "# OLS CAD wire library (.olsw)\nformat = \"ols-wire\"\nversion = 1\n\n{}",
                self.item_template(id)
            ),
            LibraryKind::WeatherCases => format!(
                "# OLS CAD weather cases (.olswc)\nformat = \"ols-weather-cases\"\nversion = 1\nname = \"{id}\"\n\n{}",
                self.item_template("everyday")
            ),
            LibraryKind::Insulators => format!(
                "format = \"ols-insulator\"\nversion = 1\nid = \"{id}\"\nname = \"{id}\"\nkind = \"suspension\"\nlength = \"4.5 m\"\nweight = \"1500 N\"\n"
            ),
            LibraryKind::StructureTypes => format!(
                "format = \"ols-structure\"\nversion = 1\nid = \"{id}\"\nname = \"{id}\"\nkind = \"lattice\"\n\n[[attachment]]\nset = 1\nphase = 1\nlabel = \"\"\npoint = [\"6 m\", \"0 m\", \"30 m\"]\nstring = {{ length = \"4 m\", weight = \"1000 N\" }}\n"
            ),
        }
    }

    /// A new item of a several-per-file kind.
    fn item_template(self, id: &str) -> String {
        match self {
            LibraryKind::Cables => format!(
                "[[cable]]\nid = \"{id}\"\nname = \"{id}\"\nkind = \"acsr\"\ndiameter = \"28.1 mm\"\nmass_per_length = \"1.63 kg/m\"\nrated_tensile_strength = \"120 kN\"\nouter = {{ area = \"400 mm²\", modulus = \"55 GPa\", thermal_expansion = 23e-6 }}\n"
            ),
            LibraryKind::WeatherCases => {
                format!("[[case]]\nname = \"{id}\"\ntemperature = \"15 °C\"\n")
            }
            _ => String::new(),
        }
    }
}

/// An open library page.
pub struct LibraryPage {
    /// Which library.
    pub kind: LibraryKind,
    /// Its files.
    pub files: Vec<PathBuf>,
    /// The file open.
    pub file: Option<PathBuf>,
    /// Its content, as edited.
    doc: Option<DocumentMut>,
    /// The text as read (unsaved changes when different).
    saved: String,
    /// The item shown (several-per-file kinds).
    item: usize,
    /// Why the file does not read, as edited.
    pub error: Option<String>,
    /// What happened last (saved, reloaded…).
    pub message: Option<(String, bool)>,
    /// Name typed for a new file.
    new_name: String,
    /// Fields being typed, by path (a number half typed does not parse).
    buffers: HashMap<String, String>,
}

/// What the page asks of the application.
pub enum PageAction {
    /// A file was saved: read the libraries again.
    Saved(PathBuf),
    /// Use this weather case file for the model.
    UseWeather(PathBuf),
}

impl LibraryPage {
    /// A page for `kind`, with its first file open.
    pub fn new(kind: LibraryKind, sources: &Sources) -> Self {
        let files = kind.files(sources);
        let mut page = Self {
            kind,
            files,
            file: None,
            doc: None,
            saved: String::new(),
            item: 0,
            error: None,
            message: None,
            new_name: String::new(),
            buffers: HashMap::new(),
        };
        let first = match kind {
            LibraryKind::WeatherCases => sources.weather.clone(),
            _ => None,
        }
        .or_else(|| page.files.first().cloned());
        if let Some(f) = first {
            page.open(&f);
        }
        page
    }

    /// Opens `path` (unsaved changes are dropped).
    pub fn open(&mut self, path: &Path) {
        self.buffers.clear();
        self.item = 0;
        self.message = None;
        match std::fs::read_to_string(path) {
            Ok(text) => match text.parse::<DocumentMut>() {
                Ok(doc) => {
                    self.saved = text;
                    self.doc = Some(doc);
                    self.file = Some(path.to_path_buf());
                    self.check();
                }
                Err(e) => {
                    self.doc = None;
                    self.file = Some(path.to_path_buf());
                    self.error = Some(format!("not TOML: {e}"));
                }
            },
            Err(e) => {
                self.doc = None;
                self.error = Some(format!("{}: {e}", path.display()));
            }
        }
    }

    /// The text as edited.
    pub fn text(&self) -> Option<String> {
        self.doc.as_ref().map(DocumentMut::to_string)
    }

    /// Whether there are unsaved changes.
    pub fn dirty(&self) -> bool {
        self.text().is_some_and(|t| t != self.saved)
    }

    fn check(&mut self) {
        self.error = match (&self.file, self.text()) {
            (Some(path), Some(text)) => self.kind.check(path, &text).err(),
            _ => None,
        };
    }

    /// Saves the file when it reads.
    pub fn save(&mut self) -> Result<PathBuf, String> {
        let (Some(path), Some(text)) = (self.file.clone(), self.text()) else {
            return Err("no file open".into());
        };
        self.kind.check(&path, &text)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.saved = text;
        Ok(path)
    }

    /// Creates a new file named `name` in the library's directory.
    pub fn create(&mut self, sources: &Sources, name: &str) -> Result<(), String> {
        let stem = name
            .trim()
            .trim_end_matches(&format!(".{}", self.kind.extension()));
        if stem.is_empty()
            || !stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return Err("a file name of letters, digits, - _ and .".into());
        }
        let path = self
            .kind
            .new_file_dir(sources)
            .join(format!("{stem}.{}", self.kind.extension()));
        if path.exists() {
            return Err(format!("{} exists already", path.display()));
        }
        let text = self.kind.template(stem);
        self.kind.check(&path, &text)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &text).map_err(|e| e.to_string())?;
        self.files = self.kind.files(sources);
        if !self.files.contains(&path) {
            self.files.push(path.clone());
        }
        self.open(&path);
        Ok(())
    }

    /// The items of a several-per-file kind: (label, index).
    fn items(&self) -> Vec<String> {
        let Some(key) = self.kind.items_key() else {
            return Vec::new();
        };
        self.doc
            .as_ref()
            .and_then(|d| d.get(key))
            .and_then(Item::as_array_of_tables)
            .map(|a| a.iter().map(label_of).collect())
            .unwrap_or_default()
    }

    /// Adds an item (a copy of the one shown with a new id, or the
    /// template).
    fn add_item(&mut self, copy: bool) {
        let Some(key) = self.kind.items_key() else {
            return;
        };
        let Some(doc) = &mut self.doc else {
            return;
        };
        let taken: Vec<String> = doc
            .get(key)
            .and_then(Item::as_array_of_tables)
            .map(|a| a.iter().map(label_of).collect())
            .unwrap_or_default();
        let id_key = if key == "case" { "name" } else { "id" };
        let mut table = match (copy, doc.get(key).and_then(Item::as_array_of_tables)) {
            (true, Some(a)) if a.get(self.item).is_some() => {
                a.get(self.item).expect("checked").clone()
            }
            _ => self
                .kind
                .item_template("new")
                .parse::<DocumentMut>()
                .ok()
                .and_then(|d| d.get(key)?.as_array_of_tables()?.get(0).cloned())
                .unwrap_or_default(),
        };
        let like = table
            .get(id_key)
            .and_then(Item::as_str)
            .unwrap_or("new")
            .to_owned();
        let id = crate::editors::next_id(taken.iter().map(String::as_str), &like);
        table.insert(id_key, toml_edit::value(id));
        if doc.get(key).and_then(Item::as_array_of_tables).is_none() {
            doc.insert(key, Item::ArrayOfTables(ArrayOfTables::new()));
        }
        let a = doc
            .get_mut(key)
            .and_then(Item::as_array_of_tables_mut)
            .expect("inserted");
        a.push(table);
        self.item = a.len() - 1;
        self.buffers.clear();
        self.check();
    }

    fn remove_item(&mut self) {
        let Some(key) = self.kind.items_key() else {
            return;
        };
        if let Some(a) = self
            .doc
            .as_mut()
            .and_then(|d| d.get_mut(key))
            .and_then(Item::as_array_of_tables_mut)
            && self.item < a.len()
        {
            a.remove(self.item);
            self.item = self.item.saturating_sub(1);
        }
        self.buffers.clear();
        self.check();
    }

    /// The page.
    pub fn ui(&mut self, ui: &mut egui::Ui, sources: &Sources) -> Option<PageAction> {
        let mut action = None;
        let kind = self.kind;
        ui.horizontal_wrapped(|ui| {
            let name = |p: &Path| {
                p.file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned())
            };
            let mut chosen = self.file.clone();
            ui.add_enabled_ui(!self.dirty(), |ui| {
                egui::ComboBox::from_id_salt(("library-file", kind))
                    .selected_text(chosen.as_deref().map_or("choose a file…".into(), name))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for f in &self.files {
                            ui.selectable_value(&mut chosen, Some(f.clone()), name(f))
                                .on_hover_text(f.display().to_string());
                        }
                    });
            })
            .response
            .on_disabled_hover_text("Save or discard the changes first");
            if chosen != self.file
                && let Some(f) = chosen
            {
                self.open(&f);
            }
            let dirty = self.dirty();
            let can_save = dirty && self.error.is_none();
            if ui
                .add_enabled(can_save, egui::Button::new("Save"))
                .on_disabled_hover_text(if dirty {
                    "The file does not read: see the error"
                } else {
                    "No changes"
                })
                .clicked()
            {
                match self.save() {
                    Ok(p) => action = Some(PageAction::Saved(p)),
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if dirty
                && ui.button("Discard").clicked()
                && let Some(f) = self.file.clone()
            {
                self.open(&f);
            }
            if dirty {
                ui.colored_label(egui::Color32::YELLOW, "● unsaved");
            }
            ui.separator();
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("new file name")
                    .desired_width(120.0),
            );
            if ui
                .add_enabled(
                    !dirty && !self.new_name.trim().is_empty(),
                    egui::Button::new("New file"),
                )
                .clicked()
            {
                let name = std::mem::take(&mut self.new_name);
                if let Err(e) = self.create(sources, &name) {
                    self.message = Some((e, true));
                    self.new_name = name;
                }
            }
            if kind == LibraryKind::WeatherCases
                && let Some(f) = &self.file
                && sources.weather.as_ref() != Some(f)
                && !dirty
                && ui
                    .button("Use for the model")
                    .on_hover_text("Solve the model in these weather cases")
                    .clicked()
            {
                action = Some(PageAction::UseWeather(f.clone()));
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "{} — values with their units, e.g. \"28.1 mm\"; the file is checked as you type",
                kind.location(sources).display()
            ))
            .small()
            .weak(),
        );
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, egui::RichText::new(e).small());
        }
        if let Some((m, error)) = &self.message {
            let t = egui::RichText::new(m).small();
            if *error {
                ui.colored_label(egui::Color32::LIGHT_RED, t);
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, t);
            }
        }
        ui.separator();
        if self.doc.is_none() {
            ui.label("No file open.");
            return action;
        }
        let mut changed = false;
        match kind.items_key() {
            Some(key) => {
                let items = self.items();
                egui::Panel::left(egui::Id::new(("library-items", kind)))
                    .resizable(true)
                    .default_size(200.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Add").clicked() {
                                self.add_item(false);
                            }
                            if ui.button("Duplicate").clicked() {
                                self.add_item(true);
                            }
                            if ui.button("Delete").clicked() {
                                self.remove_item();
                            }
                        });
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (k, l) in items.iter().enumerate() {
                                if ui.selectable_label(k == self.item, l).clicked() {
                                    self.item = k;
                                    self.buffers.clear();
                                }
                            }
                        });
                        // The file's own fields (name of a set of cases…).
                        if !kind.fields("").is_empty() {
                            ui.separator();
                            let doc = self.doc.as_mut().expect("checked");
                            changed |= table_ui(
                                ui,
                                kind,
                                doc.as_table_mut(),
                                "",
                                "file",
                                &mut self.buffers,
                                true,
                            );
                        }
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let doc = self.doc.as_mut().expect("checked");
                        if let Some(t) = doc
                            .get_mut(key)
                            .and_then(Item::as_array_of_tables_mut)
                            .and_then(|a| a.get_mut(self.item))
                        {
                            changed |= table_ui(
                                ui,
                                kind,
                                t,
                                key,
                                &format!("{key}{}", self.item),
                                &mut self.buffers,
                                false,
                            );
                        }
                    });
                });
            }
            None => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let doc = self.doc.as_mut().expect("checked");
                    changed |= table_ui(
                        ui,
                        kind,
                        doc.as_table_mut(),
                        "",
                        "",
                        &mut self.buffers,
                        false,
                    );
                });
            }
        }
        if changed {
            self.message = None;
            self.check();
        }
        action
    }
}

/// An item's label: its id or name.
fn label_of(t: &Table) -> String {
    ["id", "name"]
        .iter()
        .find_map(|k| t.get(k).and_then(Item::as_str))
        .unwrap_or("(no id)")
        .to_owned()
}

/// A value as typed (no surrounding spaces or comments).
fn repr(v: &Value) -> String {
    let mut c = v.clone();
    c.decor_mut().clear();
    c.to_string()
}

/// Edits one value; true when changed.
fn value_ui(
    ui: &mut egui::Ui,
    v: &mut Value,
    path: &str,
    buffers: &mut HashMap<String, String>,
) -> bool {
    match v {
        Value::String(s) => {
            let mut text = s.value().clone();
            let r = ui.add_sized([280.0, 20.0], egui::TextEdit::singleline(&mut text));
            if r.changed() {
                let decor = v.decor().clone();
                *v = Value::from(text);
                *v.decor_mut() = decor;
                return true;
            }
            false
        }
        Value::InlineTable(t) => {
            let mut changed = false;
            let keys: Vec<String> = t.iter().map(|(k, _)| k.to_owned()).collect();
            ui.vertical(|ui| {
                for k in keys {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [110.0, 20.0],
                            egui::Label::new(egui::RichText::new(&k).small()),
                        );
                        if let Some(inner) = t.get_mut(&k) {
                            changed |= value_ui(ui, inner, &format!("{path}.{k}"), buffers);
                        }
                    });
                }
            });
            changed
        }
        _ => {
            let buffer = buffers.entry(path.to_owned()).or_insert_with(|| repr(v));
            let parsed = buffer.parse::<Value>();
            let mut edit = egui::TextEdit::singleline(buffer);
            if parsed.is_err() {
                edit = edit.text_color(egui::Color32::LIGHT_RED);
            }
            let r = ui.add_sized([280.0, 20.0], edit);
            let mut changed = false;
            if r.changed()
                && let Ok(new) = buffer.parse::<Value>()
            {
                let decor = v.decor().clone();
                *v = new;
                *v.decor_mut() = decor;
                changed = true;
            }
            if r.lost_focus() {
                buffers.remove(path);
            }
            changed
        }
    }
}

/// Edits a table: its values (with the fields that can be added), then its
/// arrays of tables; true when changed. `context`: the key of the array it
/// is in (`""`: top level). `only_known`: show only the context's fields
/// (the file's own fields next to the items).
fn table_ui(
    ui: &mut egui::Ui,
    kind: LibraryKind,
    table: &mut Table,
    context: &str,
    path: &str,
    buffers: &mut HashMap<String, String>,
    only_known: bool,
) -> bool {
    let fields = kind.fields(context);
    let help = |k: &str| fields.iter().find(|f| f.0 == k).map(|f| f.2);
    let mut changed = false;
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
    let mut remove: Option<String> = None;
    egui::Grid::new(("table", path))
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for k in &keys {
                if only_known && help(k).is_none() {
                    continue;
                }
                let Some(Item::Value(v)) = table.get_mut(k) else {
                    continue;
                };
                let r = ui.label(k);
                if let Some(h) = help(k) {
                    r.on_hover_text(h);
                }
                changed |= value_ui(ui, v, &format!("{path}.{k}"), buffers);
                if ui
                    .small_button("🗑")
                    .on_hover_text("Remove this field")
                    .clicked()
                {
                    remove = Some(k.clone());
                }
                ui.end_row();
            }
        });
    if let Some(k) = remove {
        table.remove(&k);
        buffers.retain(|p, _| !p.starts_with(&format!("{path}.{k}")));
        changed = true;
    }
    let missing: Vec<&Field> = fields.iter().filter(|f| !table.contains_key(f.0)).collect();
    if !missing.is_empty() {
        let mut add: Option<&Field> = None;
        egui::ComboBox::from_id_salt(("add-field", path))
            .selected_text("add a field…")
            .show_ui(ui, |ui| {
                for f in &missing {
                    if ui.selectable_label(false, f.0).on_hover_text(f.2).clicked() {
                        add = Some(f);
                    }
                }
            });
        if let Some(f) = add
            && let Ok(v) = f.1.parse::<Value>()
        {
            table.insert(f.0, Item::Value(v));
            changed = true;
        }
    }
    if only_known {
        return changed;
    }
    // Arrays of tables (a structure type's attachments, extensions, body).
    let mut arrays: Vec<String> = keys
        .iter()
        .filter(|k| matches!(table.get(k), Some(Item::ArrayOfTables(_))))
        .cloned()
        .collect();
    for a in kind.arrays(context) {
        if !arrays.iter().any(|x| x == a) {
            arrays.push((*a).to_owned());
        }
    }
    for key in arrays {
        let n = table
            .get(&key)
            .and_then(Item::as_array_of_tables)
            .map_or(0, ArrayOfTables::len);
        egui::CollapsingHeader::new(format!("{key} ({n})"))
            .id_salt(("array", path, &key))
            .show(ui, |ui| {
                let mut remove: Option<usize> = None;
                let mut copy: Option<usize> = None;
                if let Some(a) = table.get_mut(&key).and_then(Item::as_array_of_tables_mut) {
                    for (i, t) in a.iter_mut().enumerate() {
                        let title = match (
                            t.get("set").and_then(Item::as_integer),
                            t.get("phase").and_then(Item::as_integer),
                        ) {
                            (Some(s), Some(p)) => format!(
                                "{key} {}: {s}:{p} {}",
                                i + 1,
                                t.get("label").and_then(Item::as_str).unwrap_or("")
                            ),
                            _ => format!(
                                "{key} {}: {}",
                                i + 1,
                                t.get("name").and_then(Item::as_str).unwrap_or("")
                            ),
                        };
                        egui::CollapsingHeader::new(title)
                            .id_salt(("entry", path, &key, i))
                            .show(ui, |ui| {
                                changed |= table_ui(
                                    ui,
                                    kind,
                                    t,
                                    &key,
                                    &format!("{path}.{key}{i}"),
                                    buffers,
                                    false,
                                );
                                ui.horizontal(|ui| {
                                    if ui.small_button("Duplicate").clicked() {
                                        copy = Some(i);
                                    }
                                    if ui.small_button("Remove").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                    }
                }
                if ui.small_button(format!("Add {key}")).clicked() {
                    let mut t = Table::new();
                    for f in kind.fields(&key).iter().take(4) {
                        if let Ok(v) = f.1.parse::<Value>() {
                            t.insert(f.0, Item::Value(v));
                        }
                    }
                    if table.get(&key).and_then(Item::as_array_of_tables).is_none() {
                        table.insert(&key, Item::ArrayOfTables(ArrayOfTables::new()));
                    }
                    if let Some(a) = table.get_mut(&key).and_then(Item::as_array_of_tables_mut) {
                        a.push(t);
                    }
                    changed = true;
                }
                if let Some(a) = table.get_mut(&key).and_then(Item::as_array_of_tables_mut) {
                    if let Some(i) = copy
                        && let Some(t) = a.get(i).cloned()
                    {
                        a.push(t);
                        changed = true;
                    }
                    if let Some(i) = remove {
                        a.remove(i);
                        buffers.retain(|p, _| !p.starts_with(&format!("{path}.{key}")));
                        changed = true;
                    }
                }
            });
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ols-library-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn new_files_of_every_kind_read() {
        for k in LibraryKind::ALL {
            let path = PathBuf::from(format!("x.{}", k.extension()));
            k.check(&path, &k.template("new-one"))
                .unwrap_or_else(|e| panic!("{k:?}: {e}"));
        }
    }

    /// A weather case changed keeps the file's comments; a value that does
    /// not read blocks the save.
    #[test]
    fn edits_keep_comments_and_are_checked() {
        let dir = temp("weather");
        let path = dir.join("cases.olswc");
        std::fs::copy(
            root().join("examples/synthetic-line/weather-cases.olswc"),
            &path,
        )
        .unwrap();
        let sources = Sources {
            file: dir.join("line.ols.json"),
            weather: Some(path.clone()),
            ..Sources::default()
        };
        let mut page = LibraryPage::new(LibraryKind::WeatherCases, &sources);
        assert_eq!(page.file.as_deref(), Some(path.as_path()));
        assert!(page.error.is_none(), "{:?}", page.error);
        assert!(page.items().len() >= 4);
        let set = |page: &mut LibraryPage, v: &str| {
            let doc = page.doc.as_mut().unwrap();
            doc["case"]
                .as_array_of_tables_mut()
                .unwrap()
                .get_mut(0)
                .unwrap()["temperature"] = toml_edit::value(v);
            page.check();
        };
        set(&mut page, "hot");
        assert!(page.error.is_some());
        assert!(page.save().is_err());
        set(&mut page, "12 °C");
        assert!(page.error.is_none(), "{:?}", page.error);
        assert!(page.dirty());
        page.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# Example weather cases"));
        assert!(text.contains("temperature = \"12 °C\""));
        assert!(!page.dirty());
    }

    /// A new wire file, with a second wire next to the first.
    #[test]
    fn new_files_and_items() {
        let dir = temp("wires");
        let sources = Sources {
            file: dir.join("line.ols.json"),
            library: Some(dir.clone()),
            ..Sources::default()
        };
        let mut page = LibraryPage::new(LibraryKind::Cables, &sources);
        assert!(page.files.is_empty());
        page.create(&sources, "my-wires").unwrap();
        assert_eq!(page.files, vec![dir.join("my-wires.olsw")]);
        page.add_item(true);
        assert_eq!(page.items(), vec!["my-wires", "my-wires-1"]);
        assert!(page.error.is_none(), "{:?}", page.error);
        page.save().unwrap();
        let lib = ols_cables::Library::load(&dir).unwrap();
        assert!(lib.get("my-wires-1").is_some());
        assert!(page.create(&sources, "my-wires").is_err(), "exists");
    }
}
