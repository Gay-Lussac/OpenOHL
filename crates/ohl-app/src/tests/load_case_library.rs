// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Load cases, step 5 (owner 2026-11-03): the Load cases library as a
//! form, the per-pylon sets, the structure loads with ice, snow and the
//! cases' notes, and one rule for the local conditions. Every value here
//! is INVENTED (the synthetic example line).

use super::*;
use crate::library::LibraryKind;
use egui_kittest::kittest::NodeT;

fn app(model: model::Model) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1100.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h
}

/// The form: a new case, its weather picked from the project's list, S1's
/// second span broken, S2 removed, the wind on the structures from 30°;
/// saved, the file reads back at version 4 with those values; one undo
/// puts the file back as it was.
#[test]
fn the_load_case_form_writes_a_version_4_case_and_undo_restores_it() {
    let mut h = app(synthetic_project("lc-form"));
    h.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Library(LibraryKind::LoadCases), None));
    h.run();
    h.run();
    let (path, before) = {
        let r = h.state_mut().ready().unwrap();
        let path = r.libraries[&LibraryKind::LoadCases].file.clone().unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        (path, before)
    };
    h.get_by_label("Add").click();
    h.run();
    assert!(h.query_all_by_label("new case-1").next().is_some());
    // 1 Weather: the project's weather cases only.
    h.get_all_by_label("Weather case")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .unwrap()
        .click();
    h.run();
    h.get_by_label("high wind").click();
    h.run();
    // 2 Wind on the wires: there, the weather case has wind.
    h.get_by_label_contains("as the weather case (from the");
    // 3 Wires removed and broken.
    h.get_by_label_contains("S1 span 2 (").click();
    h.run();
    h.get_by_label("S2 removed (not strung)").click();
    h.run();
    // 4 Wind on the structures from 30°.
    h.get_by_label("at an angle").scroll_to_me();
    h.run();
    h.get_by_label("at an angle").click();
    h.run();
    let angle = h.get_by_role_and_label(egui::accesskit::Role::TextInput, "angle °");
    angle.focus();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "angle °")
        .type_text("30");
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let page = r.libraries.get_mut(&LibraryKind::LoadCases).unwrap();
        assert!(page.dirty());
        assert!(page.error.is_none(), "{:?}", page.error);
    }
    h.get_by_label("Save").click();
    h.run();
    let read = ohl_loads::LoadCases::load(&path).unwrap();
    assert_eq!(read.version, Some(4));
    let c = read.cases.iter().find(|c| c.name == "new case-1").unwrap();
    assert_eq!(c.weather, "high wind");
    let broken: Vec<String> = c.broken_wires().map(|b| b.describe()).collect();
    assert_eq!(broken, ["S1 span 2 broken"]);
    assert_eq!(c.removed.len(), 1);
    assert_eq!(c.removed[0].section, "S2");
    assert_eq!(
        c.on_structure.direction,
        Some(ohl_loads::WindDirection::Angle(30.0))
    );
    // The other cases as they were.
    let old = ohl_loads::LoadCases::parse(&before).unwrap();
    for o in &old.cases {
        assert_eq!(read.cases.iter().find(|c| c.name == o.name), Some(o));
    }
    // One undo: the file as before.
    let r = h.state_mut().ready().unwrap();
    r.model.undo().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

/// A section with a broken span cannot be ticked removed too (a removed
/// wire is not there to break): its tick is greyed, the case stays
/// valid.
#[test]
fn a_broken_section_cannot_be_removed_too() {
    let mut h = app(synthetic_project("lc-form-error"));
    h.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Library(LibraryKind::LoadCases), None));
    h.run();
    h.run();
    h.get_by_label_contains("S1 span 1 (").click();
    h.run();
    assert!(
        h.get_by_label("S1 removed (not strung)")
            .accesskit_node()
            .is_disabled()
    );
    let r = h.state_mut().ready().unwrap();
    let page = r.libraries.get_mut(&LibraryKind::LoadCases).unwrap();
    assert!(page.error.is_none(), "{:?}", page.error);
    assert!(page.text().unwrap().contains("[case.broken]"));
}

