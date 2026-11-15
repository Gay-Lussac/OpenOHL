// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Load cases, step 5 (owner 2026-11-03): the Load cases library as a
//! form, the structure groups (2026-11-15, the per-pylon sets before), the
//! structure loads with ice, snow and the cases' notes, and one rule for
//! the local conditions. Every value here
//! is INVENTED (the synthetic example line).

use super::session_edits::press;
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

/// The groups of the line (in memory) and their cases.
fn groups(h: &mut Harness<'_, App>) -> Vec<ohl_line::StructureGroup> {
    h.state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure_groups
        .clone()
}

/// The Structures table's load cases cell of `id`.
fn cases_cell(h: &mut Harness<'_, App>, id: &str) -> String {
    let r = h.state_mut().ready().unwrap();
    let rows = crate::tables::TableKind::Structures.rows(&r.model, r.case);
    let row = rows.iter().find(|x| x.key == id).unwrap();
    row.cells[crate::tables::TableKind::Structures.column("load cases")].clone()
}

/// Line › Structures › Load cases per group (owner, 2026-11-15): New
/// group, renamed, the two structures selected added, a case ticked, a
/// type rule and the dead-end rule set — each change one command (one
/// undo each); the Structures table shows the groups and cases, a
/// structure in no group every case; Who is in lists the members.
#[test]
fn the_groups_window_makes_a_group_one_command_per_change() {
    let mut model = typed_project("lc-groups-window");
    model.sources.load_cases = Some(root().join("examples/synthetic-line/load-cases.ohllc"));
    let mut h = app(model);
    {
        let r = h.state_mut().ready().unwrap();
        r.select(Selection::Structure("T2".into()));
        r.model.selected_also = vec![Selection::Structure("T6".into())];
        r.structure_groups.open = true;
    }
    h.run();
    h.get_by_label_contains("No group yet");
    assert_eq!(cases_cell(&mut h, "T2"), crate::load_case_sets::NO_GROUP);
    h.get_by_label("New group").click();
    h.run();
    assert_eq!(groups(&mut h).len(), 1);
    assert_eq!(groups(&mut h)[0].name, "group 1");
    // Renamed: the text taken on Enter.
    let name = || egui::accesskit::Role::TextInput;
    h.get_by_role_and_label(name(), "name").focus();
    h.run();
    press(&mut h, egui::Key::A, egui::Modifiers::COMMAND);
    h.get_by_role_and_label(name(), "name")
        .type_text("suspension");
    h.run();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    h.run();
    assert_eq!(groups(&mut h)[0].name, "suspension");
    // The two selected picked.
    h.get_by_label("Add the 2 selected").click();
    h.run();
    assert_eq!(groups(&mut h)[0].structures, ["T2", "T6"]);
    // No case ticked yet: T2 takes none, said.
    assert!(cases_cell(&mut h, "T2").starts_with(crate::load_case_sets::NO_CASE));
    h.get_by_label_contains("None ticked: its structures take no load case");
    let wind = || egui::accesskit::Role::CheckBox;
    h.get_by_role_and_label(wind(), "wind").scroll_to_me();
    h.run();
    h.get_by_role_and_label(wind(), "wind").click();
    h.run();
    assert_eq!(groups(&mut h)[0].load_cases, ["wind"]);
    assert_eq!(cases_cell(&mut h, "T2"), "suspension: 1: wind");
    assert_eq!(cases_cell(&mut h, "T1"), crate::load_case_sets::NO_GROUP);
    // A rule: type-03 (T3), and the dead-end rule.
    h.get_by_label("type type-03").scroll_to_me();
    h.run();
    h.get_by_label("type type-03").click();
    h.run();
    assert_eq!(groups(&mut h)[0].rules.types, ["type-03"]);
    h.get_by_label("a dead end").scroll_to_me();
    h.run();
    h.get_by_label("a dead end").click();
    h.run();
    assert_eq!(groups(&mut h)[0].rules.dead_end, Some(true));
    // T3 (type-03, where S3 ends) in by its rules; T2 and T6 picked.
    {
        let r = h.state_mut().ready().unwrap();
        let sets = r.model.line.case_sets();
        assert_eq!(sets.members(0), ["T2", "T3", "T6"]);
        assert!(sets.takes("T3", "wind") && !sets.takes("T3", "ice"));
        assert!(sets.takes("T1", "ice"));
    }
    h.get_by_label_contains("3 structure(s) in \"suspension\"")
        .scroll_to_me();
    // One undo per change: the dead-end rule, then the type.
    h.state_mut().ready().unwrap().model.undo().unwrap();
    h.run();
    assert_eq!(groups(&mut h)[0].rules.dead_end, None);
    assert_eq!(groups(&mut h)[0].rules.types, ["type-03"]);
    h.state_mut().ready().unwrap().model.undo().unwrap();
    h.run();
    assert!(groups(&mut h)[0].rules.types.is_empty());
    assert_eq!(groups(&mut h)[0].load_cases, ["wind"]);
    // Kept in the project: the session's line has it.
    let r = h.state_mut().ready().unwrap();
    let stored = r.model.stored_line().unwrap().structure_groups.clone();
    assert_eq!(stored, r.model.line.structure_groups);
}

