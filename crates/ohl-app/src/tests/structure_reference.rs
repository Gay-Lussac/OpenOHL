// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's reference state of the structures in OHL-Design (plan of
//! 2026-11-15, phase 5b): solved and kept by one command, the case drawn
//! with the structures' points moved, the bodies drawn deflected and the
//! 3D view saying so; undone and redone; out of date on an edit and on a
//! drag (not used, said); the As-built wires window comparing with it by
//! default and with the structures as drawn when unticked.

use super::*;

/// Runs frames until `done` (at most 300 s).
fn until(
    harness: &mut egui_kittest::Harness<'_, App>,
    done: impl Fn(&mut crate::app::Ready) -> bool,
) {
    let started = std::time::Instant::now();
    loop {
        harness.run();
        if done(harness.state_mut().ready().unwrap()) {
            return;
        }
        assert!(started.elapsed().as_secs() < 300, "not done in 300 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// The lowest point of span 1 of section S1 in weather case `k`.
fn low(r: &mut crate::app::Ready, k: usize) -> [f64; 3] {
    r.model.ensure_solved(k);
    r.model.cases[k]
        .shapes
        .iter()
        .find(|s| s.section == "S1")
        .unwrap()
        .spans[0]
        .low_point
}

#[test]
fn a_reference_case_solved_drawn_undone_and_stale() {
    let model = typed_project("reference-state");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let ctx = harness.ctx.clone();
    let (k, before) = {
        let r = harness.state_mut().ready().unwrap();
        let k = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "high wind")
            .unwrap();
        assert!(crate::structure_reference::view_note(&r.model).is_none());
        let before = low(r, k);
        r.structure_reference.start(&r.model, &ctx, "high wind");
        (k, before)
    };
    // Solved, kept (one command), checked current, its bodies drawn.
    until(&mut harness, |r| r.model.reference_drawn.is_some());
    let r = harness.state_mut().ready().unwrap();
    let state = r.model.line.reference_in_use().cloned().expect("in use");
    assert_eq!(state.case, "high wind");
    assert!(
        r.model
            .stored_line()
            .unwrap()
            .reference
            .as_ref()
            .is_some_and(|x| x.state.is_some()),
        "kept in the project"
    );
    assert_eq!(r.model.bodies.placed_count(), 3);
    let t2 = r.model.line.structure("T2").unwrap().clone();
    let deflected = crate::scene::drawn_body(&r.model.bodies, &t2).unwrap();
    let straight = r
        .model
        .bodies
        .get(
            t2.structure_type.as_deref().unwrap(),
            t2.extension.as_deref(),
        )
        .unwrap();
    assert_ne!(deflected, straight);
    let (note, warn) = crate::structure_reference::view_note(&r.model).unwrap();
    assert!(!warn);
    assert!(
        note.starts_with(
            "Structures drawn as deflected in case \"high wind\" (calculation level 3)"
        ),
        "{note}"
    );
    // The case shown with the points moved: the wires lean further.
    let after = low(r, k);
    assert!(
        (0..3).any(|c| (after[c] - before[c]).abs() > 1e-3),
        "{before:?} {after:?}"
    );
    // The As-built wires window: the reference state by default, as drawn
    // when unticked — said either way.
    let mut w = crate::wirefit::WireFitWindow::default();
    assert!(w.reference);
    assert!(w.line_compared(&r.model).reference_in_use().is_some());
    w.start(&r.model, &ctx);
    assert!(
        w.reference_note
            .as_deref()
            .unwrap()
            .starts_with("structures as deflected in case \"high wind\"")
    );
    w.reference = false;
    assert!(w.line_compared(&r.model).reference_in_use().is_none());
    w.start(&r.model, &ctx);
    assert!(
        w.reference_note
            .as_deref()
            .unwrap()
            .contains("As-built wires: compared with the structures as drawn"),
        "{:?}",
        w.reference_note
    );

    // Undo: none; the structures as drawn again.
    r.perform(crate::edit::Action::Undo);
    assert!(r.model.line.reference.is_none());
    assert_eq!(low(r, k), before, "as before, bit for bit");
    until(&mut harness, |r| r.model.bodies.placed_count() == 0);
    let r = harness.state_mut().ready().unwrap();
    assert!(crate::structure_reference::view_note(&r.model).is_none());
    // Redo: in use again.
    r.perform(crate::edit::Action::Redo);
    assert!(r.model.line.reference_in_use().is_some());
    until(&mut harness, |r| r.model.bodies.placed_count() == 3);

    // A structure dragged: out of date at once, not used.
    let r = harness.state_mut().ready().unwrap();
    let t3 = r.model.line.structure("T3").unwrap().position;
    r.model.preview_move(k, "T3", [t3[0] + 1.0, t3[1], t3[2]]);
    assert!(r.model.line.reference_in_use().is_none());
    // Let go: one command, the line edited — out of date, said, not used.
    r.perform(crate::edit::Action::Command {
        what: "move".into(),
        command: ohl_session::Command::MoveStructure {
            id: "T3".into(),
            position: [t3[0] + 1.0, t3[1], t3[2]],
        },
    });
    assert!(r.model.line.reference_in_use().is_none());
    let note = r.model.line.reference_note().unwrap();
    assert!(
        note.contains("out of date (the line changed since it was solved)"),
        "{note}"
    );
    until(&mut harness, |r| r.model.bodies.placed_count() == 0);
    let r = harness.state_mut().ready().unwrap();
    let (note, warn) = crate::structure_reference::view_note(&r.model).unwrap();
    assert!(warn && note.contains("out of date"), "{note}");
}

/// Measured foot movements (owner 2026-11-16) entered in the structure
/// window: kept by its Apply (one command, line file version 23), the
/// structure moved by them alone (no reference case: automatically, said),
/// the case's wires following, its body drawn on its moved feet; undone:
/// as before, bit for bit.
#[test]
fn foot_movements_entered_drawn_and_undone() {
    let model = typed_project("foot-movements");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (k, before) = {
        let r = harness.state_mut().ready().unwrap();
        let k = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "everyday")
            .unwrap();
        let before = low(r, k);
        let mut f = crate::editors::StructureForm::of(&r.model, "T2").unwrap();
        f.foot_movements.source = "survey".into();
        f.foot_movements.feet.push(ohl_line::feet::FootMovement {
            joint: "L0B".into(),
            translation: [0.0, 0.0, -0.02],
            ..Default::default()
        });
        let (what, command) = f.command(r.model.stored_line()).unwrap();
        assert!(r.perform(crate::edit::Action::Command { what, command }));
        (k, before)
    };
    until(&mut harness, |r| r.model.bodies.placed_count() == 1);
    let r = harness.state_mut().ready().unwrap();
    let stored = r.model.stored_line().unwrap();
    assert!(stored.has_foot_movements());
    assert!(stored.to_json().contains("\"version\": 23"));
    let state = r.model.line.reference_in_use().cloned().expect("in use");
    assert_eq!(state.level, ohl_loads::reference::FEET_LEVEL);
    assert!(state.points.iter().all(|p| p.structure == "T2"));
    let (note, warn) = crate::structure_reference::view_note(&r.model).unwrap();
    assert!(!warn);
    assert!(
        note.starts_with("Structures drawn on their measured foot movements alone"),
        "{note}"
    );
    let after = low(r, k);
    assert!(
        (0..3).any(|c| (after[c] - before[c]).abs() > 1e-4),
        "{before:?} {after:?}"
    );
    // The window's guidance: towers from their plans.
    assert!(crate::structure_reference::TOWERS_FROM_PLANS.contains("design plans"));
    // Undo: none; as drawn, bit for bit.
    r.perform(crate::edit::Action::Undo);
    assert!(!r.model.line.has_foot_movements());
    assert!(r.model.line.reference_in_use().is_none());
    assert_eq!(low(r, k), before, "as before, bit for bit");
    until(&mut harness, |r| r.model.bodies.placed_count() == 0);
    let r = harness.state_mut().ready().unwrap();
    assert!(crate::structure_reference::view_note(&r.model).is_none());
}
