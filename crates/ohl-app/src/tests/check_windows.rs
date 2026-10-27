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
    let id = ("Vertical clearances", "rules");
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
