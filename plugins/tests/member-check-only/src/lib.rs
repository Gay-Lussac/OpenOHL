// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin: the `member-check` export **alone** (as a plugin built
//! before `joint-check` and `bolt-distances` existed). Method
//! `members-only` gives every member the invented usage |axial force| /
//! (area × 100 MPa × strength factor) — not a design code. OpenOHL must
//! then report every joint part as NOT CHECKED by this plugin and say
//! that it gives no minimum distances, never another code's.
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    inline: r#"
        package local:member-check-only;
        world only {
            include ohl:plugin/member-check-plugin@0.1.0;
        }
    "#,
    path: "../../../crates/ohl-plugins/wit",
    world: "local:member-check-only/only",
    generate_all,
});

use exports::ohl::plugin::member_check::{CaseUsage, CheckInput, MemberResult, MethodInfo};

const METHOD: &str = "members-only";
const TEST: &str = "TEST METHOD, not a design code";

struct Only;

impl Guest for Only {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "member-check-only".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Test member check without joint checks (not a design code)".into(),
        }
    }
}

impl exports::ohl::plugin::member_check::Guest for Only {
    fn methods() -> Vec<MethodInfo> {
        vec![MethodInfo {
            name: METHOD.into(),
            description: TEST.into(),
        }]
    }

    fn check_members(method: String, input: CheckInput) -> Result<Vec<MemberResult>, String> {
        if method != METHOD {
            return Err(format!("no method \"{method}\""));
        }
        Ok(input
            .members
            .iter()
            .map(|m| MemberResult {
                id: m.id.clone(),
                capacities: Vec::new(),
                usages: m
                    .forces
                    .iter()
                    .map(|f| CaseUsage {
                        load_case: f.load_case,
                        usage: f.axial.abs()
                            / (m.area
                                * 100e6
                                * input
                                    .cases
                                    .get(f.load_case as usize)
                                    .map_or(1.0, |c| c.strength)),
                        check: "area × 100 MPa (test)".into(),
                    })
                    .collect(),
                governs: "area × 100 MPa (test)".into(),
                notes: vec![TEST.to_owned()],
                not_safe: Some(TEST.to_owned()),
            })
            .collect())
    }
}

export!(Only);
