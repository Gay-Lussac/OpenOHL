// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The check windows' one design (owner, 2026-10-27): each docks as a
//! panel and floats again the same way, its lists are made taller by a
//! handle (remembered for the session), and a result clicked draws its
//! spans in its weather case.

use super::*;
use crate::app::Tab;
use crate::dockable::Panel;
use egui_kittest::kittest::NodeT;

fn window() -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(synthetic(), None));
    h.run();
    h
}

/// Thermal rating (a window that could not dock before): 📌 makes it a
/// tab of the main window, ⬈ a window again; its tab closed closes it.
#[test]
fn a_check_window_docks_as_a_panel_and_floats_again() {
    let mut h = window();
    {
        let r = h.state_mut().ready().unwrap();
        r.thermal.show(&r.model);
    }
    h.run();
    let tab = Tab::Tool(Panel::Thermal);
    assert!(h.state_mut().dock().find_tab(&tab).is_none(), "floating");
    h.get_by_label("Dock as a panel").click();
    h.run();
    h.run();
    assert!(h.state_mut().dock().find_tab(&tab).is_some(), "docked");
    assert!(h.state_mut().ready().unwrap().thermal.open);
    // Its content in the panel: Run in its footer.
    h.get_by_label("Run");
    h.get_by_label("Float").click();
    h.run();
    h.run();
    assert!(
        h.state_mut().dock().find_tab(&tab).is_none(),
        "floating again"
    );
    assert!(h.state_mut().ready().unwrap().thermal.open, "still open");
    h.get_by_label("Run");
    // Docked again, its tab closed: the window closes.
    h.get_by_label("Dock as a panel").click();
    h.run();
    h.run();
    let at = h.state_mut().dock().find_tab(&tab).unwrap();
    h.state_mut().dock().remove_tab(at);
    crate::dockable::host(&h.ctx, |x| x.closed.insert(Panel::Thermal));
    h.run();
    h.run();
    assert!(!h.state_mut().ready().unwrap().thermal.open, "closed");
}

/// The rules list of Vertical clearances: its handle dragged down 120 px
/// makes it 120 px taller, kept when the window is closed and opened
/// again (this session).
#[test]
fn a_check_list_is_made_taller() {
    let mut h = window();
    {
        let r = h.state_mut().ready().unwrap();
        r.vertical_clearances.show(&r.model);
    }
    h.run();
    h.run();
    let id = ("Vertical clearances", "rule-rows");
    let height = |h: &Harness<'_, App>| {
        crate::check_help::list_height(&h.ctx, id, crate::check_help::LIST_HEIGHT)
    };
    assert_eq!(height(&h), crate::check_help::LIST_HEIGHT);
    let handle = h
        .query_all_by_label(crate::check_help::LIST_HANDLE)
        .next()
        .expect("a handle under the rules")
        .rect();
    let from = handle.center();
    let to = from + egui::vec2(0.0, 120.0);
    let events = [
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    h.input_mut().events.extend(events);
    h.step();
    for k in 1..=6 {
        let at = from + egui::vec2(0.0, 20.0 * k as f32);
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.step();
    }
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    let taller = height(&h);
    assert!(
        (taller - (crate::check_help::LIST_HEIGHT + 120.0)).abs() < 1.0,
        "{taller}"
    );
    // Closed and opened again: as tall.
    h.state_mut().ready().unwrap().vertical_clearances.open = false;
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        r.vertical_clearances.show(&r.model);
    }
    h.run();
    assert_eq!(height(&h), taller);
}

