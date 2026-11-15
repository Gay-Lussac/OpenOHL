// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Structure loads and the Load cases library (owner reports
//! 2026-11-15: "i added the default structure loads in the library and it
//! does not appear in the compute structure loads"; "when the load cases
//! in the library are edited and saved, the compute load case doesn't
//! update the load case library"; "if i restart the project it's good";
//! "the compute structure loads starts immediately when the window is
//! opened. i would like to have to click on run to start it"). The
//! window follows the library as saved, lists every case with its state,
//! runs the groups chosen, and computes only on Run. Every value here is
//! INVENTED (the synthetic example line).

use super::plugins_hosting::run_until;
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

/// Checks › Compute structure loads.
fn open_window(h: &mut Harness<'_, App>) {
    h.get_by_label("Checks").click();
    h.run();
    h.get_by_label("Compute structure loads").click();
    h.run();
    h.run();
}

/// The window closed (it floats over the library page otherwise).
fn close_window(h: &mut Harness<'_, App>) {
    h.state_mut().ready().unwrap().loads.open = false;
    h.run();
}

/// The Load cases library page.
fn open_library(h: &mut Harness<'_, App>) {
    h.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Library(LibraryKind::LoadCases), None));
    h.run();
    h.run();
}

/// Run in the window, until its answer is taken.
fn run(h: &mut Harness<'_, App>) {
    run_in(h, "Structure loads").click();
    h.step();
    run_until(h, |a| {
        let r = a.ready().unwrap();
        r.loads.poll();
        !r.loads.running()
    });
    h.run();
    h.run();
}

/// The window's Setup tab (a new answer opens Results).
fn setup_tab(h: &mut Harness<'_, App>) {
    let w = h.get_by_label("Structure loads").rect();
    h.get_all_by_label("Setup")
        .find(|n| w.contains_rect(n.rect()))
        .expect("the Setup tab")
        .click();
    h.run();
}

/// Whether a node whose label contains `label` is inside the Structure
/// loads window.
fn in_window(h: &Harness<'_, App>, label: &str) -> bool {
    let w = h.get_by_label("Structure loads").rect();
    h.query_all_by_label_contains(label)
        .any(|n| w.contains_rect(n.rect()))
}

/// The results' load cases, by name.
fn computed(h: &mut Harness<'_, App>) -> Vec<ohl_loads::CaseLoads> {
    let r = h.state_mut().ready().unwrap();
    match &r.loads.results {
        Some(Ok(v)) => v.clone(),
        other => panic!("no results: {:?}", other.as_ref().map(|r| r.as_ref().err())),
    }
}

/// The library's case names, as the file has them.
fn library_names(h: &mut Harness<'_, App>) -> Vec<String> {
    let r = h.state_mut().ready().unwrap();
    crate::load_case_sets::library_names(&r.model).unwrap()
}

/// Opening the window computes nothing; Run does (owner 2026-11-15).
#[test]
fn opening_the_window_starts_nothing_and_run_does() {
    let mut h = app(synthetic_project("loads-window-run"));
    open_window(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.loads.open);
        assert_eq!(r.loads.runs_started, 0, "nothing started on open");
        assert!(!r.loads.running() && r.loads.results.is_none());
    }
    // A few frames later: still nothing.
    h.run();
    h.run();
    assert_eq!(h.state_mut().ready().unwrap().loads.runs_started, 0);
    // The summary before Run (section 5; scrolled below the setup).
    assert!(h.query_all_by_label_contains("Before Run").next().is_some());
    run(&mut h);
    let r = h.state_mut().ready().unwrap();
    assert_eq!(r.loads.runs_started, 1);
    assert!(matches!(r.loads.results, Some(Ok(_))));
}

