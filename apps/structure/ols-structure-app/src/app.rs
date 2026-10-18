// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The window of OLS Structure: the menus at the top (File, Edit, View,
//! then Geometry, Body, Design, Load cases, Results, In the line, Help),
//! the panels under them (docked, movable, closable: [`crate::layout`]),
//! the last message at the bottom.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use eframe::egui_wgpu;
use ols_structure_edit::{
    Handle, Missing, Need, Place, Removal, StructureFile, geometry_with, missing, next_level,
};
use ols_structure_loads::PlacedValues;
use ols_structures::InsulatorLibrary;

/// How often the line-values file is looked at.
const VALUES_POLL: Duration = Duration::from_millis(500);

use egui_dock::{DockArea, DockState};

use crate::layout::{Area, Panel};
use crate::load_cases_tab::LoadCasesTab;
use crate::results_tab::ResultsTab;
use crate::view::{View, ViewInput};

/// A table of a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sheet {
    /// The file's own fields: name, height, foot, extensions.
    Structure,
    /// Points where insulators hang.
    Points,
    /// Insulators (strings).
    Strings,
    /// Wire points.
    Wires,
    /// Sets on wire points.
    Sets,
    /// Joints.
    Joints,
    /// Members.
    Members,
    /// Body generators.
    Generators,
    /// Member groups: a section and kind per group.
    Groups,
    /// Supports of the joints.
    Supports,
    /// Members' sections, kinds, k.
    Design,
    /// Sections.
    Sections,
    /// Materials.
    Materials,
    /// The pole's outline.
    Pole,
    /// The pole's design.
    PoleDesign,
    /// Height extensions, their pieces and legs.
    Extensions,
}

/// The structure application.
pub struct StructureApp {
    /// The file open.
    pub file: Option<StructureFile>,
    /// Why the last file did not open.
    pub open_error: Option<String>,
    /// Insulator library folder given (else next to the file).
    pub insulators_dir: Option<PathBuf>,
    /// The assemblies strings may take.
    pub insulators: InsulatorLibrary,
    /// The angle that means ahead (the compass).
    pub ahead: f64,
    /// The menu worked in: the one of the panel last focused (the loads
    /// or the results are drawn over the structure while it is theirs).
    pub area: Area,
    /// The table shown in each panel with several.
    pub sheets: HashMap<Panel, Sheet>,
    /// The panels and where they are.
    pub dock: DockState<Panel>,
    /// A panel to bring to the front after the panels are drawn (a link
    /// clicked in one of them).
    pub focus_request: Option<Panel>,
    /// The panels drawn in the last frame.
    pub panels_drawn: Vec<Panel>,
    /// Help › About is open.
    pub about: bool,
    /// Picked, in order (two joints: a member between them).
    pub selected: Vec<Handle>,
    /// A row to bring into view (an item of the header clicked).
    pub scroll_to: Option<Handle>,
    /// Grid a dragged joint or point snaps to, m (0: none).
    pub grid: f64,
    /// Length of a new insulator, m.
    pub string_length: f64,
    /// A deletion waiting for its confirmation.
    pub confirm: Option<Removal>,
    /// What the last action did (or why not: `true`).
    pub message: Option<(String, bool)>,
    /// Fields being typed (sections, materials), by key.
    pub buffers: std::collections::HashMap<String, String>,
    /// The window asked to close with unsaved changes: the question shown.
    pub closing: bool,
    /// File › Export: another section library or load table chosen.
    pub export: crate::export_menu::ExportSettings,
    /// Closing confirmed.
    close_now: bool,
    /// Fit the view to the structure (Fit clicked).
    pub view_fit: bool,
    /// The Load cases tab.
    pub loads: LoadCasesTab,
    /// The Results tab.
    pub results: ResultsTab,
    /// The section library given (`--sections`: a file or folder; none:
    /// `Sections/` beside the file).
    pub sections_path: Option<PathBuf>,
    /// The line-values file of the placed structure (`--line-values`),
    /// read again when it changes: its time when read, when last looked.
    pub line_values: Option<PathBuf>,
    values_seen: Option<(Option<SystemTime>, Instant)>,
    view: View,
    rs: Option<egui_wgpu::RenderState>,
    /// What the view drew last (drawn again when it changes).
    drawn: Option<(String, Vec<Handle>, u64, usize)>,
    /// The window's title as set.
    title: String,
    /// The Body tab's tools: generator form, preview, group filter.
    pub body: crate::body_tab::BodyState,
    /// The Pole panels (the section library next to the file).
    pub pole: crate::pole_panel::PoleState,
    /// The Extensions panel (the extension shown in 3D).
    pub ext: crate::extensions_panel::ExtensionsState,
    /// The Libraries panel and File › Libraries.
    pub libs: crate::libraries::LibrariesState,
    /// The files opened recently (the start page).
    pub recent: crate::start::Recent,
    /// The New structure window.
    pub new_structure: Option<crate::start::NewStructure>,
    /// The plugins (Plugins menu).
    pub plugins: crate::plugins_ui::PluginsState,
}

