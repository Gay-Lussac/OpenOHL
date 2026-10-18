// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structures › New pylon from a template…: a pylon template's face (its
//! dimensions typed, drawn as they change) built into a structure type of
//! level 1, for suspension or strain (the toggle: each role is a type of
//! its own); a type built before is built again from what it records,
//! keeping what the template does not make ([`rebuilt`]).
//! Structures › Place on alignment points…: a type stood on many points
//! of an alignment at once, along the line, in one command.
//!
//! The templates are those shipped (`libraries/pylon-templates`) and the
//! project's own (`Pylon templates` next to the line): the library grows
//! with files.

use std::collections::BTreeSet;
use std::path::PathBuf;

use eframe::egui;
use ols_structures::LibraryItem;
use ols_structures::templates::{PylonTemplate, Role, Values};
use toml_edit::{DocumentMut, Item, Table};

use crate::model::Model;

/// The folder of a project's own pylon templates.
pub fn project_templates(model: &Model) -> PathBuf {
    crate::model::model_dir(&model.sources).join("Pylon templates")
}

/// The pylon templates: shipped, then the project's (a project's template
/// of the same id is used instead of the shipped one). What could not be
/// read is said.
pub fn templates(model: &Model) -> (Vec<PylonTemplate>, Vec<String>) {
    let mut out: Vec<PylonTemplate> = Vec::new();
    let mut errors = Vec::new();
    for dir in [
        ols_core::files::shipped_library("pylon-templates"),
        project_templates(model),
    ] {
        for path in ols_core::files::files_with_extension(&dir, PylonTemplate::EXTENSION) {
            let read = std::fs::read_to_string(&path)
                .map_err(|e| format!("{}: {e}", path.display()))
                .and_then(|t| PylonTemplate::parse(&path, &t).map_err(|e| e.to_string()));
            match read {
                Ok(t) => {
                    out.retain(|x| x.id != t.id);
                    out.push(t);
                }
                Err(e) => errors.push(e),
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    (out, errors)
}

/// The keys of a structure type its template writes: built again, they
/// are the new ones.
const TEMPLATE_KEYS: [&str; 8] = [
    "format", "version", "id", "name", "kind", "height", "source", "template",
];

/// The lists the template fills, by label: built again, its own entries
/// are replaced, others kept.
const MADE: [&str; 3] = ["point", "wire", "string"];

/// The entries of list `key` of a type's file (`[[key]]`).
fn entries<'a>(doc: &'a Item, key: &str) -> impl Iterator<Item = &'a Table> {
    doc.get(key)
        .and_then(Item::as_array_of_tables)
        .into_iter()
        .flatten()
}

/// The labels of the points, wire points and strings of a type's text.
fn made_labels(text: &str) -> Result<BTreeSet<String>, String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    Ok(MADE
        .iter()
        .flat_map(|k| entries(doc.as_item(), k))
        .filter_map(|t| Some(t.get("label")?.as_str()?.to_owned()))
        .collect())
}

/// Every table in `item` (nested ones too), in the order they are written.
fn tables_mut(item: &mut Item, f: &mut dyn FnMut(&mut Table)) {
    let table = |t: &mut Table, f: &mut dyn FnMut(&mut Table)| {
        f(t);
        for (_, v) in t.iter_mut() {
            tables_mut(v, f);
        }
    };
    match item {
        Item::Table(t) => table(t, f),
        Item::ArrayOfTables(a) => {
            for t in a.iter_mut() {
                table(t, f);
            }
        }
        _ => {}
    }
}

