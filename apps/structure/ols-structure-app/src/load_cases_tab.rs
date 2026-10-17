// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Load cases tab: the recipes of the structure (`[[load_case]]`),
//! each change one edit of the file, and the load table evaluated from
//! them, with values typed over it kept as overrides. Recipes are
//! imported and exported as `.olsslc` or CSV, the table as CSV. The loads
//! of the case shown are drawn as arrows in the view.
//!
//! Overrides live in the session and are kept beside the structure file
//! as `<stem>.loads.csv` (the table's CSV, typed rows only) when it is
//! saved, and read back when it is opened; for a placed structure (started
//! from the line application with `--structure`) as
//! `<stem>.<structure>.loads.csv`, which the line application keeps in
//! the project with that placed structure.
//!
//! Surface loads take the members' areas from the section library
//! (`--sections`, else `Sections/` beside the file).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use eframe::egui;
use ols_render::Scene;
use ols_structure_edit::{self as edit, Geometry, StructureFile};
use ols_structure_link::{SectionAreas, SectionLibrary};
use ols_structure_loads::{
    At, LineQuantity, LineValues, LoadCaseRecipe, LoadRow, LoadTable, Override, PlacedValues,
    Source, StructureLoadCases, evaluate_with_areas,
};

use crate::app::{StructureApp, error_label};

/// Colour of the load arrows.
const ARROW: [u8; 4] = [235, 100, 235, 255];

/// The part of the tab shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadSheet {
    /// The recipes.
    #[default]
    Recipes,
    /// The evaluated load table.
    Table,
}

/// The recipes as read, the table as evaluated and with the overrides.
#[derive(Debug, Clone)]
struct Evaluated {
    key: u64,
    recipes: Result<Vec<LoadCaseRecipe>, String>,
    evaluated: LoadTable,
    table: LoadTable,
}

/// The state of the Load cases tab.
pub struct LoadCasesTab {
    /// The part shown.
    pub sheet: LoadSheet,
    /// The recipe shown (its place in the file).
    pub recipe: usize,
    /// The case shown in the table and drawn (none: every case, no arrows).
    pub shown: Option<String>,
    /// The line's values at the placed structure (empty: opened alone).
    pub line: LineValues,
    /// The placed structure the loads are of (`--structure`; none: the
    /// structure on its own).
    pub placed: Option<String>,
    /// The line-values file as last read (its line, cases, other places).
    pub info: Option<PlacedValues>,
    /// Why the line-values file did not read.
    pub values_error: Option<String>,
    /// The section library (member areas for surface loads).
    pub sections: SectionLibrary,
    /// Where it came from, or why it did not read (`true`).
    pub sections_note: Option<(String, bool)>,
    /// Values typed over the table (this session; kept beside the file).
    pub overrides: Vec<Override>,
    /// Overrides changed since the file was saved.
    pub overrides_dirty: bool,
    /// Arrows drawn in the view.
    pub arrows: bool,
    /// Arrow length per kN, m.
    pub arrow_scale: f64,
    /// Changes to the overrides (the table is evaluated again).
    generation: u64,
    cache: Option<Evaluated>,
    /// Fields being typed: (value in the file, text typed).
    buffers: HashMap<String, (String, String)>,
}

impl Default for LoadCasesTab {
    fn default() -> Self {
        Self {
            sheet: LoadSheet::default(),
            recipe: 0,
            shown: None,
            line: LineValues::default(),
            placed: None,
            info: None,
            values_error: None,
            sections: SectionLibrary::default(),
            sections_note: None,
            overrides: Vec::new(),
            overrides_dirty: false,
            arrows: true,
            arrow_scale: 0.5,
            generation: 0,
            cache: None,
            buffers: HashMap::new(),
        }
    }
}