impl StructureApp {
    /// No file open yet.
    pub fn new(rs: Option<egui_wgpu::RenderState>) -> Self {
        Self {
            file: None,
            open_error: None,
            insulators_dir: None,
            insulators: InsulatorLibrary::default(),
            ahead: 0.0,
            area: Area::default(),
            sheets: HashMap::new(),
            dock: crate::layout::default_dock(),
            focus_request: None,
            panels_drawn: Vec::new(),
            about: false,
            selected: Vec::new(),
            scroll_to: None,
            grid: 0.05,
            string_length: 4.0,
            confirm: None,
            message: None,
            buffers: Default::default(),
            closing: false,
            export: Default::default(),
            close_now: false,
            view_fit: false,
            loads: LoadCasesTab::default(),
            results: ResultsTab::default(),
            sections_path: None,
            line_values: None,
            values_seen: None,
            view: View::default(),
            rs,
            drawn: None,
            title: String::new(),
            body: Default::default(),
            pole: Default::default(),
            ext: Default::default(),
            libs: Default::default(),
            recent: Default::default(),
            new_structure: None,
            plugins: Default::default(),
        }
    }

    /// Opens `path` (what was open is dropped), with the insulator
    /// assemblies of the folder given, else of its folder and its
    /// Insulators folder.
    pub fn open(&mut self, path: &Path) {
        match StructureFile::open(path) {
            Ok(f) => {
                // The level decides the panel in front: where the work is.
                let area = match f.structure().detail().level() {
                    1 => Area::Geometry,
                    2 => Area::Body,
                    _ => Area::Design,
                };
                self.file = Some(f);
                self.open_error = None;
                self.selected.clear();
                self.confirm = None;
                self.message = None;
                self.buffers.clear();
                self.view = View::default();
                self.drawn = None;
                self.results = ResultsTab::default();
                self.load_libraries(path);
                self.libs.listing = None;
                self.recent.add(path);
                self.values_seen = None;
                self.poll_line_values();
                if let Err(e) = self.loads.opened(path) {
                    self.message = Some((format!("typed loads not read: {e}"), true));
                }
                self.body = Default::default();
                self.pole = Default::default();
                self.ext = Default::default();
                // A pole: its own panels.
                let pole = self
                    .file
                    .as_ref()
                    .is_some_and(|f| crate::pole_panel::has_pole(f.structure()));
                self.focus(match area {
                    Area::Body if pole => Panel::Pole,
                    Area::Design if pole => Panel::PoleDesign,
                    _ => area.first_panel(),
                });
            }
            Err(e) => self.open_error = Some(e),
        }
    }

