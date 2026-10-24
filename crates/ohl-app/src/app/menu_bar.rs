// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The menus: File, the menu bar, the commands run, the snap and model
//! panels, the plugins' checks.

use super::*;

/// The open file's entries of the File menu: Save, every import and
/// export, the bill of materials.
pub(super) fn file_entries(ui: &mut egui::Ui, r: &mut Ready, out: &mut Vec<crate::menu::Command>) {
    use crate::menu::Command;
    if Command::Save.menu_item(ui) {
        out.push(Command::Save);
        ui.close();
    }
    let busy = r.survey_job.is_some();
    ui.menu_button("Import", |ui| {
        if ui
            .button("Into the line…")
            .on_hover_text(
                "Read a file into the line you work on (built-in formats and those of the \
                 plugins); one command, undoable",
            )
            .clicked()
        {
            r.plugin_request = Some(PluginRequest::Import);
            ui.close();
        }
        if ui
            .add_enabled(!busy, egui::Button::new("LiDAR points (LAS/LAZ)…"))
            .on_hover_text("Add LiDAR tiles to the project's point cloud (or a new one)")
            .clicked()
        {
            r.survey_import = Some(crate::survey_io::ImportDialog::new(&r.model));
            ui.close();
        }
        if ui
            .button("Terrain surface (TIN)…")
            .on_hover_text(
                "Terrain › Terrain surfaces, at its import: a surface file (built-in formats \
                 and those of the plugins) kept in the project",
            )
            .clicked()
        {
            r.surfaces.show(&r.model);
            ui.close();
        }
    });
    ui.menu_button("Export", |ui| {
        if ui
            .button("The line…")
            .on_hover_text(
                "Write the line and its solved wires in a format (built-in or from a plugin)",
            )
            .clicked()
        {
            r.plugin_request = Some(PluginRequest::Export);
            ui.close();
        }
        if ui
            .button("Drawings…")
            .on_hover_text(
                "Plan and profile sheets and profile exports of the routes: SVG, DXF, PDF, PNG, \
                 CSV, XML, at chosen scales and station range",
            )
            .clicked()
        {
            let (rules, markers) = drawing_checks(r);
            r.drawings.set_checks(rules, markers);
            r.drawings.open();
            ui.close();
        }
        if ui
            .add_enabled(
                !busy && r.model.cloud.is_some(),
                egui::Button::new("LiDAR points (LAS/LAZ)…"),
            )
            .on_hover_text("Write the project's point cloud (or part of it) to a LAS/LAZ file")
            .clicked()
        {
            r.survey_export = Some(Default::default());
            ui.close();
        }
        if ui
            .button("Terrain surfaces (TIN)…")
            .on_hover_text("Terrain › Terrain surfaces: export a surface kept in the project")
            .clicked()
        {
            r.surfaces.show(&r.model);
            ui.close();
        }
        if ui
            .add_enabled(
                r.model.library_dir.is_some(),
                egui::Button::new("Libraries…"),
            )
            .on_hover_text(
                "Write the project's libraries (wires, structure types, insulators, weather \
                 cases) as files into a folder: to share them or compare them",
            )
            .clicked()
        {
            ui.close();
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                r.status = Some(match r.model.export_libraries(&dir) {
                    Ok(n) => (
                        format!("{n} library files written to {}", dir.display()),
                        false,
                    ),
                    Err(e) => (format!("export: {e}"), true),
                });
            }
        }
    });
    ui.menu_button("Bill of materials", |ui| {
        if ui
            .button("Wires…")
            .on_hover_text(
                "The wire lengths of each wire type (and their stringing), priced with a cost file",
            )
            .clicked()
        {
            r.bom.open(&r.model, ui.ctx());
            ui.close();
        }
        ui.label(
            egui::RichText::new("Structures, insulator assemblies, hardware: not done yet")
                .small()
                .weak(),
        );
    });
}

