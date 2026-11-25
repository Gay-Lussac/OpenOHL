// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: File › Join routes from another project… on the synthetic line
//! split at its Y tower T3 (the main route open, the branch with its own
//! T3 in another file): the preview, a twin ticked made one shared tower,
//! one undo step, the revision, Escape.

use super::*;

/// The main route alone.
fn main_route(l: &mut ohl_line::Line) {
    l.routes.retain(|r| r.id == "main");
    l.alignments.retain(|a| a.id == "main");
    l.alignment_points
        .retain(|p| ["P1", "P2", "P3"].contains(&p.id.as_str()));
    l.structures
        .retain(|s| !["T6", "T7"].contains(&s.id.as_str()));
    l.sections.retain(|s| s.id != "S3");
    l.jumpers.clear();
    l.circuits.retain(|c| c.name != "Branch");
}

/// The branch with its own T3, in a project file of its own.
fn branch_file(name: &str) -> PathBuf {
    let mut l =
        ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json"))
            .unwrap();
    l.name = "Branch line".into();
    l.routes.retain(|r| r.id == "branch");
    l.alignments.retain(|a| a.id == "branch");
    l.alignment_points
        .retain(|p| ["P2", "P4", "P5"].contains(&p.id.as_str()));
    l.structures
        .retain(|s| ["T3", "T6", "T7"].contains(&s.id.as_str()));
    l.sections.retain(|s| s.id == "S3");
    l.jumpers.clear();
    l.circuits.retain(|c| c.name == "Branch");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("branch.ohl");
    std::fs::remove_file(&path).ok();
    ohl_session::Session::open(&path)
        .unwrap()
        .import(&l, "tester")
        .unwrap();
    path
}

fn harness(name: &str) -> (Harness<'static, App>, PathBuf) {
    let model = synthetic_project_with(name, main_route);
    let other = branch_file(name);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run_steps(2);
    (harness, other)
}

#[test]
fn routes_are_joined_a_ticked_twin_shared_one_undo_and_a_revision() {
    let (mut harness, other) = harness("join-routes");
    // The File menu's entry opens the window.
    harness.get_by_label("File").click();
    harness.run_steps(2);
    harness
        .get_by_label("Join routes from another project…")
        .click();
    harness.run_steps(2);
    assert!(harness.state_mut().ready().unwrap().join_routes.open);
    harness
        .state_mut()
        .ready()
        .unwrap()
        .join_routes
        .set_file(other.clone());
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("Twins (1)").is_some());
    assert!(
        harness
            .query_by_label_contains("structure T3 ➡ branch-T3")
            .is_some()
    );
    assert!(harness.query_by_label("two towers").is_some());
    // Ticked: one shared tower in the preview.
    {
        let w = &mut harness.state_mut().ready().unwrap().join_routes;
        w.merge.insert(("T3".into(), "T3".into()));
        w.stale = true;
    }
    harness.run_steps(3);
    assert!(harness.query_by_label("one shared tower").is_some());
    if let Ok(shots) = ohl_core::env::var("OHL_SHOTS") {
        assert_eq!(crate::files::user(), "tester", "made with USER=tester");
        harness.input_mut().events.push(egui::Event::PointerGone);
        harness.run_steps(2);
        let image = harness.render().unwrap();
        let ppp = image.width() as f32 / 1100.0;
        let image =
            match super::tool_shots::window_rect(&harness, "Join routes from another project") {
                Some(rect) => super::tool_shots::crop(&image, rect, ppp),
                None => image,
            };
        image.save(format!("{shots}/join-routes.png")).unwrap();
    }
    let before = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .len();
    assert_eq!(before, 5);
    harness.get_by_label("Join").click();
    harness.run_steps(3);
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.join_routes.open, "{:?}", r.join_routes.status);
    assert_eq!(
        r.model.line.structures.len(),
        7,
        "T3 shared: {:?}",
        r.status
    );
    let s3 = r.model.line.sections.iter().find(|s| s.id == "S3").unwrap();
    assert_eq!(
        (s3.points[0].structure.as_str(), s3.points[0].set),
        ("T3", 2)
    );
    let s = r.model.session().unwrap();
    let revisions = s.store().revisions().unwrap();
    assert!(
        revisions
            .iter()
            .any(|x| x.message == "joined routes from branch.ohl"),
        "the revision"
    );
    // One undo step takes it all back.
    r.model.undo().unwrap();
    assert_eq!(r.model.line.structures.len(), 5);
    assert!(r.model.line.sections.iter().all(|s| s.id != "S3"));
}

#[test]
fn escape_cancels_and_nothing_changes() {
    let (mut harness, other) = harness("join-routes-escape");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .join_routes
        .show(Some(other));
    harness.run_steps(3);
    let before = harness.state_mut().ready().unwrap().model.line.clone();
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.join_routes.open);
    assert_eq!(r.model.line, before);
}

#[test]
fn the_prefix_comes_from_the_file_name() {
    use crate::join_routes::prefix_of;
    assert_eq!(
        prefix_of(std::path::Path::new("/x/branch line.ohl")),
        "branch-line"
    );
    assert_eq!(prefix_of(std::path::Path::new("/x/__.ohl")), "J");
    assert_eq!(prefix_of(std::path::Path::new("/x/_b2.ohl")), "b2");
}
