// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Started for a placed structure (`--structure`, `--line-values`), driven
//! as a user would (egui_kittest).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ols_structure_loads::{
    LinePoint, LineQuantity, LineValue, LineValues, LoadRow, OtherPlace, PlacedValues, Source,
    line_values_path, overrides_path,
};

use crate::app::StructureApp;
use crate::layout::Panel;
use crate::tests::menu;

/// A copy of example structure type-02 in a folder of its own.
fn copy_of(tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures/type-02.olss");
    let dir =
        std::env::temp_dir().join(format!("ols-structure-placed-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let to = dir.join("type-02.olss");
    std::fs::copy(&from, &to).unwrap();
    to
}

/// Invented line values at T2's points A1 (set 1) and A11 (set 11): the
/// wind case's factored forces `wind` (N) at both, the ice case's
/// vertical.
fn values(wind: [f64; 3]) -> LineValues {
    let mut out = LineValues::default();
    for (label, set) in [("A1", 1), ("A11", 11)] {
        out.points.push(LinePoint {
            label: label.into(),
            sets: vec![set],
            phases: vec![],
        });
        let at = |case: &str, quantity, value| LineValue {
            case: case.into(),
            point: Some(label.into()),
            set: None,
            quantity,
            value,
        };
        out.values.extend([
            at("wind", LineQuantity::TransverseFactored, wind[0]),
            at("wind", LineQuantity::LongitudinalFactored, wind[1]),
            at("wind", LineQuantity::VerticalFactored, wind[2]),
            at("ice", LineQuantity::Vertical, 8000.0),
        ]);
    }
    out
}

fn write(path: &Path, wind: [f64; 3]) {
    let mut v = PlacedValues::pending("T2", "test line", "type-02", "");
    v.note = None;
    v.cases = vec!["wind".into(), "ice".into()];
    v.values = values(wind);
    v.others = vec![OtherPlace {
        structure: "T4".into(),
        values: values([1000.0, 0.0, 2000.0]),
    }];
    v.save(path).unwrap();
}

fn harness(path: &Path, values: &Path) -> Harness<'static, StructureApp> {
    let gpu = ols_render::headless_device().is_ok();
    assert!(
        gpu || std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut builder = Harness::builder().with_size(egui::vec2(1300.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let (path, values) = (path.to_path_buf(), values.to_path_buf());
    let mut h = builder.build_eframe(move |cc| {
        let mut app = StructureApp::new(cc.wgpu_render_state.clone());
        app.loads.placed = Some("T2".into());
        app.line_values = Some(values.clone());
        app.open(&path);
        app
    });
    h.run();
    h
}

/// The table evaluated as it is now.
fn refresh(h: &mut Harness<'static, StructureApp>) {
    let s = h.state_mut();
    s.loads.refresh(s.file.as_ref().unwrap());
}

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

/// Started with T2's line values: the header says whose loads they are,
/// the recipes take the line's values, In the line lists them; values
/// typed are saved for T2 (not for the type); the line-values file
/// written again is read again.
#[test]
fn a_placed_structure_shows_its_line_values_and_keeps_its_typed_loads() {
    let path = copy_of("show");
    let at = line_values_path(&path, "T2");
    write(&at, [1500.0, -200.0, 6000.0]);
    let mut h = harness(&path, &at);
    h.get_by_label("loads of T2 in test line");
    menu(&mut h, "Load cases", "Load recipes…");
    refresh(&mut h);
    let r = row(&h, "wind", 1, "A1");
    assert_eq!((r.force, r.source), ([1500.0, 0.0, 0.0], Source::Line));
    assert!(r.missing.is_none());
    let r = row(&h, "ice, unbalanced", 1, "A11");
    assert!((r.force[2] - 1.3 * 8000.0).abs() < 1e-9, "{:?}", r.force);
    // In the line: the values per point and case, the other place.
    h.state_mut().focus(Panel::InTheLine);
    h.run();
    // A11 once per case.
    assert_eq!(h.get_all_by_label("A11").count(), 2);
    h.get_by_label("Other places of this structure (1)");

    // A value typed and saved: kept for T2.
    h.state_mut().focus(Panel::LoadRecipes);
    h.run();
    let r = row(&h, "wind", 1, "A1");
    h.state_mut().loads.type_value(&r, [2500.0, 0.0, 0.0]);
    h.run();
    assert!(h.state().dirty());
    assert!(h.state_mut().save(None));
    let kept = overrides_path(&path, Some("T2"));
    assert!(kept.ends_with("type-02.T2.loads.csv"));
    assert!(
        std::fs::read_to_string(&kept)
            .unwrap()
            .contains("wind,1,point,A1,2500,")
    );
    assert!(!overrides_path(&path, None).exists());

    // The line solved again: the file rewritten, read again.
    write(&at, [1700.0, 0.0, 6100.0]);
    let later = SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&at)
        .unwrap()
        .set_modified(later)
        .unwrap();
    std::thread::sleep(Duration::from_millis(600));
    h.run();
    refresh(&mut h);
    let r = row(&h, "wind", 1, "A11");
    assert_eq!(r.force, [1700.0, 0.0, 0.0]);
    // The typed value stays over the new one, the new one kept beside.
    let r = row(&h, "wind", 1, "A1");
    assert_eq!(
        (r.force, r.evaluated, r.source),
        ([2500.0, 0.0, 0.0], Some([1700.0, 0.0, 0.0]), Source::Typed)
    );

    // Opened again for T2: the typed value is back.
    let mut h = harness(&path, &at);
    refresh(&mut h);
    assert_eq!(row(&h, "wind", 1, "A1").force, [2500.0, 0.0, 0.0]);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A copy of example structure `name` in a folder of its own.
fn copy_named(name: &str, tag: &str) -> PathBuf {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures")
        .join(name);
    let dir =
        std::env::temp_dir().join(format!("ols-structure-placed-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let to = dir.join(name);
    std::fs::copy(&from, &to).unwrap();
    to
}

/// The line had no load case file: the status block of the load panels
/// says so, plainly, with what to do.
#[test]
fn no_load_case_file_is_said_plainly() {
    let path = copy_of("no-cases");
    let at = line_values_path(&path, "T2");
    PlacedValues::pending(
        "T2",
        "test line",
        "type-02",
        "no values from the line: no load case file: Criteria › Load cases (a .olslc file \
         next to the line, or the project's own)",
    )
    .save(&at)
    .unwrap();
    let mut h = harness(&path, &at);
    menu(&mut h, "Load cases", "Load table…");
    h.get_by_label_contains("The line gives no values for T2: the project has no load cases");
    h.get_by_label_contains("Criteria › Load cases, or a design-code plugin");
    // The same in Line values.
    h.state_mut().focus(Panel::InTheLine);
    h.run();
    h.get_by_label_contains("the project has no load cases");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// Without any recipe, the load table is the line's forces: a row per
/// case and attachment point, from the line. Stored in the structure
/// file, they are its own cases (read alone, without the line); stored
/// again, they replace those (asked first).
#[test]
fn the_line_forces_are_the_table_and_are_stored_in_the_file() {
    let path = copy_named("type-01.olss", "store");
    let at = line_values_path(&path, "T2");
    write(&at, [1500.0, -200.0, 6000.0]);
    let mut h = harness(&path, &at);
    menu(&mut h, "Load cases", "Load table…");
    refresh(&mut h);
    let rows = h.state().loads.table().rows.clone();
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert!(
        rows.iter()
            .all(|r| r.source == Source::Line && r.load.is_none())
    );
    let wind = |h: &Harness<'static, StructureApp>, label: &str| {
        h.state()
            .loads
            .table()
            .rows
            .iter()
            .find(|r| r.case == "wind" && r.label == label && r.load.is_none())
            .cloned()
            .unwrap()
    };
    assert_eq!(wind(&h, "A1").force, [1500.0, -200.0, 6000.0]);
    let ice = rows.iter().find(|r| r.case == "ice" && r.label == "A11");
    assert_eq!(ice.unwrap().force, [0.0, 0.0, 8000.0]);
    h.get_by_label_contains("The line gives the forces at 2 point(s) of T2 in 2 case(s)");
    // No recipe: said, with the line's cases to take.
    menu(&mut h, "Load cases", "Load recipes…");
    h.get_by_label_contains("This structure has no load cases of its own");
    h.get_by_label("Take the line's load cases as they are");

    // Stored: every case ticked, one edit.
    menu(&mut h, "Load cases", "Load table…");
    h.get_by_label("Store in the structure file…").click();
    h.run();
    h.get_by_label("Store").click();
    h.run();
    let names = |h: &Harness<'static, StructureApp>| {
        ols_structure_edit::load_cases(h.state().file.as_ref().unwrap().doc())
            .iter()
            .map(|c| c.get("name").to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&h), ["wind", "ice"]);
    assert!(h.state_mut().save(None));
    // The line's cases are now beside the stored ones.
    refresh(&mut h);
    assert!(
        h.state()
            .loads
            .table()
            .rows
            .iter()
            .any(|r| r.case == "wind (line)")
    );

    // Opened alone: the stored cases give the same forces.
    let mut alone = crate::tests::harness(&path);
    menu(&mut alone, "Load cases", "Load table…");
    alone.get_by_label_contains("No line values: open this from a placed structure");
    alone.get_by_label_contains("Stored in this file from the line: wind, ice");
    let f = alone.state().file.clone().unwrap();
    alone.state_mut().loads.refresh(&f);
    let sum = |label: &str| {
        alone
            .state()
            .loads
            .table()
            .rows
            .iter()
            .filter(|r| r.case == "wind" && r.label == label)
            .fold([0.0; 3], |s, r| [0, 1, 2].map(|i| s[i] + r.force[i]))
    };
    let a1 = sum("A1");
    assert!(
        a1.iter()
            .zip([1500.0, -200.0, 6000.0])
            .all(|(x, y)| (x - y).abs() < 0.1),
        "{a1:?}"
    );
    drop(alone);

    // The line solved again; stored again: asked, then replaced.
    write(&at, [1700.0, 0.0, 6100.0]);
    let mut h = harness(&path, &at);
    menu(&mut h, "Load cases", "Load table…");
    h.get_by_label("Store in the structure file…").click();
    h.run();
    h.get_by_label("Store").click();
    h.run();
    h.get_by_label_contains("The file has cases of these names: wind, ice");
    h.get_by_label("Replace and store").click();
    h.run();
    assert_eq!(names(&h), ["wind", "ice"]);
    let text = h.state().file.as_ref().unwrap().text();
    assert!(text.contains("1700.0 N"), "{text}");
    assert!(!text.contains("1500.0 N"), "{text}");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