/// Structure type `existing` built again from `template` as `built`:
/// what the template makes (the header, `[template]`, and the points,
/// wire points and strings it makes now or made before, by label) comes
/// from `built`; everything else of `existing` is kept — joints, members,
/// sections, materials, capacity, bodies, extensions, points and strings
/// added by hand. Also: what to look at — kept entries that refer to a
/// point or wire point no longer built (they are kept, not dropped), and
/// template points that were tied to a joint or member. A result that does
/// not read as a type is refused with those notes.
pub fn rebuilt(
    existing: &str,
    built: &str,
    template: &PylonTemplate,
) -> Result<(String, Vec<String>), String> {
    let old = ols_structures::StructureType::parse(std::path::Path::new("type"), existing)
        .map_err(|e| format!("the type's file: {}", e.message))?;
    let now = made_labels(built)?;
    let mut made = now.clone();
    if let Some((id, role, _)) = old
        .template
        .as_ref()
        .and_then(ols_structures::templates::recorded)
        && id == template.id
    {
        // The labels do not depend on the dimensions: the defaults do.
        let before = template.build(&template.defaults(role), role, "x", "x")?;
        made.extend(made_labels(&before)?);
    }
    let mut doc: DocumentMut = built.parse().map_err(|e| format!("{e}"))?;
    let from: DocumentMut = existing
        .parse()
        .map_err(|e| format!("the type's file: {e}"))?;
    // Kept tables go after the built ones, in their order.
    let mut next = 0;
    for (_, item) in doc.iter_mut() {
        tables_mut(item, &mut |t| {
            next = next.max(t.position().unwrap_or(0) + 1)
        });
    }
    let mut notes = Vec::new();
    let mut gone = BTreeSet::new();
    for (key, item) in from.iter() {
        if TEMPLATE_KEYS.contains(&key) {
            continue;
        }
        let mut item = item.clone();
        if MADE.contains(&key) {
            let mut list = match item.into_array_of_tables() {
                Ok(list) => list,
                Err(other) => {
                    return Err(format!("the type's file: {key} is {}", other.type_name()));
                }
            };
            list.retain(|t| {
                let Some(label) = t.get("label").and_then(Item::as_str) else {
                    return true;
                };
                if !made.contains(label) {
                    return true;
                }
                if !now.contains(label) {
                    gone.insert(label.to_owned());
                }
                if key == "point" && (t.contains_key("node") || t.contains_key("member")) {
                    notes.push(format!(
                        "point {label} was tied to a joint or member: built again at the \
                         template's position"
                    ));
                }
                false
            });
            if list.is_empty() {
                continue;
            }
            item = Item::ArrayOfTables(list);
        }
        tables_mut(&mut item, &mut |t| {
            t.set_position(Some(next));
            next += 1;
        });
        match (
            doc.get_mut(key).and_then(Item::as_array_of_tables_mut),
            item,
        ) {
            (Some(list), Item::ArrayOfTables(kept)) => {
                for t in kept {
                    list.push(t);
                }
            }
            (_, item) => {
                doc.insert(key, item);
            }
        }
    }
    // Kept entries that name what is no longer built.
    let field = |t: &Table, k: &str| t.get(k).and_then(Item::as_str).map(str::to_owned);
    let root = doc.as_item();
    for s in entries(root, "string") {
        for k in ["top", "wire"] {
            if let Some(l) = field(s, k).filter(|l| gone.contains(l)) {
                let label = field(s, "label").unwrap_or_default();
                notes.push(format!("string {label}: its {k} {l} is no longer built"));
            }
        }
    }
    for n in entries(root, "node") {
        if let Some(l) = field(n, "attachment").filter(|l| gone.contains(l)) {
            let id = field(n, "id").unwrap_or_default();
            notes.push(format!("joint {id}: its point {l} is no longer built"));
        }
    }
    let capacity = root.get("capacity").map(|c| entries(c, "attachment"));
    for a in capacity.into_iter().flatten() {
        if let Some(l) = field(a, "point").filter(|l| gone.contains(l)) {
            notes.push(format!("capacity.attachment: point {l} is no longer built"));
        }
    }
    let text = doc.to_string();
    match ols_structures::StructureType::parse(std::path::Path::new("built"), &text) {
        Ok(_) => Ok((text, notes)),
        Err(e) => {
            notes.insert(0, e.message);
            Err(format!("not built again: {}", notes.join("; ")))
        }
    }
}

/// What the builder asks of the application.
pub enum BuilderAction {
    /// Write structure type `id` (`text`), replacing the file of the type
    /// built before when `replace` is given.
    Save {
        /// The type's id.
        id: String,
        /// Its file's text.
        text: String,
        /// The file of the type it replaces.
        replace: Option<PathBuf>,
        /// What to look at in the type built again.
        notes: Vec<String>,
    },
}

/// The pylon builder window.
pub struct PylonBuilder {
    templates: Vec<PylonTemplate>,
    errors: Vec<String>,
    /// The template chosen (index).
    pub template: usize,
    /// Suspension or strain.
    pub role: Role,
    /// The values typed.
    pub values: Values,
    /// The type's id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// A type built before, being built again (its id).
    pub rebuild: Option<String>,
    /// Why Save failed, or what it did.
    pub message: Option<(String, bool)>,
}