    /// The insulator assemblies (of the folder given, else of the file's
    /// folder and its Insulators folder) and the section library (given,
    /// else `Sections/` beside the file) of structure file `path`.
    pub fn load_libraries(&mut self, path: &Path) {
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let dirs = match &self.insulators_dir {
            Some(d) => vec![d.clone()],
            None => vec![dir.clone(), dir.join("Insulators")],
        };
        self.insulators = InsulatorLibrary::default();
        for d in dirs {
            match InsulatorLibrary::load_dir(&d) {
                Ok(lib) => {
                    for a in lib.iter() {
                        let p = lib.path(&a.id).map(Path::to_path_buf).unwrap_or_default();
                        let _ = self.insulators.insert(&p, a.clone());
                    }
                }
                Err(e) => log::warn!("insulators: {e}"),
            }
        }
        let sections = self
            .sections_path
            .clone()
            .unwrap_or_else(|| dir.join("Sections"));
        self.loads
            .set_sections(ols_structure_link::section_library(&sections), &sections);
        self.pole = Default::default();
    }

    /// File › Open: another structure file, on its own (a placed
    /// structure's loads do not go with it).
    pub fn open_chosen(&mut self, path: &Path) {
        if self.file.as_ref().and_then(StructureFile::path) != Some(path) {
            self.line_values = None;
            self.values_seen = None;
            self.loads.placed = None;
            self.loads.info = None;
            self.loads.values_error = None;
            self.loads.line = Default::default();
        }
        self.open(path);
    }

    /// The line-values file (`--line-values`) read again when its time
    /// changed (looked at twice a second at most: the line application
    /// rewrites it when the line is solved again).
    pub fn poll_line_values(&mut self) {
        let Some(path) = &self.line_values else {
            return;
        };
        if self
            .values_seen
            .is_some_and(|(_, at)| at.elapsed() < VALUES_POLL)
        {
            return;
        }
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let before = self.values_seen.and_then(|(m, _)| m);
        let first = self.values_seen.is_none();
        self.values_seen = Some((modified, Instant::now()));
        if !first && modified == before {
            return;
        }
        if modified.is_none() {
            self.loads.values_error = Some(format!(
                "{}: not there (yet): the line application writes it",
                path.display()
            ));
            return;
        }
        match PlacedValues::load(path) {
            Ok(v) => self.loads.set_line_values(v),
            Err(e) => self.loads.values_error = Some(e),
        }
    }

    /// In the line › Read the line's values again: now.
    pub fn reload_line_values(&mut self) {
        self.values_seen = None;
        self.poll_line_values();
    }

    /// Unsaved changes (of the file, or loads typed in the load table).
    pub fn dirty(&self) -> bool {
        self.file.as_ref().is_some_and(StructureFile::dirty) || self.loads.overrides_dirty
    }

    /// Saves (to `to`: Save as).
    pub fn save(&mut self, to: Option<&Path>) -> bool {
        let Some(f) = &mut self.file else {
            return false;
        };
        match f.save(to) {
            Ok(p) => {
                self.message = Some(match self.loads.saved(&p) {
                    Ok(()) => (format!("saved {}", p.display()), false),
                    Err(e) => (
                        format!("saved {}, but not the typed loads: {e}", p.display()),
                        true,
                    ),
                });
                true
            }
            Err(e) => {
                self.message = Some((e, true));
                false
            }
        }
    }

    /// Save as: a file chosen.
    pub(crate) fn save_as(&mut self) {
        let name = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(Path::file_name)
            .map_or("structure.olss".into(), |n| {
                n.to_string_lossy().into_owned()
            });
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Structure", &["olss"])
            .set_file_name(name)
            .save_file()
        {
            self.save(Some(&p.with_extension("olss")));
        }
    }

    /// Undo.
    pub fn undo(&mut self) {
        if let Some(f) = &mut self.file {
            self.message = Some(match f.undo() {
                Some(w) => (format!("undone: {w}"), false),
                None => ("nothing to undo".into(), true),
            });
            self.buffers.clear();
        }
    }

    /// Redo.
    pub fn redo(&mut self) {
        if let Some(f) = &mut self.file {
            self.message = Some(match f.redo() {
                Some(w) => (format!("made again: {w}"), false),
                None => ("nothing to redo".into(), true),
            });
            self.buffers.clear();
        }
    }

