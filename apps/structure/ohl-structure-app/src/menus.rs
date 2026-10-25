// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The menu bar: File, Edit, View (each panel ticked to show or hide it,
//! Reset layout), then the menus of the work — Geometry, Body, Design,
//! Load cases, Results, In the line — each listing its panels (opened or
//! brought to the front) and its actions, and Help; Undo, Redo and Save
//! at the right of the menus. The windows they open: the deletion asked
//! first, About.

use eframe::egui;
use ohl_structure_edit::Mirror;
use ohl_structure_edit::body::symmetry_option;

use crate::app::{StructureApp, error_label};
use crate::icons::Icon;
use crate::layout::{Area, Panel, close_panel, open_panel, options_menu};

/// A shortcut as the platform writes it: ⌘ on a Mac, Ctrl elsewhere (as
/// OHL-Design).
pub(crate) fn shortcut(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("Ctrl+", "⌘")
    } else {
        keys.to_owned()
    }
}

/// A menu entry: true when clicked (the menu closes).
fn item(ui: &mut egui::Ui, label: &str, enabled: bool, tip: &str, why_not: &str) -> bool {
    item_with(ui, None, label, enabled, tip, why_not)
}

/// A menu entry with its icon (of `assets/icons/tools/`).
fn icon_item(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    tip: &str,
    why_not: &str,
) -> bool {
    item_with(ui, Some(icon), label, enabled, tip, why_not)
}

fn item_with(
    ui: &mut egui::Ui,
    icon: Option<Icon>,
    label: &str,
    enabled: bool,
    tip: &str,
    why_not: &str,
) -> bool {
    let button = match icon {
        Some(i) => i.button(ui, label),
        None => egui::Button::new(label),
    };
    let r = ui
        .add_enabled(enabled, button)
        .on_hover_text(tip)
        .on_disabled_hover_text(why_not);
    if r.clicked() {
        ui.close();
        true
    } else {
        false
    }
}

