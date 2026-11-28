// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The rights' one function (phase 7): default deny, the union of grants,
//! `see` needed, denies over grants whatever the order, regions on the
//! safe side, routes and their parts, the line's settings, the line and
//! the network as wholes; property tests over random roles, assignments
//! and targets.

use std::collections::{BTreeSet, HashSet};

use ohl_core::grid::Bounds;

use crate::in_place_tests::Rng;
use crate::rights::*;
use crate::{Kind, Role};

fn data(assignments: Vec<Assignment>) -> RightsData {
    let mut d = RightsData::shipped();
    for u in ["ann", "bob", "cid"] {
        d.users.insert(u.into(), true);
    }
    d.groups
        .insert("crew".into(), ["bob".to_owned(), "cid".to_owned()].into());
    d.assignments = assignments;
    d
}

fn grant(id: i64, subject: &str, role: &str, scope: Scope) -> Assignment {
    Assignment {
        id,
        subject: Subject::parse(subject).unwrap(),
        role: Some(role.into()),
        deny: false,
        scope,
        by: "test".into(),
        note: String::new(),
    }
}

fn deny(id: i64, subject: &str, role: Option<&str>, scope: Scope) -> Assignment {
    Assignment {
        id,
        subject: Subject::parse(subject).unwrap(),
        role: role.map(str::to_owned),
        deny: true,
        scope,
        by: "test".into(),
        note: String::new(),
    }
}

fn at(x: f64, y: f64) -> Placement<'static> {
    Placement {
        bounds: Some(Bounds::new([x, y], [x, y])),
        routes: &[],
    }
}

fn structure<'a>(line: &'a str, id: &'a str, p: Placement<'a>) -> Target<'a> {
    Target::Object {
        line,
        kind: Kind::Structure,
        id,
        at: p,
    }
}

/// Route parts resolved from a fixed list.
struct Parts(Vec<(Kind, String)>, BTreeSet<String>);

impl Resolve for Parts {
    fn route_part(&self, _: &str, _: &str, _: &str, _: &str) -> Option<HashSet<(Kind, String)>> {
        Some(self.0.iter().cloned().collect())
    }

    fn region_lines(&self, _: &[[f64; 2]]) -> BTreeSet<String> {
        self.1.clone()
    }
}

#[test]
fn shipped_roles_hold_the_adr_permissions() {
    let d = RightsData::shipped();
    let p = |r: &str| d.roles[r].0;
    assert_eq!(p("viewer"), Perms::of(&[Permission::See]));
    assert!(p("designer").contains(Permission::Request));
    assert!(!p("designer").contains(Permission::Review));
    assert!(p("checker").contains(Permission::Review) && !p("checker").contains(Permission::Edit));
    assert!(p("library_maintainer").contains(Permission::Lock));
    assert_eq!(p("line_admin"), Perms::ALL);
    assert_eq!(p("network_admin"), Perms::ALL);
    assert_eq!(network_role(Role::Admin), "line_admin");
    for perm in Permission::ALL {
        assert_eq!(Permission::parse(perm.as_str()), Some(perm));
    }
}

#[test]
fn default_deny_union_and_see_needed() {
    let t = structure("A", "T1", at(5.0, 5.0));
    // Nothing without a grant.
    let d = data(vec![]);
    let ann = d.compile("ann", &NoModels);
    for p in Permission::ALL {
        assert!(!ann.allows(p, &t), "{p:?}");
    }
    // A user unknown to the network, or disabled: nothing whatever is set.
    let mut d = data(vec![grant(
        1,
        "user:zed",
        "network_admin",
        Scope::network(),
    )]);
    assert!(!d.compile("zed", &NoModels).allows(Permission::See, &t));
    d.users.insert("zed".into(), false);
    assert!(!d.compile("zed", &NoModels).allows(Permission::See, &t));
    d.users.insert("zed".into(), true);
    assert!(d.compile("zed", &NoModels).allows(Permission::See, &t));
    // The union of the user's and the groups' grants.
    let mut s = Scope::line("A");
    s.kind = Some("structure".into());
    let d = data(vec![
        grant(1, "group:crew", "viewer", Scope::line("A")),
        grant(2, "user:bob", "designer", s),
    ]);
    let bob = d.compile("bob", &NoModels);
    let cid = d.compile("cid", &NoModels);
    assert!(bob.allows(Permission::Edit, &t) && cid.allows(Permission::See, &t));
    assert!(!cid.allows(Permission::Edit, &t));
    assert!(!bob.allows(Permission::Edit, &structure("B", "T1", at(5.0, 5.0))));
    // Edit needs see: a role of edit only gives nothing.
    let mut d = data(vec![grant(1, "user:ann", "editor", Scope::network())]);
    d.roles
        .insert("editor".into(), (Perms::of(&[Permission::Edit]), false));
    assert!(!d.compile("ann", &NoModels).allows(Permission::Edit, &t));
    // A lock or a request needs no see.
    d.roles
        .insert("editor".into(), (Perms::of(&[Permission::Lock]), false));
    assert!(d.compile("ann", &NoModels).allows(Permission::Lock, &t));
}

