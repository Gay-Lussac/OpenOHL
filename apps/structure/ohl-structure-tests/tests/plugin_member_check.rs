// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The member check by a plugin (the `member-check` interface,
//! 2026-11-11), end to end through the link and the real plugin host on
//! an invented truss: two bars in line, a–b (steel, f_y 355 MPa) and b–c
//! (a material without a yield strength), 1.5 m each, EA alike, 20 kN
//! along them at b — 10 kN compression in a–b, 10 kN tension in b–c.
//!
//! The example plugin (`plugins/examples/member-check-example`, method
//! `example-yield-euler`: an illustration, not a design code) gives a–b
//! the Euler load π² E I_v / L² = π² · 210e9 · 2e-7 / 1.5² = 184.2 kN
//! (below A f_y = 355 kN), so its usage is 10 / 184.2; it returns nothing
//! for b–c (no yield strength): NOT CHECKED, NOT ON THE SAFE SIDE, without
//! a usage. A file naming a plugin that is not installed is refused, the
//! plugin named. The two plugin runs are skipped when the example is not
//! built (`cargo build --release --target wasm32-wasip2` in its folder).

use std::path::{Path, PathBuf};

use ohl_structure_link::{CaseResult, CaseToRun, analyse_cases, plugin_check};
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::{LibraryItem, StructureType};

const TEXT: &str = r#"id = "P"
name = "plugin check"
kind = "lattice"
[analysis]
nonlinear = false
self_weight = false
capacity = "plugin:member-check-example/example-yield-euler"
[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "355 MPa"
[[material]]
id = "unknown"
elasticity = "210 GPa"
[[section]]
id = "s"
material = "steel"
area = "10 cm2"
iy = "50 cm4"
iz = "50 cm4"
i_min = "20 cm4"
[[section]]
id = "u"
material = "unknown"
area = "10 cm2"
iy = "50 cm4"
iz = "50 cm4"
i_min = "20 cm4"
[[node]]
id = "a"
point = [0, 0, 0]
support = "pinned"
[[node]]
id = "b"
point = [1.5, 0, 0]
support = ["y", "z"]
[[node]]
id = "c"
point = [3, 0, 0]
support = "pinned"
[[member]]
id = "ab"
nodes = ["a", "b"]
section = "s"
[[member]]
id = "bc"
nodes = ["b", "c"]
section = "u"
"#;

/// A folder for the structure, with the example plugin in `Plugins/`
/// beside it when `installed` (none: the example is not built).
fn folder(tag: &str, installed: bool) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!("ohl-plugin-check-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    if installed {
        let from = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../plugins/examples/member-check-example");
        let wasm = "target/wasm32-wasip2/release/member_check_example.wasm";
        if !from.join(wasm).is_file() {
            eprintln!("skipping: plugins/examples/member-check-example not built");
            return None;
        }
        let to = dir.join(plugin_check::BESIDE).join("member-check-example");
        std::fs::create_dir_all(to.join(wasm).parent().unwrap()).unwrap();
        std::fs::copy(from.join("ohl-plugin.toml"), to.join("ohl-plugin.toml")).unwrap();
        std::fs::copy(from.join(wasm), to.join(wasm)).unwrap();
    }
    Some(dir)
}

fn solve(dir: &Path) -> Result<CaseResult, String> {
    let structure = StructureType::parse(Path::new("p.ohls"), TEXT).unwrap();
    let table = LoadTable {
        rows: vec![LoadRow {
            case: "push".into(),
            load: None,
            at: At::Joint,
            label: "b".into(),
            force: [-20e3, 0.0, 0.0],
            source: Source::Typed,
            evaluated: None,
            missing: None,
            note: String::new(),
            area: None,
        }],
    };
    let cases = [CaseToRun {
        name: "push".into(),
        strength: 1.0,
        self_weight: 0.0,
    }];
    let mut results = analyse_cases(
        &structure,
        TEXT,
        Some(dir),
        &Default::default(),
        &table,
        &cases,
        &ohl_structures_fe::default_budget(),
    )?;
    results.remove(0).1
}

#[test]
fn the_example_plugin_checks_a_truss_as_by_hand() {
    let Some(dir) = folder("run", true) else {
        return;
    };
    let r = solve(&dir).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let ab = r.members.iter().find(|m| m.id == "ab").unwrap();
    assert!((ab.axial + 10e3).abs() < 1e-3, "{}", ab.axial);
    let euler = std::f64::consts::PI.powi(2) * 210e9 * 2e-7 / (1.5 * 1.5);
    assert!((euler - 184_232.6).abs() < 1.0, "{euler}");
    let usage = ab.usage.unwrap();
    assert!((usage - 10e3 / euler).abs() < 1e-9, "{usage}");
    // What governs is the plugin's words, and it says it is an example
    // and not on the safe side.
    assert!(
        ab.governs
            .starts_with("Euler buckling, L_k 1.50 m (example)"),
        "{}",
        ab.governs
    );
    assert!(
        ab.governs
            .contains("NOT ON THE SAFE SIDE: EXAMPLE METHOD, not a design code"),
        "{}",
        ab.governs
    );
    assert!(
        r.notes.iter().any(|n| n.contains(
            "method example-yield-euler of the plugin member-check-example (not checked by OpenOHL)"
        )),
        "{:?}",
        r.notes
    );
}

#[test]
fn a_member_the_plugin_returns_nothing_for_is_not_checked() {
    let Some(dir) = folder("bare", true) else {
        return;
    };
    let r = solve(&dir).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    let bc = r.members.iter().find(|m| m.id == "bc").unwrap();
    assert!((bc.axial - 10e3).abs() < 1e-3, "{}", bc.axial);
    assert_eq!(bc.usage, None);
    assert!(
        bc.governs
            .starts_with("NOT CHECKED, NOT ON THE SAFE SIDE: plugin member-check-example"),
        "{}",
        bc.governs
    );
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("1 member(s) NOT CHECKED, NOT ON THE SAFE SIDE")),
        "{:?}",
        r.notes
    );
}

#[test]
fn a_plugin_that_is_not_installed_is_refused_by_name() {
    let dir = folder("missing", false).unwrap();
    let e = solve(&dir).unwrap_err();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        e.contains(
            "plugin \"member-check-example\" (method \"example-yield-euler\") is not installed"
        ) && e.contains("not checked by another method in its place"),
        "{e}"
    );
    // The core alone (no plugin host): every member not checked, said —
    // never ASCE 10-97 in its place.
    let model = ohl_structures_fe::Model::from_structure_type(TEXT).unwrap();
    assert_eq!(
        model.capacity_plugin.as_ref().unwrap().name(),
        "plugin:member-check-example/example-yield-euler"
    );
    let loads = ohl_structures_fe::Loads::default();
    let (_, usage) =
        ohl_structures_fe::analyse(&model, &loads, 1.0, &ohl_structures_fe::default_budget())
            .unwrap();
    assert!(usage.iter().all(|u| {
        u.usage.is_none()
            && u.governs
                .starts_with(ohl_structures_fe::check::PLUGIN_NOT_RUN)
    }));
    // A method the plugin does not have, and a name that is no method.
    assert!(
        ohl_structures_fe::Model::from_structure_type(&TEXT.replace(
            "plugin:member-check-example/example-yield-euler",
            "plugin:member-check-example"
        ))
        .unwrap_err()
        .contains("plugin:<plugin>/<method>")
    );
}
