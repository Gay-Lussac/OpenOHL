// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: what the views asked on every frame is made once per version
//! of what it is made from (owner, 2026-11-10), and is what was made
//! before — the old computation is kept here to compare with.

use super::*;

/// The example line with both wind sides solved and the labels ticked.
fn both_sides() -> model::Model {
    let mut m = synthetic();
    m.set_wind(model::WindDisplay::Both);
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    assert!(m.cases.iter().any(|c| c.mirror.is_some()));
    m
}

/// The alignment views are worked out once per version of the line and
/// are the line's.
#[test]
fn alignment_views_are_made_once_per_line_version() {
    let mut m = synthetic();
    let made = || model::ALIGNMENT_VIEWS_MADE.with(std::cell::Cell::get);
    let before = made();
    let first = m.alignment_views();
    for _ in 0..4 {
        assert!(std::sync::Arc::ptr_eq(&first, &m.alignment_views()));
    }
    assert_eq!(made(), before + 1);
    assert_eq!(*first, *m.line.alignment_views());
    assert!(!first.is_empty());
    // The line changed (an edit counts its version up).
    let moved = m.line.structures[1].id.clone();
    m.line.structures[1].position[0] += 3.0;
    m.line_version += 1;
    let again = m.alignment_views();
    assert_eq!(made(), before + 2);
    assert_eq!(*again, *m.line.alignment_views());
    assert!(again.iter().any(|v| v.structures.contains(&moved)));
    m.alignment_views();
    assert_eq!(made(), before + 2);
}

/// [`crate::selection::string_pieces`] as it was made until 2026-11-10.
fn string_pieces_before(model: &model::Model, case: usize) -> crate::selection::StringPieces {
    let mut out = crate::selection::StringPieces::new();
    for solved in model.cases.get(case).into_iter().flat_map(|c| c.drawn()) {
        for shape in &solved.shapes {
            let Some(sec) = model.line.sections.iter().find(|s| s.id == shape.section) else {
                continue;
            };
            for (p, strings) in sec.points.iter().zip(&shape.strings) {
                for x in strings {
                    out.entry((p.structure.clone(), x.label.clone()))
                        .or_default()
                        .push((x.top, x.bottom));
                }
            }
        }
        for j in &solved.jumpers {
            for s in &j.supports {
                for x in &s.strings {
                    out.entry((s.structure.clone(), x.label.clone()))
                        .or_default()
                        .push((x.top, s.clamp));
                }
            }
        }
    }
    out
}

/// Where the strings are drawn is made once per weather case, version of
/// the line and solution of the case (both wind sides), and is what was
/// made on every frame before.
#[test]
fn string_pieces_are_made_once_per_case_and_solution() {
    let mut m = both_sides();
    let made = || crate::selection::PIECES_MADE.with(std::cell::Cell::get);
    let before = made();
    let n = m.cases.len();
    assert!(n >= 2);
    for _ in 0..3 {
        for k in 0..n {
            let pieces = crate::selection::string_pieces(&m, k);
            assert_eq!(*pieces, string_pieces_before(&m, k), "case {k}");
        }
    }
    assert_eq!(made(), before + n);
    assert!(!crate::selection::string_pieces(&m, 0).is_empty());
    // A case no model has: nothing drawn.
    assert!(crate::selection::string_pieces(&m, n).is_empty());
    let before = made();
    // A case solved again: its pieces again, the others' kept.
    let windy = m.cases.iter().position(|c| c.mirror.is_some()).unwrap();
    let case = m.cases[windy].case.clone();
    m.cases[windy] = model::Solved::with_wind(&m.line, &m.library, case, m.wind);
    for k in 0..n {
        assert_eq!(
            *crate::selection::string_pieces(&m, k),
            string_pieces_before(&m, k)
        );
    }
    assert_eq!(made(), before + 1);
    // Its other wind side dropped (the line's version counts up with
    // it): one side's pieces, all made again.
    let both = crate::selection::string_pieces(&m, windy);
    m.set_wind(model::WindDisplay::Right);
    for k in 0..n {
        m.ensure_solved(k);
    }
    let one = crate::selection::string_pieces(&m, windy);
    assert_eq!(*one, string_pieces_before(&m, windy));
    assert_ne!(*one, *both);
    // The line changed: every case again.
    let before = made();
    m.line_version += 1;
    for _ in 0..2 {
        for k in 0..n {
            crate::selection::string_pieces(&m, k);
        }
    }
    assert_eq!(made(), before + n);
}

