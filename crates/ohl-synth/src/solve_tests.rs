// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The solver on a synthetic country (O42): a case solved whole, then
//! updated after edits as OHL-Design updates it.

use super::*;
use ohl_line::incremental::SolvedLine;
use std::path::Path;
use std::time::Instant;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn cables() -> ohl_cables::Library {
    ohl_cables::Library::load(&root().join("libraries/cables")).unwrap()
}

fn weather(k: usize) -> ohl_weather::WeatherCase {
    ohl_weather::WeatherCases::load(&root().join("examples/synthetic-line/weather-cases.ohlwc"))
        .unwrap()
        .cases[k]
        .clone()
}

/// Structure `id` of `line` moved by 1 m east.
fn moved(line: &Line, id: &str) -> Line {
    let mut out = line.clone();
    let s = out.structures.iter_mut().find(|s| s.id == id).unwrap();
    *s = s.moved_to([s.position[0] + 1.0, s.position[1], s.position[2]]);
    out
}

/// Every result of `solved` in `line`, as text (`Debug`: every number bit
/// for bit).
fn text(line: &Line, solved: &SolvedLine) -> String {
    format!(
        "{:?}\n{:?}",
        solved.results(line),
        solved.jumper_results(line)
    )
}

/// The cases solved with the solve's lookups (O42: each object looked up
/// once per solve, the route views and the jumpers' loads worked out once)
/// equal the cases solved with every lookup searched through the line as
/// before ([`ohl_line::lookups::searched`]), bit for bit (every result's
/// `Debug` text): a whole solve, then updates after two structures moved
/// and moved back, and the full solves (`solve_line`, `solve_jumpers`).
/// The jumpers' cut lengths are forgotten before each way (each cuts them
/// again).
fn solves_as_the_searches(name: &str, line: &Line, case: &ohl_weather::WeatherCase) {
    let lib = cables();
    let both = |f: &dyn Fn() -> String| {
        ohl_line::solve::forget_solves();
        let looked = f();
        ohl_line::solve::forget_solves();
        let searched = ohl_line::lookups::searched(f);
        assert!(
            looked == searched,
            "{name}, {}: the lookups solve otherwise",
            case.name
        );
    };
    let st = &line.structures;
    let ids: Vec<String> = (0..2)
        .map(|k| st[(k * 7919 + st.len() / 3) % st.len()].id.clone())
        .collect();
    both(&|| {
        let mut solved = SolvedLine::solve(line, &lib, case);
        let mut out = text(line, &solved);
        for id in &ids {
            let after = moved(line, id);
            for (from, to) in [(line, &after), (&after, line)] {
                let dirty: Vec<String> = ohl_line::diff::sections_to_solve_again(from, to)
                    .into_iter()
                    .collect();
                solved.update(to, &lib, &dirty);
                out.push_str(&text(to, &solved));
            }
        }
        out
    });
    both(&|| {
        let shapes = ohl_line::solve::solve_line(line, &lib, case);
        let ok: Vec<_> = shapes.iter().filter_map(|r| r.clone().ok()).collect();
        let jumpers = ohl_line::jumper::solve_jumpers(line, &lib, case, &ok);
        format!("{shapes:?}\n{jumpers:?}")
    });
}

/// [`solves_as_the_searches`] on the example lines (the typed one
/// resolved with its libraries), in still air and in wind.
#[test]
fn the_lookups_solve_the_examples_as_the_searches() {
    let ex = root().join("examples/synthetic-line");
    let mut lines: Vec<(String, Line)> = Vec::new();
    for name in ["synthetic-line.ohl.json", "crossing-line.ohl.json"] {
        lines.push((name.into(), Line::load(&ex.join(name)).unwrap()));
    }
    let typed = Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap();
    let structures = ohl_structures::StructureLibrary::load_dir(&ex.join("Structures")).unwrap();
    let insulators = ohl_structures::InsulatorLibrary::load_dir(&ex.join("Insulators")).unwrap();
    lines.push((
        "typed".into(),
        typed.resolve(&structures, &insulators).unwrap(),
    ));
    for (name, line) in &lines {
        for case in [weather(0), weather(4)] {
            solves_as_the_searches(name, line, &case);
        }
    }
}