impl StructureApp {
    /// The menu bar.
    pub(crate) fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| self.file_menu(ui));
            ui.menu_button("Edit", |ui| self.edit_menu(ui));
            options_menu(ui, "View", |ui| self.view_menu(ui));
            let has = self.file.is_some();
            ui.add_enabled_ui(has, |ui| {
                for area in Area::ALL {
                    ui.menu_button(area.name(), |ui| self.area_menu(ui, area));
                }
            });
            ui.menu_button("Plugins", |ui| self.plugins_menu(ui));
            ui.menu_button("Help", |ui| {
                crate::help::manual_entries(ui, &mut self.message);
                if ui
                    .button("About OHL-Structure")
                    .on_hover_text("Its version, what it is, its licence")
                    .clicked()
                {
                    self.about = true;
                    ui.close();
                }
            });
            ui.separator();
            self.quick_buttons(ui);
        });
    }

    /// Undo, Redo and Save, right of the menus.
    fn quick_buttons(&mut self, ui: &mut egui::Ui) {
        let undo_name = self.undo_name();
        let undo_tip = undo_name
            .as_ref()
            .map_or("Nothing to undo".to_owned(), |w| {
                format!("Undo {w} ({})", shortcut("Ctrl+Z"))
            });
        let can_undo = undo_name.is_some();
        if ui
            .add_enabled(can_undo, egui::Button::new("Undo"))
            .on_hover_text(&undo_tip)
            .on_disabled_hover_text(&undo_tip)
            .clicked()
        {
            self.undo();
        }
        let can_redo = self.redo_name().is_some();
        if ui
            .add_enabled(can_redo, egui::Button::new("Redo"))
            .on_hover_text(format!(
                "Make again what was undone ({})",
                shortcut("Shift+Ctrl+Z")
            ))
            .clicked()
        {
            self.redo();
        }
        if ui
            .add_enabled(self.dirty(), egui::Button::new("Save"))
            .on_hover_text(format!("Save the file ({})", shortcut("Ctrl+S")))
            .clicked()
        {
            self.save(None);
        }
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        let dirty = self.dirty();
        if ui
            .add_enabled(!dirty, egui::Button::new("New structure…"))
            .on_hover_text("An empty level 1 structure, or one built from a structure template")
            .on_disabled_hover_text("Save or undo the changes first")
            .clicked()
        {
            ui.close();
            self.new_structure = Some(crate::start::NewStructure::default());
        }
        if ui
            .add_enabled(!dirty, egui::Button::new("Open…"))
            .on_disabled_hover_text("Save or undo the changes first")
            .clicked()
        {
            ui.close();
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Structure", &ohl_core::files::open_extensions("ohls"))
                .pick_file()
            {
                self.open_chosen(&p);
            }
        }
        let has = self.file.is_some();
        if ui
            .add_enabled(
                dirty,
                egui::Button::new("Save").shortcut_text(shortcut("Ctrl+S")),
            )
            .clicked()
        {
            ui.close();
            self.save(None);
        }
        if ui
            .add_enabled(
                has,
                egui::Button::new("Save as…").shortcut_text(shortcut("Shift+Ctrl+S")),
            )
            .clicked()
        {
            ui.close();
            self.save_as();
        }
        ui.separator();
        self.libraries_menu(ui);
        self.export_menu(ui);
        ui.separator();
        if ui
            .button("Close")
            .on_hover_text("Close the window (asks first with unsaved changes)")
            .clicked()
        {
            ui.close();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        let (u, r) = (self.undo_name(), self.redo_name());
        let label =
            |w: &str, n: &Option<String>| n.as_ref().map_or(w.to_owned(), |n| format!("{w} {n}"));
        if ui
            .add_enabled(
                u.is_some(),
                egui::Button::new(label("Undo", &u)).shortcut_text(shortcut("Ctrl+Z")),
            )
            .clicked()
        {
            ui.close();
            self.undo();
        }
        if ui
            .add_enabled(
                r.is_some(),
                egui::Button::new(label("Redo", &r)).shortcut_text(shortcut("Shift+Ctrl+Z")),
            )
            .clicked()
        {
            ui.close();
            self.redo();
        }
        ui.separator();
        if item(
            ui,
            "Clear selection",
            !self.selected.is_empty(),
            "Select nothing (as Edit › Clear selection in OHL-Design)",
            "Nothing is selected",
        ) {
            self.selected.clear();
        }
    }

    /// View: each panel ticked to show or hide it (the menu stays open),
    /// Reset layout, Fit.
    fn view_menu(&mut self, ui: &mut egui::Ui) {
        if ui
            .button("Fit the view")
            .on_hover_text("Show the whole structure in the 3D view")
            .clicked()
        {
            self.view_fit = true;
            if !self.is_open(Panel::View3d) {
                open_panel(&mut self.dock, Panel::View3d);
            }
            ui.close();
        }
        ui.checkbox(&mut self.rigid_links, "Rigid links")
            .on_hover_text(
                "The rigid links drawn: from a joint to an attachment point at an offset \
                 (magenta; none at zero offset)",
            );
        ui.separator();
        ui.label(egui::RichText::new("Panels").small().weak());
        let tick = |ui: &mut egui::Ui, app: &mut StructureApp, p: Panel| {
            let mut open = app.is_open(p);
            if ui
                .checkbox(&mut open, p.title())
                .on_hover_text(p.tip())
                .changed()
            {
                if open {
                    app.focus(p);
                } else {
                    close_panel(&mut app.dock, p);
                }
            }
        };
        tick(ui, self, Panel::Level);
        tick(ui, self, Panel::View3d);
        tick(ui, self, Panel::Libraries);
        for area in Area::ALL {
            ui.label(egui::RichText::new(area.name()).small().weak());
            for p in Panel::of(area) {
                tick(ui, self, p);
            }
        }
        ui.separator();
        if ui
            .button("Reset layout")
            .on_hover_text(
                "Every panel back where it starts: the tables on the left, the 3D view on the \
                 right",
            )
            .clicked()
        {
            self.reset_layout();
            ui.close();
        }
    }

    /// The menu of `area`: its panels, then its actions.
    fn area_menu(&mut self, ui: &mut egui::Ui, area: Area) {
        for p in Panel::of(area) {
            let title = format!("{}…", p.title());
            let button = match p {
                Panel::Generators => Icon::Generator.button(ui, title),
                Panel::Supports => Icon::Support.button(ui, title),
                _ => egui::Button::new(title),
            };
            if ui.add(button).on_hover_text(p.tip()).clicked() {
                self.focus(p);
                ui.close();
            }
        }
        ui.separator();
        match area {
            Area::Geometry => self.geometry_actions(ui),
            Area::Body => self.body_actions(ui),
            Area::Design => {
                let joints = self
                    .file
                    .as_ref()
                    .is_some_and(|f| !f.structure().nodes.is_empty());
                if item(
                    ui,
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
            Area::InTheLine => {
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
        }
    }

    fn geometry_actions(&mut self, ui: &mut egui::Ui) {
        let tops = self.picked_tops().len();
        if item(
            ui,
            "Add a point",
            true,
            "A point where strings hang, at the top of the stick figure",
            "",
        ) {
            self.add_point();
            self.focus(Panel::PointsInsulators);
        }
        if item(
            ui,
            "One string below the point",
            tops == 1,
            "A string from the point selected down to a new wire point",
            "Select one point (or a wire point: a string below it)",
        ) {
            self.add_insulators();
            self.focus(Panel::PointsInsulators);
        }
        if item(
            ui,
            "Two strings to one wire point",
            tops == 2,
            "Two strings from the two points selected to one wire point below their middle",
            "Select two points (Shift + click the second)",
        ) {
            self.add_insulators();
            self.focus(Panel::PointsInsulators);
        }
        if item(
            ui,
            if self.link.is_some() {
                "Let go of the Link tool"
            } else {
                "Link tool"
            },
            true,
            "Click an attachment point, then the joint to link it to (where it is: the distance \
             kept as its offset); right-click or Escape cancels",
            "",
        ) {
            self.toggle_link_tool();
        }
        self.delete_item(ui);
    }

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
            "Member between the two joints",
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
        let now = self
            .file
            .as_ref()
            .and_then(|f| symmetry_option(f.structure()));
        let symmetry = (
            Icon::Symmetry
                .image(ui.ctx(), 16.0)
                .tint(ui.visuals().text_color()),
            "Symmetry",
        );
        options_menu(ui, symmetry, |ui| {
            let mut chosen = now;
            ui.radio_value(&mut chosen, None, "none");
            for m in Mirror::ALL {
                ui.radio_value(&mut chosen, Some(m), m.name());
            }
            if chosen != now {
                self.set_symmetry(chosen);
            }
        });
        if item(
            ui,
            "Make symmetric",
            now.is_some(),
            "Every joint and member gets the mirror images it lacks",
            "Choose a symmetry first",
        ) {
            self.make_symmetric();
        }
        self.delete_item(ui);
    }

    fn delete_item(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        if item(
            ui,
            "Delete what is selected…",
            !self.selected.is_empty(),
            "Delete it all, with what goes with each (asks first; one undo step)",
            "Select something first",
        ) {
            self.ask_delete();
        }
    }

    fn load_actions(&mut self, ui: &mut egui::Ui) {
        let can_take = !self.loads.line_cases().is_empty();
        if item(
            ui,
            "Store the line's cases in the file…",
            can_take,
            "The line's cases (ticked) written into the structure file as cases of given \
             values: the file then has its forces without the line",
            "No line values: open this from a placed structure (Properties › Open in OHL-Structure… in \
             line application)",
        ) {
            self.open_store();
            self.focus(Panel::LoadTable);
        }
        if item(
            ui,
            "Take the line's load cases as they are",
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
            "Add a load case",
            true,
            "A new load case without loads",
            "",
        ) {
            self.add_case();
            self.focus(Panel::LoadRecipes);
        }
        self.code_item(ui);
        ui.separator();
        if item(
            ui,
            "Import load recipes…",
            true,
            "Load cases from a structure load-case file (.ohlslc) or CSV",
            "",
        ) {
            self.import_recipes_dialog();
            self.focus(Panel::LoadRecipes);
        }
        if item(
            ui,
            "Export load recipes…",
            true,
            "The load cases as a structure load-case file (.ohlslc) or CSV",
            "",
        ) {
            self.export_recipes_dialog();
        }
        if item(
            ui,
            "Import a load table…",
            true,
            "A load table (CSV): what it has beyond the evaluation is kept as typed values",
            "",
        ) {
            self.import_table_dialog();
            self.focus(Panel::LoadTable);
        }
        if item(
            ui,
            "Export the load table…",
            true,
            "The load table as CSV (forces in N, vertical down)",
            "",
        ) {
            self.export_table_dialog();
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

    /// The windows: the deletion asked first, About.
    pub(crate) fn windows(&mut self, ctx: &egui::Context) {
        self.confirm_window(ctx);
        self.store_window(ctx);
        self.libraries_window(ctx);
        self.new_structure_window(ctx);
        self.plugin_windows(ctx);
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
                            "View shows and hides the panels; drag a panel's tab to move it.",
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
