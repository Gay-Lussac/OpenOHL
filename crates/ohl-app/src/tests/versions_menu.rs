// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Versions menu as laid out after the version-management review of
//! 2026-11-10: its entries in order, the line worked on and its
//! uncommitted changes (menu and status bar), the offline copy.

use super::*;

fn commit(r: &mut crate::app::Ready, message: &str) {
    r.project_op(
        "committed",
        ohl_server::Op::Commit {
            message: message.into(),
        },
    );
    assert!(!r.status.as_ref().unwrap().1, "{:?}", r.status);
}

fn move_t3(r: &mut crate::app::Ready, dx: f64) {
    let p = r.model.line.structure("T3").unwrap().position;
    assert!(r.perform(crate::edit::Action::Command {
        what: "T3".into(),
        command: ohl_session::Command::MoveStructure {
            id: "T3".into(),
            position: [p[0] + dx, p[1], p[2]],
        },
    }));
}

/// The menu reads top to bottom: what is worked on; Commit and Changes;
/// the lines; the history's windows; the offline copy; the ticks last.
#[test]
fn the_versions_menu_reads_in_order() {
    let model = synthetic_project("versions-menu-order");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        commit(r, "start");
        r.project_op(
            "phase",
            ohl_server::Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
    }
    harness.run();
    harness.get_by_label("Versions").click();
    harness.run();
    let entries = [
        "Working on: main · no uncommitted changes",
        "Commit…",
        "Changes since the last commit…",
        "Project lines…",
        "New line from main…",
        "Bring in changes…",
        "Submit for review…",
        "Description of main…",
        "Working tree…",
        "Revisions, tags and reviews…",
        "Operation log…",
        "Offline copy",
        "Shown next to it (in their colour):",
        "whole lines (else only what differs)",
        "phase 1",
    ];
    let tops: Vec<f32> = entries
        .iter()
        // (A submenu's button is named with its arrow.)
        .map(|e| harness.get_by_label_contains(e).rect().top())
        .collect();
    for (k, pair) in tops.windows(2).enumerate() {
        assert!(
            pair[0] < pair[1],
            "\"{}\" is not above \"{}\"",
            entries[k],
            entries[k + 1]
        );
    }
    // The submenu's two entries.
    harness.get_by_label_contains("Offline copy").hover();
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Export line as bundle…").is_some());
    assert!(harness.query_by_label("Import bundle…").is_some());
    // Operation log and the revisions window open under their own names.
    harness.get_by_label("Operation log…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().history_open);
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label("Revisions, tags and reviews…").click();
    harness.run();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().versions.open);
    assert!(
        harness
            .query_by_label("Revisions, tags and reviews")
            .is_some(),
        "the window is titled as its menu entry"
    );
}

/// The line worked on and its uncommitted changes: said at the top of the
/// menu and in the status bar (a dot), counted only when the project
/// changed; the chip and Commit… open the commit field, Changes since the
/// last commit… the comparison.
#[test]
fn the_line_worked_on_and_its_uncommitted_changes_are_shown() {
    let model = synthetic_project("versions-menu-status");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    commit(harness.state_mut().ready().unwrap(), "start");
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.line_status.uncommitted(), 0);
        assert_eq!(r.line_status.chip_text(), "line: main");
        assert_eq!(
            r.line_status.menu_text(),
            "Working on: main · no uncommitted changes"
        );
    }
    assert!(harness.query_by_label("line: main").is_some());
    // Counted once: frames without a change read nothing again.
    let before = harness.state_mut().ready().unwrap().line_status.clone();
    harness.run();
    assert_eq!(harness.state_mut().ready().unwrap().line_status, before);

    // An edit: one object changed.
    move_t3(harness.state_mut().ready().unwrap(), 2.0);
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        // (The structure and what follows it: counted as the session
        // counts them.)
        let n = r.line_status.uncommitted();
        assert!(n > 0, "{:?}", r.line_status);
        assert_eq!(r.line_status.chip_text(), "line: main •");
        assert_eq!(
            r.line_status.menu_text(),
            format!("Working on: main · {n} uncommitted changes")
        );
        assert!(
            r.line_status
                .details()
                .contains(&format!("objects: {n} changed")),
            "{}",
            r.line_status.details()
        );
    }
    let said = harness.state_mut().ready().unwrap().line_status.menu_text();
    harness.get_by_label("Versions").click();
    harness.run();
    assert!(harness.query_by_label(&said).is_some(), "{said}");
    // What a commit would record.
    harness
        .get_by_label("Changes since the last commit…")
        .click();
    harness.run();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().versions.open);
    assert!(
        harness
            .query_by_label_contains("working state: objects: ")
            .is_some(),
        "the Changes page compares the last commit with the working state"
    );
    harness.state_mut().ready().unwrap().versions.open = false;
    harness.run();

    // The chip opens Commit; committed, the line is clean again.
    harness.get_by_label("line: main •").click();
    harness.run();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().versions.open);
    assert!(harness.query_by_label("Commit").is_some());
    commit(harness.state_mut().ready().unwrap(), "T3 moved");
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.line_status.uncommitted(), 0, "{:?}", r.line_status);
    assert_eq!(r.line_status.chip_text(), "line: main");
}