/// A Distance between conductors result clicked: the pair's sections
/// drawn in its weather case first (the case shown for the line kept, the
/// other sections as they were); the selection changed or the window
/// closed, back in the cases shown.
#[test]
fn a_phase_distance_result_draws_its_pair_in_its_case() {
    let mut h = window();
    let (a, b, other, k, shown) = {
        let r = h.state_mut().ready().unwrap();
        let ids: Vec<String> = r.model.line.sections.iter().map(|s| s.id.clone()).collect();
        assert!(ids.len() >= 3);
        let shown = r.case;
        let k = (shown + 1) % r.model.cases.len();
        assert_ne!(k, shown);
        (ids[0].clone(), ids[1].clone(), ids[2].clone(), k, shown)
    };
    let pick = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.phase_distance.open = true;
        let case = r.model.cases[k].case.name.clone();
        let out = crate::phase_distance::Out {
            go: Some(crate::selection::Selection::Span {
                section: a.clone(),
                span: 0,
            }),
            measure: Some(([0.0, 0.0, 10.0], [1.0, 0.0, 10.0], "d · z".into())),
            case: Some(case),
            sections: vec![a.clone(), b.clone()],
            ..Default::default()
        };
        r.phase_distance_out(out, crate::phase_distance::Kind::Span);
    };
    pick(&mut h);
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.case, shown, "the case shown kept");
        assert_eq!(r.model.section_case(&a, r.case), k);
        assert_eq!(r.model.section_case(&b, r.case), k);
        assert_eq!(r.model.section_case(&other, r.case), shown);
        // Both cases drawn on the pair: the result's first.
        assert_eq!(r.model.section_cases(&a, r.case), vec![k, shown]);
        let rc = r.model.result_case.as_ref().unwrap();
        assert!(rc.says.contains(&r.model.cases[k].case.name), "{}", rc.says);
    }
    // Another selection: the result's case goes.
    h.state_mut()
        .ready()
        .unwrap()
        .select(crate::selection::Selection::None);
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.model.result_case.is_none());
        assert_eq!(r.model.section_case(&a, r.case), shown);
    }
    // Picked again, its window closed: gone too.
    pick(&mut h);
    h.run();
    assert!(h.state_mut().ready().unwrap().model.result_case.is_some());
    h.state_mut().ready().unwrap().phase_distance.open = false;
    h.run();
    h.run();
    assert!(h.state_mut().ready().unwrap().model.result_case.is_none());
}

/// The note beside a result (owner, 2026-10-27): in its wire's colour
/// (the section's display colour), not white, without a box.
#[test]
fn a_result_note_is_in_its_wire_colour() {
    let mut h = window();
    let r = h.state_mut().ready().unwrap();
    let a = r.model.line.sections[0].id.clone();
    let rc = crate::result_case::ResultCase::new(
        crate::result_case::From::PhaseDistance(crate::phase_distance::Kind::Span),
        crate::selection::Selection::Span {
            section: a.clone(),
            span: 0,
        },
        vec![a.clone()],
        Some(0),
        "found in a case".into(),
        [0.0; 3],
    );
    r.show_result_case(Some(rc));
    let styles = crate::wire_style::Styles::new(&r.model.line, r.model.section_display.coloring);
    let wire = styles.section(&a).color;
    assert_ne!(wire, [255, 255, 255], "the example's wire is not white");
    assert_eq!(r.model.result_case.as_ref().unwrap().colour, wire);
}

