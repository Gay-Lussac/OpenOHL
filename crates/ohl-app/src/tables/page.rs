// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! An open table page: pending changes, filter, sort, the colour and type
//! cells, the choices of a cell, the table drawn.

use super::*;

/// The values a cell is chosen from (a dropdown), and whether several can
/// be ticked (joined with `; `); `None`: typed.
pub(super) fn choices(
    kind: TableKind,
    column: usize,
    model: &Model,
    cells: &[String],
) -> Option<(Vec<String>, bool)> {
    let name = kind.columns()[column].0;
    let with_none = |v: Vec<String>| {
        let mut out = vec![String::new()];
        out.extend(v);
        out
    };
    let cables = || {
        model
            .library
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    };
    Some(match (kind, name) {
        (TableKind::Structures, "type") => (
            with_none(
                model
                    .types
                    .as_ref()
                    .map(|(s, _)| s.iter().map(|t| t.id.clone()).collect())
                    .unwrap_or_default(),
            ),
            false,
        ),
        (TableKind::Sections | TableKind::Jumpers, "cable") => (cables(), false),
        (TableKind::Sections, "system") => (
            with_none(model.line.systems.iter().map(|s| s.id.clone()).collect()),
            false,
        ),
        (TableKind::Sections, "calculation") => (
            ohl_line::condition::Calculation::ALL
                .iter()
                .map(|c| c.label().to_owned())
                .collect(),
            false,
        ),
        (TableKind::Sections, "status") => {
            (vec!["in the blocks".into(), "clipped in".into()], false)
        }
        (TableKind::Sections, "condition") => {
            let spans = cells
                .get(TableKind::Sections.column("points"))
                .map_or(0, |p| p.split(',').filter(|x| !x.trim().is_empty()).count())
                .saturating_sub(1);
            let mut v = vec!["tension".to_owned(), "sag, ruling span".to_owned()];
            v.extend((1..=spans).map(|k| format!("sag, span {k}")));
            (v, false)
        }
        (TableKind::Sections, "display cases") => (
            model.cases.iter().map(|c| c.case.name.clone()).collect(),
            true,
        ),
        (TableKind::Insulators, "kind") => (
            ["suspension", "strain", "two strings"]
                .map(str::to_owned)
                .to_vec(),
            false,
        ),
        (TableKind::Insulators, "swing across" | "swing along" | "twist") => {
            (["free", "blocked"].map(str::to_owned).to_vec(), false)
        }
        (TableKind::Insulators, "arrangement") => (
            ohl_structures::Arrangement::ALL
                .iter()
                .map(|a| a.as_str().to_owned())
                .collect(),
            false,
        ),
        (TableKind::StringShapes, "library label") => (
            model
                .types
                .as_ref()
                .map(|(_, i)| i.iter().map(|a| a.id.clone()).collect())
                .unwrap_or_default(),
            false,
        ),
        (TableKind::StringShapes, "kind") => (
            ohl_structures::ShapeKind::ALL
                .iter()
                .map(|k| k.as_str().to_owned())
                .collect(),
            false,
        ),
        (TableKind::StringShapes, "state") => (
            ohl_structures::ElectricalState::ALL
                .iter()
                .map(|k| k.as_str().to_owned())
                .collect(),
            false,
        ),
        (TableKind::StringShapes, "shared") => (vec![String::new(), "yes".into()], false),
        (TableKind::Strings, "library label") => (
            with_none(
                model
                    .types
                    .as_ref()
                    .map(|(_, i)| i.iter().map(|a| a.id.clone()).collect())
                    .unwrap_or_default(),
            ),
            false,
        ),
        (TableKind::Strings, "clamp") => (vec!["suspension".into(), "strain".into()], false),
        (TableKind::Jumpers, "rigid") => (vec!["no".into(), "yes".into()], false),
        (TableKind::ConductorPaths, "starts") => (circuit_ends(cells), true),
        (TableKind::Circuits, "system") => (
            with_none(model.line.systems.iter().map(|x| x.id.clone()).collect()),
            false,
        ),
        (TableKind::ConductorPaths, "system") => (
            with_none(model.line.systems.iter().map(|x| x.id.clone()).collect()),
            false,
        ),
        (TableKind::Systems, "kind") => (vec!["AC".into(), "DC".into()], false),
        (TableKind::WeatherCases, "Wind from\n ") => (vec!["right".into(), "left".into()], false),
        (TableKind::AlignmentPoints, "structure") => (
            with_none(model.line.structures.iter().map(|s| s.id.clone()).collect()),
            false,
        ),
        (TableKind::AlignmentPoints, "alignment") => (
            with_none(model.line.alignments.iter().map(|a| a.id.clone()).collect()),
            false,
        ),
        (TableKind::Alignments, "route") => (
            model.line.routes.iter().map(|r| r.id.clone()).collect(),
            false,
        ),
        (TableKind::Structures, "routes") => (
            model.line.routes.iter().map(|r| r.id.clone()).collect(),
            true,
        ),
        _ => return None,
    })
}

/// A cell chosen from a list; true when changed.
pub(super) fn choice_cell(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    text: &mut String,
    (values, multi): (Vec<String>, bool),
    color: Option<egui::Color32>,
    none: Option<&str>,
) -> bool {
    let mut changed = false;
    let mut shown = egui::RichText::new(if text.is_empty() {
        "—"
    } else {
        text.as_str()
    });
    if let Some(c) = color {
        shown = shown.color(c);
    }
    // Exactly the column's width: the combo box's own width leaves out
    // its arrow and padding, and wider cells shifted the columns after it.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cell.set_clip_rect(rect.intersect(ui.clip_rect()));
    let ui = &mut cell;
    let arrow = ui.spacing().icon_width + 2.0 * ui.spacing().button_padding.x + 4.0;
    let mut combo = egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width((width - arrow).max(10.0))
        .height(400.0);
    if multi {
        combo = combo.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    }
    combo.show_ui(ui, |ui| {
        if multi {
            let mut ticked: Vec<String> = text
                .split(';')
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty())
                .collect();
            for v in &values {
                let mut on = ticked.contains(v);
                if ui.checkbox(&mut on, v).changed() {
                    if on {
                        ticked.push(v.clone());
                    } else {
                        ticked.retain(|t| t != v);
                    }
                    // In the order of the list.
                    ticked.sort_by_key(|t| values.iter().position(|x| x == t));
                    *text = ticked.join("; ");
                    changed = true;
                }
            }
            // Display cases: none is "the cases shown" (circuits' starts
            // need one: no such button).
            if let Some(label) = none
                && ui.small_button(label).clicked()
            {
                text.clear();
                changed = true;
            }
        } else {
            for v in &values {
                let label = if v.is_empty() { "—" } else { v.as_str() };
                if ui.selectable_label(text == v, label).clicked() && text != v {
                    *text = v.clone();
                    changed = true;
                }
            }
        }
    });
    changed
}

