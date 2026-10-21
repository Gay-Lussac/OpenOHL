// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `--circuit` / `--circuits` with circuits defined in the line file
//! (2026-10-21): taken by id or by name, an unknown one refused. The
//! synthetic example line (invented values) with its circuit *Example 1*
//! renamed *North*, id `c-north`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn circuits_are_chosen_by_id_or_name() {
    let dir = std::env::temp_dir().join(format!("ohl-circuits-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let ex = root().join("examples/synthetic-line");
    let text = std::fs::read_to_string(ex.join("synthetic-line.ohl.json"))
        .unwrap()
        .replace(
            "\"circuit_name\": \"Example 1\"",
            "\"circuit_name\": \"North\"",
        )
        .replacen(
            '{',
            "{\"circuits\": [{\"id\": \"c-north\", \"name\": \"North\", \"description\": \"invented\"}],",
            1,
        );
    let line = dir.join("line.ohl.json");
    std::fs::write(&line, text).unwrap();
    let run = |circuit: &str| {
        Command::new(env!("CARGO_BIN_EXE_ohl"))
            .arg("safety-volume")
            .arg(&line)
            .arg("--weather")
            .arg(ex.join("weather-cases.ohlwc"))
            .args(["--cases", "max temperature", "--distance", "3"])
            .args(["--tolerance", "0.3", "--per", "circuit", "--format", "dxf"])
            .args(["--circuit", circuit])
            .arg("--out")
            .arg(&dir)
            .output()
            .unwrap()
    };
    for given in ["c-north", "North"] {
        let out = run(given);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{text}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(text.contains("North: 220 kV — d 3.00 m"), "{text}");
        assert!(!text.contains("Branch:"), "{text}");
        assert_eq!(text.matches(" triangles, ").count(), 1, "{text}");
    }
    // The branch (read into a circuit of its own, id its name).
    let out = run("Branch");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Branch: "));
    // Unknown: refused, the circuits listed.
    let out = run("Example 1");
    assert!(!out.status.success());
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(e.contains("no circuit \"Example 1\""), "{e}");
    assert!(e.contains("c-north (North), Branch"), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}