#[test]
fn a_deny_wins_whatever_the_order() {
    let t = Target::Library {
        line: "B",
        kind: "costs",
        file: "rates",
        entry: None,
    };
    let other = Target::Library {
        line: "B",
        kind: "cables",
        file: "acsr",
        entry: None,
    };
    let costs = Scope {
        line: Some("B".into()),
        library_kind: Some("costs".into()),
        ..Scope::default()
    };
    for order in [false, true] {
        let mut v = vec![
            grant(1, "group:crew", "designer", Scope::network()),
            deny(2, "group:crew", None, costs.clone()),
        ];
        if order {
            v.reverse();
        }
        let d = data(v);
        let bob = d.compile("bob", &NoModels);
        assert!(!bob.allows(Permission::See, &t));
        assert!(!bob.allows(Permission::Edit, &t));
        assert!(bob.allows(Permission::Edit, &other));
        // Not seeing everything, nor line B whole (merges refused).
        assert!(!bob.sees_everything());
        assert!(!bob.sees_whole_line("B") && bob.sees_whole_line("A"));
        let (ok, grants, denies) = bob.explain(Permission::See, &t);
        assert!(!ok && grants == vec![1] && denies == vec![2]);
    }
    // A deny of the viewer role takes see, and so every permission
    // needing it, but leaves a lock.
    let d = data(vec![
        grant(1, "user:ann", "line_admin", Scope::line("B")),
        deny(2, "user:ann", Some("viewer"), costs),
    ]);
    let ann = d.compile("ann", &NoModels);
    assert!(!ann.allows(Permission::Edit, &t) && ann.allows(Permission::Lock, &t));
    // The owner, 2026-11-20 (phase 9's question 10): a deny of one entry
    // of a line's file denies the file whole (until it is filtered per
    // entry); a grant of one entry does not give the file.
    let mut entry = Scope {
        line: Some("B".into()),
        library_kind: Some("cables".into()),
        library_file: Some("acsr".into()),
        library_entry: Some("old-wire".into()),
        ..Scope::default()
    };
    let d = data(vec![
        grant(1, "user:cy", "designer", Scope::network()),
        deny(2, "user:cy", None, entry.clone()),
    ]);
    let cy = d.compile("cy", &NoModels);
    assert!(!cy.allows(Permission::See, &other));
    entry.library_entry = Some("new-wire".into());
    let d = data(vec![grant(1, "user:di", "designer", entry)]);
    assert!(!d.compile("di", &NoModels).allows(Permission::See, &other));
}

