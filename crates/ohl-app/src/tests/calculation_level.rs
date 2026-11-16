// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's calculation level (plan of 2026-11-15, phase 5): Line ›
//! Criteria › Calculation options' **Structures in the wire solve** — one
//! command, undoable, kept in the line's settings, every result out of
//! date when it changes; full structures (level 4) offered too — and the
//! Wire tensions and String checks windows solving at levels 3 and 4:
//! both solves of every case, the worse kept, the results saying the
//! level, the run's time and effect kept for the window.

use super::*;

/// Criteria › Calculation options: full structures chosen, then linear
/// stiffness; the time beside each choice; one command, undoable; the
/// revision changed and back.
#[test]
fn calculation_options_choose_the_linear_stiffness() {
    let model = typed_project("structures-in-solve");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let before = harness.state_mut().ready().unwrap().model.revision();
    crate::tests::line_menu(&mut harness, "Criteria");
    harness.run();
    harness.get_by_label("Calculation options…").click();
    harness.run();
    // Each choice says its time (measured here, else an estimate): level
    // 4's too.
    assert!(
        harness
            .query_all_by_label_contains("each with a full solve of every flexible structure")
            .next()
            .is_some()
    );
    // (Its place settles over a few frames as the window takes its size.)
    for _ in 0..8 {
        harness
            .get_by_label("Linear stiffness (calculation level 3)")
            .scroll_to_me();
        harness.run();
    }
    harness
        .get_by_label("Linear stiffness (calculation level 3)")
        .click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .solver_window
            .unwrap()
            .structures,
        ohl_line::StructuresInSolve::Linear,
        "chosen"
    );
    // Full structures: offered (then back to linear).
    for _ in 0..8 {
        harness
            .get_by_label("Full structures (calculation level 4)")
            .scroll_to_me();
        harness.run();
    }
    harness
        .get_by_label("Full structures (calculation level 4)")
        .click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .solver_window
            .unwrap()
            .structures,
        ohl_line::StructuresInSolve::Full,
        "chosen"
    );
    for _ in 0..8 {
        harness
            .get_by_label("Linear stiffness (calculation level 3)")
            .scroll_to_me();
        harness.run();
    }
    harness
        .get_by_label("Linear stiffness (calculation level 3)")
        .click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .solver_window
            .unwrap()
            .structures,
        ohl_line::StructuresInSolve::Linear,
        "chosen"
    );
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model.line.solver.structures,
        ohl_line::StructuresInSolve::Linear
    );
    let stored = r.model.stored_line().unwrap();
    assert_eq!(
        stored.solver.structures,
        ohl_line::StructuresInSolve::Linear
    );
    assert_eq!(
        before
            .changed(&r.model.revision(), crate::revision::Uses::LINE)
            .as_deref(),
        Some("the line")
    );
    r.perform(crate::edit::Action::Undo);
    assert_eq!(
        r.model.line.solver.structures,
        ohl_line::StructuresInSolve::Rigid
    );
    assert_eq!(r.model.revision(), before, "back as it was");
}

