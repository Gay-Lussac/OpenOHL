// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The safe side, proven on the synthetic country**: each route a line
//! of its own (as a network holds them, towers shared by two routes in
//! both: twins), each line a project file opened by a session; a scoped
//! check takes what it reads as the server does (`Session::drawing_part`).
//! Each result is compared with the same check on the whole network:
//! every line solved whole, every wire a target.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use ohl_clearance::rules::{Rule, RuleSet};
use ohl_core::grid::Bounds;
use ohl_line::{Line, Point};
use ohl_session::Session;
use ohl_weather::WeatherCase;

use crate::bound::{SpanChord, chords_of};
use crate::clearance::{self, Outcome, RuleResult, WireId};
use crate::{Cases, Neighbourhood, WorkingSet};

/// The network of a synthetic country, every line opened by a session.
struct Net {
    dir: PathBuf,
    models: Vec<(String, Session)>,
    library: Arc<ohl_cables::Library>,
    cases: Arc<Cases>,
    margin: f64,
    /// Parts taken (sections asked, in all): what a scoped check read.
    asked: usize,
    /// Parts give the whole line instead (the whole network's reference).
    whole: bool,
}

impl Drop for Net {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn country(pylons: usize, seed: u64, tag: &str) -> (ohl_synth::Country, Net) {
    let country = ohl_synth::country(&ohl_synth::Settings {
        pylons,
        seed,
        ..Default::default()
    })
    .expect("a country");
    let dir = std::env::temp_dir().join(format!("ohl-scope-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let written = ohl_synth::project::write_line_projects(&country, &dir, "ana").expect("lines");
    let mut models = Vec::new();
    let mut margin: f64 = 0.0;
    for (code, path, _) in written {
        let s = Session::open(&path).expect("opens");
        margin = margin.max(crate::bound::max_margin(s.line()));
        models.push((code, s));
    }
    let library = Arc::new(
        ohl_cables::Library::load(&ohl_core::files::shipped_library("cables")).expect("wires"),
    );
    let net = Net {
        dir,
        models,
        library,
        cases: Arc::new(crate::default_cases().expect("cases")),
        margin,
        asked: 0,
        whole: false,
    };
    (country, net)
}

impl Net {
    fn line(&self, model: &str) -> &Line {
        self.models
            .iter()
            .find(|(c, _)| c == model)
            .map(|(_, s)| s.line())
            .expect("a model")
    }
}

impl Neighbourhood for Net {
    fn sections_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (code, s) in &self.models {
            let line = s.line();
            for sec in &line.sections {
                let b = Bounds::of_points(
                    sec.points
                        .iter()
                        .filter_map(|p| line.structure(&p.structure).map(|s| &s.position)),
                );
                if b.is_some_and(|b| b.intersects(area)) {
                    out.push((code.clone(), sec.id.clone()));
                }
            }
        }
        out
    }

    fn structures_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (code, s) in &self.models {
            for st in &s.line().structures {
                let p = [st.position[0], st.position[1]];
                if area.intersects(&Bounds::new(p, p)) {
                    out.push((code.clone(), st.id.clone()));
                }
            }
        }
        out
    }

    fn structure(&mut self, model: &str, id: &str) -> Option<(Point, Option<String>)> {
        self.line(model)
            .structure(id)
            .map(|s| (s.position, s.structure_type.clone()))
    }

    fn sections_at(&mut self, model: &str, id: &str) -> Vec<String> {
        self.line(model)
            .sections
            .iter()
            .filter(|s| s.points.iter().any(|p| p.structure == id))
            .map(|s| s.id.clone())
            .collect()
    }

    fn chords(&mut self, model: &str, sections: &[String]) -> Vec<(String, Vec<SpanChord>)> {
        let line = self.line(model);
        sections
            .iter()
            .filter_map(|id| line.sections.iter().find(|s| &s.id == id))
            .map(|s| (s.id.clone(), chords_of(s, |id| line.structure(id))))
            .collect()
    }

