// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! `ohl lightning` on the shielded example (`examples/lightning`): the
//! shielding wires in their own weather case (owner, 2026-11-01: "pick
//! different weather cases for the earth wires and the conductors").

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs the angle rule of the example (`extra` lines added to the rule);
/// the rows and cross-sections as JSON.
fn run(dir: &Path, name: &str, extra: &str) -> serde_json::Value {
    let ex = root().join("examples/lightning");
    let rules = dir.join(format!("rules-{name}.ohlr"));
    let mut text = String::from(
        "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"test\"\n\n\
         [[lightning_rule]]\nname = \"angle\"\ncases = [\"max temperature\"]\n\
         method = \"angle\"\nouter_angle = 30.0\ninner_angle = 45.0\n",
    );
    text.push_str(extra);
    std::fs::write(&rules, text).unwrap();
    let json = dir.join(format!("out-{name}.json"));
    let out = Command::new(env!("CARGO_BIN_EXE_ohl"))
        .arg("lightning")
        .arg(ex.join("shielded-line.ohl.json"))
        .arg("--weather")
        .arg(ex.join("weather-cases.ohlwc"))
        .args(["--case", "max temperature", "--rules"])
        .arg(&rules)
        .arg("--json")
        .arg(&json)
        .output()
        .unwrap();
    assert!(
        out.status.code().is_some_and(|c| c <= 1),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap()
}

/// The elevations of the shielding wires and of the phases summed over
/// every cross-section, m.
fn heights(v: &serde_json::Value) -> (f64, f64) {
    let sum = |key: &str, z: &dyn Fn(&serde_json::Value) -> f64| -> f64 {
        v["cuts"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c[key].as_array().unwrap().iter().map(z))
            .sum()
    };
    (
        sum("shields", &|w| w["z"].as_f64().unwrap()),
        sum("phases", &|p| p["wire"]["z"].as_f64().unwrap()),
    )
}

/// The phases at their highest temperature, the shielding wires (carrying
/// no load current) at an everyday 10 °C: the rows name both cases, the
/// shielding wires hang higher than at 80 °C, the phases where they were;
/// the case checked is still only the one given on the command line (the
/// shielding wires' case is read from the same file).
#[test]
fn the_shielding_wires_hang_in_their_own_case() {
    let dir = std::env::temp_dir().join(format!("ohl-lightning-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let same = run(&dir, "same", "");
    let cool = run(&dir, "cool", "shield_cases = [\"everyday\"]\n");
    let cases = |v: &serde_json::Value| -> Vec<String> {
        let mut c: Vec<String> = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["case"].as_str().unwrap().to_owned())
            .collect();
        c.dedup();
        c
    };
    assert_eq!(cases(&same), ["max temperature"]);
    assert_eq!(
        cases(&cool),
        ["max temperature (shielding wires: everyday)"]
    );
    let ((shields_hot, phases_hot), (shields_cool, phases_cool)) = (heights(&same), heights(&cool));
    assert!(
        shields_cool > shields_hot + 1.0,
        "the shielding wires higher at 10 °C: {shields_cool} against {shields_hot}"
    );
    assert!(
        (phases_cool - phases_hot).abs() < 1e-9,
        "the phases as they were"
    );
    // The shielding wires by the ruling span (owner, 2026-11-01: the
    // calculation of each), the phases by their own: named in the rows.
    let rs = run(&dir, "rs", "shield_calculation = \"initial-rs\"\n");
    assert_eq!(
        cases(&rs),
        ["max temperature (shielding wires: max temperature, Initial RS)"]
    );
    assert!(
        (heights(&rs).1 - phases_hot).abs() < 1e-9,
        "the phases as they were"
    );
    std::fs::remove_dir_all(&dir).ok();
}
