// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The menu bar: File, Edit, View (each panel ticked to show or hide it,
//! Reset layout), then the menus of the work — Geometry, Body, Design,
//! Load cases, Results, In the line — each listing its panels (opened or
//! brought to the front) and its actions, and Help; Undo, Redo and Save
//! at the right of the menus. The windows they open: the deletion asked
//! first, About.

use eframe::egui;
use ols_structure_edit::Mirror;
use ols_structure_edit::body::symmetry_option;

use crate::app::{StructureApp, error_label};
use crate::layout::{Area, Panel, close_panel, open_panel, options_menu};

/// A menu entry: true when clicked (the menu closes).
fn item(ui: &mut egui::Ui, label: &str, enabled: bool, tip: &str, why_not: &str) -> bool {
    let r = ui
        .add_enabled(enabled, egui::Button::new(label))
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
                if ui.button("About OHL-Structure").clicked() {
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
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        let dirty = self.dirty();
        if ui
            .add_enabled(!dirty, egui::Button::new("New structure…"))
            .on_hover_text("An empty level 1 structure, or one built from a pylon template")
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
                .add_filter("Structure", &["olss"])
                .pick_file()
            {
                self.open_chosen(&p);
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
        let (u, r) = self.file.as_ref().map_or((None, None), |f| {
            (
                f.undo_name().map(str::to_owned),
                f.redo_name().map(str::to_owned),
            )
        });
        let label =
            |w: &str, n: &Option<String>| n.as_ref().map_or(w.to_owned(), |n| format!("{w} {n}"));
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
        ui.separator();
        if item(
            ui,
            "Pick nothing",
            !self.selected.is_empty(),
            "Drop what is picked",
            "Nothing is picked",
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
            if ui
                .button(format!("{}…", p.title()))
                .on_hover_text(p.tip())
                .clicked()
            {
                self.focus(p);
                ui.close();
            }
        }
        ui.separator();
        match area {
            Area::Geometry => self.geometry_actions(ui),
            Area::Body => self.body_actions(ui),
            Area::Design => {}
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
            "A point where insulators hang, at the top of the stick figure",
            "",
        ) {
            self.add_point();
            self.focus(Panel::PointsInsulators);
        }
        if item(
            ui,
            "I insulator below the point",
            tops == 1,
            "An insulator from the point picked down to a new wire point",
            "Pick one point (or a wire point: an insulator below it)",
        ) {
            self.add_insulators();
            self.focus(Panel::PointsInsulators);
        }
        if item(
            ui,
            "V insulators to one wire point",
            tops == 2,
            "Two insulators from the two points picked to one wire point below their middle",
            "Pick two points (Shift + click the second)",
        ) {
            self.add_insulators();
            self.focus(Panel::PointsInsulators);
        }
        self.delete_item(ui);
    }

    fn body_actions(&mut self, ui: &mut egui::Ui) {
        let joints = self.picked_joints().len();
        if item(
            ui,
            "Add a joint",
            true,
            "A joint above the joint picked (or at the centre)",
            "",
        ) {
            self.add_joint();
            self.focus(Panel::Joints);
        }
        if item(
            ui,
            "Member between the two joints",
            joints == 2,
            "A member between the two joints picked",
            "Pick two joints (Shift + click the second)",
        ) {
            self.add_member();
            self.focus(Panel::Members);
        }
        if item(
            ui,
            "Point on the joint",
            joints == 1 && self.selected.len() == 1,
            "A point where insulators hang, on the joint picked (it moves with it)",
            "Pick one joint",
        ) {
            self.point_here();
            self.focus(Panel::PointsInsulators);
        }
        let now = self
            .file
            .as_ref()
            .and_then(|f| symmetry_option(f.structure()));
        options_menu(ui, "Symmetry", |ui| {
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
            "Delete what is picked…",
            self.selected.len() == 1,
            "Delete it, with what goes with it (asks first)",
            "Pick one thing",
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
            "No line values: open this from a placed structure (Properties › Edit… in the \
             line application)",
        ) {
            self.open_store();
            self.focus(Panel::LoadTable);
        }
        if item(
            ui,
            "Take the line's load cases as they are",
            can_take,
            "One load case per load case of the line: its factored transverse, longitudinal \
             and vertical loads at every point (one undo step)",
            "The line gives no load case (see the Load recipes panel)",
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
            "Load cases from a structure load-case file (.olsslc) or CSV",
            "",
        ) {
            self.import_recipes_dialog();
            self.focus(Panel::LoadRecipes);
        }
        if item(
            ui,
            "Export load recipes…",
            true,
            "The load cases as a structure load-case file (.olsslc) or CSV",
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
                    ui.heading("OHL-Structure");
                    ui.label(format!("Version {}", crate::VERSION));
                    ui.label(
                        "The structure application of OpenOHL: one structure file (.olss), from \
                         level 1 (stick figure) to level 3 (full design), its loads and its \
                         finite-element results.",
                    );
                    ui.label(
                        egui::RichText::new(
                            "View shows and hides the panels; drag a panel's tab to move it.",
                        )
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
                            f.edit("delete", |doc, _| ols_structure_edit::remove(doc, &r));
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
