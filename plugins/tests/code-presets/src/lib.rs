// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test design-code plugin (`code-rules-plugin` world): one weather case,
//! one load case, no wind factors, **one preset of clearance rules** — a
//! ground rule by voltage (base + per kV) and a crossing rule with a table
//! by voltage, in a weather case of its own — and **one preset of the
//! vegetation check** (a `[vegetation]` table, no rule: classes 42 and 43,
//! a clearance table by voltage, the danger-tree values, wind steps), with
//! two weather cases: one the example line has ("max temperature", the
//! same values) and one it lacks. Every value is **invented** for the
//! tests (`ohl-io`, `ohl-app`, `ohl-cli`): not from any standard.

wit_bindgen::generate!({
    world: "code-rules-plugin",
    path: "../../../crates/ohl-plugins/wit",
});

use exports::ohl::plugin::rule_presets::RulePreset;

struct Code;

/// The preset's rules (invented values).
const RULES: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "Test ground and crossings"

# Invented test values, not from any standard.
[[rule]]
name = "Test ground"
target = "ground"
vertical = { base = "6 m", per_kv = "0.01 m", above_kv = 0.0 }
cases = ["Test hot"]

[[rule]]
name = "Test crossings"
target = "other_lines"
radial = { by_voltage = [[0.0, "2 m"], [150.0, "3 m"], [420.0, "4.5 m"]] }
"#;

/// The vegetation preset (invented values).
const TREES: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "Test danger trees"

[wind]
steps = true
step = "100 Pa"

[vegetation]
classes = [42, 43]
clearance = { by_voltage = [[0.0, "2.5 m"], [220.0, "7 m"], [380.0, "9 m"]] }
root_ball = 0.05
fall_allowance = "2 m"
max_base_ground_distance = "5 m"
search_distance = "50 m"
"#;

/// Its weather cases: one the example line has, one it lacks.
const TREES_WEATHER: &str = r#"format = "ohl-weather-cases"
version = 1

[[case]]
name = "max temperature"
description = "maximum conductor temperature"
tags = ["clearance"]
temperature = "80 °C"

[[case]]
name = "Test warm wind"
temperature = "60 °C"
wind_pressure = "200 Pa"
"#;

const WEATHER: &str = r#"format = "ohl-weather-cases"
version = 1

[[case]]
name = "Test hot"
temperature = "40 °C"
"#;

impl Guest for Code {
    fn describe() -> CodeInfo {
        CodeInfo {
            name: "code-presets".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Test plugin: one preset of clearance rules".into(),
            parameters_template: "# no parameters\n".into(),
        }
    }

    fn generate(_parameters: String) -> Result<Generated, String> {
        Ok(Generated {
            weather_cases: WEATHER.into(),
            load_cases: "format = \"ohl-load-cases\"\nversion = 1\n[[case]]\nname = \"L\"\nweather = \"Test hot\"\n".into(),
            notes: vec!["invented test values".into()],
            clearance_presets: Vec::new(),
        })
    }
}

impl exports::ohl::plugin::structure_wind::Guest for Code {
    fn structure_wind(_parameters: String) -> Result<Option<String>, String> {
        Ok(None)
    }
}

impl exports::ohl::plugin::rule_presets::Guest for Code {
    fn rule_presets(parameters: String) -> Result<Vec<RulePreset>, String> {
        if parameters.contains("fail") {
            return Err("the parameter file asks to fail".into());
        }
        Ok(vec![
            RulePreset {
                name: "ground".into(),
                description: "Ground 6 m + 0.01 m/kV, crossings by voltage (invented test values)"
                    .into(),
                rules: RULES.into(),
                weather_cases: WEATHER.into(),
            },
            RulePreset {
                name: "trees".into(),
                description: "Danger trees: classes 42 and 43, clearance by voltage (invented \
                              test values)"
                    .into(),
                rules: TREES.into(),
                weather_cases: TREES_WEATHER.into(),
            },
        ])
    }
}

export!(Code);