/// An open table page: pending changes, filter, sort.
#[derive(Default)]
pub struct TablePage {
    /// Pending cell texts of existing rows: row key → column → text.
    pub edits: BTreeMap<String, BTreeMap<usize, String>>,
    /// New rows' cells.
    pub added: Vec<Vec<String>>,
    /// Rows marked for deletion (keys).
    pub deleted: BTreeSet<String>,
    /// Rows shown contain this (any cell, any case).
    pub filter: String,
    /// Sorted by this column (ascending when true).
    pub sort: Option<(usize, bool)>,
    /// Why Apply failed, or what it did.
    pub message: Option<(String, bool)>,
    /// What the deletions take with them, waiting for a second Apply.
    pub confirm: Option<Vec<String>>,
    /// A new route's name typed (Alignments table: "+ New route").
    pub new_route: String,
    /// The alignment shown (Alignment points table; empty: all).
    pub alignment: String,
    /// The colour tool, open for a colour cell.
    pub color: Option<(ColorCell, crate::color_tool::ColorTool)>,
    /// The structure picker, open for a structure's type (Structures).
    pub type_picker: Option<(TypeCell, crate::structure_picker::Picker)>,
    /// The rows shown and what they were built from
    /// ([`TablePage::cached_rows`]; 2026-10-27: built every frame before).
    rows: Option<(RowsKey, Arc<Vec<Row>>)>,
}

/// What a table's rows are built from: the table and weather case, the
/// line (its version: every edit counts it up), the library files, the
/// ground, each weather case's solution ([`crate::model::Solved::serial`]),
/// the cases sections are drawn in, the model's files, the plugin columns
/// and whether the project can be edited.
struct RowsKey {
    kind: TableKind,
    case: usize,
    line: u64,
    libraries: u64,
    ground: (u64, usize),
    editable: bool,
    solutions: Vec<u64>,
    display_cases: BTreeMap<String, Vec<usize>>,
    /// The model's files the rows name: its file, weather cases,
    /// structure types, insulators.
    files: (PathBuf, [Option<PathBuf>; 3]),
    plugin_columns: Vec<PluginColumn>,
    /// The route worked on (2026-11-01: its rows only).
    route: Option<String>,
}

/// The ground's revision and surface (its address).
fn ground_of(model: &Model) -> (u64, usize) {
    (
        model.revisions.ground,
        model.ground.as_ref().map_or(0, |g| Arc::as_ptr(g) as usize),
    )
}

impl RowsKey {
    fn of(kind: TableKind, model: &Model, case: usize) -> Self {
        Self {
            kind,
            case,
            line: model.line_version,
            libraries: model.revisions.libraries,
            ground: ground_of(model),
            editable: model.can_edit().is_ok(),
            solutions: model.cases.iter().map(|c| c.serial).collect(),
            display_cases: model.display_cases.clone(),
            files: (
                model.sources.file.clone(),
                [
                    model.sources.weather.clone(),
                    model.sources.structures.clone(),
                    model.sources.insulators.clone(),
                ],
            ),
            plugin_columns: model.plugin_columns.clone(),
            route: model.work_route.clone(),
        }
    }

    /// Whether the rows built for `self` are those of `kind` now (compared
    /// without copying anything).
    fn matches(&self, kind: TableKind, model: &Model, case: usize) -> bool {
        self.kind == kind
            && self.case == case
            && self.line == model.line_version
            && self.libraries == model.revisions.libraries
            && self.ground == ground_of(model)
            && self.solutions.len() == model.cases.len()
            && self
                .solutions
                .iter()
                .zip(&model.cases)
                .all(|(s, c)| *s == c.serial)
            && self.display_cases == model.display_cases
            && self.route == model.work_route
            && self.files.0 == model.sources.file
            && self.files.1[0] == model.sources.weather
            && self.files.1[1] == model.sources.structures
            && self.files.1[2] == model.sources.insulators
            && self.plugin_columns == model.plugin_columns
            && self.editable == model.can_edit().is_ok()
    }
}

/// A structure's type cell the structure picker writes to (with its
/// extension cell).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeCell {
    /// The row `key`, whose type and extension were these.
    Row {
        /// The row's key.
        key: String,
        /// Its type (empty: none).
        structure_type: String,
        /// Its extension (empty: the base).
        extension: String,
    },
    /// New row `index`.
    New {
        /// The new row.
        index: usize,
    },
}

/// What the table pages share with the application: the colour tool's
/// palette, the GPU and the structure picker's preview.
pub struct TableTools<'a> {
    /// The colour tool's palette.
    pub palette: &'a mut crate::color_tool::Palette,
    /// The GPU (the structure picker's 3D preview).
    pub rs: Option<&'a eframe::egui_wgpu::RenderState>,
    /// The structure picker's preview.
    pub preview: &'a mut crate::preview3d::Preview,
}

/// A colour cell the colour tool writes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorCell {
    /// Column `column` of the row `key`, whose value was `original` (empty
    /// for a default shown: what is picked is set even when the same).
    Row {
        /// The row's key.
        key: String,
        /// The column.
        column: usize,
        /// Its value.
        original: String,
    },
    /// Column `column` of new row `index`.
    New {
        /// The new row.
        index: usize,
        /// The column.
        column: usize,
    },
}

/// What a table asks of the application.
pub enum TableAction {
    /// Make these changes.
    Apply(Changes),
    /// Select this object.
    Select(Selection),
    /// Show weather case `k`.
    ShowCase(usize),
    /// Replace structure `id` (Replace structure, [`crate::replace`]).
    Replace(String),
    /// Open this structure in the structure application (one without a
    /// type: made a structure file first).
    EditInStructureApp(String),
    /// Open a cross-section at this structure ([`crate::xsection`]).
    CrossSection(String),
    /// Open the String builder on this assembly ([`crate::string_builder`]).
    StringBuilder(String),
}

impl TablePage {
    /// Number of changes pending.
    pub fn pending(&self) -> usize {
        self.edits.values().map(BTreeMap::len).sum::<usize>()
            + self.added.len()
            + self.deleted.len()
    }

    /// Drops every pending change.
    pub fn clear(&mut self) {
        self.edits.clear();
        self.added.clear();
        self.deleted.clear();
        self.confirm = None;
    }

