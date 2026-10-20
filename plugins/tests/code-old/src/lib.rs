// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test design-code plugin built for the plain `code-plugin` world, as
//! plugins were built before the optional `structure-wind` and
//! `rule-presets` exports: it must load and generate unchanged, and give
//! neither (`ohl-io` tests). Invented values.

wit_bindgen::generate!({
    world: "code-plugin",
    path: "../../../crates/ohl-plugins/wit",
});

struct Code;

impl Guest for Code {
    fn describe() -> CodeInfo {
        CodeInfo {
            name: "code-old".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Test plugin of the plain code-plugin world".into(),
            parameters_template: "# no parameters\n".into(),
        }
    }

    fn generate(_parameters: String) -> Result<Generated, String> {
        Ok(Generated {
            weather_cases: "format = \"ohl-weather-cases\"\nversion = 1\n[[case]]\nname = \"Old\"\ntemperature = \"10 °C\"\n".into(),
            load_cases: "format = \"ohl-load-cases\"\nversion = 1\n[[case]]\nname = \"L\"\nweather = \"Old\"\n".into(),
            notes: Vec::new(),
            clearance_presets: Vec::new(),
        })
    }
}

export!(Code);