impl LoadCasesTab {
    /// A file opened: its overrides read from beside it (an error said);
    /// the placed structure, its line values and the section library kept.
    pub fn opened(&mut self, path: &Path) -> Result<(), String> {
        let kept = Self {
            placed: self.placed.take(),
            line: std::mem::take(&mut self.line),
            info: self.info.take(),
            values_error: self.values_error.take(),
            sections: std::mem::take(&mut self.sections),
            sections_note: self.sections_note.take(),
            ..Self::default()
        };
        *self = kept;
        let at = ols_structure_loads::overrides_path(path, self.placed.as_deref());
        if !at.exists() {
            return Ok(());
        }
        let text = std::fs::read_to_string(&at).map_err(|e| format!("{}: {e}", at.display()))?;
        let table = LoadTable::from_csv(&text).map_err(|e| format!("{}: {e}", at.display()))?;
        self.overrides = table.rows.iter().map(Override::of).collect();
        Ok(())
    }

    /// The file saved to `path`: the overrides written beside it (when
    /// there are some, or a file of them is there already).
    pub fn saved(&mut self, path: &Path) -> Result<(), String> {
        let at = ols_structure_loads::overrides_path(path, self.placed.as_deref());
        if !self.overrides.is_empty() || at.exists() {
            std::fs::write(&at, Override::table(&self.overrides).to_csv())
                .map_err(|e| format!("{}: {e}", at.display()))?;
        }
        self.overrides_dirty = false;
        Ok(())
    }

    /// Evaluates the recipes again when the file or the overrides changed.
    pub fn refresh(&mut self, f: &StructureFile) {
        let mut h = DefaultHasher::new();
        f.text().hash(&mut h);
        self.generation.hash(&mut h);
        let key = h.finish();
        if self.cache.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let t = f.structure();
        let recipes = StructureLoadCases::from_structure(t);
        let cases = recipes.as_deref().unwrap_or(&[]);
        // Member areas only when a load needs them (the model is built).
        let surface = cases
            .iter()
            .any(|c| c.loads.iter().any(|l| l.members.is_some()));
        let evaluated = if surface {
            let dir = f.path().and_then(Path::parent);
            let areas = SectionAreas::new(t, &f.text(), dir, &self.sections);
            evaluate_with_areas(cases, t, &self.line, &areas)
        } else {
            evaluate_with_areas(cases, t, &self.line, &ols_structure_loads::NoAreas)
        };
        let mut table = evaluated.clone();
        table.apply(&self.overrides);
        self.cache = Some(Evaluated {
            key,
            recipes,
            evaluated,
            table,
        });
    }

    /// The load table (evaluated, the overrides applied), as of the last
    /// [`LoadCasesTab::refresh`].
    pub fn table(&self) -> &LoadTable {
        static EMPTY: LoadTable = LoadTable { rows: Vec::new() };
        self.cache.as_ref().map_or(&EMPTY, |c| &c.table)
    }

    /// The recipes as read (or why they do not read).
    pub fn recipes(&self) -> Result<&[LoadCaseRecipe], &str> {
        match self.cache.as_ref().map(|c| &c.recipes) {
            Some(Ok(r)) => Ok(r),
            Some(Err(e)) => Err(e),
            None => Ok(&[]),
        }
    }

    /// Changes with the file and the overrides: what a result was
    /// computed from.
    pub fn key(&self) -> u64 {
        self.cache.as_ref().map_or(0, |c| c.key)
    }

    /// Types `force` (N) over row `r` (or a row of its own).
    pub fn type_value(&mut self, r: &LoadRow, force: [f64; 3]) {
        match self.overrides.iter_mut().find(|o| o.is_over(r)) {
            Some(o) => o.force = force,
            None => self.overrides.push(Override {
                force,
                ..Override::of(r)
            }),
        }
        self.changed();
    }

    /// Takes back what was typed over row `r`.
    pub fn clear_value(&mut self, r: &LoadRow) {
        self.overrides.retain(|o| !o.is_over(r));
        self.changed();
    }