/// A structure's Load cases in its Properties: "its own" ticks every case
/// of the library, one unticked; Apply to the 2 other selected gives
/// them the same set (one command, one undo); the Structures table shows
/// the count and the names.
#[test]
fn per_pylon_sets_are_ticked_and_applied_to_the_selection() {
    let mut h = app(synthetic_project("lc-sets"));
    {
        let r = h.state_mut().ready().unwrap();
        r.select(Selection::Structure("T2".into()));
    }
    h.run();
    h.get_by_label_contains("as its type (every case)");
    h.get_by_label("its own").click();
    h.run();
    let set = |h: &mut Harness<'_, App>, id: &str| {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .structure(id)
            .unwrap()
            .load_cases
            .clone()
    };
    assert_eq!(set(&mut h, "T2"), ["everyday", "wind", "ice"]);
    // Untick "wind" (the checkbox in its Properties).
    let wind = h
        .get_all_by_label("wind")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::CheckBox)
        .unwrap();
    wind.click();
    h.run();
    assert_eq!(set(&mut h, "T2"), ["everyday", "ice"]);
    {
        let r = h.state_mut().ready().unwrap();
        r.model.selected_also = vec![
            Selection::Structure("T3".into()),
            Selection::Structure("T4".into()),
        ];
    }
    h.run();
    h.get_by_label("Apply to the 2 other selected").click();
    h.run();
    assert_eq!(set(&mut h, "T3"), ["everyday", "ice"]);
    assert_eq!(set(&mut h, "T4"), ["everyday", "ice"]);
    {
        let r = h.state_mut().ready().unwrap();
        let rows = crate::tables::TableKind::Structures.rows(&r.model, r.case);
        let row = rows.iter().find(|x| x.key == "T3").unwrap();
        assert_eq!(
            row.cells[crate::tables::TableKind::Structures.column("load cases")],
            "2: everyday, ice"
        );
        let t1 = rows.iter().find(|x| x.key == "T1").unwrap();
        assert_eq!(
            t1.cells[crate::tables::TableKind::Structures.column("load cases")],
            "every case"
        );
        // One undo: T3 and T4 as before, T2 keeps its own.
        r.model.undo().unwrap();
    }
    assert!(set(&mut h, "T3").is_empty() && set(&mut h, "T4").is_empty());
    assert_eq!(set(&mut h, "T2"), ["everyday", "ice"]);
}

/// Line › Criteria › Load cases per structure type: a tick gives the type
/// its default set (one command); its structures take it, one with its
/// own set keeps it; undo takes it back.
#[test]
fn a_structure_types_default_set_is_ticked() {
    let mut model = typed_project("lc-type-sets");
    model.sources.load_cases = Some(root().join("examples/synthetic-line/load-cases.ohllc"));
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 700.0))
        .build_ui_state(
            |ui, m: &mut model::Model| {
                if let Some((what, command)) = crate::load_case_sets::types_ui(ui, m) {
                    m.apply(&what, command).unwrap();
                }
            },
            model,
        );
    h.run();
    h.get_by_label_contains("type-02 — every case").click();
    h.run();
    h.get_by_label("type-02: wind").click();
    h.run();
    let m = h.state_mut();
    assert_eq!(
        m.line.structure_loads.type_load_cases.get("type-02"),
        Some(&vec!["wind".to_owned()])
    );
    let t2 = m.line.structure("T2").unwrap().clone();
    assert_eq!(m.line.load_cases_for(&t2), ["wind"]);
    assert!(m.line.takes_load_case("T1", "ice"));
    m.undo().unwrap();
    assert!(m.line.structure_loads.type_load_cases.is_empty());
}