impl PylonBuilder {
    /// The builder on the first template.
    pub fn new(model: &Model) -> Self {
        let (templates, errors) = templates(model);
        let mut b = Self {
            templates,
            errors,
            template: 0,
            role: Role::Suspension,
            values: Values {
                dimensions: Default::default(),
                string_length: 0.0,
                string_weight: 0.0,
                earth_length: 0.0,
                earth_weight: 0.0,
            },
            id: String::new(),
            name: String::new(),
            rebuild: None,
            message: None,
        };
        b.choose(model, 0);
        b
    }

    /// The template chosen, if any.
    pub fn current(&self) -> Option<&PylonTemplate> {
        self.templates.get(self.template)
    }

    /// Chooses template `k`: its values offered, a free id.
    pub fn choose(&mut self, model: &Model, k: usize) {
        self.template = k;
        self.rebuild = None;
        if let Some(t) = self.templates.get(k) {
            self.values = t.defaults(self.role);
        }
        self.name_after(model);
    }

    /// The id and name the type takes: `<template>-susp` / `-strain`, the
    /// first free.
    fn name_after(&mut self, model: &Model) {
        let Some(t) = self.current() else {
            return;
        };
        let suffix = match self.role {
            Role::Suspension => "susp",
            Role::Strain => "strain",
        };
        let base = format!("{}-{suffix}", t.id);
        let taken = |id: &str| {
            model
                .types
                .as_ref()
                .is_some_and(|(l, _)| l.get(id).is_some())
        };
        let id = (1..)
            .map(|n| {
                if n == 1 {
                    base.clone()
                } else {
                    format!("{base}-{n}")
                }
            })
            .find(|id| !taken(id))
            .expect("a free id");
        let role = match self.role {
            Role::Suspension => "suspension",
            Role::Strain => "strain",
        };
        self.name = format!("{}, {role}", t.name);
        self.id = id;
    }

    /// Opens structure type `id` built before: its template, role and
    /// values, to be built again (its file replaced).
    pub fn rebuild(&mut self, model: &Model, id: &str) -> Result<(), String> {
        let ty = model
            .types
            .as_ref()
            .and_then(|(l, _)| l.get(id))
            .ok_or_else(|| format!("no structure type {id}"))?;
        let record = ty
            .template
            .as_ref()
            .and_then(ols_structures::templates::recorded)
            .ok_or_else(|| format!("structure type {id} was not built from a template"))?;
        let (template, role, values) = record;
        let k = self
            .templates
            .iter()
            .position(|t| t.id == template)
            .ok_or_else(|| format!("no pylon template {template} (any more)"))?;
        self.template = k;
        self.role = role;
        // Values of parameters the template has now; new ones: defaults.
        let mut v = self.templates[k].defaults(role);
        for (name, x) in values.dimensions {
            if let Some(slot) = v.dimensions.get_mut(&name) {
                *slot = x;
            }
        }
        v.string_length = values.string_length;
        v.string_weight = values.string_weight;
        // Earth wires' strings: recorded, else the template's.
        if values.earth_length.is_finite() {
            v.earth_length = values.earth_length;
        }
        if values.earth_weight.is_finite() {
            v.earth_weight = values.earth_weight;
        }
        self.values = v;
        self.id = id.to_owned();
        self.name = ty.name.clone();
        self.rebuild = Some(id.to_owned());
        Ok(())
    }

