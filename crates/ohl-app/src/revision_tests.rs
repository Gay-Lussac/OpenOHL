// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Results computed on demand are marked out of date when what they were
//! computed from changes — a point edit, a line edit — and not after a
//! view setting; an undo back to the state they were computed from clears
//! it. Layers drawn from them are dimmed.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::lidar_tests::{harness, load, project_with_cloud};
use crate::revision::Uses;

/// Steps until `done` (20 s at most).
fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&mut App) -> bool) {
    let started = Instant::now();
    loop {
        h.step();
        if done(h.state_mut()) {
            h.step();
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(60), "{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What the three windows run here say about their results.
fn stale(h: &mut Harness<'_, App>) -> [Option<String>; 3] {
    let r = h.state_mut().ready().unwrap();
    [
        r.vegetation.out_of_date(&r.model),
        r.wirefit.out_of_date(&r.model),
        r.bom.out_of_date(&r.model),
    ]
}

/// Changes the class of the ground point nearest the first structure to
/// 5 (one command).
fn change_a_point(h: &mut Harness<'_, App>) {
    let r = h.state_mut().ready().unwrap();
    let p = *r
        .model
        .points
        .iter()
        .filter(|p| p.1 == 2)
        .min_by_key(|p| p.0[0].abs() + p.0[1].abs())
        .unwrap();
    let mut f = crate::lidar_edit::PointsForm::of(&r.model, &[p]).unwrap();
    f.points[0].class = 5;
    let (what, command) = f.command().unwrap();
    assert!(r.perform(crate::edit::Action::Command { what, command }));
    h.run_steps(3);
}

#[test]
fn results_are_out_of_date_after_point_and_line_edits_not_view_settings() {
    let (file, _) = project_with_cloud("stale", 30);
    let mut h = harness(load(&file), false);
    h.run_steps(3);
    {
        let ctx = h.ctx.clone();
        let r = h.state_mut().ready().unwrap();
        r.vegetation.show(0);
        r.wirefit.open = true;
        r.bom.open(&r.model, &ctx);
    }
    h.run_steps(3);
    h.get_by_label("Fit").click();
    h.get_by_label("Check").click();
    until(&mut h, "the three results", |a| {
        let r = a.ready().unwrap();
        matches!(r.vegetation.result(), (Some(Ok(_)), false))
            && matches!(r.wirefit.result(), (Some(Ok(_)), false))
            && matches!(r.bom.result, Some(Ok(_)))
            && !r.bom.running()
    });
    assert_eq!(stale(&mut h), [None, None, None]);
    assert!(!h.state_mut().ready().unwrap().view3d.vegetation.stale);

    // View settings: a class hidden, another wind side, another case
    // shown — nothing out of date.
    {
        let r = h.state_mut().ready().unwrap();
        r.model.hidden_classes.insert(5);
        r.model.set_wind(crate::model::WindDisplay::Left);
        r.case = r.model.cases.len() - 1;
    }
    h.run_steps(3);
    assert_eq!(stale(&mut h), [None, None, None]);
    let before = h.state_mut().ready().unwrap().model.revision();

    // A point edit: the results from the points are out of date (the
    // ground built again from the changed ground point too); the costs
    // are not.
    change_a_point(&mut h);
    let [veg, fit, bom] = stale(&mut h);
    assert!(
        veg.as_deref().unwrap().contains("the LiDAR points"),
        "{veg:?}"
    );
    assert_eq!(fit.as_deref(), Some("the LiDAR points"));
    assert_eq!(bom, None, "the costs do not use the points");
    h.run_steps(3);
    assert!(
        h.query_all_by_label_contains("Out of date: the LiDAR points")
            .count()
            >= 2
    );
    assert!(
        h.state_mut().ready().unwrap().view3d.vegetation.stale,
        "dimmed"
    );
    until(&mut h, "the ground built again", |a| {
        let r = a.ready().unwrap();
        !r.lidar_ground.running() && !r.lidar_ground_again
    });
    assert_ne!(h.state_mut().ready().unwrap().model.revision(), before);

    // Undone: every point back, the ground built again as it was — the
    // results are what a run would give again, no longer out of date.
    h.state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    h.run_steps(3);
    until(&mut h, "the ground built again after the undo", |a| {
        let r = a.ready().unwrap();
        !r.lidar_ground.running() && !r.lidar_ground_again
    });
    assert_eq!(h.state_mut().ready().unwrap().model.revision(), before);
    assert_eq!(stale(&mut h), [None, None, None]);
    assert!(!h.state_mut().ready().unwrap().view3d.vegetation.stale);

    // A line edit: a structure moved — every result out of date.
    {
        let r = h.state_mut().ready().unwrap();
        let s = &r.model.line.structures[1];
        let (id, p) = (s.id.clone(), s.position);
        assert!(r.perform(crate::edit::Action::Move {
            id,
            position: [p[0] + 3.0, p[1], p[2]],
        }));
    }
    h.run_steps(3);
    let [veg, fit, bom] = stale(&mut h);
    assert_eq!(veg.as_deref(), Some("the line"));
    assert_eq!(fit.as_deref(), Some("the line"));
    assert_eq!(bom.as_deref(), Some("the line"));
    assert!(
        h.query_all_by_label_contains("Out of date: the line")
            .count()
            >= 3
    );
    // Nothing runs again by itself; the results stay shown.
    {
        let r = h.state_mut().ready().unwrap();
        assert!(!r.vegetation.result().1 && r.vegetation.result().0.is_some());
    }
    // Run again (the vegetation window's): up to date.
    assert!(h.query_all_by_label("Run again").count() >= 3);
    // The vegetation panel's (the others closed: they lie over it).
    {
        let r = h.state_mut().ready().unwrap();
        r.wirefit.open = false;
        r.bom.open = false;
    }
    h.run_steps(3);
    h.get_by_label("Run again").click();
    until(&mut h, "a vegetation run again", |a| {
        let r = a.ready().unwrap();
        matches!(r.vegetation.result(), (Some(Ok(_)), false))
            && r.vegetation.out_of_date(&r.model).is_none()
    });
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// What each result window's results are computed from: a point edit
/// flags only those reading the points, a ground surface only those
/// reading the ground, a line edit every one.
#[test]
fn each_result_uses_what_it_is_computed_from() {
    let all = [
        ("vegetation", crate::vegetation::USES),
        ("as-built wires", crate::wirefit::USES),
        ("clearance windows", crate::clearance_zones::USES),
        ("thermal clearance limits", crate::thermal::USES),
        ("galloping", crate::galloping::USES),
        ("distance between conductors", crate::phase_distance::USES),
        ("structure loads", crate::loads::USES),
        ("costs", crate::bom::USES),
        ("spotting", crate::spot::USES),
    ];
    let from = crate::revision::Revision {
        line: 1,
        points: 2,
        ground: 3,
    };
    let points = crate::revision::Revision { points: 9, ..from };
    let line = crate::revision::Revision { line: 9, ..from };
    for (name, uses) in all {
        assert!(from.changed(&line, uses).is_some(), "{name}: the line");
        let reads_points = matches!(
            name,
            "vegetation" | "as-built wires" | "clearance windows" | "thermal clearance limits"
        );
        assert_eq!(
            from.changed(&points, uses).is_some(),
            reads_points,
            "{name}"
        );
        assert_eq!(uses.points, reads_points);
        assert!(from.changed(&from, uses).is_none());
    }
    assert_eq!(crate::vegetation::USES, Uses::ALL);
}