/// Opened before the library was changed (the owner's order), the window
/// lists the cases as saved after Add the default cases… › Save — no
/// reopening of the project. Run computes the defaults whose weather
/// cases the project has; the others are listed "not computed" with why
/// (never silently left out).
#[test]
fn the_window_lists_the_default_cases_added_and_saved_in_the_library() {
    let mut h = app(synthetic_project("loads-window-defaults"));
    open_window(&mut h);
    assert!(in_window(&h, "3 load cases in the library"));
    close_window(&mut h);
    open_library(&mut h);
    let defaults = ohl_loads::default_load_cases().cases;
    h.get_by_label(crate::default_cases::ADD_BUTTON).click();
    h.run();
    // Without the default weather cases (the tick unticked): only the
    // load cases are added.
    h.get_by_label(crate::load_case_form::ALSO_WEATHER).click();
    h.run();
    h.get_by_label(&format!("Add {} case(s)", defaults.len()))
        .click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    h.run();
    open_window(&mut h);
    let all = 3 + defaults.len();
    assert!(
        in_window(&h, &format!("{all} load cases in the library")),
        "the window follows the library as saved"
    );
    run(&mut h);
    let names: Vec<String> = computed(&mut h)
        .iter()
        .map(|c| c.case.name.clone())
        .collect();
    assert!(names.iter().any(|n| n == "everyday"), "{names:?}");
    // The example's weather cases are not the defaults: those cases are
    // not computed, each said with why and the structures not checked.
    let r = h.state_mut().ready().unwrap();
    let failed = r.loads.not_computed().to_vec();
    let sets = r.model.line.case_sets();
    for d in &defaults {
        // The 60 N/m ice goes only to structures at or above 1000 m: the
        // example's take none (listed so in Setup › 2).
        if sets.takers(&d.name).is_empty() {
            continue;
        }
        let have = r.model.cases.iter().any(|c| c.case.name == d.weather);
        let listed = failed.iter().find(|f| f.case == d.name);
        if have {
            assert!(names.contains(&d.name) || listed.is_some(), "{}", d.name);
        } else {
            let f = listed.unwrap_or_else(|| panic!("{} not said: {failed:?}", d.name));
            assert!(f.why.contains("no weather case"), "{}", f.why);
        }
    }
    assert!(in_window(&h, "not computed: -20 °C"));
}

/// Add the default cases… with its tick (the default) adds the default
/// weather cases the new load cases name too, as its own change: after
/// Save every default case computes.
#[test]
fn the_default_cases_come_with_their_weather_cases() {
    let mut h = app(synthetic_project("loads-window-weather"));
    open_library(&mut h);
    let defaults = ohl_loads::default_load_cases().cases;
    h.get_by_label(crate::default_cases::ADD_BUTTON).click();
    h.run();
    h.get_by_label(&format!("Add {} case(s)", defaults.len()))
        .click();
    h.run();
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        for d in &defaults {
            assert!(
                r.model.cases.iter().any(|c| c.case.name == d.weather),
                "{} added",
                d.weather
            );
        }
        assert!(
            r.model.cases.iter().any(|c| c.case.name == "everyday"),
            "kept"
        );
    }
    h.get_by_label("Save").click();
    h.run();
    h.run();
    open_window(&mut h);
    run(&mut h);
    let names: Vec<String> = computed(&mut h)
        .iter()
        .map(|c| c.case.set_name().to_owned())
        .collect();
    let sets = h.state_mut().ready().unwrap().model.line.case_sets();
    for d in &defaults {
        let taken = !sets.takers(&d.name).is_empty();
        assert_eq!(
            names.contains(&d.name),
            taken,
            "{} computed: {names:?}",
            d.name
        );
    }
    assert!(
        h.state_mut()
            .ready()
            .unwrap()
            .loads
            .not_computed()
            .is_empty()
    );
}

