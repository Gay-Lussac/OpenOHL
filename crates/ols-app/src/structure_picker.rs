// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The structure picker: a window to choose a structure type of the
//! library. On the left a table of the types (id, name, kind, level, sets
//! and wire points, height, extensions, description, how often the line
//! uses it), sorted by a click on a column's title and filtered by the
//! words typed (on id, name, kind and description); on the right the
//! selected type drawn (front and side view of its stick figure, or 3D
//! with a GPU), and under it its full name, description, extensions, level
//! and what it lacks for the next level. Choose (or a double-click on a
//! row, or Enter) takes it, with the extension ticked where the caller
//! has one; Cancel (or Escape, or closing the window) leaves things as
//! they were.
//!
//! The Add structure / structure window and the Structures table's type
//! column open it ([`Picker::show`]); the filter typed is remembered for
//! the session.

use std::collections::BTreeMap;

use eframe::egui;
use eframe::egui_wgpu;
use ols_structures::StructureType;
use ols_structures::structures::StructureKind;

use crate::model::Model;

/// What the picker ends with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Choose: this type (none: no type) with this extension.
    Choose {
        /// The type's id.
        structure_type: Option<String>,
        /// The extension (none: the base).
        extension: Option<String>,
    },
    /// Cancel (or closed): no change.
    Cancel,
}

/// A column of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    /// The type's id.
    Id,
    /// Its name.
    Name,
    /// Lattice, pole, portal, other.
    Kind,
    /// Detail level 1, 2, 3.
    Level,
    /// Sets and wire points.
    Sets,
    /// Height above the feet.
    Height,
    /// Height extensions.
    Extensions,
    /// Description.
    Description,
    /// Structures of the line with this type.
    Used,
}

impl Column {
    /// Every column, in order.
    pub const ALL: [Column; 9] = [
        Column::Id,
        Column::Name,
        Column::Kind,
        Column::Level,
        Column::Sets,
        Column::Height,
        Column::Extensions,
        Column::Description,
        Column::Used,
    ];

    fn title(self) -> &'static str {
        match self {
            Column::Id => "id",
            Column::Name => "name",
            Column::Kind => "kind",
            Column::Level => "level",
            Column::Sets => "sets / wires",
            Column::Height => "height",
            Column::Extensions => "extensions",
            Column::Description => "description",
            Column::Used => "used",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Column::Id => "The type's id (what the structures name)",
            Column::Name => "The type's name",
            Column::Kind => "Lattice, pole, portal or other",
            Column::Level => {
                "Detail level: 1 stick figure, 2 outline (members or body lines), 3 full design"
            }
            Column::Sets => "Sets of wire points / wire points",
            Column::Height => "Height of the highest point above the feet",
            Column::Extensions => "Height extensions it can be placed with",
            Column::Description => {
                "What it is: the notes at the top of its file, else where it comes from"
            }
            Column::Used => "Structures of the line with this type",
        }
    }

    fn width(self) -> f32 {
        match self {
            Column::Id => 130.0,
            Column::Name => 190.0,
            Column::Kind => 55.0,
            Column::Level => 40.0,
            Column::Sets => 75.0,
            Column::Height => 60.0,
            Column::Extensions => 90.0,
            Column::Description => 210.0,
            Column::Used => 40.0,
        }
    }
}

/// A row of the table: what is shown of a type.
pub struct TypeRow {
    /// Its id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// Its kind, in words.
    pub kind: &'static str,
    /// Detail level (1–3).
    pub level: u8,
    /// The level's name.
    pub level_name: &'static str,
    /// What it lacks for the next level (empty at level 3).
    pub lacks: Vec<String>,
    /// Number of sets.
    pub sets: usize,
    /// Number of wire points.
    pub wire_points: usize,
    /// Height of the highest point above the feet, m.
    pub height: f64,
    /// Its extensions: name and height added, m.
    pub extensions: Vec<(String, f64)>,
    /// What it is (its `description`, else the comments at the top of its
    /// file, else its source).
    pub description: String,
    /// Where it comes from.
    pub source: String,
    /// Its drawing, feet at 0.
    geometry: crate::type_workspace::Geometry,
}

