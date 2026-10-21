// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Load recipes and Load table panels: the recipes of the structure
//! (`[[load_case]]`), each change one edit of the file, and the load
//! table — first the line's forces at the attachment points (a placed
//! structure: every case the line transmits, as the line computed them),
//! then the recipes evaluated — with values typed over it kept as
//! overrides. The line's cases can be stored in the structure file as
//! cases of given values (the file then needs no line). Recipes are
//! imported and exported as `.ohlslc` or CSV, the table as CSV. The loads
//! of the case shown are drawn as arrows in the view. A status block at
//! the top of the panels (and of Line values) says what the line gave, or
//! why not, and which cases are incomplete (a section or jumper at the
//! structure not solved there: those cases' rows say "incomplete: S12 not
//! solved").
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
//!
//! **The line's wind on the structure body**: a line case that gives the
//! wind on the structure's own body ([`LineValues::structure_wind`]: the
//! case's own values for the structures, or its weather case's pressure
//! and transverse factor) puts that pressure × factor on every member as
//! surface rows of the case (source line, [`line_wind_recipes`]) and on a
//! pole as its wind — the rule of the line's own structure checks
//! (`ohl_loads::LoadCase::structure_wind`).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use eframe::egui;
use ohl_render::Scene;
use ohl_structure_edit::{self as edit, Geometry, StructureFile};
use ohl_structure_link::{SectionAreas, SectionLibrary};
use ohl_structure_loads::{
    At, LineQuantity, LineValues, LoadCaseRecipe, LoadRecipe, LoadRow, LoadTable, MissingWire,
    Override, Pick, PlacedValues, Source, StructureLoadCases, evaluate_with_areas, incomplete_text,
    resolve_line_pressures,
};

use crate::app::{StructureApp, error_label};

/// Colour of the load arrows.
const ARROW: [u8; 4] = [235, 100, 235, 255];

/// Colour of what is incomplete (a line case with wires not solved).
pub(crate) const INCOMPLETE: egui::Color32 = egui::Color32::from_rgb(255, 120, 100);

/// The recipes as read, the table as evaluated and with the overrides.
#[derive(Debug, Clone)]
struct Evaluated {
    key: u64,
    recipes: Result<Vec<LoadCaseRecipe>, String>,
    evaluated: LoadTable,
    table: LoadTable,
    /// The line's cases in the table: (its name in the table, the line's
    /// case).
    line_cases: Vec<(String, String)>,
    /// What a run solves with: the recipes, then the line cases' wind on
    /// the body, every pressure of the line written in
    /// ([`resolve_line_pressures`]).
    run_recipes: Vec<LoadCaseRecipe>,
}

