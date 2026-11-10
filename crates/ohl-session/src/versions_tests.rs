// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use super::*;
use crate::collab::{Grant, ReviewState};
use crate::versions::{Change, Choices, Head, MergeKind, Side, diff};
use std::path::PathBuf;

fn example() -> Line {
    Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap()
}

fn project(name: &str) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ohl-versions-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
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
    assert_eq!(ids, ["P4", "T6"]);
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
    s.put_library_file("ana", "cables", "x.ohlw", "# one\n")
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
        ("alignment_point:P4".to_owned(), Side::Theirs),
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
    assert!(s.put_library_file("ben", "cables", "a.ohlw", "").is_err());
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

/// The Roles page's table follows the session's checks: a sample of its
/// rows, each tried by the role below its least one (refused) and by its
/// least one (allowed) (owner, 2026-10-26).
#[test]
fn the_role_table_follows_the_checks() {
    let (_, mut s) = with_branch("roles");
    assert_eq!(s.role("vic").unwrap(), Some(Role::Viewer));
    assert_eq!(s.role("ben").unwrap(), Some(Role::Designer));
    let least = |start: &str| {
        crate::ROLE_ABILITIES
            .iter()
            .find(|(w, _)| w.starts_with(start))
            .unwrap()
            .1
    };
    // Editing: designers, not viewers.
    assert_eq!(least("Edit the line"), Role::Designer);
    assert!(shift(&mut s, "vic", "T2", 1.0).is_err());
    shift(&mut s, "ben", "T2", 1.0).unwrap();
    // Committing: designers.
    assert_eq!(least("Commit"), Role::Designer);
    assert!(s.commit("vic", "no").is_err());
    s.commit("ben", "T2 moved").unwrap();
    // Protecting: admins.
    assert_eq!(least("Protect or unprotect"), Role::Admin);
    assert!(s.protect_branch("ben", "main", true).is_err());
    s.protect_branch("ana", "main", true).unwrap();
    // A protected branch changed directly: admins.
    assert_eq!(least("Change a protected branch"), Role::Admin);
    assert!(shift(&mut s, "ben", "T2", 1.0).is_err());
    shift(&mut s, "ana", "T2", 1.0).unwrap();
    // Users and roles: admins.
    assert_eq!(least("Add users"), Role::Admin);
    assert!(s.set_user("ben", "zoe", Role::Viewer).is_err());
    s.set_user("ana", "zoe", Role::Viewer).unwrap();
}

/// An admin merges a review into a protected branch alone: no approval by
/// somebody else, an open review merged by the admin (owner, 2026-10-26).
#[test]
fn an_admin_merges_a_review_alone() {
    let (_, mut s) = with_branch("review-admin");
    s.protect_branch("ana", "main", true).unwrap();
    s.set_user("ana", "rita", Role::Designer).unwrap();
    s.checkout("ben", "alt").unwrap();
    shift(&mut s, "ben", "T6", 1.0).unwrap();
    s.commit("ben", "T6 for the road").unwrap();
    s.checkout("ben", "main").unwrap();
    let id = s
        .submit_review("ben", "alt", "main", &["rita".into()], "")
        .unwrap();
    // A designer still needs an approval; the admin does not.
    assert!(s.merge_review("ben", id, None, &Choices::new()).is_err());
    s.merge_review("ana", id, None, &Choices::new()).unwrap();
    assert_eq!(s.review(id).unwrap().state, ReviewState::Merged);
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
    // Viewers (e.g. added by the general invitation) and the author do
    // not review (security review 2026-10-26, M1).
    assert!(
        s.submit_review("ben", "alt", "main", &["vic".into()], "")
            .unwrap_err()
            .to_string()
            .contains("viewer")
    );
    assert!(
        s.submit_review("ben", "alt", "main", &["ben".into()], "")
            .is_err()
    );
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
    // Not approved yet: nobody merges.
    assert!(s.merge_review("ben", id, None, &Choices::new()).is_err());
    // A reviewer made a viewer meanwhile decides nothing.
    s.set_user("ana", "rita", Role::Viewer).unwrap();
    assert!(s.decide_review("rita", id, true, "").is_err());
    s.set_user("ana", "rita", Role::Designer).unwrap();
    s.decide_review("rita", id, true, "fine now").unwrap();
    // Into a protected branch, the approver does not merge her own
    // approval; a viewer never merges; the author does, approved by
    // somebody else (security review 2026-10-26, M1).
    assert!(
        s.merge_review("rita", id, None, &Choices::new())
            .unwrap_err()
            .to_string()
            .contains("somebody else")
    );
    assert!(s.merge_review("vic", id, None, &Choices::new()).is_err());
    let out = s.merge_review("ben", id, None, &Choices::new()).unwrap();
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
    let cables = ohl_cables::Library::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libraries/cables"),
    )
    .unwrap();
    let case =
        ohl_weather::WeatherCase::still("15 °C", ohl_core::units::Temperature::from_celsius(15.0));
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
    let copy = path.with_file_name("copy.ohl");
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

