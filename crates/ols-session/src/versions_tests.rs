// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::collab::{Grant, ReviewState};
use crate::versions::{Change, Choices, Head, MergeKind, Side, diff};
use std::path::PathBuf;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ols.json"),
    )
    .unwrap()
}

fn project(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ols-versions-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ols");
    std::fs::remove_file(&path).ok();
    let mut s = Session::open(&path).unwrap();
    s.import(&example(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s.set_user("ana", "vic", Role::Viewer).unwrap();
    (path, s)
}

fn position(s: &Session, id: &str) -> Point {
    s.line().structure(id).unwrap().position
}

fn shift(s: &mut Session, user: &str, id: &str, dx: f64) -> Result<Applied, SessionError> {
    let p = position(s, id);
    s.apply(
        user,
        Command::MoveStructure {
            id: id.into(),
            position: [p[0] + dx, p[1], p[2]],
        },
    )
}

/// A project with a first revision on `main` and a branch `alt` there.
fn with_branch(name: &str) -> (PathBuf, Session) {
    let (path, mut s) = project(name);
    s.commit("ana", "as imported").unwrap();
    s.create_branch("ana", "alt", None).unwrap();
    (path, s)
}

#[test]
fn commit_checkout_round_trip() {
    let (path, mut s) = with_branch("roundtrip");
    let t6 = position(&s, "T6");
    let first = s.head_revision().unwrap().unwrap();
    assert!(s.status().unwrap().changes.is_empty());
    // Nothing changed: nothing to commit.
    assert!(s.commit("ana", "again").is_err());

    s.checkout("ben", "alt").unwrap();
    assert_eq!(s.head().unwrap(), Head::Branch("alt".into()));
    shift(&mut s, "ben", "T6", 5.0).unwrap();
    // T6 and the branch's point tied to it.
    let st = s.status().unwrap();
    let ids: Vec<&str> = st.changes.objects.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["branch", "T6"]);
    // Each branch is a project line with its own working state: a
    // checkout only chooses the line worked on, uncommitted changes and
    // all, and changes nothing.
    let (applied, libs) = s.checkout("ben", "main").unwrap();
    assert!(applied.is_none() && !libs);
    assert_eq!(position(&s, "T6"), t6);
    s.checkout("ben", "alt").unwrap();
    assert_eq!(position(&s, "T6")[0], t6[0] + 5.0);
    let moved = s.commit("ben", "T6 moved 5 m").unwrap();
    assert_eq!(moved.parents, vec![first.clone()]);
    assert_eq!(moved.author, "ben");
    s.checkout("ben", "main").unwrap();
    assert_eq!(position(&s, "T6"), t6);

    // Revisions survive reopening; ids are content hashes.
    drop(s);
    let s = Session::open(&path).unwrap();
    assert_eq!(s.revisions().unwrap().len(), 2);
    assert_eq!(
        s.revision_state(&moved.id)
            .unwrap()
            .line()
            .unwrap()
            .structure("T6")
            .unwrap()
            .position[0],
        t6[0] + 5.0
    );
    assert_eq!(s.resolve_revision(&moved.id[..8]).unwrap(), moved.id);
    assert_eq!(s.resolve_revision("alt").unwrap(), moved.id);
    // Viewers commit nothing.
    let mut s = s;
    assert!(s.commit("vic", "no").is_err());
}

#[test]
fn diff_lists_fields() {
    let (_, mut s) = with_branch("diff");
    let a = s.head_revision().unwrap().unwrap();
    shift(&mut s, "ana", "T6", 2.0).unwrap();
    s.apply(
        "ana",
        Command::Delete {
            kind: Kind::Jumper,
            id: s.line().jumpers[0].id.clone(),
            confirm: false,
        },
    )
    .unwrap();
    s.put_library_file("ana", "cables", "x.olsw", "# one\n")
        .unwrap();
    let b = s.commit("ana", "moved, deleted, library").unwrap();
    let d = diff(
        &s.revision_state(&a).unwrap(),
        &s.revision_state(&b.id).unwrap(),
    );
    let t6 = d.objects.iter().find(|o| o.id == "T6").unwrap();
    assert_eq!(t6.change, Change::Changed);
    assert!(t6.fields.iter().any(|f| f.path == "position"));
    assert!(
        d.objects
            .iter()
            .any(|o| o.change == Change::Removed && o.kind == Kind::Jumper)
    );
    assert_eq!(d.libraries.len(), 1);
    assert_eq!(d.libraries[0].change, Change::Added);
    assert!(
        d.summary().contains("library files: 1 added"),
        "{}",
        d.summary()
    );
}

#[test]
fn merge_without_conflicts() {
    let (_, mut s) = with_branch("merge");
    let (t2, t6) = (position(&s, "T2"), position(&s, "T6"));
    s.checkout("ana", "alt").unwrap();
    shift(&mut s, "ana", "T6", 3.0).unwrap();
    s.commit("ana", "T6 east").unwrap();
    // Fast-forward: main has nothing new.
    s.checkout("ana", "main").unwrap();
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    assert_eq!(plan.kind, MergeKind::FastForward);
    // Make main diverge instead.
    shift(&mut s, "ana", "T2", -2.0).unwrap();
    let ours = s.commit("ana", "T2 west").unwrap();
    // The same structure, other fields: merged by field.
    s.checkout("ana", "alt").unwrap();
    let mut t6v = s.line().structure("T6").unwrap().clone();
    t6v.orientation_deg += 1.0;
    s.apply(
        "ana",
        Command::Put {
            kind: Kind::Structure,
            id: "T6".into(),
            value: serde_json::to_value(&t6v).unwrap(),
            ord: None,
        },
    )
    .unwrap();
    let theirs = s.commit("ana", "T6 turned").unwrap();
    s.checkout("ana", "main").unwrap();
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    assert_eq!(plan.kind, MergeKind::ThreeWay);
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    let out = s.merge("ana", "alt", None, &Choices::new()).unwrap();
    let rev = out.revision.unwrap();
    assert_eq!(rev.parents, vec![ours.id.clone(), theirs.id.clone()]);
    assert_eq!(position(&s, "T2")[0], t2[0] - 2.0);
    assert_eq!(position(&s, "T6")[0], t6[0] + 3.0);
    assert_eq!(
        s.line().structure("T6").unwrap().orientation_deg,
        t6v.orientation_deg
    );
    assert!(s.status().unwrap().changes.is_empty());
    // Merged again: up to date.
    assert!(s.merge("ana", "alt", None, &Choices::new()).is_err());
    assert_eq!(
        s.merge_base(&ours.id, &theirs.id).unwrap(),
        s.revisions()
            .unwrap()
            .iter()
            .find(|r| r.message == "as imported")
            .map(|r| r.id.clone())
    );
}

#[test]
fn merge_with_a_conflict() {
    let (_, mut s) = with_branch("conflict");
    let t6 = position(&s, "T6");
    s.checkout("ana", "alt").unwrap();
    shift(&mut s, "ana", "T6", 4.0).unwrap();
    s.commit("ana", "T6 +4").unwrap();
    s.checkout("ana", "main").unwrap();
    shift(&mut s, "ana", "T6", -4.0).unwrap();
    s.commit("ana", "T6 -4").unwrap();
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    assert!(
        plan.conflicts
            .iter()
            .any(|c| c.key == "structure:T6/position")
    );
    let refused = s.merge("ana", "alt", None, &Choices::new()).unwrap_err();
    assert!(
        refused.to_string().contains("structure:T6/position"),
        "{refused}"
    );
    // Nothing changed by the refusal.
    assert_eq!(position(&s, "T6")[0], t6[0] - 4.0);
    // Theirs for the whole structure.
    let choices = Choices::from([
        ("structure:T6".to_owned(), Side::Theirs),
        ("alignment:branch".to_owned(), Side::Theirs),
    ]);
    s.merge("ana", "alt", Some("take alt's T6"), &choices)
        .unwrap();
    assert_eq!(position(&s, "T6")[0], t6[0] + 4.0);
    let head = s.revision(&s.head_revision().unwrap().unwrap()).unwrap();
    assert_eq!(head.message, "take alt's T6");
    assert_eq!(head.parents.len(), 2);
}

#[test]
fn tags_and_detached_revisions() {
    let (_, mut s) = with_branch("tags");
    let first = s.head_revision().unwrap().unwrap();
    s.tag("ana", "IFC rev A", None, "issued for construction")
        .unwrap();
    assert!(s.tag("ana", "IFC rev A", None, "").is_err());
    assert!(s.tag("ana", "main", None, "").is_err());
    shift(&mut s, "ana", "T2", 1.0).unwrap();
    s.commit("ana", "T2 moved").unwrap();
    assert_eq!(s.resolve_revision("IFC rev A").unwrap(), first);
    // A tag is not worked on: a line is made from it.
    assert!(
        s.checkout("ana", "IFC rev A")
            .unwrap_err()
            .to_string()
            .contains("make a line")
    );
    let t2 = position(&s, "T2");
    s.create_line("ana", "as issued", "IFC rev A").unwrap();
    s.checkout("ana", "as issued").unwrap();
    assert_eq!(position(&s, "T2")[0], t2[0] - 1.0);
    assert_eq!(s.head_revision().unwrap(), Some(first.clone()));
    s.checkout("ana", "main").unwrap();
    // Deleting asks first and lists what goes; tags: admins only.
    assert!(s.delete_ref("ben", "IFC rev A", true).is_err());
    let ask = s.delete_ref("ana", "IFC rev A", false).unwrap_err();
    assert!(
        ask.to_string().contains("repeat with confirmation"),
        "{ask}"
    );
    s.delete_ref("ana", "IFC rev A", true).unwrap();
    assert!(s.resolve_revision("IFC rev A").is_err());
    assert!(s.delete_ref("ana", "main", true).is_err());
}

#[test]
fn revert_undoes_one_revision() {
    let (_, mut s) = with_branch("revert");
    let (t2, t6) = (position(&s, "T2"), position(&s, "T6"));
    shift(&mut s, "ana", "T2", 2.0).unwrap();
    let c1 = s.commit("ana", "T2 moved").unwrap();
    shift(&mut s, "ana", "T6", 2.0).unwrap();
    s.commit("ana", "T6 moved").unwrap();
    let out = s.revert("ana", &c1.id, None, &Choices::new()).unwrap();
    assert_eq!(position(&s, "T2"), t2);
    assert_eq!(position(&s, "T6")[0], t6[0] + 2.0);
    assert!(out.revision.unwrap().message.starts_with("Revert"));
    // Reverted already: nothing left to revert.
    assert!(s.revert("ana", &c1.id, None, &Choices::new()).is_err());
    // The first revision has nothing before it.
    let first = s.resolve_revision("alt").unwrap();
    assert!(s.revert("ana", &first, None, &Choices::new()).is_err());
}

#[test]
fn grants_limit_edits() {
    let (_, mut s) = project("grants");
    s.grant(
        "ana",
        &Grant {
            subject: "ben".into(),
            kinds: vec!["structure".into()],
            objects: vec!["T2".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        s.grant(
            "ben",
            &Grant {
                subject: "ben".into(),
                ..Default::default()
            }
        )
        .is_err()
    );
    shift(&mut s, "ben", "T2", 1.0).unwrap();
    let refused = shift(&mut s, "ben", "T6", 1.0).unwrap_err();
    assert!(refused.to_string().contains("is outside"), "{refused}");
    // Admins are not limited; libraries need a grant naming them.
    shift(&mut s, "ana", "T6", 1.0).unwrap();
    assert!(s.put_library_file("ben", "cables", "a.olsw", "").is_err());
    // Limited users do not merge (they may still look at other lines).
    s.commit("ana", "one").unwrap();
    assert!(s.merge("ben", "main", None, &Choices::new()).is_err());
    s.checkout("ben", "main").unwrap();

    // An area around T6 for every designer.
    let p = position(&s, "T6");
    let area = vec![
        [p[0] - 50.0, p[1] - 50.0],
        [p[0] + 50.0, p[1] - 50.0],
        [p[0] + 50.0, p[1] + 50.0],
        [p[0] - 50.0, p[1] + 50.0],
    ];
    let id = s
        .grant(
            "ana",
            &Grant {
                subject: "role:designer".into(),
                kinds: vec!["structure".into()],
                area: Some(area),
                ..Default::default()
            },
        )
        .unwrap();
    shift(&mut s, "ben", "T6", 1.0).unwrap();
    // Out of the area: refused.
    assert!(shift(&mut s, "ben", "T6", 200.0).is_err());
    // A deletion takes its sections along: those are outside the grant.
    assert!(
        s.apply(
            "ben",
            Command::Delete {
                kind: Kind::Structure,
                id: "T6".into(),
                confirm: true
            }
        )
        .is_err()
    );
    s.revoke("ana", id).unwrap();
    assert!(shift(&mut s, "ben", "T6", 1.0).is_err());
    assert_eq!(s.grants().unwrap().len(), 1);
}

#[test]
fn review_and_protected_branch() {
    let (_, mut s) = with_branch("review");
    s.protect_branch("ana", "main", true).unwrap();
    assert!(s.protect_branch("ben", "main", false).is_err());
    // Designers neither change nor commit a protected branch.
    assert!(
        shift(&mut s, "ben", "T2", 1.0)
            .unwrap_err()
            .to_string()
            .contains("protected")
    );
    assert!(s.commit("ben", "direct").is_err());
    s.checkout("ben", "alt").unwrap();
    shift(&mut s, "ben", "T6", 1.0).unwrap();
    s.commit("ben", "T6 for the road").unwrap();
    s.checkout("ben", "main").unwrap();
    assert!(s.merge("ben", "alt", None, &Choices::new()).is_err());

    s.set_user("ana", "rita", Role::Designer).unwrap();
    let id = s
        .submit_review("ben", "alt", "main", &["rita".into()], "please check T6")
        .unwrap();
    assert!(s.submit_review("ben", "alt", "main", &[], "").is_err());
    // Authors and others do not decide; reviewers and admins do.
    assert!(s.decide_review("ben", id, true, "").is_err());
    assert!(s.decide_review("vic", id, true, "").is_err());
    s.decide_review("rita", id, false, "T6 too close to the road")
        .unwrap();
    assert!(s.merge_review("rita", id, None, &Choices::new()).is_err());
    // Changed and submitted again.
    s.checkout("ben", "alt").unwrap();
    shift(&mut s, "ben", "T6", 1.0).unwrap();
    s.commit("ben", "T6 further").unwrap();
    s.checkout("ben", "main").unwrap();
    let id = s
        .submit_review("ben", "alt", "main", &["rita".into()], "again")
        .unwrap();
    s.decide_review("rita", id, true, "fine now").unwrap();
    assert!(s.merge_review("ben", id, None, &Choices::new()).is_err());
    let out = s.merge_review("rita", id, None, &Choices::new()).unwrap();
    assert_eq!(out.kind, MergeKind::FastForward);
    let r = s.review(id).unwrap();
    assert_eq!(r.state, ReviewState::Merged);
    assert_eq!(r.notes.len(), 2);
    assert_eq!(
        s.branch_revision("main").unwrap(),
        s.branch_revision("alt").unwrap()
    );
}

#[test]
fn old_project_files_are_migrated() {
    let (path, s) = project("migrate");
    drop(s);
    // Back to schema 1: without the new tables.
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "DROP TABLE vc_blob; DROP TABLE vc_revision; DROP TABLE vc_ref;
             DROP TABLE edit_grant; DROP TABLE vc_review; DROP TABLE vc_review_note;
             DELETE FROM ols_meta WHERE key = 'vc_head';
             UPDATE ols_meta SET value = '1' WHERE key = 'session_schema';",
        )
        .unwrap();
    }
    let mut s = Session::open(&path).unwrap();
    assert_eq!(s.export(), example());
    let schema: String = s
        .conn
        .query_row(
            "SELECT value FROM ols_meta WHERE key = 'session_schema'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(schema, SCHEMA_VERSION.to_string());
    assert_eq!(s.head().unwrap(), Head::Branch("main".into()));
    assert_eq!(s.head_revision().unwrap(), None);
    // Everything is uncommitted until the first commit.
    assert!(!s.status().unwrap().changes.is_empty());
    s.commit("ana", "first").unwrap();
    assert!(s.status().unwrap().changes.is_empty());
    // A newer schema is refused.
    drop(s);
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE ols_meta SET value = '99' WHERE key = 'session_schema'",
            [],
        )
        .unwrap();
    assert!(Session::open(&path).is_err());
}

#[test]
fn engineering_impact_solves_both_sides() {
    let (_, mut s) = with_branch("impact");
    let a = s.working_state().unwrap();
    shift(&mut s, "ana", "T6", 20.0).unwrap();
    let b = s.working_state().unwrap();
    let d = diff(&a, &b);
    let cables = ols_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let case =
        ols_weather::WeatherCase::still("15 °C", ols_core::units::Temperature::from_celsius(15.0));
    let flat = |_x: f64, _y: f64| Some(480.0);
    let opts = crate::impact::ImpactOptions {
        case: &case,
        cases: std::slice::from_ref(&case),
        fallback_cables: Some(&cables),
        ground: Some(&flat),
        max_sections: 50,
        time_limit: std::time::Duration::from_secs(30),
    };
    let impact = crate::impact::engineering_impact(&a, &b, &d, &opts);
    assert_eq!(impact.sections.len(), 1);
    let s3 = &impact.sections[0];
    assert_eq!(s3.section, "S3");
    assert!(s3.errors.is_empty(), "{:?}", s3.errors);
    let (x, y) = (s3.before.unwrap(), s3.after.unwrap());
    assert!(x.max_sag > 0.0 && y.max_tension > 0.0);
    assert!(x.max_sag != y.max_sag);
    assert!(x.min_clearance.is_some());
    // The limit: no section solved.
    let none = crate::impact::engineering_impact(
        &a,
        &b,
        &d,
        &crate::impact::ImpactOptions {
            max_sections: 0,
            ..opts
        },
    );
    assert_eq!(none.skipped, 1);
    assert!(none.stopped.is_some());
}

#[test]
fn offline_branch_comes_back_through_a_bundle() {
    let (path, mut s) = with_branch("bundle");
    // The offline copy (as a client joining a server receives).
    let copy = path.with_file_name("copy.ols");
    std::fs::remove_file(&copy).ok();
    s.copy_to(&copy).unwrap();
    let t6 = position(&s, "T6");
    {
        let mut c = Session::open(&copy).unwrap();
        c.create_branch("ben", "field", None).unwrap();
        c.checkout("ben", "field").unwrap();
        shift(&mut c, "ben", "T6", 7.0).unwrap();
        c.commit("ben", "as staked in the field").unwrap();
        let bundle = c.export_bundle("field").unwrap();
        let text = serde_json::to_string(&bundle).unwrap();
        let back: crate::versions::Bundle = serde_json::from_str(&text).unwrap();
        assert_eq!(s.import_bundle("ben", &back, None).unwrap(), 1);
        // Again: nothing new, the branch is there.
        assert!(s.import_bundle("ben", &back, None).is_ok());
        // Damaged bundles are refused.
        let mut bad = back.clone();
        bad.revisions[0].message.push('!');
        assert!(s.import_bundle("ben", &bad, Some("x")).is_err());
    }
    s.merge("ana", "field", None, &Choices::new()).unwrap();
    assert_eq!(position(&s, "T6")[0], t6[0] + 7.0);
}