#[test]
fn regions_grant_inside_and_deny_what_they_touch() {
    let square = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let region = Scope {
        region: Some(square.clone()),
        ..Scope::default()
    };
    let d = data(vec![grant(1, "user:ann", "designer", region.clone())]);
    let ann = d.compile("ann", &Parts(vec![], ["A".to_owned()].into()));
    let inside = Placement {
        bounds: Some(Bounds::new([10.0, 10.0], [90.0, 20.0])),
        routes: &[],
    };
    let across = Placement {
        bounds: Some(Bounds::new([90.0, 10.0], [120.0, 20.0])),
        routes: &[],
    };
    let mk = |p| Target::Object {
        line: "A",
        kind: Kind::Section,
        id: "S1",
        at: p,
    };
    assert!(ann.allows(Permission::Edit, &mk(inside)));
    assert!(!ann.allows(Permission::See, &mk(across)), "partly outside");
    assert!(
        !ann.allows(Permission::See, &mk(Placement::default())),
        "no place"
    );
    // A notch cutting through: the corners inside, an edge across.
    let notched = vec![
        [0.0, 0.0],
        [50.0, 0.0],
        [50.0, 60.0],
        [51.0, 60.0],
        [51.0, 0.0],
        [100.0, 0.0],
        [100.0, 100.0],
        [0.0, 100.0],
    ];
    let b = Bounds::new([40.0, 10.0], [60.0, 20.0]);
    assert!(!region_holds(&notched, &b) && region_touches(&notched, &b));
    // A deny touching it takes it, a grant inside the deny's box but not
    // touching it keeps it.
    let small = Scope {
        region: Some(vec![[85.0, 0.0], [95.0, 0.0], [95.0, 5.0]]),
        ..Scope::default()
    };
    let d = data(vec![
        grant(1, "user:ann", "designer", region),
        deny(2, "user:ann", None, small),
    ]);
    let ann = d.compile("ann", &Parts(vec![], ["A".to_owned()].into()));
    assert!(ann.allows(Permission::Edit, &mk(inside)));
    let near = Placement {
        bounds: Some(Bounds::new([86.0, 1.0], [99.0, 3.0])),
        routes: &[],
    };
    assert!(!ann.allows(Permission::See, &mk(near)));
    // The region's lines' settings and libraries: seen, never edited.
    let meta = Target::Object {
        line: "A",
        kind: Kind::Meta,
        id: "line",
        at: Placement::default(),
    };
    let lib = Target::object("A", Kind::Library, "cables/acsr", Placement::default());
    assert!(ann.allows(Permission::See, &meta) && !ann.allows(Permission::Edit, &meta));
    assert!(ann.allows(Permission::See, &lib) && !ann.allows(Permission::Edit, &lib));
    let meta_b = Target::Object {
        line: "B",
        kind: Kind::Meta,
        id: "line",
        at: Placement::default(),
    };
    assert!(!ann.allows(Permission::See, &meta_b));
}

#[test]
fn routes_and_their_parts() {
    let on_r1 = vec!["R1".to_owned()];
    let on_r2 = vec!["R2".to_owned()];
    let p1 = Placement {
        bounds: None,
        routes: &on_r1,
    };
    let p2 = Placement {
        bounds: None,
        routes: &on_r2,
    };
    let route = Scope {
        line: Some("A".into()),
        route: Some("R1".into()),
        ..Scope::default()
    };
    let d = data(vec![grant(1, "user:ann", "designer", route.clone())]);
    let ann = d.compile("ann", &NoModels);
    assert!(ann.allows(Permission::Edit, &structure("A", "T1", p1)));
    assert!(!ann.allows(Permission::See, &structure("A", "T2", p2)));
    // The route itself.
    let r = Target::Object {
        line: "A",
        kind: Kind::Route,
        id: "R1",
        at: Placement::default(),
    };
    assert!(ann.allows(Permission::Edit, &r));
    // Not the line whole: no merge, no rollback; a designer to the
    // session's rules.
    assert!(!ann.allows(Permission::Edit, &Target::Line("A")));
    assert_eq!(ann.model_role("A"), Some(Role::Designer));
    assert_eq!(ann.model_role("B"), None);
    // A part: its members only, resolved.
    let part = Scope {
        from_structure: Some("T1".into()),
        to_structure: Some("T5".into()),
        ..route
    };
    let d = data(vec![grant(1, "user:ann", "designer", part)]);
    let parts = Parts(
        vec![(Kind::Structure, "T1".into()), (Kind::Section, "S1".into())],
        BTreeSet::new(),
    );
    let ann = d.compile("ann", &parts);
    assert!(ann.allows(Permission::Edit, &structure("A", "T1", p1)));
    assert!(!ann.allows(Permission::See, &structure("A", "T3", p1)));
    assert!(ann.has_route_parts("A"));
    // Unresolved: nothing.
    assert!(
        !d.compile("ann", &NoModels)
            .allows(Permission::See, &structure("A", "T1", p1))
    );
}

