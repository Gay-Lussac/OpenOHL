// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ohl line` and `ohl zones` on a project file read the libraries the
//! project keeps: a project whose wire library holds a cable the shipped
//! library does not (an invented id) solves without `--library`; an
//! explicit `--library` wins.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example(name: &str) -> PathBuf {
    root().join("examples/synthetic-line").join(name)
}

/// A cable id only in the project's own library.
const OWN_CABLE: &str = "invented-project-cable";

/// A vertical ground rule in the maximum temperature case.
const RULES: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "Project rules"

[[rule]]
name = "Ground"
target = "ground"
vertical = "7 m"
vertical_band = "2 m"
cases = ["max temperature"]

[wind]
steps = false

[check]
method = "rs"
"#;

struct Setup {
    dir: PathBuf,
    project: PathBuf,
}

impl Drop for Setup {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// A project of the synthetic line on the invented cable, with its own
/// wire library, rule file and weather case file.
fn setup(tag: &str) -> Setup {
    let stem = format!("libtest-{tag}-{}", std::process::id());
    let dir = std::env::temp_dir().join(&stem);
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let text = std::fs::read_to_string(example("synthetic-line.ohl.json")).unwrap();
    let line = ohl_line::Line::from_json(&text.replace("acsr-drake-26-7", OWN_CABLE)).unwrap();
    let wires = std::fs::read_to_string(root().join("libraries/cables/acsr-examples.ohlw"))
        .unwrap()
        .replace("acsr-drake-26-7", OWN_CABLE);
    let weather = std::fs::read_to_string(example("weather-cases.ohlwc")).unwrap();
    let project = dir.join(format!("{stem}.ohl"));
    let mut s = ohl_session::Session::open(&project).unwrap();
    s.import(&line, "tester").unwrap();
    s.keep_library_files(
        "tester",
        &[
            ("cables".into(), "own.ohlw".into(), Some(wires)),
            ("rules".into(), "project.ohlr".into(), Some(RULES.into())),
            ("weather".into(), "cases.ohlwc".into(), Some(weather)),
        ],
    )
    .unwrap();
    Setup { dir, project }
}

fn ohl(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn log_len(project: &Path) -> usize {
    ohl_session::Session::open(project)
        .unwrap()
        .log()
        .unwrap()
        .len()
}

/// No working folder of the project's libraries is left behind.
fn assert_cleaned(project: &Path) {
    let stem = project.file_stem().unwrap().to_string_lossy().into_owned();
    let prefix = format!("ohl-cli-libraries-{stem}-");
    let left: Vec<String> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&prefix))
        .collect();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[test]
fn line_reads_the_projects_libraries() {
    let s = setup("line");
    let project = s.project.display().to_string();
    let before = log_len(&s.project);
    let out = ohl(&s.dir, &["line", &project]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("cables: the project's (1 file)"),
        "{stdout}"
    );
    assert!(stdout.contains("insulators: "), "{stdout}");
    // The project's own files by name (the weather case file is not in
    // the current folder).
    let out = ohl(
        &s.dir,
        &[
            "line",
            &project,
            "--weather",
            "cases.ohlwc",
            "--case",
            "max*",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // An explicit library wins: the shipped one has no such cable.
    let shipped = root().join("libraries/cables").display().to_string();
    let out = ohl(&s.dir, &["line", &project, "--library", &shipped]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stdout}");
    assert!(stdout.contains("(--library)"), "{stdout}");
    assert!(
        format!("{stdout}{stderr}").contains(OWN_CABLE),
        "{stdout}\n{stderr}"
    );
    // Reading changes nothing in the project.
    assert_eq!(log_len(&s.project), before);
    assert_cleaned(&s.project);
}

#[test]
fn zones_reads_the_projects_libraries() {
    let s = setup("zones");
    let project = s.project.display().to_string();
    let survey = example("synthetic-survey.csv").display().to_string();
    let before = log_len(&s.project);
    // The rule file by name, the weather cases: the project's only file.
    let args = [
        "zones",
        &project,
        "--rules",
        "project.ohlr",
        "--terrain",
        &survey,
        "--no-cloud",
    ];
    let out = ohl(&s.dir, &args);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        matches!(out.status.code(), Some(0 | 1)),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("cables: the project's (1 file)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Project rules (project.ohlr (the project's))"),
        "{stdout}"
    );
    assert!(
        stdout.contains("weather: cases.ohlwc (the project's)"),
        "{stdout}"
    );
    assert!(!stdout.contains("not solved"), "{stdout}");
    // An explicit library wins: nothing solves with the shipped one.
    let shipped = root().join("libraries/cables").display().to_string();
    let mut with = args.to_vec();
    with.extend(["--library", &shipped]);
    let out = ohl(&s.dir, &with);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        format!("{stdout}{stderr}").contains(OWN_CABLE),
        "{stdout}\n{stderr}"
    );
    assert_eq!(log_len(&s.project), before);
    assert_cleaned(&s.project);
}

/// A project file that does not exist is not created.
#[test]
fn missing_project_is_not_created() {
    let dir = std::env::temp_dir().join(format!("libtest-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("none.ohl");
    let out = ohl(&dir, &["line", &project.display().to_string()]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no such project file"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!project.exists());
    std::fs::remove_dir_all(&dir).ok();
}