    fn changed(&mut self) {
        self.generation += 1;
        self.overrides_dirty = true;
    }

    /// The line's values at the placed structure, read (again) from its
    /// line-values file: the table is evaluated with them.
    pub fn set_line_values(&mut self, v: PlacedValues) {
        if self.placed.is_none() && !v.structure.is_empty() {
            self.placed = Some(v.structure.clone());
        }
        self.line = v.values.clone();
        self.info = Some(v);
        self.values_error = None;
        self.generation += 1;
    }

    /// "loads of T2 in line": whose loads these are (none: the structure
    /// on its own).
    pub fn placed_title(&self) -> Option<String> {
        let id = self
            .placed
            .as_deref()
            .or(self.info.as_ref().map(|i| i.structure.as_str()))?;
        let line = self
            .info
            .as_ref()
            .map(|i| i.line.as_str())
            .filter(|l| !l.is_empty());
        Some(match line {
            Some(line) => format!("loads of {id} in {line}"),
            None => format!("loads of {id}"),
        })
    }

    /// The section library the members' areas come from (`from`: where).
    pub fn set_sections(&mut self, r: Result<SectionLibrary, String>, from: &Path) {
        match r {
            Ok(lib) => {
                self.sections_note = Some((
                    if lib.sections.is_empty() {
                        format!(
                            "no section library at {} (surface loads: no area)",
                            from.display()
                        )
                    } else {
                        format!(
                            "member areas from {} section(s) of {}",
                            lib.sections.len(),
                            from.display()
                        )
                    },
                    false,
                ));
                self.sections = lib;
            }
            Err(e) => {
                self.sections_note = Some((format!("section library: {e}"), true));
                self.sections = SectionLibrary::default();
            }
        }
        self.generation += 1;
    }

    /// Reads a load table (CSV): what it has that the evaluation does not
    /// give becomes the overrides (those typed before are dropped).
    pub fn import_table(&mut self, f: &StructureFile, path: &Path) -> Result<usize, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let table = LoadTable::from_csv(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        self.refresh(f);
        let evaluated = &self.cache.as_ref().expect("refreshed").evaluated;
        self.overrides = table.overrides_over(evaluated);
        self.changed();
        self.refresh(f);
        Ok(self.overrides.len())
    }