#[test]
fn the_line_and_the_network_as_wholes() {
    let mut kinds = Scope::line("A");
    kinds.kind = Some("structure".into());
    let d = data(vec![
        grant(1, "user:ann", "line_admin", Scope::line("A")),
        grant(2, "user:bob", "designer", kinds),
        grant(3, "user:cid", "network_admin", Scope::network()),
    ]);
    let (ann, bob, cid) = (
        d.compile("ann", &NoModels),
        d.compile("bob", &NoModels),
        d.compile("cid", &NoModels),
    );
    assert!(ann.allows(Permission::Edit, &Target::Line("A")));
    assert_eq!(ann.model_role("A"), Some(Role::Admin));
    assert!(!ann.allows(Permission::See, &Target::Network));
    assert!(!bob.allows(Permission::See, &Target::Line("A")));
    assert!(cid.allows(Permission::ManageRights, &Target::Network) && cid.sees_everything());
    assert_eq!(d.network_admins(), vec!["cid".to_owned()]);
    // Who gives what: the line admin within line A only.
    assert!(ann.manages(&Scope::line("A")));
    assert!(ann.manages(&Scope {
        line: Some("A".into()),
        route: Some("R1".into()),
        ..Scope::default()
    }));
    assert!(!ann.manages(&Scope::line("B")) && !ann.manages(&Scope::network()));
    assert!(cid.manages(&Scope::line("B")));
    // A deny of managing a part of A takes managing A whole.
    let d = data(vec![
        grant(1, "user:ann", "line_admin", Scope::line("A")),
        deny(
            2,
            "user:ann",
            Some("line_admin"),
            Scope {
                line: Some("A".into()),
                kind: Some("section".into()),
                ..Scope::default()
            },
        ),
    ]);
    let ann = d.compile("ann", &NoModels);
    assert!(!ann.manages(&Scope::line("A")));
    assert!(!ann.allows(Permission::Edit, &Target::Line("A")));
    assert_eq!(ann.model_role("A"), Some(Role::Designer));
}

#[test]
fn see_as_a_user_is_both() {
    let d = data(vec![
        grant(1, "user:ann", "network_admin", Scope::network()),
        grant(2, "user:bob", "viewer", Scope::line("A")),
    ]);
    let both = d.compile("ann", &NoModels).and(d.compile("bob", &NoModels));
    assert_eq!(both.seeing_as(), Some("bob"));
    assert!(both.allows(Permission::See, &structure("A", "T1", at(0.0, 0.0))));
    assert!(!both.allows(Permission::See, &structure("B", "T1", at(0.0, 0.0))));
    assert!(!both.allows(Permission::Edit, &structure("A", "T1", at(0.0, 0.0))));
    assert!(!both.sees_everything());
    assert_eq!(both.model_role("A"), Some(Role::Viewer));
}

#[test]
fn scopes_are_checked() {
    let bad = [
        Scope {
            from_structure: Some("T1".into()),
            ..Scope::default()
        },
        Scope {
            library_file: Some("x".into()),
            ..Scope::default()
        },
        Scope {
            library_kind: Some("costs".into()),
            route: Some("R".into()),
            ..Scope::default()
        },
        Scope {
            kind: Some("tower".into()),
            ..Scope::default()
        },
        Scope {
            region: Some(vec![[0.0, 0.0], [1.0, f64::NAN], [1.0, 1.0]]),
            ..Scope::default()
        },
        Scope {
            line: Some(" ".into()),
            ..Scope::default()
        },
    ];
    for s in bad {
        assert!(s.check().is_err(), "{s:?}");
    }
    let good = Scope {
        line: Some("B".into()),
        library_kind: Some("costs".into()),
        ..Scope::default()
    };
    assert!(good.check().is_ok());
    assert_eq!(good.scope_kind(), "library_kind");
    assert_eq!(Scope::network().scope_kind(), "network");
    assert!(Subject::parse("crew").is_err());
}