/// The revision a merge is made against (review 2026-11-10): a latest
/// common ancestor, not the nearest by distance. Main B0 → m1 … m10; a
/// short side line from B0 brought into main (M: parents m10, v1); line C
/// made at m3 with eight revisions. B0 is nearer (2 + 11 steps) than m3
/// (8 + 8), but m3 descends from it: m3 is the base. A history that
/// crosses has two latest common revisions, the nearest first.
#[test]
fn the_merge_base_is_a_latest_common_revision() {
    let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut add = |id: &str, of: &[&str]| {
        parents.insert(id.to_owned(), of.iter().map(|p| (*p).to_owned()).collect());
    };
    add("B0", &[]);
    add("m1", &["B0"]);
    for k in 2..=10 {
        add(&format!("m{k}"), &[&format!("m{}", k - 1)]);
    }
    add("v1", &["B0"]);
    add("M", &["m10", "v1"]);
    add("c1", &["m3"]);
    for k in 2..=8 {
        add(&format!("c{k}"), &[&format!("c{}", k - 1)]);
    }
    assert_eq!(crate::versions::merge_bases_in(&parents, "M", "c8"), ["m3"]);
    assert_eq!(crate::versions::merge_bases_in(&parents, "c8", "M"), ["m3"]);
    // One side contains the other: that revision.
    assert_eq!(
        crate::versions::merge_bases_in(&parents, "m5", "m9"),
        ["m5"]
    );
    // Nothing in common.
    let mut two = parents.clone();
    two.insert("z".into(), vec![]);
    assert!(crate::versions::merge_bases_in(&two, "z", "M").is_empty());
    // A criss-cross: x and y each merged into the other's line.
    let mut cross: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, of) in [
        ("r", vec![]),
        ("x", vec!["r"]),
        ("y", vec!["r"]),
        ("p", vec!["x", "y"]),
        ("q", vec!["y", "x"]),
        ("p2", vec!["p"]),
    ] {
        cross.insert(id.into(), of.into_iter().map(str::to_owned).collect());
    }
    assert_eq!(
        crate::versions::merge_bases_in(&cross, "p2", "q"),
        ["x", "y"]
    );
}

/// A structure of the example under id `id`, `dy` m to the side, as a put.
fn structure_put(s: &Session, like: &str, id: &str, dy: f64) -> Command {
    let mut st = s.line().structure(like).unwrap().clone();
    st.id = id.to_owned();
    st.position[1] += dy;
    Command::Put {
        kind: Kind::Structure,
        id: id.to_owned(),
        value: serde_json::to_value(&st).unwrap(),
        ord: None,
    }
}

