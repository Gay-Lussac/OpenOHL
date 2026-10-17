// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Checks run by plugins (`check-plugin` world, extension point `check`):
//! OLS CAD measures the line, the plugin holds the values and the rules.
//! The first: the distance between conductors in the span
//! (`phase-distance`, [`ols_clearance::phase_distance`]), its stations
//! handed to the plugin pair by pair, one plugin instance per core.
//!
//! Exit clauses: each plugin call has the plugin's limits (ADR-0003); the
//! measuring has the budget given.

use std::path::{Path, PathBuf};

use ols_clearance::phase_distance::{
    Conductor, Evaluator, PairGeometry, PhaseDistancePair, PhaseDistanceSettings, Required, check,
};
use ols_core::budget::Budget;
use ols_plugins::{CheckInfo, ConductorPair, ValueTable, WasmCheckPlugin, WasmHost};

/// The id of the distance between conductors in the span.
pub const PHASE_DISTANCE: &str = "phase-distance";

/// A check a plugin runs.
#[derive(Debug, Clone)]
pub struct PluginCheck {
    /// The plugin's directory.
    pub dir: PathBuf,
    /// The plugin's name and version.
    pub plugin: String,
    /// What it says of the check.
    pub info: CheckInfo,
}

/// The checks of the check plugins in `roots`, in order (a plugin that
/// cannot be loaded is told).
pub fn list(host: &WasmHost, roots: &[PathBuf]) -> (Vec<PluginCheck>, Vec<String>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for f in ols_plugins::discover_checks(roots) {
        if let Some(p) = f.problem() {
            problems.push(format!("{}: {p}", f.name()));
            continue;
        }
        match load(host, &f.dir) {
            Ok((plugin, checks)) => out.extend(checks.into_iter().map(|info| PluginCheck {
                dir: f.dir.clone(),
                plugin: plugin.clone(),
                info,
            })),
            Err(e) => problems.push(format!("{}: {e}", f.name())),
        }
    }
    (out, problems)
}

fn load(host: &WasmHost, dir: &Path) -> Result<(String, Vec<CheckInfo>), String> {
    let mut p = host.load_check(dir).map_err(|e| e.to_string())?;
    let m = p.manifest();
    let name = format!("{} {}", m.plugin.name, m.plugin.version);
    Ok((name, p.checks().map_err(|e| e.to_string())?))
}

/// The values of a values file as the plugin shows them.
pub fn value_tables(
    host: &WasmHost,
    check: &PluginCheck,
    values: &str,
) -> Result<Vec<ValueTable>, String> {
    let mut p = host.load_check(&check.dir).map_err(|e| e.to_string())?;
    p.value_tables(&check.info.id, values)
        .map_err(|e| e.to_string())?
}

/// `values` with a cell of its tables set (table, row, column).
pub fn edit_value(
    host: &WasmHost,
    check: &PluginCheck,
    values: &str,
    cell: [u32; 3],
    text: &str,
) -> Result<String, String> {
    let mut p = host.load_check(&check.dir).map_err(|e| e.to_string())?;
    p.edit_value(&check.info.id, values, cell, text)
        .map_err(|e| e.to_string())?
}

/// A plugin as the measuring's [`Evaluator`].
struct PluginEvaluator<'a> {
    plugin: WasmCheckPlugin,
    check: &'a str,
    values: &'a str,
    choices: &'a [u32],
}

impl Evaluator for PluginEvaluator<'_> {
    fn evaluate(&mut self, pair: &PairGeometry, stations: &[f64]) -> Result<Required, String> {
        let fields = |c: &Conductor| -> Vec<(String, String)> {
            c.fields
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let p = ConductorPair {
            same_system: pair.same_system,
            same_span: pair.same_span,
            kv: (pair.a.wire.kv(), pair.b.wire.kv()),
            fields: (fields(pair.a), fields(pair.b)),
            altitude: pair.altitude,
            diameter_mm: (pair.a.diameter_mm, pair.b.diameter_mm),
            mass_kg_m: (pair.a.mass_kg_m, pair.b.mass_kg_m),
            spacer_sag: pair.spacer_sag.map(|s| (s[0], s[1])),
        };
        let e = self
            .plugin
            .evaluate(self.check, self.values, self.choices, &p, stations)
            .map_err(|e| e.to_string())??;
        Ok(Required {
            values: e.values,
            per_station: e.per_station as usize,
            rule: e.rule,
        })
    }
}

/// The distance between conductors in the span: measured here, required
/// by the plugin of `check` with `values` (its values file) and
/// `choices`.
pub fn phase_distance(
    host: &WasmHost,
    check: &PluginCheck,
    values: &str,
    choices: &[u32],
    settings: &PhaseDistanceSettings,
    conductors: &[Conductor],
    budget: &Budget,
) -> Result<Vec<PhaseDistancePair>, String> {
    if check.info.id != PHASE_DISTANCE {
        return Err(format!(
            "{} is not the distance between conductors",
            check.info.id
        ));
    }
    // Compiled once; an instance (its own store) per core.
    let first = host.load_check(&check.dir).map_err(|e| e.to_string())?;
    let first = std::sync::Mutex::new(first);
    let make = || -> Result<PluginEvaluator, String> {
        let plugin = host
            .another_check(&first.lock().expect("not poisoned"))
            .map_err(|e| e.to_string())?;
        Ok(PluginEvaluator {
            plugin,
            check: &check.info.id,
            values,
            choices,
        })
    };
    check_with(settings, conductors, budget, make)
}