impl TypeRow {
    /// The row of type `t` (read from `path`, for its description).
    pub fn of(
        t: &StructureType,
        insulators: &ols_structures::InsulatorLibrary,
        path: Option<&std::path::Path>,
    ) -> Self {
        let mut g = crate::type_workspace::geometry(t, insulators);
        let foot = t.foot_level();
        let down = |p: &mut [f64; 3]| p[2] -= foot;
        g.joints.iter_mut().for_each(|j| down(&mut j.1));
        g.members.iter_mut().for_each(|m| {
            down(&mut m.1);
            down(&mut m.2)
        });
        g.points.iter_mut().for_each(|p| down(&mut p.1));
        g.wires.iter_mut().for_each(|w| down(&mut w.1));
        g.strings.iter_mut().for_each(|s| {
            down(&mut s.1);
            down(&mut s.2)
        });
        g.sticks.iter_mut().flatten().for_each(down);
        let height = all_points(&g).map(|p| p[2]).fold(0.0, f64::max);
        let detail = t.detail();
        let lacks = match detail {
            ols_structures::Detail::Stick => {
                vec!["the pylon's joints and members (or [[body]] polylines)".to_owned()]
            }
            ols_structures::Detail::Outline => t.missing_for_design(),
            ols_structures::Detail::Design => Vec::new(),
        };
        let source = t.source.clone().unwrap_or_default();
        let notes = path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| file_notes(&text))
            .unwrap_or_default();
        Self {
            id: t.id.clone(),
            name: t.name.clone(),
            kind: kind_name(t.kind),
            level: detail.level(),
            level_name: detail.name(),
            lacks,
            sets: t.sets().len(),
            wire_points: t.wires.len() + t.attachments.len(),
            height,
            extensions: t
                .extensions
                .iter()
                .map(|e| (e.name.clone(), e.dz.si()))
                .collect(),
            description: if !t.description.trim().is_empty() {
                t.description.trim().to_owned()
            } else if notes.is_empty() {
                source.clone()
            } else {
                notes
            },
            source,
            geometry: g,
        }
    }

    /// Whether every word of `filter` is in its id, name, kind,
    /// description or source (any case).
    pub fn matches(&self, filter: &str) -> bool {
        let hay = format!(
            "{}\n{}\n{}\n{}\n{}",
            self.id, self.name, self.kind, self.description, self.source
        )
        .to_lowercase();
        filter
            .to_lowercase()
            .split_whitespace()
            .all(|w| hay.contains(w))
    }

    fn extension_names(&self) -> String {
        self.extensions
            .iter()
            .map(|e| e.0.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The kind in words.
pub fn kind_name(k: StructureKind) -> &'static str {
    match k {
        StructureKind::Lattice => "lattice",
        StructureKind::Pole => "pole",
        StructureKind::Portal => "portal",
        StructureKind::Other => "other",
    }
}

/// The comment lines at the top of a type's file, as one text.
fn file_notes(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .take_while(|l| l.starts_with('#') || l.is_empty())
        .filter_map(|l| l.strip_prefix('#'))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn all_points(g: &crate::type_workspace::Geometry) -> impl Iterator<Item = [f64; 3]> + '_ {
    g.points
        .iter()
        .map(|p| p.1)
        .chain(g.wires.iter().map(|w| w.1))
        .chain(g.joints.iter().map(|j| j.1))
        .chain(g.sticks.iter().flatten().copied())
        .chain(g.members.iter().flat_map(|m| [m.1, m.2]))
}

/// How the selected type is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View {
    /// Front (across, up) and side (along, up) views.
    #[default]
    Sides,
    /// Turned in 3D (needs a GPU).
    ThreeD,
}

/// An open structure picker.
pub struct Picker {
    /// The window's title.
    pub title: String,
    /// The type selected (empty: no type, when that is allowed); none:
    /// nothing selected.
    pub selected: Option<String>,
    /// The extension ticked (none: the base).
    pub extension: Option<String>,
    /// The type before (shown marked).
    pub current: Option<String>,
    /// The caller takes an extension too.
    pub with_extension: bool,
    /// "No type" can be chosen.
    pub allow_none: bool,
    /// Sorted by this column (ascending when true).
    pub sort: (Column, bool),
    /// How the selection is drawn.
    pub view: View,
    /// The rows (built when the library's types change).
    rows: Vec<TypeRow>,
    rows_of: Vec<String>,
    /// Bring the selected row into view (on opening).
    scroll_to: bool,
    /// A row was double-clicked.
    choose: bool,
}

/// Height of the table and the details.
const BODY_HEIGHT: f32 = 520.0;

