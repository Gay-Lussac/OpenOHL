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
        name: "lightning",
        title: "Lightning shielding",
        open: |r| {
            r.lightning.show(&r.model);
            r.lightning.rules = ohl_clearance::rules::RuleSet::load(
                &crate::tests::root().join("examples/lightning/shielded-line.lightning-rules.ohlr"),
            )
            .unwrap()
            .lightning_rules;
        },
        done: Some(|r| r.lightning.job.result.is_some()),
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
        name: "radial-clearances",
        title: "Radial clearances",
        open: |r| r.radial_clearances.show(&r.model),
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
        // A cloud around the first structure (below), a clearance that
        // finds some of it: its Results tab with findings.
        open: |r| {
            r.vegetation.show(0);
            r.vegetation.settings.clearance = ohl_clearance::rules::Requirement::Fixed(25.0);
        },
        done: Some(|r| matches!(r.vegetation.result(), (Some(_), false))),
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
    // Its other pages (owner, 2026-11-04): at a typed current.
    Shot {
        name: "thermal-temperature",
        title: "Thermal rating",
        open: |r| {
            r.thermal.show(&r.model);
            r.thermal.page = crate::thermal::Page::Temperature;
            r.thermal.current = Some(900.0);
        },
        done: None,
    },
    Shot {
        name: "thermal-step",
        title: "Thermal rating",
        open: |r| {
            r.thermal.show(&r.model);
            r.thermal.page = crate::thermal::Page::Step;
            r.thermal.current = Some(1200.0);
        },
        done: None,
    },
    Shot {
        name: "thermal-clearance",
        title: "Thermal rating",
        open: |r| {
            r.thermal.show(&r.model);
            r.thermal.page = crate::thermal::Page::Clearance;
            r.thermal.mode = crate::thermal::ClearanceMode::AtCurrent;
            r.thermal.current = Some(900.0);
        },
        done: Some(|r| {
            r.thermal.poll();
            !r.thermal.running() && r.thermal.checks.is_some()
        }),
    },
    // The field windows (owner, 2026-11-12: their review): one circuit
    // given; the electric field's ground the plane (quick).
    Shot {
        name: "magnetic-field",
        title: "Magnetic field",
        open: |r| {
            let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
            r.magnetic_field.set_current(&circuits[0].label(), 800.0);
            r.magnetic_field.show(r.case);
        },
        done: Some(|r| r.magnetic_field.common.job.result.is_some()),
    },
    Shot {
        name: "electric-field",
        title: "Electric field",
        open: |r| {
            let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
            r.electric_field.set_voltage(&circuits[0].label(), 400.0);
            r.electric_field.set_tiled(false);
            r.electric_field.show(r.case);
        },
        done: Some(|r| r.electric_field.common.job.result.is_some()),
    },
    Shot {
        name: "performed-checks",
        title: "Performed checks",
        open: |r| r.check_library.show(),
        done: None,
    },
];

/// A field window's Model fold: titled by what it holds, the topmost
/// node naming its first setting.
fn model_fold<'h>(harness: &'h Harness<'_, App>) -> Option<egui_kittest::Node<'h>> {
    harness
        .query_all_by_label_contains("Earth wires")
        .chain(harness.query_all_by_label_contains("Jumpers, "))
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
}

/// The field window `name`'s setup turned to `page`.
fn set_page(r: &mut crate::app::Ready, name: &str, page: crate::field_common::FieldPage) {
    if name == "magnetic-field" {
        r.magnetic_field.common.page = page;
    } else {
        r.electric_field.common.page = page;
    }
}

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
                } else if shot.name == "lightning" {
                    crate::tests::shielded()
                } else if shot.name == "vegetation" {
                    let (file, _) = crate::lidar_tests::project_with_cloud("shots-vegetation", 20);
                    crate::lidar_tests::load(&file)
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
        // The field windows: each of their three pages (owner,
        // 2026-11-12), from its top, its Model opened at the end; then
        // back on the 2D profiles for the run.
        if shot.name.ends_with("-field") {
            for page in crate::field_common::FieldPage::ALL {
                set_page(harness.state_mut().ready().unwrap(), shot.name, page);
                harness.run_steps(4);
                wheel(&mut harness, body, 5000.0);
                let tag = page.guide();
                save(&mut harness, &dir, &format!("{}-{tag}-1", shot.name));
                for i in 2..=4 {
                    wheel(&mut harness, body, -550.0);
                    save(&mut harness, &dir, &format!("{}-{tag}-{i}", shot.name));
                }
                if let Some(m) = model_fold(&harness) {
                    m.click();
                    harness.run_steps(4);
                    wheel(&mut harness, body, -2000.0);
                    save(&mut harness, &dir, &format!("{}-{tag}-model", shot.name));
                    if let Some(m) = model_fold(&harness) {
                        m.click();
                        harness.run_steps(4);
                    }
                }
            }
            set_page(
                harness.state_mut().ready().unwrap(),
                shot.name,
                crate::field_common::FieldPage::Profiles,
            );
            wheel(&mut harness, body, 5000.0);
        }
        // The field windows' setup is longer: more pages.
        let pages = if shot.name.ends_with("-field") { 0 } else { 4 };
        for i in 2..=pages {
            wheel(&mut harness, body, -550.0);
            save(&mut harness, &dir, &format!("{}-setup-{i}", shot.name));
        }
        // The Model opened (where the window has one), at the bottom
        // (the field windows': per page, above).
        if let Some(m) = harness
            .query_by_label_contains("  Model")
            .filter(|_| !shot.name.ends_with("-field"))
        {
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
        // Lightning shielding: the worst row's cross-section (its verdict
        // clicked).
        if shot.name == "lightning"
            && let Some(v) = harness.query_by_label_contains("✖ ")
        {
            v.click();
            harness.run_steps(4);
            wheel(&mut harness, body, -2000.0);
            save(&mut harness, &dir, &format!("{}-cut", shot.name));
        }
        // Structure loads: its usage view too.
        if let Some(u) = harness.query_by_label("Usage") {
            u.click();
            harness.run_steps(4);
            save(&mut harness, &dir, &format!("{}-usage", shot.name));
        }
    }
}