/// The menus after File, in the order of the work: Edit (with Snap),
/// View (pages, cases shown, layers), Terrain, Structures, Sections,
/// Criteria, Checks, Tools, Versions, Plugins, Help.
/// Commands go to `out`.
pub(super) fn menus(
    ui: &mut egui::Ui,
    r: &mut Ready,
    dock: &mut DockState<Tab>,
    out: &mut Vec<crate::menu::Command>,
    customize: &mut bool,
    about: &mut bool,
) {
    use crate::menu::Command;
    let item = |ui: &mut egui::Ui, c: Command, out: &mut Vec<Command>| {
        if c.menu_item(ui) {
            out.push(c);
            ui.close();
        }
    };
    ui.menu_button("Edit", |ui| {
        let editable = r.model.can_edit().is_ok();
        ui.add_enabled_ui(editable, |ui| {
            item(ui, Command::Undo, out);
            item(ui, Command::Redo, out);
            item(ui, Command::DeleteSelected, out);
        });
        item(ui, Command::ClearSelection, out);
        item(ui, Command::GoTo, out);
        ui.separator();
        crate::menu::options_menu(ui, "Snap: what can be clicked", |ui| {
            snap_panel(ui, &mut r.snaps)
        });
    });
    crate::menu::options_menu(ui, "View", |ui| {
        item(ui, Command::FitViews, out);
        ui.separator();
        ui.label(egui::RichText::new("Pages").small().weak());
        let mut page = |ui: &mut egui::Ui, tab: Tab| {
            let mut open = dock.find_tab(&tab).is_some();
            if ui.checkbox(&mut open, tab.title()).changed() {
                if open {
                    open_page(dock, tab);
                } else {
                    close_page(dock, tab);
                }
            }
        };
        for tab in Tab::VIEWS.into_iter().filter(|t| !t.advanced()) {
            page(ui, tab);
        }
        crate::menu::options_menu(ui, "Advanced", |ui| {
            for tab in Tab::VIEWS.into_iter().filter(|t| t.advanced()) {
                page(ui, tab);
            }
        });
        ui.separator();
        // Ticks keep it open.
        crate::menu::options_menu(ui, "Work on route", |ui| {
            ui.label(
                egui::RichText::new(
                    "The profile, the checks and the structure loads keep to it; the rest is \
                     drawn faded (yours only)",
                )
                .small()
                .weak(),
            );
            let mut chosen = r.model.work_route.clone();
            ui.radio_value(&mut chosen, None, "All routes");
            for route in &r.model.line.routes {
                ui.radio_value(&mut chosen, Some(route.id.clone()), &route.id);
            }
            if chosen != r.model.work_route {
                r.model.work_route = chosen.clone();
                if let Some(id) = chosen {
                    r.profile.show = Some(crate::profile::Show::Route(id));
                }
                r.clearances.stale = true;
                r.refresh_views();
            }
        });
        item(ui, Command::DisplayCases, out);
        if ui
            .button("Cross-section… (C)")
            .on_hover_text(
                "A cut across the line at a station or a structure: a 3D panel of its own, holding only the slab",
            )
            .clicked()
        {
            r.open_cut_dialog();
            ui.close();
        }
        crate::menu::options_menu(ui, "Profile aspect ratio", |ui| {
            // Remembered with the other display choices when the app saves.
            crate::profile::ratio_menu(ui, &mut r.model.profile_ratio);
        });
        crate::menu::options_menu(ui, "Sheet preview", |ui| r.drawings.preview_menu(ui));
        // Ticks keep it open; this user's, remembered with the project.
        crate::menu::options_menu(ui, "Lines on the ground beside the line", |ui| {
            let routes = crate::ground_lines::routes(&r.model.line);
            // The weather cases and project lines the lines take: those
            // displayed (owner, 2026-10-22).
            let displayed = crate::ground_lines::Displayed::of(&r.model, r.case)
                .cases()
                .iter()
                .filter_map(|&k| r.model.cases.get(k).map(|c| c.case.name.clone()))
                .collect::<Vec<_>>()
                .join(", ");
            let projects: Vec<String> = if r.visible.contains(&Layer::OtherLines) {
                r.model.other_lines.iter().map(|o| o.name.clone()).collect()
            } else {
                Vec::new()
            };
            let mut build = false;
            crate::ground_lines::menu(
                ui,
                &mut r.model.ground_lines,
                &routes,
                &displayed,
                &projects,
                r.model.sources.corridor,
                r.ground_lines.drawn.as_deref(),
                &mut build,
            );
            // Terrain › Terrain surfaces, at Build from LiDAR.
            if build {
                r.surfaces.show(&r.model);
            }
        });
        crate::menu::options_menu(ui, "Layers", |ui| {
            for layer in Layer::ALL {
                let mut on = r.visible.contains(&layer);
                if ui.checkbox(&mut on, layer.name()).changed() {
                    if on {
                        r.visible.insert(layer);
                    } else {
                        r.visible.remove(&layer);
                    }
                }
            }
        });
        ui.separator();
        if ui.button("Customize the quick bar…").clicked() {
            *customize = true;
            ui.close();
        }
    });
    crate::menu::options_menu(ui, "Terrain", |ui| terrain_menu(ui, r, out));
    ui.menu_button("Structures", |ui| {
        use crate::library::LibraryKind;
        use crate::tables::TableKind;
        ui.label(egui::RichText::new("Libraries").small().weak());
        item(ui, Command::Library(LibraryKind::StructureTypes), out);
        item(ui, Command::Library(LibraryKind::Insulators), out);
        ui.separator();
        ui.label(egui::RichText::new("Tables").small().weak());
        item(ui, Command::Table(TableKind::Structures), out);
        item(ui, Command::Table(TableKind::Strings), out);
        item(ui, Command::Table(TableKind::Insulators), out);
        ui.separator();
        crate::menu::options_menu(ui, "Display options", |ui| {
            ui.label(
                egui::RichText::new("Labels in each view (yours only)")
                    .small()
                    .weak(),
            );
            if r.model.section_display.structures_ui(ui) {
                r.refresh_views();
            }
        });
        // Adding structures is the Add structure tool (on an alignment
        // point: also on the other points of that alignment, one command);
        // New pylon from a template is in the Tools menu (Structures).
        ui.separator();
        if ui
            .button("Spot structures…")
            .on_hover_text("The least-cost structures along a route (proposal saved as a new line)")
            .clicked()
        {
            r.spot.open(&r.model);
            ui.close();
        }
        if ui
            .button("Angles around structures…")
            .on_hover_text(
                "Which angle means ahead along the line (string directions, the compass)",
            )
            .clicked()
        {
            r.angles_window = Some(r.model.line.display.ahead_deg);
            ui.close();
        }
    });
    ui.menu_button("Sections", |ui| {
        use crate::library::LibraryKind;
        use crate::tables::TableKind;
        ui.label(egui::RichText::new("Libraries").small().weak());
        item(ui, Command::Library(LibraryKind::Cables), out);
        ui.separator();
        ui.label(egui::RichText::new("Tables").small().weak());
        item(ui, Command::Table(TableKind::Sections), out);
        item(ui, Command::Table(TableKind::Jumpers), out);
        item(ui, Command::Table(TableKind::SpanObjects), out);
        item(ui, Command::Table(TableKind::LoadTypes), out);
        item(ui, Command::Table(TableKind::Spacers), out);
        item(ui, Command::Table(TableKind::Systems), out);
        item(ui, Command::Table(TableKind::ConductorPaths), out);
        item(ui, Command::Table(TableKind::Circuits), out);
        ui.separator();
        crate::menu::options_menu(ui, "Display options", |ui| {
            ui.label(
                egui::RichText::new("How the wires are drawn and what is written next to them, in each view (yours only)")
                    .small()
                    .weak(),
            );
            if r.model.section_display.ui(ui) {
                // Colours, line codes and arrows: drawn again.
                r.refresh_views();
            }
        });
    });
    ui.menu_button("Criteria", |ui| {
        use crate::library::LibraryKind;
        use crate::tables::TableKind;
        if ui
            .button("Weather cases")
            .on_hover_text("Every weather case in one table: wind, ice, temperatures")
            .clicked()
        {
            out.push(Command::Table(TableKind::WeatherCases));
            ui.close();
        }
        if ui
            .button("Merge duplicate weather cases…")
            .on_hover_text(
                "Weather cases with the same weather under other names merged into one, \
                 everything naming them following",
            )
            .clicked()
        {
            r.merge_cases.open = true;
            r.merge_cases.groups = None;
            ui.close();
        }
        item(ui, Command::Library(LibraryKind::WeatherCases), out);
        item(ui, Command::Library(LibraryKind::LoadCases), out);
        if ui
            .button("Creep-Stretch…")
            .on_hover_text(
                "The weather cases for the permanent stretch due to creep and due to load \
                 (sections calculated after Creep, after Load or for Max Sag)",
            )
            .clicked()
        {
            r.creep_stretch_window = Some(r.model.line.creep_stretch.clone());
            ui.close();
        }
        if ui
            .button("Bimetallic conductor model…")
            .on_hover_text(
                "What the outer strands of bimetallic cables do above the transition \
                 temperature: bird cage or compression (cables that give their own keep it)",
            )
            .clicked()
        {
            r.bimetallic_window = Some(r.model.line.bimetallic);
            ui.close();
        }
        if ui
            .button("Calculation options…")
            .on_hover_text(
                "How the calculations are done for the whole line: what of the jumpers the \
                 sections carry at their clamps",
            )
            .clicked()
        {
            r.solver_window = Some(r.model.line.solver);
            ui.close();
        }
        if ui
            .button("Default values…")
            .on_hover_text(
                "What new objects start with in this project and what the calculations take \
                 where nothing else gives a value (jumpers, insulators, Q, thermal, clearances); \
                 exported and imported as a file",
            )
            .clicked()
        {
            r.defaults_window = Some((r.model.line.defaults, None));
            ui.close();
        }
        if ui
            .button("Design code…")
            .on_hover_text(
                "Weather and load cases from a design-code plugin and your parameter file",
            )
            .clicked()
        {
            r.design_code.open = true;
            ui.close();
        }
        crate::structure_app::loads_menu(ui, r);
    });
    ui.menu_button("Checks", |ui| {
        ui.label(
            egui::RichText::new("Run on demand; the model is not changed")
                .small()
                .weak(),
        );
        if ui
            .button("Performed checks…")
            .on_hover_text(
                "The library of the checks saved on this project line: whether the line changed \
                 since, status, note; open one again, export, import",
            )
            .clicked()
        {
            r.check_library.show();
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Structure loads").strong());
        if ui
            .button("Compute structure loads")
            .on_hover_text(
                "The loads on the structures at every attachment, in each load case \
                 (Criteria › Load cases library)",
            )
            .clicked()
        {
            if !r.loads.running() {
                r.loads.start(&r.model, ui.ctx());
            }
            r.open_page = Some((Tab::Loads, None));
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Clearances").strong());
        ui.label(egui::RichText::new(CLEARANCES_ABOUT).small().weak());
        ui.menu_button("Ground clearances", |ui| {
            ui.label(
                egui::RichText::new(
                    "From the wires to the ground and the LiDAR points: the rules are typed \
                     in each window (it starts with the ground's) and kept in the project",
                )
                .small()
                .weak(),
            );
            if ui
                .button("Vertical clearances…")
                .on_hover_text(
                    "Ground and LiDAR points straight under the wires, within a width on \
                     each side; rules by voltage, weather case, distance and target; the wind \
                     stepped; results in the window",
                )
                .clicked()
            {
                r.vertical_clearances.show(&r.model);
                ui.close();
            }
            if ui
                .button("Radial clearances…")
                .on_hover_text(
                    "The direct distance from the wires to the ground and LiDAR points; rules \
                     by voltage, weather case, distance and target; the wind stepped; results \
                     in the window",
                )
                .clicked()
            {
                r.radial_clearances.show(&r.model);
                ui.close();
            }
        });
        // Ticks keep it open.
        crate::menu::options_menu(ui, "Wire clearances", |ui| {
            ui.label(
                egui::RichText::new(
                    "Between the wires of this line and to exclusion zones: the rules of a \
                     rule file (.ohlr), in the weather case shown; results on the Clearances \
                     page (other lines: `ohl line --other-line`)",
                )
                .small()
                .weak(),
            );
            item(ui, Command::RunChecks, out);
            let c = &mut r.clearances;
            ui.checkbox(&mut c.auto, "also after every edit");
            if !c.files.is_empty() {
                ui.menu_button("Rule file", |ui| {
                    let mut chosen = None;
                    for (k, p) in c.files.iter().enumerate() {
                        let name = p
                            .file_name()
                            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                        if ui.radio(c.chosen == Some(k), name).clicked() {
                            chosen = Some(k);
                        }
                    }
                    if let Some(k) = chosen {
                        c.choose(k);
                    }
                });
            }
            if ui
                .button("Add a rule file…")
                .on_hover_text("A clearance rule file (.ohlr) kept in the project")
                .clicked()
            {
                ui.close();
                add_rule_file(r);
            }
            if ui.button("Show results").clicked() {
                open_page(dock, Tab::Clearances);
                ui.close();
            }
        });
        ui.separator();
        ui.label(egui::RichText::new("Thermal rating").strong());
        if ui
            .button("Thermal rating…")
            .on_hover_text(
                "Ampacity per section and ambient case (IEEE 738, CIGRE TB 601 / 207), and the \
                 highest temperature the clearances allow",
            )
            .clicked()
        {
            r.thermal.show(&r.model);
            ui.close();
        }
        ui.separator();
        ui.label(egui::RichText::new("Analyses").strong());
        if ui
            .button("Galloping ellipses…")
            .on_hover_text(
                "Ellipses of the wires of each span by a galloping parameter file (.ohlg)",
            )
            .clicked()
        {
            r.galloping.show(&r.model, r.case);
            ui.close();
        }
        if ui
            .button("Wire tensions…")
            .on_hover_text(
                "The tension of every span against the largest a rule allows: a factor of the \
                 wire type's rated tensile strength, per weather case",
            )
            .clicked()
        {
            r.wire_tensions.show(&r.model);
            ui.close();
        }
        // The checks of the check plugins in use, in their own submenus
        // (none without them).
        let checks: Vec<(Vec<String>, ohl_io::checks::PluginCheck)> = r
            .model
            .checks
            .iter()
            .map(|c| (c.info.menu.clone(), c.clone()))
            .collect();
        if let Some(id) = plugin_checks_menu(ui, &checks) {
            if id == ohl_io::checks::PHASE_DISTANCE {
                r.phase_distance.show(&r.model);
            } else if id == ohl_io::checks::UNEQUAL_WIND {
                r.unequal_wind.show(&r.model);
            }
            ui.close();
        }
        if ui
            .button("Vegetation…")
            .on_hover_text("Vegetation points near the wires, growth and trees falling in")
            .clicked()
        {
            r.vegetation.show(r.case);
            ui.close();
        }
        if ui
            .button("Safety volumes…")
            .on_hover_text(
                "The space within a distance (by voltage) of the wires in every position of \
                 the weather cases, for work near the line; exported as DXF, IFC, KML",
            )
            .clicked()
        {
            r.safety_volume.show(r.case);
            ui.close();
        }
        if ui
            .button("As-built wires…")
            .on_hover_text("Catenaries fitted to the LiDAR wire points, against the model")
            .clicked()
        {
            r.wirefit.open = true;
            ui.close();
        }
        ui.separator();
        ui.add_enabled(false, egui::Button::new("String swing (to come)"));
    });
    // A tick menu (the option below): ticking it keeps the menu open;
    // choosing a tool closes it.
    crate::menu::options_menu(ui, "Tools", |ui| {
        item(ui, Command::Inventory, out);
        tools_on_left_checkbox(ui, &mut r.view3d.controls);
        // Theme by theme, a line between them (as the inventory groups them).
        let mut theme = "";
        for t in crate::tools::Tool::ALL {
            if t.theme() != theme {
                // The Structures group ends with its command (a window,
                // no tool held).
                if theme == "Structures" {
                    item(ui, Command::NewPylon, out);
                }
                ui.separator();
                theme = t.theme();
            }
            let held = r.hotbar.tool() == t;
            let mut b = egui::Button::selectable(held, (t.image(ui.ctx(), 16.0), t.name()))
                .image_tint_follows_text_color(true);
            if let Some(k) = t.key() {
                b = b.shortcut_text(k);
            }
            let help = crate::controls::click_words(t.help(), r.view3d.controls.tools_on_left);
            if ui.add(b).on_hover_text(help.as_ref()).clicked() {
                out.push(Command::Hold(t));
                ui.close();
            }
        }
    });
    crate::menu::options_menu(ui, "Versions", |ui| {
        crate::project_lines::menu(ui, r);
        ui.separator();
        if ui
            .button("History…")
            .on_hover_text("The operation log: who did what; admins reverse entries")
            .clicked()
        {
            r.history_open = true;
            ui.close();
        }
        if ui
            .button("Revisions and branches…")
            .on_hover_text("Revisions, branches, tags, diff, merge, reviews and edit rights")
            .clicked()
        {
            r.versions.open = true;
            ui.close();
        }
    });
    ui.menu_button("Plugins", |ui| {
        if ui
            .button("Plugin manager…")
            .on_hover_text("The plugins installed and found: turn on or off, install, remove")
            .clicked()
        {
            r.plugin_request = Some(PluginRequest::Manager);
            ui.close();
        }
        if ui
            .button("Python console")
            .on_hover_text(
                "Scripts driving the project through the ohl module: read the line and results, \
                 edit with session commands (undoable)",
            )
            .clicked()
        {
            r.open_page = Some((Tab::Python, None));
            ui.close();
        }
    });
    ui.menu_button("Help", |ui| {
        crate::help::manual_entries(ui, &mut r.status);
        if ui
            .button(format!("About {}", crate::APP_NAME))
            .on_hover_text("Its version, what it is, its licence")
            .clicked()
        {
            *about = true;
            ui.close();
        }
        ui.separator();
        if ui
            .button("Next steps")
            .on_hover_text(
                "What a line still needs, computed from the project: each step a link to \
                 where it is done",
            )
            .clicked()
        {
            r.open_page = Some((Tab::NextSteps, None));
            ui.close();
        }
        ui.menu_button("Controls", |ui| {
            for page in ControlsPage::ALL {
                if ui
                    .button(page.name())
                    .on_hover_text(format!(
                        "How the {} view is moved and used: the keys and mouse",
                        page.name()
                    ))
                    .clicked()
                {
                    r.controls_window = Some(page);
                    ui.close();
                }
            }
        });
    });
}

/// Runs a command of the menus or the quick bar.
pub(super) fn run_command(
    ctx: &egui::Context,
    r: &mut Ready,
    dock: &mut DockState<Tab>,
    c: crate::menu::Command,
) {
    use crate::edit::Action;
    use crate::menu::Command;
    let editable = r.model.can_edit().is_ok();
    match c {
        Command::Undo if editable => {
            r.perform(Action::Undo);
        }
        Command::Redo if editable => {
            r.perform(Action::Redo);
        }
        Command::Undo | Command::Redo => {}
        Command::FitViews => {
            r.view3d.camera = None;
            r.plan.refit();
        }
        Command::ClearSelection => r.select(Selection::None),
        Command::DeleteSelected => {
            if editable {
                let target = r.selection.clone();
                r.delete_object(&target);
            }
        }
        Command::GoTo => {
            r.goto = match &r.selection {
                Selection::Structure(id) => Some(id.clone()),
                _ => r.model.line.structures.first().map(|s| s.id.clone()),
            };
        }
        Command::Inventory => {
            r.inventory = true;
            if r.view3d.captured {
                crate::view3d::View3d::set_captured(&mut r.view3d.captured, ctx, false);
            }
            r.xsections.free_mouse(ctx);
        }
        Command::RunChecks => {
            r.clearances.stale = true;
            start_clearance_check(ctx, r);
            open_page(dock, Tab::Clearances);
        }
        Command::Library(k) => open_page(dock, Tab::Library(k)),
        Command::Save => r.save_all(),
        Command::DisplayCases => {
            let shown = r.model.shown(r.case);
            r.display_window = Some(DisplayWindow {
                ticks: (0..r.model.cases.len())
                    .map(|k| shown.contains(&k))
                    .collect(),
                mode: r.model.cases_shown,
                wind: r.model.wind,
            });
        }
        Command::Table(k) => open_page(dock, Tab::Table(k)),
        Command::Hold(t) => r.hotbar.hold(t),
        Command::NewPylon => {
            r.pylon_builder = Some(crate::pylon_builder::PylonBuilder::new(&r.model));
        }
    }
}

pub(super) fn snap_panel(ui: &mut egui::Ui, snaps: &mut crate::selection::Snaps) {
    use crate::selection::Snap;
    ui.label(egui::RichText::new("What can be clicked").strong());
    for kind in Snap::ALL {
        let mut on = snaps.on(kind);
        if ui.checkbox(&mut on, kind.name()).changed() {
            if on {
                snaps.0.insert(kind);
            } else {
                snaps.0.remove(&kind);
            }
        }
    }
    ui.horizontal(|ui| {
        if ui.button("All").clicked() {
            snaps.0 = Snap::ALL.into_iter().collect();
        }
        if ui.button("None").clicked() {
            snaps.0.clear();
        }
        if ui.button("Default").clicked() {
            *snaps = crate::selection::Snaps::default();
        }
    });
    ui.label(
        egui::RichText::new(
            "At about the same distance, points win over the lines they sit on, structures over wires.",
        )
        .small(),
    );
}

pub(super) fn model_panel(ui: &mut egui::Ui, r: &mut Ready) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(egui::RichText::new("Layers").strong());
        for layer in Layer::ALL {
            let mut on = r.visible.contains(&layer);
            if ui.checkbox(&mut on, layer.name()).changed() {
                if on {
                    r.visible.insert(layer);
                } else {
                    r.visible.remove(&layer);
                }
            }
        }
        if let Some(cam) = r.view3d.camera.as_mut() {
            ui.add(
                egui::Slider::new(&mut cam.exaggeration, 1.0..=10.0)
                    .text("height ×")
                    .logarithmic(true),
            );
        }
        ui.separator();
        // Failures by weather case (the cases are chosen with Display
        // weather cases).
        // Each one a link: that weather case is shown and the views go to
        // the section or jumper (to its structure when it has no shape).
        let mut clicked: Option<(usize, String)> = None;
        let pending: Vec<&str> = r
            .model
            .cases
            .iter()
            .filter(|c| !c.ready)
            .map(|c| c.case.name.as_str())
            .collect();
        if !pending.is_empty() {
            ui.label(
                egui::RichText::new(format!("Solved when shown: {}", pending.join(", ")))
                    .small()
                    .weak(),
            );
        }
        for (k, c) in r.model.cases.iter().enumerate() {
            if c.failed.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(
                egui::RichText::new(format!(
                    "{}: {} sections or jumpers failed",
                    c.case.name,
                    c.failed.len()
                ))
                .color(ui.visuals().error_fg_color),
            )
            .id_salt(("failed", k))
            .default_open(c.failed.len() <= 10)
            .show(ui, |ui| {
                for f in &c.failed {
                    let (id, why) = f.split_once(": ").unwrap_or((f.as_str(), ""));
                    if ui
                        .link(egui::RichText::new(id).small())
                        .on_hover_text(why)
                        .clicked()
                    {
                        clicked = Some((k, id.to_owned()));
                    }
                    ui.label(egui::RichText::new(why).small().weak());
                }
            });
        }
        // Warnings of the solved sections (2026-10-22: a state beyond the
        // stress-strain curves' range; strings beyond their compression),
        // each a link like the failures; a long list scrolls.
        for (k, c) in r.model.cases.iter().enumerate() {
            let notes: Vec<(&str, &str)> = c
                .shapes
                .iter()
                .flat_map(|s| s.flags.iter().map(|f| (s.section.as_str(), f.as_str())))
                .collect();
            if notes.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{}: {} warnings", c.case.name, notes.len()))
                    .color(ui.visuals().warn_fg_color),
            )
            .id_salt(("warnings", k))
            .default_open(notes.len() <= 10)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("warnings-scroll", k))
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for (id, f) in &notes {
                            if ui
                                .link(egui::RichText::new(*id).small())
                                .on_hover_text(*f)
                                .clicked()
                            {
                                clicked = Some((k, (*id).to_owned()));
                            }
                            ui.label(egui::RichText::new(*f).small().weak());
                        }
                    });
            });
        }
        if let Some((k, id)) = clicked {
            r.go_to_failure(k, &id);
        }
        ui.separator();
        ui.label(egui::RichText::new(r.model.sources.file.display().to_string()).small());
        // What was read when the project opened: folded (a long list of
        // files and libraries).
        egui::CollapsingHeader::new(format!("Loaded ({} lines)", r.model.log.len()))
            .id_salt("load-log")
            .default_open(false)
            .show(ui, |ui| {
                for l in &r.model.log {
                    ui.label(egui::RichText::new(l).small());
                }
            });
    });
}