    /// The window; what was asked for.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        open: &mut bool,
    ) -> Option<BuilderAction> {
        let mut out = None;
        egui::Window::new("New pylon from a template")
            .open(open)
            .default_width(820.0)
            .show(ctx, |ui| {
                out = self.content(ui, model);
            });
        out
    }

    fn content(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<BuilderAction> {
        let mut out = None;
        if self.templates.is_empty() {
            ui.label("No pylon template found (libraries/pylon-templates, or the project's Pylon templates folder).");
            return None;
        }
        for e in &self.errors {
            crate::menu::error_label(ui, egui::RichText::new(e).small());
        }
        // Types built before, to build again.
        let built: Vec<(String, String)> = model
            .types
            .as_ref()
            .map(|(l, _)| {
                l.iter()
                    .filter(|t| t.template.is_some())
                    .map(|t| (t.id.clone(), t.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Template");
            let mut chosen = None;
            egui::ComboBox::from_id_salt("pylon-template")
                .selected_text(self.current().map_or("", |t| t.name.as_str()))
                .width(320.0)
                .show_ui(ui, |ui| {
                    for (k, t) in self.templates.iter().enumerate() {
                        if ui
                            .selectable_label(k == self.template && self.rebuild.is_none(), &t.name)
                            .on_hover_text(&t.description)
                            .clicked()
                        {
                            chosen = Some(k);
                        }
                    }
                });
            if let Some(k) = chosen {
                self.choose(model, k);
            }
            if !built.is_empty() {
                let mut again = None;
                egui::ComboBox::from_id_salt("pylon-rebuild")
                    .selected_text(match &self.rebuild {
                        Some(id) => format!("building {id} again"),
                        None => "or a type built before…".into(),
                    })
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (id, name) in &built {
                            if ui
                                .selectable_label(self.rebuild.as_deref() == Some(id), id)
                                .on_hover_text(name)
                                .clicked()
                            {
                                again = Some(id.clone());
                            }
                        }
                    });
                if let Some(id) = again
                    && let Err(e) = self.rebuild(model, &id)
                {
                    self.message = Some((e, true));
                }
            }
        });
        let t = self.current().cloned()?;
        if !t.description.is_empty() {
            ui.label(egui::RichText::new(&t.description).small().weak());
        }
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(380.0);
                ui.horizontal(|ui| {
                    ui.label("For");
                    let before = self.role;
                    ui.selectable_value(&mut self.role, Role::Suspension, "Suspension")
                        .on_hover_text("A string hanging down at each phase; sets: the slots");
                    ui.selectable_value(&mut self.role, Role::Strain, "Strain")
                        .on_hover_text(format!(
                            "Two level strain strings at each phase along the line (directions \
                             180° and 0°); sets: the slots + {} and + {}",
                            t.first_offset, t.second_offset
                        ));
                    if self.role != before {
                        // The other role's strings; the dimensions stay.
                        let d = t.defaults(self.role);
                        self.values.string_length = d.string_length;
                        self.values.string_weight = d.string_weight;
                        self.values.earth_length = d.earth_length;
                        self.values.earth_weight = d.earth_weight;
                        if self.rebuild.is_none() {
                            self.name_after(model);
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Suspension and strain are two types: build one, switch, build the other.",
                    )
                    .small()
                    .weak(),
                );
                egui::Grid::new("pylon-values")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        for p in &t.parameters {
                            ui.label(&p.name);
                            if let Some(v) = self.values.dimensions.get_mut(&p.name) {
                                ui.add(egui::DragValue::new(v).speed(0.1).max_decimals(3).suffix(" m"));
                            }
                            ui.label(egui::RichText::new(&p.description).small().weak());
                            ui.end_row();
                        }
                        ui.label("string length");
                        ui.add(
                            egui::DragValue::new(&mut self.values.string_length)
                                .speed(0.05)
                                .max_decimals(3)
                                .suffix(" m"),
                        );
                        ui.label(egui::RichText::new("each string, top to wire").small().weak());
                        ui.end_row();
                        ui.label("string weight");
                        let mut kn = self.values.string_weight / 1000.0;
                        if ui
                            .add(egui::DragValue::new(&mut kn).speed(0.05).max_decimals(3).suffix(" kN"))
                            .changed()
                        {
                            self.values.string_weight = kn * 1000.0;
                        }
                        ui.label(egui::RichText::new("each string").small().weak());
                        ui.end_row();
                        if !t.earths.is_empty() {
                            ui.label("earth wire string length");
                            ui.add(
                                egui::DragValue::new(&mut self.values.earth_length)
                                    .speed(0.01)
                                    .max_decimals(3)
                                    .suffix(" m"),
                            );
                            ui.label(
                                egui::RichText::new("the earth wire's short string (clamp)")
                                    .small()
                                    .weak(),
                            );
                            ui.end_row();
                            ui.label("earth wire string weight");
                            let mut kn = self.values.earth_weight / 1000.0;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut kn)
                                        .speed(0.01)
                                        .max_decimals(3)
                                        .suffix(" kN"),
                                )
                                .changed()
                            {
                                self.values.earth_weight = kn * 1000.0;
                            }
                            ui.label(egui::RichText::new("each").small().weak());
                            ui.end_row();
                        }
                    });
                ui.separator();
                egui::Grid::new("pylon-id").num_columns(2).show(ui, |ui| {
                    ui.label("type id");
                    ui.add_enabled(
                        self.rebuild.is_none(),
                        egui::TextEdit::singleline(&mut self.id).desired_width(200.0),
                    );
                    ui.end_row();
                    ui.label("name");
                    ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(260.0));
                    ui.end_row();
                });
                let built = t.build(&self.values, self.role, self.id.trim(), self.name.trim());
                if let Err(e) = &built {
                    crate::menu::error_label(ui, egui::RichText::new(e).small());
                }
                let taken = self.rebuild.is_none()
                    && model
                        .types
                        .as_ref()
                        .is_some_and(|(l, _)| l.get(self.id.trim()).is_some());
                if taken {
                    crate::menu::error_label(
                        ui,
                        egui::RichText::new(format!("there is a structure type {} already", self.id.trim()))
                            .small(),
                    );
                }
                let editable = model.can_edit().is_ok();
                let (label, hover) = if self.rebuild.is_some() {
                    (
                        "Build it again (keep what was added)",
                        "Its points, strings and wire points from the template again; joints, \
                         members, sections, capacity, bodies and what was added by hand are kept",
                    )
                } else {
                    (
                        "Save the type",
                        "Into the project's structure types; then Structures › Place on alignment points…",
                    )
                };
                if ui
                    .add_enabled(
                        editable && built.is_ok() && !taken && !self.id.trim().is_empty(),
                        egui::Button::new(label),
                    )
                    .on_hover_text(hover)
                    .clicked()
                    && let Ok(text) = built
                {
                    let replace = self.rebuild.as_ref().and_then(|id| {
                        model
                            .types
                            .as_ref()
                            .and_then(|(l, _)| l.path(id).map(|p| p.to_path_buf()))
                    });
                    // Built again: what the template does not make is kept.
                    let merged = match &replace {
                        Some(path) => std::fs::read_to_string(path)
                            .map_err(|e| format!("{}: {e}", path.display()))
                            .and_then(|old| rebuilt(&old, &text, &t)),
                        None => Ok((text, Vec::new())),
                    };
                    match merged {
                        Ok((text, notes)) => {
                            out = Some(BuilderAction::Save {
                                id: self.id.trim().to_owned(),
                                text,
                                replace,
                                notes,
                            });
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
                if let Some((m, error)) = &self.message {
                    let t = egui::RichText::new(m).small();
                    egui::ScrollArea::vertical()
                        .id_salt("pylon-message")
                        .max_height(120.0)
                        .show(ui, |ui| {
                            if *error {
                                crate::menu::error_label(ui, t);
                            } else {
                                ui.colored_label(crate::menu::ok_color(ui), t);
                            }
                        });
                }
            });
            ui.separator();
            face(ui, &t, &self.values, self.role);
        });
        out
    }
}

