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
//! plugin named. The plugin runs are skipped when the example is not
//! built (`cargo build --release --target wasm32-wasip2` in its folder);
//! with `OHL_REQUIRE_PLUGIN_TESTS` set they fail instead.
//!
//! One call per structure with all its load cases
//! ([`plugin_check::calls`]): in the link's run and in the line's
//! structure check (the synthetic example line, its lattice type given
//! the plugin's method: one call per placed structure). Level 4 with the
//! joint details taken: the ends' connections reach the plugin (it says
//! how many it received), and the core's remark on an eccentric end
//! stays on the plugin's result.

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
    if installed && !install(&dir) {
        return None;
    }
    Some(dir)
}

/// The example plugin copied into `Plugins/` of `dir`; false: it is not
/// built (a failure when `OHL_REQUIRE_PLUGIN_TESTS` is set).
fn install(dir: &Path) -> bool {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plugins/examples/member-check-example");
    let wasm = "target/wasm32-wasip2/release/member_check_example.wasm";
    if !from.join(wasm).is_file() {
        assert!(
            std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "plugins/examples/member-check-example not built"
        );
        eprintln!("skipping: plugins/examples/member-check-example not built");
        return false;
    }
    let to = dir.join(plugin_check::BESIDE).join("member-check-example");
    std::fs::create_dir_all(to.join(wasm).parent().unwrap()).unwrap();
    std::fs::copy(from.join("ohl-plugin.toml"), to.join("ohl-plugin.toml")).unwrap();
    std::fs::copy(from.join(wasm), to.join(wasm)).unwrap();
    true
}

/// The tests that call a plugin, one at a time: they count the calls.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
    let _guard = one_at_a_time();
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
            .contains("[not on the safe side: EXAMPLE METHOD, not a design code")
            && ohl_structures_fe::check::not_safe(&ab.governs),
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
    let _guard = one_at_a_time();
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
    let _guard = one_at_a_time();
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