    fn part(
        &mut self,
        model: &str,
        structures: &[String],
        sections: &[String],
    ) -> Result<Line, String> {
        self.asked += sections.len();
        let whole = self.whole;
        let s = self
            .models
            .iter_mut()
            .find(|(c, _)| c == model)
            .map(|(_, s)| s)
            .ok_or("no model")?;
        Ok(if whole {
            s.line().clone()
        } else {
            s.drawing_part(structures, sections, &[], ohl_session::interest::SOLVE_NEAR)
        })
    }

    fn potentials(&mut self, model: &str) -> Vec<Option<ohl_line::electrical::Potential>> {
        crate::system_potentials(self.line(model))
    }

    fn library(&mut self, _: &str) -> Result<Arc<ohl_cables::Library>, String> {
        Ok(self.library.clone())
    }

    fn cases(&mut self, _: &str) -> Result<Arc<Cases>, String> {
        Ok(self.cases.clone())
    }

    fn max_margin(&mut self) -> f64 {
        self.margin
    }
}

/// A working set: the sections of `model` with a point on a tower shared
/// with another line (and `extra` more of its sections), its jumpers
/// between them, their structures.
fn working_set(net: &Net, model: &str, extra: usize) -> WorkingSet {
    let line = net.line(model);
    let shared: BTreeSet<String> = line
        .structures
        .iter()
        .filter(|s| {
            net.models.iter().any(|(c, o)| {
                c != model
                    && o.line().structures.iter().any(|t| {
                        (t.position[0] - s.position[0]).hypot(t.position[1] - s.position[1]) < 0.5
                    })
            })
        })
        .map(|s| s.id.clone())
        .collect();
    let mut sections: Vec<String> = line
        .sections
        .iter()
        .filter(|s| s.points.iter().any(|p| shared.contains(&p.structure)))
        .map(|s| s.id.clone())
        .collect();
    sections.extend(
        line.sections
            .iter()
            .filter(|s| !sections.contains(&s.id))
            .take(extra)
            .map(|s| s.id.clone())
            .collect::<Vec<_>>(),
    );
    let set: BTreeSet<&String> = sections.iter().collect();
    let on = |p: Option<&ohl_line::SectionPoint>| {
        p.is_some_and(|p| {
            line.sections
                .iter()
                .any(|s| set.contains(&s.id) && s.points.contains(p))
        })
    };
    let jumpers = line
        .jumpers
        .iter()
        .filter(|j| on(j.from.point()) && on(j.to.point()))
        .map(|j| j.id.clone())
        .collect();
    let structures = line
        .structures
        .iter()
        .filter(|s| shared.contains(&s.id))
        .map(|s| s.id.clone())
        .collect();
    WorkingSet {
        model: model.to_owned(),
        sections,
        jumpers,
        structures,
    }
}

fn rules() -> Vec<Rule> {
    RuleSet::parse(
        r#"
        [[rule]]
        name = "other lines"
        target = "other_lines"
        vertical = { by_voltage = [[150, 5.0], [420, 7.0]] }
        radial = { by_voltage = [[150, 4.0], [420, 6.5]] }
        [[rule]]
        name = "phase to phase"
        target = "same_line"
        radial = "3 m"
        [[rule]]
        name = "ground"
        target = "ground"
        vertical = "6 m"
        "#,
    )
    .expect("rules")
    .rules
}

fn case(net: &Net, name: &str) -> WeatherCase {
    net.cases
        .weather
        .iter()
        .find(|c| c.name == name)
        .expect("the case")
        .clone()
}

/// The same check on the whole network: every line solved whole, its
/// wires the line's or other lines' targets.
fn whole_clearances(
    net: &mut Net,
    ws: &WorkingSet,
    rules: &[Rule],
    case: &WeatherCase,
    min_reach: f64,
) -> Vec<RuleResult> {
    let mut line = Vec::new();
    let mut others = Vec::new();
    for (code, s) in &net.models {
        let all: BTreeSet<String> = s.line().sections.iter().map(|x| x.id.clone()).collect();
        let (wires, failed) = crate::solve_wires(s.line(), &net.library, case, &all, None);
        assert!(failed.is_empty(), "{code}: {failed:?}");
        for w in wires {
            let id = WireId {
                model: code.clone(),
                section: w.section.clone(),
                span: w.span,
            };
            if *code == ws.model {
                line.push((id, w));
            } else {
                others.push((id, w));
            }
        }
    }
    let sections: BTreeSet<&String> = ws.sections.iter().collect();
    let jumpers: BTreeSet<&String> = ws.jumpers.iter().collect();
    let checked = |id: &WireId| {
        id.model == ws.model
            && match id.section.strip_prefix("jumper:") {
                Some(j) => jumpers.contains(&j.to_owned()),
                None => sections.contains(&id.section),
            }
    };
    clearance::run_rules(rules, &case.name, &line, &others, &checked, min_reach).expect("runs")
}

type Found = BTreeMap<(String, WireId, Option<WireId>), ohl_clearance::WireResult>;

fn by_pair(results: &[RuleResult]) -> (Found, Vec<String>) {
    let mut out = BTreeMap::new();
    let mut not = Vec::new();
    for r in results {
        match &r.outcome {
            Outcome::Wires(w) => {
                for (a, x) in w {
                    out.insert((r.name.clone(), a.clone(), None), x.clone());
                }
            }
            Outcome::Pairs(p) => {
                for (a, b, x) in p {
                    out.insert((r.name.clone(), a.clone(), Some(b.clone())), x.clone());
                }
            }
            Outcome::NotChecked(why) => not.push(format!("{}: {why}", r.name)),
        }
    }
    (out, not)
}

/// Clearances and crossings on working sets of several lines, scoped
/// against the whole network: the same pairs, the same distances to the
/// bit, the same violations — in two weather cases (still air, wind).
#[test]
fn scoped_clearances_equal_the_whole_networks() {
    let (_, mut net) = country(300, 1, "clearances");
    let rules = rules();
    let codes: Vec<String> = net.models.iter().map(|(c, _)| c.clone()).collect();
    let mut compared = 0;
    let mut violations = 0;
    let mut other_lines_used = 0;
    let mut checked_sets = 0;
    for code in &codes {
        let ws = working_set(&net, code, 4);
        if ws.structures.is_empty() {
            continue;
        }
        checked_sets += 1;
        for name in ["10 °C", "10 °C + 300 Pa wind"] {
            let case = case(&net, name);
            net.whole = false;
            let scoped = clearance::clearances(&mut net, &ws, &rules, &case).expect("scoped");
            assert!(
                scoped.not_all_checked.is_empty(),
                "{code} {name}: {:?}",
                scoped.not_all_checked
            );
            assert!(scoped.notes.is_empty(), "{:?}", scoped.notes);
            let crate::Reach::Wires { metres } = scoped.reach else {
                panic!("a reach")
            };
            other_lines_used += scoped
                .pulled
                .used
                .iter()
                .filter(|n| n.model != *code)
                .count();
            let own: Vec<(WireId, ohl_clearance::Wire)> = Vec::new();
            let _ = own;
            let min = scoped_min_reach(&mut net, &ws, &case, metres);
            let whole = whole_clearances(&mut net, &ws, &rules, &case, min);
            let (a, not_a) = by_pair(&scoped.rules);
            let (b, not_b) = by_pair(&whole);
            assert_eq!(not_a, not_b);
            assert!(
                not_a.iter().any(|n| n.starts_with("ground: no ground")),
                "{not_a:?}"
            );
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{code} {name}: the same pairs"
            );
            for (k, x) in &a {
                assert_eq!(Some(x), b.get(k), "{code} {name}: {k:?}");
                violations += usize::from(x.violates());
            }
            compared += a.len();
        }
    }
    eprintln!(
        "clearances: {checked_sets} working sets, {compared} results equal, {violations} \
         violations, {other_lines_used} other lines' objects used"
    );
    assert!(checked_sets >= 3, "{checked_sets}");
    assert!(compared > 500, "{compared}");
    assert!(violations > 10, "{violations}");
    assert!(other_lines_used > 20, "{other_lines_used}");
}

