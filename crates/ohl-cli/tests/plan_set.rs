// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl drawing --set` (roadmap O14): a plan set written on the example
//! line in one go — the sheets, two situation plans (the second named
//! apart) and the phase plan, the line solved once — and with `--set-pdf`
//! every sheet of them in one PDF.

use std::path::{Path, PathBuf};
use std::process::Command;

fn examples(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

/// The number of pages of PDF `path` (its page objects).
fn pages(path: &Path) -> usize {
    let bytes = std::fs::read(path).unwrap();
    let text: String = bytes
        .iter()
        .map(|&b| if b < 128 { b as char } else { '.' })
        .collect();
    text.matches("/Type /Page ").count() + text.matches("/Type /Page>").count()
}

#[test]
fn a_plan_set_is_written_with_its_one_pdf() {
    let dir = std::env::temp_dir().join(format!("ohl-plan-set-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let set = dir.join("submission.ohlset");
    std::fs::write(
        &set,
        "format = \"ohl-plan-set\"\nversion = 1\nname = \"Submission\"\n\
         [[drawing]]\nkind = \"sheets\"\ntemplate = \"default\"\npage = \"A3\"\n\
         scale = 2000\nvertical_scale = 500\n\
         [[drawing]]\nkind = \"situation\"\nscale = 1000\n\
         [[drawing]]\nkind = \"situation\"\npage = \"A4\"\nscale = 2000\n\
         [[drawing]]\nkind = \"phase\"\n",
    )
    .unwrap();
    let out = dir.join("out");
    let run = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("drawing")
        .arg(examples("synthetic-line/synthetic-line.ohl.json"))
        .arg("--terrain")
        .arg(examples("synthetic-line/synthetic-survey.csv"))
        .arg("--weather")
        .arg(examples("synthetic-line/weather-cases.ohlwc"))
        .arg("--route")
        .arg("main")
        .arg("--format")
        .arg("pdf")
        .arg("--set")
        .arg(&set)
        .arg("--set-pdf")
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stdout}\n{stderr}");
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for n in [
        "main.pdf",
        "main-situation-plan.pdf",
        "main-situation-plan-2.pdf",
        "main-phase-plan.pdf",
        "Submission.pdf",
    ] {
        assert!(names.iter().any(|x| x == n), "{n} not in {names:?}");
    }
    // The plans write no plan-and-profile sheets of their own.
    assert_eq!(names.len(), 5, "{names:?}");
    // The one PDF: every page of the others, in the set's order.
    let parts: usize = names
        .iter()
        .filter(|n| *n != "Submission.pdf")
        .map(|n| pages(&out.join(n)))
        .sum();
    assert!(parts >= 4, "{names:?}");
    assert_eq!(pages(&out.join("Submission.pdf")), parts, "{stdout}");
    assert!(stdout.contains("pages in one PDF"), "{stdout}");
    // The line solved once for the whole set.
    assert_eq!(
        stdout.matches("solved").count(),
        {
            let single = Command::new(env!("CARGO_BIN_EXE_ohl"))
                .arg("line")
                .arg(examples("synthetic-line/synthetic-line.ohl.json"))
                .arg("--terrain")
                .arg(examples("synthetic-line/synthetic-survey.csv"))
                .arg("--weather")
                .arg(examples("synthetic-line/weather-cases.ohlwc"))
                .output()
                .unwrap();
            String::from_utf8_lossy(&single.stdout)
                .matches("solved")
                .count()
        },
        "{stdout}"
    );
    // --set-pdf needs --set; an unknown plugin set is refused, named.
    let bad = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("drawing")
        .arg(examples("synthetic-line/synthetic-line.ohl.json"))
        .arg("--set")
        .arg("noplugin/noset")
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("plan set \"noplugin/noset\""));
    std::fs::remove_dir_all(&dir).ok();
}