    /// The keys: undo, redo, save, save as (not while typing in a field,
    /// which has its own undo).
    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::{Key, Modifiers};
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let (redo, undo, save_as, save) = ctx.input_mut(|i| {
            (
                i.consume_key(shift, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y),
                i.consume_key(Modifiers::COMMAND, Key::Z),
                i.consume_key(shift, Key::S),
                i.consume_key(Modifiers::COMMAND, Key::S),
            )
        });
        if redo {
            self.redo();
        } else if undo {
            self.undo();
        }
        if save_as {
            self.save_as();
        } else if save && self.dirty() {
            self.save(None);
        }
    }

    /// The window closing with unsaved changes: asked first.
    fn close_question(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && !self.close_now {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.closing = true;
        }
        if !self.closing {
            return;
        }
        let mut open = true;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("The structure has changes not saved. Save them before closing?");
                if let Some((m, true)) = &self.message {
                    error_label(ui, m.as_str());
                }
                ui.horizontal(|ui| {
                    if ui.button("Save and close").clicked() && self.save(None) {
                        self.close_now = true;
                    }
                    if ui.button("Close without saving").clicked() {
                        self.close_now = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.closing = false;
                    }
                });
            });
        if !open {
            self.closing = false;
        }
        if self.close_now {
            self.closing = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// The header: file, level and what the next level lacks (each item
    /// picks its row), the advice folded, the last message.
    fn header(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure();
        let d = t.detail();
        let mut go: Option<Missing> = None;
        ui.horizontal_wrapped(|ui| {
            ui.heading(format!("Level {} — {}", d.level(), d.name()));
            ui.separator();
            ui.strong(&t.id);
            ui.label(&t.name);
            if let Some(p) = f.path() {
                ui.label(egui::RichText::new(p.display().to_string()).weak().small());
            }
            if f.dirty() {
                ui.colored_label(egui::Color32::YELLOW, "* unsaved");
            }
        });
        if let Some(text) = self.loads.placed_title() {
            let mut open_tab = false;
            ui.horizontal_wrapped(|ui| {
                open_tab = ui
                    .link(egui::RichText::new(text).strong())
                    .on_hover_text("The line's values at this placed structure: In the line")
                    .clicked();
                if let Some(note) = self.loads.info.as_ref().and_then(|i| i.note.as_ref()) {
                    ui.label(egui::RichText::new(note).weak());
                }
            });
            if let Some(e) = &self.loads.values_error {
                error_label(ui, e.as_str());
            }
            if open_tab {
                self.focus_request = Some(Panel::InTheLine);
            }
        }
        let next = next_level(d);
        let items = missing(t, next.unwrap_or(3));
        let (needed, advice): (Vec<&Missing>, Vec<&Missing>) =
            items.iter().partition(|m| m.need == Need::Level);
        if let Some(n) = next {
            ui.label(egui::RichText::new(format!("for level {n}:")).strong());
            // Many items (a member each) scroll in a box of fixed height.
            egui::ScrollArea::vertical()
                .id_salt("missing")
                .max_height(48.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for m in &needed {
                            if ui
                                .link(&m.reason)
                                .on_hover_text("Show it in its table")
                                .clicked()
                            {
                                go = Some((*m).clone());
                            }
                        }
                    });
                });
        }
        if !advice.is_empty() {
            egui::CollapsingHeader::new(format!("advice ({})", advice.len()))
                .id_salt("advice")
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("advice-list")
                        .max_height(80.0)
                        .show(ui, |ui| {
                            for m in &advice {
                                if ui.link(&m.reason).clicked() {
                                    go = Some((*m).clone());
                                }
                            }
                        });
                });
        }
        if let Some(e) = f.error() {
            error_label(ui, format!("the file as edited does not read: {e}"));
        }
        if let Some(m) = go {
            self.go_to(&m);
        }
    }

    /// An item of the header clicked: its panel (opened if closed) and
    /// table, its row picked.
    pub fn go_to(&mut self, m: &Missing) {
        let sheet = match (m.need, m.place) {
            (Need::Level, Place::Members) if m.target.is_some() => Sheet::Design,
            (_, Place::Members) => Sheet::Members,
            (Need::Level, Place::Joints) => Sheet::Supports,
            (_, Place::Joints) => Sheet::Joints,
            (_, Place::Points) => Sheet::Points,
            (_, Place::Sections) => Sheet::Sections,
            (Need::Level, Place::Pole)
                if self
                    .file
                    .as_ref()
                    .is_some_and(|f| f.structure().detail().level() >= 2) =>
            {
                Sheet::PoleDesign
            }
            (_, Place::Pole) => Sheet::Pole,
            (_, Place::Extensions) => Sheet::Extensions,
        };
        let panel = Panel::of_sheet(sheet);
        self.sheets.insert(panel, sheet);
        self.focus_request = Some(panel);
        if let Some(h) = &m.target {
            self.selected = vec![h.clone()];
            self.scroll_to = Some(h.clone());
        }
    }

    /// Brings `panel` to the front (opened where it belongs if closed);
    /// its menu becomes the one worked in.
    pub fn focus(&mut self, panel: Panel) {
        crate::layout::open_panel(&mut self.dock, panel);
        if let Some(a) = panel.area() {
            self.area = a;
        }
    }

    /// `panel` is open (in front or behind another).
    pub fn is_open(&self, panel: Panel) -> bool {
        self.dock.find_tab(&panel).is_some()
    }

    /// The panel in front in the leaf last clicked or opened.
    pub fn focused(&mut self) -> Option<Panel> {
        crate::layout::focused(&mut self.dock)
    }

    /// The table shown in `panel` (its first when none was chosen).
    pub fn sheet(&self, panel: Panel) -> Option<Sheet> {
        let sheets = panel.sheets();
        self.sheets
            .get(&panel)
            .copied()
            .filter(|s| sheets.iter().any(|x| x.0 == *s))
            .or(sheets.first().map(|s| s.0))
    }

    /// The panels as kept from the last run (none kept, or not read: the
    /// default arrangement).
    pub fn restore(&mut self, storage: Option<&dyn eframe::Storage>) {
        if let Some(dock) =
            storage.and_then(|s| eframe::get_value::<DockState<Panel>>(s, crate::layout::DOCK_KEY))
        {
            self.dock = dock;
        }
        self.recent = crate::start::Recent::load(storage);
    }

    /// View › Reset layout: the default arrangement.
    pub fn reset_layout(&mut self) {
        self.dock = crate::layout::default_dock();
        let panel = self.area.first_panel();
        crate::layout::open_panel(&mut self.dock, panel);
    }

    /// The window's title: the file, starred with unsaved changes.
    fn title(&mut self, ctx: &egui::Context) {
        let title = match &self.file {
            Some(f) => format!(
                "OLS Structure — {}{}",
                if f.dirty() { "* " } else { "" },
                f.path()
                    .and_then(Path::file_name)
                    .map_or(f.structure().id.clone(), |n| n
                        .to_string_lossy()
                        .into_owned())
            ),
            None => "OLS Structure".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

/// An error in red; long ones scroll in a box of fixed height, so the
/// window and its buttons stay in reach.
pub fn error_label(ui: &mut egui::Ui, text: impl Into<egui::RichText>) {
    let text: egui::RichText = text.into();
    let raw = text.text();
    if raw.len() <= 300 && raw.lines().count() <= 4 {
        ui.colored_label(egui::Color32::LIGHT_RED, text);
        return;
    }
    let id = egui::Id::new(("error", raw.len(), raw.lines().next().unwrap_or("")));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(120.0)
            .auto_shrink([false, true])
            .show(ui, |ui| ui.colored_label(egui::Color32::LIGHT_RED, text));
    });
}