/// A group ticking no load case (2026-11-15, the former "none"): its
/// structures take none — the Structures table and the Properties say so;
/// one undo gives the case back.
#[test]
fn a_group_ticking_no_case_gives_its_structures_none() {
    let mut h = app(synthetic_project_with("lc-none", |l| {
        let mut g = ohl_line::StructureGroup::new("dismantled");
        g.structures = vec!["T2".into()];
        g.load_cases = vec!["wind".into()];
        l.structure_groups.push(g);
    }));
    {
        let r = h.state_mut().ready().unwrap();
        r.select(Selection::Structure("T2".into()));
        r.structure_groups.open = true;
    }
    h.run();
    assert_eq!(cases_cell(&mut h, "T2"), "dismantled: 1: wind");
    h.get_by_label("None").click();
    h.run();
    {
        let m = &h.state_mut().ready().unwrap().model;
        assert!(m.line.structure_groups[0].load_cases.is_empty());
        assert!(!m.line.takes_load_case("T2", "wind"));
        assert!(m.line.takes_no_load_case("T2"));
        assert!(m.line.takes_load_case("T1", "wind"));
    }
    assert_eq!(
        cases_cell(&mut h, "T2"),
        format!("{} (dismantled)", crate::load_case_sets::NO_CASE)
    );
    // The Properties: shown only.
    h.get_by_label_contains("they tick no case — it takes none");
    h.state_mut().ready().unwrap().model.undo().unwrap();
    h.run();
    assert_eq!(cases_cell(&mut h, "T2"), "dismantled: 1: wind");
}

/// The window follows the check pages' guidelines: every section its
/// **user guide ↗** buttons open is a heading of its own page (mdBook's
/// anchors: lower case, spaces as `-`, punctuation dropped).
#[test]
fn each_groups_guide_button_opens_a_heading_of_the_page() {
    use crate::structure_groups::{anchors, guide};
    let text =
        std::fs::read_to_string(root().join("docs/user-guide/load-cases-per-group.md")).unwrap();
    let headings: Vec<String> = text
        .lines()
        .filter_map(|l| l.strip_prefix('#'))
        .map(|h| {
            h.trim_start_matches('#')
                .trim()
                .to_lowercase()
                .chars()
                .filter_map(|c| match c {
                    ' ' => Some('-'),
                    c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                    _ => None,
                })
                .collect()
        })
        .collect();
    for a in [
        anchors::HOW,
        anchors::GROUPS,
        anchors::NAME,
        anchors::PICKED,
        anchors::RULES,
        anchors::CASES,
        anchors::WHO,
    ] {
        let (page, s) = guide(a);
        assert_eq!(page, "user-guide/load-cases-per-group");
        assert!(headings.iter().any(|x| x == s), "{s} not in {headings:?}");
    }
}

/// The window's first **user guide ↗** (1 How it works) opens its own
/// page at that section.
#[test]
fn the_groups_user_guide_button_opens_the_window_page() {
    let mut h = app(synthetic_project("lc-groups-guide"));
    h.state_mut().ready().unwrap().structure_groups.open = true;
    h.run();
    h.query_all_by_label("user guide ↗")
        .next()
        .expect("a user guide button")
        .click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    let (said, error) = r.status.clone().expect("what was opened");
    assert!(!error, "{said}");
    assert!(said.contains("load-cases-per-group"), "{said}");
    assert!(
        said.contains(crate::structure_groups::anchors::HOW),
        "{said}"
    );
}