/// Each line made a structure under the same id (review 2026-11-10): a
/// conflict that can keep both — the other line's under an id of its own,
/// the jumper made on it there following (it must not end up on this
/// line's structure of that id).
#[test]
fn both_kept_where_each_line_made_the_same_id() {
    let (path, mut s) = with_branch("keep-both");
    s.checkout("ana", "alt").unwrap();
    s.apply("ana", structure_put(&s, "T3", "T8", 40.0)).unwrap();
    let mut j = s.line().jumpers[0].clone();
    j.id = "J8".into();
    for p in [&mut j.from, &mut j.to]
        .into_iter()
        .filter_map(ohl_line::JumperEnd::point_mut)
    {
        p.structure = "T8".into();
    }
    s.apply(
        "ana",
        Command::Put {
            kind: Kind::Jumper,
            id: "J8".into(),
            value: serde_json::to_value(&j).unwrap(),
            ord: None,
        },
    )
    .unwrap();
    s.commit("ana", "alt's T8 with a jumper").unwrap();
    let theirs_at = position(&s, "T8");
    s.checkout("ana", "main").unwrap();
    s.apply("ana", structure_put(&s, "T3", "T8", -40.0))
        .unwrap();
    s.commit("ana", "main's T8").unwrap();
    let ours_at = position(&s, "T8");
    assert_ne!(ours_at, theirs_at);

    // The conflict offers both, under the id the other line's would take.
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    let c = plan
        .conflicts
        .iter()
        .find(|c| c.key == "structure:T8")
        .expect("a conflict on T8");
    assert_eq!(c.keep_both.as_deref(), Some("T8 (2)"));
    assert_eq!(c.chosen, None);
    // "Both" on a conflict that is not two new objects is no choice.
    let not_both = Choices::from([("structure:T8".to_owned(), Side::Both)]);
    let plan = s.merge_plan("alt", &not_both).unwrap();
    assert!(plan.unresolved().is_empty(), "{:?}", plan.unresolved());
    assert_eq!(plan.problem, None);
    let kept = plan
        .conflicts
        .iter()
        .find(|c| c.key == "structure:T8")
        .unwrap();
    assert_eq!(
        (kept.chosen, kept.keep_both.as_deref()),
        (Some(Side::Both), Some("T8 (2)"))
    );

    let out = s.merge("ana", "alt", None, &not_both).unwrap();
    assert_eq!(out.revision.unwrap().parents.len(), 2);
    assert_eq!(position(&s, "T8"), ours_at);
    assert_eq!(position(&s, "T8 (2)"), theirs_at);
    let j8 = s.line().jumpers.iter().find(|j| j.id == "J8").unwrap();
    assert!(j8.points().all(|p| p.structure == "T8 (2)"));
    assert!(s.status().unwrap().changes.is_empty());
    // As stored: the same after opening the file again.
    drop(s);
    let s = Session::open(&path).unwrap();
    assert_eq!(position(&s, "T8 (2)"), theirs_at);

    // Elsewhere "both" settles nothing: a field changed on both lines.
    let (_, mut s) = with_branch("keep-both-not");
    s.checkout("ana", "alt").unwrap();
    shift(&mut s, "ana", "T6", 4.0).unwrap();
    s.commit("ana", "T6 +4").unwrap();
    s.checkout("ana", "main").unwrap();
    shift(&mut s, "ana", "T6", -4.0).unwrap();
    s.commit("ana", "T6 -4").unwrap();
    let both = Choices::from([("structure:T6".to_owned(), Side::Both)]);
    let plan = s.merge_plan("alt", &both).unwrap();
    assert!(!plan.unresolved().is_empty());
    assert!(plan.conflicts.iter().all(|c| c.keep_both.is_none()));
    assert!(s.merge("ana", "alt", None, &both).is_err());
}