/// The scoped run's own reach (the same on the whole network's run, so
/// that both look as far).
fn scoped_min_reach(net: &mut Net, ws: &WorkingSet, case: &WeatherCase, reach: f64) -> f64 {
    let lib = net.library.clone();
    let part = net
        .part(&ws.model, &[], &ws.sections)
        .expect("the working set's part");
    let sections: BTreeSet<String> = ws.sections.iter().cloned().collect();
    let jumpers: BTreeSet<String> = ws.jumpers.iter().cloned().collect();
    let (wires, _) = crate::solve_wires(&part, &lib, case, &sections, Some(&jumpers));
    let own: Vec<(WireId, ohl_clearance::Wire)> = wires
        .into_iter()
        .map(|w| {
            (
                WireId {
                    model: ws.model.clone(),
                    section: w.section.clone(),
                    span: w.span,
                },
                w,
            )
        })
        .collect();
    clearance::search_reach(reach, &own)
}

/// A scoped run reads its reach's objects only, never a whole line: on a
/// working set of a few sections it asks far fewer sections than the
/// network holds.
#[test]
fn a_scoped_check_reads_only_its_reach() {
    let (_, mut net) = country(300, 1, "reads");
    let total: usize = net
        .models
        .iter()
        .map(|(_, s)| s.line().sections.len())
        .sum();
    let code = net
        .models
        .iter()
        .map(|(c, _)| c.clone())
        .find(|c| !working_set(&net, c, 0).structures.is_empty())
        .expect("a line with a shared tower");
    let ws = working_set(&net, &code, 0);
    let case = case(&net, "10 °C");
    net.asked = 0;
    let r = clearance::clearances(&mut net, &ws, &rules(), &case).expect("runs");
    assert!(!r.pulled.used.is_empty());
    assert!(
        net.asked * 3 < total,
        "asked {} of {total} sections",
        net.asked
    );
    // What it used is within its reach, what it read beyond is listed.
    assert!(r.pulled.used.is_subset(&r.pulled.read));
}