/// Drags the list handle at `from` down by `dy` px (in steps of 20 px).
fn drag_handle(h: &mut Harness<'_, App>, from: egui::Pos2, dy: f32) {
    h.input_mut().events.extend([
        egui::Event::PointerMoved(from),
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    h.step();
    let steps = (dy / 20.0).round() as i32;
    for k in 1..=steps {
        let at = from + egui::vec2(0.0, 20.0 * k as f32);
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.step();
    }
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: from + egui::vec2(0.0, dy),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
}

/// The Clearances window (a page until 2026-10-27; owner: "yes
/// standardise"): opened by Checks › Wire clearances › Show results (no
/// page any more), docked with 📌, floated with ⬈; its violations drawn
/// while it is open, taken off when it closes, back when it opens again.
#[test]
fn the_clearances_window_docks_and_its_marks_follow_it() {
    let mut h = window();
    assert!(!h.state_mut().ready().unwrap().clearances.open);
    h.get_by_label("Checks").click();
    h.run();
    h.get_by_label_contains("Wire clearances").hover();
    h.run();
    h.get_by_label("Show results").click();
    h.run();
    h.run();
    assert!(h.state_mut().ready().unwrap().clearances.open);
    let tab = Tab::Tool(Panel::Clearances);
    assert!(h.state_mut().dock().find_tab(&tab).is_none(), "floating");
    // Its check run from its footer.
    h.get_by_label("Run").click();
    let started = std::time::Instant::now();
    loop {
        h.step();
        let r = h.state_mut().ready().unwrap();
        if r.clearances.report.is_some() && !r.clearances.running() {
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.run();
    let marks = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let n = match &r.clearances.report {
            Some(Ok(rep)) => rep.findings.len(),
            _ => panic!("no report"),
        };
        (n, r.view3d.marks.len(), r.plan.marks.len())
    };
    let (n, shown, plan) = marks(&mut h);
    assert!(n > 0, "the example rules find violations");
    assert_eq!((shown, plan), (n, n), "drawn while open");
    // Docked: still drawn; floated again: still drawn.
    h.get_by_label("Dock as a panel").click();
    h.run();
    h.run();
    assert!(h.state_mut().dock().find_tab(&tab).is_some(), "docked");
    assert_eq!(marks(&mut h).1, n);
    h.get_by_label("Float").click();
    h.run();
    h.run();
    assert!(h.state_mut().dock().find_tab(&tab).is_none(), "floating");
    assert_eq!(marks(&mut h).1, n);
    // Closed: taken off; opened again: back.
    h.state_mut().ready().unwrap().clearances.open = false;
    h.run();
    assert_eq!(marks(&mut h).1, 0, "taken off when closed");
    h.state_mut().ready().unwrap().clearances.open = true;
    h.run();
    assert_eq!(marks(&mut h).1, n, "back when opened again");
}

/// Spot structures (owner, 2026-10-27: "keep it standard too"): a check
/// window like the others, docked with 📌.
#[test]
fn spot_structures_docks_as_a_panel() {
    let mut h = window();
    {
        let r = h.state_mut().ready().unwrap();
        r.spot.open(&r.model);
    }
    h.run();
    let tab = Tab::Tool(Panel::Spot);
    assert!(h.state_mut().dock().find_tab(&tab).is_none(), "floating");
    h.get_by_label("Dock as a panel").click();
    h.run();
    h.run();
    assert!(h.state_mut().dock().find_tab(&tab).is_some(), "docked");
    h.get_by_label("Run");
    h.get_by_label("Float").click();
    h.run();
    h.run();
    assert!(h.state_mut().dock().find_tab(&tab).is_none(), "floating");
}

/// A settings window's list (Criteria › Default values; owner,
/// 2026-10-27: "yes if the list is scrollable and risks getting big"):
/// its handle dragged down 100 px makes it 100 px taller.
#[test]
fn a_settings_list_is_made_taller() {
    let mut h = window();
    {
        let r = h.state_mut().ready().unwrap();
        r.defaults_window = Some((r.model.line.defaults, None));
    }
    h.run();
    h.run();
    let height =
        |h: &Harness<'_, App>| crate::check_help::list_height(&h.ctx, "default-values", 0.0);
    assert_eq!(height(&h), 0.0, "not dragged yet");
    let handle = h
        .query_all_by_label(crate::check_help::LIST_HANDLE)
        .next()
        .expect("a handle under the default values")
        .rect();
    drag_handle(&mut h, handle.center(), 100.0);
    let taller = height(&h);
    assert!((taller - 520.0).abs() < 1.0, "{taller}");
}

/// Internal clearances (owner, 2026-10-27: "Internal clearances should be
/// just under the wire clearance"): in the Checks menu's Wire clearances
/// submenu, no more among the Analyses; it opens the window.
#[test]
fn internal_clearances_is_under_wire_clearances() {
    let mut h = window();
    h.get_by_label("Checks").click();
    h.run();
    // Not in the Checks menu itself (the submenu is closed).
    assert!(h.query_by_label("Internal clearances…").is_none());
    h.query_all_by_label_contains("Wire clearances")
        .next()
        .expect("the Wire clearances submenu")
        .hover();
    h.run();
    h.run();
    h.get_by_label("Internal clearances…").click();
    h.run();
    assert!(h.state_mut().ready().unwrap().internal_clearances.open);
}

/// Internal clearances refuses a rule whose distance is not set (a new
/// rule's 0 m), as the ground windows do: Run greyed, "Before Run: …"
/// says why, Setup shown; a distance set, Run is offered.
#[test]
fn internal_clearances_refuses_a_rule_without_a_distance() {
    let mut h = window();
    {
        let r = h.state_mut().ready().unwrap();
        r.internal_clearances.show(&r.model);
        r.internal_clearances.rules = vec![ohl_clearance::internal::InternalRule::blank("a")];
    }
    h.run();
    h.run();
    h.get_by_label_contains("Before Run: \"a\"");
    assert!(h.get_by_label("Run").accesskit_node().is_disabled());
    assert_eq!(
        crate::check_ui::current_tab(&h.ctx, "internal-clearances-window"),
        crate::check_ui::Tab::Setup
    );
    h.state_mut().ready().unwrap().internal_clearances.rules[0].distance =
        ohl_clearance::rules::Requirement::Fixed(1.5);
    h.run();
    assert!(!h.get_by_label("Run").accesskit_node().is_disabled());
    assert!(h.query_by_label_contains("Before Run").is_none());
}

/// What the Stop test's window keeps: its job, the run's token, whether
/// Results is shown.
type StopState = (
    crate::job::Job<u32>,
    Option<ohl_core::budget::CancelToken>,
    bool,
);

/// Stop in every check (2026-10-27): a run under way is stopped by the
/// footer's Stop — the job no longer runs, its token is cancelled, the
/// previous results stay; Results opens by itself when a run ends.
#[test]
fn stop_cancels_a_run_and_a_new_result_opens_results() {
    let mut h = Harness::new_ui_state(
        |ui, s: &mut StopState| {
            let tab = crate::check_ui::tabs(ui, "stop-test", None, s.0.generation, false);
            s.2 = tab == crate::check_ui::Tab::Results;
            s.0.poll();
            if crate::check_ui::job_footer(ui, &mut s.0, None, "run", None, |_| {}) {
                let token = s.0.cancel_token();
                s.1 = Some(token.clone());
                let ctx = ui.ctx().clone();
                s.0.start(&ctx, move || {
                    // Until cancelled (at most 30 s).
                    for _ in 0..3000 {
                        if token.is_cancelled() {
                            return Err("cancelled".into());
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Ok(7)
                });
            }
        },
        (crate::job::Job::default(), None, false),
    );
    h.run();
    assert!(!h.state().2, "Setup first");
    h.get_by_label("Run").click();
    h.run_steps(2);
    assert!(h.state().0.running());
    h.get_by_label("Stop").click();
    h.run_steps(2);
    assert!(!h.state().0.running(), "stopped");
    assert!(h.state().1.as_ref().unwrap().is_cancelled());
    assert!(h.state().0.result.is_none(), "no new result");
    // A run that ends: Results opens by itself.
    h.state_mut().0.start(&egui::Context::default(), || Ok(1));
    for _ in 0..200 {
        h.step();
        if h.state().0.result.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run();
    assert!(h.state().2, "Results opened after the run");
}

/// The scope of a check window (owner, 2026-10-28: circuits, routes and
/// sections in every window): Wire tensions with one circuit ticked checks
/// only its spans, and keeps the scope in its rule file's `[check]`.
#[test]
fn a_scope_restricts_a_check() {
    let model = synthetic();
    let circuits = crate::check_ui::circuits_of(&model.line);
    assert!(circuits.len() >= 2, "{circuits:?}");
    let one = circuits[0].clone();
    let of_one: std::collections::BTreeSet<String> = model
        .line
        .sections
        .iter()
        .filter(|s| s.circuit_name == one)
        .map(|s| s.id.clone())
        .collect();
    let mut w = crate::wire_tensions::WireTensionsWindow::default();
    w.rules = vec![ohl_clearance::tension::TensionRule {
        factor: 0.01,
        ..ohl_clearance::tension::TensionRule::blank("invented 0.01")
    }];
    w.scope.circuits = Some(vec![one.clone()]);
    // Kept in the rule file's [check], read back the same.
    let set = ohl_clearance::RuleSet::parse(&w.rule_set().to_toml()).unwrap();
    assert_eq!(
        crate::check_ui::Scope::of_check(set.check.as_ref().unwrap()),
        w.scope
    );
    w.start(&model, &egui::Context::default());
    let started = std::time::Instant::now();
    while w.job.result.is_none() {
        w.job.poll();
        assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let rep = w.job.result.clone().unwrap().unwrap();
    assert!(!rep.report.rows.is_empty());
    assert!(
        rep.report.rows.iter().all(|r| of_one.contains(&r.section)),
        "only the circuit ticked"
    );
    // No circuit ticked: Run says why.
    w.scope.circuits = Some(Vec::new());
    assert!(w.scope.why_not(&model.line).is_some());
}

/// A check window's setup kept in the project (owner, 2026-10-28): the
/// Galloping window's scope written as a project setting of the line
/// worked on, read back by a window opened anew.
#[test]
fn a_check_setup_is_kept_in_the_project() {
    let mut model = crate::tests::synthetic_project("galloping-scope-kept");
    let ctx = egui::Context::default();
    let mut w = crate::galloping::GallopingWindow::default();
    // Read first (nothing kept yet), then changed and kept.
    w.keep(&mut model, &ctx);
    assert!(w.scope.whole());
    let circuit = crate::check_ui::circuits_of(&model.line)[0].clone();
    w.scope.circuits = Some(vec![circuit.clone()]);
    w.keep(&mut model, &ctx);
    let key = format!(
        "{}.{}",
        crate::galloping::SETTING,
        model.session().unwrap().current_line()
    );
    let text = model.session().unwrap().setting(&key).unwrap().unwrap();
    assert!(text.contains(&circuit), "{text}");
    let mut again = crate::galloping::GallopingWindow::default();
    again.keep(&mut model, &ctx);
    assert_eq!(again.scope, w.scope);
}