fn write_cases(model: &mut model::Model, text: &str) {
    let path = model.sources.file.with_file_name("cases.ohllc");
    std::fs::write(&path, text).unwrap();
    model.sources.load_cases = Some(path);
}

fn inputs(model: &model::Model, local: bool) -> crate::loads::Inputs {
    crate::loads::Inputs {
        line: model.line.clone(),
        library: model.library.clone(),
        types: model
            .types
            .as_ref()
            .map(|(t, _)| t.clone())
            .unwrap_or_default(),
        weather: model.cases.iter().map(|c| c.case.clone()).collect(),
        sections: model.section_library(),
        cases: ohl_loads::LoadCases::load(model.sources.load_cases.as_deref().unwrap())
            .map(|s| s.cases),
        local,
        scope: None,
        structures: None,
    }
}

/// One rule for the local conditions in Checks › Structure loads: T3
/// (wind bands) of wind_static 1.5 gets exactly 1.5 × its band wind with
/// the window's switch ticked, not 2.25 ×; the case says it took the
/// local conditions.
#[test]
fn the_structure_loads_switch_takes_wind_static_once() {
    let mut model = typed_project("lc-local-once");
    model.sources.load_cases = Some(root().join("examples/synthetic-line/load-cases.ohllc"));
    model
        .line
        .structures
        .iter_mut()
        .find(|s| s.id == "T3")
        .unwrap()
        .local
        .wind_static = Some(1.5);
    let band = |r: &[ohl_loads::CaseLoads]| -> f64 {
        r.iter()
            .find(|c| c.case.name == "wind")
            .unwrap()
            .loads
            .iter()
            .filter(|l| l.structure == "T3" && l.label.starts_with("wind on the structure"))
            .map(|l| l.factored[0])
            .sum()
    };
    let (plain, _) = crate::loads::compute(&inputs(&model, false));
    let (local, _) = crate::loads::compute(&inputs(&model, true));
    let (plain, local) = (plain.unwrap(), local.unwrap());
    let (a, b) = (band(&plain), band(&local));
    assert!(a.abs() > 0.0);
    assert!((b / a - 1.5).abs() < 1e-12, "{b} / {a}");
    let wind = local.iter().find(|c| c.case.name == "wind").unwrap();
    assert!(wind.case.use_local);
    assert!(wind.notes.iter().any(|n| n.contains("local conditions")));
}

/// Checks › Structure loads with a case putting ice on the structures:
/// the pole (T6) gets its row "ice on the structure", the window shows it
/// and the case's notes (ice and snow, what was not counted where).
#[test]
fn the_loads_window_shows_ice_rows_and_the_cases_notes() {
    let mut model = typed_project("lc-ice-rows");
    write_cases(
        &mut model,
        "format = \"ohl-load-cases\"\nversion = 4\n\
         [[case]]\nname = \"iced towers\"\nweather = \"glaze ice\"\nvertical = 1.3\n\
         [case.structure]\nice = \"20 mm\"\nice_density = \"900 kg/m³\"\nsnow = true\n",
    );
    let (r, _) = crate::loads::compute(&inputs(&model, false));
    let r = r.unwrap();
    let ice: Vec<&ohl_loads::AttachmentLoad> = r[0]
        .loads
        .iter()
        .filter(|l| l.label == "ice on the structure")
        .collect();
    assert!(ice.iter().any(|l| l.structure == "T6"), "{:?}", r[0].notes);
    assert!(ice.iter().all(|l| l.factored[2] > 0.0));
    let mut page = crate::loads::LoadsPage::default();
    page.results = Some(Ok(r));
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(
            move |ui, m: &mut model::Model| {
                page.body_ui(ui, m);
            },
            model,
        );
    h.run();
    h.run();
    assert!(
        h.query_all_by_label_contains("ice on the structure")
            .next()
            .is_some()
    );
    assert!(
        h.query_all_by_label_contains("• ice and snow on the structures")
            .next()
            .is_some()
    );
    h.get_by_label_contains("no snow on the ground given");
}
