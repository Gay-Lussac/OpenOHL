// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl structures fe` with each capacity method (2026-10-27), on an
//! invented bar 1.5 m long, 10 cm², r_v 1.414 cm, f_y 355 MPa, pushed by
//! 20 kN: ASCE 10-97 by default (no design data: both ends eccentric
//! assumed, KL/r 113, 162.2 kN, said), the simple check (Euler 184.2 kN,
//! said unsafe), and a plugin's method (the test plugin's invented
//! area × yield, 355 kN) when the plugin is built.

use std::path::Path;
use std::process::Command;

const MODEL: &str = r#"[analysis]
nonlinear = false
[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "355 MPa"
[[section]]
id = "s"
material = "steel"
area = "10 cm2"
iy = "50 cm4"
iz = "50 cm4"
i_min = "20 cm4"
[[node]]
id = "a"
point = ["0 m", "0 m", "0 m"]
support = "pinned"
[[node]]
id = "b"
point = ["1.5 m", "0 m", "0 m"]
support = ["y", "z"]
[[member]]
id = "m"
nodes = ["a", "b"]
section = "s"
[[load]]
node = "b"
force = ["-20 kN", "0 kN", "0 kN"]
"#;

fn run(name: &str, extra: &[&str]) -> String {
    let dir = std::env::temp_dir().join(format!("ohl-fe-cap-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("bar.ohlfe");
    std::fs::write(&file, MODEL).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .args(["structures", "fe"])
        .arg(&file)
        .args(extra)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "{stdout}\n{stderr}");
    stdout
}

#[test]
fn each_capacity_method_is_used_and_said() {
    let asce = run("asce", &[]);
    assert!(asce.contains("member capacities: ASCE 10-97"), "{asce}");
    assert!(
        asce.contains("compression 20.0 kN of 162.2 kN (KL/r 113) [assumed: length ratios 1, both ends eccentric"),
        "{asce}"
    );
    let simple = run("simple", &["--capacity", "simple"]);
    assert!(
        simple.contains("not a design code; unsafe compared with design codes"),
        "{simple}"
    );
    assert!(simple.contains("of 184.2 kN (buckling)"), "{simple}");
    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/tests/member-capacity");
    if plugin
        .join("target/wasm32-wasip2/release/member_capacity.wasm")
        .is_file()
    {
        let given = run(
            "plugin",
            &[
                "--capacity",
                "area-yield",
                "--capacity-plugin",
                plugin.to_str().unwrap(),
            ],
        );
        assert!(given.contains("method area-yield of the plugin"), "{given}");
        assert!(given.contains("of 355.0 kN (area × yield)"), "{given}");
    } else {
        eprintln!("skipping the plugin: tests/member-capacity not built");
    }
}