/// The loads on shared towers, every line's sections on them under each
/// line's cases: scoped (each line's sections there, as solving them
/// alone reads them) equal to the whole network's (every line whole), to
/// the bit; and each case's loads equal to the country as one line, the
/// tower one structure carrying every route's wires.
#[test]
fn shared_tower_loads_equal_the_whole() {
    let (country, mut net) = country(300, 1, "loads");
    let budget = ohl_loads::default_budget();
    let cases = net.cases.clone();
    let known: Vec<String> = cases.loads.iter().map(|c| c.name.clone()).collect();
    let single = ohl_loads::structure_loads_each(
        &country.line,
        &net.library,
        &cases.weather,
        &cases.loads,
        &known,
        &budget,
    );
    let codes: Vec<String> = net.models.iter().map(|(c, _)| c.clone()).collect();
    let mut towers = 0;
    let mut points = 0;
    let mut worst: f64 = 0.0;
    for code in &codes {
        let mut ws = working_set(&net, code, 0);
        ws.structures.truncate(3);
        if ws.structures.is_empty() {
            continue;
        }
        net.whole = false;
        let scoped = crate::loads::shared_tower_loads(&mut net, &ws, &budget).expect("scoped");
        assert!(
            scoped.not_all_checked.is_empty(),
            "{:?}",
            scoped.not_all_checked
        );
        net.whole = true;
        let whole = crate::loads::shared_tower_loads(&mut net, &ws, &budget).expect("whole");
        net.whole = false;
        assert_eq!(
            scoped.towers, whole.towers,
            "{code}: scoped = whole network"
        );
        for t in &scoped.towers {
            assert!(
                !t.twins.is_empty(),
                "{code} {}: a shared tower",
                t.structure
            );
            towers += 1;
            for c in t.cases.iter().filter(|c| c.line == *code) {
                let k = cases
                    .loads
                    .iter()
                    .position(|x| x.name == c.case)
                    .expect("a case");
                let one = single[k].as_ref().expect("the country's case");
                for p in &c.points {
                    let at: Vec<&ohl_loads::AttachmentLoad> = one
                        .loads
                        .iter()
                        .filter(|a| a.structure == t.structure && a.point == p.point)
                        .collect();
                    assert_eq!(at.len(), 1, "{} {}", t.structure, p.point);
                    // In the line's axes: a tower of two routes has no
                    // one direction of the line in a single project (the
                    // structure's axes follow the route there), and its
                    // sections are solved with the nearest alignment's
                    // direction (another route's there): not the same
                    // model, so only near (measured: about 1 %) — a line's
                    // sections missing or counted twice would be off by
                    // far more.
                    for k in 0..3 {
                        let (x, y) = (p.force[k], at[0].force[k]);
                        let size = at[0].force.iter().map(|v| v.abs()).fold(1.0, f64::max);
                        let rel = (x - y).abs() / size;
                        worst = worst.max(rel);
                        assert!(
                            rel <= 5e-2,
                            "{code} {} {} {}: {x} against the single line's {y}",
                            t.structure,
                            c.case,
                            p.point
                        );
                    }
                    points += 1;
                }
            }
            assert!(!t.worst.is_empty());
        }
    }
    assert!(towers >= 3, "{towers}");
    assert!(points > 50, "{points}");
    eprintln!("loads: {towers} towers, {points} points, against one project at most {worst:.2e}");
}

