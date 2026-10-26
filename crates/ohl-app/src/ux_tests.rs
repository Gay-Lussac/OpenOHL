// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The first steps of a new user: the New project window, the starter
//! template, the Next steps page, the Help menu, the clearance windows'
//! first rule, the tool windows as panels. With `OHL_RENDER_PNG` set to a
//! folder (and a GPU) the windows are saved there as images.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};

use crate::app::{App, Tab};
use crate::model::{self, Sources};
use crate::tests::root;

/// A folder of its own for test `name`.
fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-ux-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Saves the window as `name`.png in `OHL_RENDER_PNG`, when set and a GPU
/// renders.
pub fn shot(harness: &mut Harness<'_, App>, name: &str) {
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = harness.render()
    {
        // The status bar shows the login name ("editing as …"): images
        // for the docs are made as a neutral user, never the real one.
        let user = crate::files::user();
        assert_eq!(
            user, "tester",
            "screenshots are made with USER=tester (the status bar shows the user)"
        );
        let path = PathBuf::from(dir).join(format!("{name}.png"));
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// A harness on the GPU when there is one (images), else without.
fn harness(
    app: impl FnOnce(&mut eframe::CreationContext<'_>) -> App + 'static,
) -> Harness<'static, App> {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut b = Harness::builder().with_size(egui::vec2(1500.0, 950.0));
    if gpu {
        b = b.wgpu();
    }
    b.build_eframe(app)
}

/// The project at `path` loaded as "tester" (its admin).
fn load(path: &std::path::Path) -> model::Model {
    model::load(
        Sources {
            file: path.to_path_buf(),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap()
}

/// A project from the starter template holds the starter's libraries and
/// nothing else: no route, alignment, structure or wire; its pylon
/// templates and survey points lie beside it.
#[test]
fn a_starter_project_has_the_libraries_and_nothing_else() {
    use crate::files::{Template, create_project_from};
    let dir = temp("starter");
    let path = create_project_from(
        &dir.join("first"),
        "My first line",
        crate::files::LOCAL_CRS,
        "tester",
        Template::Starter,
    )
    .unwrap();
    let session = ohl_session::Session::open(&path).unwrap();
    let line = session.line();
    assert!(line.routes.is_empty() && line.alignments.is_empty());
    assert!(line.structures.is_empty() && line.sections.is_empty());
    assert!(line.jumpers.is_empty());
    assert_eq!(line.crs, "local");
    let starter = crate::starter::library_files(&root().join("examples/starter")).unwrap();
    for (kind, _) in crate::model::PROJECT_LIBRARIES {
        let kept = session.library_files(kind).unwrap();
        let wanted: Vec<(String, String)> = starter
            .iter()
            .filter(|(k, _, _)| k == kind)
            .map(|(_, n, t)| (n.clone(), t.clone()))
            .collect();
        assert_eq!(kept, wanted, "{kind}");
    }
    drop(session);
    assert!(dir.join("Structure templates/delta-1c.ohlpt").is_file());
    assert!(dir.join(crate::starter::SURVEY).is_file());
    // Opened: the libraries are read; the line still needs its route.
    let m = load(&path);
    assert!(!m.library.is_empty());
    assert_eq!(m.types.as_ref().map(|(s, _)| s.len()), Some(2));
    assert_eq!(m.cases.len(), 5);
    assert!(m.sources.load_cases.is_some());
    let steps = crate::next_steps::steps(&m, true);
    let done = |t: &str| steps.iter().find(|s| s.title == t).unwrap().done;
    assert!(!done("A route") && !done("An alignment") && !done("Structures on the line"));
    assert!(done("Wire types") && done("Structure types") && done("Weather cases"));
    assert!(done("Load cases"));
    // A project's own template is found in its folder, and in the folder's
    // name before 2026-10-21 ("Pylon templates") too.
    let own = |m: &crate::model::Model| {
        let (all, errors) = crate::pylon_builder::templates(m);
        assert!(errors.is_empty(), "{errors:?}");
        all.iter().any(|t| t.id == "starter-own")
    };
    let delta = std::fs::read_to_string(dir.join("Structure templates/delta-1c.ohlpt")).unwrap();
    let mine = delta.replace("id = \"delta-1c\"", "id = \"starter-own\"");
    assert_ne!(mine, delta);
    std::fs::write(dir.join("Structure templates/own.ohlpt"), &mine).unwrap();
    assert!(own(&m));
    std::fs::remove_file(dir.join("Structure templates/own.ohlpt")).unwrap();
    assert!(!own(&m));
    std::fs::create_dir_all(dir.join("Pylon templates")).unwrap();
    std::fs::write(dir.join("Pylon templates/own.ohlpt"), &mine).unwrap();
    assert!(own(&m));
    // An empty project: nothing but its line.
    let empty =
        crate::files::create_project(&dir.join("empty"), "Empty", "local", "tester").unwrap();
    let session = ohl_session::Session::open(&empty).unwrap();
    for (kind, _) in crate::model::PROJECT_LIBRARIES {
        assert!(session.library_files(kind).unwrap().is_empty(), "{kind}");
    }
}

/// The Next steps follow the model: an empty project needs everything,
/// the example has its route, alignments, structures, cables and wires.
#[test]
fn next_steps_follow_the_model() {
    let dir = temp("next-steps");
    let empty = crate::files::create_project(&dir.join("e"), "E", "local", "tester").unwrap();
    let m = load(&empty);
    let steps = crate::next_steps::steps(&m, false);
    assert!(steps.iter().all(|s| !s.done), "{steps:?}");
    let m = crate::tests::synthetic_project("next-steps");
    let steps = crate::next_steps::steps(&m, false);
    for t in [
        "A route",
        "An alignment",
        "Structures on the line",
        "Strings on the structures",
        "Wire types",
        "Wires strung",
        "Jumpers",
        "Weather cases",
        "Load cases",
    ] {
        assert!(steps.iter().find(|s| s.title == t).unwrap().done, "{t}");
    }
    assert!(
        !steps
            .iter()
            .find(|s| s.title == "Clearance rules")
            .unwrap()
            .done
    );
    // The page: its links work (the route's link opens the first steps).
    let model = load(&empty);
    let mut h = harness(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut().show_tab(Tab::NextSteps);
    h.run();
    h.get_by_label("Next: Name the route…");
    shot(&mut h, "next-steps");
    h.state_mut().ready().unwrap().first_steps = None;
    h.get_by_label("Next: Name the route…").click();
    h.run();
    assert!(h.state_mut().ready().unwrap().first_steps.is_some());
    // (Its window closed first: it may lie over the page.)
    h.get_by_label("Later").click();
    h.run();
    h.get_by_label("Hold New alignment").click();
    h.run();
    assert_eq!(
        h.state_mut().ready().unwrap().hotbar.tool(),
        crate::tools::Tool::NewAlignment
    );
}

/// File › New project: wide fields, the coordinate system checked and
/// named, the starter template offered; the start page offers the
/// starter and the example.
#[test]
fn the_new_project_window_offers_the_starter() {
    let mut h = harness(|cc| App::empty(Sources::default(), cc.wgpu_render_state.clone()));
    h.run();
    h.get_by_label("New project from the starter template…");
    h.get_by_label("Open the example (copy)…");
    shot(&mut h, "start-page");
    h.state_mut()
        .file_action_for_test(crate::files::FileAction::NewStarter);
    h.run();
    h.run();
    h.get_by_label("Starter template (for learning)");
    h.get_by_label("Coordinates: local, in metres");
    shot(&mut h, "new-project-starter");
    // A coordinate system of the list: searched, named.
    h.get_by_label("A coordinate system…").click();
    h.run();
    h.get_by_label("choose a coordinate system");
}

/// The example opened as a copy: its files in a new folder, the line
/// with its ground and weather cases.
#[test]
fn the_example_is_copied_before_it_opens() {
    let dir = temp("example");
    let s = crate::files::copy_example(&dir).unwrap();
    assert!(s.file.is_file());
    assert!(s.file.starts_with(&dir));
    assert!(s.terrain.as_ref().unwrap().is_file());
    assert!(s.weather.as_ref().unwrap().is_file());
    let again = crate::files::copy_example(&dir).unwrap();
    assert_ne!(again.file, s.file, "a second copy beside the first");
}

/// Help: the user guide, the glossary and the tutorial open the manual
/// on this computer (nothing started in tests: the file is noted); About
/// shows the version.
#[test]
fn help_opens_the_manual_and_about() {
    let model = crate::tests::synthetic_project("help");
    let mut h = harness(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label("User guide…").click();
    h.run();
    let opened = crate::help::OPENED.lock().unwrap().clone();
    assert!(opened.iter().any(|p| p.is_file()), "{opened:?}");
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label("Glossary…").click();
    h.run();
    let opened = crate::help::OPENED.lock().unwrap().clone();
    assert!(
        opened
            .iter()
            .any(|p| p.is_file() && p.to_string_lossy().contains("glossary")),
        "{opened:?}"
    );
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label("Start from scratch (tutorial)…").click();
    h.run();
    let opened = crate::help::OPENED.lock().unwrap().clone();
    assert!(
        opened
            .iter()
            .any(|p| p.is_file() && p.to_string_lossy().contains("tutorial")),
        "{opened:?}"
    );
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label("About OHL-Design").click();
    h.run();
    h.get_by_label_contains("GNU Affero General Public License");
}

/// A clearance window always has a rule: it starts with the ground's,
/// its distance to set, and Run refuses until it is set (in its row of the
/// rules table); the last rule is not
/// removed; the Next steps see the rule.
#[test]
fn clearance_windows_start_with_a_rule_to_set() {
    use crate::clearance_zones::{first_rule, unset};
    let dir = temp("zones");
    let path = crate::files::create_project(&dir.join("z"), "Z", "local", "tester").unwrap();
    let model = load(&path);
    let mut h = harness(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut()
        .ready()
        .unwrap()
        .vertical_clearances
        .show(&load(&path));
    h.run();
    h.run();
    {
        let w = &h.state_mut().ready().unwrap().vertical_clearances;
        assert_eq!(w.rules, vec![first_rule()]);
        assert!(w.why_not_run().unwrap().contains("set the distance"));
    }
    assert!(h.get_by_label("Run").accesskit_node().is_disabled());
    h.get_by_label_contains("Before Run:");
    shot(&mut h, "vertical-clearances-first-rule");
    // A start refused: no check runs.
    {
        let r = h.state_mut().ready().unwrap();
        let ctx = egui::Context::default();
        r.vertical_clearances.start(&mut r.model, &ctx);
        let (report, running) = r.vertical_clearances.result();
        assert!(!running);
        assert!(
            report
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("set the distance")
        );
        assert!(!crate::next_steps::has_rules(r));
    }
    // The distance set in its row.
    {
        let w = &mut h.state_mut().ready().unwrap().vertical_clearances;
        w.rules[0].distance = ohl_clearance::Requirement::Fixed(7.0);
        w.mark_dirty();
    }
    h.run();
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let w = &r.vertical_clearances;
        assert_eq!(w.rules.len(), 1);
        assert!(!unset(&w.rules[0]) && w.rules[0].ground);
        assert!(w.why_not_run().is_none());
        assert!(crate::next_steps::has_rules(r));
    }
    assert!(!h.get_by_label("Run").accesskit_node().is_disabled());
}

/// A tool window becomes a panel of the dock ("Dock as a panel"), works
/// there, and still closes with its tool (its tab with it); it comes up
/// docked the next time; Float makes it a window again; closing its tab
/// closes the tool. A page's tab moves with its context menu.
#[test]
fn tool_windows_dock_and_still_close_with_their_tool() {
    use crate::dockable::Panel;
    let model = crate::tests::synthetic_project("dockable");
    let mut h = harness(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    let open = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.display_window = Some(crate::app::DisplayWindow {
            ticks: vec![true; r.model.cases.len()],
            mode: r.model.cases_shown,
            wind: r.model.wind,
        });
        h.run();
        h.run();
        h.run();
    };
    let docked = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .dock()
            .find_tab(&Tab::Tool(Panel::DisplayCases))
            .is_some()
    };
    open(&mut h);
    assert!(!docked(&mut h), "a window first");
    h.get_by_label("Dock as a panel").click();
    h.run();
    h.run();
    h.run();
    assert!(docked(&mut h));
    // Its content is in the panel and works: Cancel closes the tool, the
    // tab goes with it.
    h.get_by_label("Float");
    shot(&mut h, "display-cases-docked");
    h.get_by_label("Cancel").click();
    h.run();
    h.run();
    assert!(h.state_mut().ready().unwrap().display_window.is_none());
    assert!(!docked(&mut h));
    // Docked again next time; Float: a window again.
    open(&mut h);
    assert!(docked(&mut h));
    h.get_by_label("Float").click();
    h.run();
    h.run();
    assert!(!docked(&mut h));
    h.get_by_label("Dock as a panel");
    // Docked, its tab closed: the tool closes.
    h.get_by_label("Dock as a panel").click();
    h.run();
    h.run();
    assert!(docked(&mut h));
    crate::dockable::host(&h.ctx, |x| x.closed.insert(Panel::DisplayCases));
    h.run();
    h.run();
    assert!(h.state_mut().ready().unwrap().display_window.is_none());
    assert!(!docked(&mut h));
    // A page moves with its tab's context menu: a table to the left.
    let t = Tab::Table(crate::tables::TableKind::Structures);
    crate::app::open_page(h.state_mut().dock(), t);
    crate::app::move_page(h.state_mut().dock(), t, crate::app::Place::Left);
    let dock = h.state_mut().dock();
    assert_eq!(
        dock.find_tab(&t).unwrap().node_path(),
        dock.find_tab(&Tab::Model).unwrap().node_path()
    );
}

/// File › Export › Drawings… with a sheet template: the window shows the
/// template, its values, the versions and its options; a sheet is
/// previewed in a window of its own, as the export draws it.
#[test]
fn the_export_window_shows_the_sheet_template_and_a_sheet() {
    let model = crate::tests::synthetic_project("ux-sheet-template");
    let mut h = harness(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let file = root().join("examples/sheet-templates/example-a1.ohlsheet");
        let t = ohl_drawing::SheetTemplate::load(&file).unwrap();
        r.model
            .project_op(
                "sheet template",
                ohl_server::Op::PutLibrary {
                    kind: crate::sheet_ui::KIND.into(),
                    name: "example-a1.ohlsheet".into(),
                    text: t.embedded(),
                },
            )
            .unwrap();
        r.drawings.open();
        let w = r.drawings.window.as_mut().unwrap();
        w.sheet.template = "project:example-a1".into();
        w.page = "template".into();
        w.sheet
            .fields
            .values
            .insert("project_no".into(), "P-1".into());
        w.sheet_preview = Some(1);
    }
    h.run();
    h.run();
    h.get_by_label_contains("Values of the template");
    h.get_by_label_contains("Sheet 1 of");
    shot(&mut h, "drawings-sheet-template");
}