/// A unit of work that fails leaves nothing (review 2026-11-10: a merge
/// was three separate writes): not in the file — the command's log entry
/// and the revision made inside it are gone — and not in memory, where
/// the state the command replaced is put back.
#[test]
fn a_failed_unit_leaves_nothing() {
    let (path, mut s) = with_branch("unit");
    let t6 = position(&s, "T6");
    let (revisions, log) = (s.revisions().unwrap().len(), s.log().unwrap().len());
    let failed: Result<(), SessionError> = s.unit(|s| {
        shift(s, "ana", "T6", 7.0)?;
        s.commit("ana", "inside the unit")?;
        assert_eq!(position(s, "T6")[0], t6[0] + 7.0);
        Err(SessionError::Refused("stopped".into()))
    });
    assert!(failed.is_err());
    assert_eq!(position(&s, "T6"), t6);
    assert_eq!(s.revisions().unwrap().len(), revisions);
    assert_eq!(s.log().unwrap().len(), log);
    assert!(s.status().unwrap().changes.is_empty());
    // The session works on: a unit that succeeds stays, in memory and file.
    s.unit(|s| {
        shift(s, "ana", "T6", 2.0)?;
        s.commit("ana", "kept").map(drop)
    })
    .unwrap();
    assert_eq!(position(&s, "T6")[0], t6[0] + 2.0);
    drop(s);
    let s = Session::open(&path).unwrap();
    assert_eq!(position(&s, "T6")[0], t6[0] + 2.0);
    assert_eq!(s.revisions().unwrap().len(), revisions + 1);
}

/// The merged line is checked with its libraries (review 2026-11-10): one
/// line stops using a structure type and deletes its file, the other puts
/// a structure on that type. Each line is valid; merged, the structure has
/// no type any more — refused, where before it was accepted and failed to
/// solve afterwards.
#[test]
fn a_merge_is_checked_with_its_libraries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-versions-{}-typed", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut s = Session::open(&path).unwrap();
    s.import(
        &Line::load(&root.join("synthetic-line-typed.ohl.json")).unwrap(),
        "ana",
    )
    .unwrap();
    for (kind, folder, ext) in [
        ("structures", "Structures", "ohls"),
        ("insulators", "Insulators", "ohli"),
    ] {
        for f in std::fs::read_dir(root.join(folder)).unwrap() {
            let f = f.unwrap().path();
            if f.extension().is_some_and(|e| e == ext) {
                let name = f.file_name().unwrap().to_string_lossy().into_owned();
                let text = std::fs::read_to_string(&f).unwrap();
                s.put_library_file("ana", kind, &name, &text).unwrap();
            }
        }
    }
    s.commit("ana", "as imported, with its libraries").unwrap();
    s.create_branch("ana", "alt", None).unwrap();
    let state = s.working_state().unwrap();
    assert!(crate::versions::resolved_line(&state).is_ok());
    let retype = |s: &mut Session, id: &str, to: &str| {
        let mut st = s.line().structure(id).unwrap().clone();
        st.structure_type = Some(to.to_owned());
        s.apply(
            "ana",
            Command::Put {
                kind: Kind::Structure,
                id: id.to_owned(),
                value: serde_json::to_value(&st).unwrap(),
                ord: None,
            },
        )
        .unwrap();
    };
    // The other line: T7 on type-05.
    s.checkout("ana", "alt").unwrap();
    retype(&mut s, "T7", "type-05");
    s.commit("ana", "T7 on type-05").unwrap();
    // This line: nothing on type-05 any more, its file deleted.
    s.checkout("ana", "main").unwrap();
    retype(&mut s, "T6", "type-06");
    s.delete_library_file("ana", "structures", "type-05.ohls")
        .unwrap();
    s.commit("ana", "type-05 dropped").unwrap();
    let plan = s.merge_plan("alt", &Choices::new()).unwrap();
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    let problem = plan.problem.expect("the merged line lacks type-05");
    assert!(problem.contains("type-05"), "{problem}");
    let refused = s.merge("ana", "alt", None, &Choices::new()).unwrap_err();
    assert!(refused.to_string().contains("type-05"), "{refused}");
    assert!(s.status().unwrap().changes.is_empty());
}
