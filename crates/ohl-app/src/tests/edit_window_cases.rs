// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: an edit from a structure's Edit window re-solves the weather
//! case shown at once, the others later (owner, 2026-11-18: "it
//! recomputes all weather cases where it could only recompute the
//! displayed one which would be quicker").

use super::*;

/// The example (five weather cases, all solved), T3's Edit window: A1
/// raised 0.1 m and applied three times. Each Apply solves the case shown
/// at once — the first too, whose strings are taken into the insulator
/// library (a library change: before, every case was solved again there,
/// in the window's frame); the others are not solved but marked to solve
/// (nothing shows or uses a stale one), and are worked out in the
/// background once the edits have settled ([`model::SETTLE`]), then equal
/// to a full solve of the line. Timings printed (`--nocapture`).
#[test]
fn an_edit_solves_the_case_shown_and_the_others_later() {
    if ohl_render::headless_device().is_err() {
        return;
    }
    let model = synthetic_project("edit-window-cases");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    h.run();
    h.get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    h.run();
    h.run();
    let ready = |h: &mut Harness<'_, App>| -> Vec<bool> {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .cases
            .iter()
            .map(|c| c.ready)
            .collect()
    };
    assert_eq!(ready(&mut h), [true; 5], "every case solved to begin with");
    let shown = h.state_mut().ready().unwrap().case;
    for round in 0..3 {
        {
            let r = h.state_mut().ready().unwrap();
            let p = r.placed.as_mut().unwrap();
            let t = p.doc["point"].as_array_of_tables_mut().unwrap();
            let first = t.get_mut(0).unwrap();
            let mut a = first["point"].as_array().unwrap().clone();
            let z: f64 = a
                .get(2)
                .unwrap()
                .as_str()
                .unwrap()
                .trim_end_matches(" m")
                .parse()
                .unwrap();
            a.replace(2, format!("{:.3} m", z + 0.1));
            first["point"] = toml_edit::value(a);
        }
        h.run();
        let t0 = std::time::Instant::now();
        h.get_by_label("Apply").click();
        h.step();
        let took = t0.elapsed();
        let r = h.state_mut().ready().unwrap();
        eprintln!("Apply {round}: {took:.0?} — {:?}", r.status);
        for (k, c) in r.model.cases.iter().enumerate() {
            assert_eq!(c.ready, k == shown, "case {k} after Apply {round}");
        }
        // Solved as a full solve of the line gives it.
        assert_solved_fully(&r.model);
        // The window follows the line before the next change.
        h.run();
    }
    // A library read again (a type or an entry saved): the case shown at
    // once, the others later.
    let t0 = std::time::Instant::now();
    let report = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .reload_libraries()
        .unwrap();
    eprintln!("libraries read again: {:.0?} — {report}", t0.elapsed());
    assert_eq!(
        ready(&mut h)
            .iter()
            .enumerate()
            .filter(|(k, r)| **r != (*k == shown))
            .count(),
        0
    );
    // The edits settled: the others worked out in the background (as the
    // application does; off in the tests by default).
    let r = h.state_mut().ready().unwrap();
    r.model.catch_up_after = Some(crate::model::SETTLE);
    r.model.edited_at = Some(std::time::Instant::now() - crate::model::SETTLE);
    let t0 = std::time::Instant::now();
    while ready(&mut h).iter().any(|r| !r) {
        assert!(t0.elapsed().as_secs() < 120, "not caught up in 120 s");
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    eprintln!("the other cases caught up in {:.0?}", t0.elapsed());
    assert_solved_fully(&h.state_mut().ready().unwrap().model);
    // Every case shown (asked for by the views): a library read again
    // solves them all at once, as it did every time before 2026-11-18.
    let r = h.state_mut().ready().unwrap();
    let now = std::time::Instant::now();
    for k in 0..r.model.cases.len() {
        r.model.recently_wanted.insert(k, now);
    }
    let t0 = std::time::Instant::now();
    let report = r.model.reload_libraries().unwrap();
    eprintln!(
        "libraries read again, every case wanted (as before): {:.0?} — {report}",
        t0.elapsed()
    );
    assert!(r.model.cases.iter().all(|c| c.ready));
}
