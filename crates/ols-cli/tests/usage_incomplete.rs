// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ohl line --usage-csv`: a section that does not solve (its cable not in
//! the library) leaves its structures' usage incomplete — the CSV's
//! `incomplete` column says which section and why, the other structures'
//! rows leave it empty; the run says so (and fails for the section).

use std::path::{Path, PathBuf};
use std::process::Command;

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line")
        .join(name)
}

#[test]
fn usage_csv_marks_the_structures_of_a_section_not_solved() {
    let dir = std::env::temp_dir().join(format!("ols-usage-incomplete-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let mut line = ols_line::Line::load(&example("synthetic-line-typed.ohl.json")).unwrap();
    for s in line.sections.iter_mut().filter(|s| s.id == "S3") {
        s.cable = "no-such-cable".into();
    }
    let file = dir.join("broken.ohl.json");
    std::fs::write(&file, line.to_json()).unwrap();
    let csv = dir.join("usage.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("line")
        .arg(&file)
        .arg("--structures")
        .arg(example("Structures"))
        .arg("--weather")
        .arg(example("weather-cases.ohlwc"))
        .arg("--load-cases")
        .arg(example("load-cases.ohllc"))
        .arg("--usage-csv")
        .arg(&csv)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The run fails for the section not solved, after writing the CSV.
    assert!(
        !out.status.success() && stderr.contains("sections or jumpers failed"),
        "{stdout}\n{stderr}"
    );
    assert!(stdout.contains("structure usage incomplete"), "{stdout}");
    let text = std::fs::read_to_string(&csv).unwrap();
    let header = text.lines().next().unwrap();
    assert!(header.ends_with(",notes,incomplete"), "{header}");
    let row = |case: &str, s: &str| {
        text.lines()
            .find(|l| l.starts_with(&format!("{case},{s},")))
            .unwrap_or_else(|| panic!("{case} {s}: {text}"))
            .to_owned()
    };
    for s in ["T3", "T6", "T7"] {
        let r = row("everyday", s);
        assert!(r.contains("incomplete: section S3 not solved ("), "{r}");
    }
    for s in ["T1", "T2", "T4", "T5"] {
        let r = row("everyday", s);
        assert!(!r.contains("incomplete:"), "{r}");
    }
    std::fs::remove_dir_all(&dir).ok();
}