/// A random scope over a tiny world: lines A, B; routes R1 (A), R2 (B);
/// structures T1..T4; two regions; kinds; libraries.
fn random_scope(r: &mut Rng) -> Scope {
    let mut s = Scope::default();
    if r.next().is_multiple_of(3) {
        s.line = Some(["A", "B"][(r.next() % 2) as usize].into());
    }
    match r.next() % 6 {
        0 => s.route = Some(["R1", "R2"][(r.next() % 2) as usize].into()),
        1 => {
            s.region = Some(if r.next().is_multiple_of(2) {
                vec![[0.0, 0.0], [50.0, 0.0], [50.0, 50.0], [0.0, 50.0]]
            } else {
                vec![[40.0, 40.0], [100.0, 40.0], [100.0, 100.0]]
            })
        }
        2 => s.kind = Some(["structure", "section", "meta"][(r.next() % 3) as usize].into()),
        3 => s.object = Some(format!("T{}", 1 + r.next() % 4)),
        4 => {
            s.library_kind = Some(["costs", "cables"][(r.next() % 2) as usize].into());
            if r.next().is_multiple_of(2) {
                s.library_file = Some("f1".into());
            }
        }
        _ => {}
    }
    s
}

/// A target of the tiny world: line, kind, id, bounds, routes, library.
type WorldTarget = (
    String,
    Kind,
    String,
    Option<Bounds>,
    Vec<String>,
    Option<(String, String)>,
);

/// Every target of the tiny world.
fn world() -> Vec<WorldTarget> {
    let mut v = Vec::new();
    for (line, route) in [("A", "R1"), ("B", "R2")] {
        for k in 1..=4 {
            let x = (k as f64) * 22.0;
            v.push((
                line.to_owned(),
                Kind::Structure,
                format!("T{k}"),
                Some(Bounds::new([x, x], [x + 5.0, x + 5.0])),
                vec![route.to_owned()],
                None,
            ));
        }
        v.push((
            line.to_owned(),
            Kind::Section,
            "S1".into(),
            Some(Bounds::new([20.0, 20.0], [70.0, 70.0])),
            vec![route.to_owned()],
            None,
        ));
        v.push((
            line.to_owned(),
            Kind::Meta,
            "line".into(),
            None,
            vec![],
            None,
        ));
        for (k, f) in [("costs", "f1"), ("cables", "f1"), ("cables", "f2")] {
            v.push((
                line.to_owned(),
                Kind::Library,
                format!("{k}/{f}"),
                None,
                vec![],
                Some((k.into(), f.into())),
            ));
        }
    }
    v
}