impl eframe::App for StructureApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, crate::layout::DOCK_KEY, &self.dock);
        self.recent.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.keys(&ctx);
        self.close_question(&ctx);
        self.title(&ctx);
        if self.line_values.is_some() {
            self.poll_line_values();
            ctx.request_repaint_after(VALUES_POLL);
        }
        // A group of edits (a drag) ends with the pointer let go.
        if !ctx.input(|i| i.pointer.any_down())
            && let Some(f) = &mut self.file
        {
            f.end();
        }
        self.results.poll();
        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        self.status_bar(ui);
        self.windows(&ctx);
        if self.file.is_none() {
            egui::CentralPanel::default().show(ui, |ui| self.start_page(ui));
            return;
        }
        // The generator's preview is drawn while its panel is.
        if !self.panels_drawn.contains(&Panel::Generators) {
            self.body.preview.clear();
        }
        let before = (self.body.preview_stamp(), self.area);
        let mut dock = std::mem::replace(&mut self.dock, DockState::new(Vec::new()));
        let mut viewer = crate::layout::Viewer {
            app: self,
            drawn: Vec::new(),
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                DockArea::new(&mut dock)
                    .show_close_buttons(true)
                    .show_leaf_close_all_buttons(false)
                    .show_inside(ui, &mut viewer);
            });
        self.panels_drawn = viewer.drawn;
        self.dock = dock;
        if let Some(p) = self.focus_request.take() {
            self.focus(p);
        } else if let Some(a) = self.focused().and_then(Panel::area) {
            self.area = a;
        }
        // The view drew before a panel changed what it shows: again.
        if before != (self.body.preview_stamp(), self.area) {
            ctx.request_repaint();
        }
    }
}