/// The face drawn: the ground, the mast, an arm to each attachment, the
/// strings (suspension: down; strain: a dot), labels with the sets.
fn face(ui: &mut egui::Ui, t: &PylonTemplate, values: &Values, role: Role) {
    let size = egui::vec2(380.0, 440.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let Ok(points) = t.face(&values.dimensions) else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "the dimensions do not make a face",
            egui::FontId::proportional(13.0),
            ui.visuals().warn_fg_color,
        );
        return;
    };
    let mast = t
        .mast(&values.dimensions)
        .ok()
        .flatten()
        .unwrap_or_else(|| points.iter().map(|p| p.up).fold(0.0, f64::max));
    let half = points.iter().map(|p| p.across.abs()).fold(1.0, f64::max) * 1.25;
    let top = mast.max(1.0) * 1.08;
    let scale = ((size.x as f64 - 40.0) / (2.0 * half)).min((size.y as f64 - 50.0) / top);
    let at = |x: f64, z: f64| {
        egui::pos2(
            rect.center().x + (x * scale) as f32,
            rect.bottom() - 25.0 - (z * scale) as f32,
        )
    };
    let fg = ui.visuals().strong_text_color();
    let weak = ui.visuals().weak_text_color();
    painter.line_segment(
        [at(-half, 0.0), at(half, 0.0)],
        egui::Stroke::new(1.0, weak),
    );
    painter.line_segment([at(0.0, 0.0), at(0.0, mast)], egui::Stroke::new(2.0, fg));
    let font = egui::FontId::proportional(11.0);
    for p in &points {
        // The arm: level from the mast.
        painter.line_segment(
            [at(0.0, p.up), at(p.across, p.up)],
            egui::Stroke::new(1.5, fg),
        );
        let sets = match (role, p.earth) {
            (Role::Suspension, _) => format!("{}", p.slot),
            (Role::Strain, _) => {
                format!("{}/{}", p.slot + t.first_offset, p.slot + t.second_offset)
            }
        };
        let colour = if p.earth {
            egui::Color32::from_rgb(160, 160, 160)
        } else {
            egui::Color32::from_rgb(90, 200, 220)
        };
        let length = if p.earth {
            values.earth_length
        } else {
            values.string_length
        };
        // Its point on the pylon; a suspension string hangs below it.
        painter.circle_stroke(at(p.across, p.up), 3.0, egui::Stroke::new(1.5, fg));
        if role == Role::Suspension {
            painter.line_segment(
                [at(p.across, p.up), at(p.across, p.up - length)],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 180, 90)),
            );
            painter.circle_filled(at(p.across, p.up - length), 3.5, colour);
        } else {
            painter.circle_filled(at(p.across, p.up), 4.0, colour);
        }
        let below = if role == Role::Suspension {
            length
        } else {
            0.0
        };
        let anchor = if p.across < 0.0 {
            egui::Align2::RIGHT_TOP
        } else {
            egui::Align2::LEFT_TOP
        };
        let shift = if p.across < 0.0 { -6.0 } else { 6.0 };
        painter.text(
            at(p.across, p.up - below) + egui::vec2(shift, 2.0),
            anchor,
            format!("{} ({sets})", p.label),
            font.clone(),
            fg,
        );
    }
    painter.text(
        rect.left_top() + egui::vec2(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        "the face, looking ahead along the line (right: +across); (sets)",
        egui::FontId::proportional(10.0),
        weak,
    );
}