/// The field with the lines within the field distance, against the whole
/// network's (every line whole, added by size as the scoped check adds
/// them): what the distance leaves out is said NOT ON THE SAFE SIDE and
/// measured here.
#[test]
fn fields_with_the_lines_near() {
    let (_, mut net) = country(300, 1, "fields");
    let codes: Vec<String> = net.models.iter().map(|(c, _)| c.clone()).collect();
    let spec = crate::fields::FieldSpec {
        amps: Some(500.0),
        width: 30.0,
        step: 10.0,
        ..Default::default()
    };
    let mut worst_left_out: f64 = 0.0;
    let mut largest: f64 = 0.0;
    let mut sets = 0;
    for code in codes.iter().take(6) {
        let mut ws = working_set(&net, code, 2);
        if ws.structures.is_empty() {
            continue;
        }
        ws.sections.truncate(8);
        let case = case(&net, "40 °C");
        net.whole = false;
        let scoped = crate::fields::fields(&mut net, &ws, &case, &spec).expect("scoped");
        assert!(
            scoped
                .notes
                .iter()
                .any(|n| n.starts_with("NOT ON THE SAFE SIDE")),
            "{:?}",
            scoped.notes
        );
        assert!(
            scoped.not_all_checked.is_empty(),
            "{:?}",
            scoped.not_all_checked
        );
        // The whole network: the line whole at the same profiles, every
        // other line whole added by size.
        let sections: BTreeSet<String> = ws.sections.iter().cloned().collect();
        let own_line = net.line(code).clone();
        let stations = crate::fields::stations_of(&own_line, &sections);
        let (own, _, _) =
            crate::fields::line_field(&own_line, &net.library, &case, &spec, stations)
                .expect("own");
        let mut others = Vec::new();
        for (c, s) in &net.models {
            if c == code {
                continue;
            }
            let (_, w, _) =
                crate::fields::line_field(s.line(), &net.library, &case, &spec, Vec::new())
                    .expect("other");
            others.push(w);
        }
        let refs: Vec<&ohl_electrical::magnetic::Wires3d> = others.iter().collect();
        let (whole, max, _) = crate::fields::add_by_size(&own, &refs, None);
        assert_eq!(whole.len(), scoped.profiles.len());
        for (w, s) in whole.iter().zip(&scoped.profiles) {
            assert_eq!((&w.from, &w.to), (&s.from, &s.to));
            for (a, b) in w.points.iter().zip(&s.points) {
                assert_eq!(a.at, b.at);
                worst_left_out = worst_left_out.max(a.b_ut - b.b_ut);
            }
        }
        largest = largest.max(max);
        sets += 1;
    }
    assert!(sets >= 2, "{sets}");
    assert!(largest > 1.0, "{largest}");
    // What the field distance leaves out: below a thousandth of the
    // largest field here (measured, said in the theory).
    assert!(
        worst_left_out <= 1e-3 * largest,
        "left out {worst_left_out} µT of {largest} µT"
    );
    eprintln!("field: largest {largest:.3} µT, left out at most {worst_left_out:.6} µT");
}