impl StructureApp {
    /// The last message, at the bottom of the window.
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let Some((m, error)) = &self.message else {
            return;
        };
        let (m, error) = (m.clone(), *error);
        egui::Panel::bottom("status").show(ui, |ui| {
            let text = egui::RichText::new(m).small();
            if error {
                error_label(ui, text);
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, text);
            }
        });
    }

    /// One panel's content.
    pub fn panel_ui(&mut self, ui: &mut egui::Ui, panel: Panel) {
        match panel {
            Panel::Level => self.header(ui),
            Panel::View3d => self.view_ui(ui),
            Panel::LoadRecipes => self.load_recipes_ui(ui),
            Panel::LoadTable => self.load_table_ui(ui),
            Panel::Results => self.results_ui(ui),
            Panel::InTheLine => self.in_the_line_ui(ui),
            Panel::Libraries => self.libraries_ui(ui),
            _ => self.sheet_panel(ui, panel),
        }
    }

    /// The 3D view, with the loads or the results of the menu worked in
    /// drawn over the structure.
    fn view_ui(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &mut self.file else {
            return;
        };
        let t = f.structure().clone();
        let shown = self.ext.shown.clone().filter(|n| t.extension(n).is_some());
        let g = geometry_with(&t, &self.insulators, shown.as_deref());
        // Drawn again when the file, what is picked, the compass, the
        // preview or the extension shown change.
        let key = (
            f.text(),
            self.selected.clone(),
            self.ahead.to_bits(),
            self.body.preview_stamp() ^ shown.as_ref().map_or(0, |n| n.len() + 1),
        );
        if self.drawn.as_ref() != Some(&key) {
            self.view.redraw();
            self.drawn = Some(key);
        }
        if std::mem::take(&mut self.view_fit) {
            self.view.fit();
        }
        let overlay = match self.area {
            Area::LoadCases => self.loads.overlay(&g),
            Area::Results => self.results.overlay(),
            _ => None,
        };
        self.view.ui(
            ui,
            self.rs.as_ref(),
            f,
            ViewInput {
                g: &g,
                foot: t.foot_level(),
                selected: &mut self.selected,
                grid: self.grid,
                ahead: self.ahead,
                message: &mut self.message,
                overlay,
                preview: &self.body.preview,
            },
        );
    }
}
