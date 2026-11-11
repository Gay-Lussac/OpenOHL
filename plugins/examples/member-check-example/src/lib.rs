// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Example plugin for the `member-check` interface (2026-11-11): the
//! member check of a structure supplied by a plugin.
//!
//! **An illustration, not a design code.** Method `example-yield-euler`
//! takes, from the records it is sent and nothing else:
//!
//! - tension capacity = area × yield strength (the gross area);
//! - compression capacity = the smaller of area × yield strength and the
//!   Euler load π² E I_v / L_k², with L_k the effective length given
//!   (`lk`, else `k` × length) times the largest length ratio given;
//! - the usage in each load case = |axial force| / (capacity × the case's
//!   strength factor).
//!
//! No safety factors, no net section, no connection, no local buckling,
//! no bending: every result says so and is flagged as not on the safe
//! side. A member without a yield strength is not returned at all —
//! OpenOHL then reports it as NOT CHECKED, NOT ON THE SAFE SIDE.
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    inline: r#"
        package local:member-check-example;
        world example {
            include ohl:plugin/member-check-plugin@0.1.0;
        }
    "#,
    path: "../../../crates/ohl-plugins/wit",
    world: "local:member-check-example/example",
    generate_all,
});

use exports::ohl::plugin::member_check::{
    Capacity, CaseUsage, CheckInput, Member, MemberResult, MethodInfo,
};

/// The method's name.
const METHOD: &str = "example-yield-euler";

/// Said with every result.
const EXAMPLE: &str = "EXAMPLE METHOD, not a design code: gross yield and Euler buckling only \
                       — no safety factors, no net section, connection, local buckling or \
                       bending check";

struct Example;

impl Guest for Example {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "member-check-example".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Example member check (an illustration, not a design code)".into(),
        }
    }
}

/// The effective length the buckling load is taken on, m.
fn buckling_length(m: &Member) -> f64 {
    let ratio = m.length_ratios.iter().copied().fold(1.0_f64, f64::max);
    m.lk.unwrap_or(m.k * m.length) * ratio
}

fn check(m: &Member, input: &CheckInput) -> Option<MemberResult> {
    let fy = m.fy?;
    let yield_load = m.area * fy;
    let lk = buckling_length(m);
    let euler = std::f64::consts::PI.powi(2) * m.e * m.iv / (lk * lk);
    let (compression, by) = if euler < yield_load {
        (euler, format!("Euler buckling, L_k {lk:.2} m (example)"))
    } else {
        (yield_load, "gross yield (example)".to_owned())
    };
    let mut worst = (0.0, String::new());
    let usages = m
        .forces
        .iter()
        .map(|f| {
            let strength = input
                .cases
                .get(f.load_case as usize)
                .map_or(1.0, |c| c.strength);
            let (capacity, check) = if f.axial < 0.0 {
                (compression, by.clone())
            } else {
                (yield_load, "gross yield (example)".to_owned())
            };
            let usage = f.axial.abs() / (capacity * strength);
            if usage >= worst.0 {
                worst = (usage, check.clone());
            }
            CaseUsage {
                load_case: f.load_case,
                usage,
                check,
            }
        })
        .collect();
    // What was received beyond the profile, said (the example reads it
    // only to say so; the tests of the interface look for these notes).
    let mut notes = vec![EXAMPLE.to_owned()];
    if input.joint_details {
        notes.push(format!(
            "joint details received: {} end connection(s) of this member (not used by the example)",
            m.connections.len()
        ));
    }
    if !m.parts.is_empty() {
        notes.push(format!(
            "built-up member: {} part(s) received (checked as one section by the example)",
            m.parts.len()
        ));
    }
    Some(MemberResult {
        id: m.id.clone(),
        capacities: vec![
            Capacity {
                name: "tension".into(),
                value: yield_load,
                governs: "gross yield (example)".into(),
            },
            Capacity {
                name: "compression".into(),
                value: compression,
                governs: by,
            },
        ],
        usages,
        governs: worst.1,
        notes,
        not_safe: Some(EXAMPLE.to_owned()),
    })
}

impl exports::ohl::plugin::member_check::Guest for Example {
    fn methods() -> Vec<MethodInfo> {
        vec![MethodInfo {
            name: METHOD.into(),
            description: EXAMPLE.into(),
        }]
    }

    fn check_members(method: String, input: CheckInput) -> Result<Vec<MemberResult>, String> {
        if method != METHOD {
            return Err(format!("no method \"{method}\" (it gives: {METHOD})"));
        }
        Ok(input
            .members
            .iter()
            .filter_map(|m| check(m, &input))
            .collect())
    }
}

export!(Example);
