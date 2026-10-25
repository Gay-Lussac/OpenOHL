// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! From the command line: a load case with a broken wire (roadmap O9) in
//! `ohl line --load-cases` — the run names it, the loads CSV says which
//! span gave each component; local winds per span (roadmap O15) set with
//! `ohl project apply` (`set_wind_factor`), exported in the line file and
//! taken by `ohl line` in a wind case.

use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line")
        .join(name)
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-cli-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ohl(args: &[&std::ffi::OsStr]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_broken_wire_load_case_from_the_command_line() {
    let d = dir("broken-wire");
    let cases = d.join("broken.ohllc");
    std::fs::write(
        &cases,
        "format = \"ohl-load-cases\"\nversion = 3\n\
         [[case]]\nname = \"S1 span 2 broken\"\nweather = \"everyday\"\nlongitudinal = 1.2\n\
         [case.broken]\nsection = \"S1\"\nspan = 2\n\
         [[case]]\nname = \"S1 every span\"\nweather = \"everyday\"\n\
         [case.broken]\nsection = \"S1\"\nspan = \"every\"\n",
    )
    .unwrap();
    let csv = d.join("loads.csv");
    let (ok, stdout, stderr) = ohl(&[
        "line".as_ref(),
        example("synthetic-line.ohl.json").as_os_str(),
        "--weather".as_ref(),
        example("weather-cases.ohlwc").as_os_str(),
        "--load-cases".as_ref(),
        cases.as_os_str(),
        "--loads-csv".as_ref(),
        csv.as_os_str(),
    ]);
    assert!(ok, "{stdout}\n{stderr}");
    assert!(stdout.contains("broken wire: S1 span 2 broken"), "{stdout}");
    assert!(
        stdout.contains("broken wire: S1 every span broken, one at a time"),
        "{stdout}"
    );
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.lines().next().unwrap().ends_with(",broken_span"));
    assert!(
        text.lines()
            .any(|l| l.starts_with("S1 span 2 broken,") && l.ends_with(",2 2 2")),
        "{text}"
    );
    // Every span: each row names a span of S1 (1 to 4) per component.
    let every: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("S1 every span,"))
        .collect();
    assert!(!every.is_empty());
    for l in every {
        let spans = l.rsplit(',').next().unwrap();
        assert!(
            spans
                .split(' ')
                .all(|k| (1..=4).contains(&k.parse::<u32>().unwrap())),
            "{l}"
        );
    }
    // A span the section does not have: the case fails, saying so.
    std::fs::write(
        &cases,
        "[[case]]\nname = \"bad\"\nweather = \"everyday\"\n\
         [case.broken]\nsection = \"S1\"\nspan = 9\n",
    )
    .unwrap();
    let (ok, stdout, stderr) = ohl(&[
        "line".as_ref(),
        example("synthetic-line.ohl.json").as_os_str(),
        "--weather".as_ref(),
        example("weather-cases.ohlwc").as_os_str(),
        "--load-cases".as_ref(),
        cases.as_os_str(),
    ]);
    assert!(!ok, "{stdout}");
    assert!(stderr.contains("has no span 9"), "{stderr}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn local_winds_from_the_command_line() {
    let d = dir("local-wind");
    let project = d.join("p.ohl");
    let (ok, out, err) = ohl(&[
        "project".as_ref(),
        "import".as_ref(),
        example("synthetic-line.ohl.json").as_os_str(),
        "--into".as_ref(),
        project.as_os_str(),
        "--user".as_ref(),
        "ana".as_ref(),
    ]);
    assert!(ok, "{out}\n{err}");
    let (ok, out, err) = ohl(&[
        "project".as_ref(),
        "apply".as_ref(),
        project.as_os_str(),
        "--user".as_ref(),
        "ana".as_ref(),
        r#"{"op": "set_wind_factor", "section": "S1", "spans": [1, 2], "factor": 1.3}"#.as_ref(),
    ]);
    assert!(ok, "{out}\n{err}");
    let line = d.join("windy.ohl.json");
    let (ok, out, err) = ohl(&[
        "project".as_ref(),
        "export".as_ref(),
        project.as_os_str(),
        "--out".as_ref(),
        line.as_os_str(),
    ]);
    assert!(ok, "{out}\n{err}");
    let text = std::fs::read_to_string(&line).unwrap();
    assert!(text.contains("\"version\": 9,"), "{}", &text[..120]);
    assert!(text.contains("\"wind_factors\""));
    let (ok, out, err) = ohl(&[
        "line".as_ref(),
        line.as_os_str(),
        "--wind-pressure".as_ref(),
        "500".as_ref(),
    ]);
    assert!(ok, "{out}\n{err}");
    assert!(
        out.contains("local winds of S1 (factor by span): 2–3: 1.3"),
        "{out}"
    );
    // Out of range: refused, the project unchanged.
    let (ok, _, err) = ohl(&[
        "project".as_ref(),
        "apply".as_ref(),
        project.as_os_str(),
        "--user".as_ref(),
        "ana".as_ref(),
        r#"{"op": "set_wind_factor", "section": "S1", "spans": [0, 9], "factor": 1.3}"#.as_ref(),
    ]);
    assert!(!ok);
    assert!(err.contains("spans 1 to 4"), "{err}");
    std::fs::remove_dir_all(&d).ok();
}
