// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The check windows rendered as a user sees them (check pages'
//! guidelines §6): with `OHL_RENDER_PNG` naming a folder (and a GPU),
//! each window's Setup tab — scrolled a page at a time, then with its
//! *Model* opened — and its Results tab after a run, on the synthetic
//! line, written as PNGs to look at (nothing overlapping or clipped).
//! Without it, the test does nothing.

use super::*;

/// A check window: its title, how it is opened (and made ready to run),
/// whether it runs here, whether its run ended.
struct Shot {
    name: &'static str,
    title: &'static str,
    open: fn(&mut crate::app::Ready),
    done: Option<fn(&mut crate::app::Ready) -> bool>,
}

const SHOTS: &[Shot] = &[
    Shot {
        name: "wire-tensions",
        title: "Wire tensions",
        open: |r| {
            r.wire_tensions.show(&r.model);
            r.wire_tensions.rules = vec![ohl_clearance::tension::TensionRule {
                factor: 0.2,
                ..ohl_clearance::tension::TensionRule::blank("invented 0.2")
            }];
        },
        done: Some(|r| r.wire_tensions.job.result.is_some()),
    },
    Shot {
        name: "string-checks",
        title: "String checks",
        open: |r| r.string_checks.show(&r.model),
        done: Some(|r| r.string_checks.job.result.is_some()),
    },
    Shot {
        name: "internal-clearances",
        title: "Internal clearances",
        open: |r| {
            r.internal_clearances.show(&r.model);
            r.internal_clearances.rules = vec![ohl_clearance::internal::InternalRule {
                distance: ohl_clearance::rules::Requirement::Fixed(1.5),
                ..ohl_clearance::internal::InternalRule::blank("invented 1.5 m")
            }];
        },
        done: Some(|r| r.internal_clearances.job.result.is_some()),
    },
    Shot {
        name: "galloping",
        title: "Galloping ellipses",
        open: |r| r.galloping.show(&r.model, r.case),
        done: Some(|r| r.galloping.job.result.is_some()),
    },
    Shot {
        name: "line-parameters",
        title: "Line parameters",
        open: |r| r.line_parameters.show(r.case),
        done: Some(|r| r.line_parameters.job.result.is_some()),
    },
    Shot {
        name: "structure-loads",
        title: "Structure loads",
        open: |r| r.loads.open = true,
        done: Some(|r| r.loads.results.is_some() && !r.loads.running()),
    },
    Shot {
        name: "clearances",
        title: "Clearances",
        open: |r| r.clearances.open = true,
        done: Some(|r| r.clearances.report.is_some() && !r.clearances.running()),
    },
    Shot {
        name: "vertical-clearances",
        title: "Vertical clearances",
        open: |r| r.vertical_clearances.show(&r.model),
        done: None,
    },
    Shot {
        name: "safety-volumes",
        title: "Safety volumes",
        open: |r| {
            r.safety_volume.show(r.case);
            r.safety_volume.settings.distance = ohl_clearance::rules::Requirement::Fixed(3.0);
        },
        done: Some(|r| r.safety_volume.job.result.is_some()),
    },
    Shot {
        name: "distance-between-conductors",
        title: "Distance between conductors",
        open: |r| r.phase_distance.show(&r.model),
        done: None,
    },
    Shot {
        name: "unequal-wind",
        title: "Distance between conductors under unequal wind",
        open: |r| r.unequal_wind.show(&r.model),
        done: None,
    },
    Shot {
        name: "vegetation",
        title: "Vegetation and danger trees",
        open: |r| r.vegetation.show(r.case),
        done: None,
    },
    Shot {
        name: "as-built-wires",
        title: "As-built wires",
        open: |r| r.wirefit.open = true,
        done: None,
    },
    Shot {
        name: "thermal",
        title: "Thermal rating",
        open: |r| r.thermal.show(&r.model),
        done: None,
    },
    Shot {
        name: "performed-checks",
        title: "Performed checks",
        open: |r| r.check_library.show(),
        done: None,
    },
];

