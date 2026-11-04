// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl thermal clearance-check` (2026-11-04) as a user types it: the
//! current given, the temperature per section in the hottest ambient case,
//! pass or fail per section; the clearance-limit search's options still
//! read after the arguments were shared.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn thermal(args: &[&str]) -> (bool, String) {
    let ex = root().join("examples/synthetic-line");
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .current_dir(&ex)
        .arg("thermal")
        .args(args)
        .args([
            "synthetic-line.ohl.json",
            "--terrain",
            "synthetic-survey.csv",
            "--clearance",
            "5",
            "--weather",
            "weather-cases.ohlwc",
            "--case",
            "max temperature",
            "--ambient",
            "thermal-ambient.ohlta",
            "--method",
            "cigre601",
        ])
        .arg("--library")
        .arg(root().join("libraries/cables"))
        .output()
        .unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn clearance_check_from_the_command_line() {
    let (ok, text) = thermal(&["clearance-check", "--current", "1500"]);
    assert!(ok, "{text}");
    assert!(
        text.contains("clearance at the current of 3 sections"),
        "{text}"
    );
    assert!(text.contains("FAIL"), "{text}");
    assert!(text.contains("1500 A"), "{text}");
    let (ok, text) = thermal(&["clearance-check", "--current", "100"]);
    assert!(ok, "{text}");
    assert!(text.contains("3 pass, 0 fail"), "{text}");
    // Without a current (the example's systems give none): refused.
    let (ok, text) = thermal(&["clearance-check"]);
    assert!(!ok);
    assert!(text.contains("--current"), "{text}");
    // The search still takes its options.
    let (ok, text) = thermal(&["clearance-limit", "--max-temp", "100", "--step", "20"]);
    assert!(ok, "{text}");
    assert!(text.contains("(0–100 °C"), "{text}");
}