    /// Opens the colour tool for `cell`, showing `text` (its colour, if
    /// it reads) as the old colour.
    pub fn open_color_tool(&mut self, kind: TableKind, cell: ColorCell, text: &str) {
        let what = match &cell {
            ColorCell::Row { key, .. } => key.clone(),
            ColorCell::New { .. } => "new row".into(),
        };
        let old = ohl_core::text::parse_rgb(text.trim());
        self.color = Some((
            cell,
            crate::color_tool::ColorTool::new(format!("Colour — {}: {what}", kind.title()), old),
        ));
    }

    /// Writes a colour chosen in the colour tool to its cell (a typed edit,
    /// pending until Apply).
    pub fn write_color(&mut self, cell: &ColorCell, color: [u8; 3]) {
        let text = ohl_core::text::format_rgb(color);
        match cell {
            ColorCell::Row {
                key,
                column,
                original,
            } => self.set(key, *column, &text, original),
            ColorCell::New { index, column } => {
                if let Some(c) = self.added.get_mut(*index).and_then(|r| r.get_mut(*column)) {
                    *c = text;
                }
            }
        }
    }

    /// Opens the structure picker for a structure's type cell, showing
    /// `structure_type` and `extension` (the cells' texts) chosen.
    pub fn open_type_picker(&mut self, cell: TypeCell, structure_type: &str, extension: &str) {
        let what = match &cell {
            TypeCell::Row { key, .. } => key.clone(),
            TypeCell::New { .. } => "new row".into(),
        };
        let none = |t: &str| (!t.trim().is_empty()).then(|| t.trim().to_owned());
        self.type_picker = Some((
            cell,
            crate::structure_picker::Picker::new(
                format!("Structure type — {what}"),
                none(structure_type).as_deref(),
                none(extension).as_deref(),
                true,
                true,
            ),
        ));
    }

    /// Writes a type (and extension) chosen in the structure picker to its
    /// cells (typed edits, pending until Apply).
    pub fn write_type(&mut self, cell: &TypeCell, structure_type: &str, extension: &str) {
        let (t, e) = (
            TableKind::Structures.column("type"),
            TableKind::Structures.column("extension"),
        );
        match cell {
            TypeCell::Row {
                key,
                structure_type: old_type,
                extension: old_extension,
            } => {
                self.set(key, t, structure_type, old_type);
                self.set(key, e, extension, old_extension);
            }
            TypeCell::New { index } => {
                if let Some(row) = self.added.get_mut(*index) {
                    for (c, v) in [(t, structure_type), (e, extension)] {
                        if let Some(x) = row.get_mut(c) {
                            *x = v.to_owned();
                        }
                    }
                }
            }
        }
    }

    /// Sets a cell of an existing row (back to its value: not pending).
    pub fn set(&mut self, key: &str, column: usize, text: &str, original: &str) {
        if text == original {
            if let Some(e) = self.edits.get_mut(key) {
                e.remove(&column);
                if e.is_empty() {
                    self.edits.remove(key);
                }
            }
        } else {
            self.edits
                .entry(key.to_owned())
                .or_default()
                .insert(column, text.to_owned());
        }
        self.confirm = None;
    }

    /// The rows of `kind` in weather case `case`: built when what they
    /// are built from changed ([`RowsKey`]), else those kept (display
    /// only: the same rows either way).
    pub fn cached_rows(&mut self, kind: TableKind, model: &Model, case: usize) -> Arc<Vec<Row>> {
        if let Some((key, rows)) = &self.rows
            && key.matches(kind, model, case)
        {
            return Arc::clone(rows);
        }
        let rows = Arc::new(kind.rows(model, case));
        self.rows = Some((RowsKey::of(kind, model, case), Arc::clone(&rows)));
        rows
    }

    /// What Apply does: the changes, or — when deletions take other objects
    /// and that was not confirmed — `None` (the list is in `confirm`).
    pub fn prepare(
        &mut self,
        kind: TableKind,
        model: &Model,
        case: usize,
        confirmed: bool,
    ) -> Result<Option<Changes>, String> {
        let rows = kind.rows(model, case);
        let changes = kind.changes(model, case, &rows, self)?;
        if !changes.takes.is_empty() && !confirmed {
            self.confirm = Some(changes.takes.clone());
            return Ok(None);
        }
        Ok(Some(changes))
    }