/// A case's factor edited in the library form and saved: the earlier
/// results are marked out of date, and Run computes with the new factor;
/// a case renamed there is listed under its new name.
#[test]
fn an_edited_and_saved_case_reaches_the_window() {
    let mut h = app(synthetic_project("loads-window-edit"));
    open_window(&mut h);
    run(&mut h);
    let before = computed(&mut h);
    let everyday = |c: &[ohl_loads::CaseLoads]| {
        c.iter()
            .find(|c| c.case.name == "everyday")
            .unwrap()
            .clone()
    };
    assert_eq!(everyday(&before).case.vertical, 1.0);
    close_window(&mut h);
    open_library(&mut h);
    // The first case is picked: "everyday"; its vertical factor 1 → 15.
    let field = || egui::accesskit::Role::TextInput;
    h.get_by_role_and_label(field(), "vertical ×")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(field(), "vertical ×").focus();
    h.run();
    h.get_by_role_and_label(field(), "vertical ×")
        .type_text("5");
    h.run();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    h.run();
    h.get_by_label("Save").click();
    h.run();
    h.run();
    let path = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .sources
        .load_cases
        .clone()
        .unwrap();
    let saved = ohl_loads::LoadCases::load(&path).unwrap();
    let v = saved
        .cases
        .iter()
        .find(|c| c.name == "everyday")
        .unwrap()
        .vertical;
    assert_ne!(v, 1.0, "the factor was edited");
    open_window(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(
            r.loads.out_of_date(&r.model).is_some(),
            "the library changed"
        );
    }
    run(&mut h);
    let after = computed(&mut h);
    assert_eq!(everyday(&after).case.vertical, v);
    // Factored vertical loads scale with the factor (unfactored the same).
    let (a, b) = (everyday(&before), everyday(&after));
    let (la, lb) = (&a.loads[0], &b.loads[0]);
    assert_eq!(la.local, lb.local);
    assert!((lb.factored[2] - la.factored[2] * v).abs() <= 1e-9 * lb.factored[2].abs().max(1.0));
    // Renamed in the library: listed by its new name.
    close_window(&mut h);
    open_library(&mut h);
    h.get_by_label("Rename").click();
    h.run();
    h.get_by_role_and_label(field(), "new name").focus();
    h.run();
    h.get_by_role_and_label(field(), "new name")
        .type_text(" SLS");
    h.run();
    h.get_by_label("OK").click();
    h.run();
    h.run();
    assert!(library_names(&mut h).contains(&"everyday SLS".to_owned()));
    open_window(&mut h);
    run(&mut h);
    let names: Vec<String> = computed(&mut h)
        .iter()
        .map(|c| c.case.name.clone())
        .collect();
    assert!(names.contains(&"everyday SLS".to_owned()), "{names:?}");
    assert!(!names.contains(&"everyday".to_owned()), "{names:?}");
}