/// A row of the placement table: an alignment point.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceRow {
    /// Its index on the alignment.
    pub index: usize,
    /// Its position.
    pub position: [f64; 3],
    /// The structure on it already.
    pub under: Option<String>,
    /// A structure is put on it.
    pub on: bool,
    /// The type it takes (none: the one chosen for all).
    pub structure_type: Option<String>,
}

/// Structures › Place on alignment points…
pub struct Placement {
    /// The alignment.
    pub alignment: String,
    /// The type of every point not given its own.
    pub structure_type: Option<String>,
    /// The points.
    pub rows: Vec<PlaceRow>,
    /// The base on the ground.
    pub on_ground: bool,
    /// Why Apply failed.
    pub message: Option<String>,
}

impl Placement {
    /// For the first alignment (the type of the line's last structure).
    pub fn new(model: &Model) -> Self {
        let alignment = model
            .line
            .alignments
            .first()
            .map(|a| a.id.clone())
            .unwrap_or_default();
        let structure_type = model
            .line
            .structures
            .last()
            .and_then(|s| s.structure_type.clone())
            .or_else(|| model.types.as_ref()?.0.iter().next().map(|t| t.id.clone()));
        let mut p = Self {
            alignment,
            structure_type,
            rows: Vec::new(),
            on_ground: true,
            message: None,
        };
        p.rows(model);
        p
    }

