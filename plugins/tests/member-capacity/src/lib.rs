// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin (`member-capacity-plugin` world): member capacities by an
//! **invented**, trivial method for the tests (`ohl-plugins`,
//! `ohl structures fe --capacity-plugin`) — not a design code. Method
//! `area-yield`: compression and tension both area × yield (OpenOHL applies
//! the strength factor); a member without
//! a yield strength gets none ("no yield"). Its notes say what it
//! received: `design: class` when the design data name a class,
//! `partner <N> N` with a crossing partner's force. Method `fail` fails.

wit_bindgen::generate!({
    inline: r#"
        package local:member-capacity;
        world test {
            include ohl:plugin/member-capacity-plugin@0.1.0;
        }
    "#,
    path: "../../../crates/ohl-plugins/wit",
    world: "local:member-capacity/test",
    generate_all,
});

use exports::ohl::plugin::member_capacity::{MemberInput, MemberResult, MethodInfo};

struct Capacity;

impl Guest for Capacity {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "member-capacity".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Test plugin: member capacities (invented)".into(),
        }
    }
}

impl exports::ohl::plugin::member_capacity::Guest for Capacity {
    fn methods() -> Vec<MethodInfo> {
        vec![
            MethodInfo {
                name: "area-yield".into(),
                description: "Invented test method: area × yield both ways".into(),
            },
            MethodInfo {
                name: "fail".into(),
                description: "Always fails".into(),
            },
        ]
    }

    fn member_capacities(
        method: String,
        _strength: f64,
        members: Vec<MemberInput>,
    ) -> Result<Vec<MemberResult>, String> {
        if method != "area-yield" {
            return Err(format!("no method \"{method}\""));
        }
        Ok(members
            .iter()
            .map(|m| {
                let mut notes = Vec::new();
                if m.design.contains("class") {
                    notes.push("design: class".into());
                }
                if let Some(p) = m.partner_axial {
                    notes.push(format!("partner {p:.0} N"));
                }
                let cap = m.fy.map(|fy| m.area * fy);
                let why = if cap.is_some() { "area × yield" } else { "no yield" };
                MemberResult {
                    compression: cap,
                    compression_governs: why.into(),
                    tension: cap,
                    tension_governs: why.into(),
                    notes,
                }
            })
            .collect())
    }
}

export!(Capacity);
