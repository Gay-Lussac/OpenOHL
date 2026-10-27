// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin (`wire-colour-plugin` world): wire colours from an
//! **invented** colour table for the tests (`ohl-plugins`, `ohl-app`): no
//! company's or authority's. Owner "Utility A": L1 red, L2 green, L3 blue,
//! by the column `tint` = "pale" lighter; earth wires orange; anything
//! else no suggestion; a circuit named "fail" makes it fail. System
//! styles: owner "Utility A": red at 300 kV and above, else green; the
//! phases' line codes solid, ·–, ·–– and angles 0°, −120°, 120° in order;
//! a system named "fail" makes it fail.

wit_bindgen::generate!({
    inline: r#"
        package local:wire-colours;
        world test {
            include ohl:plugin/wire-colour-plugin@0.1.0;
            export ohl:plugin/system-styles@0.1.0;
        }
    "#,
    path: "../../../crates/ohl-plugins/wit",
    world: "local:wire-colours/test",
    generate_all,
});

use exports::ohl::plugin::system_styles::{PhaseStyle, SystemInfo, SystemStyle};
use exports::ohl::plugin::wire_colours::WireGroup;

struct Colours;

impl Guest for Colours {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "wire-colours".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Test plugin: wire colours (invented)".into(),
        }
    }
}

impl exports::ohl::plugin::wire_colours::Guest for Colours {
    fn wire_colours(groups: Vec<WireGroup>) -> Result<Vec<Option<String>>, String> {
        if groups.iter().any(|g| g.circuit == "fail") {
            return Err("a circuit named \"fail\"".into());
        }
        Ok(groups
            .iter()
            .map(|g| {
                if g.system.is_none() {
                    return Some("#ff8000".into());
                }
                if g.owner != "Utility A" {
                    return None;
                }
                let pale = g.fields.iter().any(|(k, v)| k == "tint" && v == "pale");
                let c = match (g.phase.as_str(), pale) {
                    ("L1", false) => "#ff0000",
                    ("L2", false) => "#00ff00",
                    ("L3", false) => "#0000ff",
                    ("L1", true) => "#ff8080",
                    ("L2", true) => "#80ff80",
                    ("L3", true) => "#8080ff",
                    _ => return None,
                };
                Some(c.into())
            })
            .collect())
    }
}

impl exports::ohl::plugin::system_styles::Guest for Colours {
    fn system_styles(systems: Vec<SystemInfo>) -> Result<Vec<SystemStyle>, String> {
        if systems.iter().any(|s| s.name == "fail") {
            return Err("a system named \"fail\"".into());
        }
        Ok(systems
            .iter()
            .map(|s| {
                if s.owner != "Utility A" {
                    return SystemStyle {
                        color: None,
                        phases: Vec::new(),
                    };
                }
                SystemStyle {
                    color: Some(if s.voltage_kv >= 300.0 { "#ff0000" } else { "#00ff00" }.into()),
                    phases: s
                        .phases
                        .iter()
                        .enumerate()
                        .map(|(k, _)| PhaseStyle {
                            code: Some(["-", ".-", ".--"][k % 3].into()),
                            angle_deg: Some([0.0, -120.0, 120.0][k % 3]),
                        })
                        .collect(),
                }
            })
            .collect())
    }
}

export!(Colours);