    /// The rows of the alignment chosen: every free point ticked.
    pub fn rows(&mut self, model: &Model) {
        let line = &model.line;
        self.rows = line
            .alignment_by_id(&self.alignment)
            .map(|a| {
                line.points_of(a)
                    .into_iter()
                    .enumerate()
                    .map(|(index, p)| PlaceRow {
                        index,
                        position: p.position,
                        on: p.structure.is_none(),
                        under: p.structure,
                        structure_type: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    /// The command: a structure on every point ticked, its type, along the
    /// line, tied to the point (ids after the line's last). What it does.
    pub fn command(&self, model: &Model) -> Result<(String, ols_session::Command), String> {
        let stored = model
            .stored_line()
            .ok_or("read-only: open a project to edit")?;
        let mut taken: Vec<String> = stored.structures.iter().map(|s| s.id.clone()).collect();
        let mut commands = Vec::new();
        let mut ids = Vec::new();
        for r in self.rows.iter().filter(|r| r.on && r.under.is_none()) {
            let ty = r
                .structure_type
                .clone()
                .or_else(|| self.structure_type.clone())
                .ok_or("choose the type")?;
            let mut f = crate::editors::StructureForm::new_at(model, r.position);
            let like = taken.last().map_or("T0", |s| s.as_str()).to_owned();
            f.id = crate::editors::next_id(taken.iter().map(String::as_str), &like);
            f.structure_type = Some(ty);
            f.extension = None;
            f.along_line = true;
            f.on_ground = self.on_ground;
            f.place_on(model, (self.alignment.clone(), r.index));
            if !self.on_ground {
                f.position = r.position;
            }
            let (_, c) = f.command(Some(stored))?;
            taken.push(f.id.clone());
            ids.push(f.id.clone());
            commands.push(c);
        }
        if commands.is_empty() {
            return Err("no point ticked (points under a structure already are left)".into());
        }
        let what = format!(
            "{} structure(s) on alignment {}: {}",
            ids.len(),
            self.alignment,
            ids.join(", ")
        );
        Ok((what, ols_session::Command::Batch { commands }))
    }

    /// The window; the command when applied.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        model: &Model,
        open: &mut bool,
    ) -> Option<(String, ols_session::Command)> {
        let mut out = None;
        egui::Window::new("Place on alignment points")
            .open(open)
            .default_width(620.0)
            .show(ctx, |ui| {
                let types: Vec<String> = model
                    .types
                    .as_ref()
                    .map(|(l, _)| l.iter().map(|t| t.id.clone()).collect())
                    .unwrap_or_default();
                egui::Grid::new("place-head").num_columns(2).show(ui, |ui| {
                    ui.label("alignment");
                    let before = self.alignment.clone();
                    egui::ComboBox::from_id_salt("place-alignment")
                        .selected_text(&self.alignment)
                        .show_ui(ui, |ui| {
                            for a in &model.line.alignments {
                                ui.selectable_value(
                                    &mut self.alignment,
                                    a.id.clone(),
                                    format!("{} (route {})", a.id, a.route),
                                );
                            }
                        });
                    if self.alignment != before {
                        self.rows(model);
                    }
                    ui.end_row();
                    ui.label("type");
                    egui::ComboBox::from_id_salt("place-type")
                        .selected_text(self.structure_type.as_deref().unwrap_or("—"))
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            for t in &types {
                                ui.selectable_value(&mut self.structure_type, Some(t.clone()), t);
                            }
                        });
                    ui.end_row();
                });
                ui.checkbox(&mut self.on_ground, "base on the ground");
                ui.label(
                    egui::RichText::new(
                        "Each stands on its point, tied to it, turned along the line (the bisector \
                         at an angle); then it stays as placed — Edit › Put it on the line again \
                         turns it again. A row's own type (a strain type at an angle point) \
                         replaces the type above.",
                    )
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        for r in &mut self.rows {
                            r.on = r.under.is_none();
                        }
                    }
                    if ui.small_button("None").clicked() {
                        for r in &mut self.rows {
                            r.on = false;
                        }
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        egui::Grid::new("place-rows")
                            .num_columns(4)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("#");
                                ui.strong("point");
                                ui.strong("put");
                                ui.strong("type");
                                ui.end_row();
                                for r in &mut self.rows {
                                    ui.label((r.index + 1).to_string());
                                    ui.label(format!(
                                        "E {:.1}, N {:.1}",
                                        r.position[0], r.position[1]
                                    ));
                                    match &r.under {
                                        Some(s) => {
                                            ui.label(
                                                egui::RichText::new(format!("under {s}")).weak(),
                                            );
                                        }
                                        None => {
                                            ui.checkbox(&mut r.on, "");
                                        }
                                    }
                                    ui.add_enabled_ui(r.on && r.under.is_none(), |ui| {
                                        egui::ComboBox::from_id_salt(("place-row-type", r.index))
                                            .selected_text(
                                                r.structure_type
                                                    .as_deref()
                                                    .unwrap_or("(the type above)"),
                                            )
                                            .height(300.0)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut r.structure_type,
                                                    None,
                                                    "(the type above)",
                                                );
                                                for t in &types {
                                                    ui.selectable_value(
                                                        &mut r.structure_type,
                                                        Some(t.clone()),
                                                        t,
                                                    );
                                                }
                                            });
                                    });
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(m) = &self.message {
                    crate::menu::error_label(ui, egui::RichText::new(m).small());
                }
                let n = self
                    .rows
                    .iter()
                    .filter(|r| r.on && r.under.is_none())
                    .count();
                if ui
                    .add_enabled(
                        model.can_edit().is_ok() && n > 0,
                        egui::Button::new(format!("Place {n} structure(s)")),
                    )
                    .on_hover_text("One command: one undo")
                    .clicked()
                {
                    match self.command(model) {
                        Ok(c) => out = Some(c),
                        Err(e) => self.message = Some(e),
                    }
                }
            });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_structures::StructureType;

    fn delta() -> PylonTemplate {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../libraries/pylon-templates/delta-1c.olspt");
        PylonTemplate::parse(&path, &std::fs::read_to_string(&path).unwrap()).unwrap()
    }

    fn read(text: &str) -> StructureType {
        StructureType::parse(std::path::Path::new("x"), text).unwrap()
    }

    /// What was added to a built type: a joint and a member with its
    /// section, a body, an extension, capacity, a point and a string by
    /// hand (from a template point), a comment.
    const ADDED: &str = r#"
# hand-made below
[[extension]]
name = "+3"
dz = "3 m"

[[body]]
points = [["0 m", "0 m", "0 m"], ["0 m", "1 m", "0 m"], ["0 m", "0 m", "30 m"]]

[[node]]
id = "J1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "J2"
point = ["0 m", "0 m", "10 m"]

[[member]]
id = "M1"
nodes = ["J1", "J2"]
section = "s"

[[material]]
id = "steel"
elasticity = "210 GPa"

[[section]]
id = "s"
material = "steel"
area = "1000 mm2"

[[point]]
label = "X1"
node = "J2"

[[wire]]
label = "WX"

[[string]]
label = "SX"
top = "A-L1"
wire = "WX"
length = "1 m"
weight = "10 N"

[[capacity.attachment]]
point = "A-L1"
vertical = "20 kN"
"#;

    /// Built again with other dimensions: the template's points move, all
    /// that was added stays (a key at the top and the comment too),
    /// nothing to look at.
    #[test]
    fn building_again_keeps_what_was_added() {
        let t = delta();
        let role = Role::Suspension;
        let v = t.defaults(role);
        let first = t.build(&v, role, "d", "D").unwrap();
        let existing = format!("foot = \"0.5 m\"\n{first}{ADDED}");
        let before = read(&existing);
        let mut higher = v.clone();
        *higher.dimensions.get_mut("lower_level").unwrap() += 2.0;
        let built = t.build(&higher, role, "d", "D").unwrap();
        let (text, notes) = rebuilt(&existing, &built, &t).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(text.contains("# hand-made below"), "{text}");
        let after = read(&text);
        assert_eq!(after.nodes, before.nodes);
        assert_eq!(after.members, before.members);
        assert_eq!(after.sections, before.sections);
        assert_eq!(after.materials, before.materials);
        assert_eq!(after.body, before.body);
        assert_eq!(after.extensions, before.extensions);
        assert_eq!(after.capacity, before.capacity);
        assert_eq!(after.foot, before.foot);
        assert_eq!(after.points.len(), before.points.len());
        assert_eq!(after.strings.len(), before.strings.len());
        assert_eq!(after.wires.len(), before.wires.len());
        assert!(after.points.iter().any(|p| p.label == "X1"));
        assert!(after.strings.iter().any(|s| s.label == "SX"));
        let z = |ty: &StructureType| ty.point_local("A-L1", None).unwrap()[2];
        assert!((z(&after) - z(&before) - 2.0).abs() < 1e-9);
        // Each template point once, the record new.
        assert_eq!(text.matches("label = \"A-L1\"").count(), 1);
        assert_eq!(text.matches("[template]").count(), 1);
    }

    /// Built again for the other role: a kept entry naming a wire point no
    /// longer built is reported (kept, not dropped); one that would not
    /// read as a type is refused with the reason.
    #[test]
    fn building_again_reports_what_it_no_longer_makes() {
        let t = delta();
        let susp = t
            .build(&t.defaults(Role::Suspension), Role::Suspension, "d", "D")
            .unwrap();
        let strain = t
            .build(&t.defaults(Role::Strain), Role::Strain, "d", "D")
            .unwrap();
        // A joint naming a suspension wire point the strain type has not.
        let joint =
            "\n[[node]]\nid = \"J9\"\npoint = [\"0 m\", \"0 m\", \"5 m\"]\nattachment = \"W-L1\"\n";
        let (text, notes) = rebuilt(&format!("{susp}{joint}"), &strain, &t).unwrap();
        assert!(text.contains("attachment = \"W-L1\""), "kept: {text}");
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("J9") && notes[0].contains("W-L1"));
        // A hand string down to a wire point no longer built: refused.
        let string = "\n[[string]]\nlabel = \"SX\"\ntop = \"A-L2\"\nwire = \"W-L1\"\n\
                      length = \"1 m\"\nweight = \"1 N\"\n";
        let e = rebuilt(&format!("{susp}{string}"), &strain, &t).unwrap_err();
        assert!(
            e.contains("string SX: its wire W-L1 is no longer built"),
            "{e}"
        );
        // Back to suspension: the strain strings go, the suspension ones
        // come back.
        let (text, _) = rebuilt(&strain, &susp, &t).unwrap();
        assert_eq!(read(&text).strings.len(), read(&susp).strings.len());
    }
}