/// Waits for a check window's job.
fn wait<T: Send + 'static>(job: &mut crate::job::Job<T>) -> Result<T, String> {
    let started = std::time::Instant::now();
    loop {
        job.poll();
        if let Some(r) = job.result.take() {
            return r;
        }
        assert!(started.elapsed().as_secs() < 300, "no answer in 300 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Wire tensions at level 3 on the example line (invented factor 1.0, the
/// high wind): every span checked once with both solves' tensions, the
/// larger checked; the notes say the level, the reference, the rigid
/// structures and the rule; the run kept for Calculation options. Rigid:
/// no level at all. Full structures (level 4): the same rows, never below
/// rigid, its notes and its run kept.
#[test]
fn wire_tensions_at_level_three_keep_the_larger_tension() {
    use ohl_clearance::tension::TensionRule;
    let mut model = typed_project("tensions-level3");
    let ctx = egui::Context::default();
    let run = |model: &model::Model| {
        let mut w = crate::wire_tensions::WireTensionsWindow::default();
        w.rules = vec![TensionRule {
            factor: 1.0,
            cases: vec!["high wind".into(), "glaze ice".into()],
            ..TensionRule::blank("invented 1.0")
        }];
        w.start(model, &ctx);
        wait(&mut w.job)
    };
    let rigid = run(&model).unwrap();
    assert!(rigid.report.rows.iter().all(|r| r.levels.is_none()));
    assert!(
        !rigid
            .report
            .notes
            .iter()
            .any(|n| n.contains("calculation level"))
    );
    model.line.solver.structures = ohl_line::StructuresInSolve::Linear;
    let three = run(&model).unwrap();
    assert_eq!(three.report.rows.len(), rigid.report.rows.len());
    let mut flexible_governs = 0;
    for r in &three.report.rows {
        let [a, b] = r.levels.expect("both solves");
        assert_eq!(r.tension, a.max(b));
        let same = rigid
            .report
            .rows
            .iter()
            .find(|x| x.case == r.case && x.section == r.section && x.span == r.span)
            .unwrap();
        assert!(
            (same.tension - a).abs() <= 1e-6 * a.abs().max(1.0),
            "{} {a}",
            same.tension
        );
        assert!(r.tension >= same.tension, "never below rigid");
        flexible_governs += usize::from(r.level() == 3);
    }
    let notes = three.report.notes.join("\n");
    assert!(notes.contains("calculation level 3"), "{notes}");
    assert!(
        notes.contains("structures as drawn are the structures as strung"),
        "{notes}"
    );
    assert!(notes.contains("never less safe than rigid"), "{notes}");
    assert!(notes.contains("3 structure(s) flexible"), "{notes}");
    assert!(notes.contains("change against rigid"), "{notes}");
    let last = crate::calc_level::last(3).expect("recorded");
    assert!(last.what.starts_with("Wire tensions"), "{}", last.what);
    assert!(last.setup_seconds.is_some());
    // Said for the record: how many spans the level-3 solve governs.
    eprintln!(
        "level 3 governs {flexible_governs} of {} rows; {}",
        three.report.rows.len(),
        last.effect.unwrap_or_default()
    );
    model.line.solver.structures = ohl_line::StructuresInSolve::Full;
    let four = run(&model).unwrap();
    assert_eq!(four.report.rows.len(), rigid.report.rows.len());
    for r in &four.report.rows {
        let [a, b] = r.levels.expect("both solves");
        assert_eq!(r.tension, a.max(b));
        let same = rigid
            .report
            .rows
            .iter()
            .find(|x| x.case == r.case && x.section == r.section && x.span == r.span)
            .unwrap();
        assert!(r.tension >= same.tension, "never below rigid");
    }
    let notes = four.report.notes.join("\n");
    assert!(notes.contains("calculation level 4"), "{notes}");
    assert!(notes.contains("level-4 solve of its case"), "{notes}");
    assert!(!notes.contains("failed at calculation level 4"), "{notes}");
    let last = crate::calc_level::last(4).expect("recorded");
    assert!(last.what.starts_with("Wire tensions"), "{}", last.what);
    eprintln!("level 4: {}", last.effect.unwrap_or_default());
}

/// String checks at level 3 on the example line (invented limits): each
/// string's swing and load from both solves, the row of the larger usage
/// kept with both values; the swing from the moved top, the tilt left
/// out, said.
#[test]
fn string_checks_at_level_three_keep_the_worse_row() {
    use ohl_clearance::strings::{StringKind, StringRule};
    let mut model = typed_project("strings-level3");
    let ctx = egui::Context::default();
    let run = |model: &model::Model| {
        let mut w = crate::string_checks::StringChecksWindow::default();
        w.rules = vec![StringRule {
            strings: StringKind::Suspension,
            max_swing_across_deg: Some(60.0),
            cases: vec!["high wind".into()],
            ..StringRule::blank("invented 60° across")
        }];
        w.start(model, &ctx);
        wait(&mut w.job)
    };
    let rigid = run(&model).unwrap();
    assert!(!rigid.report.rows.is_empty());
    model.line.solver.structures = ohl_line::StructuresInSolve::Linear;
    let three = run(&model).unwrap();
    assert_eq!(three.report.rows.len(), rigid.report.rows.len());
    for r in &three.report.rows {
        let [a, b] = r.levels.expect("both solves");
        let same = rigid
            .report
            .rows
            .iter()
            .find(|x| {
                x.structure == r.structure
                    && x.string == r.string
                    && x.case == r.case
                    && x.measure == r.measure
                    && x.source == r.source
                    && x.rule == r.rule
            })
            .unwrap();
        assert!(
            (same.value - a).abs() <= 1e-6 * a.abs().max(1.0),
            "{} {a}",
            same.value
        );
        assert!(r.usage >= same.usage - 1e-12, "never less safe than rigid");
        assert_eq!(r.value, if r.level == 3 { b } else { a });
    }
    let notes = three.report.notes.join("\n");
    assert!(notes.contains("calculation level 3"), "{notes}");
    assert!(notes.contains("tilt at the point left out"), "{notes}");
}
