// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The check windows keep their result rows from frame to frame
//! ([`crate::check_ui::CachedRows`], 2026-11-10): a new run, or the result
//! taken away, shows the new rows — never those of the run before.

use super::*;

fn window(model: model::Model) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 1200.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h
}

/// Starts the window's check (`start`, as its Run does) and steps until
/// `answers` says more answers were taken than before, then draws a few
/// frames.
fn run_again(
    h: &mut Harness<'_, App>,
    start: impl Fn(&mut crate::app::Ready, &egui::Context),
    answers: impl Fn(&mut crate::app::Ready) -> (u64, bool),
) {
    h.run();
    let before = answers(h.state_mut().ready().unwrap()).0;
    let ctx = h.ctx.clone();
    start(h.state_mut().ready().unwrap(), &ctx);
    let started = std::time::Instant::now();
    loop {
        h.step();
        let (now, running) = answers(h.state_mut().ready().unwrap());
        if now > before && !running {
            break;
        }
        assert!(
            started.elapsed().as_secs() < 120,
            "no answer in 120 s (answer {now} after {before}, running: {running})"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // More than one frame: the second draws from the rows kept.
    h.run();
    h.run();
}

/// How many cells say exactly `text`.
fn cells(h: &Harness<'_, App>, text: &str) -> usize {
    h.query_all_by_label(text).count()
}

/// Wire tensions: a rule every span fails, then another every span meets;
/// the table shows the second run's rows (its rule's name, its verdicts),
/// and none once the result is taken away.
#[test]
fn wire_tensions_shows_the_rows_of_the_new_run() {
    use ohl_clearance::tension::TensionRule;
    let mut h = window(synthetic());
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-rows-tensions", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let r = h.state_mut().ready().unwrap();
        r.wire_tensions.show(&r.model);
        r.wire_tensions.file = Some(dir.join("t.tension-rules.ohlr"));
        r.wire_tensions.rules = vec![TensionRule {
            factor: 0.01,
            ..TensionRule::blank("first rule")
        }];
    }
    let start = |r: &mut crate::app::Ready, ctx: &egui::Context| {
        r.wire_tensions.start(&r.model, ctx);
    };
    let answers = |r: &mut crate::app::Ready| {
        (
            r.wire_tensions.job.generation,
            r.wire_tensions.job.running(),
        )
    };
    run_again(&mut h, start, answers);
    let n = match &h.state_mut().ready().unwrap().wire_tensions.job.result {
        Some(Ok(rep)) => {
            assert_eq!(rep.report.failures().count(), rep.report.rows.len());
            rep.report.rows.len()
        }
        _ => panic!("no report"),
    };
    assert!(n > 0);
    h.get_by_label(&format!("{n} of {n} rows shown"));
    assert!(cells(&h, "first rule") > 0 && cells(&h, "fail") > 0);
    // Another rule, every span within it: run again.
    h.state_mut().ready().unwrap().wire_tensions.rules = vec![TensionRule {
        factor: 1.0,
        ..TensionRule::blank("second rule")
    }];
    run_again(&mut h, start, answers);
    match &h.state_mut().ready().unwrap().wire_tensions.job.result {
        Some(Ok(rep)) => {
            assert_eq!(rep.report.failures().count(), 0);
            assert_eq!(rep.report.rows.len(), n);
        }
        _ => panic!("no report"),
    }
    // As many rows as before, all others: no violation among them.
    h.get_by_label(&format!("0 of {n} rows shown"));
    crate::check_ui::set_only_violations(&h.ctx, "wire-tension-rows", false);
    h.run();
    h.run();
    h.get_by_label(&format!("{n} of {n} rows shown"));
    assert!(cells(&h, "second rule") > 0 && cells(&h, "pass") > 0);
    assert_eq!(cells(&h, "first rule") + cells(&h, "fail"), 0);
    // The result taken away: no table, no row.
    h.state_mut().ready().unwrap().wire_tensions.job.result = None;
    h.run();
    h.get_by_label("Run to check every span.");
    assert_eq!(cells(&h, "second rule") + cells(&h, "pass"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// String checks: with an invented limit some strings fail, without it
/// the rows are fewer; the table counts the second run's rows.
#[test]
fn string_checks_shows_the_rows_of_the_new_run() {
    use ohl_clearance::strings::{StringKind, StringRule};
    let mut h = window(synthetic());
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-rows-strings", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let r = h.state_mut().ready().unwrap();
        r.string_checks.show(&r.model);
        r.string_checks.file = Some(dir.join("t.string-rules.ohlr"));
        r.string_checks.rules = vec![StringRule {
            strings: StringKind::Suspension,
            max_swing_across_deg: Some(0.1),
            ..StringRule::blank("invented across")
        }];
    }
    let start = |r: &mut crate::app::Ready, ctx: &egui::Context| {
        r.string_checks.start(&r.model, ctx);
    };
    let answers = |r: &mut crate::app::Ready| {
        (
            r.string_checks.job.generation,
            r.string_checks.job.running(),
        )
    };
    let counts =
        |h: &mut Harness<'_, App>| match &h.state_mut().ready().unwrap().string_checks.job.result {
            Some(Ok(rep)) => (rep.report.failures().count(), rep.report.rows.len()),
            _ => panic!("no report"),
        };
    run_again(&mut h, start, answers);
    let (failed, n) = counts(&mut h);
    assert!(failed > 0);
    h.get_by_label(&format!("{failed} of {n} rows shown"));
    assert!(cells(&h, "invented across") > 0);
    // The limit so wide that no string is beyond it, under another name.
    h.state_mut().ready().unwrap().string_checks.rules = vec![StringRule {
        strings: StringKind::Suspension,
        max_swing_across_deg: Some(89.0),
        ..StringRule::blank("wide across")
    }];
    run_again(&mut h, start, answers);
    let (failed_now, n_now) = counts(&mut h);
    assert_eq!(failed_now, 0);
    h.get_by_label(&format!("0 of {n_now} rows shown"));
    crate::check_ui::set_only_violations(&h.ctx, "string-check-rows", false);
    h.run();
    h.run();
    h.get_by_label(&format!("{n_now} of {n_now} rows shown"));
    assert!(cells(&h, "wide across") > 0);
    assert_eq!(cells(&h, "invented across") + cells(&h, "fail"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ice shedding: a distance no pair keeps, then one every pair keeps; the
/// table shows the second run's verdicts, and none once the result is
/// taken away.
#[test]
fn ice_shedding_shows_the_rows_of_the_new_run() {
    use ohl_clearance::rules::Requirement;
    let mut h = window(synthetic_project("rows-ice-shedding"));
    {
        let r = h.state_mut().ready().unwrap();
        r.ice_shedding.show(&r.model);
        r.ice_shedding.rules = ohl_clearance::rules::RuleSet::load(
            &root().join("examples/synthetic-line/ice-shedding-rules.ohlr"),
        )
        .unwrap()
        .ice_shedding_rules;
        for rule in &mut r.ice_shedding.rules {
            rule.distance = Requirement::Fixed(50.0);
        }
        assert_eq!(r.ice_shedding.why_not_run(&r.model), None);
    }
    let start = |r: &mut crate::app::Ready, ctx: &egui::Context| {
        r.ice_shedding.start(&r.model, ctx);
    };
    let answers =
        |r: &mut crate::app::Ready| (r.ice_shedding.job.generation, r.ice_shedding.job.running());
    let counts =
        |h: &mut Harness<'_, App>| match &h.state_mut().ready().unwrap().ice_shedding.job.result {
            Some(Ok(rep)) => (rep.report.failures().count(), rep.report.rows.len()),
            other => panic!("no report: {:?}", other.as_ref().map(|r| r.is_ok())),
        };
    run_again(&mut h, start, answers);
    let (failed, n) = counts(&mut h);
    assert!(n > 0);
    assert_eq!(failed, n, "50 m: no pair keeps it");
    h.get_by_label(&format!("{n} of {n} rows shown"));
    assert!(cells(&h, "fail") > 0);
    assert_eq!(cells(&h, "pass"), 0);
    // A centimetre: every pair keeps it.
    for rule in &mut h.state_mut().ready().unwrap().ice_shedding.rules {
        rule.distance = Requirement::Fixed(0.01);
    }
    run_again(&mut h, start, answers);
    let (failed_now, n_now) = counts(&mut h);
    assert_eq!((failed_now, n_now), (0, n));
    h.get_by_label(&format!("0 of {n} rows shown"));
    crate::check_ui::set_only_violations(&h.ctx, "ice-shedding-rows", false);
    h.run();
    h.run();
    h.get_by_label(&format!("{n} of {n} rows shown"));
    assert!(cells(&h, "pass") > 0);
    assert_eq!(cells(&h, "fail"), 0);
    // The result taken away: no table.
    h.state_mut().ready().unwrap().ice_shedding.job.result = None;
    h.run();
    h.get_by_label("Run to check every span.");
    assert_eq!(cells(&h, "pass"), 0);
}
