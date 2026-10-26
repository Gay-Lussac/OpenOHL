// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The window of OHL-Structure: the menus at the top (File, Edit, View,
//! then Geometry, Body, Design, Load cases, Results, Line loads, Help),
//! the panels under them (docked, movable, closable: [`crate::layout`]),
//! the last message at the bottom.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use eframe::egui_wgpu;
use ohl_structure_edit::{
    Geometry, Handle, Missing, Need, Place, Removal, StructureFile, geometry_with, missing,
    next_level,
};
use ohl_structure_loads::PlacedValues;
use ohl_structures::InsulatorLibrary;

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
    /// The section library's sections, typed here.
    Library,
    /// The pole's outline.
    Pole,
    /// The pole's design.
    PoleDesign,
    /// Height extensions, their pieces and legs.
    Extensions,
}

/// What a geometry is kept for: the file's revision, the libraries'
/// stamp, the extension shown.
type GeometryKey = (u64, u64, Option<String>);

/// What the list of what the next level lacks is kept for: the file's
/// revision, the level.
type MissingKey = (u64, u8);

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
    /// What the view drew last (drawn again when it changes): the file's
    /// revision, what is picked, the options, the preview.
    drawn: Option<(u64, Vec<Handle>, u64, usize)>,
    /// Changes when the libraries are read again (what is kept from them
    /// is worked out again).
    libs_stamp: u64,
    /// The geometry of the file's revision, the libraries' stamp and the
    /// extension shown (worked out again when one changes, not every
    /// frame: a large lattice took tens of ms).
    geometry: Option<(GeometryKey, std::sync::Arc<Geometry>)>,
    /// What the next level lacks, for the file's revision and that level.
    missing: Option<(MissingKey, std::sync::Arc<Vec<Missing>>)>,
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
    /// The section library's sections typed (Design › Sections &
    /// materials › Section library), their undo and redo.
    pub section_editor: crate::section_editor::SectionEditor,
    /// The Link tool, held (Geometry › Link tool): [`crate::link_tool`].
    pub link: Option<crate::link_tool::LinkTool>,
    /// Where "Insert a joint" puts it on a member: the fraction of its
    /// length from its first joint (the member's menu).
    pub insert_at: f64,
    /// The rigid links drawn in the 3D view (View › Rigid links).
    pub rigid_links: bool,
    /// The volumes on the strings drawn in the 3D view (View › Volumes on
    /// the strings; off by default).
    pub string_volumes: bool,
    /// The messages as notices at the bottom right.
    pub notices: crate::notices::Notices,
    /// The UI scale (View › UI scale; kept between runs).
    pub ui_scale: f32,
    /// The scale and style applied to the window (none yet).
    applied_scale: Option<f32>,
    /// The short "No line values" line of the load panels closed (its ×),
    /// until the program is started again.
    pub line_notice_closed: bool,
    /// The hotbar of the 3D view's tools ([`crate::tools`]; kept).
    pub hotbar: crate::tools::Hotbar,
    /// The tool held last frame (the Link tool follows a change).
    pub held: crate::tools::Tool,
    /// The inventory (E) is open.
    pub inventory: bool,
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
            libs_stamp: 0,
            geometry: None,
            missing: None,
            title: String::new(),
            body: Default::default(),
            pole: Default::default(),
            ext: Default::default(),
            libs: Default::default(),
            recent: Default::default(),
            new_structure: None,
            plugins: Default::default(),
            section_editor: Default::default(),
            link: None,
            insert_at: 0.5,
            rigid_links: true,
            string_volumes: false,
            notices: Default::default(),
            ui_scale: 1.0,
            applied_scale: None,
            line_notice_closed: false,
            hotbar: Default::default(),
            held: crate::tools::Tool::Select,
            inventory: false,
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
                self.link = None;
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
        self.libs_stamp += 1;
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
            .set_sections(ohl_structure_link::section_library(&sections), &sections);
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
                "{}: not there (yet): OHL-Design writes it",
                path.display()
            ));
            return;
        }
        match PlacedValues::load(path) {
            Ok(v) => self.loads.set_line_values(v),
            Err(e) => self.loads.values_error = Some(e),
        }
    }

    /// Line loads › Read the values again: now.
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
            .map_or("structure.ohls".into(), |n| {
                n.to_string_lossy().into_owned()
            });
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Structure", &["ohls"])
            .set_file_name(name)
            .save_file()
        {
            self.save(Some(&p.with_extension("ohls")));
        }
    }

    /// Undo: the last edit, of the structure file or of the section
    /// library (in the order they were made).
    pub fn undo(&mut self) {
        let steps = self.file.as_ref().map(|f| f.undo_steps());
        if self.section_editor.undo_first(steps) {
            self.message = Some(match self.undo_library() {
                Ok(m) => (m, false),
                Err(e) => (e, true),
            });
            return;
        }
        if let Some(f) = &mut self.file {
            self.message = Some(match f.undo() {
                Some(w) => (format!("undone: {w}"), false),
                None => ("nothing to undo".into(), true),
            });
            self.buffers.clear();
        }
    }

    /// What undo would take back.
    pub fn undo_name(&self) -> Option<String> {
        let steps = self.file.as_ref().map(|f| f.undo_steps());
        if self.section_editor.undo_first(steps) {
            return self.section_editor.undo_name();
        }
        self.file
            .as_ref()
            .and_then(|f| f.undo_name())
            .map(str::to_owned)
    }

    /// What redo would make again.
    pub fn redo_name(&mut self) -> Option<String> {
        let (steps, redo) = self
            .file
            .as_ref()
            .map_or((None, 0), |f| (Some(f.undo_steps()), f.redo_steps()));
        if self.section_editor.redo_first(steps, redo) {
            return self.section_editor.redo_name();
        }
        self.file
            .as_ref()
            .and_then(|f| f.redo_name())
            .map(str::to_owned)
    }

    /// Redo: what was undone last, of the structure file or the section
    /// library.
    pub fn redo(&mut self) {
        let (steps, redo) = self
            .file
            .as_ref()
            .map_or((None, 0), |f| (Some(f.undo_steps()), f.redo_steps()));
        if self.section_editor.redo_first(steps, redo) {
            self.message = Some(match self.redo_library() {
                Ok(m) => (m, false),
                Err(e) => (e, true),
            });
            return;
        }
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
        if self.file.is_some() {
            // Escape: the tool let go (with the Link tool, the point
            // chosen dropped first); 1-9: a slot of the hotbar; E: the
            // inventory.
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.let_go_of_tool();
            }
            const SLOTS: [Key; 9] = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ];
            if let Some(k) = ctx.input_mut(|i| {
                SLOTS
                    .iter()
                    .position(|key| i.consume_key(Modifiers::NONE, *key))
            }) {
                self.hotbar.active = k;
                self.tool_changed();
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::E)) {
                self.inventory = !self.inventory;
            }
        }
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

    /// What the next level lacks and the advice, for the file's revision
    /// (kept until it changes): the level they are for (none: level 3
    /// reached), what it needs, the advice.
    fn open_items(&mut self) -> Option<(Option<u8>, Vec<Missing>, Vec<Missing>)> {
        let f = self.file.as_ref()?;
        let d = f.structure().detail();
        let next = next_level(d);
        let level = next.unwrap_or(3);
        let key = (f.revision(), level);
        let items = match &self.missing {
            Some((k, items)) if *k == key => std::sync::Arc::clone(items),
            _ => {
                let items = std::sync::Arc::new(missing(f.structure(), level));
                self.missing = Some((key, std::sync::Arc::clone(&items)));
                items
            }
        };
        let (needed, advice): (Vec<Missing>, Vec<Missing>) =
            items.iter().cloned().partition(|m| m.need == Need::Level);
        Some((next, needed, advice))
    }

    /// The header above the panels (owner: UI review step 1, 2026-10-26):
    /// the file's name (its full path on hover), the level, unsaved, a
    /// button "N to do" bringing the To do list (the Level panel) to the
    /// front; the placed structure's loads; a file that does not read.
    pub(crate) fn header(&mut self, ui: &mut egui::Ui) {
        let Some((next, needed, advice)) = self.open_items() else {
            return;
        };
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure();
        let d = t.detail();
        let mut open_tab = false;
        let mut to_do = false;
        ui.horizontal_wrapped(|ui| {
            let name = f
                .path()
                .and_then(Path::file_name)
                .map_or(t.id.clone(), |n| n.to_string_lossy().into_owned());
            let r = ui.strong(name);
            match f.path() {
                Some(p) => r.on_hover_text(format!("{} — {}\n{}", t.id, t.name, p.display())),
                None => r.on_hover_text(format!("{} — {}", t.id, t.name)),
            };
            if f.dirty() {
                ui.colored_label(ui.visuals().warn_fg_color, "* unsaved");
            }
            ui.separator();
            let badge = egui::RichText::new(format!("Level {} — {}", d.level(), d.name()))
                .strong()
                .color(ui.visuals().strong_text_color());
            egui::Frame::new()
                .fill(ui.visuals().selection.bg_fill.gamma_multiply(0.5))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(6, 1))
                .show(ui, |ui| {
                    ui.label(badge).on_hover_text(match next {
                        Some(n) => format!("What level {n} needs: the To do list"),
                        None => "The full design: the highest level".to_owned(),
                    });
                });
            let n = needed.len() + advice.len();
            if n > 0 {
                let text = format!("{n} to do");
                let tip = format!(
                    "{} for level {}, {} advice: the To do list (click an item to show it)",
                    needed.len(),
                    next.unwrap_or(3),
                    advice.len()
                );
                to_do = ui.button(text).on_hover_text(tip).clicked();
            } else {
                ui.colored_label(ok_color(ui), "nothing to do");
            }
            if let Some(text) = self.loads.placed_title() {
                ui.separator();
                open_tab = ui
                    .link(egui::RichText::new(text).strong())
                    .on_hover_text("The line's values at this placed structure: Line loads")
                    .clicked();
                if let Some(note) = self.loads.info.as_ref().and_then(|i| i.note.as_ref()) {
                    ui.label(egui::RichText::new(note).weak());
                }
            }
        });
        if self.loads.placed_title().is_some()
            && let Some(e) = &self.loads.values_error
        {
            error_label(ui, e.as_str());
        }
        if let Some(e) = f.error() {
            error_label(ui, format!("the file as edited does not read: {e}"));
        }
        if open_tab {
            self.focus_request = Some(Panel::InTheLine);
        }
        if to_do {
            self.focus_request = Some(Panel::Level);
        }
    }

    /// The To do list (the Level panel): what the next level needs, then
    /// the advice, each item showing its row (and selecting its object)
    /// when clicked.
    fn checklist_ui(&mut self, ui: &mut egui::Ui) {
        let Some((next, needed, advice)) = self.open_items() else {
            return;
        };
        let mut go: Option<Missing> = None;
        let item = |ui: &mut egui::Ui, m: &Missing, go: &mut Option<Missing>| {
            let tip = if m.target.is_some() {
                "Show it in its table and select it"
            } else {
                "Show it in its table"
            };
            if ui
                .link(format!("\u{2022} {}", m.reason))
                .on_hover_text(tip)
                .clicked()
            {
                *go = Some(m.clone());
            }
        };
        match next {
            Some(n) if !needed.is_empty() => {
                ui.strong(format!("For level {n} ({})", needed.len()));
                for m in &needed {
                    item(ui, m, &mut go);
                }
            }
            Some(n) => {
                ui.label(format!("Nothing lacks for level {n}."));
            }
            None => {
                ui.colored_label(ok_color(ui), "Level 3, the full design: nothing lacks.");
            }
        }
        if !advice.is_empty() {
            ui.add_space(6.0);
            egui::CollapsingHeader::new(format!("Advice ({})", advice.len()))
                .id_salt("advice")
                .default_open(needed.is_empty())
                .show(ui, |ui| {
                    for m in &advice {
                        item(ui, m, &mut go);
                    }
                });
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
        // An arrangement of an older version gives way to the default
        // once; one that does not read too.
        let version = storage
            .and_then(|s| eframe::get_value::<u32>(s, crate::layout::DOCK_VERSION_KEY))
            .unwrap_or(1);
        if version >= crate::layout::DOCK_VERSION
            && let Some(dock) = storage
                .and_then(|s| eframe::get_value::<DockState<Panel>>(s, crate::layout::DOCK_KEY))
        {
            self.dock = dock;
        }
        self.recent = crate::start::Recent::load(storage);
        self.ui_scale = crate::style::load_scale(storage);
        self.hotbar = crate::tools::Hotbar::load(storage);
        self.held = self.hotbar.tool();
        if self.held == crate::tools::Tool::Link {
            // The Link tool is not taken up at the start.
            self.hotbar.hold(crate::tools::Tool::Select);
            self.held = crate::tools::Tool::Select;
        }
    }

    /// View › Reset layout: the default arrangement.
    pub fn reset_layout(&mut self) {
        self.dock = crate::layout::default_dock();
        let panel = self.area.first_panel();
        crate::layout::open_panel(&mut self.dock, panel);
    }

    /// Where `p` (local axes) was drawn in the 3D view last (tests).
    #[cfg(test)]
    pub fn view_screen(&self, p: [f64; 3]) -> Option<egui::Pos2> {
        self.view.screen_of(p)
    }

    /// The 3D view's camera (tests).
    #[cfg(test)]
    pub fn view_camera(&self) -> Option<ohl_render::Camera> {
        self.view.camera()
    }

    /// The joint under the pointer with the Link tool (tests).
    #[cfg(test)]
    pub fn view_link_hover(&self) -> Option<String> {
        self.view.link_hover.clone()
    }

    /// The window's title: the file, starred with unsaved changes.
    fn title(&mut self, ctx: &egui::Context) {
        let title = match &self.file {
            Some(f) => format!(
                "{} — {}{}",
                crate::APP_NAME,
                if f.dirty() { "* " } else { "" },
                f.path()
                    .and_then(Path::file_name)
                    .map_or(f.structure().id.clone(), |n| n
                        .to_string_lossy()
                        .into_owned())
            ),
            None => crate::APP_NAME.into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

/// The colour of a good result (nothing wrong): a green readable on the
/// dark and the light theme (errors and warnings take the theme's own
/// colours), as in OHL-Design.
pub(crate) fn ok_color(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::LIGHT_GREEN
    } else {
        egui::Color32::from_rgb(0, 120, 40)
    }
}

/// An error in the theme's error colour; long ones scroll in a box of
/// fixed height, so the window and its buttons stay in reach.
pub fn error_label(ui: &mut egui::Ui, text: impl Into<egui::RichText>) {
    let text: egui::RichText = text.into();
    let raw = text.text();
    if raw.len() <= 300 && raw.lines().count() <= 4 {
        ui.colored_label(ui.visuals().error_fg_color, text);
        return;
    }
    let id = egui::Id::new(("error", raw.len(), raw.lines().next().unwrap_or("")));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(120.0)
            .auto_shrink([false, true])
            .show(ui, |ui| ui.colored_label(ui.visuals().error_fg_color, text));
    });
}

