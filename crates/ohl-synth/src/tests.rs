// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use super::*;

fn small(seed: u64) -> Country {
    country(&Settings {
        pylons: 600,
        seed,
        ..Settings::default()
    })
    .unwrap()
}

/// The line's SHA-256 per seed (600 pylons): a change of the generator
/// that changes its output must change these on purpose (and the
/// baselines measured on it are then measured again).
#[test]
fn hash_per_seed() {
    for (seed, hash) in [
        (
            1,
            "1fe927632b9e382bc1f1daa9b5d9a3cb964184c96773a0e33ab0dd7bc2df9999",
        ),
        (
            2,
            "92b87ae8e443001daa5e9b94fa60f400c6b170562a2b195caea661e41a2c6cae",
        ),
    ] {
        let c = small(seed);
        assert_eq!(c.summary.hash, hash, "seed {seed}: {:#?}", c.summary);
        assert_eq!(c.hash(), hash);
    }
}

#[test]
fn same_seed_same_country_other_seed_another() {
    let a = small(7);
    let b = small(7);
    assert_eq!(a.line, b.line);
    assert_eq!(a.summary, b.summary);
    assert_ne!(small(8).summary.hash, a.summary.hash);
}

#[test]
fn counts_follow_the_settings() {
    let c = country(&Settings {
        pylons: 3000,
        ..Settings::default()
    })
    .unwrap();
    let s = &c.summary;
    assert_eq!(s.pylons, 3000);
    assert_eq!(c.line.structures.len(), 3000);
    assert_eq!(c.line.sections.len(), s.sections);
    assert_eq!(c.line.jumpers.len(), s.jumpers);
    assert!((2.1..2.9).contains(&s.systems_per_pylon), "{s:#?}");
    assert!((0.04..0.2).contains(&s.shared_share), "{s:#?}");
    // Sections of about two spans (the plan's proportions).
    let spans = s.wire_spans as f64 / s.sections as f64;
    assert!((1.5..2.6).contains(&spans), "{spans}");
    // Every pylon within the plane's extent, grown by the longest route.
    assert!(c.line.structures.iter().all(|t| t.position[0] > 0.0));
}

#[test]
fn lidar_tiles_are_deterministic_and_dense_as_asked() {
    let c = small(1);
    let corner = c.sample_tiles[0];
    let a = lidar_tile(1, corner, 0.5);
    let b = lidar_tile(1, corner, 0.5);
    assert_eq!(a.len(), (0.5 * TILE * TILE) as usize);
    assert!(
        a.iter()
            .zip(&b)
            .all(|(p, q)| p.x == q.x && p.z == q.z && p.class == q.class)
    );
    assert!(a.iter().any(|p| p.class == 5) && a.iter().any(|p| p.class == 2));
    assert!(a.iter().all(|p| p.x >= corner[0] && p.x < corner[0] + TILE));
}