    /// The page.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        kind: TableKind,
        model: &Model,
        case: usize,
        selection: &Selection,
        tools: &mut TableTools<'_>,
    ) -> Option<TableAction> {
        // Weather cases are a file of the model, not part of the project.
        let editable = match kind {
            TableKind::WeatherCases => weather_path(model).is_some(),
            _ => model.can_edit().is_ok(),
        };
        // Its own columns, then the plugins'.
        let own = kind.columns().len();
        let plugin_columns = kind.plugin_columns(model);
        let columns = columns_of(kind, &plugin_columns);
        let choices_of = |c: usize, cells: &[String]| {
            if c < own {
                return choices(kind, c, model, cells);
            }
            let p = &plugin_columns[c - own];
            (!p.values.is_empty()).then(|| {
                let mut v = vec![String::new()];
                v.extend(p.values.iter().cloned());
                (v, false)
            })
        };
        let none_of = |c: usize| if c < own { kind.none_choice(c) } else { None };
        for cells in &mut self.added {
            cells.resize(columns.len(), String::new());
        }
        let rows = self.cached_rows(kind, model, case);
        let row_height = 20.0;
        let buttons = 52.0;
        let type_column = (kind == TableKind::Structures).then(|| kind.column("type"));
        let extension_column = kind.column_of("extension");
        let cx = Table {
            kind,
            model,
            case,
            selection,
            editable,
            columns: &columns,
            choices_of: &choices_of,
            none_of: &none_of,
            type_column,
            extension_column,
            row_height,
            buttons,
        };
        let mut asks = Asks::default();
        route_note(ui, kind, model);
        self.toolbar_ui(ui, &cx, &mut asks, &rows);
        self.confirm_ui(ui, &mut asks);
        weather_cases_note(ui, kind, model);
        self.alignments_ui(ui, &cx, &mut asks);
        self.alignment_points_ui(ui, kind, model);
        self.apply_ui(ui, &cx, &mut asks);
        let order = self.order(kind, &rows);
        let added = self.added.len();
        // The grid: a line at every column edge and under every row, from
        // the columns' widths (each cell `width` wide, 2 apart, after the
        // two row buttons) — every row the same widths whatever its cells.
        let widths: Vec<f32> = columns.iter().map(|c| c.1).collect();
        let grid = move |ui: &egui::Ui, row: egui::Rect| {
            let stroke =
                egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
            let p = ui.painter();
            let mut x = row.left() + buttons - 1.0;
            p.line_segment(
                [egui::pos2(x, row.top()), egui::pos2(x, row.bottom())],
                stroke,
            );
            for w in &widths {
                x += w + 2.0;
                p.line_segment(
                    [egui::pos2(x, row.top()), egui::pos2(x, row.bottom())],
                    stroke,
                );
            }
            p.line_segment(
                [
                    egui::pos2(row.left() + buttons - 1.0, row.bottom() + 1.0),
                    egui::pos2(x, row.bottom() + 1.0),
                ],
                stroke,
            );
        };
        egui::ScrollArea::horizontal().show(ui, |ui| {
            let header = self.header_ui(ui, &cx);
            grid(ui, header.response.rect);
            ui.add_space(2.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, order.len() + added, |ui, range| {
                    for i in range {
                        if i >= order.len() {
                            // A new row.
                            let k = i - order.len();
                            let cells = self.new_row_ui(ui, &cx, &mut asks, k);
                            grid(ui, cells.response.rect);
                            continue;
                        }
                        let row = &rows[order[i]];
                        let cells = self.row_ui(ui, &cx, &mut asks, row);
                        grid(ui, cells.response.rect);
                    }
                });
        });
        let Asks {
            action,
            open_tool,
            open_picker,
            ..
        } = asks;
        if let Some((cell, text)) = open_tool {
            self.open_color_tool(kind, cell, &text);
        }
        if let Some((cell, t, e)) = open_picker {
            self.open_type_picker(cell, &t, &e);
        }
        self.tools_ui(ui, kind, model, tools);
        action
    }

    /// The bar above the table: the filter, Add row, Apply, Revert, the
    /// number of rows, the String builder's menu.
    fn toolbar_ui(&mut self, ui: &mut egui::Ui, cx: &Table<'_>, asks: &mut Asks, rows: &[Row]) {
        let Table {
            kind,
            model,
            case,
            editable,
            columns,
            ..
        } = *cx;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filter")
                    .desired_width(160.0),
            );
            ui.separator();
            if ui
                .add_enabled(editable && kind.adds_rows(), egui::Button::new("+ Add row"))
                .on_hover_text("A new object, typed in the row (pending until Apply)")
                .on_disabled_hover_text(match kind {
                    TableKind::Spacers => "Add spacers with Add string, between two span points",
                    TableKind::SpanObjects => "Add them with the Add on span tool",
                    _ => "Read-only",
                })
                .clicked()
            {
                let mut cells = kind.new_row(model, case, &self.added);
                cells.resize(columns.len(), String::new());
                self.added.push(cells);
                self.confirm = None;
            }
            let n = self.pending();
            asks.apply = ui
                .add_enabled(
                    editable && n > 0,
                    egui::Button::new(format!("Apply {n} changes")),
                )
                .on_hover_text("All of them in one command (one undo)")
                .clicked();
            if ui.add_enabled(n > 0, egui::Button::new("Revert")).clicked() {
                self.clear();
                self.message = None;
            }
            ui.label(
                egui::RichText::new(format!("{} rows", rows.len()))
                    .small()
                    .weak(),
            );
            // The String builder on an assembly (2026-10-31): the shapes
            // built in a window of their own, with a preview.
            if matches!(kind, TableKind::StringShapes | TableKind::Insulators)
                && let Some((_, library)) = model.types.as_ref()
            {
                ui.separator();
                ui.menu_button("String builder ▾", |ui| {
                    for a in library.iter() {
                        if ui.button(format!("{} — {}", a.id, a.name)).clicked() {
                            asks.action = Some(TableAction::StringBuilder(a.id.clone()));
                        }
                    }
                })
                .response
                .on_hover_text(
                    "An assembly's shapes in the String builder: presets, common fittings, a \
                     grading ring (torus), each field explained, a preview",
                );
            }
            if !editable {
                ui.label(egui::RichText::new("read-only").small().weak());
            }
        });
    }

    /// What the deletions take with them, to be confirmed.
    fn confirm_ui(&mut self, ui: &mut egui::Ui, asks: &mut Asks) {
        if let Some(takes) = &self.confirm {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "This also changes (one log entry, undo brings everything back):",
            );
            egui::ScrollArea::vertical()
                .id_salt("table-confirm")
                .max_height(160.0)
                .show(ui, |ui| {
                    for t in takes {
                        ui.label(egui::RichText::new(format!("  {t}")).small());
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Apply all that").clicked() {
                    asks.apply = true;
                    asks.confirmed = true;
                }
                if ui.button("Cancel").clicked() {
                    self.confirm = None;
                }
            });
        }
    }

    /// Alignments: a new route made here, and what the table is.
    fn alignments_ui(&mut self, ui: &mut egui::Ui, cx: &Table<'_>, asks: &mut Asks) {
        let Table {
            kind,
            model,
            editable,
            ..
        } = *cx;
        if kind == TableKind::Alignments {
            // An alignment needs a route: one is made here when there is none.
            let stored = model.stored_line().unwrap_or(&model.line);
            ui.horizontal(|ui| {
                if stored.routes.is_empty() {
                    crate::menu::message(
                        ui,
                        egui::RichText::new("No route yet: an alignment needs one. Create it:")
                            .small(),
                        Some(ui.visuals().warn_fg_color),
                    );
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_route)
                        .hint_text("+ New route…")
                        .desired_width(140.0),
                );
                let name = self.new_route.trim().to_owned();
                if ui
                    .add_enabled(
                        editable && !name.is_empty(),
                        egui::Button::new("Create the route"),
                    )
                    .clicked()
                {
                    match crate::editors::new_route(stored, &name, &name) {
                        Ok((what, command)) => {
                            self.new_route.clear();
                            asks.action = Some(TableAction::Apply(Changes {
                                commands: vec![(what, command)],
                                ..Changes::default()
                            }));
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
            });
            ui.label(
                egui::RichText::new("Each alignment belongs to one route. # orders them (3.5 goes between 3 and 4). Their points: Alignment points table.")
                    .small()
                    .weak(),
            );
        }
    }

    /// Alignment points: the alignment shown, and what the table is.
    fn alignment_points_ui(&mut self, ui: &mut egui::Ui, kind: TableKind, model: &Model) {
        if kind == TableKind::AlignmentPoints {
            let stored = model.stored_line().unwrap_or(&model.line);
            ui.horizontal(|ui| {
                ui.label("Alignment");
                let shown = if self.alignment.is_empty() {
                    "all"
                } else if self.alignment == "\t" {
                    "(on no alignment)"
                } else {
                    self.alignment.as_str()
                };
                egui::ComboBox::from_id_salt("points-alignment")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.alignment, String::new(), "all");
                        for a in &stored.alignments {
                            ui.selectable_value(
                                &mut self.alignment,
                                a.id.clone(),
                                format!("{} (route {})", a.id, a.route),
                            );
                        }
                        ui.selectable_value(
                            &mut self.alignment,
                            "\t".to_owned(),
                            "(on no alignment)",
                        );
                    });
            });
            ui.label(
                egui::RichText::new("The profile view follows these points. A point on several alignments has a row on each, with its place (#) there; # orders the points (3.5 goes between 3 and 4). Deleting a row deletes the point: it comes off every alignment, the others keep their order. A point may wait on no alignment.")
                    .small()
                    .weak(),
            );
        }
    }

    /// Apply asked for: the changes prepared; and the page's message.
    fn apply_ui(&mut self, ui: &mut egui::Ui, cx: &Table<'_>, asks: &mut Asks) {
        let Table {
            kind, model, case, ..
        } = *cx;
        if asks.apply {
            match self.prepare(kind, model, case, asks.confirmed) {
                Ok(Some(c)) => asks.action = Some(TableAction::Apply(c)),
                Ok(None) => {}
                Err(e) => self.message = Some((e, true)),
            }
        }
        if let Some((m, error)) = &self.message {
            let t = egui::RichText::new(m).small();
            if *error {
                crate::menu::error_label(ui, t);
            } else if m.contains("warning:") {
                crate::menu::message(ui, t, Some(ui.visuals().warn_fg_color));
            } else {
                ui.colored_label(crate::menu::ok_color(ui), t);
            }
        }
    }

    /// The rows shown, in the order shown: filtered and sorted.
    fn order(&self, kind: TableKind, rows: &[Row]) -> Vec<usize> {
        // Filter and sort (on the cells as shown, edits included).
        let shown_cell = |r: &Row, c: usize| -> String {
            self.edits
                .get(&r.key)
                .and_then(|e| e.get(&c))
                .cloned()
                .unwrap_or_else(|| r.cells[c].clone())
        };
        let needle = self.filter.trim().to_lowercase();
        // Alignment points: the alignment chosen ("\t": on none).
        let wanted = |r: &Row| {
            kind != TableKind::AlignmentPoints
                || self.alignment.is_empty()
                || (self.alignment == "\t" && r.cells[0].is_empty())
                || r.cells[0] == self.alignment
        };
        let mut order: Vec<usize> = (0..rows.len())
            .filter(|&i| wanted(&rows[i]))
            .filter(|&i| {
                needle.is_empty()
                    || rows[i]
                        .cells
                        .iter()
                        .any(|c| c.to_lowercase().contains(&needle))
            })
            .collect();
        if let Some((c, up)) = self.sort {
            // Each row's cell and its number worked out once, not twice
            // per comparison (2026-10-26; the same order).
            let mut keys: Vec<Option<(String, Option<f64>)>> = vec![None; rows.len()];
            for &i in &order {
                let x = shown_cell(&rows[i], c);
                let n = x.trim().parse::<f64>().ok();
                keys[i] = Some((x, n));
            }
            order.sort_by(|&a, &b| {
                let (Some((x, p)), Some((y, q))) = (&keys[a], &keys[b]) else {
                    return std::cmp::Ordering::Equal;
                };
                let o = match (p, q) {
                    (Some(p), Some(q)) => p.total_cmp(q),
                    // sec2 before sec10.
                    _ => ohl_core::text::natural_cmp(x, y),
                };
                if up { o } else { o.reverse() }
            });
        }
        order
    }

    /// The header: a column's name sorts by it.
    fn header_ui(&mut self, ui: &mut egui::Ui, cx: &Table<'_>) -> egui::InnerResponse<()> {
        let Table {
            columns,
            row_height,
            ..
        } = *cx;
        // Headers of two lines: the name, then the unit.
        let header_height = if columns.iter().any(|c| c.0.contains('\n')) {
            34.0
        } else {
            row_height
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // Where the rows have their two buttons.
            ui.add_sized([24.0, header_height], egui::Label::new(""));
            ui.add_sized([24.0, header_height], egui::Label::new(""));
            for (c, col) in columns.iter().enumerate() {
                let arrow = match self.sort {
                    Some((s, true)) if s == c => " ⏶",
                    Some((s, false)) if s == c => " ⏷",
                    _ => "",
                };
                // One style in every table: sentence case, the unit
                // its help gives in brackets ("E [m]"; the column's
                // key stays as it is).
                let shown = crate::style::column_header(&col.0, &col.3);
                let text = egui::RichText::new(format!("{shown}{arrow}")).strong();
                if ui
                    .add_sized([col.1, header_height], egui::Button::new(text).frame(false))
                    .on_hover_text(format!("{} — click to sort", col.3))
                    .clicked()
                {
                    self.sort = match self.sort {
                        Some((s, true)) if s == c => Some((c, false)),
                        Some((s, false)) if s == c => None,
                        _ => Some((c, true)),
                    };
                }
            }
        })
    }

    /// New row `k`: its remove button and its cells.
    fn new_row_ui(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Table<'_>,
        asks: &mut Asks,
        k: usize,
    ) -> egui::InnerResponse<()> {
        let Table {
            kind,
            columns,
            choices_of,
            none_of,
            type_column,
            extension_column,
            row_height,
            buttons,
            ..
        } = *cx;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if ui
                .add_sized([buttons - 2.0, row_height], egui::Button::new("🗑"))
                .on_hover_text("Remove this new row")
                .clicked()
            {
                self.added.remove(k);
                self.confirm = None;
                return;
            }
            for (c, col) in columns.iter().enumerate() {
                if col.2 == Derived {
                    ui.add_sized(
                        [col.1, row_height],
                        egui::Label::new(egui::RichText::new("new").weak()),
                    );
                    continue;
                }
                if Some(c) == type_column {
                    if type_cell(
                        ui,
                        [col.1, row_height],
                        &self.added[k][c],
                        Some(crate::menu::ok_color(ui)),
                    ) {
                        let e = extension_column
                            .and_then(|e| self.added[k].get(e).cloned())
                            .unwrap_or_default();
                        asks.open_picker =
                            Some((TypeCell::New { index: k }, self.added[k][c].clone(), e));
                    }
                    continue;
                }
                if let Some(list) = choices_of(c, &self.added[k]) {
                    let mut text = self.added[k][c].clone();
                    if choice_cell(
                        ui,
                        ("new", kind, k, c),
                        col.1,
                        &mut text,
                        list,
                        Some(crate::menu::ok_color(ui)),
                        none_of(c),
                    ) {
                        self.added[k][c] = text;
                    }
                    continue;
                }
                if col.0 == "colour" {
                    let (tool, _) = color_cell(
                        ui,
                        [col.1, row_height],
                        &mut self.added[k][c],
                        Some(crate::menu::ok_color(ui)),
                        false,
                        &col.3,
                    );
                    if tool {
                        asks.open_tool = Some((
                            ColorCell::New {
                                index: k,
                                column: c,
                            },
                            self.added[k][c].clone(),
                        ));
                    }
                    continue;
                }
                let edit = egui::TextEdit::singleline(&mut self.added[k][c])
                    .text_color(crate::menu::ok_color(ui));
                ui.add_sized([col.1, row_height], edit);
            }
        })
    }

    /// An existing row: its two buttons and its cells.
    fn row_ui(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Table<'_>,
        asks: &mut Asks,
        row: &Row,
    ) -> egui::InnerResponse<()> {
        let Table {
            editable, columns, ..
        } = *cx;
        let deleted = self.deleted.contains(&row.key);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            go_button(ui, cx, asks, row);
            self.row_button(ui, cx, row, deleted);
            for (c, col) in columns.iter().enumerate() {
                if deleted || col.2 != Yes || !editable {
                    fixed_cell(ui, cx, row, c, deleted);
                    continue;
                }
                self.edit_cell(ui, cx, asks, row, c);
            }
        })
    }

    /// A row's second button: Reverse (Conductor paths), else delete or
    /// keep.
    fn row_button(&mut self, ui: &mut egui::Ui, cx: &Table<'_>, row: &Row, deleted: bool) {
        let Table {
            kind,
            editable,
            row_height,
            ..
        } = *cx;
        if kind == TableKind::ConductorPaths {
            // Reverse: its starts become its ends and
            // back (pending until Apply).
            let shown = self
                .edits
                .get(&row.key)
                .and_then(|e| e.get(&4))
                .cloned()
                .unwrap_or_else(|| row.cells[4].clone());
            let starts: Vec<&str> = shown.split(';').map(str::trim).collect();
            let flipped: Vec<String> = circuit_ends(&row.cells)
                .into_iter()
                .filter(|p| !starts.contains(&p.as_str()))
                .collect();
            if ui
                .add_enabled(
                    editable && !flipped.is_empty(),
                    egui::Button::new("⬌").min_size(egui::vec2(24.0, row_height)),
                )
                .on_hover_text(
                    "Reverse: its starts become its ends and its ends its starts (on Apply)",
                )
                .clicked()
            {
                self.edits
                    .entry(row.key.clone())
                    .or_default()
                    .insert(4, flipped.join("; "));
                self.confirm = None;
            }
        } else if ui
            .add_enabled(
                editable && !kind.fixed_rows(),
                egui::Button::selectable(deleted, "🗑").min_size(egui::vec2(24.0, row_height)),
            )
            .on_hover_text(if deleted {
                "Marked for deletion: click to keep it"
            } else {
                "Delete it (on Apply)"
            })
            .clicked()
        {
            if deleted {
                self.deleted.remove(&row.key);
            } else {
                self.deleted.insert(row.key.clone());
            }
            self.confirm = None;
        }
    }

    /// A cell that can be edited: the type's button, a choice, a colour
    /// or a text typed.
    fn edit_cell(
        &mut self,
        ui: &mut egui::Ui,
        cx: &Table<'_>,
        asks: &mut Asks,
        row: &Row,
        c: usize,
    ) {
        let Table {
            kind,
            choices_of,
            none_of,
            type_column,
            extension_column,
            row_height,
            ..
        } = *cx;
        let col = &cx.columns[c];
        let original = &row.cells[c];
        let edited = self.edits.get(&row.key).and_then(|e| e.get(&c));
        let mut text = edited.cloned().unwrap_or_else(|| original.clone());
        // A default shown (not stored): dimmed until typed over.
        let default = edited.is_none() && row.defaulted.get(c).copied().unwrap_or(false);
        let dim = ui.visuals().weak_text_color();
        if Some(c) == type_column {
            if type_cell(
                ui,
                [col.1, row_height],
                &text,
                edited.is_some().then_some(ui.visuals().warn_fg_color),
            ) {
                let e = extension_column.unwrap_or(c);
                let shown_extension = self
                    .edits
                    .get(&row.key)
                    .and_then(|x| x.get(&e))
                    .cloned()
                    .unwrap_or_else(|| row.cells[e].clone());
                asks.open_picker = Some((
                    TypeCell::Row {
                        key: row.key.clone(),
                        structure_type: original.clone(),
                        extension: row.cells[e].clone(),
                    },
                    text.clone(),
                    shown_extension,
                ));
            }
            return;
        }
        if let Some(list) = choices_of(c, &row.cells) {
            let color = match (edited, default) {
                (Some(_), _) => Some(ui.visuals().warn_fg_color),
                (None, true) => Some(dim),
                _ => None,
            };
            if choice_cell(
                ui,
                (kind, &row.key, c),
                col.1,
                &mut text,
                list,
                color,
                none_of(c),
            ) {
                self.set(&row.key, c, &text, original);
            }
            return;
        }
        if col.0 == "colour" {
            let hover = if default {
                format!(
                    "default (not set on it; type or pick to set it) — {}",
                    col.3
                )
            } else {
                col.3.to_string()
            };
            let (tool, changed) = color_cell(
                ui,
                [col.1, row_height],
                &mut text,
                edited.is_some().then_some(ui.visuals().warn_fg_color),
                default,
                &hover,
            );
            if changed {
                self.set(&row.key, c, &text, original);
            }
            if tool {
                asks.open_tool = Some((
                    ColorCell::Row {
                        key: row.key.clone(),
                        column: c,
                        // A default: set even when the same.
                        original: if default {
                            String::new()
                        } else {
                            original.clone()
                        },
                    },
                    text.clone(),
                ));
            }
            return;
        }
        let mut edit = egui::TextEdit::singleline(&mut text);
        // A number (a column with a unit): right-
        // aligned, as the read-only numbers.
        if crate::style::unit_of(&col.3).is_some() {
            edit = edit.horizontal_align(egui::Align::RIGHT);
        }
        if edited.is_some() {
            edit = edit.text_color(ui.visuals().warn_fg_color);
        } else if default {
            edit = edit.text_color(dim);
        }
        let hover = if default {
            format!("default (not set on it; type to set it) — {}", col.3)
        } else {
            col.3.to_string()
        };
        if ui
            .add_sized([col.1, row_height], edit)
            .on_hover_text(hover)
            .changed()
        {
            self.set(&row.key, c, &text, original);
        }
    }

    /// The structure picker and the colour tool, open for a cell.
    fn tools_ui(
        &mut self,
        ui: &egui::Ui,
        kind: TableKind,
        model: &Model,
        tools: &mut TableTools<'_>,
    ) {
        if let Some((cell, picker)) = &mut self.type_picker {
            let id = egui::Id::new(("structure-picker", kind));
            match picker.show(ui.ctx(), id, model, tools.rs, tools.preview) {
                Some(crate::structure_picker::Outcome::Choose {
                    structure_type,
                    extension,
                }) => {
                    let cell = cell.clone();
                    self.write_type(
                        &cell,
                        structure_type.as_deref().unwrap_or(""),
                        extension.as_deref().unwrap_or(""),
                    );
                    self.type_picker = None;
                }
                Some(crate::structure_picker::Outcome::Cancel) => self.type_picker = None,
                None => {}
            }
        }
        if let Some((cell, tool)) = &mut self.color {
            let id = egui::Id::new(("colour-tool", kind));
            match tool.show(ui.ctx(), id, tools.palette) {
                Some(crate::color_tool::Outcome::Pick(c)) => {
                    let cell = cell.clone();
                    self.write_color(&cell, c);
                    self.color = None;
                }
                Some(crate::color_tool::Outcome::Cancel) => self.color = None,
                None => {}
            }
        }
    }
}

