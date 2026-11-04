// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The menu bar, aligned with OHL-Design's (owner 2026-11-03: "yes
//! implement the menus"): File (new, open, recent, save, Import ▸,
//! Export ▸, Plugins ▸, close, quit), Edit (undo, redo, delete, clear
//! selection, Tools ▸), View (fit, the main panels, Tables ▸, Display
//! options ▸, UI scale ▸, Reset layout: a tick menu that stays open),
//! then the menus of the work — Geometry, Body, Design, Loads, Results —
//! each listing its panels (opened or brought to the front; those of
//! another structure kind left out) and its actions, named as the tools
//! that do the same, and Help; Undo, Redo and Save as icons at the right
//! of the menus. The windows they open: the deletion asked first, About.
//!
//! The conventions: "…" only on an entry that opens a dialog or asks
//! first (never on a panel); an entry greyed out says why on hover.

use eframe::egui;
use ohl_structure_edit::Mirror;
use ohl_structure_edit::body::symmetry_option;

use crate::app::{StructureApp, error_label};
use crate::icons::Icon;
use crate::layout::{Area, Panel, close_panel, open_panel, options_menu};
use crate::tools::Tool;

/// A shortcut as the platform writes it: ⌘ on a Mac, Ctrl elsewhere (as
/// OHL-Design).
pub(crate) fn shortcut(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("Ctrl+", "⌘")
    } else {
        keys.to_owned()
    }
}

/// Why a menu of the work is greyed out.
const NO_FILE: &str = "Open a structure first (File › Open…, or New structure…)";

/// A menu entry: true when clicked (the menu closes).
fn item(ui: &mut egui::Ui, label: &str, enabled: bool, tip: &str, why_not: &str) -> bool {
    entry(ui, None, label, None, enabled, tip, why_not)
}

/// A menu entry with its icon (a tool's, of `assets/icons/tools/`).
fn icon_item(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    tip: &str,
    why_not: &str,
) -> bool {
    entry(ui, Some(icon), label, None, enabled, tip, why_not)
}

/// A menu entry with its icon and its shortcut (none: none).
fn entry(
    ui: &mut egui::Ui,
    icon: Option<Icon>,
    label: &str,
    keys: Option<String>,
    enabled: bool,
    tip: &str,
    why_not: &str,
) -> bool {
    let mut button = match icon {
        Some(i) => i.button(ui, label),
        None => egui::Button::new(label),
    };
    if let Some(k) = keys {
        button = button.shortcut_text(k);
    }
    let mut r = ui.add_enabled(enabled, button);
    if !tip.is_empty() {
        r = r.on_hover_text(tip);
    }
    if !why_not.is_empty() {
        r = r.on_disabled_hover_text(why_not);
    }
    if r.clicked() {
        ui.close();
        true
    } else {
        false
    }
}

/// A grey heading inside a menu.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().weak());
}

