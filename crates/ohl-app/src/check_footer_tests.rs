// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The check windows' Run footer (owner, 2026-10-21): Run at the bottom
//! left of every check window and page, visible however long the window's
//! content (the body scrolls above it, the footer stays), and clickable;
//! and the violations of a clearance window off the views once it is
//! closed, back when it is opened again.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::app::{App, Ready};
use crate::tests::synthetic;

/// The screenshots of the footer go here when `OHL_FOOTER_SHOTS` names a
/// folder (none by default).
fn shots() -> Option<std::path::PathBuf> {
    std::env::var_os("OHL_FOOTER_SHOTS").map(std::path::PathBuf::from)
}

/// A check window or page: its name, how it is opened, whether its run
/// started (running, or an answer).
struct Check {
    name: &'static str,
    open: fn(&mut Ready, &egui::Context),
    started: fn(&mut Ready) -> bool,
}

const CHECKS: &[Check] = &[
    Check {
        name: "vertical-clearances",
        open: |r, _| r.vertical_clearances.show(&r.model),
        started: |r| {
            let (report, running) = r.vertical_clearances.result();
            report.is_some() || running
        },
    },
    Check {
        name: "radial-clearances",
        open: |r, _| r.radial_clearances.show(&r.model),
        started: |r| {
            let (report, running) = r.radial_clearances.result();
            report.is_some() || running
        },
    },
    Check {
        name: "vegetation",
        open: |r, _| r.vegetation.show(r.case),
        started: |r| r.vegetation.job.started(),
    },
    Check {
        name: "safety-volumes",
        open: |r, _| r.safety_volume.show(r.case),
        started: |r| r.safety_volume.job.started(),
    },
    Check {
        name: "galloping",
        open: |r, _| r.galloping.show(&r.model, r.case),
        started: |r| r.galloping.job.started(),
    },
    Check {
        name: "as-built-wires",
        open: |r, _| r.wirefit.open = true,
        started: |r| r.wirefit.job.started(),
    },
    Check {
        name: "thermal",
        open: |r, _| r.thermal.show(&r.model),
        started: |r| r.thermal.running() || r.thermal.limits.is_some(),
    },
    Check {
        name: "bill-of-materials",
        open: |r, ctx| {
            r.bom.open(&r.model, ctx);
            // Its first answer in, so that Run is not greyed.
            while r.bom.running() {
                r.bom.poll();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            r.bom.result = None;
        },
        started: |r| r.bom.running() || r.bom.result.is_some(),
    },
    Check {
        name: "spotting",
        open: |r, _| r.spot.open(&r.model),
        started: |r| r.spot.running() || r.spot.result.is_some(),
    },
    Check {
        name: "structure-loads",
        open: |r, _| r.loads.open = true,
        started: |r| r.loads.running() || r.loads.results.is_some(),
    },
];

/// Turns the mouse wheel at `at`, `dy` points (negative: down).
fn wheel(harness: &mut Harness<'_, App>, at: egui::Pos2, dy: f32) {
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    harness.run_steps(3);
}

/// Every check window and page: Run on screen, at the bottom of the
/// window, in the same place after the body scrolled (the screen is short:
/// the content is longer than the window), and clickable — its run starts
/// (or, greyed, it says why).
#[test]
fn every_check_window_keeps_run_in_its_footer() {
    for check in CHECKS {
        let size = egui::vec2(1300.0, 620.0);
        let mut harness = Harness::builder()
            .with_size(size)
            .build_eframe(|_| App::with_model(synthetic(), None));
        harness.run();
        {
            let ctx = harness.ctx.clone();
            let r = harness.state_mut().ready().unwrap();
            (check.open)(r, &ctx);
        }
        harness.run();
        harness.run();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let run = harness.get_by_label("Run").rect();
        assert!(
            screen.contains_rect(run),
            "{}: Run off the screen at {run:?}",
            check.name
        );
        // At the bottom: nothing of the window but the footer below it
        // (the lower part of the screen).
        assert!(
            run.top() > size.y * 0.45,
            "{}: Run not at the bottom: {run:?}",
            check.name
        );
        if check.name == "vertical-clearances"
            && let Some(dir) = shots()
        {
            harness.run();
            std::fs::create_dir_all(&dir).unwrap();
            harness
                .render()
                .unwrap()
                .save(dir.join("vertical-clearances.png"))
                .unwrap();
        }
        // The body scrolled down: Run stays.
        let body = egui::pos2(run.left() + 120.0, run.top() - 120.0);
        wheel(&mut harness, body, -4000.0);
        let after = harness.get_by_label("Run").rect();
        assert!(
            (after.min - run.min).length() < 1.0,
            "{}: Run moved from {run:?} to {after:?} when the body scrolled",
            check.name
        );
        if check.name == "safety-volumes"
            && let Some(dir) = shots()
        {
            harness
                .render()
                .unwrap()
                .save(dir.join("safety-volumes-scrolled.png"))
                .unwrap();
        }
        let node = harness.get_by_label("Run");
        if node.accesskit_node().is_disabled() {
            // Greyed (thermal without a rule file, a clearance window
            // whose first rule has no distance yet): it says why.
            assert!(
                matches!(
                    check.name,
                    "thermal" | "vertical-clearances" | "radial-clearances"
                ),
                "{}: Run greyed",
                check.name
            );
            continue;
        }
        node.click();
        harness.step();
        assert!(
            (check.started)(harness.state_mut().ready().unwrap()),
            "{}: the click on Run did not start it",
            check.name
        );
    }
}

/// A vertical check run; the window closed: its violations are taken off
/// the views (3D and plan); opened again: they are back. Docked, they stay
/// (the window is not closed).
#[test]
fn closing_a_clearance_window_takes_its_marks_off() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(synthetic(), None));
    harness.run();
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-zone-close", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        r.vertical_clearances.show(&r.model);
        let w = &mut r.vertical_clearances;
        w.file = Some(dir.join("rules.ohlr"));
        w.rules.clear();
        // 100 m over the ground: every span is below it.
        w.rules.push(ohl_clearance::ZoneRule {
            name: "High ground".into(),
            voltages: Vec::new(),
            cases: Vec::new(),
            distance: ohl_clearance::Requirement::Fixed(100.0),
            classes: Vec::new(),
            ground: true,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
            screen: false,
        });
    }
    harness.run();
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    let found = loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.vertical_clearances.result() {
            break rep.as_ref().unwrap().zone.findings.len();
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    harness.run();
    assert!(found > 0);
    let marks = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        (r.view3d.marks.len(), r.plan.marks.len())
    };
    assert_eq!(marks(&mut harness), (found, found));
    // Closed: off the views.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .vertical_clearances
        .open = false;
    harness.run();
    assert_eq!(marks(&mut harness), (0, 0));
    // Opened again: back.
    {
        let r = harness.state_mut().ready().unwrap();
        r.vertical_clearances.show(&r.model);
    }
    harness.run();
    harness.run();
    assert_eq!(marks(&mut harness), (found, found));
    // Another window closed (never run): the marks drawn stay.
    {
        let r = harness.state_mut().ready().unwrap();
        r.radial_clearances.show(&r.model);
    }
    harness.run();
    harness.state_mut().ready().unwrap().radial_clearances.open = false;
    harness.run();
    assert_eq!(marks(&mut harness), (found, found));
    std::fs::remove_dir_all(dir).ok();
}