/// [`model::Model::shapes_shown_ref`] as it was made until 2026-11-10.
fn shapes_shown_before(m: &model::Model, case: usize) -> Vec<&ohl_line::solve::SectionShape> {
    m.line
        .sections
        .iter()
        .flat_map(|s| {
            m.section_cases(&s.id, case)
                .into_iter()
                .filter_map(|k| m.cases.get(k))
                .flat_map(|c| c.drawn())
                .filter_map(|c| c.shapes.iter().find(|x| x.section == s.id))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The shapes shown (each section in each of its display cases, both
/// wind sides) are the same shapes in the same order as before: the
/// cases shown for the line, a section's own, a case with a section not
/// solved, a case out of range.
#[test]
fn shapes_shown_are_those_found_before() {
    let mut m = both_sides();
    let same = |m: &model::Model, case: usize, what: &str| {
        let (now, before) = (m.shapes_shown_ref(case), shapes_shown_before(m, case));
        assert_eq!(now.len(), before.len(), "{what}");
        for (a, b) in now.iter().zip(&before) {
            assert!(
                std::ptr::eq(*a, *b),
                "{what}: {} / {}",
                a.section,
                b.section
            );
        }
        now.len()
    };
    let windy = m.cases.iter().position(|c| c.mirror.is_some()).unwrap();
    let still = m.cases.iter().position(|c| c.mirror.is_none()).unwrap();
    // (No case shown for the line: the one given.)
    m.shown_cases = Vec::new();
    let one = same(&m, still, "one case");
    assert!(one > 1);
    assert_eq!(same(&m, windy, "both wind sides"), 2 * one);
    m.shown_cases = vec![still, windy, 99];
    assert_eq!(same(&m, 0, "two cases shown"), 3 * one);
    // A section's own display cases.
    let id = m.cases[still].shapes[1].section.clone();
    m.cases_shown = model::CasesShown::PerSection;
    m.display_cases.insert(id.clone(), vec![windy]);
    assert_eq!(same(&m, 0, "a section's own case"), 3 * one - 1);
    // A section not solved in a case, the shapes in another order than
    // the sections: left out there, as before.
    m.cases[still].shapes.retain(|s| s.section != id);
    m.cases[still].shapes.reverse();
    m.display_cases.insert(id, vec![still, windy]);
    assert_eq!(same(&m, 0, "a section not solved"), 3 * one - 1);
    m.shown_cases = vec![still];
    m.display_cases.clear();
    assert_eq!(same(&m, 0, "back to one case"), one - 1);
}

/// The notes of the spans chosen by section and span are those of all
/// the spans, chosen afterwards; the objects' labels without the
/// structures' names are the labels with the names taken out; the
/// structures' tops are found once per version of the line.
#[test]
fn notes_are_those_made_before() {
    use crate::annotations::{self, Kind, View};
    let mut m = both_sides();
    let d = &mut m.section_display;
    for v in [
        View::Plan as usize,
        View::Profile as usize,
        View::ThreeD as usize,
    ] {
        d.label[v] = true;
        d.sag[v] = true;
        d.structure_label[v] = true;
        d.point_label[v] = true;
        d.string_label[v] = true;
        d.insulator_label[v] = true;
        d.library_label[v] = true;
        d.wire_label[v] = true;
        d.alignment_point_label[v] = true;
        d.descriptions[v] = true;
    }
    for case in 0..m.cases.len() {
        // Every other span, by section and span.
        let all = annotations::notes(&m, case, View::Profile, None);
        assert!(!all.is_empty());
        let wanted = |section: &str, span: usize| (span + section.len()).is_multiple_of(2);
        let chosen = annotations::notes_of(&m, case, View::Profile, |shape, k| {
            wanted(&shape.section, k)
        });
        let after: Vec<_> = all
            .iter()
            .filter(|n| wanted(&n.section, n.span))
            .cloned()
            .collect();
        assert_eq!(chosen, after);
        assert!(!chosen.is_empty() && chosen.len() < all.len());
        // By the span's points, as the plan and the 3D view ask.
        let east = all[all.len() / 2].at[0];
        let by_place =
            annotations::notes_where(&m, case, View::Plan, |_, mid: [f64; 3]| mid[0] < east);
        let direct: Vec<_> = annotations::notes(&m, case, View::Plan, None)
            .into_iter()
            .filter(|n| n.at[0] < east)
            .collect();
        // (A label is kept by its section's middle span.)
        assert_eq!(
            by_place
                .iter()
                .filter(|n| n.kind == Kind::Sag)
                .collect::<Vec<_>>(),
            direct
                .iter()
                .filter(|n| n.kind == Kind::Sag)
                .collect::<Vec<_>>()
        );
        // The objects' labels.
        for near in [None, Some((m.line.structures[1].position, 150.0))] {
            let named = annotations::object_notes(&m, case, View::Plan, near);
            let unnamed = annotations::object_notes_unnamed(&m, case, View::Plan, near);
            let names = named.iter().filter(|n| n.kind == Kind::Structure).count();
            assert_eq!(names, m.line.structures.len());
            assert!(!unnamed.is_empty());
            assert_eq!(
                unnamed,
                named
                    .into_iter()
                    .filter(|n| n.kind != Kind::Structure)
                    .collect::<Vec<_>>()
            );
        }
    }
    // The names' heights: each structure's highest point, 5 m above its
    // foot at least, 2 m above that; found once per version of the line.
    let top = |s: &ohl_line::Structure| {
        s.points
            .iter()
            .map(|p| p.position[2])
            .chain(s.members.iter().flatten().map(|p| p[2]))
            .fold(s.position[2] + 5.0, f64::max)
    };
    let made = || annotations::TOPS_MADE.with(std::cell::Cell::get);
    let heights = |m: &model::Model| -> Vec<f64> {
        annotations::object_notes(m, 0, View::ThreeD, None)
            .iter()
            .filter(|n| n.kind == Kind::Structure)
            .map(|n| n.at[2])
            .collect()
    };
    let direct =
        |m: &model::Model| -> Vec<f64> { m.line.structures.iter().map(|s| top(s) + 2.0).collect() };
    m.line_version += 1;
    let before = made();
    for _ in 0..4 {
        assert_eq!(heights(&m), direct(&m));
    }
    assert_eq!(made(), before + 1);
    // Only the names off: the tops are not asked for.
    m.section_display.structure_label = [false; 3];
    m.line_version += 1;
    assert!(heights(&m).is_empty());
    assert_eq!(made(), before + 1);
    m.section_display.structure_label = [true; 3];
    // A structure raised (the line's version with it): found again.
    m.line.structures[0].position[2] += 40.0;
    m.line_version += 1;
    assert_eq!(heights(&m), direct(&m));
    assert_eq!(made(), before + 2);
}