    /// Writes the load table as CSV.
    pub fn export_table(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.table().to_csv()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The arrows of the case shown: each row's force at its point, joint
    /// or member (its middle), `arrow_scale` m per kN; none when every
    /// case is shown or arrows are off. Its key changes with what it shows.
    pub fn overlay(&self, g: &Geometry) -> Option<(u64, Scene)> {
        let case = self
            .shown
            .as_deref()
            .filter(|c| self.arrows && !c.is_empty())?;
        let mut h = DefaultHasher::new();
        (self.key(), case, self.arrow_scale.to_bits(), g.joints.len()).hash(&mut h);
        let mut s = Scene::new([0.0; 3]);
        for r in self.table().case(case).filter(|r| r.missing.is_none()) {
            let at = match r.at {
                At::Point => g
                    .points
                    .iter()
                    .find(|p| p.0 == r.label)
                    .map(|p| p.1)
                    .or_else(|| g.wires.iter().find(|w| w.0 == r.label).map(|w| w.1)),
                At::Joint => g.joints.iter().find(|j| j.0 == r.label).map(|j| j.1),
                At::Member => g
                    .members
                    .iter()
                    .find(|m| m.0 == r.label)
                    .map(|m| [0, 1, 2].map(|k| (m.1[k] + m.2[k]) / 2.0)),
            };
            if let Some(p) = at {
                // Table axes (vertical down) → structure axes (z up).
                let v = [r.force[0], r.force[1], -r.force[2]].map(|x| x / 1000.0);
                arrow(&mut s, p, v.map(|x| x * self.arrow_scale), ARROW, true);
            }
        }
        Some((h.finish(), s))
    }
}

/// An arrow of vector `v` (m) at `p`: ending there (`to`, a load) or
/// starting there (a reaction).
pub(crate) fn arrow(s: &mut Scene, p: [f64; 3], v: [f64; 3], colour: [u8; 4], to: bool) {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-6 {
        return;
    }
    let (tail, head) = if to {
        ([0, 1, 2].map(|k| p[k] - v[k]), p)
    } else {
        (p, [0, 1, 2].map(|k| p[k] + v[k]))
    };
    s.polyline(&[tail, head], colour, 2.5);
    // Two barbs, in a plane holding the arrow.
    let d = v.map(|x| x / len);
    let side = if d[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let n = [
        d[1] * side[2] - d[2] * side[1],
        d[2] * side[0] - d[0] * side[2],
        d[0] * side[1] - d[1] * side[0],
    ];
    let b = (len * 0.2).min(0.6);
    for sign in [1.0, -1.0] {
        let q = [0, 1, 2].map(|k| head[k] - d[k] * b + n[k] * b * 0.5 * sign);
        s.polyline(&[q, head], colour, 2.5);
    }
}

/// A text field of the file: written (Some) when left with another text.
fn cell(
    ui: &mut egui::Ui,
    buffers: &mut HashMap<String, (String, String)>,
    key: String,
    current: &str,
    width: f32,
    hint: &str,
) -> Option<String> {
    let e = buffers
        .entry(key)
        .or_insert_with(|| (current.to_owned(), current.to_owned()));
    if e.0 != current {
        *e = (current.to_owned(), current.to_owned());
    }
    let r = ui.add_sized(
        [width, 20.0],
        egui::TextEdit::singleline(&mut e.1).hint_text(hint),
    );
    (r.lost_focus() && e.1 != e.0).then(|| e.1.clone())
}

/// A choice among `options` (the first meaning none): the text chosen
/// when it changed.
fn choice(
    ui: &mut egui::Ui,
    id: (usize, usize, &str),
    current: &str,
    options: &[&str],
) -> Option<String> {
    let shown = if current.is_empty() { "—" } else { current };
    let mut chosen = shown.to_owned();
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(if options.len() > 8 { 150.0 } else { 60.0 })
        .show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(&mut chosen, (*o).to_owned(), *o);
            }
        });
    (chosen != shown).then(|| {
        if chosen == "—" {
            String::new()
        } else {
            chosen
        }
    })
}

/// Hover texts of the load columns.
const LOAD_TIPS: [(&str, &str); 13] = [
    (
        "what",
        "A value the line gives at the point (or none: a value or pressure of the case)",
    ),
    (
        "value",
        "A force of the case itself (2 kN; a plain number is N); needs a direction",
    ),
    (
        "pressure",
        "A pressure on the members' area (0.6 kPa; plain: Pa); needs members",
    ),
    (
        "line_case",
        "The line's load case for this load (default: the case's)",
    ),
    ("factor", "Factor (default 1)"),
    (
        "one_sided",
        "A share on one side only (0.4 or 40 %); needs a side",
    ),
    (
        "side",
        "ahead or back: points pulling to the other side are left out",
    ),
    (
        "direction",
        "Structure axes: x right of the line, y ahead, z up",
    ),
    ("points", "all, or labels (separated by ;)"),
    (
        "sets",
        "Or the points carrying these sets (numbers, separated by ;)",
    ),
    (
        "phases",
        "Or the points carrying these phases (the line's labels)",
    ),
    ("members", "all, or member ids: a surface load"),
    ("note", "Free text"),
];

