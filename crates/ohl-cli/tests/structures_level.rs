// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structures in the wire solve on the command line (plan of
//! 2026-11-15, phase 5): `--structures-as linear` over a rigid line, the
//! line's own `solver.structures` taken without it and overridden by
//! `--structures-as rigid`; `ohl tensions` at level 3 prints the level's
//! header and both solves' tensions with the same rows; full structures
//! (level 4) the same with level 4's header; `ohl line
//! --calculation-level 3` and `4` work (level 4 printing its passes and
//! the change from level 3); a check that does not take the level says
//! it stays rigid.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ex() -> PathBuf {
    root().join("examples/synthetic-line")
}

/// `ohl tensions` on `line` with the example's weather and tension rules,
/// the example's structure types, and `extra` arguments.
fn tensions(line: &Path, extra: &[&str]) -> Output {
    tensions_by(line, &ex().join("tension-rules.ohlr"), extra)
}

/// [`tensions`] by the rule file `rules`.
fn tensions_by(line: &Path, rules: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("tensions")
        .arg(line)
        .arg("--weather")
        .arg(ex().join("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(rules)
        .arg("--structures")
        .arg(ex().join("Structures"))
        .args(extra)
        .output()
        .unwrap()
}

/// The table's rows (rule, case, section, span …), the level's words cut
/// off.
fn rows(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.ends_with("pass") || l.ends_with("FAIL") || l.ends_with("kN)"))
        .map(|l| l.split("  (rigid").next().unwrap().to_owned())
        .collect()
}

#[test]
fn ohl_tensions_takes_the_lines_level_and_the_run_s() {
    let line = ex().join("synthetic-line-typed.ohl.json");
    let rigid = tensions(&line, &[]);
    let rigid_text = String::from_utf8_lossy(&rigid.stdout).into_owned();
    assert!(!rigid_text.contains("calculation level 3"), "{rigid_text}");
    let linear = tensions(&line, &["--structures-as", "linear"]);
    let text = String::from_utf8_lossy(&linear.stdout).into_owned();
    assert_eq!(linear.status.code(), rigid.status.code(), "{text}");
    for said in [
        "calculation level 3 (in active development)",
        "never less safe than rigid",
        "structure(s) flexible",
        "(rigid ",
        " / level 3 ",
    ] {
        assert!(text.contains(said), "{said}: {text}");
    }
    // The same rows (rule, case, section, span), each checked once.
    let (a, b) = (rows(&rigid_text), rows(&text));
    assert_eq!(a.len(), b.len(), "{text}");

    // The line's own setting, and the run's over it.
    let dir = std::env::temp_dir().join(format!("ohl-structures-level-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&line).unwrap()).unwrap();
    v["solver"]["structures"] = "linear".into();
    let own = dir.join("linear.ohl.json");
    std::fs::write(&own, v.to_string()).unwrap();
    let text = String::from_utf8_lossy(&tensions(&own, &[]).stdout).into_owned();
    assert!(
        text.contains("calculation level 3 (in active development)"),
        "{text}"
    );
    let text =
        String::from_utf8_lossy(&tensions(&own, &["--structures-as", "rigid"]).stdout).into_owned();
    assert!(!text.contains("calculation level 3"), "{text}");

    // A rule by the ruling span on sections of finite elements: solved
    // rigid, said.
    let rules = dir.join("rs.ohlr");
    std::fs::write(
        &rules,
        "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"test\"\n\n\
         [[tension_rule]]\nname = \"rs\"\ncases = [\"everyday\"]\ncalculation = \"initial-rs\"\nfactor = 0.9\n",
    )
    .unwrap();
    let text =
        String::from_utf8_lossy(&tensions_by(&line, &rules, &["--structures-as", "linear"]).stdout)
            .into_owned();
    assert!(
        text.contains("solved rigid (calculation level 2)"),
        "{text}"
    );

    // Full structures (level 4): the same rows, level 4's header.
    let full = tensions(&line, &["--structures-as", "full"]);
    let text = String::from_utf8_lossy(&full.stdout).into_owned();
    assert_eq!(full.status.code(), rigid.status.code(), "{text}");
    for said in [
        "calculation level 4 (in active development)",
        "never less safe than rigid",
        " / level 4 ",
    ] {
        assert!(text.contains(said), "{said}: {text}");
    }
    assert_eq!(rows(&text).len(), a.len(), "{text}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ohl_line_at_calculation_level_four() {
    let line = ex().join("synthetic-line-typed.ohl.json");
    for args in [
        &["--calculation-level", "4"][..],
        &["--structures-as", "full"][..],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
            .arg("line")
            .arg(&line)
            .arg("--temperature=5")
            .arg("--wind-pressure=450")
            .args(args)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{args:?}: {text}");
        for said in [
            "calculation level 4: full structures",
            "structures flexible",
            "level 4: ",
            "against level 3: the points' moves differ by up to",
        ] {
            assert!(text.contains(said), "{said}: {text}");
        }
        assert!(!text.contains("FAILED"), "{text}");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("line")
        .arg(&line)
        .arg("--calculation-level")
        .arg("5")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("give 2, 3 or 4"));
}

#[test]
fn ohl_line_keeps_the_calculation_level_flag_and_others_say_rigid() {
    let line = ex().join("synthetic-line-typed.ohl.json");
    for args in [
        &["--calculation-level", "3"][..],
        &["--structures-as", "linear"][..],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
            .arg("line")
            .arg(&line)
            .arg("--temperature=-20")
            .arg("--wind-pressure=500")
            .args(args)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{args:?}: {text}");
        assert!(
            text.contains("calculation level 3: structures as a linear stiffness"),
            "{text}"
        );
        assert!(text.contains("structures flexible"), "{text}");
    }
    // A check that does not take the level yet: said, rigid.
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("ice-shedding")
        .arg(&line)
        .arg("--weather")
        .arg(ex().join("weather-cases.ohlwc"))
        .arg("--rules")
        .arg(ex().join("ice-shedding-rules.ohlr"))
        .arg("--structures-as")
        .arg("linear")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains(
            "calculation level 2 (structures rigid): this check does not take the line's level yet"
        ),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