/// The structure groups in the window: listed with the cases they give;
/// a group unticked leaves out the structures only it holds; the cases
/// ticked for the structures in no group change what they are computed
/// for; every structure computed has exactly the cases the groups model
/// gives it (none silently missing).
#[test]
fn the_window_runs_the_groups_chosen() {
    let mut h = app(synthetic_project_with("loads-window-groups", |l| {
        let mut a = ohl_line::StructureGroup::new("towers A");
        a.structures = vec!["T2".into(), "T3".into()];
        a.load_cases = vec!["wind".into()];
        l.structure_groups = vec![a];
    }));
    open_window(&mut h);
    assert!(in_window(&h, "towers A"));
    assert!(in_window(&h, "gives wind"));
    run(&mut h);
    // Every structure computed: the cases the groups give it, all of them.
    let check = |h: &mut Harness<'_, App>| {
        let results = computed(h);
        let r = h.state_mut().ready().unwrap();
        let sets = r.model.line.case_sets();
        let aligned = crate::loads_plan::on_alignment(&r.model.line);
        let choice = r.loads.choice.clone();
        for s in &r.model.line.structures {
            for c in &results {
                // Loads, or a wire not solved there (incomplete): in it.
                let has = c.loads.iter().any(|l| l.structure == s.id)
                    || c.missing.iter().any(|m| m.structure == s.id);
                let in_group = !sets.groups_of(&s.id).is_empty();
                let wanted = sets.takes(&s.id, &c.case.name)
                    && choice.ticks(&s.id, aligned.contains(&s.id))
                    && (in_group || crate::tick_list::ticked(&choice.alone_cases, &c.case.name))
                    && sets.groups_of(&s.id).iter().all(|g| choice.runs_group(g));
                assert_eq!(has, wanted, "{} in {}", s.id, c.case.name);
            }
        }
        results
    };
    let all = check(&mut h);
    let t2_cases: Vec<&str> = all
        .iter()
        .filter(|c| c.loads.iter().any(|l| l.structure == "T2"))
        .map(|c| c.case.name.as_str())
        .collect();
    assert_eq!(t2_cases, ["wind"], "T2 takes its group's case only");
    // The group unticked: T2, T3 not computed, said.
    setup_tab(&mut h);
    h.get_by_label("towers A").click();
    h.run();
    assert_eq!(
        h.state_mut().ready().unwrap().loads.choice.groups_off,
        ["towers A"]
    );
    run(&mut h);
    let results = check(&mut h);
    assert!(
        results
            .iter()
            .all(|c| c.loads.iter().all(|l| l.structure != "T2"))
    );
    assert!(in_window(&h, "its groups not run: towers A"));
    // Back on; the structures in no group run in "ice" only.
    {
        let r = h.state_mut().ready().unwrap();
        r.loads.choice.groups_off.clear();
        r.loads.choice.alone_cases = Some(vec!["ice".into()]);
    }
    h.run();
    run(&mut h);
    let results = check(&mut h);
    let names: Vec<&str> = results.iter().map(|c| c.case.name.as_str()).collect();
    assert_eq!(names, ["wind", "ice"], "everyday run by no structure");
    // The setup is kept in the project.
    h.run();
    let r = h.state_mut().ready().unwrap();
    let session = r.model.session().unwrap();
    let key = format!("{}.{}", crate::loads::SETTING, session.current_line());
    let kept = session.setting(&key).unwrap().unwrap();
    assert!(kept.contains("\"alone_cases\":[\"ice\"]"), "{kept}");
}

/// A structure on no alignment is left out by default, listed at the
/// bottom under its heading and said in the results; ticked, it is
/// computed. (The example line without its alignments: every structure
/// on none.)
#[test]
fn structures_on_no_alignment_are_left_out_by_default() {
    let mut h = app(synthetic_project_with("loads-window-aligned", |l| {
        l.alignments.clear();
    }));
    assert!(crate::loads_plan::on_alignment(&h.state_mut().ready().unwrap().model.line).is_empty());
    open_window(&mut h);
    assert!(in_window(
        &h,
        "Not on an alignment (not computed unless ticked)"
    ));
    // Nothing ticked: Run says why.
    assert!(
        h.query_all_by_label_contains("nothing to compute")
            .next()
            .is_some()
    );
    {
        let r = h.state_mut().ready().unwrap();
        r.loads.choice.tick("T2", false, true);
    }
    h.run();
    run(&mut h);
    let results = computed(&mut h);
    assert!(
        results
            .iter()
            .all(|c| c.loads.iter().all(|l| l.structure == "T2"))
    );
    assert!(
        results
            .iter()
            .any(|c| c.loads.iter().any(|l| l.structure == "T2"))
    );
    assert!(
        h.query_all_by_label_contains(crate::loads_plan::OFF_ALIGNMENT)
            .next()
            .is_some()
    );
}

/// Every section the window's **user guide ↗** buttons open is a heading
/// of its own page (mdBook's anchors: lower case, spaces as `-`,
/// punctuation dropped).
#[test]
fn each_loads_guide_button_opens_a_heading_of_the_page() {
    use crate::loads::{anchors, guide};
    let text =
        std::fs::read_to_string(root().join("docs/user-guide/structure-loads-window.md")).unwrap();
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
        anchors::WHAT,
        anchors::CASES,
        anchors::GROUPS,
        anchors::ALONE,
        anchors::BEFORE,
        anchors::RESULTS,
    ] {
        let (page, s) = guide(a);
        assert_eq!(page, "user-guide/structure-loads-window");
        assert!(headings.iter().any(|x| x == s), "{s} not in {headings:?}");
    }
}