/// The property (ADR-0013 phase 7's tests): over random roles and
/// assignments, (1) a permission is allowed only with a grant covering
/// the target with it, (2) never with a deny covering it, (3) a permission
/// needing see only with see, (4) the order of assignments changes
/// nothing, (5) adding a deny never allows more, adding a grant never
/// less; (6) a user without assignments has nothing.
#[test]
fn property_random_assignments() {
    let mut r = Rng(0x5eed_7a11);
    let roles = [
        "viewer",
        "designer",
        "checker",
        "library_maintainer",
        "line_admin",
        "odd",
    ];
    let world = world();
    let resolve = Parts(
        vec![(Kind::Structure, "T1".into())],
        ["A".to_owned()].into(),
    );
    for round in 0..300 {
        let mut d = data(vec![]);
        d.roles.insert(
            "odd".into(),
            (Perms::of(&[Permission::Edit, Permission::Lock]), false),
        );
        let n = 1 + (r.next() % 6) as usize;
        for id in 0..n {
            let subject = ["user:ann", "group:crew"][(r.next() % 2) as usize];
            let role = roles[(r.next() % roles.len() as u64) as usize];
            let scope = random_scope(&mut r);
            let a = if r.next().is_multiple_of(4) {
                deny(
                    id as i64,
                    subject,
                    (r.next().is_multiple_of(2)).then_some(role),
                    scope,
                )
            } else {
                grant(id as i64, subject, role, scope)
            };
            d.assignments.push(a);
        }
        let ann = d.compile("ann", &resolve);
        let mut shuffled = d.clone();
        shuffled.assignments.reverse();
        let ann_rev = shuffled.compile("ann", &resolve);
        let mut more_deny = d.clone();
        more_deny
            .assignments
            .push(deny(99, "user:ann", None, random_scope(&mut r)));
        let ann_deny = more_deny.compile("ann", &resolve);
        let mut more_grant = d.clone();
        more_grant
            .assignments
            .push(grant(98, "user:ann", "designer", random_scope(&mut r)));
        let ann_grant = more_grant.compile("ann", &resolve);
        let none = data(vec![]).compile("ann", &resolve);
        for (line, kind, id, bounds, routes, _) in &world {
            let t = Target::object(
                line,
                *kind,
                id,
                Placement {
                    bounds: *bounds,
                    routes,
                },
            );
            for p in Permission::ALL {
                let ok = ann.allows(p, &t);
                let (_, grants, denies) = ann.explain(p, &t);
                if ok {
                    assert!(
                        !grants.is_empty(),
                        "round {round}: {p:?} {id} without a grant"
                    );
                    assert!(
                        denies.is_empty(),
                        "round {round}: {p:?} {id} denied yet allowed"
                    );
                }
                if ok && p.needs_see() {
                    assert!(
                        ann.allows(Permission::See, &t),
                        "round {round}: {p:?} without see"
                    );
                }
                assert_eq!(ok, ann_rev.allows(p, &t), "round {round}: order");
                assert!(
                    !ann_deny.allows(p, &t) || ok,
                    "round {round}: a deny allowed more"
                );
                assert!(
                    ann_grant.allows(p, &t) || !ok,
                    "round {round}: a grant allowed less"
                );
                assert!(!none.allows(p, &t));
            }
        }
        for line in ["A", "B"] {
            assert_eq!(
                ann.allows(Permission::See, &Target::Line(line)),
                ann_rev.allows(Permission::See, &Target::Line(line))
            );
            // Seeing the line whole means seeing each of its objects.
            if ann.sees_whole_line(line) {
                for (l, kind, id, bounds, routes, _) in &world {
                    if l == line {
                        let t = Target::object(
                            l,
                            *kind,
                            id,
                            Placement {
                                bounds: *bounds,
                                routes,
                            },
                        );
                        assert!(
                            ann.allows(Permission::See, &t),
                            "round {round}: {line} {id} {:#?}",
                            d.assignments
                        );
                    }
                }
            }
        }
    }
}