impl StructureApp {
    /// The Load cases tab.
    pub fn load_cases_ui(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            return;
        };
        self.loads.refresh(f);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.loads.sheet, LoadSheet::Recipes, "Recipes")
                .on_hover_text("The structure's load cases: each a list of loads ([[load_case]])");
            ui.selectable_value(&mut self.loads.sheet, LoadSheet::Table, "Load table")
                .on_hover_text(
                    "The recipes evaluated: per case, the loads at each point or member",
                );
            ui.separator();
            self.case_choice(ui);
            ui.checkbox(&mut self.loads.arrows, "Arrows")
                .on_hover_text("The loads of the case shown as arrows in the view");
            ui.add(
                egui::Slider::new(&mut self.loads.arrow_scale, 0.01..=5.0)
                    .logarithmic(true)
                    .text("m per kN"),
            );
        });
        ui.separator();
        match self.loads.sheet {
            LoadSheet::Recipes => self.recipes_sheet(ui),
            LoadSheet::Table => self.table_sheet(ui),
        }
    }

    /// The case shown (in the table and as arrows), or every case.
    fn case_choice(&mut self, ui: &mut egui::Ui) {
        let cases: Vec<String> = self
            .loads
            .table()
            .cases()
            .into_iter()
            .map(str::to_owned)
            .collect();
        // "": every case, as chosen; a case gone: the first.
        let valid = match self.loads.shown.as_deref() {
            Some("") => true,
            Some(c) => cases.iter().any(|x| x == c),
            None => false,
        };
        if !valid {
            self.loads.shown = Some(cases.first().cloned().unwrap_or_default());
        }
        let text = match self.loads.shown.as_deref() {
            None | Some("") => "every case".to_owned(),
            Some(c) => c.to_owned(),
        };
        let mut chosen = self.loads.shown.clone().unwrap_or_default();
        ui.label("Case:");
        egui::ComboBox::from_id_salt("load-case-shown")
            .selected_text(text)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, String::new(), "every case");
                for c in &cases {
                    ui.selectable_value(&mut chosen, c.clone(), c);
                }
            });
        if Some(&chosen) != self.loads.shown.as_ref() {
            if let Some(k) = self
                .loads
                .recipes()
                .ok()
                .and_then(|r| r.iter().position(|c| c.name == chosen))
            {
                self.loads.recipe = k;
            }
            self.loads.shown = Some(chosen);
        }
    }

    fn recipes_sheet(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &mut self.file else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("+ Case")
                .on_hover_text("A new load case without loads")
                .clicked()
            {
                let (k, name) =
                    f.edit("add load case", |doc, _| edit::add_load_case(doc, "case 1"));
                self.loads.recipe = k;
                self.loads.shown = Some(name.clone());
                self.message = Some((format!("added load case \"{name}\""), false));
            }
            if ui
                .button("Import recipes…")
                .on_hover_text(
                    "Load cases from a structure load-case file (.olsslc) or CSV: a case with \
                     the name of one here replaces it, the others are added",
                )
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Structure load cases", &["olsslc", "csv"])
                    .pick_file()
            {
                let r = import_recipes(f, &p);
                self.message = Some(match r {
                    Ok((a, r)) => (
                        format!("imported {a} new and {r} replaced load case(s)"),
                        false,
                    ),
                    Err(e) => (e, true),
                });
            }
            if ui
                .button("Export recipes…")
                .on_hover_text("The load cases as a structure load-case file (.olsslc) or CSV")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Structure load cases", &["olsslc"])
                    .add_filter("CSV", &["csv"])
                    .set_file_name(format!("{}.olsslc", f.structure().id))
                    .save_file()
            {
                self.message = Some(match export_recipes(f, &p) {
                    Ok(()) => (format!("written {}", p.display()), false),
                    Err(e) => (e, true),
                });
            }
        });
        if let Err(e) = self.loads.recipes() {
            error_label(ui, format!("the load cases do not read: {e}"));
        }
        // As the buttons above left them.
        let cases = edit::load_cases(f.doc());
        if cases.is_empty() {
            ui.label(
                "No load case yet: + Case adds one, Import recipes… takes them from another \
                 structure. A case is a list of loads: a value the line gives (or one of its \
                 own), a factor, a one-sided share, the points or members it acts on.",
            );
            return;
        }
        let k = self.loads.recipe.min(cases.len() - 1);
        self.loads.recipe = k;
        let case = &cases[k];
        let mut pick = k;
        ui.horizontal_wrapped(|ui| {
            ui.label("Recipe:");
            egui::ComboBox::from_id_salt("recipe")
                .selected_text(case.get("name"))
                .show_ui(ui, |ui| {
                    for (i, c) in cases.iter().enumerate() {
                        ui.selectable_value(&mut pick, i, c.get("name"));
                    }
                });
            if ui
                .button("+ Load")
                .on_hover_text(
                    "A new load in this case (the line's vertical load at every point, to change)",
                )
                .clicked()
            {
                let name = case.get("name").to_owned();
                match f.edit(&format!("add a load to {name}"), |doc, _| {
                    edit::add_load(doc, k)
                }) {
                    Ok(n) => self.message = Some((format!("{name}: load {} added", n + 1), false)),
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if ui
                .button("🗑 Case")
                .on_hover_text("Delete this load case with its loads (Undo takes it back)")
                .clicked()
            {
                let name = case.get("name").to_owned();
                f.edit(&format!("delete load case {name}"), |doc, _| {
                    edit::remove_load_case(doc, k);
                });
                self.loads.recipe = k.saturating_sub(1);
                self.message = Some((format!("deleted load case \"{name}\""), false));
            }
        });
        if pick != k {
            self.loads.recipe = pick;
            self.loads.shown = Some(cases[pick].get("name").to_owned());
        }
        let buffers = &mut self.loads.buffers;
        let mut written: Option<(String, Result<(), String>)> = None;
        egui::Grid::new("case-fields")
            .num_columns(4)
            .show(ui, |ui| {
                for (n, field) in edit::CASE_FIELDS.iter().enumerate() {
                    let tip = match *field {
                        "name" => "Unique; model is the name of the [[load]] tables' case",
                        "description" => "What it stands for",
                        "line_case" => "The line's load case whose values the loads take",
                        "strength" => "Strength factor of the checks (default 1)",
                        _ => "Factor on the structure's own weight (default 1; 0: none)",
                    };
                    ui.label(*field).on_hover_text(tip);
                    let (w, hint) = match *field {
                        "description" => (360.0, ""),
                        "strength" | "self_weight" => (140.0, "1"),
                        _ => (140.0, ""),
                    };
                    if let Some(typed) = cell(
                        ui,
                        buffers,
                        format!("case/{k}/{field}"),
                        case.get(field),
                        w,
                        hint,
                    ) {
                        let r = f.edit(&format!("{} of {}", field, case.get("name")), |doc, _| {
                            edit::set_load_case_field(doc, k, field, &typed)
                        });
                        if *field == "name" && r.is_ok() {
                            self.loads.shown = Some(typed.trim().to_owned());
                        }
                        written = Some((field.to_string(), r));
                    }
                    if n % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        ui.separator();
        let mut drop: Option<usize> = None;
        let quantities: Vec<&str> = std::iter::once("—")
            .chain(LineQuantity::ALL.iter().map(|q| q.as_str()))
            .collect();
        egui::ScrollArea::both()
            .id_salt("loads")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("loads").striped(true).show(ui, |ui| {
                    ui.strong("#");
                    for (h, tip) in LOAD_TIPS {
                        ui.strong(h).on_hover_text(tip);
                    }
                    ui.end_row();
                    for l in 0..case.loads.len() {
                        ui.label(format!("{}", l + 1));
                        for field in edit::LOAD_FIELDS {
                            let current = case.load(l, field);
                            let typed = match field {
                                "what" => choice(ui, (k, l, field), current, &quantities),
                                "side" => {
                                    choice(ui, (k, l, field), current, &["—", "ahead", "back"])
                                }
                                "direction" => choice(
                                    ui,
                                    (k, l, field),
                                    current,
                                    &["—", "x", "-x", "y", "-y", "z", "-z"],
                                ),
                                "note" => cell(
                                    ui,
                                    buffers,
                                    format!("load/{k}/{l}/{field}"),
                                    current,
                                    160.0,
                                    "",
                                ),
                                "factor" => cell(
                                    ui,
                                    buffers,
                                    format!("load/{k}/{l}/{field}"),
                                    current,
                                    50.0,
                                    "1",
                                ),
                                _ => cell(
                                    ui,
                                    buffers,
                                    format!("load/{k}/{l}/{field}"),
                                    current,
                                    70.0,
                                    "",
                                ),
                            };
                            if let Some(typed) = typed {
                                let r = f.edit(
                                    &format!("{} of load {} of {}", field, l + 1, case.get("name")),
                                    |doc, _| edit::set_load_field(doc, k, l, field, &typed),
                                );
                                written = Some((field.to_owned(), r));
                            }
                        }
                        if ui
                            .small_button("🗑")
                            .on_hover_text(format!("Delete load {}", l + 1))
                            .clicked()
                        {
                            drop = Some(l);
                        }
                        ui.end_row();
                    }
                });
                if case.loads.is_empty() {
                    ui.label("No load yet: + Load adds one.");
                }
            });
        if let Some(l) = drop {
            f.edit(
                &format!("delete load {} of {}", l + 1, case.get("name")),
                |doc, _| {
                    edit::remove_load(doc, k, l);
                },
            );
        }
        match written {
            Some((field, Err(e))) => self.message = Some((format!("{field}: {e}"), true)),
            Some((_, Ok(()))) => self.message = None,
            None => {}
        }
    }

    fn table_sheet(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            if self.loads.line.values.is_empty() {
                let text = match &self.loads.placed {
                    None => {
                        "Opened on its own: the line gives no values, so loads that take one \
                         show \"no line value\". Type values in the table or import one."
                    }
                    Some(_) => {
                        "No values from the line (yet): see In the line. Loads that take one \
                         show \"no line value\"."
                    }
                };
                ui.label(egui::RichText::new(text).weak());
            }
            if let Some((note, error)) = &self.loads.sections_note {
                let text = egui::RichText::new(note).weak().small();
                if *error {
                    error_label(ui, text);
                } else {
                    ui.label(text);
                }
            }
            if ui
                .button("Import table…")
                .on_hover_text(
                    "A load table (CSV, the columns of Export table): what it has beyond the \
                     evaluation is kept as typed values",
                )
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .pick_file()
            {
                self.message = Some(match self.loads.import_table(f, &p) {
                    Ok(n) => (
                        format!("imported {}: {n} typed value(s)", p.display()),
                        false,
                    ),
                    Err(e) => (e, true),
                });
            }
            if ui
                .button("Export table…")
                .on_hover_text("The load table as CSV (forces in N, vertical down)")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name(format!("{}-loads.csv", f.structure().id))
                    .save_file()
            {
                self.message = Some(match self.loads.export_table(&p) {
                    Ok(()) => (format!("written {}", p.display()), false),
                    Err(e) => (e, true),
                });
            }
        });
        let shown = self.loads.shown.clone().filter(|s| !s.is_empty());
        let rows: Vec<LoadRow> = self
            .loads
            .table()
            .rows
            .iter()
            .filter(|r| shown.as_ref().is_none_or(|c| &r.case == c))
            .cloned()
            .collect();
        let missing = rows.iter().filter(|r| r.missing.is_some()).count();
        ui.label(format!(
            "{} row(s), {} typed, {missing} without a value — forces in kN, transverse to the \
             right of the line, longitudinal ahead, vertical down",
            rows.len(),
            rows.iter().filter(|r| r.source == Source::Typed).count(),
        ));
        let mut typed: Option<(LoadRow, [f64; 3])> = None;
        let mut cleared: Option<LoadRow> = None;
        egui::ScrollArea::both()
            .id_salt("load-table")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("load-table-grid")
                    .striped(true)
                    .show(ui, |ui| {
                        for (h, tip) in [
                            ("case", ""),
                            ("load", "The recipe's load (none: a row of its own)"),
                            ("at", "point, member (a surface load, its total) or joint"),
                            ("label", ""),
                            ("transverse", "kN, to the right of the line"),
                            ("longitudinal", "kN, ahead"),
                            ("vertical", "kN, down"),
                            ("source", "line, recipe or typed"),
                            (
                                "area",
                                "A surface load: the member's area it acts on (the section \
                                 library's profile width for the direction × length), m²",
                            ),
                            (
                                "evaluated",
                                "Typed over an evaluated value: the evaluated one, kN",
                            ),
                            ("why none / note", ""),
                            ("", ""),
                        ] {
                            ui.strong(h).on_hover_text(tip);
                        }
                        ui.end_row();
                        for r in &rows {
                            ui.label(&r.case);
                            ui.label(r.load.map_or(String::new(), |l| l.to_string()));
                            ui.label(r.at.as_str());
                            ui.label(&r.label);
                            let mut kn = r.force.map(|x| x / 1000.0);
                            let mut changed = false;
                            for v in &mut kn {
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(v)
                                            .speed(0.01)
                                            .max_decimals(3)
                                            .suffix(" kN"),
                                    )
                                    .changed();
                            }
                            if changed {
                                typed = Some((r.clone(), kn.map(|x| x * 1000.0)));
                            }
                            let source = egui::RichText::new(if r.missing.is_some() {
                                "—"
                            } else {
                                r.source.as_str()
                            });
                            ui.label(if r.source == Source::Typed {
                                source.color(egui::Color32::YELLOW)
                            } else {
                                source
                            });
                            ui.label(r.area.map_or(String::new(), |a| format!("{a:.4} m²")));
                            ui.label(r.evaluated.map_or(String::new(), |e| {
                                format!(
                                    "{:.3}; {:.3}; {:.3}",
                                    e[0] / 1000.0,
                                    e[1] / 1000.0,
                                    e[2] / 1000.0
                                )
                            }));
                            match &r.missing {
                                Some(m) => {
                                    ui.colored_label(egui::Color32::from_rgb(255, 170, 60), m)
                                }
                                None => ui.label(&r.note),
                            };
                            let is_typed = self.loads.overrides.iter().any(|o| o.is_over(r));
                            if is_typed
                                && ui
                                    .small_button("⟲")
                                    .on_hover_text(
                                        "Take back the value typed: the evaluated one again",
                                    )
                                    .clicked()
                            {
                                cleared = Some(r.clone());
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some((r, force)) = typed {
            self.loads.type_value(&r, force);
        }
        if let Some(r) = cleared {
            self.loads.clear_value(&r);
        }
    }
}

/// Takes the recipes of a `.olsslc` (or CSV) file into the structure: one
/// undo step. How many cases were added and replaced.
pub fn import_recipes(f: &mut StructureFile, path: &Path) -> Result<(usize, usize), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let toml = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
    {
        let cases = StructureLoadCases::cases_from_csv(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        StructureLoadCases::new("", cases).to_toml()
    } else {
        StructureLoadCases::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        text
    };
    f.edit("import load cases", |doc, _| {
        edit::merge_load_cases(doc, &toml)
    })
}

/// Writes the recipes as a `.olsslc` file, or CSV (by the extension).
pub fn export_recipes(f: &StructureFile, path: &Path) -> Result<(), String> {
    let t = f.structure();
    let cases = StructureLoadCases::from_structure(t)
        .map_err(|e| format!("the load cases do not read: {e}"))?;
    let text = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
    {
        StructureLoadCases::cases_to_csv(&cases)
    } else {
        StructureLoadCases::new(&format!("{} load cases", t.name), cases).to_toml()
    };
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
