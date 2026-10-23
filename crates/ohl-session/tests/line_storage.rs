// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What starting a project line costs on an invented long line —
//! `SPANS` spans (default 1100: 1 101 structures, 552 sections) with
//! `LIBRARY` library files of 200 kB (default 5): the time to start a
//! line, the rows it writes, the growth of the project file, and the
//! time to open the project again (the memory held was measured once with
//! a counting allocator, docs/development/decisions.md 2026-10-23; the
//! workspace keeps no unsafe code). Ignored by
//! default; run with `cargo test -p ohl-session --release --test
//! line_storage -- --ignored --nocapture`.

use std::path::Path;
use std::time::Instant;

use ohl_line::Line;
use ohl_session::Session;

fn env(name: &str, or: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(or)
}

/// The invented line: towers every 350 m along x, three phases 6 m apart;
/// sections of six spans.
fn long_line(spans: usize) -> Line {
    const PER_SECTION: usize = 6;
    let mut structures = Vec::new();
    for i in 0..=spans {
        let x = i as f64 * 350.0;
        let z = 500.0 + 8.0 * (x / 700.0).sin();
        let dead = i % PER_SECTION == 0 || i == spans;
        let mut points = Vec::new();
        let mut strings = Vec::new();
        let mut wires = Vec::new();
        for (k, y) in [(1, -6.0), (2, 0.0), (3, 6.0)] {
            points.push(format!(
                r#"{{"label": "A{k}", "position": [{x}, {y}, {}]}}"#,
                z + 30.0
            ));
            if dead {
                let mut sides = Vec::new();
                if i > 0 {
                    sides.push((k, 180.0, -5.0));
                }
                if i < spans {
                    sides.push((10 + k, 0.0, 5.0));
                }
                for (set, dir, dx) in sides {
                    strings.push(format!(
                        r#"{{"label": "S{set}", "top": "A{k}", "wire": "W{set}", "length": 5.024937810560445, "angle_deg": 84.28940686250036, "direction_deg": {dir}}}"#
                    ));
                    wires.push(format!(
                        r#"{{"label": "W{set}", "position": [{}, {y}, {}], "set": {set}}}"#,
                        x + dx,
                        z + 29.5
                    ));
                }
            } else {
                strings.push(format!(
                    r#"{{"label": "S{k}", "top": "A{k}", "wire": "W{k}", "length": 3.5, "weight": 900.0}}"#
                ));
                wires.push(format!(
                    r#"{{"label": "W{k}", "position": [{x}, {y}, {}], "set": {k}}}"#,
                    z + 26.5
                ));
            }
        }
        structures.push(format!(
            r#"{{"id": "T{i}", "position": [{x}, 0.0, {z}], "orientation_deg": -90.0, "points": [{}], "strings": [{}], "wires": [{}]}}"#,
            points.join(", "),
            strings.join(", "),
            wires.join(", ")
        ));
    }
    let mut sections = Vec::new();
    let mut start = 0;
    while start < spans {
        let end = (start + PER_SECTION).min(spans);
        for k in 1..=3 {
            let mut pts = vec![format!(r#"{{"structure": "T{start}", "set": {}}}"#, 10 + k)];
            for i in start + 1..=end {
                pts.push(format!(r#"{{"structure": "T{i}", "set": {k}}}"#));
            }
            sections.push(format!(
                r#"{{"id": "S{start}-L{k}", "cable": "acsr-drake-26-7", "circuit_name": "C", "phase_label": "L{k}", "system": "ex220", "bundle": 1, "points": [{}], "stringing": {{"kind": "tension", "horizontal_tension": 25000.0, "temperature": 15.0}}}}"#,
                pts.join(", ")
            ));
        }
        start = end;
    }
    let text = format!(
        r#"{{"format": "ohl-line", "version": 1, "name": "invented long line", "structures": [{}], "sections": [{}], "jumpers": [], "systems": [{{"id": "ex220", "name": "220 kV", "kind": "ac", "voltage_kv": 220.0, "frequency_hz": 50.0, "phases": [{{"label": "L1", "angle_deg": 0.0}}, {{"label": "L2", "angle_deg": -120.0}}, {{"label": "L3", "angle_deg": 120.0}}]}}]}}"#,
        structures.join(",\n"),
        sections.join(",\n")
    );
    Line::from_json(&text).unwrap()
}

/// The file's size once its write-ahead log is written into it.
fn file_size(path: &Path) -> u64 {
    let c = rusqlite::Connection::open(path).unwrap();
    c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    drop(c);
    std::fs::metadata(path).unwrap().len()
}

fn rows(path: &Path, table: &str, line: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            &format!("SELECT count(*) FROM {table} WHERE line = ?1"),
            [line],
            |r| r.get(0),
        )
        .unwrap()
}

fn mb(bytes: f64) -> String {
    format!("{:.2} MB", bytes / 1e6)
}

#[test]
#[ignore = "a measurement: run with --ignored --nocapture"]
fn starting_a_line_on_a_long_line() {
    let spans = env("SPANS", 1100);
    let library = env("LIBRARY", 5);
    let dir = std::env::temp_dir().join(format!("ohl-line-storage-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let line = long_line(spans);
    let mut s = Session::open(&path).unwrap();
    s.import(&line, "ana").unwrap();
    for k in 0..library {
        let text: String = (0..200_000)
            .map(|i| (b'a' + ((i * 7 + k) % 26) as u8) as char)
            .collect();
        s.put_library_file("ana", "cables", &format!("lib{k}.ohlw"), &text)
            .unwrap();
    }
    s.commit("ana", "first").unwrap();
    let objects = s.lines().unwrap()[0].objects;
    println!(
        "line: {} structures, {} sections, {objects} objects with {library} library files",
        line.structures.len(),
        line.sections.len()
    );
    let size0 = file_size(&path);
    let t = Instant::now();
    s.create_line("ana", "phase 1", "main").unwrap();
    let create = t.elapsed();
    let size1 = file_size(&path);
    println!("start a line:     {create:.2?}");
    println!(
        "rows written:     model_object {}, spatial_index {}",
        rows(&path, "model_object", "phase 1"),
        rows(&path, "spatial_index", "phase 1")
    );
    println!(
        "file:             {} -> {} (+{})",
        mb(size0 as f64),
        mb(size1 as f64),
        mb(size1 as f64 - size0 as f64)
    );
    let t = Instant::now();
    s.select_line("phase 1").unwrap();
    println!("select it:        {:.2?}", t.elapsed());
    s.select_line("main").unwrap();
    assert_eq!(s.line_named("phase 1").unwrap(), s.line());
    drop(s);
    let t = Instant::now();
    let s = Session::open(&path).unwrap();
    let open = t.elapsed();
    println!("open (2 lines):   {open:.2?}");
    assert_eq!(s.line_named("phase 1").unwrap(), s.line());
    drop(s);
    std::fs::remove_dir_all(&dir).ok();
}