/// The filter typed, kept for the session.
fn filter_id() -> egui::Id {
    egui::Id::new("structure-picker-filter")
}

impl Picker {
    /// A picker showing `current` (and its `extension`) selected.
    pub fn new(
        title: impl Into<String>,
        current: Option<&str>,
        extension: Option<&str>,
        with_extension: bool,
        allow_none: bool,
    ) -> Self {
        Self {
            title: title.into(),
            selected: current
                .map(str::to_owned)
                .or_else(|| allow_none.then(String::new)),
            extension: extension.map(str::to_owned),
            current: current.map(str::to_owned),
            with_extension,
            allow_none,
            sort: (Column::Id, true),
            view: View::default(),
            rows: Vec::new(),
            rows_of: Vec::new(),
            scroll_to: true,
            choose: false,
        }
    }

    /// The filter typed (this session).
    pub fn filter(ctx: &egui::Context) -> String {
        ctx.data(|d| d.get_temp::<String>(filter_id()).unwrap_or_default())
    }

    /// Sets the filter (this session).
    pub fn set_filter(ctx: &egui::Context, text: &str) {
        ctx.data_mut(|d| d.insert_temp(filter_id(), text.to_owned()));
    }

    /// The rows of the library's types (built again when they change).
    fn refresh(&mut self, model: &Model) {
        let Some((types, insulators)) = &model.types else {
            self.rows.clear();
            self.rows_of.clear();
            return;
        };
        let ids: Vec<String> = types
            .iter()
            .map(|t| format!("{}\t{}\t{:?}", t.id, t.name, t.detail()))
            .collect();
        if ids == self.rows_of && self.rows.len() == ids.len() {
            return;
        }
        self.rows = types
            .iter()
            .map(|t| TypeRow::of(t, insulators, types.path(&t.id)))
            .collect();
        self.rows_of = ids;
    }

