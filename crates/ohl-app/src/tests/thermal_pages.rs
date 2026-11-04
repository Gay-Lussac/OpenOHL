// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Thermal rating in four pages (owner, 2026-11-04): the pages
//! exist; a system's operating current typed in Line › Electrical systems
//! (one undo) is the default current of the Temperature and Step change
//! pages and of the Magnetic field, a current typed in the window
//! overrides it; the clearance check at the current gives a pass or fail
//! per section and draws its clearances.

use super::*;
use crate::thermal::{ClearanceMode, Page};

/// Opens Checks › Thermal rating… with the example's ambient cases.
fn open_thermal(h: &mut Harness<'_, App>) {
    h.get_by_label("Checks").click();
    h.run();
    h.get_by_label("Thermal rating…").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.thermal.open);
    if r.thermal.file.is_none() {
        r.thermal
            .choose(root().join("examples/synthetic-line/thermal-ambient.ohlta"));
    }
    h.run();
}

#[test]
fn the_systems_current_is_the_default_of_the_pages() {
    use crate::tables::TableKind;
    let model = synthetic_project("thermal-current");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    open_thermal(&mut h);
    // The four pages, at the window's top.
    for p in Page::ALL {
        h.get_by_label(p.title());
    }
    // The system's operating current, typed in its table: one command.
    let id = h.state_mut().ready().unwrap().model.line.systems[0]
        .id
        .clone();
    set_cell(&mut h, TableKind::Systems, &id, "A", "900");
    apply_table(&mut h, TableKind::Systems, false).unwrap();
    assert_eq!(
        h.state_mut().ready().unwrap().model.line.systems[0].current_a,
        Some(900.0)
    );
    // A negative one does not read.
    set_cell(&mut h, TableKind::Systems, &id, "A", "-5");
    let e = apply_table(&mut h, TableKind::Systems, false).unwrap_err();
    assert!(e.contains("operating current"), "{e}");
    set_cell(&mut h, TableKind::Systems, &id, "A", "900");
    // Temperature at a current: a row per section at the system's 900 A.
    h.get_by_label("Temperature at a current").click();
    h.run();
    h.get_by_label("Results").click();
    h.run();
    h.get_by_label("Hottest [°C]");
    let hot = {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.thermal.page, Page::Temperature);
        assert_eq!(r.thermal.current, None);
        let heats = r.thermal.temperatures(&r.model);
        assert_eq!(heats.len(), r.model.line.sections.len());
        let s1 = heats.iter().find(|h| h.section == "S1").unwrap();
        assert_eq!(s1.current, Ok(900.0));
        let (_, t) = s1.hottest().unwrap();
        assert!(t > 35.0 && t < 200.0, "{t}");
        // The Magnetic field takes it too (nothing typed twice).
        let label = ohl_electrical::magnetic::circuits(&r.model.line)[0].label();
        assert_eq!(r.magnetic_field.shown_current(&r.model, &label), 900.0);
        r.magnetic_field.set_current(&label, 450.0);
        assert_eq!(r.magnetic_field.shown_current(&r.model, &label), 450.0);
        // A current typed in the window overrides the line's: cooler.
        r.thermal.current = Some(300.0);
        let s1 = r
            .thermal
            .temperatures(&r.model)
            .into_iter()
            .find(|h| h.section == "S1")
            .unwrap();
        assert_eq!(s1.current, Ok(300.0));
        assert!(s1.hottest().unwrap().1 < t);
        r.thermal.current = None;
        t
    };
    h.run();
    // Step change: from 300 A to the line's 900 A, rising towards the
    // steady temperature.
    h.get_by_label("Step change").click();
    h.run();
    h.get_by_label("Results").click();
    h.run();
    h.get_by_label("Time [s]");
    {
        let r = h.state_mut().ready().unwrap();
        r.thermal.step_from = 300.0;
        let curves = r.thermal.step_curves_now(&r.model);
        assert!(!curves.is_empty());
        let c = &curves[0];
        assert_eq!(c.to, 900.0);
        let points = c.points.as_ref().unwrap();
        assert!(points.last().unwrap().1 > points[0].1);
        assert!(points.last().unwrap().1 <= *c.end.as_ref().unwrap() + 1e-6);
        // Its steady end is the Temperature page's in the same case.
        let k = r.thermal.step_case;
        let s = r
            .thermal
            .temperatures(&r.model)
            .into_iter()
            .find(|h| h.section == c.sections[0])
            .unwrap();
        assert!((s.in_case(k).unwrap() - c.end.as_ref().unwrap()).abs() < 1e-6);
        assert!(s.in_case(k).unwrap() <= hot);
    }
    // Undone: the system has no current; the pages say so per section.
    press(&mut h, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = h.state_mut().ready().unwrap();
    assert_eq!(r.model.line.systems[0].current_a, None);
    assert!(
        r.thermal
            .temperatures(&r.model)
            .iter()
            .all(|h| h.current.is_err())
    );
}

#[test]
fn the_clearance_at_a_current_passes_or_fails_each_section() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(synthetic(), None));
    h.run();
    open_thermal(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        r.thermal.page = Page::Clearance;
        r.thermal.mode = ClearanceMode::AtCurrent;
        r.thermal.current = Some(400.0);
    }
    h.run();
    h.get_by_label("Run").click();
    h.step();
    run_until(&mut h, |a| {
        let r = a.ready().unwrap();
        r.thermal.poll();
        !r.thermal.running() && r.thermal.checks.is_some()
    });
    h.step();
    h.step();
    h.run();
    // Results opened by itself.
    h.get_by_label("Margin [m]");
    let r = h.state_mut().ready().unwrap();
    let check = r.thermal.checks.as_ref().unwrap();
    let checks = check.result.as_ref().unwrap();
    assert_eq!(checks.len(), r.model.line.sections.len());
    // Each at its hottest case's temperature, with a verdict.
    for (c, heat) in checks.iter().zip(&check.heats) {
        assert_eq!(heat.hottest().unwrap().1, c.temperature);
        assert!(c.passes().is_some(), "{c:?}");
    }
    // S2 over the hill misses the 7 m ground rule even when cold.
    let s2 = checks.iter().find(|c| c.section == "S2").unwrap();
    assert_eq!(s2.passes(), Some(false));
    // Its clearances drawn in the views, not out of date.
    assert_eq!(r.marks_from, crate::app::MarksFrom::Thermal);
    assert!(!r.view3d.marks.is_empty());
    assert!(r.thermal.out_of_date(&r.model).is_none());
    // Another current: the result is out of date, said.
    r.thermal.current = Some(800.0);
    assert!(
        r.thermal
            .out_of_date(&r.model)
            .is_some_and(|w| w.contains("current"))
    );
    // Closed: its marks are taken off.
    r.thermal.open = false;
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.view3d.marks.is_empty());
}