/// The checks of the plugins as menus: each in its submenu (`menu`, outer
/// first), as its plugin gives it; the id of the one clicked. A check this
/// OpenOHL does not measure for is shown, not usable.
pub(super) fn plugin_checks_menu(
    ui: &mut egui::Ui,
    checks: &[(Vec<String>, ohl_io::checks::PluginCheck)],
) -> Option<String> {
    let mut clicked = None;
    let mut submenus: Vec<&String> = Vec::new();
    for (path, _) in checks {
        if let Some(first) = path.first()
            && !submenus.contains(&first)
        {
            submenus.push(first);
        }
    }
    for name in submenus {
        let inner: Vec<(Vec<String>, ohl_io::checks::PluginCheck)> = checks
            .iter()
            .filter(|(p, _)| p.first() == Some(name))
            .map(|(p, c)| (p[1..].to_vec(), c.clone()))
            .collect();
        ui.menu_button(name, |ui| {
            if let Some(id) = plugin_checks_menu(ui, &inner) {
                clicked = Some(id);
            }
        });
    }
    for (_, c) in checks.iter().filter(|(p, _)| p.is_empty()) {
        let known = [ohl_io::checks::PHASE_DISTANCE, ohl_io::checks::UNEQUAL_WIND]
            .contains(&c.info.id.as_str());
        let help = c
            .info
            .explanation
            .split("\n\n")
            .next()
            .unwrap_or_default()
            .replace('\n', " ");
        let b = ui
            .add_enabled(known, egui::Button::new(format!("{}…", c.info.title)))
            .on_hover_text(format!("{help} ({})", c.plugin))
            .on_disabled_hover_text(format!(
                "{}: this version of OpenOHL does not measure for \"{}\"",
                c.plugin, c.info.id
            ));
        if b.clicked() {
            clicked = Some(c.info.id.clone());
        }
    }
    clicked
}