impl eframe::App for StructureApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, crate::layout::DOCK_KEY, &self.dock);
        eframe::set_value(
            storage,
            crate::layout::DOCK_VERSION_KEY,
            &crate::layout::DOCK_VERSION,
        );
        self.recent.save(storage);
        eframe::set_value(storage, crate::style::SCALE_KEY, &self.ui_scale);
        self.hotbar.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.applied_scale != Some(self.ui_scale) {
            if self.applied_scale.is_none() {
                crate::style::apply(&ctx);
            }
            ctx.set_zoom_factor(self.ui_scale);
            self.applied_scale = Some(self.ui_scale);
        }
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
        self.follow_link_tool();
        self.inventory_window(&ctx);
        self.status_bar(ui);
        self.notices.observe(&self.message, ctx.input(|i| i.time));
        self.notices.ui(&ctx);
        self.windows(&ctx);
        if self.file.is_some() {
            egui::Panel::top("header").show(ui, |ui| self.header(ui));
        }
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
                ui.colored_label(crate::app::ok_color(ui), text);
            }
        });
    }

    /// One panel's content.
    pub fn panel_ui(&mut self, ui: &mut egui::Ui, panel: Panel) {
        match panel {
            Panel::Level => self.checklist_ui(ui),
            Panel::View3d => self.view_ui(ui),
            Panel::LoadRecipes => self.load_recipes_ui(ui),
            Panel::LoadTable => self.load_table_ui(ui),
            Panel::Results => self.results_ui(ui),
            Panel::InTheLine => self.in_the_line_ui(ui),
            Panel::Libraries => self.libraries_ui(ui),
            Panel::Inspector => self.inspector_ui(ui),
            _ => self.sheet_panel(ui, panel),
        }
    }

    /// The geometry of structure `t` (the file's at `revision`), standing
    /// on extension `shown`: kept until one of them or the libraries
    /// change.
    pub(crate) fn geometry_of(
        &mut self,
        t: &ohl_structures::StructureType,
        revision: u64,
        shown: Option<String>,
    ) -> std::sync::Arc<Geometry> {
        let key = (revision, self.libs_stamp, shown);
        if let Some((k, g)) = &self.geometry
            && *k == key
        {
            return std::sync::Arc::clone(g);
        }
        let g = std::sync::Arc::new(geometry_with(t, &self.insulators, key.2.as_deref()));
        self.geometry = Some((key, std::sync::Arc::clone(&g)));
        g
    }

    /// The 3D view, with the loads or the results of the menu worked in
    /// drawn over the structure.
    fn view_ui(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &mut self.file else {
            return;
        };
        let t = f.structure_arc();
        let shown = self.ext.shown.clone().filter(|n| t.extension(n).is_some());
        let revision = f.revision();
        let g = self.geometry_of(&t, revision, shown.clone());
        let Some(f) = &mut self.file else {
            return;
        };
        // Drawn again when the file, what is picked, the compass, the
        // preview or the extension shown change.
        let key = (
            f.revision(),
            self.selected.clone(),
            self.ahead.to_bits()
                ^ u64::from(self.rigid_links)
                ^ (u64::from(self.string_volumes) << 1),
            self.body.preview_stamp() ^ shown.as_ref().map_or(0, |n| n.len() + 1),
        );
        if self.drawn.as_ref() != Some(&key) {
            self.view.redraw();
            self.drawn = Some(key);
        }
        if std::mem::take(&mut self.view_fit) {
            self.view.fit();
        }
        let drawn = self.view.overlay_key();
        let overlay = match self.area {
            Area::LoadCases => self.loads.overlay(&g, drawn),
            Area::Results => self.results.overlay(drawn),
            _ => None,
        };
        let g = if self.rigid_links || g.links.is_empty() {
            g
        } else {
            let mut g = (*g).clone();
            g.links.clear();
            std::sync::Arc::new(g)
        };
        let linkable = crate::link_tool::linkable(&t);
        // The volumes on the strings, in still air (placed by the link
        // crate as the line places them).
        let volumes = if self.string_volumes {
            ohl_structure_link::string_volumes(&t, &g.strings, &self.insulators)
        } else {
            Vec::new()
        };
        // The moments of the points' offsets in the case the results show.
        let moments: Vec<(String, String)> = self
            .results
            .shown()
            .map(|r| {
                r.points
                    .iter()
                    .map(|p| {
                        (
                            p.label.clone(),
                            format!(
                                "{}: M = {:.2} kN·m{}",
                                r.case,
                                p.size() / 1000.0,
                                if p.applied {
                                    ""
                                } else {
                                    " (left out: truss joint)"
                                }
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut action = None;
        let tool = self.tool();
        self.tool_strip(ui);
        let Some(f) = &mut self.file else {
            return;
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
                link: self.link.as_mut(),
                linkable: &linkable,
                moments: &moments,
                volumes: &volumes,
                action: &mut action,
                usage_legend: self.area == Area::Results && self.results.shown().is_some(),
                tool,
            },
        );
        match action {
            Some(crate::view::ViewAction::LinkClick(h)) => self.link_click(h),
            Some(crate::view::ViewAction::Cancel) => self.cancel_link(),
            Some(crate::view::ViewAction::Link(a)) => self.link_action(a),
            Some(crate::view::ViewAction::ToolClick(h, at)) => self.tool_click(h, at),
            None => {}
        }
        self.follow_link_tool();
        if let Some(rect) = self.view.rect() {
            self.hotbar_ui(ui, rect);
        }
    }
}