    /// The rows shown: filtered by `filter`, sorted.
    fn shown<'a>(&'a self, filter: &str, used: &BTreeMap<&str, usize>) -> Vec<&'a TypeRow> {
        let mut rows: Vec<&TypeRow> = self.rows.iter().filter(|r| r.matches(filter)).collect();
        let (col, up) = self.sort;
        let n = |r: &TypeRow| used.get(r.id.as_str()).copied().unwrap_or(0);
        rows.sort_by(|a, b| {
            let o = match col {
                Column::Id => a.id.cmp(&b.id),
                Column::Name => a.name.cmp(&b.name),
                Column::Kind => a.kind.cmp(b.kind),
                Column::Level => a.level.cmp(&b.level),
                Column::Sets => (a.sets, a.wire_points).cmp(&(b.sets, b.wire_points)),
                Column::Height => a.height.total_cmp(&b.height),
                Column::Extensions => a.extensions.len().cmp(&b.extensions.len()),
                Column::Description => a.description.cmp(&b.description),
                Column::Used => n(a).cmp(&n(b)),
            }
            .then_with(|| a.id.cmp(&b.id));
            if up { o } else { o.reverse() }
        });
        rows
    }

    /// The rows' ids shown with `filter` (in order).
    #[cfg(test)]
    pub fn shown_ids(&mut self, model: &Model, filter: &str) -> Vec<String> {
        self.refresh(model);
        let used = used(model);
        self.shown(filter, &used)
            .into_iter()
            .map(|r| r.id.clone())
            .collect()
    }

    /// What Choose gives now (none: nothing selected).
    pub fn outcome(&self) -> Option<Outcome> {
        let id = self.selected.clone()?;
        let row = self.rows.iter().find(|r| r.id == id);
        Some(Outcome::Choose {
            extension: if self.with_extension {
                self.extension
                    .clone()
                    .filter(|e| row.is_some_and(|r| r.extensions.iter().any(|x| &x.0 == e)))
            } else {
                None
            },
            structure_type: (!id.is_empty()).then_some(id),
        })
    }

    /// The window (identified by `id`); the outcome once chosen or
    /// cancelled.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        model: &Model,
        rs: Option<&egui_wgpu::RenderState>,
        preview: &mut crate::preview3d::Preview,
    ) -> Option<Outcome> {
        self.refresh(model);
        let mut open = true;
        let mut outcome = None;
        egui::Window::new(self.title.clone())
            .id(id)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(1280.0)
            .show(ctx, |ui| {
                outcome = self.ui(ui, model, rs, preview);
            });
        if outcome.is_none() {
            outcome = ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    Some(Outcome::Cancel)
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    self.outcome()
                } else {
                    None
                }
            });
        }
        if !open {
            outcome = Some(Outcome::Cancel);
        }
        outcome
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        rs: Option<&egui_wgpu::RenderState>,
        preview: &mut crate::preview3d::Preview,
    ) -> Option<Outcome> {
        let mut outcome = None;
        let mut filter = Self::filter(ui.ctx());
        let used = used(model);
        ui.horizontal(|ui| {
            ui.label("Filter");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut filter)
                    .desired_width(280.0)
                    .hint_text("words in the id, name, kind or description"),
            );
            if self.scroll_to {
                edit.request_focus();
            }
            if edit.changed() {
                Self::set_filter(ui.ctx(), &filter);
            }
            if !filter.is_empty()
                && ui
                    .small_button("×")
                    .on_hover_text("Clear the filter")
                    .clicked()
            {
                filter.clear();
                Self::set_filter(ui.ctx(), "");
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} of {} types",
                    self.rows.iter().filter(|r| r.matches(&filter)).count(),
                    self.rows.len()
                ))
                .weak(),
            );
        });
        ui.separator();
        let preview_width = 380.0;
        // A fixed height: the table and the details scroll in it, the
        // buttons stay in view.
        let body = egui::vec2(ui.available_width(), BODY_HEIGHT);
        ui.allocate_ui_with_layout(body, egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.set_height(BODY_HEIGHT);
            let table_width = (ui.available_width() - preview_width - 16.0).max(300.0);
            ui.vertical(|ui| {
                ui.set_width(table_width);
                ui.set_height(BODY_HEIGHT);
                self.table(ui, &filter, &used);
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(preview_width);
                egui::ScrollArea::vertical()
                    .id_salt("structure-picker-details")
                    .max_height(BODY_HEIGHT)
                    .show(ui, |ui| self.details(ui, rs, preview, &used));
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let can = self.selected.is_some();
            let what = match self.selected.as_deref() {
                Some("") => "no type".to_owned(),
                Some(id) => id.to_owned(),
                None => "nothing selected".to_owned(),
            };
            if ui
                .add_enabled(can, egui::Button::new("Choose"))
                .on_hover_text(format!("Use {what} (Enter, or a double-click on a row)"))
                .clicked()
            {
                outcome = self.outcome();
            }
            if ui.button("Cancel").on_hover_text("Escape").clicked() {
                outcome = Some(Outcome::Cancel);
            }
        });
        if std::mem::take(&mut self.choose) {
            outcome = self.outcome();
        }
        self.scroll_to = false;
        outcome
    }

    /// The table: titles (a click sorts), then the rows.
    fn table(&mut self, ui: &mut egui::Ui, filter: &str, used: &BTreeMap<&str, usize>) {
        let row_height = 22.0;
        let width: f32 = Column::ALL.iter().map(|c| c.width() + 4.0).sum::<f32>() + 8.0;
        egui::ScrollArea::horizontal()
            .id_salt("structure-picker-columns")
            .show(ui, |ui| {
                ui.set_min_width(width);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_space(4.0);
                    for c in Column::ALL {
                        let arrow = match self.sort {
                            (s, true) if s == c => " ⏶",
                            (s, false) if s == c => " ⏷",
                            _ => "",
                        };
                        let title = egui::RichText::new(format!("{}{arrow}", c.title())).strong();
                        if ui
                            .add_sized(
                                [c.width(), row_height],
                                egui::Button::new(title).frame(false),
                            )
                            .on_hover_text(format!("{} — click to sort", c.help()))
                            .clicked()
                        {
                            self.sort = if self.sort.0 == c {
                                (c, !self.sort.1)
                            } else {
                                (c, true)
                            };
                        }
                    }
                });
                ui.separator();
                let mut clicked: Option<(String, bool)> = None;
                let selected = self.selected.clone();
                let scroll_to = self.scroll_to;
                let rows = self.shown(filter, used);
                egui::ScrollArea::vertical()
                    .id_salt("structure-picker-rows")
                    .max_height(BODY_HEIGHT - 40.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if self.allow_none && filter.trim().is_empty() {
                            let on = selected.as_deref() == Some("");
                            let r = none_row(ui, width, row_height, on);
                            if r.0 {
                                clicked = Some((String::new(), r.1));
                            }
                        }
                        if rows.is_empty() {
                            ui.label(
                                egui::RichText::new(if self.rows.is_empty() {
                                    "No structure type in the library yet."
                                } else {
                                    "No type matches the filter."
                                })
                                .weak(),
                            );
                        }
                        for (k, row) in rows.iter().enumerate() {
                            let on = selected.as_deref() == Some(row.id.as_str());
                            let n = used.get(row.id.as_str()).copied().unwrap_or(0);
                            let current = self.current.as_deref() == Some(row.id.as_str());
                            let (click, double, rect) =
                                type_row(ui, width, row_height, row, n, on, current, k);
                            if on && scroll_to {
                                ui.scroll_to_rect(rect, Some(egui::Align::Center));
                            }
                            if click {
                                clicked = Some((row.id.clone(), double));
                            }
                        }
                    });
                if let Some((id, double)) = clicked {
                    if self.selected.as_deref() != Some(id.as_str()) {
                        // An extension the new type does not have: the base.
                        let has = self.rows.iter().find(|r| r.id == id).is_some_and(|r| {
                            self.extension
                                .as_ref()
                                .is_none_or(|e| r.extensions.iter().any(|x| &x.0 == e))
                        });
                        if !has {
                            self.extension = None;
                        }
                    }
                    self.selected = Some(id);
                    self.choose |= double;
                }
            });
    }

    /// The selected type: its drawing, then what it is.
    fn details(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        preview: &mut crate::preview3d::Preview,
        used: &BTreeMap<&str, usize>,
    ) {
        let Some(row) = self
            .selected
            .as_deref()
            .and_then(|id| self.rows.iter().find(|r| r.id == id))
        else {
            ui.label(
                egui::RichText::new(match self.selected.as_deref() {
                    Some("") => {
                        "No type: the structure's points are given in the structure window, \
                         its strings added with Add string."
                    }
                    _ => "Select a type to see it here.",
                })
                .weak(),
            );
            return;
        };
        if rs.is_some() {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view, View::Sides, "Front · side")
                    .on_hover_text("Seen across the line (front) and along it (side)");
                ui.selectable_value(&mut self.view, View::ThreeD, "3D")
                    .on_hover_text("Turned with a drag, zoomed with the wheel");
            });
        }
        let size = egui::vec2(380.0, 300.0);
        match (self.view, rs) {
            (View::ThreeD, Some(rs)) => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                ("structure-picker", &row.id).hash(&mut h);
                preview.shows(&format!("structure-picker {}", row.id));
                let labels: Vec<crate::preview3d::Label> = Vec::new();
                preview.ui(
                    ui,
                    Some(rs),
                    size,
                    h.finish(),
                    || scene(&row.geometry),
                    &labels,
                );
            }
            _ => sides(ui, size, &row.geometry),
        }
        ui.add_space(4.0);
        ui.heading(&row.name);
        ui.label(egui::RichText::new(format!("{} · {}", row.id, row.kind)).weak());
        egui::ScrollArea::vertical()
            .id_salt("structure-picker-description")
            .max_height(70.0)
            .show(ui, |ui| {
                if row.description.is_empty() {
                    ui.label(egui::RichText::new("(no description)").weak());
                } else {
                    ui.label(&row.description);
                }
                if !row.source.is_empty() && row.source != row.description {
                    ui.label(egui::RichText::new(format!("source: {}", row.source)).small());
                }
            });
        ui.label(format!(
            "{} set(s), {} wire point(s), {:.1} m high · used by {} structure(s) of the line",
            row.sets,
            row.wire_points,
            row.height,
            used.get(row.id.as_str()).copied().unwrap_or(0)
        ));
        ui.label(format!("Level {} — {}", row.level, row.level_name));
        if !row.lacks.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "For level {}: {}",
                    row.level + 1,
                    row.lacks.join(", ")
                ))
                .small()
                .weak(),
            );
        }
        if row.extensions.is_empty() {
            ui.label(egui::RichText::new("No extensions").weak());
        } else if self.with_extension {
            ui.label("Extension");
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut self.extension, None, "(base)");
                for (name, dz) in &row.extensions {
                    ui.radio_value(&mut self.extension, Some(name.clone()), name)
                        .on_hover_text(format!("{dz:+.2} m on every point"));
                }
            });
        } else {
            ui.label(format!(
                "Extensions: {}",
                row.extensions
                    .iter()
                    .map(|(n, dz)| format!("{n} ({dz:+.2} m)"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
}

/// Structures of the line by type.
fn used(model: &Model) -> BTreeMap<&str, usize> {
    let mut out = BTreeMap::new();
    for s in &model.line.structures {
        if let Some(t) = &s.structure_type {
            *out.entry(t.as_str()).or_insert(0) += 1;
        }
    }
    out
}

/// Clicked twice in a row (a third click after another one counts too:
/// egui calls that a triple click).
fn twice(r: &egui::Response) -> bool {
    r.double_clicked() || r.triple_clicked()
}

/// The "no type" row: clicked, double-clicked.
fn none_row(ui: &mut egui::Ui, width: f32, height: f32, on: bool) -> (bool, bool) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    paint_row(ui, rect, &response, on, 0);
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let label = cell
        .selectable_label(on, "(no type: points given in the structure window)")
        .on_hover_text("A structure without a type: its points and strings given on it");
    (
        response.clicked() || label.clicked(),
        twice(&response) || twice(&label),
    )
}

/// A type's row: clicked, double-clicked, where.
#[allow(clippy::too_many_arguments)]
fn type_row(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    row: &TypeRow,
    used: usize,
    on: bool,
    current: bool,
    k: usize,
) -> (bool, bool, egui::Rect) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    paint_row(ui, rect, &response, on, k);
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(4.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cell.set_clip_rect(rect.intersect(ui.clip_rect()));
    cell.spacing_mut().item_spacing.x = 4.0;
    let mut clicked = response.clicked();
    let mut double = twice(&response);
    let id_text = if current {
        egui::RichText::new(&row.id).strong()
    } else {
        egui::RichText::new(&row.id)
    };
    let id = cell
        .add_sized(
            [Column::Id.width(), height],
            egui::Button::selectable(on, id_text).truncate(),
        )
        .on_hover_text(if current {
            format!("{} (its type now)", row.id)
        } else {
            row.id.clone()
        });
    clicked |= id.clicked();

    double |= twice(&id);
    let text = |cell: &mut egui::Ui, c: Column, t: String, hover: Option<&str>| {
        let r = cell.add_sized([c.width(), height], egui::Label::new(t.clone()).truncate());
        let hover = hover.map_or(t, str::to_owned);
        if !hover.is_empty() {
            r.on_hover_text(hover);
        }
    };
    text(&mut cell, Column::Name, row.name.clone(), None);
    text(&mut cell, Column::Kind, row.kind.to_owned(), None);
    text(
        &mut cell,
        Column::Level,
        row.level.to_string(),
        Some(&format!("level {} — {}", row.level, row.level_name)),
    );
    text(
        &mut cell,
        Column::Sets,
        format!("{} / {}", row.sets, row.wire_points),
        Some(&format!(
            "{} set(s), {} wire point(s)",
            row.sets, row.wire_points
        )),
    );
    text(
        &mut cell,
        Column::Height,
        format!("{:.1} m", row.height),
        None,
    );
    text(&mut cell, Column::Extensions, row.extension_names(), None);
    text(
        &mut cell,
        Column::Description,
        row.description.clone(),
        None,
    );
    text(&mut cell, Column::Used, used.to_string(), None);
    (clicked, double, rect)
}

fn paint_row(ui: &egui::Ui, rect: egui::Rect, response: &egui::Response, on: bool, k: usize) {
    let v = ui.visuals();
    let fill = if on {
        Some(v.selection.bg_fill.gamma_multiply(0.6))
    } else if response.hovered() {
        Some(v.widgets.hovered.weak_bg_fill)
    } else if k % 2 == 1 {
        Some(v.faint_bg_color)
    } else {
        None
    };
    if let Some(f) = fill {
        ui.painter().rect_filled(rect, 2.0, f);
    }
}

const STEEL: egui::Color32 = egui::Color32::from_rgb(200, 200, 208);
const STRING: egui::Color32 = egui::Color32::from_rgb(120, 180, 255);
const POINT: egui::Color32 = egui::Color32::from_rgb(255, 150, 60);
const WIRE: egui::Color32 = egui::Color32::from_rgb(90, 200, 255);

/// The front (across, up) and side (along, up) views of `g`, side by side
/// at one scale, the ground at the feet.
fn sides(ui: &mut egui::Ui, size: egui::Vec2, g: &crate::type_workspace::Geometry) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(24));
    let gap = 12.0;
    let half = (rect.width() - gap) / 2.0;
    let views = [
        (
            egui::Rect::from_min_size(rect.min, egui::vec2(half, rect.height())),
            0,
            "front (across)",
        ),
        (
            egui::Rect::from_min_size(
                rect.min + egui::vec2(half + gap, 0.0),
                egui::vec2(half, rect.height()),
            ),
            1,
            "side (along)",
        ),
    ];
    // One scale for both: the widest reach and the height.
    let mut lo = [f64::MAX; 2];
    let mut hi = [f64::MIN; 2];
    let mut top: f64 = 1.0;
    for p in all_points(g) {
        for a in 0..2 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
        top = top.max(p[2]);
    }
    let reach = (0..2)
        .map(|a| {
            if lo[a] > hi[a] {
                1.0
            } else {
                lo[a].abs().max(hi[a].abs())
            }
        })
        .fold(1.0, f64::max);
    let margin = 18.0;
    let scale = f64::min(
        f64::from(half - 2.0 * margin) / (2.0 * reach),
        f64::from(rect.height() - 2.0 * margin) / top,
    );
    for (r, axis, name) in views {
        let ground = r.bottom() - margin;
        let at = |p: [f64; 3]| {
            egui::pos2(
                r.center().x + (p[axis] * scale) as f32,
                ground - (p[2] * scale) as f32,
            )
        };
        painter.line_segment(
            [
                egui::pos2(r.left() + 4.0, ground),
                egui::pos2(r.right() - 4.0, ground),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(110, 90, 60)),
        );
        for l in &g.sticks {
            let pts: Vec<egui::Pos2> = l.iter().map(|p| at(*p)).collect();
            painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, STEEL)));
        }
        for (_, a, b, _) in &g.members {
            painter.line_segment([at(*a), at(*b)], egui::Stroke::new(1.0, STEEL));
        }
        for (_, a, b) in &g.strings {
            painter.line_segment([at(*a), at(*b)], egui::Stroke::new(2.0, STRING));
        }
        for (_, p) in &g.points {
            painter.circle_filled(at(*p), 3.0, POINT);
        }
        for (_, p, _) in &g.wires {
            painter.circle_filled(at(*p), 3.0, WIRE);
        }
        painter.text(
            r.left_bottom() + egui::vec2(6.0, -2.0),
            egui::Align2::LEFT_BOTTOM,
            name,
            egui::FontId::proportional(11.0),
            egui::Color32::GRAY,
        );
    }
    painter.text(
        rect.right_bottom() + egui::vec2(-6.0, -2.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{top:.1} m"),
        egui::FontId::proportional(10.0),
        egui::Color32::GRAY,
    );
}