/// The load cases are under Line › Structures (2026-11-15): Load cases…
/// opens the library, Load cases per group… the window; Line › Criteria
/// has neither, nor the old Loads for structures menu.
#[test]
fn the_load_cases_are_in_the_structures_menu_and_loads_for_structures_is_gone() {
    let mut h = app(synthetic_project("lc-menus"));
    crate::tests::line_menu(&mut h, "Criteria");
    assert!(h.query_by_label_contains("Loads for structures").is_none());
    assert!(
        h.query_by_label_contains("Load cases per structure type")
            .is_none()
    );
    assert!(h.query_by_label("Load cases library").is_none());
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    crate::tests::line_menu(&mut h, "Structures");
    h.get_by_label("Load cases per group…").click();
    h.run();
    assert!(h.state_mut().ready().unwrap().structure_groups.open);
    crate::tests::line_menu(&mut h, "Structures");
    h.get_by_label("Load cases…").click();
    h.run();
    h.run();
    let tab = crate::app::Tab::Library(LibraryKind::LoadCases);
    assert!(
        h.state_mut().dock().find_tab(&tab).is_some(),
        "the library's page"
    );
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
        known: crate::load_case_sets::library_names(model).unwrap_or_default(),
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

/// A case renamed in the Load cases library (2026-11-14: a rename must not
/// drop structures out of the checks): Rename › OK renames it in the file
/// (its comments kept) and in every structure group ticking it — one
/// command, one undo puts all back. Deleted from the library, a group
/// still ticking it is said in the Structures table (error colour), not
/// dropped.
#[test]
fn a_renamed_load_case_follows_into_the_groups_and_undoes_as_one() {
    let mut h = app(synthetic_project_with("lc-rename", |l| {
        let mut a = ohl_line::StructureGroup::new("T2");
        a.structures = vec!["T2".into()];
        a.load_cases = vec!["everyday".into(), "wind".into()];
        let mut b = ohl_line::StructureGroup::new("dead ends");
        b.rules.dead_end = Some(true);
        b.load_cases = vec!["everyday".into()];
        l.structure_groups = vec![a, b];
    }));
    h.state_mut()
        .ready()
        .unwrap()
        .model
        .store_libraries()
        .unwrap();
    h.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Library(LibraryKind::LoadCases), None));
    h.run();
    h.run();
    // The first case is picked: "everyday".
    h.get_by_label("Rename").click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "new name")
        .focus();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "new name")
        .type_text(" SLS");
    h.run();
    h.get_by_label("OK").click();
    h.run();
    h.run();
    let state = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let line = &r.model.line;
        let text = std::fs::read_to_string(r.model.sources.load_cases.clone().unwrap()).unwrap();
        (
            line.structure_groups[0].load_cases.clone(),
            line.structure_groups[1].load_cases.clone(),
            text,
        )
    };
    let (own, ends, text) = state(&mut h);
    assert_eq!(own, ["everyday SLS", "wind"]);
    assert_eq!(ends, ["everyday SLS"]);
    assert!(text.contains("name = \"everyday SLS\""), "{text}");
    assert!(
        text.contains("weather = \"everyday\""),
        "the weather case kept: {text}"
    );
    assert!(
        text.starts_with("# Synthetic example"),
        "comments kept: {text}"
    );
    assert!(
        h.state_mut()
            .ready()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.0.contains("also in the group(s) T2, dead ends")),
    );
    // One undo: everything back.
    h.state_mut().ready().unwrap().model.undo().unwrap();
    h.run();
    let (own, ends, text) = state(&mut h);
    assert_eq!(own, ["everyday", "wind"]);
    assert_eq!(ends, ["everyday"]);
    assert!(text.contains("name = \"everyday\""), "{text}");
    // A group ticking a case the library has not: said in the Structures
    // table, never silently dropped.
    let r = h.state_mut().ready().unwrap();
    let sets = r.model.line.case_sets();
    let s = r.model.line.structure("T2").unwrap().clone();
    let known = vec!["wind".to_owned(), "ice".to_owned()];
    let cell = crate::load_case_sets::cell(&sets, &s, Some(&known));
    assert!(
        cell.starts_with(crate::load_case_sets::UNKNOWN_MARK)
            && cell.contains("everyday not in the library"),
        "{cell}"
    );
    let t1 = r.model.line.structure("T1").unwrap().clone();
    let all = vec!["wind".to_owned()];
    assert!(crate::load_case_sets::cell(&sets, &t1, Some(&all)).contains("NOT CHECKED"));
}

/// **Use local conditions** of Checks › Structure loads is kept in the
/// project (2026-11-14): what OHL-Structure gets follows it, so it must
/// outlive the window; the cases sent to the placed structures then take
/// the local conditions, as the line's check.
#[test]
fn the_local_switch_is_kept_and_reaches_the_placed_structures() {
    let mut h = app(synthetic_project("lc-local-kept"));
    h.state_mut().ready().unwrap().loads.local = true;
    h.run();
    let r = h.state_mut().ready().unwrap();
    let session = r.model.session().unwrap();
    let key = format!("{}.{}", crate::loads::SETTING, session.current_line());
    let kept = session.setting(&key).unwrap().unwrap();
    assert!(kept.contains("\"local\":true"), "{kept}");
    let loads = crate::structure_app::LineInputs::of_with_local(&r.model, true)
        .compute()
        .unwrap();
    assert!(loads.local);
    assert!(loads.library.iter().all(|c| c.case.use_local));
    let plain = crate::structure_app::LineInputs::of(&r.model)
        .compute()
        .unwrap();
    assert!(!plain.local);
    assert!(plain.library.iter().all(|c| !c.case.use_local));
}