/// The project's ground clearance rules (Checks › Ground clearances ›
/// Vertical clearances, else the rule file chosen in Checks ›
/// Clearances) and the last clearance check's violations: what the
/// drawings take for the clearance line and the markers.
pub(super) fn drawing_checks(r: &Ready) -> (Vec<ohl_clearance::Rule>, Vec<([f64; 3], String)>) {
    let file = crate::clearance_zones::ZoneWindow::project_file(
        &r.model,
        ohl_clearance::zone::Measure::Vertical,
    );
    let ground = |rules: Vec<ohl_clearance::Rule>| -> Vec<ohl_clearance::Rule> {
        rules
            .into_iter()
            .filter(|x| x.target == ohl_clearance::Target::Ground && x.vertical.is_some())
            .collect()
    };
    let mut rules = if file.is_file() {
        ohl_clearance::RuleSet::load_with(&file, &(&r.model.line.defaults).into())
            .map(|s| ground(s.rules))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if rules.is_empty()
        && let Some(Ok(chosen)) = &r.clearances.rules
    {
        rules = ground(chosen.clone());
    }
    let markers = match &r.clearances.report {
        Some(Ok(report)) => report
            .findings
            .iter()
            .map(|f| {
                (
                    f.at,
                    format!(
                        "{} {}: {:.2} m < {:.2} m",
                        f.rule, f.measure, f.distance, f.required
                    ),
                )
            })
            .collect(),
        _ => Vec::new(),
    };
    (rules, markers)
}