/// The window in front, saved as `dir`/`name`.png.
fn save(harness: &mut Harness<'_, App>, dir: &Path, name: &str) {
    let image = harness.render().unwrap();
    let path = dir.join(format!("{name}.png"));
    crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
}

/// The window titled `title`: of the nodes so named (a heading may be
/// too), the largest.
fn window_rect(harness: &Harness<'_, App>, title: &str) -> egui::Rect {
    harness
        .query_all_by_label(title)
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("the window")
}

/// Turns the mouse wheel over the window's body, `dy` points.
fn wheel(harness: &mut Harness<'_, App>, at: egui::Pos2, dy: f32) {
    harness.event(egui::Event::PointerMoved(at));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    harness.run_steps(4);
}

/// Every check window's Setup (scrolled, then its Model opened) and
/// Results, as PNGs in `OHL_RENDER_PNG` (made as `USER=tester`).
#[test]
fn check_windows_rendered_for_review() {
    let Some(dir) =
        ohl_core::env::var_os("OHL_RENDER_PNG").filter(|_| ohl_render::headless_device().is_ok())
    else {
        return;
    };
    let dir = PathBuf::from(dir);
    let only = ohl_core::env::var_os("OHL_RENDER_ONLY").map(|s| s.to_string_lossy().into_owned());
    for shot in SHOTS {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == shot.name))
        {
            continue;
        }
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1300.0, 980.0))
            .wgpu()
            .build_eframe(|cc| {
                // The loads need the project's load cases; the others the
                // example line as it is (its clearance rule file).
                let model = if matches!(shot.name, "structure-loads" | "performed-checks") {
                    synthetic_project(&format!("shots-{}", shot.name))
                } else {
                    synthetic()
                };
                App::with_model(model, cc.wgpu_render_state.clone())
            });
        harness.run();
        (shot.open)(harness.state_mut().ready().unwrap());
        harness.run();
        harness.run_steps(4);
        let window = window_rect(&harness, shot.title);
        let body = egui::pos2(window.center().x, window.top() + 200.0);
        save(&mut harness, &dir, &format!("{}-setup-1", shot.name));
        // How it is computed opened: its drawings (closed again after).
        if let Some(h) = harness.query_by_label("How it is computed") {
            h.click();
            harness.run_steps(4);
            save(&mut harness, &dir, &format!("{}-how", shot.name));
            harness.get_by_label("How it is computed").click();
            harness.run_steps(4);
        }
        for i in 2..=4 {
            wheel(&mut harness, body, -550.0);
            save(&mut harness, &dir, &format!("{}-setup-{i}", shot.name));
        }
        // The Model opened (where the window has one), at the bottom.
        if let Some(m) = harness.query_by_label_contains("  Model") {
            m.click();
            harness.run_steps(4);
            wheel(&mut harness, body, -2000.0);
            save(&mut harness, &dir, &format!("{}-setup-model", shot.name));
        }
        let Some(done) = shot.done else {
            // The tab: the topmost "Results" (a heading may say it too).
            if let Some(tab) = harness
                .query_all_by_label_contains("Results")
                .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
            {
                tab.click();
            }
            harness.run_steps(4);
            save(&mut harness, &dir, &format!("{}-results", shot.name));
            continue;
        };
        harness
            .query_all_by_label("Run")
            .filter(|n| window.contains_rect(n.rect()))
            .max_by(|a, b| a.rect().bottom().total_cmp(&b.rect().bottom()))
            .expect("Run in its footer")
            .click();
        let started = std::time::Instant::now();
        loop {
            harness.step();
            if done(harness.state_mut().ready().unwrap()) {
                break;
            }
            assert!(
                started.elapsed().as_secs() < 180,
                "{}: no result in 180 s",
                shot.name
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        harness.run_steps(6);
        save(&mut harness, &dir, &format!("{}-results", shot.name));
        // Structure loads: its usage view too.
        if let Some(u) = harness.query_by_label("Usage") {
            u.click();
            harness.run_steps(4);
            save(&mut harness, &dir, &format!("{}-usage", shot.name));
        }
    }
}