#[test]
fn written_project_opens_and_takes_an_edit() {
    let dir = std::env::temp_dir().join(format!("ohl-synth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let c = country(&Settings {
        pylons: 120,
        ..Settings::default()
    })
    .unwrap();
    let path = dir.join("c.ohl");
    project::write_project(&c, &path, "synth").unwrap();
    let tiles = project::add_lidar(&c, &path, 0.2, std::time::Duration::from_secs(60)).unwrap();
    assert_eq!(tiles.len(), c.sample_tiles.len());
    assert!(tiles.iter().all(|t| t.bytes > 0));
    let b = project::measure(&path, "synth", 2, std::time::Duration::from_secs(60), false).unwrap();
    assert_eq!(b.move_s.len(), 2);
    assert_eq!(b.undo_s.len(), 2);
    assert!(b.stopped.is_none());
    let session = ohl_session::Session::open(&path).unwrap();
    assert_eq!(session.line().structures.len(), 120);
    // Undone: as generated.
    assert_eq!(
        session.line().structures[1].position,
        c.line.structures[1].position
    );
    drop(session);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Sections of five spans asked: about five on average, the default's
/// output unchanged (its hashes above).
#[test]
fn longer_sections_when_asked() {
    let c = country(&Settings {
        pylons: 3000,
        section_spans: 5.0,
        ..Settings::default()
    })
    .unwrap();
    let spans = c.summary.wire_spans as f64 / c.summary.sections as f64;
    assert!((4.5..5.5).contains(&spans), "{spans}");
    c.line.validate().unwrap();
}

/// The rows of a project file that matter, times left out.
fn rows(path: &std::path::Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut out = Vec::new();
    for (table, cols) in [
        ("model_object", "line, kind, id, ord, version, json, gone"),
        (
            "oplog",
            "seq, user, kind, command, inverse, objects, sections, undoes, undone_by, note, line",
        ),
        (
            "spatial_index",
            "rowid, line, kind, id, level, tx, ty, xmin, ymin, xmax, ymax",
        ),
    ] {
        let mut stmt = conn
            .prepare(&format!("SELECT {cols} FROM {table} ORDER BY rowid"))
            .unwrap();
        let n = stmt.column_count();
        let mut q = stmt.query([]).unwrap();
        while let Some(r) = q.next().unwrap() {
            let mut row = format!("{table}:");
            for k in 0..n {
                let v: rusqlite::types::Value = r.get(k).unwrap();
                row.push_str(&format!(" {v:?}"));
            }
            out.push(row);
        }
    }
    out
}

/// A small synthetic country (towers of two routes, jumpers at every
/// strain tower, alignment points tied to the angle towers): random
/// commands applied in place and the whole way (country server, phase
/// 3) give the same answers, lines and files; the line kept in place is
/// the line made again from the objects.
#[test]
fn in_place_equals_the_whole_way_on_a_small_country() {
    use ohl_session::{Command, Kind, Session};
    let dir = std::env::temp_dir().join(format!("ohl-synth-inplace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (k, spans) in [(3u64, 2.0), (4, 5.0)] {
        let c = country(&Settings {
            pylons: 400,
            seed: k,
            section_spans: spans,
            ..Settings::default()
        })
        .unwrap();
        let (pa, pb) = (dir.join(format!("a{k}.ohl")), dir.join(format!("b{k}.ohl")));
        project::write_project(&c, &pa, "synth").unwrap();
        project::write_project(&c, &pb, "synth").unwrap();
        let mut a = Session::open(&pa).unwrap();
        let mut b = Session::open(&pb).unwrap();
        b.set_whole_only(true);
        let mut rng = Rng(k.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 77);
        let answer = |r: Result<ohl_session::Applied, ohl_session::SessionError>| match r {
            Ok(v) => format!("ok {v:?}"),
            Err(e) => format!("err {e:?}"),
        };
        for step in 0..120 {
            let line = a.line();
            let t = &line.structures[rng.int(0, line.structures.len() - 1)];
            let sec = &line.sections[rng.int(0, line.sections.len() - 1)];
            let command = match rng.int(0, 9) {
                0..=3 => Command::MoveStructure {
                    id: t.id.clone(),
                    position: [
                        t.position[0] + rng.range(-10.0, 10.0),
                        t.position[1] + rng.range(-10.0, 10.0),
                        t.position[2],
                    ],
                },
                4 => Command::SetStringing {
                    section: sec.id.clone(),
                    stringing: ohl_line::Stringing::Tension {
                        horizontal_tension: rng.range(-1000.0, 30000.0),
                        temperature: 15.0,
                        condition: Default::default(),
                    },
                },
                5 => Command::Delete {
                    kind: Kind::Structure,
                    id: t.id.clone(),
                    confirm: rng.f64() < 0.7,
                },
                6 => Command::Delete {
                    kind: Kind::Section,
                    id: sec.id.clone(),
                    confirm: true,
                },
                7 => Command::Rename {
                    kind: Kind::Structure,
                    id: t.id.clone(),
                    new_id: format!("{}-r", t.id),
                },
                _ => Command::DeleteAttachment {
                    structure: sec.points[0].structure.clone(),
                    set: sec.points[0].set,
                    phase: None,
                    confirm: true,
                },
            };
            let at = format!("seed {k} step {step}: {command:?}");
            match rng.int(0, 5) {
                0 => assert_eq!(
                    answer(a.undo("synth")),
                    answer(b.undo("synth")),
                    "{at}: undo"
                ),
                1 => assert_eq!(
                    answer(a.redo("synth")),
                    answer(b.redo("synth")),
                    "{at}: redo"
                ),
                _ => assert_eq!(
                    answer(a.apply("synth", command.clone())),
                    answer(b.apply("synth", command)),
                    "{at}"
                ),
            }
            assert!(a.line() == b.line(), "{at}: line");
            a.check_kept().unwrap_or_else(|e| panic!("{at}: {e}"));
        }
        drop((a, b));
        assert_eq!(rows(&pa), rows(&pb), "seed {k}: the files");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Towers shared by several routes (phase 8) on a small synthetic
/// country: the user works on every other route. Random commands, most of
/// them at towers shared with a route outside the working set (moves,
/// strings turned to strain, sets and strings removed, renames,
/// deletions, stringing of the other routes' sections): a command
/// accepted is accepted the same without a working set; after it, the
/// sections of the routes outside the working set (all of whose towers
/// stand on such routes only) are as they were and the line is valid.
#[test]
fn shared_towers_keep_the_other_routes_connected() {
    use ohl_session::{Command, Kind, Session};
    use std::collections::{BTreeMap, BTreeSet};
    let dir = std::env::temp_dir().join(format!("ohl-synth-shared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let c = country(&Settings {
        pylons: 400,
        seed: 5,
        ..Settings::default()
    })
    .unwrap();
    let (pa, pb) = (dir.join("ws.ohl"), dir.join("plain.ohl"));
    project::write_project(&c, &pa, "synth").unwrap();
    project::write_project(&c, &pb, "synth").unwrap();
    let mut ws = Session::open(&pa).unwrap();
    let mut plain = Session::open(&pb).unwrap();
    let inside: BTreeSet<String> = ws
        .line()
        .routes
        .iter()
        .step_by(2)
        .map(|r| r.id.clone())
        .collect();
    ws.set_working_set("synth", Some(inside.clone())).unwrap();
    // The other routes' own sections: every tower of theirs on outside
    // routes only (by the line's own routes of its structures).
    let theirs = |s: &Session| -> BTreeMap<String, ohl_line::Section> {
        let routes = s.line().structure_routes();
        s.line()
            .sections
            .iter()
            .filter(|x| {
                x.points.iter().all(|p| {
                    routes
                        .get(&p.structure)
                        .is_some_and(|r| !r.is_empty() && r.iter().all(|r| !inside.contains(r)))
                })
            })
            .map(|x| (x.id.clone(), x.clone()))
            .collect()
    };
    let shared: Vec<String> = {
        let routes = ws.line().structure_routes();
        routes
            .iter()
            .filter(|(_, r)| {
                r.iter().any(|x| inside.contains(x)) && r.iter().any(|x| !inside.contains(x))
            })
            .map(|(id, _)| id.clone())
            .collect()
    };
    assert!(shared.len() > 5, "{} shared towers", shared.len());
    let mut rng = Rng(std::env::var("OHL_SHARED_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x5eed_0008));
    let (mut accepted, mut refused, mut notices) = (0, 0, 0);
    for step in 0..250 {
        let line = ws.line();
        let pick = shared[rng.int(0, shared.len() - 1)].clone();
        let Some(t) = line.structure(&pick).cloned() else {
            continue;
        };
        let sec = line.sections[rng.int(0, line.sections.len() - 1)].clone();
        let command = match rng.int(0, 9) {
            0..=2 => Command::MoveStructure {
                id: t.id.clone(),
                position: [
                    t.position[0] + rng.range(-3.0, 3.0),
                    t.position[1] + rng.range(-3.0, 3.0),
                    t.position[2],
                ],
            },
            3 if !t.strings.is_empty() => {
                let mut x = t.clone();
                let k = rng.int(0, x.strings.len() - 1);
                x.strings[k].strain = !x.strings[k].strain;
                Command::Put {
                    kind: Kind::Structure,
                    id: x.id.clone(),
                    value: serde_json::to_value(&x).unwrap(),
                    ord: None,
                }
            }
            4 if !t.wires.is_empty() => Command::DeleteAttachment {
                structure: t.id.clone(),
                set: t.wires[rng.int(0, t.wires.len() - 1)].set.unwrap_or(1),
                phase: None,
                confirm: true,
            },
            5 => Command::Rename {
                kind: Kind::Structure,
                id: t.id.clone(),
                new_id: format!("{}-r", t.id),
            },
            6 => Command::Delete {
                kind: Kind::Structure,
                id: t.id.clone(),
                confirm: true,
            },
            7 => Command::SetStringing {
                section: sec.id.clone(),
                stringing: ohl_line::Stringing::Tension {
                    horizontal_tension: rng.range(5000.0, 30000.0),
                    temperature: 15.0,
                    condition: Default::default(),
                },
            },
            8 if !t.points.is_empty() => {
                let mut x = t.clone();
                let k = rng.int(0, x.points.len() - 1);
                x.points[k].position[2] += rng.range(-0.5, 0.5);
                Command::Put {
                    kind: Kind::Structure,
                    id: x.id.clone(),
                    value: serde_json::to_value(&x).unwrap(),
                    ord: None,
                }
            }
            _ => Command::MoveStructure {
                id: t.id.clone(),
                position: t.position,
            },
        };
        let at = format!("step {step}: {command:?}");
        let before = theirs(&ws);
        // Undo and redo too (a move taken back moves the other route
        // again: allowed, stale again).
        let undo = rng.int(0, 5);
        let (a, at) = match undo {
            0 => (ws.undo("synth"), format!("{at} (undo)")),
            1 => (ws.redo("synth"), format!("{at} (redo)")),
            _ => (ws.apply("synth", command.clone()), at),
        };
        match a {
            Ok(a) => {
                accepted += 1;
                notices += usize::from(a.notice.is_some());
                let p = match undo {
                    0 => plain.undo("synth"),
                    1 => plain.redo("synth"),
                    _ => plain.apply("synth", command),
                };
                let p =
                    p.unwrap_or_else(|e| panic!("{at}: not accepted without a working set: {e}"));
                assert_eq!((&a.objects, &a.sections), (&p.objects, &p.sections), "{at}");
                let line = ws.line();
                for (id, s) in &before {
                    assert_eq!(
                        line.sections.iter().find(|x| x.id == *id),
                        Some(s),
                        "{at}: section {id} of an outside route"
                    );
                }
                line.validate().unwrap_or_else(|e| panic!("{at}: {e}"));
                assert!(ws.line() == plain.line(), "{at}: line");
            }
            Err(e) => {
                // An undo or redo refused by the rules only where it
                // would break the other route (never a mere move back).
                if undo <= 1 {
                    let t = e.to_string();
                    assert!(!t.contains("§7.4"), "{at}: {t}");
                }
                refused += 1
            }
        }
    }
    assert!(
        accepted > 20 && refused > 20 && notices > 10,
        "{accepted} accepted, {refused} refused, {notices} with a notice"
    );
    assert!(!ws.stale_routes().unwrap().is_empty());
    drop((ws, plain));
    // Measured with a working set: moves of shared towers, each with a
    // notice; the working set and the marks taken off again.
    let b = project::measure_with(
        &pb,
        "synth",
        3,
        std::time::Duration::from_secs(60),
        false,
        project::Shared::WorkingSet,
    )
    .unwrap();
    let run = b.working_set.unwrap();
    assert_eq!(run.moves_with_notice, 3);
    assert!(run.shared_towers > 5);
    let s = Session::open(&pb).unwrap();
    assert!(s.working_sets().is_empty() && s.stale_routes().unwrap().is_empty());
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The owner's size; about 20 s (most of it the line's validation), run
/// by hand: `cargo test -p ohl-synth --release -- --ignored`.
#[test]
#[ignore]
fn owner_size() {
    let t = std::time::Instant::now();
    let c = country(&Settings::default()).unwrap();
    println!("generated in {:?}: {:#?}", t.elapsed(), c.summary);
    assert_eq!(
        c.summary.hash,
        "1c14a252dda1b8d1e397fb5675a3600ff7a792d7d4af4a128b1de3a02b053d7a"
    );
}
