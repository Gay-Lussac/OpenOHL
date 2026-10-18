// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The first steps of a new user: the New project window, the starter
//! template, the Next steps page, the Help menu, the clearance windows'
//! first rule, the tool windows as panels. With `OLS_RENDER_PNG` set to a
//! folder (and a GPU) the windows are saved there as images.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::{App, Tab};
use crate::model::{self, Sources};
use crate::tests::root;

/// A folder of its own for test `name`.
fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ols-ux-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Saves the window as `name`.png in `OLS_RENDER_PNG`, when set and a GPU
/// renders.
pub fn shot(harness: &mut Harness<'_, App>, name: &str) {
    if let Some(dir) = std::env::var_os("OLS_RENDER_PNG")
        && let Ok(image) = harness.render()
    {
        let path = PathBuf::from(dir).join(format!("{name}.png"));
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// A harness on the GPU when there is one (images), else without.
fn harness(
    app: impl FnOnce(&mut eframe::CreationContext<'_>) -> App + 'static,
) -> Harness<'static, App> {
    let gpu = ols_render::headless_device().is_ok();
    assert!(
        gpu || std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    let session = ols_session::Session::open(&path).unwrap();
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
    assert!(dir.join("Pylon templates/delta-1c.olspt").is_file());
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
    assert!(done("Cables") && done("Structure types") && done("Weather cases"));
    assert!(done("Load cases"));
    // An empty project: nothing but its line.
    let empty =
        crate::files::create_project(&dir.join("empty"), "Empty", "local", "tester").unwrap();
    let session = ols_session::Session::open(&empty).unwrap();
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
        "Cables",
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

/// Help: the user guide opens the manual on this computer (nothing
/// started in tests: the file is noted); About shows the version.
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
    h.get_by_label("About OHL-Design").click();
    h.run();
    h.get_by_label_contains("Mozilla Public License");
}