/// A synthetic country of 20 pylons: several routes, shared towers, more
/// than 64 sections (the wind sides worked out through the routes' grid).
fn small_country() -> Line {
    let c = country(&Settings {
        pylons: 20,
        ..Settings::default()
    })
    .unwrap();
    assert!(c.line.sections.len() > 64, "{:?}", c.summary);
    c.line
}

/// [`solves_as_the_searches`] on a small synthetic country in still air.
#[test]
fn the_lookups_solve_a_country_in_still_air_as_the_searches() {
    solves_as_the_searches("country", &small_country(), &weather(0));
}

/// [`solves_as_the_searches`] on a small synthetic country in wind.
#[test]
fn the_lookups_solve_a_country_in_wind_as_the_searches() {
    solves_as_the_searches("country", &small_country(), &weather(4));
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// A case of the synthetic country solved whole (as OHL-Design opens a
/// project) and updated after structures moved by 1 m, each followed by
/// its undo and redo (as OHL-Design updates it after an edit; the
/// sections to solve again as the desktop finds them). Ignored by
/// default.
///
/// ```bash
/// OHL_BENCH_PYLONS=2000 cargo test --profile bench-fast -p ohl-synth solve_update_benchmark -- --ignored --nocapture
/// # OHL_BENCH_EDITS=N moves (default 10); OHL_BENCH_CASE=k the example
/// # weather case (default 0, still air)
/// ```
#[test]
#[ignore = "benchmark: run optimised with --ignored --nocapture"]
fn solve_update_benchmark() {
    let var = |k: &str, d: usize| {
        ohl_core::env::var(k)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(d)
    };
    let pylons = var("OHL_BENCH_PYLONS", 500);
    let edits = var("OHL_BENCH_EDITS", 10);
    let case = weather(var("OHL_BENCH_CASE", 0));
    let lib = cables();
    let t = Instant::now();
    let line = country(&Settings {
        pylons,
        ..Settings::default()
    })
    .unwrap()
    .line;
    eprintln!(
        "country of {pylons} pylons: {} sections, {} jumpers, made in {:.1?}",
        line.sections.len(),
        line.jumpers.len(),
        t.elapsed()
    );
    let t = Instant::now();
    let mut solved = SolvedLine::solve(&line, &lib, &case);
    let open = t.elapsed();
    eprintln!("case \"{}\" solved whole in {open:.2?}", case.name);
    let st = &line.structures;
    let ids: Vec<String> = (0..edits)
        .map(|k| st[(k * 7919 + st.len() / 3) % st.len()].id.clone())
        .collect();
    let mut times = Vec::new();
    let mut counts = Vec::new();
    for id in &ids {
        let after = moved(&line, id);
        // The move, its undo and its redo.
        for (from, to) in [(&line, &after), (&after, &line), (&line, &after)] {
            let dirty: Vec<String> = ohl_line::diff::sections_to_solve_again(from, to)
                .into_iter()
                .collect();
            let t = Instant::now();
            let u = solved.update(to, &lib, &dirty);
            times.push(t.elapsed().as_secs_f64() * 1e3);
            counts.push((u.solved, u.jumpers));
        }
        // Back to the line as it was.
        let dirty: Vec<String> = ohl_line::diff::sections_to_solve_again(&after, &line)
            .into_iter()
            .collect();
        solved.update(&line, &lib, &dirty);
    }
    let max = times.iter().copied().fold(0.0, f64::max);
    println!(
        "| {pylons} pylons ({} sections, {} jumpers) | case {} | open {:.1} s | edit median {:.1} ms, at most {:.1} ms | {} updates; sections, jumpers solved again {:?} |",
        line.sections.len(),
        line.jumpers.len(),
        case.name,
        open.as_secs_f64(),
        median(&mut times),
        max,
        times.len(),
        &counts[..counts.len().min(6)],
    );
}
