// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Bringing changes into a project line, as its windows show it since the
//! version-management review of 2026-11-10 ([`crate::merge_conflicts`]):
//! the Merge page, Versions › Bring in changes…, a review's merge and a
//! revision's revert — the same list of conflicts in readable terms, the
//! choices kept through a refusal, a preview before anything is applied.

use super::*;
use crate::merge_conflicts::{Plan, Settling, Source};
use crate::project_lines::LinesWindow;
use ohl_server::Op;
use ohl_session::versions::Side;

fn op(r: &mut crate::app::Ready, what: &str, op: Op) {
    r.project_op(what, op);
    assert!(!r.status.as_ref().unwrap().1, "{what}: {:?}", r.status);
}

fn commit(r: &mut crate::app::Ready, message: &str) {
    op(
        r,
        "committed",
        Op::Commit {
            message: message.into(),
        },
    );
}

fn work_on(r: &mut crate::app::Ready, line: &str) {
    r.switch_line(line);
    for k in 0..r.model.cases.len() {
        r.model.ensure_solved(k);
    }
}

fn position(r: &crate::app::Ready, id: &str) -> [f64; 3] {
    r.model.line.structure(id).unwrap().position
}

fn move_by(r: &mut crate::app::Ready, id: &str, d: [f64; 3]) {
    let p = position(r, id);
    assert!(
        r.perform(crate::edit::Action::Command {
            what: id.into(),
            command: ohl_session::Command::MoveStructure {
                id: id.into(),
                position: [p[0] + d[0], p[1] + d[1], p[2] + d[2]],
            },
        }),
        "{:?}",
        r.status
    );
}

/// A line made from main as it is, T3 moved by `d` on it and committed;
/// main worked on again.
fn line_with_t3_moved(r: &mut crate::app::Ready, name: &str, d: [f64; 3]) {
    op(
        r,
        "line",
        Op::CreateLine {
            name: name.into(),
            from: "main".into(),
        },
    );
    work_on(r, name);
    move_by(r, "T3", d);
    commit(r, &format!("T3 moved on {name}"));
    work_on(r, "main");
}

fn harness_of(name: &str) -> Harness<'static, App> {
    let model = synthetic_project(name);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness
}

/// The plan a window last worked out.
fn plan(st: &Settling) -> &Plan {
    st.plan().expect("worked out").as_ref().expect("a plan")
}

fn shown(harness: &Harness<'_, App>, text: &str) -> usize {
    harness.query_all_by_label_contains(text).count()
}

/// The button `label` lowest in the window (the Merge page's tab and its
/// button are both "Merge").
fn click_lowest(harness: &mut Harness<'_, App>, label: &str) {
    harness
        .query_all_by_label(label)
        .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap_or_else(|| panic!("no \"{label}\""))
        .click();
    harness.run();
    harness.run();
}