/// A column as the page shows it: title, width, how it is edited, help.
type PageColumn = (Cow<'static, str>, f32, Edit, Cow<'static, str>);

/// What [`choices`] gives: the values a cell is chosen from and whether
/// several can be ticked; `None`: typed.
type Choices = Option<(Vec<String>, bool)>;

/// What the page's parts share: the table, the model and the case shown,
/// the selection, whether it can be edited, the columns, a cell's choices
/// and its "none", the type and extension columns (Structures), the rows'
/// height and the width of their two buttons.
#[derive(Clone, Copy)]
struct Table<'a> {
    kind: TableKind,
    model: &'a Model,
    case: usize,
    selection: &'a Selection,
    editable: bool,
    columns: &'a [PageColumn],
    choices_of: &'a dyn Fn(usize, &[String]) -> Choices,
    none_of: &'a dyn Fn(usize) -> Option<&'static str>,
    type_column: Option<usize>,
    extension_column: Option<usize>,
    row_height: f32,
    buttons: f32,
}

/// What the page's parts ask for.
#[derive(Default)]
struct Asks {
    /// What the page asks of the application.
    action: Option<TableAction>,
    /// Apply clicked,
    apply: bool,
    /// and what the deletions take with them confirmed.
    confirmed: bool,
    /// A colour cell's tool button clicked: the cell and its text.
    open_tool: Option<(ColorCell, String)>,
    /// A structure's type cell clicked: the cell, its type and extension.
    open_picker: Option<(TypeCell, String, String)>,
}

