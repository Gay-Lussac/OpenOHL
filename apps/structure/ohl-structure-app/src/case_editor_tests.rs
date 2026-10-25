// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Load case editor driven as a user would (egui_kittest), started
//! for a placed structure with invented line results: line forces added,
//! a point unticked, Mirror wind.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_loads::{
    CaseWind, Direction, LinePoint, LineQuantity, LineValue, LineValues, PlacedValues, Source,
    line_values_path,
};

use crate::app::StructureApp;
use crate::tests::{menu, press};

const LEFT: &str = "storm — wind from the left";
const RIGHT: &str = "storm — wind from the right";

/// type-02 and invented line results at T2's points A1 (set 1) and A11
/// (set 11), in a folder of its own.
fn placed(tag: &str) -> (PathBuf, PathBuf) {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/synthetic-line/Structures/type-02.ohls");
    let dir =
        std::env::temp_dir().join(format!("ohl-structure-editor-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("type-02.ohls");
    std::fs::copy(&from, &path).unwrap();
    let mut values = LineValues::default();
    for (label, set) in [("A1", 1), ("A11", 11)] {
        values.points.push(LinePoint {
            label: label.into(),
            sets: vec![set],
            phases: vec![],
        });
        for (case, t) in [(LEFT, 3000.0), (RIGHT, -3000.0)] {
            for (q, v) in [
                (LineQuantity::Transverse, t),
                (LineQuantity::Longitudinal, 100.0),
                (LineQuantity::Vertical, 6000.0),
            ] {
                values.values.push(LineValue {
                    case: case.into(),
                    point: Some(label.into()),
                    set: None,
                    quantity: q,
                    value: v,
                });
            }
        }
    }
    values.structure_wind = [(LEFT, Direction::Right), (RIGHT, Direction::Left)]
        .map(|(case, towards)| CaseWind {
            case: case.into(),
            pressure: 500.0,
            factor: 1.0,
            towards,
            origin: String::new(),
        })
        .to_vec();
    let at = line_values_path(&path, "T2");
    let mut v = PlacedValues::pending("T2", "test line", "type-02", "");
    v.note = None;
    v.cases = vec![LEFT.into(), RIGHT.into()];
    v.values = values;
    v.save(&at).unwrap();
    (path, at)
}

fn harness(path: &Path, values: &Path) -> Harness<'static, StructureApp> {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 950.0));
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

/// Saves the window as `name` in `OHL_RENDER_PNG` (a GPU rendering).
fn shot(h: &mut Harness<'static, StructureApp>, name: &str) {
    if ohl_render::headless_device().is_ok()
        && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
    {
        let image = h.render().unwrap();
        image.save(Path::new(&dir).join(name)).unwrap();
    }
}

fn last_case(h: &Harness<'static, StructureApp>) -> ohl_structure_loads::LoadCaseRecipe {
    let f = h.state().file.as_ref().unwrap();
    ohl_structure_loads::StructureLoadCases::from_structure(f.structure())
        .unwrap()
        .pop()
        .unwrap()
}

/// A case added, its line forces added (the first line result), a point
/// unticked (its forces left out, the menu still open), each one undo
/// step; Mirror wind makes the case of the other side.
#[test]
fn line_forces_are_added_a_point_unticked_and_the_wind_mirrored() {
    let (path, at) = placed("parts");
    let mut h = harness(&path, &at);
    menu(&mut h, "Load cases", "Load case editor…");
    press(&mut h, "+ Case");
    press(&mut h, "Add line forces");
    let c = last_case(&h);
    assert_eq!(c.name, "case 1");
    assert_eq!(c.line.len(), 1);
    assert_eq!(c.line[0].result, LEFT);
    assert_eq!(
        h.state().file.as_ref().unwrap().undo_name(),
        Some("add line forces of case 1")
    );
    // The sentence and the rows.
    h.get_by_label_contains(
        "storm — wind from the left: vertical × 1.0, transverse × 1.0, longitudinal × 1.0 at \
         every point",
    );
    let f = h.state().file.clone().unwrap();
    h.state_mut().loads.refresh(&f);
    let row = |h: &Harness<'static, StructureApp>, label: &str| {
        h.state()
            .loads
            .table()
            .rows
            .iter()
            .find(|r| r.case == "case 1" && r.load == Some(1001) && r.label == label)
            .cloned()
            .unwrap()
    };
    let a11 = row(&h, "A11");
    assert_eq!(
        (a11.force, a11.source),
        ([3000.0, 100.0, 6000.0], Source::Line)
    );
    assert!(
        h.state()
            .file
            .as_ref()
            .unwrap()
            .text()
            .contains("version = 7")
    );

    // A11 unticked: its line forces left out; the menu stays open.
    press(&mut h, "Points (0 off)");
    press(&mut h, "A11 · set 11");
    assert_eq!(last_case(&h).line[0].off, ["A11"]);
    h.get_by_label("A1 · set 1");
    let f = h.state().file.clone().unwrap();
    h.state_mut().loads.refresh(&f);
    let a11 = row(&h, "A11");
    assert_eq!(a11.force, [0.0; 3]);
    assert!(a11.note.starts_with("off"), "{}", a11.note);
    assert_eq!(
        h.state().file.as_ref().unwrap().undo_name(),
        Some("A11's line forces off of case 1")
    );
    h.get_by_label_contains("(without A11)");
    // Closed with a click outside it.
    h.key_press(egui::Key::Escape);
    h.run();

    // Mirror wind: the case from the right, one undo step.
    let before = h.state().file.as_ref().unwrap().text();
    press(&mut h, "Mirror wind");
    let m = last_case(&h);
    assert_eq!(m.name, "case 1 (mirrored)");
    assert_eq!(m.line[0].result, RIGHT);
    assert_eq!(m.line[0].off, ["A11"]);
    h.state_mut().undo();
    h.run();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The example's case of the two parts mirrored from the menu's case:
/// its name, line result and body wind turned to the right.
#[test]
fn mirror_wind_turns_the_body_wind_too() {
    let (path, at) = placed("mirror");
    let mut h = harness(&path, &at);
    menu(&mut h, "Load cases", "Load case editor…");
    let k = h
        .state()
        .loads
        .recipes()
        .unwrap()
        .iter()
        .position(|c| c.name == "high wind, from the left")
        .unwrap();
    h.state_mut().loads.recipe = k;
    h.run();
    shot(&mut h, "load-case-editor.png");
    press(&mut h, "Mirror wind");
    let m = last_case(&h);
    assert_eq!(m.name, "high wind, from the right");
    assert_eq!(m.line[0].result, "high wind — wind from the right");
    let w = &m.structure.as_ref().unwrap().wind[0];
    assert_eq!(w.from, ohl_structure_loads::WindFrom::Right);
    assert_eq!(
        w.pressure_of.as_deref(),
        Some("high wind — wind from the right")
    );
    // Mirrored again: the name is there already, nothing added.
    let n = h.state().loads.recipes().unwrap().len();
    let k = h
        .state()
        .loads
        .recipes()
        .unwrap()
        .iter()
        .position(|c| c.name == "high wind, from the left")
        .unwrap();
    h.state_mut().loads.recipe = k;
    h.run();
    press(&mut h, "Mirror wind");
    assert_eq!(h.state().loads.recipes().unwrap().len(), n);
    assert!(h.state().message.as_ref().unwrap().1);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