impl StructureApp {
    /// The menu bar.
    pub(crate) fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| self.file_menu(ui));
            ui.menu_button("Edit", |ui| self.edit_menu(ui));
            options_menu(ui, "View", |ui| self.view_menu(ui));
            let has = self.file.is_some();
            for area in Area::ALL {
                ui.add_enabled_ui(has, |ui| {
                    ui.menu_button(area.name(), |ui| self.area_menu(ui, area))
                        .response
                })
                .inner
                .on_disabled_hover_text(NO_FILE);
            }
            ui.menu_button("Help", |ui| self.help_menu(ui));
            ui.separator();
            self.quick_buttons(ui);
        });
    }

    /// Undo, Redo and Save as icon buttons right of the menus (the icons
    /// of OHL-Design's quick bar), their names and shortcuts on hover.
    fn quick_buttons(&mut self, ui: &mut egui::Ui) {
        let icon_button = |ui: &mut egui::Ui, icon: &str, name: &str, enabled: bool| {
            let r = ui.add_enabled(
                enabled,
                egui::Button::new(egui::RichText::new(icon).size(16.0))
                    .min_size(egui::vec2(28.0, 24.0)),
            );
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
            r
        };
        let undo_name = self.undo_name();
        let undo_tip = undo_name
            .as_ref()
            .map_or("Undo: nothing to undo".to_owned(), |w| {
                format!("Undo {w} ({})", shortcut("Ctrl+Z"))
            });
        if icon_button(ui, "⟲", "Undo", undo_name.is_some())
            .on_hover_text(&undo_tip)
            .on_disabled_hover_text(&undo_tip)
            .clicked()
        {
            self.undo();
        }
        let redo_name = self.redo_name();
        let redo_tip = redo_name
            .as_ref()
            .map_or("Redo: nothing to redo".to_owned(), |w| {
                format!("Redo {w} ({})", shortcut("Shift+Ctrl+Z"))
            });
        if icon_button(ui, "⟳", "Redo", redo_name.is_some())
            .on_hover_text(&redo_tip)
            .on_disabled_hover_text(&redo_tip)
            .clicked()
        {
            self.redo();
        }
        let save_tip = format!("Save the file ({})", shortcut("Ctrl+S"));
        if icon_button(ui, "💾", "Save", self.dirty())
            .on_hover_text(&save_tip)
            .on_disabled_hover_text("Save: no changes to save")
            .clicked()
        {
            self.save(None);
        }
    }

    /// File › New structure… (and ⌘N): the New structure window, unless
    /// there are changes to save first.
    pub(crate) fn new_structure_asked(&mut self) {
        if self.dirty() {
            self.message = Some(("Save or undo the changes first".into(), true));
            return;
        }
        self.new_structure = Some(crate::start::NewStructure::default());
    }

    /// File › Open… (and ⌘O): a structure file chosen and opened.
    pub(crate) fn open_asked(&mut self) {
        if self.dirty() {
            self.message = Some(("Save or undo the changes first".into(), true));
            return;
        }
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("Structure", &["ohls"])
            .pick_file()
        {
            self.open_chosen(&p);
        }
    }

    /// File › Close: the structure closed (the start page again); the
    /// window stays.
    pub fn close_file(&mut self) {
        if self.dirty() {
            self.message = Some(("Save or undo the changes first".into(), true));
            return;
        }
        let name = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(std::path::Path::file_name)
            .map(|n| n.to_string_lossy().into_owned());
        self.file = None;
        self.selected.clear();
        self.confirm = None;
        self.link = None;
        self.buffers.clear();
        self.results = Default::default();
        self.line_values = None;
        self.loads.placed = None;
        self.loads.info = None;
        self.loads.values_error = None;
        self.loads.line = Default::default();
        self.joints.ask_open = None;
        if let Some(n) = name {
            self.message = Some((format!("closed {n}"), false));
        }
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        let dirty = self.dirty();
        let has = self.file.is_some();
        let save_first = "Save or undo the changes first";
        if entry(
            ui,
            None,
            "New structure…",
            Some(shortcut("Ctrl+N")),
            !dirty,
            "An empty level 1 structure, or one built from a structure template",
            save_first,
        ) {
            self.new_structure_asked();
        }
        if entry(
            ui,
            None,
            "Open…",
            Some(shortcut("Ctrl+O")),
            !dirty,
            "A structure file (.ohls)",
            save_first,
        ) {
            self.open_asked();
        }
        ui.add_enabled_ui(!dirty && !self.recent.files.is_empty(), |ui| {
            ui.menu_button("Open recent", |ui| {
                for p in self.recent.files.clone() {
                    let name = p.file_name().map_or(p.display().to_string(), |n| {
                        n.to_string_lossy().into_owned()
                    });
                    let there = p.is_file();
                    if item(
                        ui,
                        &name,
                        there,
                        &p.display().to_string(),
                        &format!("{} (missing)", p.display()),
                    ) {
                        self.open_chosen(&p);
                    }
                }
            })
            .response
        })
        .inner
        .on_disabled_hover_text(if dirty {
            save_first
        } else {
            "No file opened yet"
        });
        ui.separator();
        if entry(
            ui,
            None,
            "Save",
            Some(shortcut("Ctrl+S")),
            dirty,
            "",
            "No changes to save",
        ) {
            self.save(None);
        }
        if entry(
            ui,
            None,
            "Save as…",
            Some(shortcut("Shift+Ctrl+S")),
            has,
            "",
            NO_FILE,
        ) {
            self.save_as();
        }
        ui.separator();
        self.import_menu(ui);
        self.export_menu(ui);
        ui.menu_button("Plugins", |ui| self.plugins_menu(ui));
        ui.separator();
        if item(
            ui,
            "Close",
            has && !dirty,
            "Close the structure (the start page again)",
            if has { save_first } else { "No structure open" },
        ) {
            self.close_file();
        }
        if entry(
            ui,
            None,
            "Quit",
            Some(shortcut("Ctrl+Q")),
            true,
            "Close the window (asks first with unsaved changes)",
            "",
        ) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// File › Import: the libraries (a file or a folder's files, copied
    /// into the folder the window reads), the load recipes, a load table.
    fn import_menu(&mut self, ui: &mut egui::Ui) {
        let has = self.file.is_some();
        ui.menu_button("Import", |ui| {
            self.import_library_menus(ui);
            ui.separator();
            if item(
                ui,
                "Load recipes…",
                has,
                "Load cases from a structure load-case file (.ohlslc) or CSV",
                NO_FILE,
            ) {
                self.import_recipes_dialog();
                self.focus(Panel::LoadRecipes);
            }
            if item(
                ui,
                "Load table…",
                has,
                "A load table (CSV): what it has beyond the evaluation is kept as typed values",
                NO_FILE,
            ) {
                self.import_table_dialog();
                self.focus(Panel::LoadTable);
            }
        });
    }

    /// File › Export: the file formats, the libraries, the loads, the
    /// report; what the exports take besides the file in Export options….
    fn export_menu(&mut self, ui: &mut egui::Ui) {
        let has = self.file.is_some();
        ui.menu_button("Export", |ui| {
            self.export_format_items(ui);
            ui.separator();
            self.export_library_items(ui);
            ui.separator();
            if item(
                ui,
                "Load recipes…",
                has,
                "The load cases as a structure load-case file (.ohlslc) or CSV",
                NO_FILE,
            ) {
                self.export_recipes_dialog();
            }
            if item(
                ui,
                "Load table…",
                has,
                "The load table as CSV (forces in N, vertical down)",
                NO_FILE,
            ) {
                self.export_table_dialog();
            }
            self.report_item(ui);
            ui.separator();
            if item(
                ui,
                "Export options…",
                true,
                "What the file formats take besides the structure: the cross-section library, the \
                 bolt library, the loads",
                "",
            ) {
                self.export_options = true;
            }
        });
    }

    /// Structure report… (File › Export and the Results menu).
    fn report_item(&mut self, ui: &mut egui::Ui) {
        if item(
            ui,
            "Structure report…",
            self.can_write_report(),
            "The results as a printable HTML page (with the pre-alpha warning) and the \
             members' results as CSV",
            "Run the cases first (results of the file and the load table as they are)",
        ) {
            self.report_dialog();
        }
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        let (u, r) = (self.undo_name(), self.redo_name());
        let label =
            |w: &str, n: &Option<String>| n.as_ref().map_or(w.to_owned(), |n| format!("{w} {n}"));
        if entry(
            ui,
            None,
            &label("Undo", &u),
            Some(shortcut("Ctrl+Z")),
            u.is_some(),
            "",
            "Nothing to undo",
        ) {
            self.undo();
        }
        if entry(
            ui,
            None,
            &label("Redo", &r),
            Some(shortcut("Shift+Ctrl+Z")),
            r.is_some(),
            "",
            "Nothing to redo",
        ) {
            self.redo();
        }
        ui.separator();
        if entry(
            ui,
            Some(Icon::Delete),
            "Delete selected…",
            Some("Del".into()),
            !self.selected.is_empty(),
            "Delete what is selected, with what goes with each (asks first; one undo step)",
            "Nothing is selected",
        ) {
            self.ask_delete();
        }
        if item(
            ui,
            "Clear selection",
            !self.selected.is_empty(),
            "Select nothing",
            "Nothing is selected",
        ) {
            self.selected.clear();
        }
        ui.separator();
        let has = self.file.is_some();
        ui.add_enabled_ui(has, |ui| {
            options_menu(ui, "Tools", |ui| self.tools_menu(ui));
        })
        .response
        .on_disabled_hover_text(NO_FILE);
    }

    /// Edit › Tools: the inventory, then every tool of the 3D view by its
    /// group (as OHL-Design's), its hotbar key shown, the one held marked.
    fn tools_menu(&mut self, ui: &mut egui::Ui) {
        if entry(
            ui,
            None,
            "Tool inventory",
            Some("E".into()),
            true,
            "Every tool of the 3D view: drag one onto a slot of the hotbar",
            "",
        ) {
            self.inventory = true;
        }
        let mut theme = "";
        for t in Tool::ALL {
            if t.theme() != theme {
                ui.separator();
                heading(ui, t.theme());
                theme = t.theme();
            }
            let held = self.tool() == t;
            let mut b = egui::Button::selectable(held, (t.icon().image(ui.ctx(), 16.0), t.name()))
                .image_tint_follows_text_color(true);
            if let Some(k) = self.hotbar.slots.iter().position(|s| *s == Some(t)) {
                b = b.shortcut_text((k + 1).to_string());
            }
            if ui.add(b).on_hover_text(t.help()).clicked() {
                self.hold_tool(t);
                ui.close();
            }
        }
    }

    /// View: Fit, the main panels ticked, Tables ▸ (every panel of the
    /// work by its menu), Display options ▸, UI scale ▸, Reset layout. A
    /// tick never closes it.
    fn view_menu(&mut self, ui: &mut egui::Ui) {
        if item(
            ui,
            "Fit the view",
            self.file.is_some(),
            "Show the whole structure in the 3D view",
            NO_FILE,
        ) {
            self.view_fit = true;
            if !self.is_open(Panel::View3d) {
                open_panel(&mut self.dock, Panel::View3d);
            }
        }
        ui.separator();
        for p in Panel::MAIN {
            self.panel_tick(ui, p);
        }
        ui.separator();
        options_menu(ui, "Tables", |ui| {
            for (k, area) in Area::ALL.into_iter().enumerate() {
                if k > 0 {
                    ui.separator();
                }
                heading(ui, area.name());
                for p in Panel::of(area) {
                    self.panel_tick(ui, p);
                }
            }
        });
        options_menu(ui, "Display options", |ui| self.display_options(ui));
        ui.separator();
        options_menu(ui, "UI scale", |ui| {
            for s in crate::style::SCALES {
                ui.radio_value(&mut self.ui_scale, s, format!("{:.0} %", s * 100.0))
                    .on_hover_text("The size of the whole window's text and controls (kept)");
            }
        });
        if item(
            ui,
            "Reset layout",
            true,
            "Every panel back where it starts: the text and edit panels (Inspector, To do, tables) \
             on the left, the 3D view on the right (the Detail view a tab on top of it)",
            "",
        ) {
            self.reset_layout();
        }
    }

    /// A panel's tick in View: shown or not (the menu stays open); one
    /// that does not apply to the structure's kind greyed out, why on
    /// hover.
    fn panel_tick(&mut self, ui: &mut egui::Ui, p: Panel) {
        let applies = self
            .file
            .as_ref()
            .map_or(Ok(()), |f| p.applies(f.structure()));
        let mut open = self.is_open(p);
        let r = ui
            .add_enabled(applies.is_ok(), egui::Checkbox::new(&mut open, p.title()))
            .on_hover_text(p.tip());
        let r = match applies {
            Err(why) => r.on_disabled_hover_text(why),
            Ok(()) => r,
        };
        if r.changed() {
            if open {
                self.focus(p);
            } else {
                close_panel(&mut self.dock, p);
            }
        }
    }

    /// View › Display options: what the 3D view draws (each kept until
    /// the program is started again).
    fn display_options(&mut self, ui: &mut egui::Ui) {
        heading(ui, "The structure");
        ui.checkbox(&mut self.rigid_links, "Rigid links")
            .on_hover_text(
                "The rigid links drawn: from a joint to an attachment point at an offset \
                 (magenta; none at zero offset)",
            );
        ui.checkbox(&mut self.joint_colors, "Joints by their details")
            .on_hover_text(
                "A tower's joints coloured by how complete their details are (Design › Joint \
                 details): green complete, orange partly assumed, red no connection data",
            );
        ui.checkbox(&mut self.string_volumes, "Volumes on the strings")
            .on_hover_text(
                "The shapes of the strings' insulator assemblies (fittings, horns, rings) on \
                 the strings as they hang in still air: grounded grey, electrified orange, \
                 insulated pale",
            );
        ui.separator();
        heading(ui, "Loads (a Loads panel in front)");
        ui.checkbox(&mut self.loads.arrows, "Load arrows")
            .on_hover_text(
                "The loads of the case shown as arrows (their scale in the Load case editor and \
                 the Load table)",
            );
        ui.separator();
        heading(ui, "Results (FE results in front)");
        ui.checkbox(&mut self.results.deformed, "Deformed")
            .on_hover_text("The deformed shape (its scale in FE results)");
        ui.checkbox(&mut self.results.reactions, "Reactions")
            .on_hover_text(
                "Drawn as the loads on the foundations (arrows into the ground), the largest 4 m",
            );
        ui.checkbox(&mut self.usage_legend, "Usage legend")
            .on_hover_text("The colours of the members' usage, over the view's corner");
    }

    /// The menu of `area`: its panels (those of its structure's kind),
    /// then its actions.
    fn area_menu(&mut self, ui: &mut egui::Ui, area: Area) {
        if area == Area::Geometry {
            self.kind_menu(ui);
            ui.separator();
        }
        let t = self.file.as_ref().map(|f| f.structure_arc());
        for p in Panel::of(area) {
            if t.as_ref().is_some_and(|t| p.applies(t).is_err()) {
                continue;
            }
            // An icon on an entry that builds (as the tools' actions), none
            // on a plain panel.
            let icon = (p == Panel::Generators).then_some(Icon::Generator);
            if entry(ui, icon, p.title(), None, true, p.tip(), "") {
                self.focus(p);
            }
        }
        ui.separator();
        match area {
            Area::Geometry => self.geometry_actions(ui),
            Area::Body => self.body_actions(ui),
            Area::Design => {
                let joints = t.as_ref().is_some_and(|t| !t.nodes.is_empty());
                if icon_item(
                    ui,
                    Icon::Support,
                    "Support the feet (pinned)",
                    joints,
                    "Every joint at the lowest level pinned (Supports)",
                    "No joint yet: build the body first",
                ) {
                    self.support_feet();
                    self.focus(Panel::Supports);
                }
            }
            Area::LoadCases => self.load_actions(ui),
            Area::Results => self.results_actions(ui),
        }
    }

    /// Geometry › Structure kind (owner 2026-11-02, stage 0 of the joint
    /// details): a tower (lattice, portal) has joint details; a pole its
    /// Pole panels. Nothing is deleted: what the new kind does not show
    /// stays in the file, and the message says so.
    fn kind_menu(&mut self, ui: &mut egui::Ui) {
        use ohl_structures::structures::StructureKind as K;
        let Some(kind) = self.file.as_ref().map(|f| f.structure().kind) else {
            return;
        };
        let mut chosen = kind;
        options_menu(ui, "Structure kind", |ui| {
            for (k, word, tip) in [
                (
                    K::Lattice,
                    "lattice tower",
                    "Joint details (Design › Joint details)",
                ),
                (K::Portal, "portal", "Joint details, as a lattice tower"),
                (K::Pole, "pole", "The Pole panels; no joint details"),
                (K::Other, "other", "Neither: every panel listed"),
            ] {
                ui.radio_value(&mut chosen, k, word).on_hover_text(tip);
            }
        });
        if chosen == kind {
            return;
        }
        let Some(f) = self.file.as_mut() else {
            return;
        };
        let note = ohl_structure_edit::kind_change_note(f.structure(), chosen);
        let word = match chosen {
            K::Lattice => "lattice",
            K::Portal => "portal",
            K::Pole => "pole",
            K::Other => "other",
        };
        let r = f.edit("Structure kind", |doc, _| {
            ohl_structure_edit::set_structure_kind(doc, word)
        });
        self.message = Some(match r {
            Ok(()) => (
                match note {
                    Some(n) => format!("structure kind: {word}; {n}"),
                    None => format!("structure kind: {word}"),
                },
                false,
            ),
            Err(e) => (e, true),
        });
    }

    /// Geometry's actions: those of the Geometry tools, on what is
    /// selected (the Link tool is in Edit › Tools and a point's
    /// right-click menu).
    fn geometry_actions(&mut self, ui: &mut egui::Ui) {
        let tops = self.picked_tops().len();
        if icon_item(
            ui,
            Icon::AddPoint,
            "Add a point",
            true,
            "A point where strings hang, at the top of the stick figure (the Add a point tool: \
             on a joint clicked)",
            "",
        ) {
            self.add_point();
            self.focus(Panel::PointsInsulators);
        }
        if icon_item(
            ui,
            Icon::OneString,
            "One string below the point",
            tops == 1,
            "A string from the point selected down to a new wire point",
            "Select one point (or a wire point: a string below it)",
        ) {
            self.add_insulators();
            self.focus(Panel::PointsInsulators);
        }
        if icon_item(
            ui,
            Icon::TwoStrings,
            "Two strings to one wire point",
            tops == 2,
            "Two strings from the two points selected to one wire point below their middle",
            "Select two points (Shift + click the second)",
        ) {
            self.add_insulators();
            self.focus(Panel::PointsInsulators);
        }
    }

    /// Body's actions: those of the Body tools, on what is selected; the
    /// symmetry.
    fn body_actions(&mut self, ui: &mut egui::Ui) {
        let joints = self.picked_joints().len();
        if icon_item(
            ui,
            Icon::AddJoint,
            "Add a joint",
            true,
            "A joint above the joint selected (or at the centre)",
            "",
        ) {
            self.add_joint();
            self.focus(Panel::Joints);
        }
        if icon_item(
            ui,
            Icon::AddMember,
            "Add a member",
            joints == 2,
            "A member between the two joints selected",
            "Select two joints (Shift + click the second)",
        ) {
            self.add_member();
            self.focus(Panel::Members);
        }
        if icon_item(
            ui,
            Icon::PointOnJoint,
            "Point on the joint",
            self.picked_attachable_joint(),
            "A point where strings hang, on the joint selected (it moves with it)",
            "Select one joint that is not a support joint (strings hang from attach joints)",
        ) {
            self.point_here();
            self.focus(Panel::PointsInsulators);
        }
        ui.separator();
        let now = self
            .file
            .as_ref()
            .and_then(|f| symmetry_option(f.structure()));
        options_menu(ui, "Symmetry", |ui| {
            let mut chosen = now;
            ui.radio_value(&mut chosen, None, "none")
                .on_hover_text("Joints and members added, moved or deleted alone");
            for m in Mirror::ALL {
                ui.radio_value(&mut chosen, Some(m), m.name())
                    .on_hover_text(
                        "Joints and members added, moved or deleted take their mirror images along",
                    );
            }
            if chosen != now {
                self.set_symmetry(chosen);
            }
            ui.separator();
            if item(
                ui,
                "Make symmetric",
                now.is_some(),
                "Every joint and member gets the mirror images it lacks",
                "Choose a symmetry first",
            ) {
                self.make_symmetric();
            }
        });
    }

    fn load_actions(&mut self, ui: &mut egui::Ui) {
        if item(
            ui,
            "Add a load case",
            true,
            "A new load case without loads",
            "",
        ) {
            self.add_case();
            self.focus(Panel::LoadRecipes);
        }
        let can_take = !self.loads.line_cases().is_empty();
        if item(
            ui,
            "Take the line's load cases",
            can_take,
            "One load case per line result: its line forces at every point, every factor 1, \
             and the wind on the body at its weather case's pressure (one undo step)",
            "The line gives no line result (see the Load case editor)",
        ) {
            self.take_line_cases();
            self.focus(Panel::LoadRecipes);
        }
        if item(
            ui,
            "Store the line's cases…",
            can_take,
            "The line's cases (ticked) written into the structure file as cases of given \
             values: the file then has its forces without the line",
            crate::load_cases_tab::NO_LINE_VALUES_WHY,
        ) {
            self.open_store();
            self.focus(Panel::LoadTable);
        }
        self.code_item(ui);
        if item(
            ui,
            "Read the line's values again",
            self.line_values.is_some(),
            "Read the line-values file again now (it is also read whenever the line \
             application writes it)",
            "Opened on its own: no line-values file",
        ) {
            self.reload_line_values();
            self.focus(Panel::InTheLine);
        }
    }

    fn results_actions(&mut self, ui: &mut egui::Ui) {
        let can = self.can_run_results();
        let which = if self.results.case.is_empty() {
            "every case".to_owned()
        } else {
            format!("case {}", self.results.case)
        };
        if item(
            ui,
            "Run the cases",
            can,
            &format!("Solve {which} of the load table with the finite-element model"),
            "Needs level 3 and a case in the load table; or a run is going on",
        ) {
            let ctx = ui.ctx().clone();
            self.run_results(&ctx);
            self.focus(Panel::Results);
        }
        if item(
            ui,
            "Cancel the run",
            self.results.running(),
            "Stop the run going on",
            "No run is going on",
        ) {
            self.cancel_results();
        }
        ui.separator();
        self.report_item(ui);
    }

    /// Help: the manual, the To do list, the controls, About.
    fn help_menu(&mut self, ui: &mut egui::Ui) {
        crate::help::manual_entries(ui, &mut self.message);
        ui.separator();
        if item(
            ui,
            "To do",
            self.file.is_some(),
            "What the next level lacks, then advice (the To do panel)",
            NO_FILE,
        ) {
            self.focus(Panel::Level);
        }
        ui.menu_button("Controls", |ui| {
            ui.set_max_width(420.0);
            heading(ui, "3D view");
            ui.label(crate::view::CONTROLS);
            ui.separator();
            heading(ui, "Detail view");
            ui.label(crate::detail_view::CONTROLS);
            ui.separator();
            heading(ui, "Keys");
            ui.label(format!(
                "{}: new structure, {}: open, {}: save, {}: save as, {}: undo, {}: redo, \
                 Delete: delete what is selected (asked first), E: the tool inventory, 1-9: a \
                 hotbar slot, Escape: let go of the tool",
                shortcut("Ctrl+N"),
                shortcut("Ctrl+O"),
                shortcut("Ctrl+S"),
                shortcut("Shift+Ctrl+S"),
                shortcut("Ctrl+Z"),
                shortcut("Shift+Ctrl+Z"),
            ));
        });
        ui.separator();
        if item(
            ui,
            "About OHL-Structure",
            true,
            "Its version, what it is, its licence",
            "",
        ) {
            self.about = true;
        }
    }

    /// The windows: the deletion asked first, About, Export options.
    pub(crate) fn windows(&mut self, ctx: &egui::Context) {
        self.confirm_window(ctx);
        self.detail_question(ctx);
        self.store_window(ctx);
        self.libraries_window(ctx);
        self.new_structure_window(ctx);
        self.plugin_windows(ctx);
        self.export_options_window(ctx);
        if self.about {
            let mut open = true;
            egui::Window::new("About OHL-Structure")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(crate::icons::mark(ui.ctx(), 48.0));
                        ui.vertical(|ui| {
                            ui.heading("OHL-Structure");
                            ui.label(format!("Version {}", crate::VERSION));
                        });
                    });
                    ui.label(
                        "The structure application of OpenOHL: one structure file (.ohls), from \
                         level 1 (stick figure) to level 3 (full design), its loads and its \
                         finite-element results.",
                    );
                    ui.label(
                        egui::RichText::new(
                            "View shows and hides the panels; drag a panel's tab to move it, or \
                             right-click it.",
                        )
                        .weak(),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Free software under the GNU Affero General Public License v3.0 or later. Help › User \
                             guide opens the manual on this computer.",
                        )
                        .small()
                        .weak(),
                    );
                });
            self.about = open;
        }
    }

    /// A deletion: what goes with it, Delete or Cancel.
    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(r) = self.confirm.clone() else {
            return;
        };
        let mut open = true;
        let mut done = false;
        egui::Window::new("Delete")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Delete, with everything that goes with it:");
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .id_salt("removal")
                    .show(ui, |ui| {
                        for l in r.lines() {
                            ui.label(l);
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        if let Some(f) = &mut self.file {
                            f.edit("delete", |doc, _| ohl_structure_edit::remove(doc, &r));
                            self.selected.clear();
                            self.message =
                                Some((format!("deleted {}", r.lines().join("; ")), false));
                        } else {
                            error_label(ui, "no structure open");
                        }
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                });
            });
        if !open || done {
            self.confirm = None;
        }
    }
}
