// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: the desktop's line kept in place equals the line made again
//! whole (O42) — two copies of a project, the same random commands on
//! both, one taking them in place, the other making its line again whole
//! every time: the same answers, the same line, the same solved cases.

use super::*;
use ohl_session::{Command, Kind};

/// A xorshift generator (deterministic).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The project of `file` (an example) copied into a folder of its own,
/// edited by "tester"; `typed`: with the example's structure types.
fn project(file: &str, tag: &str, typed: bool) -> model::Model {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-in-place-{}-{tag}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    let line = ohl_line::Line::load(&ex.join(file)).unwrap();
    ohl_session::Session::open(&path)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let mut m = model::load(
        Sources {
            file: path,
            library: Some(root().join("libraries/cables")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            structures: typed.then(|| ex.join("Structures")),
            insulators: typed.then(|| ex.join("Insulators")),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    // Two cases shown (solved at once after each edit), the others
    // pending until wanted.
    m.shown_cases = vec![0, 1];
    m.ensure_solved(0);
    m.ensure_solved(1);
    m
}

/// A random command on the line `m` holds as stored.
fn command(m: &model::Model, rng: &mut Rng, types: &[String]) -> Command {
    let line = m.stored_line().unwrap();
    let nothing = || Command::Batch { commands: vec![] };
    let structure = |rng: &mut Rng| {
        (!line.structures.is_empty())
            .then(|| line.structures[rng.below(line.structures.len())].clone())
    };
    let section = |rng: &mut Rng| {
        (!line.sections.is_empty()).then(|| line.sections[rng.below(line.sections.len())].clone())
    };
    let put = |kind: Kind, id: &str, v: serde_json::Value| Command::Put {
        kind,
        id: id.to_owned(),
        value: v,
        ord: None,
    };
    match rng.below(16) {
        0..=3 => match structure(rng) {
            Some(t) => Command::MoveStructure {
                id: t.id,
                position: [
                    t.position[0] + rng.f() * 20.0 - 10.0,
                    t.position[1] + rng.f() * 20.0 - 10.0,
                    t.position[2] + rng.f() * 2.0 - 1.0,
                ],
            },
            None => nothing(),
        },
        // A structure turned, given another type (or one not in the
        // library: refused by the desktop, as the whole way does), or its
        // own wire point taken away.
        4 | 5 => match structure(rng) {
            Some(mut t) => {
                match rng.below(4) {
                    0 => t.orientation_deg += rng.f() * 40.0 - 20.0,
                    1 if !types.is_empty() && t.structure_type.is_some() => {
                        t.structure_type = Some(types[rng.below(types.len())].clone());
                    }
                    2 if t.structure_type.is_some() && rng.below(4) == 0 => {
                        t.structure_type = Some("no such type".into());
                    }
                    _ => {
                        if !t.wires.is_empty() {
                            let k = rng.below(t.wires.len());
                            t.wires.remove(k);
                        }
                    }
                }
                put(Kind::Structure, &t.id, serde_json::to_value(&t).unwrap())
            }
            None => nothing(),
        },
        6 => match section(rng) {
            Some(sec) => Command::SetStringing {
                section: sec.id,
                stringing: ohl_line::Stringing::Tension {
                    horizontal_tension: 8000.0 + rng.f() * 20000.0,
                    temperature: 15.0,
                    condition: Default::default(),
                },
            },
            None => nothing(),
        },
        7 => match section(rng) {
            Some(sec) => Command::SetWindFactor {
                section: sec.id,
                spans: [0, rng.below(2)],
                factor: 0.5 + rng.f(),
            },
            None => nothing(),
        },
        8 => {
            let kinds = [Kind::Structure, Kind::Section, Kind::Jumper];
            let kind = kinds[rng.below(3)];
            let ids: Vec<String> = match kind {
                Kind::Structure => line.structures.iter().map(|x| x.id.clone()).collect(),
                Kind::Section => line.sections.iter().map(|x| x.id.clone()).collect(),
                _ => line.jumpers.iter().map(|x| x.id.clone()).collect(),
            };
            if ids.is_empty() {
                return nothing();
            }
            Command::Delete {
                kind,
                id: ids[rng.below(ids.len())].clone(),
                confirm: true,
            }
        }
        9 => {
            let kinds = [Kind::Structure, Kind::Section, Kind::Jumper];
            let kind = kinds[rng.below(3)];
            let ids: Vec<String> = match kind {
                Kind::Structure => line.structures.iter().map(|x| x.id.clone()).collect(),
                Kind::Section => line.sections.iter().map(|x| x.id.clone()).collect(),
                _ => line.jumpers.iter().map(|x| x.id.clone()).collect(),
            };
            if ids.is_empty() {
                return nothing();
            }
            let id = ids[rng.below(ids.len())].clone();
            Command::Rename {
                kind,
                new_id: format!("{id}r{}", rng.below(9)),
                id,
            }
        }
        // A section copied (a new object, placed last by the session).
        10 => match section(rng) {
            Some(mut sec) => {
                sec.id = format!("{}c{}", sec.id, rng.below(50));
                put(Kind::Section, &sec.id, serde_json::to_value(&sec).unwrap())
            }
            None => nothing(),
        },
        11 => {
            if line.alignments.is_empty() {
                return nothing();
            }
            let a = &line.alignments[rng.below(line.alignments.len())];
            let n = a.points.len();
            match rng.below(3) {
                0 => Command::MoveAlignmentPoint {
                    route: a.id.clone(),
                    index: rng.below(n),
                    position: [rng.f() * 1000.0, rng.f() * 500.0, 500.0],
                },
                1 => match structure(rng) {
                    Some(t) => Command::TieAlignmentPoint {
                        route: a.id.clone(),
                        index: rng.below(n),
                        structure: t.id,
                    },
                    None => nothing(),
                },
                _ => Command::DetachAlignmentPoint {
                    route: a.id.clone(),
                    index: rng.below(n),
                },
            }
        }
        // Settings: the whole way in both.
        12 => {
            let mut solver = line.solver;
            solver.jumper_loads = if rng.below(2) == 0 {
                ohl_line::JumperLoads::Estimate
            } else {
                ohl_line::JumperLoads::Pull
            };
            m.session().unwrap().solver_command(solver)
        }
        13 => {
            let a = command(m, rng, types);
            let b = command(m, rng, types);
            Command::Batch {
                commands: vec![a, b],
            }
        }
        _ => match structure(rng) {
            Some(t) => Command::MoveStructure {
                id: t.id,
                position: t.position,
            },
            None => nothing(),
        },
    }
}

/// An edit's answer without its time.
fn answer(r: Result<String, crate::edit::EditError>) -> String {
    match r {
        Ok(s) => {
            let s = s.split(" in ").next().unwrap_or("").to_owned();
            format!("ok {s}")
        }
        Err(e) => format!("err {e}"),
    }
}

/// The two models hold the same line and the same solved cases.
fn same(a: &model::Model, b: &model::Model, at: &str) {
    assert!(a.line == b.line, "{at}: the lines differ");
    assert_eq!(a.cases.len(), b.cases.len());
    for (x, y) in a.cases.iter().zip(&b.cases) {
        assert_eq!(x.ready, y.ready, "{at}: {}", x.case.name);
        if !x.ready {
            continue;
        }
        assert_eq!(x.shapes, y.shapes, "{at}: {} shapes", x.case.name);
        assert_eq!(x.jumpers, y.jumpers, "{at}: {} jumpers", x.case.name);
        assert_eq!(x.failed, y.failed, "{at}: {}", x.case.name);
        assert_eq!(x.unsolved, y.unsolved, "{at}: {}", x.case.name);
    }
}

/// Random commands, undos, redos and drags on both copies of `file`.
fn run(file: &str, typed: bool, seed: u64, steps: usize) -> [usize; 3] {
    let tag = format!("{}-{seed}", if typed { "typed" } else { "plain" });
    let mut a = project(file, &format!("{tag}-a"), typed);
    let mut b = project(file, &format!("{tag}-b"), typed);
    b.set_whole_line_edits(true);
    same(&a, &b, "opened");
    let types: Vec<String> = a
        .types
        .as_ref()
        .map(|(t, _)| t.iter().map(|x| x.id.clone()).collect())
        .unwrap_or_default();
    let mut rng = Rng(seed);
    // In place, whole, refused.
    let mut counts = [0; 3];
    for step in 0..steps {
        let (ra, rb, what) = match rng.below(10) {
            0 => (answer(a.undo()), answer(b.undo()), "undo".to_owned()),
            1 => (answer(a.redo()), answer(b.redo()), "redo".to_owned()),
            // A drag previewed, then let go (one command) or cancelled.
            2 => {
                let line = a.line.clone();
                if line.structures.is_empty() {
                    continue;
                }
                let s = &line.structures[rng.below(line.structures.len())];
                let to = [s.position[0] + 3.0, s.position[1] - 2.0, s.position[2]];
                a.preview_move(0, &s.id, to);
                b.preview_move(0, &s.id, to);
                if rng.below(2) == 0 {
                    a.cancel_preview(0, &s.id);
                    b.cancel_preview(0, &s.id);
                    same(&a, &b, &format!("step {step}: cancelled drag of {}", s.id));
                    continue;
                }
                let c = Command::MoveStructure {
                    id: s.id.clone(),
                    position: to,
                };
                (
                    answer(a.apply("drag", c.clone())),
                    answer(b.apply("drag", c)),
                    format!("drag of {}", s.id),
                )
            }
            _ => {
                let c = command(&a, &mut rng, &types);
                let what = format!("{c:?}");
                (
                    answer(a.apply("edit", c.clone())),
                    answer(b.apply("edit", c)),
                    what,
                )
            }
        };
        let at = format!("step {step} ({what})");
        assert_eq!(ra, rb, "{at}");
        if ra.starts_with("err") {
            counts[2] += 1;
        } else if a.last_edit.whole.is_some() {
            counts[1] += 1;
        } else {
            counts[0] += 1;
        }
        same(&a, &b, &at);
    }
    counts
}

/// The plain example (structures without types) and the typed one (each
/// structure merged with its type), three seeds each: in place equals the
/// whole way, and most commands are taken in place.
#[test]
fn the_line_kept_in_place_equals_the_line_made_again() {
    for (file, typed) in [
        ("synthetic-line.ohl.json", false),
        ("synthetic-line-typed.ohl.json", true),
    ] {
        for seed in [7, 19, 2026] {
            let [in_place, whole, refused] = run(file, typed, seed, 60);
            eprintln!("{file} seed {seed}: {in_place} in place, {whole} whole, {refused} refused");
            assert!(
                in_place > whole && in_place > 20,
                "{file} seed {seed}: {in_place} in place, {whole} whole, {refused} refused"
            );
        }
    }
}

/// A move is taken in place (the structure and what refers to it); a
/// change of the line's settings makes the line again whole and says so
/// in the edit's report; so does every edit with the switch set.
#[test]
fn an_edit_says_when_the_line_is_made_again_whole() {
    let mut m = project("synthetic-line-typed.ohl.json", "report", true);
    let p = m.line.structure("T3").unwrap().position;
    let report = m
        .apply(
            "moved T3",
            Command::MoveStructure {
                id: "T3".into(),
                position: [p[0] + 1.0, p[1], p[2]],
            },
        )
        .unwrap();
    assert!(!report.contains("made again whole"), "{report}");
    assert_eq!(m.last_edit.whole, None);
    assert!(m.last_edit.objects >= 1, "{:?}", m.last_edit);
    let mut solver = m.line.solver;
    solver.jumper_loads = match solver.jumper_loads {
        ohl_line::JumperLoads::Pull => ohl_line::JumperLoads::Estimate,
        _ => ohl_line::JumperLoads::Pull,
    };
    let c = m.session().unwrap().solver_command(solver);
    let report = m.apply("options", c).unwrap();
    assert!(
        report.ends_with("the line made again whole: the line's settings changed"),
        "{report}"
    );
    // Its undo too; every edit whole with the switch set.
    let report = m.undo().unwrap();
    assert!(report.contains("settings changed"), "{report}");
    m.set_whole_line_edits(true);
    let report = m.redo().unwrap();
    assert!(report.contains("every edit made whole"), "{report}");
    m.set_whole_line_edits(false);
    let report = m.undo().unwrap();
    assert!(report.contains("settings changed"), "{report}");
    // Then a move in place again.
    let p = m.line.structure("T3").unwrap().position;
    m.apply(
        "moved T3",
        Command::MoveStructure {
            id: "T3".into(),
            position: [p[0] - 1.0, p[1], p[2]],
        },
    )
    .unwrap();
    assert_eq!(m.last_edit.whole, None);
}