/// Three load cases of the truss: the plugin is called once, with all
/// three, and each case has its own usage (the strength factor applied).
#[test]
fn the_link_calls_the_plugin_once_for_all_cases() {
    let _guard = one_at_a_time();
    let Some(dir) = folder("once", true) else {
        return;
    };
    let structure = StructureType::parse(Path::new("p.ohls"), TEXT).unwrap();
    let row = |case: &str, f: f64| LoadRow {
        case: case.into(),
        load: None,
        at: At::Joint,
        label: "b".into(),
        force: [f, 0.0, 0.0],
        source: Source::Typed,
        evaluated: None,
        missing: None,
        note: String::new(),
        area: None,
    };
    let table = LoadTable {
        rows: vec![row("one", -20e3), row("two", -40e3), row("three", -20e3)],
    };
    let case = |name: &str, strength: f64| CaseToRun {
        name: name.into(),
        strength,
        self_weight: 0.0,
    };
    let cases = [case("one", 1.0), case("two", 1.0), case("three", 0.5)];
    let before = plugin_check::calls();
    let results = analyse_cases(
        &structure,
        TEXT,
        Some(&dir),
        &Default::default(),
        &table,
        &cases,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    assert_eq!(
        plugin_check::calls() - before,
        1,
        "one call for three cases"
    );
    std::fs::remove_dir_all(&dir).ok();
    let usage: Vec<f64> = results
        .iter()
        .map(|(_, r)| r.as_ref().unwrap().members[0].usage.unwrap())
        .collect();
    let euler = std::f64::consts::PI.powi(2) * 210e9 * 2e-7 / 1.5_f64.powi(2);
    for (u, by_hand) in usage
        .iter()
        .zip([10e3 / euler, 20e3 / euler, 10e3 / (euler * 0.5)])
    {
        assert!((u - by_hand).abs() < 1e-9, "{u} {by_hand}");
    }
    for (_, r) in &results {
        let r = r.as_ref().unwrap();
        // One capacity note, the plugin's.
        let notes: Vec<&String> = r
            .notes
            .iter()
            .filter(|n| n.starts_with("member capacities:"))
            .collect();
        assert_eq!(notes.len(), 1, "{:?}", r.notes);
        assert!(notes[0].contains("method example-yield-euler of the plugin"));
        assert!(r.members[1].governs.starts_with(plugin_check::NOT_CHECKED));
    }
}

/// A level 4 structure with its joint details in the calculation: a
/// column C and a tie T bolted onto it 20 mm off its axis, every end with
/// a connection. The plugin receives the ends' connections (two per
/// member, said in its notes) and the core's remark on T's eccentric end
/// is appended to the plugin's result; with the option off the plugin is
/// sent no connection.
#[test]
fn level_4_connections_reach_the_plugin_and_the_cores_remarks_stay() {
    let _guard = one_at_a_time();
    let text = |option: &str| {
        format!(
            r#"id = "E"
name = "eccentric end"
kind = "lattice"
{option}
[analysis]
nonlinear = false
self_weight = false
capacity = "plugin:member-check-example/example-yield-euler"
[[material]]
id = "steel"
elasticity = "210 GPa"
yield = "355 MPa"
[[section]]
id = "s"
material = "steel"
area = "10 cm2"
iy = "200 cm4"
iz = "200 cm4"
i_min = "200 cm4"
[[node]]
id = "N1"
point = [0, 0, 0]
support = "fixed"
[[node]]
id = "N2"
point = [0, 0, 4]
[[node]]
id = "N3"
point = [3, 0, 4]
support = ["y", "z"]
[[member]]
id = "C"
nodes = ["N1", "N2"]
section = "s"
kind = "frame"
[[member]]
id = "T"
nodes = ["N2", "N3"]
section = "s"
[[connection]]
member = "T"
joint = "N2"
onto = "C"
shift = "20 mm"
[[connection]]
member = "T"
joint = "N3"
[[connection]]
member = "C"
joint = "N1"
[[connection]]
member = "C"
joint = "N2"
"#
        )
    };
    let Some(dir) = folder("level4", true) else {
        return;
    };
    let run = |text: &str| {
        let t = StructureType::parse(Path::new("e.ohls"), text).unwrap();
        assert_eq!(t.detail(), ohl_structures::Detail::Joints);
        let model = ohl_structures_fe::Model::from_structure_type(text).unwrap();
        let model = plugin_check::find(model, Some(&dir)).unwrap();
        let loads = ohl_structures_fe::Loads {
            nodal: vec![ohl_structures_fe::model::NodalLoad {
                node: model.nodes.iter().position(|n| n.id == "N3").unwrap(),
                force: [10e3, 0.0, 0.0],
                moment: [0.0; 3],
            }],
            ..Default::default()
        };
        let (solution, _) =
            ohl_structures_fe::analyse(&model, &loads, 1.0, &ohl_structures_fe::default_budget())
                .unwrap();
        let checked = plugin_check::check(
            &model,
            &[plugin_check::Solved {
                name: "pull",
                strength: 1.0,
                self_weight: 0.0,
                solution: &solution,
            }],
        )
        .expect("a plugin's method")
        .unwrap();
        (model, checked)
    };
    let (model, taken) = run(&text("joint_details = true"));
    assert!(model.joint_use.taken());
    for r in &taken.results {
        assert!(
            r.notes
                .iter()
                .any(|n| n.starts_with("joint details received: 2 end connection(s)")),
            "{r:?}"
        );
    }
    assert_eq!(taken.results.len(), 2);
    // T (the second member): the plugin's usage, with the core's remark
    // on its shifted end.
    let t = &taken.usage[0][1];
    assert!(t.usage.is_some());
    assert!(
        t.governs.contains("(example)") && t.governs.contains(ohl_structures_fe::check::SHIFTED),
        "{}",
        t.governs
    );
    // The simple form: no connection is sent.
    let (model, simple) = run(&text(""));
    assert!(!model.joint_use.taken());
    std::fs::remove_dir_all(&dir).ok();
    for r in &simple.results {
        assert!(
            r.notes
                .iter()
                .all(|n| !n.contains("joint details received")),
            "{r:?}"
        );
    }
    assert!(
        !simple.usage[0][1]
            .governs
            .contains(ohl_structures_fe::check::SHIFTED)
    );
}

/// The line's structure check runs the plugin (2026-11-11): the synthetic
/// example line with its lattice type's members given to the example
/// plugin. Each of the two placed structures of that type is checked by
/// one call with all the load cases; the check says the method and what
/// the example leaves out (not on the safe side). Without the plugin
/// installed the check is refused, the plugin named — never passing,
/// never another method.
#[test]
fn the_lines_structure_check_runs_the_plugin_once_per_structure() {
    let _guard = one_at_a_time();
    let ex = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-plugin-check-line-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    for f in std::fs::read_dir(ex.join("Structures")).unwrap() {
        let f = f.unwrap().path();
        if f.is_file() {
            std::fs::copy(&f, dir.join(f.file_name().unwrap())).unwrap();
        }
    }
    let file = dir.join("type-02.ohls");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[analysis]\n"));
    std::fs::write(
        &file,
        text.replace("version = 8", "version = 12").replace(
            "[analysis]\n",
            "[analysis]\ncapacity = \"plugin:member-check-example/example-yield-euler\"\n",
        ),
    )
    .unwrap();
    let run = || {
        let types = ohl_structures::StructureLibrary::load_dir(&dir).unwrap();
        let mut line = ohl_line::Line::load(&ex.join("synthetic-line-typed.ohl.json"))
            .unwrap()
            .resolve(&types, &ohl_structures::InsulatorLibrary::default())
            .unwrap();
        line.creep_stretch.creep_case = Some("everyday".into());
        let library = ohl_cables::Library::load(&ex.join("../../libraries/cables")).unwrap();
        let weather = ohl_weather::WeatherCases::load(&ex.join("weather-cases.ohlwc"))
            .unwrap()
            .cases;
        let cases = ohl_loads::LoadCases::load(&ex.join("load-cases.ohllc"))
            .unwrap()
            .cases;
        let loads = ohl_loads::structure_loads(
            &line,
            &library,
            &weather,
            &cases,
            &ohl_wirefe::default_fe_budget(),
        )
        .unwrap();
        let placed: Vec<String> = line
            .structures
            .iter()
            .filter(|s| s.structure_type.as_deref() == Some("type-02"))
            .map(|s| s.id.clone())
            .collect();
        (
            ohl_loads::capacity::usage(&line, &types, &library, &weather, &loads),
            placed,
        )
    };
    // Not installed: refused by name in every case.
    let (usage, placed) = run();
    assert_eq!(placed.len(), 2);
    assert!(usage.len() > 1, "several load cases");
    for case in &usage {
        for s in case
            .structures
            .iter()
            .filter(|s| placed.contains(&s.structure))
        {
            let fe = s.fe.as_ref().expect("a refused check, not none");
            assert!(fe.usage.is_infinite(), "{fe:?}");
            assert!(
                fe.governs.contains("plugin \"member-check-example\"")
                    && fe.governs.contains("is not installed"),
                "{}",
                fe.governs
            );
        }
    }
    if !install(&dir) {
        std::fs::remove_dir_all(&dir).ok();
        return;
    }
    let before = plugin_check::calls();
    let (usage, placed) = run();
    assert_eq!(
        plugin_check::calls() - before,
        placed.len(),
        "one call per placed structure, whatever the number of cases"
    );
    std::fs::remove_dir_all(&dir).ok();
    for case in &usage {
        for s in case
            .structures
            .iter()
            .filter(|s| placed.contains(&s.structure))
        {
            let fe = s.fe.as_ref().expect("checked by the plugin");
            assert!(fe.usage.is_finite() && fe.usage > 0.0, "{fe:?}");
            assert!(fe.governs.contains("(example)"), "{}", fe.governs);
            // The example flags every result: never a plain pass.
            assert!(fe.not_safe > 0, "{fe:?}");
            assert!(
                s.notes.iter().any(|n| n.contains(
                    "member capacities: method example-yield-euler of the plugin \
                     member-check-example"
                )),
                "{:?}",
                s.notes
            );
        }
    }
}
