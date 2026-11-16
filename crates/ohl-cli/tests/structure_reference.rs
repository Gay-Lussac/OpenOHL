// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The reference state of the structures on the command line (plan of
//! 2026-11-15, phase 5b): `ohl line --solve-reference` solves and saves
//! one; `ohl tensions` on the saved line takes the structures as
//! deflected there (said), `--structure-reference ignore` the structures
//! as drawn (said, the line's own results); an edited line's state is out
//! of date and not used (said).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ex() -> PathBuf {
    root().join("examples/synthetic-line")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// `ohl tensions` on `line` with the example's weather, rules and types.
fn tensions(line: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("tensions")
        .arg(line)
        .arg("--weather")
        .arg(ex().join("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(ex().join("tension-rules.ohlr"))
        .arg("--structures")
        .arg(ex().join("Structures"))
        .args(extra)
        .output()
        .unwrap()
}

/// The table's rows.
fn rows(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.ends_with("pass") || l.ends_with("FAIL"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn solved_saved_used_ignored_and_stale() {
    let dir = std::env::temp_dir().join(format!("ohl-structure-reference-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let saved = dir.join("with-reference.ohl.json");
    let solve = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("line")
        .arg(ex().join("synthetic-line-typed.ohl.json"))
        .arg("--weather")
        .arg(ex().join("weather-cases.ohlwc"))
        .arg("--case")
        .arg("everyday")
        .arg("--structures")
        .arg(ex().join("Structures"))
        .arg("--solve-reference")
        .arg("high wind")
        .arg("--save-reference")
        .arg(&saved)
        .output()
        .unwrap();
    let said = text(&solve);
    assert!(solve.status.success(), "{said}");
    for s in [
        "reference case \"high wind\" solved at calculation level 3",
        "T6: largest move",
        "T1 as drawn: ",
        "structures as deflected in case \"high wind\" (calculation level 3)",
    ] {
        assert!(said.contains(s), "{s}: {said}");
    }
    let file = std::fs::read_to_string(&saved).unwrap();
    assert!(file.contains("\"version\": 22"), "{}", &file[..200]);

    // Used by default, said; ignored on asking, said; the rows differ.
    let used = tensions(&saved, &[]);
    let used_text = text(&used);
    assert!(
        used_text.contains("structures as deflected in case \"high wind\""),
        "{used_text}"
    );
    let ignored = tensions(&saved, &["--structure-reference", "ignore"]);
    let ignored_text = text(&ignored);
    assert!(
        ignored_text.contains("structures as drawn (ignored by --structure-reference ignore)"),
        "{ignored_text}"
    );
    let plain = tensions(&ex().join("synthetic-line-typed.ohl.json"), &[]);
    assert_eq!(rows(&ignored_text), rows(&text(&plain)));
    assert_ne!(rows(&used_text), rows(&ignored_text), "{used_text}");

    // An edited line: out of date, not used, said.
    let mut v: serde_json::Value = serde_json::from_str(&file).unwrap();
    let o = &mut v["structures"][1]["orientation_deg"];
    *o = serde_json::json!(o.as_f64().unwrap_or(0.0) + 1.0);
    let edited = dir.join("edited.ohl.json");
    std::fs::write(&edited, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let stale = tensions(&edited, &[]);
    let stale_text = text(&stale);
    assert!(
        stale_text.contains("out of date (the line changed since it was solved)"),
        "{stale_text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