/// Offline copy: a line exported as a bundle (the file `ohl project
/// bundle` reads) and imported back — under another name a new line at
/// the bundle's revision; the line worked on is refused.
#[test]
fn a_bundle_is_exported_and_imported_back() {
    let mut model = synthetic_project("versions-menu-bundle");
    let dir = model.sources.file.parent().unwrap().to_owned();
    let file = dir.join("main-bundle.json");
    // Nothing committed yet: no revision to carry.
    assert!(crate::offline_copy::export_to(&model, "main", &file).is_err());
    model
        .project_op(
            "committed",
            ohl_server::Op::Commit {
                message: "start".into(),
            },
        )
        .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    move_t3(harness.state_mut().ready().unwrap(), 2.0);
    {
        let r = harness.state_mut().ready().unwrap();
        let said = crate::offline_copy::export_to(&r.model, "main", &file).unwrap();
        assert!(said.contains("1 revision(s)"), "{said}");
        assert!(said.contains("uncommitted changes are not in it"), "{said}");
    }
    // The file is the bundle as the command line writes it.
    let bundle: ohl_session::versions::Bundle =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(bundle.branch, "main");
    assert_eq!(bundle.format, ohl_session::versions::BUNDLE_FILE_FORMAT);

    // Imported through its window: the line worked on gets another name.
    {
        let r = harness.state_mut().ready().unwrap();
        crate::offline_copy::open(r, file.clone());
        let w = r.bundle_import.as_mut().expect("the window");
        assert_eq!(w.as_line, "main (imported 1)");
        // Its own name: refused, said in the window.
        w.as_line = "main".into();
    }
    harness.run();
    harness.get_by_label("Import").click();
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = r.bundle_import.as_ref().expect("still open");
        assert!(
            matches!(&w.report, Some(Err(e)) if e.contains("current")),
            "{:?}",
            w.report
        );
    }
    harness.get_by_label("Back").click();
    harness.run();
    harness
        .state_mut()
        .ready()
        .unwrap()
        .bundle_import
        .as_mut()
        .unwrap()
        .as_line = "from the field".into();
    harness.run();
    harness.get_by_label("Import").click();
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = r.bundle_import.as_ref().expect("the report");
        let Some(Ok(report)) = &w.report else {
            panic!("{:?}", w.report);
        };
        assert!(report.contains("0 new revision(s)"), "{report}");
        let s = r.model.session().unwrap();
        assert!(s.is_line("from the field"));
        assert_eq!(
            s.branch_revision("from the field").unwrap().as_deref(),
            Some(bundle.head.as_str())
        );
        // The line worked on keeps its uncommitted move.
        assert_eq!(r.model.project_line(), "main");
        assert!(r.line_status.uncommitted() > 0);
    }
    harness.get_by_label("Close").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().bundle_import.is_none());
    std::fs::remove_dir_all(&dir).ok();
}

/// Revisions, tags and reviews: deleting a line says whether it has
/// uncommitted changes; "New line from it…" asks for the name as the
/// working tree does.
#[test]
fn the_revisions_window_says_what_a_deletion_drops_and_asks_for_a_name() {
    use crate::project_lines::LinesWindow;
    let model = synthetic_project("versions-menu-delete");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        commit(r, "start");
        r.project_op(
            "phase",
            ohl_server::Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
        // Uncommitted work on phase 1; back on main.
        r.switch_line("phase 1");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        move_t3(r, 2.0);
        commit(r, "T3 moved");
        move_t3(r, 1.0);
        r.switch_line("main");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        r.versions.open = true;
    }
    harness.run();
    harness.run();
    harness.get_by_label("Delete…").click();
    harness.run();
    assert!(
        harness
            .query_by_label_contains("It has uncommitted changes: they are deleted with it")
            .is_some()
    );
    harness.get_by_label("Cancel").click();
    harness.run();
    // The first revision is not the one main is at... phase 1's is: a
    // line from it is asked for by name.
    harness.get_by_label("New line from it…").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let Some(LinesWindow::New { from, at, .. }) = &r.lines_window else {
        panic!("{:?}", r.lines_window);
    };
    assert_eq!(at.as_ref().map(|a| &a.0), Some(from));
    assert!(
        !r.model
            .session()
            .unwrap()
            .is_line(&format!("from {}", &from[..8])),
        "no line is made before its name is given"
    );
}
