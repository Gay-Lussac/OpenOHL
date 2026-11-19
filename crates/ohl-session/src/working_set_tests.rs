// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Working sets ([`crate::working_set`], phase 6): the picks snapped out to
//! whole tension sections; a replica written, then fed by rows after
//! random commands on the whole model, holding at every step what a
//! replica written again would hold; a route added; a replica joining
//! again with what it holds.
//!
//! The line: the example's two routes, `main` (T1–T5, sections S1 and S2
//! from T1 to T5) and `branch` (T3, T6, T7, section S3), sharing T3, with
//! a spacer, a jumper on a tap of a span, a jumper on that jumper's point
//! and a circuit start (the in-place tests' line).

use super::*;
use crate::in_place_tests::{Rng, command, rich};
use crate::working_set::{Extent, Pick, Rows, object_hash};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-working-set-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn whole(name: &str) -> (PathBuf, Session) {
    let d = dir(name);
    let mut s = Session::open(&d.join("whole.ohl")).unwrap();
    s.import(&rich(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    (d, s)
}

fn ids(e: &Extent, kind: Kind) -> Vec<&str> {
    e.wanted
        .iter()
        .filter(|(k, _)| *k == kind)
        .map(|(_, id)| id.as_str())
        .collect()
}

/// What every extent must be: each section whole with its structures; the
/// context closed under references; the settings and library files kept.
fn sound(s: &mut Session, e: &Extent) {
    let line = s.line().clone();
    for (k, id) in &e.wanted {
        assert!(e.keep.contains(&(*k, id.clone())), "{id} kept");
    }
    for sec in ids(e, Kind::Section) {
        for p in &line.sections.iter().find(|x| x.id == sec).unwrap().points {
            assert!(
                e.wanted.contains(&(Kind::Structure, p.structure.clone())),
                "{sec}'s structure {}",
                p.structure
            );
        }
    }
    let index = ohl_line::index::LineIndex::of(&line);
    for (k, id) in &e.keep {
        let Some(of) = crate::in_place::of_kind(*k) else {
            continue;
        };
        let item = index.get(&line, of, id).expect("kept objects exist");
        for t in item.targets() {
            let key = match t {
                ohl_line::index::Target::Object(of, id) => (crate::in_place::kind_of(of), id),
                ohl_line::index::Target::Point(st, _) => (Kind::Structure, st),
            };
            assert!(e.keep.contains(&key), "{id} refers to {key:?}: kept");
        }
    }
    assert!(e.keep.iter().any(|(k, _)| *k == Kind::Meta));
    assert_eq!(e.summary.structures, ids(e, Kind::Structure).len());
    assert_eq!(e.summary.sections, ids(e, Kind::Section).len());
    assert_eq!(e.summary.context, e.keep.len() - e.wanted.len());
}

#[test]
fn picks_snap_out_to_whole_tension_sections() {
    let (_d, mut s) = whole("picks");
    // A whole route: every object of it.
    let main = s
        .pick_extent(&[Pick::Route {
            route: "main".into(),
        }])
        .unwrap();
    sound(&mut s, &main);
    assert_eq!(ids(&main, Kind::Structure), ["T1", "T2", "T3", "T4", "T5"]);
    assert!(main.wanted.contains(&(Kind::Route, "main".into())));
    assert!(main.wanted.contains(&(Kind::Alignment, "main".into())));
    assert!(
        main.summary
            .routes
            .iter()
            .any(|r| r.route == "main" && r.whole)
    );
    // A station range inside one span of main: S1 and S2 whole (T1 to
    // T5), the spacer and the jumpers on them.
    let a = s.line().alignments.iter().find(|a| a.id == "main").unwrap();
    let poly: Vec<[f64; 3]> = a
        .points
        .iter()
        .map(|p| s.line().alignment_point(p).unwrap().position)
        .collect();
    let st = |s: &Session, id: &str| {
        ohl_line::route::project(&poly, s.line().structure(id).unwrap().position).0
    };
    let (t2, t3) = (st(&s, "T2"), st(&s, "T3"));
    let range = s
        .pick_extent(&[Pick::Stations {
            route: "main".into(),
            alignment: None,
            from: t2 + 1.0,
            to: t3 - 1.0,
        }])
        .unwrap();
    sound(&mut s, &range);
    assert_eq!(ids(&range, Kind::Section), ["S1", "S2"]);
    assert_eq!(ids(&range, Kind::Structure), ["T1", "T2", "T3", "T4", "T5"]);
    assert!(range.wanted.contains(&(Kind::Spacer, "SP1".into())));
    assert!(range.wanted.contains(&(Kind::Jumper, "JT".into())));
    let r = &range.summary.routes[0];
    assert!(!r.whole);
    assert_eq!(
        (r.first.as_deref(), r.last.as_deref()),
        (Some("T1"), Some("T5"))
    );
    assert_eq!(r.outline.len(), 5);
    assert!(
        range
            .summary
            .notes
            .iter()
            .any(|n| n.contains("whole tension sections"))
    );
    // From one structure to another: the same.
    let two = s
        .pick_extent(&[Pick::Structures {
            route: "main".into(),
            from: "T3".into(),
            to: "T2".into(),
        }])
        .unwrap();
    assert_eq!(two.wanted, range.wanted);
    // An area around T6: S3 whole (T3, T6, T7), not main's sections.
    let t6 = s.line().structure("T6").unwrap().position;
    let square = |c: [f64; 3], h: f64| {
        vec![
            [c[0] - h, c[1] - h],
            [c[0] + h, c[1] - h],
            [c[0] + h, c[1] + h],
            [c[0] - h, c[1] + h],
        ]
    };
    let area = s
        .pick_extent(&[Pick::Area {
            polygon: square(t6, 5.0),
        }])
        .unwrap();
    sound(&mut s, &area);
    assert_eq!(ids(&area, Kind::Section), ["S3"]);
    assert_eq!(ids(&area, Kind::Structure), ["T3", "T6", "T7"]);
    assert!(!area.wanted.contains(&(Kind::Section, "S1".into())));
    // An area crossed by a span only (its ends outside): that span's
    // sections whole.
    let (p1, p2) = (
        s.line().structure("T1").unwrap().position,
        s.line().structure("T2").unwrap().position,
    );
    let mid = [(p1[0] + p2[0]) / 2.0, (p1[1] + p2[1]) / 2.0, 0.0];
    let crossed = s
        .pick_extent(&[Pick::Area {
            polygon: square(mid, 2.0),
        }])
        .unwrap();
    assert_eq!(ids(&crossed, Kind::Section), ["S1", "S2"]);
    // Two picks together; the routes' extents.
    let both = s
        .pick_extent(&[
            Pick::Route {
                route: "branch".into(),
            },
            Pick::Stations {
                route: "main".into(),
                alignment: None,
                from: t2 + 1.0,
                to: t3 - 1.0,
            },
        ])
        .unwrap();
    sound(&mut s, &both);
    assert_eq!(ids(&both, Kind::Section), ["S1", "S2", "S3"]);
    assert_eq!(both.summary.routes.len(), 2);
    // A route's structures by station, to pick a part of it.
    let detail = s.route_detail("main").unwrap();
    let order: Vec<&str> = detail.structures.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(order, ["T1", "T2", "T3", "T4", "T5"]);
    assert!(
        detail
            .structures
            .windows(2)
            .all(|w| w[0].station <= w[1].station)
    );
    assert_eq!(detail.alignments[0].id, "main");
    assert!(detail.alignments[0].length > 0.0);
    assert!(s.route_detail("nowhere").is_err());
    // Refused.
    for (picks, words) in [
        (vec![], "pick at least one"),
        (
            vec![Pick::Route {
                route: "nowhere".into(),
            }],
            "no route nowhere",
        ),
        (
            vec![Pick::Structures {
                route: "main".into(),
                from: "T6".into(),
                to: "T2".into(),
            }],
            "T6 is not on route main",
        ),
        (
            vec![Pick::Area {
                polygon: vec![[0.0, 0.0], [1.0, 1.0]],
            }],
            "three corners",
        ),
        (
            vec![Pick::Area {
                polygon: square([1e7, 1e7, 0.0], 1.0),
            }],
            "holds no structure",
        ),
    ] {
        let e = s.pick_extent(&picks).unwrap_err().to_string();
        assert!(e.contains(words), "{words}: {e}");
    }
}

/// The replica holds what the whole model holds of the extent: the same
/// rows (JSON and places), the working set noted, the line valid.
fn replica_is(r: &Session, s: &Session, e: &Extent, at: &str) {
    let mine: BTreeMap<&(Kind, String), &Object> = r
        .objects
        .iter()
        .filter(|((k, _), _)| *k != Kind::Report && *k != Kind::Annotation)
        .collect();
    let theirs: BTreeMap<&(Kind, String), &Object> = s
        .objects
        .iter()
        .filter(|(k, _)| e.keep.contains(k))
        .collect();
    assert_eq!(
        mine.keys().collect::<Vec<_>>(),
        theirs.keys().collect::<Vec<_>>(),
        "{at}: the same objects"
    );
    for (k, o) in &mine {
        assert!(
            o.value == theirs[k].value && o.ord == theirs[k].ord,
            "{at}: {k:?} as the server holds it"
        );
    }
    assert_eq!(r.part(), Some(&e.wanted), "{at}: the working set");
    r.check_kept().unwrap_or_else(|x| panic!("{at}: {x}"));
}

#[test]
fn a_replica_follows_by_rows() {
    let (d, mut s) = whole("rows");
    let mut picks = vec![Pick::Area {
        polygon: {
            let p = s.line().structure("T2").unwrap().position;
            vec![
                [p[0] - 3.0, p[1] - 3.0],
                [p[0] + 3.0, p[1] - 3.0],
                [p[0] + 3.0, p[1] + 3.0],
                [p[0] - 3.0, p[1] + 3.0],
            ]
        },
    }];
    let mut e = s.pick_extent(&picks).unwrap();
    let meta = vec![("network.line".to_owned(), "A".to_owned())];
    s.write_working_set(&e, &d.join("r.ohl"), "ana", &meta)
        .unwrap();
    assert!(
        s.write_working_set(&e, &d.join("r.ohl"), "ana", &meta)
            .is_err(),
        "exists"
    );
    // The replica as a user left it: joins again later.
    std::fs::copy(d.join("r.ohl"), d.join("stale.ohl")).unwrap();
    let mut r = Session::open(&d.join("r.ohl")).unwrap();
    assert_eq!(r.file_meta("network.line").unwrap().as_deref(), Some("A"));
    replica_is(&r, &s, &e, "written");
    // Its import logged with an empty command (2026-11-19): the objects
    // once, in their rows; the log still says what it changed.
    let log = r.log().unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].kind, "import");
    assert!(!log[0].objects.is_empty());
    let entry = r.conn.entry(log[0].seq).unwrap().unwrap();
    assert_eq!((entry.4.as_str(), entry.5.as_str()), (crate::EMPTY_COMMAND, crate::EMPTY_COMMAND));
    assert!(r.undo("ana").is_err(), "an import cannot be undone");
    // The next command of the replica is logged in full.
    assert!(!r.lean_import.load(std::sync::atomic::Ordering::SeqCst));
    let mut users = s.extent_users(&e);
    let mut rng = Rng(7);
    let (mut applied_n, mut moved_in) = (0, 0);
    for step in 0..300 {
        let c = command(&s, &mut rng);
        let user = if rng.below(3) == 0 { "ben" } else { "ana" };
        let Ok(a) = s.apply(user, c) else {
            continue;
        };
        applied_n += 1;
        let changed: BTreeSet<(Kind, String)> = a.objects.iter().cloned().collect();
        let new = s.extent_of(&picks);
        let rows = s.rows_for(&e, &users, &new, &changed);
        moved_in += rows
            .puts
            .iter()
            .filter(|p| !e.keep.contains(&(p.kind, p.id.clone())))
            .count();
        // Through JSON, as the server sends them.
        let rows: Rows = serde_json::from_str(&serde_json::to_string(&rows).unwrap()).unwrap();
        r.take_rows("ana", rows).unwrap();
        users = s.extent_users(&new);
        e = new;
        replica_is(&r, &s, &e, &format!("step {step}"));
        if step % 75 == 74 {
            // A replica written again holds the same line.
            let again = d.join(format!("again-{step}.ohl"));
            s.write_working_set(&e, &again, "ana", &[]).unwrap();
            let fresh = Session::open(&again).unwrap();
            assert!(*fresh.line() == *r.line(), "step {step}: the same line");
        }
    }
    assert!(applied_n > 100, "{applied_n} commands applied");
    assert!(
        moved_in > 0,
        "objects came into the working set as it changed"
    );
    // A route added: its objects come, nothing else changes.
    picks.push(Pick::Route {
        route: "branch".into(),
    });
    let new = s.extent_of(&picks);
    let rows = s.rows_for(&e, &users, &new, &BTreeSet::new());
    assert!(rows.deletes.is_empty());
    assert!(
        rows.puts
            .iter()
            .all(|p| !e.keep.contains(&(p.kind, p.id.clone())))
    );
    r.take_rows("ana", rows).unwrap();
    e = new;
    replica_is(&r, &s, &e, "branch added");
    // A replica joining again with what it holds: only what differs.
    let mut stale = Session::open(&d.join("stale.ohl")).unwrap();
    let held = stale.held();
    assert!(held.values().all(|h| *h != 0));
    let rows = s.rows_against(&held, &e);
    let unchanged = e
        .keep
        .iter()
        .filter(|k| held.get(*k) == s.objects.get(*k).map(|o| object_hash(&o.value)).as_ref())
        .count();
    assert_eq!(rows.puts.len(), e.keep.len() - unchanged);
    stale.take_rows("ana", rows).unwrap();
    replica_is(&stale, &s, &e, "joined again");
    assert!(*stale.line() == *r.line());
    // Nothing left to send.
    let rows = s.rows_against(&stale.held(), &e);
    assert!(rows.puts.is_empty() && rows.deletes.is_empty());
}
