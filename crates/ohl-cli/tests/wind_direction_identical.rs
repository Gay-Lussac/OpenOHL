// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The wind at any direction (owner 2026-11-15: "the current cases need
//! to be identical"): the example lines solved in every weather case and
//! load case without a direction — and an old (version 4) load case file
//! with the structure bodies' angles in the old convention — give the same
//! output, bit for bit, as the program before the change. The digests
//! below were taken from the program before the change (2026-11-15) with
//! these very inputs: the points, spans, structure loads and usage CSVs
//! (every number as written, to the last digit).
//!
//! Every value here is INVENTED (the example's own invented cases).
//!
//! To take the digests of another program (e.g. the one before a change):
//! `OHL_REGRESSION_BIN=/path/to/ohl OHL_REGRESSION_PRINT=1 cargo test -p
//! ohl-cli --test wind_direction_identical -- --nocapture`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The example's weather cases (a copy: the test keeps its own inputs).
const WEATHER: &str = r#"format = "ohl-weather-cases"
version = 1
name = "Synthetic example cases"

[[case]]
name = "everyday"
temperature = "10 °C"

[[case]]
name = "max temperature"
temperature = "80 °C"

[[case]]
name = "glaze ice"
temperature = "-5 °C"
ice_thickness = "12.7 mm"
ice_density = "913 kg/m³"

[[case]]
name = "rime ice + wind"
temperature = "-5 °C"
ice_load = "20 N/m"
ice_density = "500 kg/m³"
wind_pressure = "200 Pa"

[[case]]
name = "high wind"
temperature = "15 °C"
wind_pressure = "600 Pa"

[[case]]
name = "high wind from the left"
temperature = "15 °C"
wind_pressure = "600 Pa"
wind_from = "left"
"#;

/// Load cases of versions 1 and 4: factors, the wires' side, the bodies'
/// directions in the old convention (0° from the right, 90° from ahead).
const LOAD_CASES: &str = r#"format = "ohl-load-cases"
version = 4

[[case]]
name = "everyday"
weather = "everyday"

[[case]]
name = "wind"
weather = "high wind"
transverse = 1.4

[[case]]
name = "ice"
weather = "glaze ice"
condition = "creep"
vertical = 1.3
strength = 0.9

[[case]]
name = "wind left"
weather = "high wind"
wind_from = "left"
transverse = 1.4

[[case]]
name = "body 30"
weather = "high wind"
transverse = 1.3
longitudinal = 1.1
[case.structure]
direction = 30

[[case]]
name = "body 137.25"
weather = "rime ice + wind"
[case.structure]
direction = 137.25
wind_factor = 1.2

[[case]]
name = "body 0.1, wires from the left"
weather = "high wind from the left"
[case.structure]
direction = 0.1

[[case]]
name = "body ahead"
weather = "high wind"
[case.structure]
direction = "ahead"
"#;

/// FNV-1a, 64 bits.
fn fnv(bytes: &[u8], mut h: u64) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The digest of every CSV of `ohl line` on `line` with the cases above.
fn digest(line: &str, tag: &str) -> u64 {
    let bin = std::env::var("OHL_REGRESSION_BIN").unwrap_or(env!("CARGO_BIN_EXE_ohl").into());
    let dir = std::env::temp_dir().join(format!("ohl-wind-identical-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (w, lc) = (dir.join("w.ohlwc"), dir.join("lc.ohllc"));
    std::fs::write(&w, WEATHER).unwrap();
    std::fs::write(&lc, LOAD_CASES).unwrap();
    let files = ["points.csv", "spans.csv", "loads.csv", "usage.csv"];
    let out = Command::new(bin)
        .current_dir(root())
        .arg("line")
        .arg(root().join("examples/synthetic-line").join(line))
        .arg("--weather")
        .arg(&w)
        .arg("--load-cases")
        .arg(&lc)
        .arg("--csv")
        .arg(dir.join(files[0]))
        .arg("--spans-csv")
        .arg(dir.join(files[1]))
        .arg("--loads-csv")
        .arg(dir.join(files[2]))
        .arg("--usage-csv")
        .arg(dir.join(files[3]))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut h = 0xcbf2_9ce4_8422_2325;
    for f in files {
        let text = std::fs::read(dir.join(f)).unwrap();
        assert!(!text.is_empty(), "{f}");
        h = fnv(&text, h);
    }
    let _ = std::fs::remove_dir_all(&dir);
    if std::env::var("OHL_REGRESSION_PRINT").is_ok() {
        println!("{line}: {h:#018x}");
    }
    h
}

#[test]
fn the_example_line_gives_the_same_results_as_before() {
    assert_eq!(
        digest("synthetic-line.ohl.json", "plain"),
        0x7931_8a05_edbf_e400
    );
}

/// 2026-11-18: ASCE 10-97 moved into its bundled plugin — every number
/// the same (the CSVs compared column by column), only the usage CSV's
/// notes now name the plugin's method ("… — method asce10-97 of the
/// plugin asce10"): a new digest (before: 0x0e5e_5589_c259_806e).
#[test]
fn the_typed_example_line_gives_the_same_results_as_before() {
    assert_eq!(
        digest("synthetic-line-typed.ohl.json", "typed"),
        0x99b0_4263_1f60_c370
    );
}
