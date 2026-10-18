// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `ohl plugin info` on the in-repo plugins of every world (when built:
//! `cargo build --release --target wasm32-wasip2` in each; with
//! `OLS_REQUIRE_PLUGIN_TESTS` set an unbuilt plugin fails the test).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The plugin's directory when its WebAssembly file is built, `None` to
/// skip it.
fn built(relative: &str) -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(relative);
    let m = ols_plugins::Manifest::load(&dir).unwrap();
    if m.entry_path().is_file() {
        return Some(dir);
    }
    assert!(
        std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
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
            "format ols-bundle:",
        ),
        ("codes/en50341", "code-plugin", "parameter template:"),
        ("codes/iec60826", "code-plugin", "parameter template:"),
        ("codes/nesc", "code-plugin", "parameter template:"),
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
        let name = ols_plugins::Manifest::load(&dir).unwrap().plugin.name;
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