/// The 3D scene of `g`.
fn scene(g: &crate::type_workspace::Geometry) -> ols_render::Scene {
    let mut s = ols_render::Scene::new([0.0; 3]);
    let c = |c: egui::Color32| [c.r(), c.g(), c.b(), 255];
    for l in &g.sticks {
        s.polyline(l, c(STEEL), 2.0);
    }
    for (_, a, b, _) in &g.members {
        s.polyline(&[*a, *b], c(STEEL), 1.5);
    }
    for (_, a, b) in &g.strings {
        s.polyline(&[*a, *b], c(STRING), 2.5);
    }
    for (_, p) in &g.points {
        s.point(*p, c(POINT), 9.0);
    }
    for (_, p, _) in &g.wires {
        s.point(*p, c(WIRE), 9.0);
    }
    s
}

/// A button showing the type chosen (`structure_type`, its `extension`),
/// `width` wide; clicked: open the picker.
pub fn button(
    ui: &mut egui::Ui,
    model: &Model,
    structure_type: Option<&str>,
    extension: Option<&str>,
    none: &str,
    width: f32,
) -> egui::Response {
    let name = structure_type.and_then(|id| {
        let (types, _) = model.types.as_ref()?;
        Some(types.get(id)?.name.clone())
    });
    let text = match (structure_type, &name) {
        (None, _) => none.to_owned(),
        (Some(id), Some(n)) if n != id => format!("{id} — {n}"),
        (Some(id), _) => id.to_owned(),
    };
    let text = match extension {
        Some(e) => format!("{text} ({e})"),
        None => text,
    };
    ui.add_sized(
        [width, ui.spacing().interact_size.y],
        egui::Button::new(format!("{text} …")).truncate(),
    )
    .on_hover_text(format!(
        "{text}\nClick to choose from the table of structure types (with a preview)"
    ))
}
