// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Load recipes, Load table and FE results panels driven as a user
//! would (egui_kittest).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_loads::{At, LoadRow, Source};

use crate::app::StructureApp;
use crate::layout::{Area, Panel};
use crate::tests::{menu, press};

/// A copy of example structure `name` in a folder of its own.
fn copy_of(name: &str, tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures")
        .join(name);
    let dir =
        std::env::temp_dir().join(format!("ohl-structure-loads-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let to = dir.join(name);
    std::fs::copy(&from, &to).unwrap();
    to
}

fn harness(path: &Path) -> Harness<'static, StructureApp> {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut builder = Harness::builder().with_size(egui::vec2(1300.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let path = path.to_path_buf();
    let mut h = builder.build_eframe(move |cc| {
        let mut app = StructureApp::new(cc.wgpu_render_state.clone());
        app.open(&path);
        app
    });
    h.run();
    h
}

/// The loads of the recipe shown (its place in the file).
fn loads_of(h: &Harness<'static, StructureApp>, case: usize) -> usize {
    let f = h.state().file.as_ref().unwrap();
    ohl_structure_edit::load_cases(f.doc())[case].loads.len()
}

/// A row of the table.
fn row(h: &Harness<'static, StructureApp>, case: &str, load: usize, label: &str) -> LoadRow {
    h.state()
        .loads
        .table()
        .rows
        .iter()
        .find(|r| r.case == case && r.load == Some(load) && r.label == label)
        .cloned()
        .unwrap_or_else(|| panic!("{case} {load} {label}"))
}

/// A load added to a recipe is one edit of the file, undone with Ctrl+Z;
/// a case added and removed likewise.
#[test]
fn a_load_row_is_added_and_undone() {
    let path = copy_of("type-02.ohls", "add");
    let mut h = harness(&path);
    let before = h.state().file.as_ref().unwrap().text();
    menu(&mut h, "Load cases", "Load case editor…");
    assert_eq!(h.state().area, Area::LoadCases);
    assert_eq!(loads_of(&h, 0), 3);
    press(&mut h, "+ Load");
    assert_eq!(loads_of(&h, 0), 4);
    assert!(h.state().dirty());
    assert_eq!(
        h.state().file.as_ref().unwrap().undo_name(),
        Some("add a load to wind")
    );
    // The new load reads: the line's vertical load at every point.
    assert!(h.state().loads.recipes().is_ok());
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(loads_of(&h, 0), 3);
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    // A case: added, its first line forces, then undone in two steps.
    h.get_by_label("+ Case").click();
    h.run();
    press(&mut h, "Add line forces");
    let f = h.state().file.as_ref().unwrap();
    let cases = ohl_structure_edit::load_cases(f.doc());
    assert_eq!(cases.len(), 5);
    assert_eq!(cases[4].get("name"), "case 1");
    let recipes = h.state().loads.recipes().unwrap().to_vec();
    assert_eq!(recipes[4].line.len(), 1);
    // Opened on its own: no line result to choose, one to type.
    assert_eq!(recipes[4].line[0].result, crate::case_editor::NO_RESULT);
    h.state_mut().undo();
    h.state_mut().undo();
    h.run();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// Opened on its own, the loads that take a line value have rows marked
/// "no line value"; the values of the cases themselves are there.
#[test]
fn opened_alone_the_line_values_are_missing() {
    let path = copy_of("type-02.ohls", "alone");
    let mut h = harness(&path);
    menu(&mut h, "Load cases", "Load table…");
    assert_eq!(h.state_mut().focused(), Some(Panel::LoadTable));
    let r = row(&h, "wind", 1, "A1");
    assert_eq!(
        r.missing.as_deref(),
        Some("no line value: transverse_factored in \"wind\"")
    );
    assert_eq!(r.force, [0.0; 3]);
    // Shown as such (the "wind" case is shown first).
    assert!(
        !h.query_all_by_label("no line value: transverse_factored in \"wind\"")
            .collect::<Vec<_>>()
            .is_empty()
    );
    // The short line (the whole sentence on hover).
    h.get_by_label("No line values (opened on its own)");
    // A value of the case needs no line.
    let r = row(&h, "ice, unbalanced", 2, "A1");
    assert_eq!((r.force, r.source), ([0.0, 2000.0, 0.0], Source::Recipe));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A value typed over a row stays when the recipes are evaluated again,
/// is saved beside the file and read back with it.
#[test]
fn a_typed_value_is_kept_after_evaluating_again() {
    let path = copy_of("type-02.ohls", "typed");
    let mut h = harness(&path);
    menu(&mut h, "Load cases", "Load case editor…");
    let r = row(&h, "wind", 1, "A1");
    h.state_mut().loads.type_value(&r, [3000.0, 0.0, 0.0]);
    h.run();
    let typed = row(&h, "wind", 1, "A1");
    assert_eq!(
        (typed.force, typed.source),
        ([3000.0, 0.0, 0.0], Source::Typed)
    );
    assert!(typed.missing.is_none());
    assert!(h.state().dirty());
    // A recipe changed: the table is evaluated again, the value stays.
    press(&mut h, "+ Load");
    assert_eq!(loads_of(&h, 0), 4);
    let again = row(&h, "wind", 1, "A1");
    assert_eq!(
        (again.force, again.source),
        ([3000.0, 0.0, 0.0], Source::Typed)
    );
    // Saved: the file and the typed loads beside it.
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().dirty());
    let kept = ohl_structure_loads::overrides_path(&path, None);
    assert!(kept.ends_with("type-02.loads.csv"), "{}", kept.display());
    assert!(
        std::fs::read_to_string(&kept)
            .unwrap()
            .contains("wind,1,point,A1,3000,")
    );
    // Opened again: the value is back.
    let mut h = harness(&path);
    menu(&mut h, "Load cases", "Load case editor…");
    let back = row(&h, "wind", 1, "A1");
    assert_eq!(
        (back.force, back.source),
        ([3000.0, 0.0, 0.0], Source::Typed)
    );
    // Taken back: the row says again why it has no value.
    h.state_mut().loads.clear_value(&back);
    h.run();
    assert!(row(&h, "wind", 1, "A1").missing.is_some());
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The table written as CSV and read back is the same table.
#[test]
fn the_table_goes_out_and_back_as_csv() {
    let path = copy_of("type-02.ohls", "csv");
    let mut h = harness(&path);
    menu(&mut h, "Load cases", "Load case editor…");
    let r = row(&h, "ice, unbalanced", 1, "A11");
    h.state_mut().loads.type_value(&r, [0.0, 0.0, 7000.0]);
    let own = LoadRow {
        case: "client".into(),
        load: None,
        at: At::Point,
        label: "A1".into(),
        force: [1000.0, 2000.0, 3000.0],
        source: Source::Typed,
        evaluated: None,
        missing: None,
        note: String::new(),
        area: None,
    };
    h.state_mut().loads.type_value(&own, own.force);
    h.run();
    let table = h.state().loads.table().clone();
    let csv = path.with_file_name("table.csv");
    h.state().loads.export_table(&csv).unwrap();
    // Typed values dropped, then the table read back.
    let rows: Vec<LoadRow> = h.state().loads.table().rows.clone();
    for r in rows.iter().filter(|r| r.source == Source::Typed) {
        h.state_mut().loads.clear_value(r);
    }
    h.run();
    assert_ne!(h.state().loads.table(), &table);
    let f = h.state().file.clone().unwrap();
    let n = h.state_mut().loads.import_table(&f, &csv).unwrap();
    assert_eq!(n, 2);
    h.run();
    assert_eq!(h.state().loads.table(), &table);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A typed case solved in the FE results panel: the loads in equilibrium with
/// the reactions, the usage shown.
#[test]
fn results_of_a_typed_case() {
    let path = copy_of("type-02.ohls", "results");
    let mut h = harness(&path);
    menu(&mut h, "Load cases", "Load case editor…");
    for (label, force) in [
        ("A1", [4000.0, 1000.0, 9000.0]),
        ("A11", [4000.0, 0.0, 9000.0]),
    ] {
        let own = LoadRow {
            case: "typed".into(),
            load: None,
            at: At::Point,
            label: label.into(),
            force,
            source: Source::Typed,
            evaluated: None,
            missing: None,
            note: String::new(),
            area: None,
        };
        h.state_mut().loads.type_value(&own, force);
    }
    // The editor's window closed (it lies over the panels).
    crate::layout::close_panel(&mut h.state_mut().dock, crate::layout::Panel::LoadRecipes);
    h.run();
    menu(&mut h, "Results", "FE results…");
    h.state_mut().results.case = "typed".into();
    h.run();
    h.get_by_label("▶ Run").click();
    let started = Instant::now();
    loop {
        h.step();
        if !h.state().results.running() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "no result in 60 s"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run();
    let run = h.state().results.results.clone().unwrap().unwrap();
    assert_eq!(run.cases.len(), 1);
    assert_eq!(run.extension, None);
    assert!(run.cases[0].pole.is_none());
    let r = run.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(r.case, "typed");
    // Σ loads: the typed (8 kN across, 1 kN ahead, 18 kN down) and the
    // weight; the reactions balance them.
    assert!((r.load_sum[0] - 8000.0).abs() < 1e-6);
    assert!((r.load_sum[1] - 1000.0).abs() < 1e-6);
    assert!(r.load_sum[2] < -18_000.0);
    assert!(r.imbalance() < 1e-6 * r.load_sum[2].abs(), "{r:?}");
    let g = r.governing().unwrap();
    assert!(g.usage.unwrap() > 0.0);
    h.get_by_label_contains(&format!("member {} — ", g.id));
    h.get_by_label("equilibrium");
    // Drawn: the deformed shape over the structure.
    assert!(h.state().results.overlay(None).is_some());
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