/// The Merge page: a conflict reads in metres, not as stored JSON; what
/// was settled is dropped when another line is previewed, and kept when
/// the merge is refused (here: work not committed, said by the session
/// when Merge is clicked) — committed, the same choices merge.
#[test]
fn the_merge_page_keeps_choices_through_a_refusal_and_drops_them_for_another_line() {
    let mut harness = harness_of("merge-page-choices");
    let start = {
        let r = harness.state_mut().ready().unwrap();
        commit(r, "start");
        let start = position(r, "T3");
        line_with_t3_moved(r, "alt", [4.0, 0.0, 0.0]);
        // (And a change of alt's alone: it combines by itself.)
        work_on(r, "alt");
        move_by(r, "T5", [0.0, 0.5, 0.0]);
        commit(r, "T5 north on alt");
        work_on(r, "main");
        line_with_t3_moved(r, "beta", [-2.0, 0.0, 0.0]);
        move_by(r, "T3", [0.0, 1.0, 0.0]);
        commit(r, "T3 north on main");
        r.versions.open = true;
        r.versions.other = "alt".into();
        start
    };
    harness.run();
    harness.get_by_label("Merge").click();
    harness.run();
    // The wording: project lines, not branches; nothing of ours/theirs.
    assert_eq!(shown(&harness, "branch"), 0);
    harness.get_by_label("Preview").click();
    harness.run();
    harness.run();
    assert!(shown(&harness, "Bring changes of alt into main") > 0);
    assert_eq!(shown(&harness, "ours") + shown(&harness, "theirs"), 0);
    // Readable: this line's move and alt's, in metres.
    assert!(shown(&harness, "moved 1.0 m north") > 0);
    assert!(shown(&harness, "moved 4.0 m east") > 0);
    assert!(shown(&harness, "still to settle") > 0);
    // What the merge would change is listed before it is made.
    assert!(shown(&harness, "What it changes in main") > 0);
    harness.get_by_label("All: keep this line's").click();
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let st = r.versions.merge.as_ref().unwrap();
        assert!(!st.choices.is_empty());
        assert_eq!(plan(st).open, 0);
        // Another line previewed: nothing of alt's choices settles it.
        r.versions.other = "beta".into();
    }
    harness.get_by_label("Preview").click();
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let st = r.versions.merge.as_ref().unwrap();
        assert!(st.choices.is_empty(), "{:?}", st.choices);
        assert!(plan(st).open > 0, "beta's conflict arrived settled");
        r.versions.other = "alt".into();
    }
    harness.get_by_label("Preview").click();
    harness.run();
    harness.get_by_label("All: keep this line's").click();
    harness.run();
    harness.run();
    // Work not committed that the window has not seen yet (as when
    // somebody else's edit arrives): the session refuses the merge.
    let settled = {
        let r = harness.state_mut().ready().unwrap();
        let p = position(r, "T2");
        let e = r.model.editing.as_mut().unwrap();
        e.session
            .apply(
                "tester",
                ohl_session::Command::MoveStructure {
                    id: "T2".into(),
                    position: [p[0] + 1.0, p[1], p[2]],
                },
            )
            .unwrap();
        r.versions.merge.as_ref().unwrap().choices.clone()
    };
    click_lowest(&mut harness, "Merge");
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.status.as_ref().unwrap().1, "refused: {:?}", r.status);
        let st = r.versions.merge.as_ref().expect("still being settled");
        assert_eq!(st.choices, settled, "the choices survive the refusal");
        assert!(st.error.as_deref().unwrap().contains("uncommitted"));
        assert_eq!(position(r, "T3")[1], start[1] + 1.0, "nothing changed");
    }
    // Said in the window, with the way out.
    assert!(shown(&harness, "Not applied") > 0);
    assert!(shown(&harness, "Commit first") > 0);
    // Committed: the same choices merge.
    commit(harness.state_mut().ready().unwrap(), "T2 east");
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let st = r.versions.merge.as_ref().unwrap();
        assert_eq!(st.choices, settled);
        assert!(plan(st).ready().is_ok(), "{:?}", plan(st).ready());
    }
    click_lowest(&mut harness, "Merge");
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert!(
        r.versions.merge.is_none(),
        "merged: the choices are done with"
    );
    let p = position(r, "T3");
    assert_eq!(
        (p[0], p[1]),
        (start[0], start[1] + 1.0),
        "this line's T3 kept"
    );
    assert_eq!(
        position(r, "T5")[1],
        r.model
            .session()
            .unwrap()
            .line_named("alt")
            .unwrap()
            .structure("T5")
            .unwrap()
            .position[1],
        "alt's own change came in"
    );
    let s = r.model.session().unwrap();
    let head = s.revision(&s.head_revision().unwrap().unwrap()).unwrap();
    assert_eq!(head.parents.len(), 2, "a merge revision: {}", head.message);
}

/// Before anything is previewed the Merge page says that uncommitted
/// work is refused, with Commit first (the History page, message ready).
#[test]
fn the_merge_page_says_uncommitted_work_first() {
    let mut harness = harness_of("merge-page-uncommitted");
    {
        let r = harness.state_mut().ready().unwrap();
        commit(r, "start");
        move_by(r, "T3", [1.0, 0.0, 0.0]);
        r.versions.open = true;
    }
    harness.run();
    harness.get_by_label("Merge").click();
    harness.run();
    assert!(shown(&harness, "main has uncommitted changes") > 0);
    harness.get_by_label("Commit first").click();
    harness.run();
    harness.run();
    // The History page: its Commit button and message field.
    assert!(harness.query_by_label("Commit").is_some());
    assert_eq!(shown(&harness, "Commit first"), 0);
}

