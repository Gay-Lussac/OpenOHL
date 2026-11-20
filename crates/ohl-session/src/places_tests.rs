// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The place index ([`crate::places`]): every extent the same with it as
//! by the whole scan — whole routes, station ranges, areas small and
//! large, structure to structure — on the in-place tests' line, before
//! and after each of hundreds of random commands (the index kept up to
//! date in place, or dropped and made again).

use super::*;
use crate::in_place_tests::{Rng, command, rich};
use crate::working_set::Pick;

fn open(name: &str) -> Session {
    // Cells of 4 m: the line's structures in cells of their own, its spans
    // across several.
    crate::places::TEST_CELL.with(|c| c.set(4.0));
    let dir = std::env::temp_dir().join(format!("ohl-places-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::open(&dir.join("p.ohl")).unwrap();
    s.import(&rich(), "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    s
}

fn square(c: [f64; 2], h: f64) -> Vec<[f64; 2]> {
    vec![
        [c[0] - h, c[1] - h],
        [c[0] + h, c[1] - h],
        [c[0] + h, c[1] + h],
        [c[0] - h, c[1] + h],
    ]
}

/// The picks compared: each route whole, a station range and the first
/// to the last structure of each; an area around each structure, a thin
/// one across a span (no structure inside), the whole line's bounds, a
/// random triangle.
fn picks_of(s: &Session, rng: &mut Rng) -> Vec<Vec<Pick>> {
    let line = s.line();
    let mut out = Vec::new();
    for r in &line.routes {
        out.push(vec![Pick::Route {
            route: r.id.clone(),
        }]);
        let a = rng.below(400) as f64;
        out.push(vec![Pick::Stations {
            route: r.id.clone(),
            alignment: None,
            from: a,
            to: a + rng.below(600) as f64,
        }]);
        if let (Some(f), Some(t)) = (line.structures.first(), line.structures.last()) {
            out.push(vec![Pick::Structures {
                route: r.id.clone(),
                from: f.id.clone(),
                to: t.id.clone(),
            }]);
        }
    }
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for t in &line.structures {
        let p = [t.position[0], t.position[1]];
        out.push(vec![Pick::Area {
            polygon: square(p, 1.0 + rng.below(30) as f64),
        }]);
        b = [
            b[0].min(p[0]),
            b[1].min(p[1]),
            b[2].max(p[0]),
            b[3].max(p[1]),
        ];
    }
    // Across the middle of a span: no structure inside, the span crossing.
    if let Some(sec) = line.sections.first()
        && let [a, z, ..] = sec.points.as_slice()
        && let (Some(a), Some(z)) = (line.structure(&a.structure), line.structure(&z.structure))
    {
        let m = [
            (a.position[0] + z.position[0]) / 2.0,
            (a.position[1] + z.position[1]) / 2.0,
        ];
        out.push(vec![Pick::Area {
            polygon: square(m, 0.5),
        }]);
    }
    if b[0].is_finite() {
        out.push(vec![Pick::Area {
            polygon: vec![[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]],
        }]);
        let at = |rng: &mut Rng| {
            [
                b[0] + (b[2] - b[0]) * rng.below(1000) as f64 / 1000.0,
                b[1] + (b[3] - b[1]) * rng.below(1000) as f64 / 1000.0,
            ]
        };
        out.push(vec![Pick::Area {
            polygon: vec![at(rng), at(rng), at(rng)],
        }]);
    }
    out
}

/// Every pick's extent (and its structures' users) the same with the
/// place index as by the whole scan; a strict pick refused alike.
fn same(s: &mut Session, picks: &[Vec<Pick>], at: &str) {
    for p in picks {
        let a = s.extent_of(p);
        let au = s.extent_users(&a);
        let strict = s.pick_extent(p).map_err(|e| e.to_string());
        s.set_whole_scan(true);
        let b = s.extent_of(p);
        let bu = s.extent_users(&b);
        let strict_b = s.pick_extent(p).map_err(|e| e.to_string());
        s.set_whole_scan(false);
        assert_eq!(a, b, "{at}: {p:?}");
        assert_eq!(au, bu, "{at}: users of {p:?}");
        assert_eq!(strict, strict_b, "{at}: {p:?} strictly");
    }
}

#[test]
fn extents_by_the_place_index_are_the_whole_scans() {
    let mut s = open("random");
    let mut rng = Rng(11);
    let picks = picks_of(&s, &mut rng);
    same(&mut s, &picks, "at first");
    assert!(s.places.is_some(), "made when first needed");
    let (mut applied, mut kept) = (0, 0);
    for step in 0..400 {
        let c = command(&s, &mut rng);
        let user = if rng.below(3) == 0 { "ben" } else { "ana" };
        if s.apply(user, c).is_err() {
            continue;
        }
        applied += 1;
        kept += usize::from(s.places.is_some());
        let picks = picks_of(&s, &mut rng);
        same(&mut s, &picks, &format!("step {step}"));
        // Dropped and made again: the same as kept up to date.
        if step % 7 == 0 {
            let before: Vec<_> = picks.iter().map(|p| s.extent_of(p)).collect();
            s.places = None;
            let again: Vec<_> = picks.iter().map(|p| s.extent_of(p)).collect();
            assert_eq!(before, again, "step {step}: made again");
        }
    }
    assert!(applied > 100, "{applied} commands applied");
    assert!(kept > 20, "the index kept up to date in place {kept} times");
}

#[test]
fn the_index_follows_a_move_and_drops_on_a_route_change() {
    let mut s = open("follows");
    let all = Pick::Route {
        route: "main".into(),
    };
    s.pick_extent(std::slice::from_ref(&all)).unwrap();
    let longest = s.places.as_ref().unwrap().longest();
    // T2 moved 3 km away: its cell, its spans (the longest grows), an
    // area there finds it and its sections.
    let p = s.line().structure("T2").unwrap().position;
    let far = [p[0] + 3000.0, p[1] + 2000.0, p[2]];
    s.apply(
        "ana",
        Command::MoveStructure {
            id: "T2".into(),
            position: far,
        },
    )
    .unwrap();
    let places = s.places.as_ref().expect("kept up to date in place");
    assert!(places.longest() > longest + 1000.0);
    let area = [Pick::Area {
        polygon: square([far[0], far[1]], 5.0),
    }];
    let e = s.pick_extent(&area).unwrap();
    assert!(e.wanted.contains(&(Kind::Structure, "T2".into())));
    same(&mut s, &[area.to_vec(), vec![all.clone()]], "moved");
    // A route's change: dropped, made again when next needed.
    let r = s.line().routes.iter().find(|r| r.id == "main").unwrap();
    let mut v = serde_json::to_value(r).unwrap();
    v["name"] = "main again".into();
    s.apply(
        "ana",
        Command::Put {
            kind: Kind::Route,
            id: "main".into(),
            value: v,
            ord: None,
        },
    )
    .unwrap();
    assert!(s.places.is_none(), "dropped on a route's change");
    same(&mut s, &[area.to_vec(), vec![all]], "after the route");
}