/// The server's unit of work (`Session::atomically`): a command refused
/// after it ran (its objects decided by the rights) leaves nothing — the
/// line in memory, the file, the log; and the roles come from the role
/// source when one is set.
#[test]
fn a_refusal_after_the_command_leaves_nothing() {
    use std::sync::Arc;
    struct Roles;
    impl crate::RoleSource for Roles {
        fn role(&self, user: &str) -> Option<Role> {
            (user == "zed").then_some(Role::Designer)
        }
    }
    let line = crate::in_place_tests::rich();
    let dir = std::env::temp_dir().join(format!("ohl-rights-unit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    for ext in ["", "-wal", "-shm"] {
        std::fs::remove_file(format!("{}{ext}", path.display())).ok();
    }
    let mut s = crate::Session::open(&path).unwrap();
    s.import(&line, "ana").unwrap();
    let id = s.line().structures[1].id.clone();
    let p = s.line().structures[1].position;
    let seq = s.last_seq().unwrap();
    let moved = crate::Command::MoveStructure {
        id: id.clone(),
        position: [p[0] + 3.0, p[1], p[2]],
    };
    let refused = s.atomically(|s| {
        let a = s.with_line("main", |s| s.apply("ana", moved.clone()))?;
        assert!(a.objects.contains(&(Kind::Structure, id.clone())));
        Err::<(), _>(crate::SessionError::Refused("decided after it ran".into()))
    });
    assert!(refused.is_err());
    assert_eq!(
        s.line().structure(&id).unwrap().position,
        p,
        "the line put back"
    );
    assert_eq!(s.last_seq().unwrap(), seq, "no log entry kept");
    drop(s);
    let mut s = crate::Session::open(&path).unwrap();
    assert_eq!(
        s.line().structure(&id).unwrap().position,
        p,
        "nothing in the file"
    );
    // The roles from the role source: zed is a designer, ana nobody.
    s.set_role_source(Some(Arc::new(Roles)));
    assert_eq!(s.role("zed").unwrap(), Some(Role::Designer));
    assert_eq!(s.role("ana").unwrap(), None);
    s.apply("zed", moved).unwrap();
    assert!(
        s.apply("ana", crate::Command::MoveStructure { id, position: p })
            .is_err()
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// The network's own library (phase 9): covered by scopes of no line, no
/// place and no object kind but `library`; a maintainer of one kind holds
/// nothing of another; a grant naming an entry covers only it, a deny
/// naming one covers its file and kind too.
#[test]
fn the_country_library_is_of_no_line() {
    let cables = Target::CountryLibrary {
        kind: "cables",
        file: Some("acsr.ohlw"),
        entry: Some("drake"),
    };
    let kind = Target::CountryLibrary {
        kind: "cables",
        file: None,
        entry: None,
    };
    let costs = Target::CountryLibrary {
        kind: "costs",
        file: None,
        entry: None,
    };
    let of_kind = |k: &str| Scope {
        library_kind: Some(k.into()),
        ..Scope::default()
    };
    let mut entry = of_kind("cables");
    entry.library_file = Some("acsr.ohlw".into());
    entry.library_entry = Some("drake".into());
    let mut on_line = of_kind("cables");
    on_line.line = Some("A".into());
    let d = data(vec![
        grant(1, "user:ann", "library_maintainer", of_kind("cables")),
        grant(2, "user:bob", "designer", Scope::line("A")),
        grant(3, "user:bob", "viewer", on_line),
        grant(4, "user:cid", "viewer", entry.clone()),
    ]);
    let ann = d.compile("ann", &NoModels);
    let bob = d.compile("bob", &NoModels);
    let cid = d.compile("cid", &NoModels);
    for p in [Permission::See, Permission::Review, Permission::Lock] {
        assert!(ann.allows(p, &cables) && ann.allows(p, &kind), "{p:?}");
        assert!(!ann.allows(p, &costs), "{p:?}");
    }
    // A line's designer, or a library kind on a line: not the network's.
    for t in [&cables, &kind] {
        assert!(!bob.allows(Permission::See, t));
        assert!(!bob.allows(Permission::Request, t));
    }
    // Their own line's library files: yes.
    assert!(bob.allows(
        Permission::Request,
        &Target::Library {
            line: "A",
            kind: "cables",
            file: "acsr.ohlw",
            entry: Some("drake"),
        }
    ));
    // An entry's grant: that entry only.
    assert!(cid.allows(Permission::See, &cables));
    assert!(!cid.allows(Permission::See, &kind));
    // An entry's deny: its file and kind too, whatever the order.
    for order in [false, true] {
        let mut v = vec![
            grant(1, "user:ann", "library_maintainer", of_kind("cables")),
            deny(2, "user:ann", None, entry.clone()),
        ];
        if order {
            v.reverse();
        }
        let ann = data(v).compile("ann", &NoModels);
        assert!(!ann.allows(Permission::See, &cables));
        assert!(!ann.allows(Permission::Lock, &kind));
        assert!(ann.allows(
            Permission::See,
            &Target::CountryLibrary {
                kind: "cables",
                file: Some("acsr.ohlw"),
                entry: Some("other"),
            }
        ));
    }
    // A network admin: everything.
    let d = data(vec![grant(
        1,
        "user:ann",
        "network_admin",
        Scope::network(),
    )]);
    let ann = d.compile("ann", &NoModels);
    assert!(ann.allows(Permission::Lock, &costs));
}

#[test]
fn a_region_whose_edges_cross_is_refused() {
    // 2026-11-28: a polygon drawn on OHL-Server's map.
    let square = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let bow = vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]];
    let l_shape = vec![
        [0.0, 0.0],
        [5.0, 0.0],
        [10.0, 0.0],
        [10.0, 5.0],
        [5.0, 5.0],
        [5.0, 10.0],
        [0.0, 10.0],
    ];
    let back = vec![
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 5.0],
        [12.0, 0.0],
        [12.0, 5.0],
    ];
    assert!(!region_self_crosses(&square));
    assert!(!region_self_crosses(&l_shape));
    assert!(region_self_crosses(&bow));
    assert!(region_self_crosses(&back));
    let region = |r: Vec<[f64; 2]>| Scope {
        region: Some(r),
        ..Scope::default()
    };
    assert!(region(l_shape).check().is_ok());
    assert!(region(bow).check().is_err());
}
