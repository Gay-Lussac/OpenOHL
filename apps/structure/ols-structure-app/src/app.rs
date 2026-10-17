// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The window: the File and Edit menus, the header (the level and what
//! the next one lacks), the 3D view on top and the tabs underneath.

use std::path::{Path, PathBuf};

use eframe::egui;
use eframe::egui_wgpu;
use ols_structure_edit::{
    Handle, Missing, Need, Place, Removal, StructureFile, geometry, missing, next_level,
};
use ols_structures::InsulatorLibrary;

use crate::load_cases_tab::LoadCasesTab;
use crate::results_tab::ResultsTab;
use crate::view::{View, ViewInput};

/// The tabs under the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// Level 1: points, insulators, wire points, sets, the file's fields.
    #[default]
    Geometry,
    /// Level 2: joints and members.
    Body,
    /// Level 3: supports, sections per member, sections, materials.
    Design,
    /// Load-case recipes and the load table.
    LoadCases,
    /// The finite-element results per case.
    Results,
    /// The places using the structure in a line (to come).
    InTheLine,
}

impl Tab {
    /// Every tab, in order.
    pub const ALL: [Tab; 6] = [
        Tab::Geometry,
        Tab::Body,
        Tab::Design,
        Tab::LoadCases,
        Tab::Results,
        Tab::InTheLine,
    ];

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            Tab::Geometry => "Geometry",
            Tab::Body => "Body",
            Tab::Design => "Design",
            Tab::LoadCases => "Load cases",
            Tab::Results => "Results",
            Tab::InTheLine => "In the line",
        }
    }
}

