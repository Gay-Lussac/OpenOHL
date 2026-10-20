// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl plugin info` on the in-repo plugins of every world (when built:
//! `cargo build --release --target wasm32-wasip2` in each; with
//! `OHL_REQUIRE_PLUGIN_TESTS` set an unbuilt plugin fails the test).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The plugin's directory when its WebAssembly file is built, `None` to
/// skip it.
fn built(relative: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(relative);
    let m = ohl_plugins::Manifest::load(&dir).unwrap();
    if m.entry_path().is_file() {
        return Some(dir);
    }
    assert!(
        ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugin not built: {}",
        m.entry_path().display()
    );
    eprintln!("skipping: {relative} not built");
    None
}

#[test]
fn plugin_info_knows_every_world() {
    // (plugin, its world, a line of its details)
    let cases = [
        ("examples/hello-wasm", "plugin", ""),
        ("examples/geojson", "io-plugin", "format geojson:"),
        (
            "examples/esri-grid",
            "terrain-plugin",
            "format esri-ascii-grid:",
        ),
        (
            "examples/project-bundle",
            "project-plugin",
            "format ohl-bundle:",
        ),
        ("codes/en50341", "code-plugin", "parameter template:"),
        ("codes/iec60826", "code-plugin", "parameter template:"),
        ("codes/nesc", "code-plugin", "parameter template:"),
        (
            "tests/code-presets",
            "code-plugin",
            "optional exports: structure-wind, rule-presets",
        ),
        ("tests/code-old", "code-plugin", "parameter template:"),
    ];
    for (relative, world, detail) in cases {
        let Some(dir) = built(relative) else {
            continue;
        };
        let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
            .args(["plugin", "info"])
            .arg(&dir)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{relative}: {stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let name = ohl_plugins::Manifest::load(&dir).unwrap().plugin.name;
        assert!(
            stdout.contains(&format!("name         {name}\n")),
            "{stdout}"
        );
        assert!(
            stdout.contains(&format!("world        {world}\n")),
            "{stdout}"
        );
        assert!(stdout.contains(detail), "{relative}: {stdout}");
    }
}

/// `ohl code rule-presets` lists a plugin's presets of clearance rules
/// with their distances and writes them with `--out`; `ohl code generate`
/// writes them beside the weather cases.
#[test]
fn code_rule_presets_are_listed_and_written() {
    let Some(dir) = built("tests/code-presets") else {
        return;
    };
    let out_dir = std::env::temp_dir().join(format!("ohl-rule-presets-cli-{}", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["code", "rule-presets", "--plugin"])
        .arg(&dir)
        .arg("--out")
        .arg(&out_dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("ground — Ground 6 m"), "{stdout}");
    assert!(stdout.contains("vegetation: classes 42, 43"), "{stdout}");
    assert!(
        stdout.contains("Test ground: vertical 6.00 m + 0.01 m/kV above 0 kV"),
        "{stdout}"
    );
    assert!(out_dir.join("code-presets-rules-ground.ohlr").is_file());
    assert!(out_dir.join("code-presets-rules-ground.ohlwc").is_file());
    let params = out_dir.join("p.toml");
    std::fs::write(&params, "").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["code", "generate", "--plugin"])
        .arg(&dir)
        .arg("--params")
        .arg(&params)
        .arg("--weather")
        .arg(out_dir.join("g/w.ohlwc"))
        .arg("--load-cases")
        .arg(out_dir.join("g/l.ohllc"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("rule preset →"), "{stdout}");
    assert!(out_dir.join("g/code-presets-rules-ground.ohlr").is_file());
    std::fs::remove_dir_all(&out_dir).unwrap();
}