/// The electric field with the lines near, solved together: every point
/// at least this line alone, the method said, the field distance said NOT
/// ON THE SAFE SIDE; measured.
#[test]
fn electric_field_with_the_lines_near() {
    let (_, mut net) = country(300, 1, "electric");
    let codes: Vec<String> = net.models.iter().map(|(c, _)| c.clone()).collect();
    let spec = crate::electric::ElectricSpec {
        width: 30.0,
        step: 5.0,
        ..Default::default()
    };
    let (mut together, mut sets) = (0, 0);
    let mut largest_gain: f64 = 0.0;
    for code in codes.iter().take(6) {
        let mut ws = working_set(&net, code, 2);
        if ws.structures.is_empty() {
            continue;
        }
        ws.sections.truncate(4);
        let case = case(&net, "40 °C");
        net.whole = false;
        let t = std::time::Instant::now();
        let r = crate::electric::electric_field(&mut net, &ws, &case, &spec).expect("electric");
        let secs = t.elapsed().as_secs_f64();
        assert!(
            r.notes
                .iter()
                .any(|n| n.starts_with("NOT ON THE SAFE SIDE")),
            "{:?}",
            r.notes
        );
        assert!(r.not_all_checked.is_empty(), "{:?}", r.not_all_checked);
        assert!(!r.profiles.is_empty());
        assert!(r.max_kv_m > 0.0);
        let with_others = r.taken_in.len() > 1;
        if with_others {
            together += 1;
            assert!(r.method.starts_with("solved together"), "{}", r.method);
            assert!(r.profiles.iter().all(|p| p.pieces > 0));
        }
        for p in &r.profiles {
            for q in &p.points {
                assert!(q.e_kv_m >= q.own_kv_m);
                assert_eq!(q.joint_kv_m.is_some(), with_others);
                if let Some(j) = q.joint_kv_m {
                    assert!(j.is_finite() && j > 0.0);
                    largest_gain = largest_gain.max(j - q.own_kv_m);
                }
            }
        }
        eprintln!(
            "electric: line {code}, {} profile(s), {} line(s) in, {} pieces at most, largest \
             {:.3} kV/m, {secs:.2} s ({})",
            r.profiles.len(),
            r.taken_in.len(),
            r.profiles.iter().map(|p| p.pieces).max().unwrap_or(0),
            r.max_kv_m,
            r.method
        );
        sets += 1;
    }
    assert!(sets >= 2, "{sets}");
    assert!(together >= 1, "{together}");
    eprintln!("electric: largest rise over this line alone {largest_gain:.4} kV/m");
}