/// The table shown in a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Body generators (Body tab).
    Generators,
    /// Member groups: a section and kind per group (Design tab).
    Groups,
    /// Supports of the joints.
    Supports,
    /// Members' sections, kinds, k.
    Design,
    /// Sections.
    Sections,
    /// Materials.
    Materials,
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
    /// The tab shown.
    pub tab: Tab,
    /// The table shown in each tab.
    pub sheets: [Sheet; 3],
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
    /// Closing confirmed.
    close_now: bool,
    /// Fit the view to the structure (Fit clicked).
    pub view_fit: bool,
    /// The Load cases tab.
    pub loads: LoadCasesTab,
    /// The Results tab.
    pub results: ResultsTab,
    view: View,
    rs: Option<egui_wgpu::RenderState>,
    /// What the view drew last (drawn again when it changes).
    drawn: Option<(String, Vec<Handle>, u64, usize)>,
    /// The window's title as set.
    title: String,
    /// The Body tab's tools: generator form, preview, group filter.
    pub body: crate::body_tab::BodyState,
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
            tab: Tab::default(),
            sheets: [Sheet::Points, Sheet::Joints, Sheet::Supports],
            selected: Vec::new(),
            scroll_to: None,
            grid: 0.05,
            string_length: 4.0,
            confirm: None,
            message: None,
            buffers: Default::default(),
            closing: false,
            close_now: false,
            view_fit: false,
            loads: LoadCasesTab::default(),
            results: ResultsTab::default(),
            view: View::default(),
            rs,
            drawn: None,
            title: String::new(),
            body: Default::default(),
        }
    }

    /// Opens `path` (what was open is dropped), with the insulator
    /// assemblies of the folder given, else of its folder and its
    /// Insulators folder.
    pub fn open(&mut self, path: &Path) {
        match StructureFile::open(path) {
            Ok(f) => {
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
                // The level decides the tab: where the work is.
                self.tab = match f.structure().detail().level() {
                    1 => Tab::Geometry,
                    2 => Tab::Body,
                    _ => Tab::Design,
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
                if let Err(e) = self.loads.opened(path) {
                    self.message = Some((format!("typed loads not read: {e}"), true));
                }
                self.body = Default::default();
            }
            Err(e) => self.open_error = Some(e),
        }
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
    fn save_as(&mut self) {
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

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                let dirty = self.dirty();
                if ui
                    .add_enabled(!dirty, egui::Button::new("Open…"))
                    .on_disabled_hover_text("Save or undo the changes first")
                    .clicked()
                {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Structure", &["olss"])
                        .pick_file()
                    {
                        self.open(&p);
                    }
                }
                let has = self.file.is_some();
                if ui
                    .add_enabled(dirty, egui::Button::new("Save").shortcut_text("Ctrl+S"))
                    .clicked()
                {
                    ui.close();
                    self.save(None);
                }
                if ui
                    .add_enabled(
                        has,
                        egui::Button::new("Save as…").shortcut_text("Ctrl+Shift+S"),
                    )
                    .clicked()
                {
                    ui.close();
                    self.save_as();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.close();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                let (u, r) = self.file.as_ref().map_or((None, None), |f| {
                    (
                        f.undo_name().map(str::to_owned),
                        f.redo_name().map(str::to_owned),
                    )
                });
                let label = |w: &str, n: &Option<String>| {
                    n.as_ref().map_or(w.to_owned(), |n| format!("{w} {n}"))
                };
                if ui
                    .add_enabled(
                        u.is_some(),
                        egui::Button::new(label("Undo", &u)).shortcut_text("Ctrl+Z"),
                    )
                    .clicked()
                {
                    ui.close();
                    self.undo();
                }
                if ui
                    .add_enabled(
                        r.is_some(),
                        egui::Button::new(label("Redo", &r)).shortcut_text("Ctrl+Shift+Z"),
                    )
                    .clicked()
                {
                    ui.close();
                    self.redo();
                }
            });
            ui.separator();
            let undo_tip = self
                .file
                .as_ref()
                .and_then(|f| f.undo_name())
                .map_or("Nothing to undo".to_owned(), |w| {
                    format!("Undo {w} (Ctrl+Z)")
                });
            let can_undo = self.file.as_ref().is_some_and(|f| f.undo_name().is_some());
            if ui
                .add_enabled(can_undo, egui::Button::new("⟲ Undo"))
                .on_hover_text(&undo_tip)
                .on_disabled_hover_text(&undo_tip)
                .clicked()
            {
                self.undo();
            }
            let can_redo = self.file.as_ref().is_some_and(|f| f.redo_name().is_some());
            if ui
                .add_enabled(can_redo, egui::Button::new("⟳ Redo"))
                .on_hover_text("Make again what was undone (Ctrl+Shift+Z)")
                .clicked()
            {
                self.redo();
            }
            if ui
                .add_enabled(self.dirty(), egui::Button::new("Save"))
                .on_hover_text("Save the file (Ctrl+S)")
                .clicked()
            {
                self.save(None);
            }
        });
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
        if let Some((m, error)) = &self.message {
            let text = egui::RichText::new(m).small();
            if *error {
                error_label(ui, text);
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, text);
            }
        }
        if let Some(m) = go {
            self.go_to(&m);
        }
    }

    /// An item of the header clicked: its tab and table, its row picked.
    pub fn go_to(&mut self, m: &Missing) {
        let (tab, sheet) = match (m.need, m.place) {
            (Need::Level, Place::Members) if m.target.is_some() => (Tab::Design, Sheet::Design),
            (_, Place::Members) => (Tab::Body, Sheet::Members),
            (Need::Level, Place::Joints) => (Tab::Design, Sheet::Supports),
            (_, Place::Joints) => (Tab::Body, Sheet::Joints),
            (_, Place::Points) => (Tab::Geometry, Sheet::Points),
            (_, Place::Sections) => (Tab::Design, Sheet::Sections),
        };
        self.tab = tab;
        self.sheets[tab_index(tab)] = sheet;
        if let Some(h) = &m.target {
            self.selected = vec![h.clone()];
            self.scroll_to = Some(h.clone());
        }
    }

    /// The window without a file.
    fn empty(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("OLS CAD — structure");
            ui.label("A structure file (.olss): its points and insulators, its body, its design.");
            ui.add_space(12.0);
            if ui.button("Open a structure file…").clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Structure", &["olss"])
                    .pick_file()
            {
                self.open(&p);
            }
            if let Some(e) = &self.open_error {
                error_label(ui, e.as_str());
            }
        });
    }

    /// The window's title: the file, starred with unsaved changes.
    fn title(&mut self, ctx: &egui::Context) {
        let title = match &self.file {
            Some(f) => format!(
                "{}{} — OLS CAD structure",
                if f.dirty() { "* " } else { "" },
                f.path()
                    .and_then(Path::file_name)
                    .map_or(f.structure().id.clone(), |n| n
                        .to_string_lossy()
                        .into_owned())
            ),
            None => "OLS CAD structure".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

/// The index of a tab with tables.
pub fn tab_index(tab: Tab) -> usize {
    match tab {
        Tab::Geometry => 0,
        Tab::Body => 1,
        _ => 2,
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.keys(&ctx);
        self.close_question(&ctx);
        self.title(&ctx);
        // A group of edits (a drag) ends with the pointer let go.
        if !ctx.input(|i| i.pointer.any_down())
            && let Some(f) = &mut self.file
        {
            f.end();
        }
        egui::Panel::top("menu").show(ui, |ui| {
            self.menu(ui);
            self.header(ui);
        });
        if self.file.is_none() {
            egui::CentralPanel::default().show(ui, |ui| self.empty(ui));
            return;
        }
        egui::Panel::bottom("tabs")
            .resizable(true)
            .default_size(340.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for tab in Tab::ALL {
                        ui.selectable_value(&mut self.tab, tab, tab.name());
                    }
                });
                ui.separator();
                self.tab_ui(ui);
            });
        let Some(f) = &mut self.file else {
            return;
        };
        let t = f.structure().clone();
        let g = geometry(&t, &self.insulators);
        // Drawn again when the file, what is picked, the compass or the
        // preview change.
        let key = (
            f.text(),
            self.selected.clone(),
            self.ahead.to_bits(),
            self.body.preview_stamp(),
        );
        if self.drawn.as_ref() != Some(&key) {
            self.view.redraw();
            self.drawn = Some(key);
        }
        let rs = self.rs.clone();
        // The tab's drawing over the structure: loads, or results.
        self.results.poll();
        let overlay = match self.tab {
            Tab::LoadCases => self.loads.overlay(&g),
            Tab::Results => self.results.overlay(),
            _ => None,
        };
        egui::CentralPanel::default().show(ui, |ui| {
            self.view.ui(
                ui,
                rs.as_ref(),
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
        });
    }
}

impl StructureApp {
    /// Fits the view to the whole structure.
    pub fn fit(&mut self) {
        self.view.fit();
    }
}