/// The page's columns: the table's own, then the plugins'.
fn columns_of(kind: TableKind, plugin_columns: &[PluginColumn]) -> Vec<PageColumn> {
    kind.columns()
        .iter()
        .map(|c| (Cow::Borrowed(c.0), c.1, c.2, Cow::Borrowed(c.3)))
        .chain(plugin_columns.iter().map(|c| {
            let help = if c.in_use {
                format!("{} (plugin {})", c.help, c.plugin)
            } else {
                c.help.clone()
            };
            let title = if c.in_use {
                c.title.clone()
            } else {
                format!("{} (unused)", c.title)
            };
            let width = (title.chars().count() as f32 * 7.5).clamp(80.0, 180.0);
            (Cow::Owned(title), width, Yes, Cow::Owned(help))
        }))
        .collect()
}

/// The route worked on: its rows only (owner, 2026-11-01), said.
fn route_note(ui: &mut egui::Ui, kind: TableKind, model: &Model) {
    if let Some(route) = &model.work_route
        && !matches!(
            kind,
            TableKind::Insulators
                | TableKind::StringShapes
                | TableKind::Routes
                | TableKind::WeatherCases
                | TableKind::LoadTypes
        )
    {
        ui.label(
            egui::RichText::new(format!(
                "Route {route} worked on: its rows only (View › Work on route › All routes \
                     shows every row)"
            ))
            .small()
            .color(ui.visuals().warn_fg_color),
        );
    }
}