/// Bring in changes: keep both is offered only for the object each line
/// made under one id, and bringing in with it leaves both; closing with
/// choices made asks first.
#[test]
fn bring_in_offers_keep_both_where_both_made_an_object_and_asks_before_closing() {
    let mut harness = harness_of("bring-in-keep-both");
    let add = |r: &mut crate::app::Ready, dx: f64| {
        let mut s = r.model.line.structure("T3").unwrap().clone();
        s.id = "TX".into();
        s.position[0] += dx;
        s.description = format!("made {dx} m east of T3");
        assert!(
            r.perform(crate::edit::Action::Command {
                what: "TX".into(),
                command: ohl_session::Command::Put {
                    kind: ohl_session::Kind::Structure,
                    id: "TX".into(),
                    value: serde_json::to_value(&s).unwrap(),
                    ord: None,
                },
            }),
            "{:?}",
            r.status
        );
    };
    {
        let r = harness.state_mut().ready().unwrap();
        commit(r, "start");
        op(
            r,
            "line",
            Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
        // Main: T3 moved, a structure TX made.
        move_by(r, "T3", [0.0, 1.0, 0.0]);
        add(r, 12.0);
        // Phase 1: T3 moved another way, its own TX.
        work_on(r, "phase 1");
        move_by(r, "T3", [3.0, 0.0, 0.0]);
        add(r, 15.0);
        r.lines_window = Some(LinesWindow::bring_in("main"));
    }
    harness.run();
    harness.run();
    assert!(shown(&harness, "Bring changes of main into phase 1") > 0);
    let other_id = {
        let r = harness.state_mut().ready().unwrap();
        let Some(LinesWindow::BringIn(st)) = &r.lines_window else {
            panic!("{:?}", r.lines_window);
        };
        let p = plan(st);
        assert!(p.objects.len() >= 2, "{:?}", p.objects);
        let both: Vec<_> = p.objects.iter().filter(|o| o.keep_both.is_some()).collect();
        assert_eq!(both.len(), 1, "{:?}", p.objects);
        assert_eq!(both[0].key, "structure:TX");
        assert_eq!(both[0].summary, "made on both under this id");
        both[0].keep_both.clone().unwrap()
    };
    // Show: the object selected, the views brought to it.
    harness
        .query_all_by_label("Show")
        .next()
        .expect("a Show per object")
        .click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(LinesWindow::BringIn(st)) = &r.lines_window else {
            panic!("{:?}", r.lines_window);
        };
        assert_eq!(Some(&r.selection), plan(st).objects[0].target.as_ref());
        assert_ne!(r.selection, crate::selection::Selection::None);
    }
    // One radio "keep both" for all the objects in conflict.
    assert_eq!(harness.query_all_by_label("keep both").count(), 1);
    assert!(shown(&harness, "moved 3.0 m east") > 0);
    harness.get_by_label("All: take main's").click();
    harness.run();
    // (Its row may be below what the list shows at once.)
    harness.get_by_label("keep both").click_accesskit();
    harness.run();
    harness.run();
    // Esc with choices made: asked, nothing dropped.
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    harness.run();
    assert!(shown(&harness, "choice(s) made here are dropped") > 0);
    harness.get_by_label("Keep settling").click();
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(LinesWindow::BringIn(st)) = &r.lines_window else {
            panic!("closed: {:?}", r.lines_window);
        };
        assert_eq!(st.choices.get("structure:TX"), Some(&Side::Both));
        assert_eq!(plan(st).open, 0);
    }
    harness.get_by_label("Bring in").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert!(r.lines_window.is_none());
    // Both kept: this line's TX as it was, main's under its new id.
    let mine = r.model.line.structure("TX").expect("this line's");
    assert_eq!(mine.description, "made 15 m east of T3");
    let theirs = r.model.line.structure(&other_id).expect("main's, renamed");
    assert_eq!(theirs.description, "made 12 m east of T3");
    assert_ne!(other_id, "TX");
}

