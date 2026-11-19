// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Commands applied in place against the same commands the whole way
//! ([`Session::set_whole_only`]): two project files fed the same random
//! command sequences (moves, puts valid and not, deletions with what they
//! take, renames, alignment points, spacers, taps, undo, redo, rollbacks,
//! project lines, units of work failing half-way). After every step: the
//! same answer, the same objects, the same line — which is also the line
//! made again from the objects — the same spatial index; at the end the
//! same rows in both files.

use super::*;
use std::path::PathBuf;

/// A small deterministic random source (xorshift).
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    pub(crate) fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The example line with a spacer, a jumper on a tap of a span, a jumper
/// on that jumper's point and a circuit start.
pub(crate) fn rich() -> Line {
    let mut line = Line::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/synthetic-line/synthetic-line.ohl.json"),
    )
    .unwrap();
    let clamp = |label: &str, span: usize, f: f64| ohl_line::SpanPoint {
        label: label.into(),
        span,
        at: ohl_line::SpanPlace::at_fraction(f),
        description: String::new(),
        object: Some(ohl_line::LineObject::clamp(30.0)),
    };
    line.sections[0].span_points.push(clamp("C1", 0, 0.4));
    // A bare end: the spacer gives it a clamp when the line is read.
    line.sections[1].span_points.push(ohl_line::SpanPoint {
        object: None,
        ..clamp("C1", 0, 0.4)
    });
    line.sections[0].span_points.push(clamp("TAP", 1, 0.5));
    line.spacers.push(ohl_line::Spacer {
        id: "SP1".into(),
        a: ohl_line::SpanPointRef {
            section: line.sections[0].id.clone(),
            point: "C1".into(),
        },
        b: ohl_line::SpanPointRef {
            section: line.sections[1].id.clone(),
            point: "C1".into(),
        },
        compression: Some(10_000.0),
        ..Default::default()
    });
    let mut j = line.jumpers[0].clone();
    j.id = "JT".into();
    j.from = ohl_line::JumperEnd::tap(ohl_line::Tap::Span(ohl_line::SpanPointRef {
        section: line.sections[0].id.clone(),
        point: "TAP".into(),
    }));
    j.set_supports(Vec::new());
    j.set_support(None);
    j.sag = None;
    j.length = Some(400.0);
    j.points.push(ohl_line::JumperPoint {
        label: "K".into(),
        at: ohl_line::JumperPlace::Fraction(0.5),
        description: String::new(),
        object: Some(ohl_line::LineObject::clamp(10.0)),
    });
    line.jumpers.push(j);
    let mut jj = line.jumpers[0].clone();
    jj.id = "JJ".into();
    jj.from = ohl_line::JumperEnd::tap(ohl_line::Tap::Jumper {
        jumper: "JT".into(),
        point: "K".into(),
    });
    jj.to = ohl_line::JumperEnd::Wire(ohl_line::SectionPoint::new("T4", 1));
    jj.set_supports(Vec::new());
    jj.set_support(None);
    jj.sag = None;
    jj.length = Some(400.0);
    line.jumpers.push(jj);
    line.circuit_starts
        .push(ohl_line::SectionPoint::new("T1", 1));
    line.read_older();
    line.validate().expect("the rich line is valid");
    line
}

