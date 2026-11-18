// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The validation by the index, whole and local, against the validation
//! as it was (`validate_reference`): on random edits of a line with every
//! kind of reference (two routes sharing a point, a tower of both, a
//! spacer, a jumper on a tap of a span, a jumper on a jumper's point, a
//! span point by station, a circuit start), valid and not.

use crate::index::{Item, LineIndex, Of};
use crate::validate::Scope;
use crate::*;

/// A small deterministic random source (xorshift).
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

/// The example line with every kind of reference.
pub(crate) fn rich() -> Line {
    let mut line = crate::tests::example();
    let clamp = |label: &str, span: usize, f: f64| SpanPoint {
        label: label.into(),
        span,
        at: SpanPlace::at_fraction(f),
        description: String::new(),
        object: Some(crate::LineObject::clamp(30.0)),
    };
    line.sections[0].span_points.push(clamp("C1", 0, 0.4));
    line.sections[1].span_points.push(clamp("C1", 0, 0.4));
    line.sections[0].span_points.push(clamp("TAP", 1, 0.5));
    line.spacers.push(Spacer {
        id: "SP1".into(),
        a: SpanPointRef {
            section: line.sections[0].id.clone(),
            point: "C1".into(),
        },
        b: SpanPointRef {
            section: line.sections[1].id.clone(),
            point: "C1".into(),
        },
        compression: Some(10_000.0),
        ..Default::default()
    });
    // A jumper from the tap to T3 set 1, with a point a second one taps.
    let mut j = line.jumpers[0].clone();
    j.id = "JT".into();
    j.from = JumperEnd::tap(Tap::Span(SpanPointRef {
        section: line.sections[0].id.clone(),
        point: "TAP".into(),
    }));
    j.set_supports(Vec::new());
    j.set_support(None);
    j.sag = None;
    let chord = |line: &Line, j: &Jumper| {
        let a = crate::jumper::approximate_end(line, &j.from, 0).unwrap();
        let b = crate::jumper::approximate_end(line, &j.to, 0).unwrap();
        (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
    };
    j.length = Some(1.05 * chord(&line, &j));
    j.points.push(JumperPoint {
        label: "K".into(),
        at: JumperPlace::Fraction(0.5),
        description: String::new(),
        object: Some(crate::LineObject::clamp(10.0)),
    });
    line.jumpers.push(j);
    let mut jj = line.jumpers[0].clone();
    jj.id = "JJ".into();
    jj.from = JumperEnd::tap(Tap::Jumper {
        jumper: "JT".into(),
        point: "K".into(),
    });
    jj.to = JumperEnd::Wire(SectionPoint::new("T4", 1));
    jj.set_supports(Vec::new());
    jj.set_support(None);
    jj.sag = None;
    jj.length = Some(1.05 * chord(&line, &jj).max(1.0));
    jj.points = vec![JumperPoint {
        label: "K".into(),
        at: JumperPlace::Fraction(0.3),
        description: String::new(),
        object: Some(crate::LineObject::clamp(10.0)),
    }];
    line.jumpers.push(jj);
    // A span point by station on the branch.
    let s3 = line.sections.len() - 1;
    let (a, b) = line
        .span_stations(
            &line.sections[s3].points[0].structure,
            &line.sections[s3].points[1].structure,
        )
        .expect("the branch's stations");
    line.sections[s3].span_points.push(SpanPoint {
        label: "ST".into(),
        span: 0,
        at: SpanPlace::Station(0.5 * (a + b)),
        description: String::new(),
        object: None,
    });
    line.circuit_starts.push(SectionPoint::new("T1", 1));
    line.validate_reference().expect("the rich line is valid");
    line
}

/// The objects whose value differs between `a` and `b` (moved ones are
/// said by the edit).
fn changed(a: &Line, b: &Line) -> Vec<(Of, String)> {
    let (ia, ib) = (LineIndex::of(a), LineIndex::of(b));
    let mut out = Vec::new();
    for of in Of::ALL {
        let mut ids: Vec<String> = Vec::new();
        for (line, index) in [(a, &ia), (b, &ib)] {
            for k in 0..crate::index::count(line, of) {
                ids.push(index_id(line, of, k));
            }
            let _ = index;
        }
        ids.sort();
        ids.dedup();
        for id in ids {
            let x = ia.get(a, of, &id);
            let y = ib.get(b, of, &id);
            if x != y {
                out.push((of, id));
            }
        }
    }
    out
}

fn index_id(line: &Line, of: Of, k: usize) -> String {
    match of {
        Of::Route => line.routes[k].id.clone(),
        Of::Alignment => line.alignments[k].id.clone(),
        Of::AlignmentPoint => line.alignment_points[k].id.clone(),
        Of::Structure => line.structures[k].id.clone(),
        Of::Section => line.sections[k].id.clone(),
        Of::Jumper => line.jumpers[k].id.clone(),
        Of::Spacer => line.spacers[k].id.clone(),
    }
}

/// One random edit of `line` (often making it invalid).
fn edit(line: &mut Line, rng: &mut Rng, moved: &mut Vec<(Of, String)>) {
    let pick = |rng: &mut Rng, n: usize| rng.below(n);
    match rng.below(8) {
        0 if !line.structures.is_empty() => {
            let k = pick(rng, line.structures.len());
            let s = &mut line.structures[k];
            match rng.below(9) {
                0..=2 => {
                    let d = [rng.f() * 40.0 - 20.0, rng.f() * 40.0 - 20.0, 0.0];
                    *s = s.moved_to([s.position[0] + d[0], s.position[1] + d[1], s.position[2]]);
                    // Its tied points follow (else the line is refused).
                    let (id, p) = (s.id.clone(), s.position);
                    if rng.below(3) > 0 {
                        for a in &mut line.alignment_points {
                            if a.structure.as_deref() == Some(&id) {
                                a.position = p;
                            }
                        }
                    }
                }
                3 => s.soil_resistivity_ohm_m = Some(-1.0),
                4 => {
                    if !s.wires.is_empty() {
                        let w = rng.below(s.wires.len());
                        s.wires.remove(w);
                    }
                }
                5 => {
                    line.structures.remove(k);
                }
                6 => {
                    let mut c = s.clone();
                    c.id = format!("{}c{}", c.id, rng.below(1000));
                    let at = rng.below(line.structures.len() + 1);
                    if !line.structures.iter().any(|x| x.id == c.id) {
                        line.structures.insert(at, c);
                    }
                }
                7 => s.routes = vec!["nope".into()],
                _ => s.position[0] = f64::NAN,
            }
        }
        1 if !line.sections.is_empty() => {
            let k = pick(rng, line.sections.len());
            let s = &mut line.sections[k];
            match rng.below(12) {
                0 => {
                    line.sections.remove(k);
                }
                1 => s.points.truncate(1),
                2 => s.points[0].set = 99,
                3 => s.points[0].structure = "TX".into(),
                4 => s.system = Some("nope".into()),
                5 => s.phase_label = "L3".into(),
                6 => s.wind_factors.push(crate::SpanWind {
                    span: 99,
                    factor: 1.0,
                }),
                7 => {
                    let s = line.sections.remove(k);
                    moved.push((Of::Section, s.id.clone()));
                    line.sections.insert(0, s);
                }
                8 => {
                    let mut c = s.clone();
                    c.id = format!("{}c{}", c.id, rng.below(1000));
                    if rng.below(2) == 0 {
                        c.phase_label = "L2".into();
                    }
                    c.span_points.clear();
                    if !line.sections.iter().any(|x| x.id == c.id) {
                        let at = rng.below(line.sections.len() + 1);
                        line.sections.insert(at, c);
                    }
                }
                9 => {
                    if let Some(p) = s.span_points.first_mut() {
                        p.at = SpanPlace::at_fraction(1.5);
                    }
                }
                10 => {
                    if !s.span_points.is_empty() {
                        s.span_points.remove(0);
                    }
                }
                _ => {
                    s.stringing = Stringing::Tension {
                        horizontal_tension: -1.0,
                        temperature: 15.0,
                        condition: Default::default(),
                    }
                }
            }
        }
        2 if !line.jumpers.is_empty() => {
            let k = pick(rng, line.jumpers.len());
            let j = &mut line.jumpers[k];
            match rng.below(7) {
                0 => {
                    line.jumpers.remove(k);
                }
                1 => j.to = j.from.clone(),
                2 => j.length = Some(-1.0),
                3 => {
                    // A ring of taps (or a tap on itself).
                    j.from = JumperEnd::tap(Tap::Jumper {
                        jumper: "JJ".into(),
                        point: "K".into(),
                    });
                }
                4 => {
                    if let JumperEnd::Wire(p) = &mut j.to {
                        p.set = 99;
                    }
                }
                5 => {
                    let mut c = j.clone();
                    c.id = format!("{}c{}", c.id, rng.below(1000));
                    // Between two conductor paths.
                    c.to = JumperEnd::Wire(SectionPoint::new("T2", 11));
                    c.from = JumperEnd::Wire(SectionPoint::new("T2", 1));
                    if !line.jumpers.iter().any(|x| x.id == c.id) {
                        line.jumpers.push(c);
                    }
                }
                _ => j.bundle_spacing = Some(-1.0),
            }
        }
        3 if !line.spacers.is_empty() => {
            let k = pick(rng, line.spacers.len());
            match rng.below(3) {
                0 => {
                    line.spacers.remove(k);
                }
                1 => line.spacers[k].a.point = "nope".into(),
                _ => line.spacers[k].weight = -1.0,
            }
        }
        4 if !line.alignment_points.is_empty() => {
            let k = pick(rng, line.alignment_points.len());
            match rng.below(3) {
                0 => line.alignment_points[k].position[0] += 5.0,
                1 => {
                    line.alignment_points.remove(k);
                }
                _ => line.alignment_points[k].structure = Some("TX".into()),
            }
        }
        5 if !line.alignments.is_empty() => {
            let k = pick(rng, line.alignments.len());
            match rng.below(4) {
                0 => {
                    line.alignments.remove(k);
                }
                1 => line.alignments[k].route = "nope".into(),
                2 => line.alignments[k].points.push("PX".into()),
                _ => {
                    let p = line.alignments[k].points[0].clone();
                    line.alignments[k].points.push(p);
                }
            }
        }
        6 if !line.routes.is_empty() => {
            if rng.below(2) == 0 {
                let k = pick(rng, line.routes.len());
                line.routes.remove(k);
            } else {
                line.routes
                    .push(Route::new(&format!("r{}", rng.below(100)), "new"));
            }
        }
        _ => {
            // A structure's id given twice (only a file can have it).
            // (And of the other kinds: the whole validation by the index
            // finds them as the reference does.)
            match rng.below(48) {
                0 if line.structures.len() > 1 => {
                    let id = line.structures[0].id.clone();
                    line.structures[1].id = id;
                }
                1 if line.sections.len() > 1 => {
                    let k = rng.below(line.sections.len());
                    let id = line.sections[k].id.clone();
                    line.sections[0].id = id;
                }
                2 if line.jumpers.len() > 1 => {
                    let id = line.jumpers[1].id.clone();
                    line.jumpers[0].id = id;
                }
                3 if line.alignment_points.len() > 1 => {
                    let id = line.alignment_points[0].id.clone();
                    line.alignment_points[1].id = id;
                }
                4 if line.routes.len() > 1 => {
                    let id = line.routes[0].id.clone();
                    line.routes[1].id = id;
                }
                5 if line.spacers.len() == 1 => {
                    let s = line.spacers[0].clone();
                    line.spacers.push(s);
                }
                _ => {}
            }
        }
    }
}

/// The local validation of an edit: the scope before and after.
fn local(before: &Line, after: &Line, moved: &[(Of, String)]) -> Result<(), LineError> {
    let mut changed = changed(before, after);
    changed.extend(moved.iter().cloned());
    let (ib, ia) = (LineIndex::of(before), LineIndex::of(after));
    let mut scope = Scope::of_change(before, &ib, &changed);
    scope.merge(Scope::of_change(after, &ia, &changed));
    after.validate_local(&ia, &scope)
}

static SEEN: std::sync::Mutex<std::collections::BTreeMap<String, usize>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

#[test]
fn whole_and_local_validation_equal_the_reference() {
    let base = rich();
    for seed in 1..=12u64 {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0x1000_0001));
        let mut line = base.clone();
        let mut sides = crate::solve::Sides::of(&line);
        let (mut valid, mut refused) = (0, 0);
        for step in 0..600 {
            let mut after = line.clone();
            let mut moved = Vec::new();
            for _ in 0..=rng.below(2) {
                edit(&mut after, &mut rng, &mut moved);
            }
            let reference = after.validate_reference();
            assert_eq!(
                after.validate(),
                reference,
                "seed {seed} step {step}: whole"
            );
            // Ids given twice exist only in files: the session's lines
            // never have them (and the local validation needs an index of
            // unique ids).
            let unique = Of::ALL.iter().all(|&of| {
                let n = crate::index::count(&after, of);
                let ids: std::collections::BTreeSet<String> =
                    (0..n).map(|k| index_id(&after, of, k)).collect();
                ids.len() == n
            });
            assert_eq!(
                crate::solve::sections_against_the_line(&after),
                crate::solve::sections_against_the_line_reference(&after),
                "seed {seed} step {step}: sides"
            );
            if unique {
                assert_eq!(
                    local(&line, &after, &moved),
                    reference,
                    "seed {seed} step {step}: local"
                );
                let ia = LineIndex::of(&after);
                for j in &after.jumpers {
                    for e in [&j.from, &j.to] {
                        assert_eq!(
                            crate::jumper::approximate_end_in(&after, &ia, e, 0),
                            crate::jumper::approximate_end(&after, e, 0)
                        );
                    }
                }
            }
            if unique && reference.is_ok() {
                // The sections to solve again, from the objects changed.
                let changed = changed(&line, &after);
                let ib = LineIndex::of(&line);
                let before: Vec<(Of, String, Option<Item>)> = changed
                    .iter()
                    .map(|(of, id)| (*of, id.clone(), ib.get(&line, *of, id)))
                    .collect();
                let routes = changed
                    .iter()
                    .any(|(of, _)| matches!(of, Of::Route | Of::Alignment | Of::AlignmentPoint));
                let ia = LineIndex::of(&after);
                // The sides kept: the sections changed and through a
                // structure changed worked out again, and those a route's
                // change may turn.
                let mut moved = std::collections::BTreeSet::new();
                for (of, id) in &changed {
                    match of {
                        Of::Section => {
                            moved.insert(id.clone());
                        }
                        Of::Structure => {
                            for i in [&ib, &ia] {
                                moved.extend(
                                    i.referrers_of(Of::Structure, id, Of::Section).cloned(),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                let turned = sides.update(&after, &ia, &moved, routes);
                let again = crate::solve::Sides::of(&after);
                assert_eq!(
                    sides.against(),
                    again.against(),
                    "seed {seed} step {step}: sides kept"
                );
                let (a, b) = (
                    crate::solve::sections_against_the_line(&line),
                    crate::solve::sections_against_the_line(&after),
                );
                let differ: std::collections::BTreeSet<String> = b
                    .iter()
                    .filter(|(id, side)| a.get(*id) != Some(side))
                    .map(|(id, _)| id.clone())
                    .collect();
                if routes {
                    assert_eq!(turned, differ, "seed {seed} step {step}: sides turned");
                }
                assert_eq!(
                    crate::diff::sections_to_solve_again_local(
                        &before,
                        &after,
                        &ia,
                        routes.then_some(&turned)
                    ),
                    crate::diff::sections_to_solve_again(&line, &after),
                    "seed {seed} step {step}: sections"
                );
                assert_eq!(
                    crate::diff::sections_to_solve_again(&line, &after),
                    crate::diff::sections_to_solve_again_reference(&line, &after),
                    "seed {seed} step {step}: sections, whole"
                );
            }
            if let Err(e) = &reference {
                let m = e.to_string();
                let key: String = m.chars().filter(|c| !c.is_ascii_digit()).take(70).collect();
                *SEEN.lock().unwrap().entry(key).or_insert(0) += 1;
            }
            if reference.is_ok() {
                line = after;
                valid += 1;
            } else {
                refused += 1;
            }
        }
        assert!(
            valid > 50 && refused > 50,
            "seed {seed}: {valid} valid, {refused} refused"
        );
    }
    for (k, n) in SEEN.lock().unwrap().iter() {
        eprintln!("{n:5} {k}");
    }
}

#[test]
fn the_index_follows_edits_made_through_it() {
    let mut line = rich();
    let mut index = LineIndex::of(&line);
    let mut rng = Rng(77);
    for _ in 0..300 {
        let of = Of::ALL[rng.below(7)];
        let n = crate::index::count(&line, of);
        if n == 0 {
            continue;
        }
        let id = index_id(&line, of, rng.below(n));
        let o = index.remove(&mut line, of, &id).expect("there");
        let at = rng.below(crate::index::count(&line, of) + 1);
        let o = match o {
            Item::Section(mut s) if rng.below(2) == 0 => {
                s.points.reverse();
                Item::Section(s)
            }
            o => o,
        };
        index.insert(&mut line, o, at);
        assert_eq!(index, LineIndex::of(&line));
    }
}
