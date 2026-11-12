// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin: the checks `phase-distance` and `unequal-wind` (the
//! windows *Distance between conductors* and *… under unequal wind*)
//! with an **invented** fixed distance z — the values file's `distance`,
//! 3 m as shipped — whatever was measured: not a design code. It returns
//! per station z, then two numbers of its own (shown as k, here 0) and
//! C = z.
//!
//! Build with `cargo build --release --target wasm32-wasip2`.

wit_bindgen::generate!({
    inline: r#"
        package local:check-distance;
        world only {
            include ohl:plugin/check-plugin@0.1.0;
        }
    "#,
    path: "../../../crates/ohl-plugins/wit",
    world: "local:check-distance/only",
    generate_all,
});

use exports::ohl::plugin::check::{CheckInfo, ConductorPair, Evaluated, Guest, ValueTable};

/// The values file as shipped.
const VALUES: &str = "# Invented test values, not a design code\ndistance = 3.0\n";

/// The distance of `values` (`distance = …`), m.
fn distance(values: &str) -> Result<f64, String> {
    let text = values
        .lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == "distance")
        .ok_or_else(|| "no `distance` in the values file".to_owned())?
        .1;
    let d: f64 = text.trim().parse().map_err(|e| format!("distance: {e}"))?;
    if d > 0.0 {
        Ok(d)
    } else {
        Err("distance: above 0".into())
    }
}

struct Plugin;

impl Guest for Plugin {
    fn checks() -> Vec<CheckInfo> {
        let info = |id: &str, title: &str| CheckInfo {
            id: id.into(),
            title: title.into(),
            menu: vec!["Test distances".into()],
            explanation: "TEST PLUGIN, not a design code: z is the invented distance of the \
                          values file between any two conductors."
                .into(),
            values: VALUES.into(),
            values_extension: "toml".into(),
            choices: Vec::new(),
            circuit_columns: Vec::new(),
        };
        vec![
            info("phase-distance", "Distance between conductors (test)"),
            info(
                "unequal-wind",
                "Distance between conductors under unequal wind (test)",
            ),
        ]
    }

    fn value_tables(_check: String, values: String) -> Result<Vec<ValueTable>, String> {
        let d = distance(&values)?;
        Ok(vec![ValueTable {
            title: "Invented distance".into(),
            note: "Test values, not a design code".into(),
            columns: vec!["z [m]".into()],
            rows: vec![vec![format!("{d}")]],
            editable: vec![true],
        }])
    }

    fn edit_value(
        _check: String,
        _values: String,
        table: u32,
        row: u32,
        column: u32,
        text: String,
    ) -> Result<String, String> {
        if (table, row, column) != (0, 0, 0) {
            return Err("no such cell".into());
        }
        let new = format!(
            "# Invented test values, not a design code\ndistance = {}\n",
            text.trim()
        );
        distance(&new)?;
        Ok(new)
    }

    fn evaluate(
        _check: String,
        values: String,
        _choices: Vec<u32>,
        _pair: ConductorPair,
        stations: Vec<f64>,
    ) -> Result<Evaluated, String> {
        let z = distance(&values)?;
        let n = stations.len() / 6;
        Ok(Evaluated {
            values: (0..n).flat_map(|_| [z, 0.0, 0.0, z]).collect(),
            per_station: 4,
            rule: "invented fixed distance".into(),
        })
    }
}

export!(Plugin);