fn open(name: &str, line: &Line) -> (PathBuf, Session) {
    let dir = std::env::temp_dir().join(format!("ohl-in-place-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    for ext in ["", "-wal", "-shm"] {
        std::fs::remove_file(format!("{}{ext}", path.display())).ok();
    }
    let mut s = Session::open(&path).unwrap();
    s.import(line, "ana").unwrap();
    s.set_user("ana", "ben", Role::Designer).unwrap();
    (path, s)
}

/// A random command on `s`'s line (valid or not).
pub(crate) fn command(s: &Session, rng: &mut Rng) -> Command {
    let line = s.line();
    let pick = |rng: &mut Rng, n: usize| rng.below(n);
    let structure = |rng: &mut Rng| {
        (!line.structures.is_empty())
            .then(|| line.structures[pick(rng, line.structures.len())].clone())
    };
    let section = |rng: &mut Rng| {
        (!line.sections.is_empty()).then(|| line.sections[pick(rng, line.sections.len())].clone())
    };
    let jumper = |rng: &mut Rng| {
        (!line.jumpers.is_empty()).then(|| line.jumpers[pick(rng, line.jumpers.len())].clone())
    };
    let nothing = || Command::Batch { commands: vec![] };
    match rng.below(20) {
        0..=3 => match structure(rng) {
            Some(t) => Command::MoveStructure {
                id: t.id,
                position: [
                    t.position[0] + rng.f() * 30.0 - 15.0,
                    t.position[1] + rng.f() * 30.0 - 15.0,
                    t.position[2],
                ],
            },
            None => nothing(),
        },
        4 => match structure(rng) {
            Some(mut t) => {
                let mut v = json(&t);
                match rng.below(5) {
                    0 => t.soil_resistivity_ohm_m = Some(-1.0),
                    1 => t.soil_resistivity_ohm_m = Some(100.0),
                    2 => {
                        if !t.wires.is_empty() {
                            let k = rng.below(t.wires.len());
                            t.wires.remove(k);
                        }
                    }
                    3 => {
                        v["position"] = Value::Null;
                        return put_value(Kind::Structure, &t.id, v);
                    }
                    _ => {
                        v["id"] = Value::String("other".into());
                        return put_value(Kind::Structure, &t.id, v);
                    }
                }
                put(Kind::Structure, &t.id, &t)
            }
            None => nothing(),
        },
        5 => match section(rng) {
            Some(sec) => Command::SetStringing {
                section: sec.id,
                stringing: Stringing::Tension {
                    horizontal_tension: if rng.below(4) == 0 {
                        -5.0
                    } else {
                        5000.0 + rng.f() * 20000.0
                    },
                    temperature: 15.0,
                    condition: Default::default(),
                },
            },
            None => nothing(),
        },
        6 => match section(rng) {
            Some(mut sec) => {
                match rng.below(6) {
                    0 => sec.points.truncate(1),
                    1 => sec.system = Some("nope".into()),
                    2 => sec.phase_label = "L3".into(),
                    3 => {
                        // A copy: joins its conductor path (perhaps
                        // with another phase).
                        sec.id = format!("{}c{}", sec.id, rng.below(50));
                        sec.span_points.clear();
                        sec.spreads.clear();
                        if rng.below(2) == 0 {
                            sec.phase_label = "L2".into();
                        }
                    }
                    4 => sec.span_points.retain(|p| p.label != "C1"),
                    _ => sec.circuit_name = "a circuit nobody has".into(),
                }
                put(Kind::Section, &sec.id, &sec)
            }
            None => nothing(),
        },
        7 => match jumper(rng) {
            Some(mut j) => {
                match rng.below(5) {
                    0 => j.to = j.from.clone(),
                    1 => {
                        j.from = ohl_line::JumperEnd::tap(ohl_line::Tap::Jumper {
                            jumper: "JJ".into(),
                            point: "K".into(),
                        })
                    }
                    2 => {
                        j.id = format!("{}c{}", j.id, rng.below(50));
                        j.from = ohl_line::JumperEnd::Wire(ohl_line::SectionPoint::new("T2", 1));
                        j.to = ohl_line::JumperEnd::Wire(ohl_line::SectionPoint::new("T2", 11));
                        j.points.clear();
                        j.length = None;
                        j.sag = Some(1.0);
                    }
                    3 => j.sag = Some(-1.0),
                    _ => j.bundle_spacing = Some(0.4),
                }
                put(Kind::Jumper, &j.id, &j)
            }
            None => nothing(),
        },
        8 => {
            let kinds = [
                Kind::Structure,
                Kind::Section,
                Kind::Jumper,
                Kind::Spacer,
                Kind::AlignmentPoint,
                Kind::Alignment,
                Kind::Route,
            ];
            let kind = kinds[rng.below(kinds.len())];
            let ids: Vec<&String> = s
                .objects
                .keys()
                .filter(|(k, _)| *k == kind)
                .map(|(_, id)| id)
                .collect();
            if ids.is_empty() {
                return nothing();
            }
            Command::Delete {
                kind,
                id: ids[rng.below(ids.len())].clone(),
                confirm: rng.below(3) > 0,
            }
        }
        9 => match structure(rng) {
            Some(t) if !t.wires.is_empty() => Command::DeleteAttachment {
                structure: t.id.clone(),
                set: t.wires[rng.below(t.wires.len())].set.unwrap_or(1),
                phase: None,
                confirm: true,
            },
            _ => nothing(),
        },
        10 => {
            let kinds = [Kind::Structure, Kind::Section, Kind::Jumper];
            let kind = kinds[rng.below(3)];
            let ids: Vec<&String> = s
                .objects
                .keys()
                .filter(|(k, _)| *k == kind)
                .map(|(_, id)| id)
                .collect();
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
        11 => {
            if line.alignments.is_empty() {
                return nothing();
            }
            let a = &line.alignments[rng.below(line.alignments.len())];
            let n = a.points.len();
            match rng.below(5) {
                0 => Command::MoveAlignmentPoint {
                    route: a.id.clone(),
                    index: rng.below(n + 1),
                    position: [rng.f() * 1000.0, rng.f() * 500.0, 500.0],
                },
                1 => Command::InsertAlignmentPoint {
                    route: a.id.clone(),
                    index: rng.below(n + 1),
                    position: [rng.f() * 1000.0, rng.f() * 500.0, 500.0],
                    point: None,
                },
                2 => Command::RemoveAlignmentPoint {
                    route: a.id.clone(),
                    index: rng.below(n + 1),
                },
                3 => match structure(rng) {
                    Some(t) => Command::TieAlignmentPoint {
                        route: a.id.clone(),
                        index: rng.below(n + 1),
                        structure: t.id,
                    },
                    None => nothing(),
                },
                _ => Command::DetachAlignmentPoint {
                    route: a.id.clone(),
                    index: rng.below(n + 1),
                },
            }
        }
        12 => match s.objects.keys().find(|(k, _)| *k == Kind::Spacer) {
            Some((_, id)) => {
                let mut sp: ohl_line::Spacer =
                    serde_json::from_value((*s.objects[&(Kind::Spacer, id.clone())].value).clone())
                        .unwrap();
                match rng.below(3) {
                    0 => sp.weight = -1.0,
                    1 => sp.weight = 5.0,
                    _ => sp.a.point = "nope".into(),
                }
                put(Kind::Spacer, id, &sp)
            }
            None => nothing(),
        },
        13 => match section(rng) {
            Some(sec) => Command::SetWindFactor {
                section: sec.id,
                spans: [0, rng.below(3)],
                factor: rng.f() * 2.0,
            },
            None => nothing(),
        },
        14 => match section(rng) {
            Some(sec) if !sec.span_points.is_empty() => Command::RenamePoint {
                kind: Kind::Section,
                id: sec.id,
                label: sec.span_points[0].label.clone(),
                new_label: format!("N{}", rng.below(9)),
            },
            _ => nothing(),
        },
        15 => {
            let a = command(s, rng);
            let b = command(s, rng);
            Command::Batch {
                commands: vec![a, b],
            }
        }
        // A setting (the whole way in both).
        16 => {
            let mut d = line.defaults;
            d.spacer_compression = 1000.0 + rng.below(10) as f64;
            s.defaults_command(d)
        }
        // A library file and a report (not part of the line).
        17 => put_value(
            Kind::Library,
            "cables/x.ohlc",
            library_value("cables", "x.ohlc", &format!("# {}", rng.below(99))),
        ),
        _ => match structure(rng) {
            Some(t) => Command::MoveStructure {
                id: t.id,
                position: t.position,
            },
            None => nothing(),
        },
    }
}

fn put_value(kind: Kind, id: &str, value: Value) -> Command {
    Command::Put {
        kind,
        id: id.to_owned(),
        value,
        ord: None,
    }
}

/// What the two sessions give, to compare.
fn answer<T: fmt::Debug>(r: &Result<T, SessionError>) -> String {
    match r {
        // (Which way a command went, `Applied::whole`, is what differs.)
        Ok(v) => format!("ok {v:?}")
            .replace(", whole: true", "")
            .replace(", whole: false", ""),
        Err(e) => format!("err {e:?}"),
    }
}

/// The two sessions hold the same, and the in-place one what the whole
/// path would make of its objects.
fn same(a: &Session, b: &Session, at: &str) {
    assert_eq!(a.current, b.current, "{at}: line worked on");
    assert!(a.objects == b.objects, "{at}: objects");
    assert!(a.line == b.line, "{at}: line");
    if !a.objects.is_empty() {
        let again = line_of(&a.objects).expect("valid");
        assert!(a.line == again, "{at}: line made again");
    }
    assert!(a.spatial == b.spatial, "{at}: spatial index");
    assert!(
        a.spatial == spatial::entries(&a.line, &a.grid),
        "{at}: spatial index made again"
    );
    a.check_kept().unwrap_or_else(|e| panic!("{at}: {e}"));
    if let Some(index) = &a.index {
        assert!(
            *index == ohl_line::index::LineIndex::of(&a.line),
            "{at}: reference index"
        );
    }
    for (name, st) in &a.others {
        let other = &b.others[name];
        assert!(
            st.objects == other.objects && st.line == other.line,
            "{at}: line {name}"
        );
    }
}

/// The rows of a project file that matter, times left out.
fn rows(path: &Path) -> Vec<String> {
    let conn = Connection::open(path).unwrap();
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

/// One random step on both sessions.
fn step(a: &mut Session, b: &mut Session, rng: &mut Rng, at: &str) {
    let user = if rng.below(4) == 0 { "ben" } else { "ana" };
    match rng.below(14) {
        0 => {
            let (x, y) = (a.undo(user), b.undo(user));
            assert_eq!(answer(&x), answer(&y), "{at}: undo");
        }
        1 => {
            let (x, y) = (a.redo(user), b.redo(user));
            assert_eq!(answer(&x), answer(&y), "{at}: redo");
        }
        2 => {
            // Another project line: made, worked on, back.
            let name = format!("p{}", rng.below(3));
            if !a.is_line(&name) {
                let (x, y) = (
                    a.create_line("ana", &name, &a.current.clone()),
                    b.create_line("ana", &name, &b.current.clone()),
                );
                assert_eq!(answer(&x), answer(&y), "{at}: line made");
            }
            let c = command(a, rng);
            let (x, y) = (
                a.with_line(&name, |s| s.apply("ana", c.clone())),
                b.with_line(&name, |s| s.apply("ana", c.clone())),
            );
            assert_eq!(answer(&x), answer(&y), "{at}: on {name}: {c:?}");
        }
        3 => {
            // A unit of work refused after two commands: nothing stays.
            let (c1, c2) = (command(a, rng), command(a, rng));
            let run = |s: &mut Session| {
                s.unit(|s| {
                    s.apply("ana", c1.clone())?;
                    s.apply("ana", c2.clone())?;
                    Err::<(), _>(refused("refused at the end"))
                })
            };
            let (x, y) = (run(a), run(b));
            assert_eq!(answer(&x), answer(&y), "{at}: unit");
        }
        4 if rng.below(4) == 0 => {
            let last = a.last_seq().unwrap();
            let seq = last - 1 - rng.below(3) as i64;
            let (x, y) = (a.rollback_after("ana", seq), b.rollback_after("ana", seq));
            assert_eq!(answer(&x), answer(&y), "{at}: rollback after {seq}");
        }
        _ => {
            let c = command(a, rng);
            let (x, y) = (a.apply(user, c.clone()), b.apply(user, c.clone()));
            assert_eq!(answer(&x), answer(&y), "{at}: {c:?}");
        }
    }
}

fn run(line: &Line, name: &str, seeds: std::ops::RangeInclusive<u64>, steps: usize) {
    for seed in seeds {
        let (pa, mut a) = open(&format!("{name}-a{seed}"), line);
        let (pb, mut b) = open(&format!("{name}-b{seed}"), line);
        b.set_whole_only(true);
        let mut rng = Rng(0x5DEE_CE66_D1CE_4E5B ^ seed.wrapping_mul(0x9E37_79B9));
        for k in 0..steps {
            let at = format!("{name} seed {seed} step {k}");
            step(&mut a, &mut b, &mut rng, &at);
            same(&a, &b, &at);
        }
        drop((a, b));
        assert_eq!(rows(&pa), rows(&pb), "{name} seed {seed}: the files");
    }
    let [done, whole, refused] = crate::in_place::COUNTS.with(|c| *c.borrow());
    eprintln!("{name}: {done} in place, {whole} the whole way, {refused} refused");
    assert!(
        done > 100 && refused > 20,
        "{name}: {done} in place, {refused} refused"
    );
}

#[test]
fn in_place_equals_the_whole_way_on_random_commands() {
    run(&rich(), "rich", 1..=8, 150);
}