/// Revert asks first: its window shows what it would undo; where the
/// object was changed again since, the conflict is settled there and the
/// revert made.
#[test]
fn a_revert_with_a_conflict_is_settled_and_applied() {
    let mut harness = harness_of("revert-conflict");
    let (start, east) = {
        let r = harness.state_mut().ready().unwrap();
        commit(r, "start");
        let start = position(r, "T3");
        move_by(r, "T3", [4.0, 0.0, 0.0]);
        commit(r, "T3 east");
        let east = r.model.session().unwrap().head_revision().unwrap().unwrap();
        move_by(r, "T3", [0.0, 1.0, 0.0]);
        commit(r, "T3 north");
        r.versions.open = true;
        (start, east)
    };
    harness.run();
    harness.run();
    // The History page's button opens the question; nothing is reverted.
    harness
        .query_all_by_label("Revert…")
        .next()
        .expect("a Revert… per revision")
        .click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(
            matches!(
                r.versions.settle.as_ref().map(|s| &s.source),
                Some(Source::Revert { .. })
            ),
            "{:?}",
            r.versions.settle
        );
        assert_eq!(position(r, "T3")[0], start[0] + 4.0, "not reverted yet");
        // "T3 east": the object was moved again after it.
        r.versions.settle = Some(Settling::new(Source::Revert {
            revision: east.clone(),
        }));
        r.versions.open = false;
    }
    harness.run();
    harness.run();
    let short = ohl_session::versions::short(&east).to_owned();
    assert!(shown(&harness, &format!("Undo revision {short} on main")) > 0);
    assert!(shown(&harness, "still to settle") > 0);
    assert!(shown(&harness, "moved 1.0 m north") > 0);
    assert!(shown(&harness, "moved 4.0 m west") > 0);
    {
        let r = harness.state_mut().ready().unwrap();
        let st = r.versions.settle.as_ref().unwrap();
        assert_eq!(
            plan(st).ready(),
            Err("Choose a side for every conflict first")
        );
    }
    harness
        .get_by_label(&format!("All: back to before {short}"))
        .click();
    harness.run();
    harness.run();
    harness.get_by_label("Revert").click();
    harness.run();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert!(r.versions.settle.is_none(), "applied: the window is closed");
    let p = position(r, "T3");
    assert_eq!((p[0], p[1]), (start[0], start[1]), "as before \"T3 east\"");
    let s = r.model.session().unwrap();
    let head = s.revision(&s.head_revision().unwrap().unwrap()).unwrap();
    assert!(head.message.starts_with("Revert "), "{}", head.message);
}

/// A review whose line conflicts with its target is settled in its
/// window and merged into the target, whichever line is worked on.
#[test]
fn a_review_with_a_conflict_is_settled_and_merged_into_its_target() {
    let mut harness = harness_of("review-conflict");
    let (start, id) = {
        let r = harness.state_mut().ready().unwrap();
        commit(r, "start");
        let start = position(r, "T3");
        line_with_t3_moved(r, "phase 1", [4.0, 0.0, 0.0]);
        move_by(r, "T3", [0.0, 1.0, 0.0]);
        commit(r, "T3 north on main");
        op(
            r,
            "submitted",
            Op::SubmitReview {
                branch: "phase 1".into(),
                target: "main".into(),
                reviewers: vec![],
                note: String::new(),
            },
        );
        let id = r.model.session().unwrap().reviews().unwrap()[0].id;
        // The one merging works on another line than the target.
        work_on(r, "phase 1");
        r.versions.settle = Some(Settling::new(Source::Review { id }));
        (start, id)
    };
    harness.run();
    harness.run();
    assert!(
        shown(
            &harness,
            &format!("Bring changes of phase 1 into main (review #{id})")
        ) > 0
    );
    assert!(shown(&harness, "moved 4.0 m east") > 0);
    {
        let r = harness.state_mut().ready().unwrap();
        let st = r.versions.settle.as_ref().unwrap();
        assert!(plan(st).open > 0);
        assert!(!plan(st).here, "main is not the line worked on");
    }
    harness.get_by_label("All: take phase 1's").click();
    harness.run();
    harness.run();
    harness.get_by_label("Merge").click();
    harness.run();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert!(r.versions.settle.is_none());
    let s = r.model.session().unwrap();
    assert_eq!(
        s.review(id).unwrap().state,
        ohl_session::collab::ReviewState::Merged
    );
    let p = s
        .line_named("main")
        .unwrap()
        .structure("T3")
        .unwrap()
        .position;
    assert_eq!((p[0], p[1]), (start[0] + 4.0, start[1]), "phase 1's T3");
}