/// Weather cases: where they are kept, and the cases that want an ice
/// density.
fn weather_cases_note(ui: &mut egui::Ui, kind: TableKind, model: &Model) {
    if kind == TableKind::WeatherCases {
        ui.label(
                egui::RichText::new(match &model.sources.weather {
                    Some(p) => format!(
                        "{} — Apply writes the file (comments kept) and solves the model again",
                        crate::menu::file_label(model, p)
                    ),
                    None if weather_path(model).is_some() => "The project uses a default case (15 °C): add cases here (+ Add row), Apply keeps them in the project".into(),
                    None => "The model uses a default case (15 °C): make a case file in Line › Criteria › Weather cases library and choose Use for the model".into(),
                })
                .small()
                .weak(),
            );
        // An iced wind case without an ice density (physics review
        // 2026-10-26): said, asking for one.
        for note in model.cases.iter().filter_map(|c| c.case.ice_density_note()) {
            crate::menu::message(
                ui,
                egui::RichText::new(note).small(),
                Some(ui.visuals().warn_fg_color),
            );
        }
    }
}

/// A row's first button: selects it (shows the weather case); a
/// structure's has a menu.
fn go_button(ui: &mut egui::Ui, cx: &Table<'_>, asks: &mut Asks, row: &Row) {
    let Table {
        kind,
        model,
        case,
        selection,
        editable,
        row_height,
        ..
    } = *cx;
    let selected = match kind {
        TableKind::WeatherCases => row.key == case.to_string(),
        _ => crate::selection::is_selected(selection, model, &row.select),
    };
    let go = ui
        .add_sized([24.0, row_height], egui::Button::selectable(selected, "⤴"))
        .on_hover_text(match kind {
            TableKind::WeatherCases => "Show this weather case",
            TableKind::Structures => {
                "Select it and bring the views to it (right-click: \
                                         Replace structure…, Open this type in \
                                         OHL-Structure…, Cross-section here)"
            }
            _ => "Select it and bring the views to it",
        });
    // A structure: replaced by another type.
    if let (TableKind::Structures, Selection::Structure(id)) = (kind, &row.select) {
        go.context_menu(|ui| {
            if ui
                .add_enabled(editable, egui::Button::new("Replace structure…"))
                .on_hover_text(
                    "Another structure type in its place: matching wire \
                                             points are rewired, the others you map",
                )
                .clicked()
            {
                asks.action = Some(TableAction::Replace(id.clone()));
                ui.close();
            }
            if ui
                .button(crate::type_workspace::EDIT_IN_APP)
                .on_hover_text(
                    "Its structure type in OHL-Structure (its own window), with the loads of \
                                             this place; one without a type is made a \
                                             structure file first",
                )
                .clicked()
            {
                asks.action = Some(TableAction::EditInStructureApp(id.clone()));
                ui.close();
            }
            if ui
                .button("Cross-section here")
                .on_hover_text(
                    "A cut across the line in the plane of its \
                                             cross-arms, in a tab of its own",
                )
                .clicked()
            {
                asks.action = Some(TableAction::CrossSection(id.clone()));
                ui.close();
            }
        });
    }
    if go.clicked() {
        asks.action = Some(match kind {
            TableKind::WeatherCases => TableAction::ShowCase(row.key.parse().unwrap_or(0)),
            _ => TableAction::Select(row.select.clone()),
        });
    }
}

