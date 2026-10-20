// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Files of before 2026-10-18, when file extensions and format ids changed
//! from `ols` to `ohl` (`.olsw` → `.ohlw`, `ols-structure` →
//! `ohl-structure`, `.ols` projects → `.ohl`), still open: the example line
//! and libraries as they were then (`tests/data/before-ohl`, old names and
//! old ids) give the same results as the current examples, to the digit.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use ohl_structures::LibraryItem;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn old() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/before-ohl")
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-older-names-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ohl(args: &[String]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ohl {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

/// The outputs of `ohl line` the comparisons read.
const OUTPUTS: [&str; 5] = ["spans", "wires", "loads", "usage", "clearances"];

/// `ohl line` on `line` with the libraries of `lib`, the line's own
/// weather, load cases and rules (`ext`: their extensions), every output
/// into `out`.
fn line(line: &Path, lib: &Path, ext: [&str; 3], out: &Path) {
    let dir = line.parent().unwrap();
    let mut args: Vec<String> = vec![
        "line".into(),
        s(line),
        "--library".into(),
        s(&lib.join("cables")),
        "--insulators".into(),
        s(&lib.join("insulators")),
        "--weather".into(),
        s(&dir.join(format!("weather-cases.{}", ext[0]))),
        "--load-cases".into(),
        s(&dir.join(format!("load-cases.{}", ext[1]))),
        "--rules".into(),
        s(&dir.join(format!("clearance-rules.{}", ext[2]))),
        "--terrain".into(),
        s(&root().join("examples/synthetic-line/synthetic-survey.csv")),
    ];
    for (flag, name) in [
        ("--spans-csv", "spans"),
        ("--csv", "wires"),
        ("--loads-csv", "loads"),
        ("--usage-csv", "usage"),
        ("--clearance-csv", "clearances"),
    ] {
        args.push(flag.into());
        args.push(s(&out.join(format!("{name}.csv"))));
    }
    ohl(&args);
}

fn same_outputs(a: &Path, b: &Path) {
    for name in OUTPUTS {
        let f = format!("{name}.csv");
        let (x, y) = (
            std::fs::read_to_string(a.join(&f)).unwrap(),
            std::fs::read_to_string(b.join(&f)).unwrap(),
        );
        assert!(x.lines().count() > 1, "{f}: no rows");
        assert_eq!(x, y, "{f} differs");
    }
}

#[test]
fn older_line_files_and_libraries_give_the_same_results() {
    let ex = root().join("examples/synthetic-line");
    for name in ["synthetic-line", "synthetic-line-typed"] {
        let (new, was) = (temp(&format!("{name}-new")), temp(&format!("{name}-old")));
        line(
            &ex.join(format!("{name}.ohl.json")),
            &root().join("libraries"),
            ["ohlwc", "ohllc", "ohlr"],
            &new,
        );
        line(
            &old().join(format!("synthetic-line/{name}.ols.json")),
            &old().join("libraries"),
            ["olswc", "olslc", "olsr"],
            &was,
        );
        same_outputs(&new, &was);
        std::fs::remove_dir_all(&new).unwrap();
        std::fs::remove_dir_all(&was).unwrap();
    }
}

/// A project file named `.ols` is a project still: imported into, solved
/// as the `.ohl` one.
#[test]
fn an_ols_project_is_a_project() {
    let dir = temp("project");
    let typed = old().join("synthetic-line/synthetic-line-typed.ols.json");
    let mut outs = Vec::new();
    for ext in ["ols", "ohl"] {
        let project = dir.join(format!("p.{ext}"));
        ohl(&[
            "project".into(),
            "import".into(),
            s(&typed),
            "--into".into(),
            s(&project),
            "--user".into(),
            "ana".into(),
        ]);
        let out = dir.join(ext);
        std::fs::create_dir_all(&out).unwrap();
        ohl(&[
            "line".into(),
            s(&project),
            "--structures".into(),
            s(&old().join("synthetic-line/Structures")),
            "--library".into(),
            s(&old().join("libraries/cables")),
            "--insulators".into(),
            s(&old().join("libraries/insulators")),
            "--weather".into(),
            s(&old().join("synthetic-line/weather-cases.olswc")),
            "--spans-csv".into(),
            s(&out.join("spans.csv")),
        ]);
        outs.push(std::fs::read_to_string(out.join("spans.csv")).unwrap());
    }
    assert_eq!(outs[0], outs[1]);
    // The line written out of it has the current id.
    let back = dir.join("back.ohl.json");
    ohl(&[
        "project".into(),
        "export".into(),
        s(&dir.join("p.ols")),
        "--out".into(),
        s(&back),
    ]);
    let text = std::fs::read_to_string(&back).unwrap();
    assert!(text.contains("\"format\": \"ohl-line\""), "{text}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The bill of materials with an older cost file, the galloping ellipses
/// with an older parameter file: the same as with the current ones.
#[test]
fn older_cost_and_galloping_files_give_the_same_results() {
    let ex = root().join("examples/synthetic-line");
    let o = old().join("synthetic-line");
    let lib = |l: &Path| {
        vec![
            "--library".to_owned(),
            s(&l.join("cables")),
            "--insulators".to_owned(),
            s(&l.join("insulators")),
        ]
    };
    let bom = |line: PathBuf, cost: PathBuf, l: &Path| {
        let mut a = vec!["bom".to_owned(), s(&line), "--cost".into(), s(&cost)];
        a.extend(lib(l));
        String::from_utf8_lossy(&ohl(&a).stdout).into_owned()
    };
    assert_eq!(
        bom(
            ex.join("synthetic-line-typed.ohl.json"),
            ex.join("unit-costs.ohlcost"),
            &root().join("libraries")
        ),
        bom(
            o.join("synthetic-line-typed.ols.json"),
            o.join("unit-costs.olscost"),
            &old().join("libraries")
        )
    );
    let dir = temp("galloping");
    let gallop = |d: &Path, line: &str, w: &str, g: &str, l: &Path, csv: &str| {
        let mut a = vec![
            "clearance".to_owned(),
            "galloping".into(),
            s(&d.join(line)),
            "--weather".into(),
            s(&d.join(w)),
            "--params".into(),
            s(&d.join(g)),
            "--csv".into(),
            s(&dir.join(csv)),
        ];
        a.extend(lib(l));
        ohl(&a);
        std::fs::read_to_string(dir.join(csv)).unwrap()
    };
    let new = gallop(
        &ex,
        "synthetic-line.ohl.json",
        "weather-cases.ohlwc",
        "galloping-example.ohlg",
        &root().join("libraries"),
        "new.csv",
    );
    let was = gallop(
        &o,
        "synthetic-line.ols.json",
        "weather-cases.olswc",
        "galloping-example.olsg",
        &old().join("libraries"),
        "old.csv",
    );
    assert!(new.lines().count() > 1);
    assert_eq!(new, was);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Each older library file reads, and as its current counterpart.
#[test]
fn older_library_files_read() {
    let lib = old().join("libraries");
    let ex = old().join("synthetic-line");
    let read = |p: PathBuf| std::fs::read_to_string(&p).unwrap();
    let now = |p: &str| read(root().join(p));
    // Line files: the same line (the id read is written as the current).
    let a = ohl_line::Line::load(&ex.join("synthetic-line.ols.json")).unwrap();
    let b = ohl_line::Line::load(&root().join("examples/synthetic-line/synthetic-line.ohl.json"))
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.format, "ohl-line");
    assert_eq!(
        ohl_line::ObjectLibrary::parse(&read(lib.join("objects/templates.olsload"))).unwrap(),
        ohl_line::ObjectLibrary::parse(&now("libraries/objects/templates.ohlload")).unwrap()
    );
    let classes = |t: &str| ohl_lidar::ClassScheme::parse(t).unwrap().to_toml();
    assert_eq!(
        classes(&read(lib.join("lidar-classes/asprs-las-1.4.olsc"))),
        classes(&now("libraries/lidar-classes/asprs-las-1.4.ohlc"))
    );
    assert!(
        ohl_structures::templates::PylonTemplate::parse(
            Path::new("delta-1c.olspt"),
            &read(lib.join("pylon-templates/delta-1c.olspt"))
        )
        .is_ok()
    );
    assert!(
        ohl_sections::SectionLibrary::load(&lib.join("sections/example-sections.olssec")).is_ok()
    );
    assert!(
        ohl_sections::SectionLibrary::load(&lib.join("sections")).is_ok_and(|l| {
            l == ohl_sections::SectionLibrary::load(&root().join("libraries/sections")).unwrap()
        })
    );
    assert!(ohl_thermal::ambient::AmbientCases::load(&ex.join("thermal-ambient.olsta")).is_ok());
    // Folders of either name: the libraries of a folder list both.
    let structures =
        ohl_structures::Library::<ohl_structures::StructureType>::load_dir(&ex.join("Structures"))
            .unwrap();
    assert_eq!(structures.len(), 6);
    let cables = ohl_cables::Library::load_dir(&lib.join("cables")).unwrap();
    assert_eq!(
        cables.len(),
        ohl_cables::Library::load_dir(&root().join("libraries/cables"))
            .unwrap()
            .len()
    );
    assert_eq!(
        ohl_structures_fe::pole::load_poles(&ex.join("Structures")).unwrap(),
        ohl_structures_fe::pole::load_poles(&root().join("examples/synthetic-line/Structures"))
            .unwrap()
    );
}