/// The state of the Load recipes and Load table panels.
pub struct LoadCasesTab {
    /// "Store in the structure file": the line's cases ticked to store.
    pub store: Option<StoreForm>,
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
            store: None,
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
        let at = ohl_structure_loads::overrides_path(path, self.placed.as_deref());
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
        let at = ohl_structure_loads::overrides_path(path, self.placed.as_deref());
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
        let names: Vec<String> = cases.iter().map(|c| c.name.clone()).collect();
        let order = self.info.as_ref().map_or(&[][..], |i| &i.cases[..]);
        let missing = self.info.as_ref().map_or(&[][..], |i| &i.missing[..]);
        let (mut evaluated, line_cases) = line_rows(&self.line, order, &names, missing);
        // The line cases' wind on the body: on the members (rows) when
        // there are some, on a pole (the run) in any case.
        let line_wind = line_wind_recipes(&line_cases, &self.line);
        let mut all: Vec<LoadCaseRecipe> = cases.to_vec();
        if !t.members.is_empty() {
            all.extend(line_wind.iter().cloned());
        }
        // Member areas only when a load needs them (the model is built).
        let surface = all
            .iter()
            .any(|c| c.loads.iter().any(|l| l.members.is_some()));
        let of_recipes = if surface {
            let dir = f.path().and_then(Path::parent);
            let areas = SectionAreas::new(t, &f.text(), dir, &self.sections);
            evaluate_with_areas(&all, t, &self.line, &areas)
        } else {
            evaluate_with_areas(&all, t, &self.line, &ohl_structure_loads::NoAreas)
        };
        let run_recipes = resolve_line_pressures(
            &cases.iter().cloned().chain(line_wind).collect::<Vec<_>>(),
            &self.line,
        );
        evaluated.rows.extend(of_recipes.rows);
        let mut table = evaluated.clone();
        table.apply(&self.overrides);
        self.cache = Some(Evaluated {
            key,
            recipes,
            evaluated,
            table,
            line_cases,
            run_recipes,
        });
    }

    /// What a run solves with (as of the last [`LoadCasesTab::refresh`]):
    /// the recipes and the line cases' wind on the body, the line's
    /// pressures written in.
    pub fn run_recipes(&self) -> &[LoadCaseRecipe] {
        self.cache.as_ref().map_or(&[], |c| &c.run_recipes)
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
                            "no cross-section library at {} (surface loads: no area)",
                            from.display()
                        )
                    } else {
                        format!(
                            "member areas from {} cross-section(s) of {}",
                            lib.sections.len(),
                            from.display()
                        )
                    },
                    false,
                ));
                self.sections = lib;
            }
            Err(e) => {
                self.sections_note = Some((format!("cross-section library: {e}"), true));
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

/// A file key as a label: `line_case` reads "Line case" (the file keeps
/// its keys).
pub(crate) fn readable(key: &str) -> String {
    let words = key.replace('_', " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|f| f.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
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
    /// The case shown and the arrows, at the top of both panels.
    fn loads_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            self.case_choice(ui);
            ui.checkbox(&mut self.loads.arrows, "Arrows")
                .on_hover_text("The loads of the case shown as arrows in the view");
            ui.add(
                egui::Slider::new(&mut self.loads.arrow_scale, 0.01..=5.0)
                    .logarithmic(true)
                    .text("m per kN"),
            );
        });
    }

    /// The Load recipes panel.
    pub fn load_recipes_ui(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            return;
        };
        self.loads.refresh(f);
        self.line_status_ui(ui);
        self.loads_header(ui);
        ui.separator();
        self.recipes_sheet(ui);
    }

    /// The Load table panel.
    pub fn load_table_ui(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            return;
        };
        self.loads.refresh(f);
        self.line_status_ui(ui);
        self.loads_header(ui);
        ui.separator();
        self.table_sheet(ui);
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
        let (mut add, mut import, mut export) = (false, false, false);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("+ Case")
                .on_hover_text("A new load case without loads")
                .clicked()
            {
                add = true;
            }
            if ui
                .button("Import recipes…")
                .on_hover_text(
                    "Load cases from a structure load-case file (.ohlslc) or CSV: a case with \
                     the name of one here replaces it, the others are added",
                )
                .clicked()
            {
                import = true;
            }
            if ui
                .button("Export recipes…")
                .on_hover_text("The load cases as a structure load-case file (.ohlslc) or CSV")
                .clicked()
            {
                export = true;
            }
        });
        if add {
            self.add_case();
            return;
        }
        if import {
            self.import_recipes_dialog();
            return;
        }
        if export {
            self.export_recipes_dialog();
        }
        let Some(f) = &mut self.file else {
            return;
        };
        if let Err(e) = self.loads.recipes() {
            error_label(ui, format!("the load cases do not read: {e}"));
        }
        // As the buttons above left them.
        let cases = edit::load_cases(f.doc());
        if cases.is_empty() {
            let line = self.loads.line_cases();
            ui.label(
                "This structure has no load cases of its own (recipes) yet. The line's forces \
                 are in the load table without any; a recipe makes a case of its own: a value \
                 the line gives (or one of its own), a factor, a one-sided share, the points \
                 or members it acts on. + Case adds one, Import recipes… takes them from \
                 another structure.",
            );
            if ui
                .add_enabled(
                    !line.is_empty(),
                    egui::Button::new("Take the line's load cases as they are"),
                )
                .on_hover_text(
                    "One recipe per load case of the line: its factored transverse, \
                     longitudinal and vertical loads at every point (one undo step)",
                )
                .on_disabled_hover_text("The line gives no load case (see above)")
                .clicked()
            {
                self.take_line_cases();
            }
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
                    ui.label(readable(field))
                        .on_hover_text(format!("{tip} (in the file: {field})"));
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
                        ui.strong(readable(h))
                            .on_hover_text(format!("{tip} (in the file: {h})"));
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
        let (mut import, mut export, mut store) = (false, false, false);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.loads.line_cases().is_empty(),
                    egui::Button::new("Store in the structure file…"),
                )
                .on_hover_text(
                    "The line's cases (ticked) written into the structure file as cases of \
                     given values: the file then has its forces without the line (exports, \
                     design elsewhere); one undo step",
                )
                .on_disabled_hover_text("No line values: nothing to store")
                .clicked()
            {
                store = true;
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
            {
                import = true;
            }
            if ui
                .button("Export table…")
                .on_hover_text("The load table as CSV (forces in N, vertical down)")
                .clicked()
            {
                export = true;
            }
        });
        if store {
            self.open_store();
        }
        if import {
            self.import_table_dialog();
        }
        if export {
            self.export_table_dialog();
        }
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
        let line_names: Vec<String> = self.loads.line_table_names();
        // A line case's source: a load case, or a weather case sent on its
        // own.
        let line_from: std::collections::BTreeMap<String, &'static str> = self
            .loads
            .line_cases()
            .into_iter()
            .map(|(shown, case)| {
                let weather = self.loads.info.as_ref().is_some_and(|i| {
                    i.source(&case) == ohl_structure_loads::CaseSource::WeatherCase
                });
                (
                    shown,
                    if weather {
                        "line weather case"
                    } else {
                        "line load case"
                    },
                )
            })
            .collect();
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
                            (
                                "from",
                                "line load case / line weather case (sent on its own, factors 1): \
                                 the line's forces as it computed them; recipe: a case of the \
                                 structure file",
                            ),
                            ("load", "The recipe's load (none: a row of its own)"),
                            ("at", "point, member (a surface load, its total) or joint"),
                            ("label", ""),
                            ("transverse", "kN, to the right of the line"),
                            ("longitudinal", "kN, ahead"),
                            ("vertical", "kN, down"),
                            ("source", "line, recipe or typed"),
                            (
                                "area",
                                "A surface load: the member's area it acts on (the cross-section \
                                 library's profile width for the direction × length), m²",
                            ),
                            (
                                "evaluated",
                                "Typed over an evaluated value: the evaluated one, kN",
                            ),
                            ("why none / note", ""),
                            ("", ""),
                        ] {
                            ui.strong(readable(h)).on_hover_text(tip);
                        }
                        ui.end_row();
                        for r in &rows {
                            ui.label(&r.case);
                            ui.label(if let Some(from) = line_from.get(&r.case) {
                                *from
                            } else if line_names.contains(&r.case) {
                                "line load case"
                            } else if r.load.is_some() {
                                "recipe"
                            } else {
                                "typed"
                            });
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
                                source.color(ui.visuals().warn_fg_color)
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
                                Some(m) => ui.colored_label(ui.visuals().warn_fg_color, m),
                                None if r.note.starts_with("incomplete:") => {
                                    ui.colored_label(INCOMPLETE, &r.note).on_hover_text(
                                        "A section or jumper at this structure did not solve in \
                                         this case of the line: its forces are missing, the \
                                         values are too low",
                                    )
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

/// Takes the recipes of a `.ohlslc` (or CSV) file into the structure: one
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

/// Writes the recipes as a `.ohlslc` file, or CSV (by the extension).
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

/// How the description of a case stored from the line starts.
pub const STORED_FROM_LINE: &str = "Stored from the line";

/// The line's forces as rows of the load table: per line case (in the
/// line's order, then any other) and attachment point, the transverse,
/// longitudinal and vertical forces as the line computed them — factored
/// where the line factors the case. A line case with the name of a
/// recipe (`recipes`) is called "NAME (line)". A case with wires at the
/// structure not solved (`missing`) has its rows' note start with
/// `incomplete: S12 not solved` (its forces lack theirs). The rows and
/// the line's cases as (name in the table, line case).
pub fn line_rows(
    line: &LineValues,
    order: &[String],
    recipes: &[String],
    missing: &[MissingWire],
) -> (LoadTable, Vec<(String, String)>) {
    let mut cases: Vec<String> = order
        .iter()
        .filter(|c| line.values.iter().any(|v| &v.case == *c))
        .cloned()
        .collect();
    for v in &line.values {
        if !cases.contains(&v.case) {
            cases.push(v.case.clone());
        }
    }
    let mut table = LoadTable::default();
    let mut names = Vec::new();
    for c in cases {
        let name = if recipes.contains(&c) {
            format!("{c} (line)")
        } else {
            c.clone()
        };
        for p in &line.points {
            let get = |q| line.get(&c, &p.label, &p.sets, q);
            let factored = [
                LineQuantity::TransverseFactored,
                LineQuantity::LongitudinalFactored,
                LineQuantity::VerticalFactored,
            ]
            .map(get);
            let plain = [
                LineQuantity::Transverse,
                LineQuantity::Longitudinal,
                LineQuantity::Vertical,
            ]
            .map(get);
            let is_factored = factored.iter().any(Option::is_some);
            let force = if is_factored { factored } else { plain };
            if force.iter().all(Option::is_none) {
                continue;
            }
            let mut note = Vec::new();
            if let Some(i) = incomplete_text(missing, &c) {
                note.push(i);
            }
            if !p.sets.is_empty() {
                note.push(format!(
                    "sets {}",
                    p.sets
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !p.phases.is_empty() {
                note.push(format!("phases {}", p.phases.join(", ")));
            }
            if is_factored {
                if plain.iter().any(Option::is_some) {
                    note.push(format!(
                        "factored (unfactored {} kN)",
                        plain
                            .map(|v| v.map_or("—".into(), |v| format!("{:.3}", v / 1000.0)))
                            .join("; ")
                    ));
                } else {
                    note.push("factored".into());
                }
            } else {
                note.push("unfactored".into());
            }
            table.rows.push(LoadRow {
                case: name.clone(),
                load: None,
                at: At::Point,
                label: p.label.clone(),
                force: force.map(|v| v.unwrap_or(0.0)),
                source: Source::Line,
                evaluated: None,
                missing: None,
                note: note.join("; "),
                area: None,
            });
        }
        names.push((name, c));
    }
    (table, names)
}

/// The note of the line cases' wind rows.
pub const LINE_WIND_NOTE: &str = "the line case's wind on the structure body";

/// The line cases' wind on the structure body as recipes of their own:
/// per line case in the table (`line_cases`: its name there, the line's
/// case) whose wind on the body ([`LineValues::structure_wind`]) has a
/// pressure, a case of that name taking `structure_wind_factored` on
/// every member (strength and self-weight factors 1, as the line's cases
/// in the table). Evaluated, its rows join the line's forces of the case;
/// in a run, its pressure is also the wind on a pole.
pub fn line_wind_recipes(
    line_cases: &[(String, String)],
    line: &LineValues,
) -> Vec<LoadCaseRecipe> {
    line_cases
        .iter()
        .filter(|(_, case)| {
            line.structure_wind(case)
                .is_some_and(|w| w.pressure * w.factor != 0.0)
        })
        .map(|(name, case)| {
            let mut r = LoadCaseRecipe::new(name);
            r.description = LINE_WIND_NOTE.to_owned();
            r.line_case = Some(case.clone());
            r.loads.push(LoadRecipe {
                what: Some(LineQuantity::StructureWindFactored),
                members: Some(Pick::All),
                note: LINE_WIND_NOTE.to_owned(),
                ..LoadRecipe::default()
            });
            r
        })
        .collect()
}

/// The date of `secs` since 1970 (UTC), as YYYY-MM-DD.
fn date(secs: i64) -> String {
    // Days to the civil date (Howard Hinnant's algorithm).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// "Store in the structure file": the line's cases, each ticked or not.
#[derive(Debug, Clone, Default)]
pub struct StoreForm {
    /// (name in the table, line case, ticked).
    pub cases: Vec<(String, String, bool)>,
    /// Stored cases of these names are there: replace them (asked).
    pub replace: Vec<String>,
}

/// How a status reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Values there.
    Good,
    /// Nothing wrong, nothing there.
    Info,
    /// Something to do.
    Warn,
}

impl LoadCasesTab {
    /// The line's cases with values, in the table: (name in the table,
    /// line case).
    pub fn line_cases(&self) -> Vec<(String, String)> {
        self.cache
            .as_ref()
            .map(|c| c.line_cases.clone())
            .unwrap_or_default()
    }

    /// The names of the line's cases in the table.
    pub fn line_table_names(&self) -> Vec<String> {
        self.line_cases().into_iter().map(|(n, _)| n).collect()
    }

    /// `incomplete: S12 not solved, …` when case `case` of the table (its
    /// name there) is a line case whose values lack a wire not solved at
    /// the structure (none: complete, or not a line case).
    pub fn incomplete(&self, case: &str) -> Option<String> {
        let info = self.info.as_ref()?;
        let line_case = self
            .line_cases()
            .into_iter()
            .find(|(n, _)| n == case)
            .map_or_else(|| case.to_owned(), |(_, c)| c);
        info.incomplete(&line_case)
    }

    /// What the line gave, or why not, in plain words (each line a
    /// sentence), and how it reads.
    pub fn line_status(&self) -> Vec<(String, Tone)> {
        let id = self
            .placed
            .clone()
            .or(self.info.as_ref().map(|i| i.structure.clone()))
            .unwrap_or_default();
        if let Some(e) = &self.values_error {
            return vec![(
                format!(
                    "The line's values for {id} do not read: {e}. Edit… on the placed structure \
                     in OHL-Design writes them again."
                ),
                Tone::Warn,
            )];
        }
        let Some(info) = &self.info else {
            return vec![if id.is_empty() {
                (
                    "No line values: open this from a placed structure (select it in the line, \
                     Properties › Open in OHL-Structure…) to get its forces."
                        .to_owned(),
                    Tone::Info,
                )
            } else {
                (
                    format!(
                        "Waiting for the line's values of {id}: OHL-Design writes \
                         them when it has solved the line."
                    ),
                    Tone::Info,
                )
            }];
        };
        let values = &info.values;
        if values.values.is_empty() {
            let note = info.note.clone().unwrap_or_default();
            if note.contains("being computed") {
                return vec![(
                    format!("The line's values for {id} are being computed…"),
                    Tone::Info,
                )];
            }
            let why = note
                .strip_prefix("no values from the line: ")
                .unwrap_or(&note)
                .to_owned();
            let text = if note.contains("no load case file") {
                format!(
                    "The line gives no values for {id}: the project has no load cases. Add \
                     them in OHL-Design (Criteria › Load cases library, or a design-code \
                     plugin); the values come here when the line is solved again."
                )
            } else if !why.is_empty() {
                format!("The line gives no values for {id}: {why}")
            } else if values.points.is_empty() {
                format!("The line gives no values for {id}: no wire hangs on it.")
            } else {
                format!("The line gives no values for {id}.")
            };
            return vec![(text, Tone::Warn)];
        }
        let line = self.line_cases();
        let mut out = vec![(
            format!(
                "The line gives the forces at {} point(s) of {id} in {} case(s): {} — the load \
                 table's first cases.",
                values.points.len(),
                line.len(),
                line.iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Tone::Good,
        )];
        let without: Vec<&str> = info
            .cases
            .iter()
            .filter(|c| !values.values.iter().any(|v| &v.case == *c))
            .map(String::as_str)
            .collect();
        if !without.is_empty() {
            out.push((
                format!(
                    "No values in case(s) {}: the line's sections there are not solved in that \
                     case (solve the line again in OHL-Design).",
                    without.join(", ")
                ),
                Tone::Warn,
            ));
        }
        // Cases whose values lack a wire not solved: incomplete.
        for case in info.incomplete_cases() {
            let wires: Vec<String> = info
                .missing_in(case)
                .map(ohl_structure_loads::MissingWire::describe)
                .collect();
            out.push((
                format!(
                    "Case {case} incomplete: {} — the line's forces at {id} lack its wires, \
                     the values (and the results of the case) are too low; not checked.",
                    wires.join("; ")
                ),
                Tone::Warn,
            ));
        }
        out
    }
}

impl StructureApp {
    /// The status block at the top of the load panels: what the line gave
    /// or why not; the cases stored from the line.
    pub fn line_status_ui(&mut self, ui: &mut egui::Ui) {
        let mut lines = self.loads.line_status();
        if let Some(f) = &self.file {
            let stored: Vec<String> = edit::load_cases(f.doc())
                .iter()
                .filter(|c| c.get("description").starts_with(STORED_FROM_LINE))
                .map(|c| c.get("name").to_owned())
                .collect();
            if !stored.is_empty() {
                lines.push((
                    format!(
                        "Stored in this file from the line: {} (as they were when stored).",
                        stored.join(", ")
                    ),
                    Tone::Info,
                ));
            }
        }
        let worst = if lines.iter().any(|l| l.1 == Tone::Warn) {
            Tone::Warn
        } else if lines.iter().any(|l| l.1 == Tone::Good) {
            Tone::Good
        } else {
            Tone::Info
        };
        let stroke = match worst {
            Tone::Warn => egui::Color32::from_rgb(255, 170, 60),
            Tone::Good => egui::Color32::from_rgb(110, 200, 120),
            Tone::Info => ui.visuals().widgets.noninteractive.bg_stroke.color,
        };
        egui::Frame::group(ui.style())
            .stroke(egui::Stroke::new(1.5, stroke))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (text, tone) in lines {
                    let t = egui::RichText::new(text).strong();
                    match tone {
                        Tone::Warn => ui.colored_label(ui.visuals().warn_fg_color, t),
                        _ => ui.label(t),
                    };
                }
            });
    }

    /// Load cases › Add a load case (+ Case).
    pub fn add_case(&mut self) {
        let Some(f) = &mut self.file else {
            return;
        };
        let (k, name) = f.edit("add load case", |doc, _| edit::add_load_case(doc, "case 1"));
        self.loads.recipe = k;
        self.loads.shown = Some(name.clone());
        self.message = Some((format!("added load case \"{name}\""), false));
    }

    /// Load cases › Take the line's load cases as they are: one recipe
    /// per line case, its factored loads at every point and, on a
    /// structure with members, the line case's wind on the body on every
    /// member when the line gives one (one edit).
    pub fn take_line_cases(&mut self) {
        let cases: Vec<String> = self.loads.line_cases().into_iter().map(|c| c.1).collect();
        let windy: Vec<String> = cases
            .iter()
            .filter(|c| {
                self.loads
                    .line
                    .structure_wind(c)
                    .is_some_and(|w| w.pressure * w.factor != 0.0)
            })
            .cloned()
            .collect();
        let Some(f) = &mut self.file else {
            return;
        };
        if cases.is_empty() {
            self.message = Some(("the line gives no load case".into(), true));
            return;
        }
        let members = !f.structure().members.is_empty();
        let written = f.edit("take the line's load cases", |doc, _| {
            let written = edit::take_line_load_cases(doc, &cases);
            if members {
                let new: Vec<String> = written
                    .iter()
                    .filter(|w| windy.contains(w))
                    .cloned()
                    .collect();
                edit::add_body_wind_load(doc, &new);
            }
            written
        });
        self.message = Some(if written.is_empty() {
            (
                "every line case has a recipe of its name already".into(),
                true,
            )
        } else {
            (
                format!("load cases taken from the line: {}", written.join(", ")),
                false,
            )
        });
    }

    /// Import recipes…: a file chosen.
    pub fn import_recipes_dialog(&mut self) {
        let Some(f) = &mut self.file else {
            return;
        };
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Structure load cases", &["ohlslc", "olsslc", "csv"])
            .pick_file()
        {
            self.message = Some(match import_recipes(f, &p) {
                Ok((a, r)) => (
                    format!("imported {a} new and {r} replaced load case(s)"),
                    false,
                ),
                Err(e) => (e, true),
            });
        }
    }

    /// Export recipes…: where chosen.
    pub fn export_recipes_dialog(&mut self) {
        let Some(f) = &self.file else {
            return;
        };
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Structure load cases", &["ohlslc"])
            .add_filter("CSV", &["csv"])
            .set_file_name(format!("{}.ohlslc", f.structure().id))
            .save_file()
        {
            self.message = Some(match export_recipes(f, &p) {
                Ok(()) => (format!("written {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
    }

    /// Import table…: a CSV chosen.
    pub fn import_table_dialog(&mut self) {
        let Some(f) = &self.file else {
            return;
        };
        if let Some(p) = rfd::FileDialog::new()
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
    }

    /// Export table…: where chosen.
    pub fn export_table_dialog(&mut self) {
        let Some(f) = &self.file else {
            return;
        };
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(format!("{}-loads.csv", f.structure().id))
            .save_file()
        {
            self.message = Some(match self.loads.export_table(&p) {
                Ok(()) => (format!("written {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
    }

    /// Store in the structure file…: the window, every line case ticked.
    pub fn open_store(&mut self) {
        if let Some(f) = &self.file {
            self.loads.refresh(f);
        }
        self.loads.store = Some(StoreForm {
            cases: self
                .loads
                .line_cases()
                .into_iter()
                .map(|(n, c)| (n, c, true))
                .collect(),
            replace: Vec::new(),
        });
    }

    /// Stores the line's cases `chosen` (line case names) in the
    /// structure file as cases of given values: the table's forces of
    /// each (values typed over them included), a case of the same name
    /// replaced (one edit). How many were added and replaced.
    pub fn store_line_cases(&mut self, chosen: &[String]) -> Result<(usize, usize), String> {
        let Some(f) = &mut self.file else {
            return Err("no structure open".into());
        };
        self.loads.refresh(f);
        let line = self
            .loads
            .info
            .as_ref()
            .map(|i| i.line.clone())
            .unwrap_or_default();
        let place = self.loads.placed.clone().unwrap_or_default();
        let today = date(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
        );
        let cases: Vec<edit::GivenCase> = self
            .loads
            .line_cases()
            .into_iter()
            .filter(|(_, c)| chosen.contains(c))
            .map(|(name, c)| edit::GivenCase {
                name: c.clone(),
                line_case: c.clone(),
                description: format!(
                    "{STORED_FROM_LINE} {line}, placed structure {place}, on {today}: its \
                     forces at each point (as the line computed them)"
                ),
                forces: self
                    .loads
                    .table()
                    .case(&name)
                    .filter(|r| r.load.is_none() && r.at == At::Point && r.missing.is_none())
                    .map(|r| (r.label.clone(), r.force))
                    .collect(),
                // The line case's wind on the body, as the run takes it.
                wind: self
                    .loads
                    .run_recipes()
                    .iter()
                    .filter(|r| r.name == name && r.description == LINE_WIND_NOTE)
                    .flat_map(|r| &r.loads)
                    .find_map(|l| Some((l.pressure?, l.direction?.as_str().to_owned()))),
            })
            .collect();
        if cases.is_empty() {
            return Err("no line case chosen".into());
        }
        let names: Vec<String> = cases.iter().map(|c| c.name.clone()).collect();
        let r = f.edit(
            &format!("store {} from the line", names.join(", ")),
            |doc, _| edit::store_given_cases(doc, &cases),
        );
        self.message = Some(match &r {
            Ok((a, rep)) => (
                format!(
                    "stored in the structure file: {} ({a} new, {rep} replaced) — save to keep \
                     them",
                    names.join(", ")
                ),
                false,
            ),
            Err(e) => (e.clone(), true),
        });
        r
    }

    /// The "Store in the structure file" window: a tick per line case,
    /// Store (a stored case of the same name: asks first).
    pub(crate) fn store_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.loads.store.take() else {
            return;
        };
        let existing: Vec<String> = self
            .file
            .as_ref()
            .map(|f| {
                edit::load_cases(f.doc())
                    .iter()
                    .map(|c| c.get("name").to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = true;
        let mut store: Option<Vec<String>> = None;
        egui::Window::new("Store in the structure file")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    "The line's cases ticked are written into the structure file as load cases \
                     of given values (a value and direction per point and component): the file \
                     then has these forces without the line. One undo step; Save keeps them.",
                );
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .id_salt("store-cases")
                    .show(ui, |ui| {
                        for (name, case, on) in &mut form.cases {
                            let label = if existing.contains(case) {
                                format!("{name} (replaces the case {case} of the file)")
                            } else {
                                name.clone()
                            };
                            ui.checkbox(on, label);
                        }
                    });
                let chosen: Vec<String> = form
                    .cases
                    .iter()
                    .filter(|c| c.2)
                    .map(|c| c.1.clone())
                    .collect();
                let replaced: Vec<String> = chosen
                    .iter()
                    .filter(|c| existing.contains(c))
                    .cloned()
                    .collect();
                if !form.replace.is_empty() && form.replace != replaced {
                    form.replace.clear();
                }
                if form.replace.is_empty() {
                    if ui
                        .add_enabled(!chosen.is_empty(), egui::Button::new("Store"))
                        .on_disabled_hover_text("Tick a case")
                        .clicked()
                    {
                        if replaced.is_empty() {
                            store = Some(chosen);
                        } else {
                            form.replace = replaced;
                        }
                    }
                } else {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "The file has cases of these names: {}. Replace them?",
                            form.replace.join(", ")
                        ),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Replace and store").clicked() {
                            store = Some(chosen.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            form.replace.clear();
                        }
                    });
                }
            });
        if let Some(chosen) = store {
            let _ = self.store_line_cases(&chosen);
            return;
        }
        if open {
            self.loads.store = Some(form);
        }
    }
}

#[cfg(test)]
mod line_rows_tests {
    use ohl_structure_loads::{LinePoint, LineValue};

    use super::*;

    /// Two line cases at two points: a row each, factored where the line
    /// factors; a line case named as a recipe is called "(line)"; a case
    /// with a wire not solved is marked incomplete on its rows.
    #[test]
    fn the_line_forces_are_rows() {
        let mut line = LineValues::default();
        for (label, set) in [("A1", 1), ("A11", 11)] {
            line.points.push(LinePoint {
                label: label.into(),
                sets: vec![set],
                phases: vec!["L1".into()],
            });
            for (case, q, v) in [
                ("wind", LineQuantity::TransverseFactored, 1500.0),
                ("wind", LineQuantity::Transverse, 1000.0),
                ("ice", LineQuantity::Vertical, 8000.0),
            ] {
                line.values.push(LineValue {
                    case: case.into(),
                    point: Some(label.into()),
                    set: None,
                    quantity: q,
                    value: v,
                });
            }
        }
        let order = ["ice".to_owned(), "wind".to_owned(), "none".to_owned()];
        let missing = [MissingWire {
            case: "ice".into(),
            kind: "section".into(),
            id: "S12".into(),
            reason: "did not converge".into(),
        }];
        let (t, names) = line_rows(&line, &order, &["wind".to_owned()], &missing);
        assert_eq!(
            names,
            [
                ("ice".to_owned(), "ice".to_owned()),
                ("wind (line)".to_owned(), "wind".to_owned())
            ]
        );
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.rows[0].force, [0.0, 0.0, 8000.0]);
        assert_eq!(t.rows[0].source, Source::Line);
        assert!(t.rows[0].note.contains("unfactored"), "{}", t.rows[0].note);
        // The ice case lacks S12: its rows say so first; wind's not.
        assert!(
            t.rows[0].note.starts_with("incomplete: S12 not solved; "),
            "{}",
            t.rows[0].note
        );
        assert!(!t.rows[3].note.contains("incomplete"));
        assert_eq!(t.rows[3].case, "wind (line)");
        assert_eq!(t.rows[3].label, "A11");
        assert_eq!(t.rows[3].force, [1500.0, 0.0, 0.0]);
        assert!(t.rows[3].note.contains("sets 11; phases L1; factored"));
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_792_195_200), "2026-10-17");
    }

    /// The line cases' wind on the body (invented: wind 600 Pa × 1.4 to
    /// −x, still air 0 Pa): a recipe for the windy case only, under its
    /// name in the table, taking `structure_wind_factored` on every
    /// member; resolved for a run, its pressure is 600 × 1.4 = 840 Pa to
    /// −x.
    #[test]
    fn the_line_cases_wind_on_the_body_is_a_case_of_its_own() {
        use ohl_structure_loads::{CaseWind, Direction};
        let mut line = LineValues::default();
        for (case, pressure) in [("wind", 600.0), ("everyday", 0.0)] {
            line.structure_wind.push(CaseWind {
                case: case.into(),
                pressure,
                factor: 1.4,
                towards: Direction::Left,
                origin: String::new(),
            });
        }
        let cases = [
            ("wind (line)".to_owned(), "wind".to_owned()),
            ("everyday".to_owned(), "everyday".to_owned()),
        ];
        let r = line_wind_recipes(&cases, &line);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "wind (line)");
        assert_eq!(r[0].line_case.as_deref(), Some("wind"));
        assert_eq!((r[0].strength, r[0].self_weight), (1.0, 1.0));
        let l = &r[0].loads[0];
        assert_eq!(l.what, Some(LineQuantity::StructureWindFactored));
        assert_eq!(l.members, Some(Pick::All));
        let resolved = resolve_line_pressures(&r, &line);
        let l = &resolved[0].loads[0];
        assert!((l.pressure.unwrap() - 840.0).abs() < 1e-9);
        assert_eq!(l.direction, Some(Direction::Left));
    }
}