/// A cell that is not edited (derived, read-only, or of a row marked for
/// deletion): its text, a colour on its colour.
fn fixed_cell(ui: &mut egui::Ui, cx: &Table<'_>, row: &Row, c: usize, deleted: bool) {
    let row_height = cx.row_height;
    let col = &cx.columns[c];
    let original = &row.cells[c];
    let mut t = egui::RichText::new(original).weak();
    if deleted {
        t = t.strikethrough().color(ui.visuals().error_fg_color);
    }
    // A colour read-only: on its colour.
    let swatch = (col.0 == "colour" && !deleted)
        .then(|| ohl_core::text::parse_rgb(original.trim()))
        .flatten()
        .map(crate::color_tool::color32);
    if let Some(fill) = swatch {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(col.1, row_height), egui::Sense::hover());
        ui.painter().rect_filled(rect.shrink(1.0), 2.0, fill);
        ui.painter().text(
            rect.left_center() + egui::vec2(4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            original,
            egui::TextStyle::Body.resolve(ui.style()),
            crate::color_tool::text_on(fill),
        );
        return;
    }
    let number = crate::style::unit_of(&col.3).is_some();
    ui.allocate_ui_with_layout(
        egui::vec2(col.1, row_height),
        if number {
            egui::Layout::right_to_left(egui::Align::Center)
        } else {
            egui::Layout::left_to_right(egui::Align::Center)
        },
        |ui| {
            ui.set_min_size(egui::vec2(col.1, row_height));
            ui.add(egui::Label::new(t).truncate())
        },
    )
    .inner
    .on_hover_text(match col.2 {
        Derived => format!("{original} — counted, not typed"),
        New => format!("{original} — {}", col.3),
        Yes => original.clone(),
    });
}

/// A structure's type cell: a button showing the type (marked in `mark`
/// when changed or new); whether it was clicked (open the structure
/// picker).
pub(super) fn type_cell(
    ui: &mut egui::Ui,
    size: [f32; 2],
    text: &str,
    mark: Option<egui::Color32>,
) -> bool {
    let shown = if text.is_empty() { "—" } else { text };
    let mut t = egui::RichText::new(format!("{shown} …"));
    if let Some(c) = mark {
        t = t.color(c);
    }
    ui.add_sized(size, egui::Button::new(t).truncate())
        .on_hover_text(format!(
            "{shown}\nClick to choose from the table of structure types (with a preview)"
        ))
        .clicked()
}

/// A colour cell: its text typed on its colour (dimmed for a `default`
/// shown, outlined in `mark` when changed), and a button opening the colour
/// tool. Whether that button was clicked, and whether the text changed.
pub(super) fn color_cell(
    ui: &mut egui::Ui,
    size: [f32; 2],
    text: &mut String,
    mark: Option<egui::Color32>,
    default: bool,
    hover: &str,
) -> (bool, bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size[0], size[1]), egui::Sense::hover());
    let bg = ui.visuals().extreme_bg_color;
    let fill = ohl_core::text::parse_rgb(text.trim()).map(|c| {
        let c = crate::color_tool::color32(c);
        if default { bg.lerp_to_gamma(c, 0.4) } else { c }
    });
    let text_color = match (fill, mark) {
        (Some(f), _) => {
            let t = crate::color_tool::text_on(f);
            if default { t.gamma_multiply(0.7) } else { t }
        }
        (None, Some(m)) => m,
        (None, None) if default => ui.visuals().weak_text_color(),
        (None, None) => ui.visuals().text_color(),
    };
    let painter = ui.painter();
    painter.rect_filled(rect.shrink(1.0), 2.0, fill.unwrap_or(bg));
    if let Some(m) = mark {
        painter.rect_stroke(
            rect.shrink(1.0),
            2.0,
            egui::Stroke::new(1.5, m),
            egui::StrokeKind::Inside,
        );
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 2.0;
    let (b, button) = child.allocate_exact_size(egui::vec2(18.0, size[1]), egui::Sense::click());
    button.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "colour tool"));
    // Three stripes of colour: the tool.
    let icon = egui::Rect::from_center_size(b.center(), egui::vec2(12.0, 12.0));
    let stripes = [
        egui::Color32::from_rgb(0xe4, 0x57, 0x2e),
        egui::Color32::from_rgb(0xf2, 0xc1, 0x4e),
        egui::Color32::from_rgb(0x4c, 0x9b, 0xe8),
    ];
    for (k, s) in stripes.into_iter().enumerate() {
        let x = icon.left() + 4.0 * k as f32;
        child.painter().rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, icon.top()), egui::vec2(4.0, 12.0)),
            0.0,
            s,
        );
    }
    let outline = if button.hovered() {
        child.visuals().widgets.hovered.fg_stroke
    } else {
        egui::Stroke::new(1.0, text_color)
    };
    child
        .painter()
        .rect_stroke(icon, 1.0, outline, egui::StrokeKind::Outside);
    let button = button.on_hover_text("The colour tool: a wheel, sliders, your palette");
    let edit = egui::TextEdit::singleline(text)
        .frame(egui::Frame::NONE)
        .text_color(text_color);
    let changed = child
        .add_sized([size[0] - 22.0, size[1]], edit)
        .on_hover_text(hover)
        .changed();
    (button.clicked(), changed)
}