fn check_with<'a>(
    settings: &PhaseDistanceSettings,
    conductors: &[Conductor],
    budget: &Budget,
    make: impl Fn() -> Result<PluginEvaluator<'a>, String> + Sync,
) -> Result<Vec<PhaseDistancePair>, String> {
    check(settings, conductors, budget, make)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_core::units::Temperature;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The lev-ch plugin runs the distance between conductors on the
    /// synthetic line at 10 °C: its values and choices listed, its tables
    /// edited through it; the pairs as measured, required by it (2.2.1);
    /// two conductors joined by a spacer: 2.2.4. Skipped when not built
    /// (unless the plugin tests are required).
    #[test]
    fn the_lev_plugin_checks_the_distance_between_conductors() {
        let dir = root().join("plugins/codes/lev-ch");
        let built = dir
            .join("target/wasm32-wasip2/release/lev_ch.wasm")
            .is_file();
        assert!(
            built || std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
            "lev-ch plugin not built"
        );
        if !built {
            return;
        }
        let host = WasmHost::new().unwrap();
        let (checks, problems) = list(&host, &[root().join("plugins/codes")]);
        assert!(problems.is_empty(), "{problems:?}");
        let check = checks.iter().find(|c| c.info.id == PHASE_DISTANCE).unwrap();
        assert!(check.plugin.starts_with("lev-ch "));
        assert_eq!(check.info.menu, vec!["LeV/OLEI".to_owned()]);
        assert_eq!(check.info.choices[0].options.len(), 2);
        let values = &check.info.values;
        let tables = value_tables(&host, check, values).unwrap();
        assert!(tables[0].title.contains("2.4.1"));
        let edited = edit_value(&host, check, values, [1, 0, 1], "0.2").unwrap();
        assert!(edited.contains("per_1000_m = 0.2"));
        assert!(edit_value(&host, check, values, [1, 0, 1], "x").is_err());

        let ex = root().join("examples/synthetic-line");
        let mut line = ols_line::Line::load(&ex.join("synthetic-line.ols.json")).unwrap();
        let library = ols_cables::Library::load(&root().join("libraries/cables")).unwrap();
        let case = ols_weather::WeatherCase::still("10", Temperature::from_celsius(10.0));
        let run = |line: &ols_line::Line| {
            let shapes: Vec<_> = ols_line::solve::solve_line(line, &library, &case)
                .into_iter()
                .map(Result::unwrap)
                .collect();
            let cs = ols_clearance::phase_distance::conductors(line, &shapes, &library);
            let pairs = phase_distance(
                &host,
                check,
                values,
                &[0],
                &PhaseDistanceSettings::default(),
                &cs,
                &Budget::new(),
            )
            .unwrap();
            (cs, pairs)
        };
        let (cs, pairs) = run(&line);
        assert!(!pairs.is_empty());
        for p in &pairs {
            let w = &p.worst;
            assert_eq!(p.rule, "2.2.1");
            let (k, c) = (&w.values[..2], w.values[2]);
            let term = |i: usize| k[i] * (w.sag[i] + w.lk[i]).sqrt();
            assert!((w.required - (term(0).max(term(1)) + c)).abs() < 1e-9);
            assert!(cs[p.a].diameter_mm > 0.0);
        }
        // A spacer between the first two sections' conductors at mid-span.
        let (a, b) = (line.sections[0].id.clone(), line.sections[1].id.clone());
        for s in &mut line.sections[..2] {
            s.span_points.push(ols_line::SpanPoint {
                label: "M".into(),
                span: 0,
                at: ols_line::SpanPlace::at_fraction(0.5),
                description: String::new(),
                object: Some(ols_line::LineObject::clamp(0.0)),
            });
        }
        let end = |section: &str| ols_line::SpanPointRef {
            section: section.into(),
            point: "M".into(),
        };
        line.spacers.push(ols_line::Spacer {
            id: "SP".into(),
            a: end(&a),
            b: end(&b),
            weight: 100.0,
            compression: Some(10_000.0),
            ..Default::default()
        });
        line.validate().unwrap();
        let (cs, pairs) = run(&line);
        let joined = pairs
            .iter()
            .find(|p| {
                let s = [&cs[p.a].wire, &cs[p.b].wire];
                s.iter().any(|w| w.section == a && w.span == 0)
                    && s.iter().any(|w| w.section == b && w.span == 0)
            })
            .expect("the pair joined");
        assert!(joined.rule.starts_with("2.2.4"), "{}", joined.rule);
        let df = joined.spacer_sag.unwrap();
        assert!(df[0] > 0.0 && df[1] > 0.0);
        assert!(joined.worst.required >= 0.9);
        assert!(pairs.iter().filter(|p| p.rule.starts_with("2.2.4")).count() == 1);
    }
}
