// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plates' and welds' checks per case, on a welded plate (invented
//! data).

use std::collections::BTreeMap;

use crate::joints::joint_details;
use crate::joints_tests::{OTHER_FACE, TOWER, library};
use crate::{PlatePart, plate_plans, plate_usage};

/// A welded plate of G1 (D1 onto it) and G2 (D2), a weld of a = 6 mm
/// given on G2 (`[[plate.weld]]`), its length the seam's.
fn welded() -> String {
    format!(
        "{TOWER}{OTHER_FACE}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         \n[[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
         [[plate.weld]]\nto = \"G1\"\nsize = \"6 mm\"\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"G1\"\n\
         \n[[connection]]\nmember = \"D2\"\njoint = \"N2\"\nonto = \"G2\"\n"
    )
}

fn axial() -> BTreeMap<String, f64> {
    [("D1", 40e3), ("D2", -25e3), ("L1", -100e3), ("L2", -90e3)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

/// Whether the bundled plugin `sia263` is built (else skip, or fail when
/// the plugin tests are required).
fn sia263_built() -> bool {
    let wasm = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plugins/codes/sia263/target/wasm32-wasip2/release/sia263.wasm");
    if wasm.is_file() {
        return true;
    }
    assert!(
        std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugins/codes/sia263 not built"
    );
    eprintln!("skipping: plugins/codes/sia263 not built");
    false
}

/// Whether the bundled plugin `asce10` is built (else skip, or fail when
/// the plugin tests are required).
fn asce10_built() -> bool {
    let wasm = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plugins/codes/asce10/target/wasm32-wasip2/release/asce10.wasm");
    if wasm.is_file() {
        return true;
    }
    assert!(
        std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugins/codes/asce10 not built"
    );
    eprintln!("skipping: plugins/codes/asce10 not built");
    false
}

/// The structure's parameter file for its plugin (`Plugins/sia263.toml`
/// beside it) reaches the joint check and Member details too, not only
/// the member check (owner 2026-11-18; before, they took the standard's
/// values): a γ_M2 of 1.5 instead of 1.25 raises each bolted plate part's
/// usage by 1.5/1.25 where γ_M2 governs it, lowers D1's tension in Member
/// details by that much, and names the file; without the folder (or the
/// file) the standard's values, as before.
#[test]
fn a_changed_factor_changes_the_joint_check_and_member_details() {
    if !sia263_built() {
        return;
    }
    let _guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let sia = format!("{}\n[analysis]\ncapacity = \"sia263-2013\"\n", welded());
    let dir = std::env::temp_dir().join(format!("ohl-sia263-params-{}", std::process::id()));
    let plugins = dir.join(ohl_member_check::BESIDE);
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(
        plugins.join("sia263.toml"),
        "[resistance]\ngamma_m2 = 1.5\n",
    )
    .unwrap();
    let forces = axial();
    let rows = |dir: Option<&std::path::Path>| {
        let plans = crate::plate_plans_in(&sia, &library(), &Default::default(), dir).unwrap();
        crate::plate_usage_cases(&plans, &[("one", 1.0, &forces)]).remove(0)
    };
    let (standard, factored) = (rows(None), rows(Some(&dir)));
    let d1 = |u: &[crate::PlateUsage]| {
        u.iter()
            .find(|p| p.part == PlatePart::Member("D1".into()))
            .cloned()
            .unwrap()
    };
    let (a, b) = (d1(&standard), d1(&factored));
    assert_eq!(a.governs, b.governs, "{a:?} {b:?}");
    assert!(
        a.governs.contains("γ_M2") || a.governs.contains("SIA 263"),
        "{a:?}"
    );
    let ratio = b.usage.unwrap() / a.usage.unwrap();
    assert!((ratio - 1.5 / 1.25).abs() < 1e-9, "{ratio}: {a:?} {b:?}");
    // Member details: the same file, D1's tension (net section or bolts by
    // γ_M2) lower by the same ratio, the file named.
    let c0 = crate::member_capacity_in(&sia, &library(), &Default::default(), "D1", None).unwrap();
    let c1 =
        crate::member_capacity_in(&sia, &library(), &Default::default(), "D1", Some(&dir)).unwrap();
    let (t0, t1) = (c0.tension.unwrap(), c1.tension.unwrap());
    assert!(t1.0 < t0.0, "{t0:?} {t1:?}");
    assert!(
        c1.assumed.iter().any(|n| n.contains("sia263.toml")),
        "{:?}",
        c1.assumed
    );
    assert!(
        c0.assumed
            .iter()
            .any(|n| n.contains("the method's own parameters")),
        "{:?}",
        c0.assumed
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// The bolt tools' force per bolt without an analysis is the plugin's own
/// (third round, 2026-11-18): SIA 263 (the bundled `sia263`) its bolt's
/// design shear resistance by Table 16 with γ_M2 — the parameter file's
/// when there is one (1.5 instead of 1.25: the force × 1.25/1.5) —, ASCE
/// 10-97 (`asce10`) 4.3.2's; Table 17's minimum distances the same for
/// either force; with an analysis its force per bolt.
#[test]
fn the_force_basis_is_the_plugins_own() {
    if !sia263_built() || !asce10_built() {
        return;
    }
    let _guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    use ohl_structures_fe::bolt_rules::Force;
    // D1's two bolts of grade 8.8, f_ub 800 MPa (no shear capacity given).
    let bolted = welded().replace(
        "bolts = { diameter = \"16 mm\", count = 1 }",
        "bolts = { diameter = \"16 mm\", count = 2, ultimate = \"800 MPa\", grade = \"8.8\" }",
    );
    let sia = format!("{bolted}\n[analysis]\ncapacity = \"sia263-2013\"\n");
    let dir = std::env::temp_dir().join(format!("ohl-sia263-basis-{}", std::process::id()));
    let plugins = dir.join(ohl_member_check::BESIDE);
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(
        plugins.join("sia263.toml"),
        "[resistance]\ngamma_m2 = 1.5\n",
    )
    .unwrap();
    let rules = |text: &str, dir: Option<&std::path::Path>, force: Option<f64>| {
        crate::bolt_rules_in(
            text,
            &library(),
            &Default::default(),
            "D1",
            "N2",
            force,
            dir,
        )
        .expect("the plugin's rules")
    };
    let (standard, factored) = (rules(&sia, None, None), rules(&sia, Some(&dir), None));
    let (Force::Capacity(a), Force::Capacity(b)) = (standard.force, factored.force) else {
        panic!("{standard:?} {factored:?}")
    };
    assert!((b / a - 1.25 / 1.5).abs() < 1e-12, "{a} {b}");
    assert!(
        standard.force_basis.contains("SIA 263 Table 16"),
        "{}",
        standard.force_basis
    );
    assert!(
        standard.words().contains("SIA 263 Table 16"),
        "{}",
        standard.words()
    );
    assert_eq!(
        (standard.end, standard.spacing, standard.edge),
        (factored.end, factored.spacing, factored.edge)
    );
    // ASCE 10-97: its own bolt shear (4.3.2), larger than SIA 263's.
    let asce = rules(&bolted, None, None);
    let Force::Capacity(c) = asce.force else {
        panic!("{asce:?}")
    };
    assert!(asce.force_basis.contains("ASCE 10-97 4.3.2"), "{asce:?}");
    assert!(c > a, "{c} {a}");
    // With an analysis: its force per bolt (D1's two bolts).
    let analysed = rules(&sia, None, Some(-40e3));
    assert_eq!(analysed.force, Force::Analysis(20e3));
    std::fs::remove_dir_all(&dir).ok();
}

/// Under SIA 263 — the bundled plugin `sia263`'s joint check since
/// 2026-11-18, one call with every part and case — each member's bolts are
/// checked in its plate (bearing, block shear, sections, distances) and
/// the weld by (87) and (88) under the larger resultant of its two sides
/// (D1's 40 kN against D2's 25 kN); the strength factor scales the
/// resistances.
#[test]
fn plates_and_welds_are_checked_by_sia263() {
    if !sia263_built() {
        return;
    }
    let sia = format!("{}\n[analysis]\ncapacity = \"sia263-2013\"\n", welded());
    let d = joint_details(&sia, &library(), &Default::default()).unwrap();
    let w = &d.joint("N2").unwrap().plates[0].welds[0];
    assert_eq!(w.size, Some(0.006));
    assert_eq!(w.label().as_deref(), Some("a6"));
    let plans = plate_plans(&sia, &library(), &Default::default()).unwrap();
    assert!(plans.plugin.is_some(), "the old name: the bundled plugin");
    let forces = axial();
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &forces), ("half", 0.5, &forces)]);
    let u = &rows[0];
    let d1 = u
        .iter()
        .find(|p| p.part == PlatePart::Member("D1".into()))
        .unwrap();
    assert_eq!(d1.plate, "G1");
    assert!(d1.usage.is_some_and(|x| x > 0.0 && x < 1.0), "{d1:?}");
    assert!(d1.governs.contains("SIA 263"), "{d1:?}");
    let d2 = u
        .iter()
        .find(|p| p.part == PlatePart::Member("D2".into()))
        .unwrap();
    assert!(
        d2.notes
            .iter()
            .any(|n| n.contains("plate buckling not checked"))
    );
    let weld = u
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!((weld.force - 40e3).abs() < 1e-6, "{weld:?}");
    assert!(weld.governs.contains("(87)"), "{weld:?}");
    assert!(weld.notes.iter().any(|n| n.contains("the seam's")));
    let w2 = rows[1]
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!((w2.usage.unwrap() - 2.0 * weld.usage.unwrap()).abs() < 1e-12);
    // The worst per joint.
    let j = crate::joint_plate_usage(u);
    assert!(j["N2"] >= weld.usage.unwrap());
}

/// Under ASCE 10-97 (the default; the bundled plugin `asce10`'s joint
/// check since 2026-11-18) the weld is not checked (the standard has no
/// weld rule), said — NOT CHECKED, NOT ON THE SAFE SIDE; the bolts are;
/// the simple check checks nothing, said.
#[test]
fn asce10_has_no_weld_rule_and_simple_checks_nothing() {
    if !asce10_built() {
        return;
    }
    let plans = plate_plans(&welded(), &library(), &Default::default()).unwrap();
    assert!(plans.plugin.is_some(), "the default: the bundled plugin");
    let forces = axial();
    let u = crate::plate_usage_cases(&plans, &[("one", 1.0, &forces)]).remove(0);
    let weld = u
        .iter()
        .find(|p| matches!(&p.part, PlatePart::Weld(_)))
        .unwrap();
    assert!(
        weld.usage.is_none()
            && weld
                .governs
                .starts_with("NOT CHECKED by asce10, NOT ON THE SAFE SIDE")
            && weld.notes.iter().any(|n| n.contains("AWS D1.1")),
        "{weld:?}"
    );
    assert!(
        u.iter()
            .any(|p| p.usage.is_some() && p.governs.contains("ASCE 10-97")),
        "{u:?}"
    );
    // The core alone (no plugin run): not checked, said.
    assert!(
        plate_usage(&plans, &axial(), 1.0)
            .iter()
            .all(|p| p.usage.is_none())
    );
    let simple = format!("{}\n[analysis]\ncapacity = \"simple\"\n", welded());
    let plans = plate_plans(&simple, &library(), &Default::default()).unwrap();
    assert!(
        plate_usage(&plans, &axial(), 1.0)
            .iter()
            .all(|p| p.usage.is_none() && p.governs.contains("not checked"))
    );
}

/// A double cover splice (2026-11-05): each of L1's covers on its long
/// leg is checked as a plate for its share of L1's force by area — the
/// outer cover across the leg's 61 mm, the inner from the root's 15 mm,
/// 46 mm (both 8 mm thick): 61/107 and 46/107 of it; the short leg's
/// cover holds none of L1's bolts and is not checked for it. (SIA 263:
/// the bundled plugin's joint check.)
#[test]
fn a_splices_covers_share_the_force_by_area() {
    if !sia263_built() {
        return;
    }
    use crate::joints_tests::SPLICED;
    let text = format!("{TOWER}{SPLICED}\n[analysis]\ncapacity = \"sia263-2013\"\n");
    let plans = plate_plans(&text, &library(), &Default::default()).unwrap();
    let mut axial = axial();
    axial.insert("L1".into(), 100e3);
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &axial)]);
    let u = &rows[0];
    let l1: Vec<_> = u
        .iter()
        .filter(|p| p.part == PlatePart::Member("L1".into()))
        .collect();
    assert_eq!(l1.len(), 2, "{l1:?}");
    let outer = l1.iter().find(|p| p.plate.contains("outer")).unwrap();
    let inner = l1.iter().find(|p| p.plate.contains("inner")).unwrap();
    assert_eq!(outer.plate, "S1 (outer, long leg)");
    assert!(
        (outer.force - 100e3 * 61.0 / 107.0).abs() < 1.0,
        "{}",
        outer.force
    );
    assert!((outer.force + inner.force - 100e3).abs() < 1e-6);
    assert!(outer.usage.is_some_and(|x| x > 0.0 && x < 1.0), "{outer:?}");
    assert!(outer.governs.contains("SIA 263"), "{outer:?}");
    assert!(
        outer
            .notes
            .iter()
            .any(|n| n.contains("shared between the covers by area")),
        "{:?}",
        outer.notes
    );
}

/// A folder with the plugin of `source` (under the repository's
/// `plugins/`) in its `Plugins/`, remembered as a place plugins are
/// looked for; none: the plugin is not built (a failure with
/// `OHL_REQUIRE_PLUGIN_TESTS` set).
fn with_plugin(source: &str, name: &str) -> Option<std::path::PathBuf> {
    let from = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../plugins")
        .join(source);
    let wasm = format!(
        "target/wasm32-wasip2/release/{}.wasm",
        name.replace('-', "_")
    );
    if !from.join(&wasm).is_file() {
        assert!(
            std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "plugins/{source} not built"
        );
        eprintln!("skipping: plugins/{source} not built");
        return None;
    }
    let dir = std::env::temp_dir().join(format!("ohl-joint-check-{name}-{}", std::process::id()));
    let to = dir.join("Plugins").join(name);
    std::fs::create_dir_all(to.join(&wasm).parent().unwrap()).unwrap();
    std::fs::copy(from.join("ohl-plugin.toml"), to.join("ohl-plugin.toml")).unwrap();
    std::fs::copy(from.join(&wasm), to.join(&wasm)).unwrap();
    ohl_member_check::roots(Some(&dir));
    Some(dir)
}

/// The tests that call a plugin's joint check, one at a time: one counts
/// the calls.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn by_plugin(plugin: &str, method: &str) -> String {
    format!(
        "{}\n[analysis]\ncapacity = \"plugin:{plugin}/{method}\"\n",
        welded()
    )
}

/// The structure's member check a plugin's method (the example plugin,
/// through the real host; 2026-11-11): the plates' bolts and the weld are
/// checked by its joint check — one call for two load cases —, each row
/// with the plugin's words and what it says is not on the safe side; the
/// weld by the example's throat × length × f_y, and half the strength
/// factor doubles the usage. The bolt tools' minimum distances are the
/// plugin's (the example's 2, 4 and 2 diameters), a pair of bolts closer
/// than its spacing breaks it; the live resistance says where it comes
/// from. No value of ASCE 10-97 anywhere.
#[test]
fn a_plugins_joint_check_and_minimum_distances_are_used() {
    let _guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(dir) = with_plugin("examples/member-check-example", "member-check-example") else {
        return;
    };
    let text = by_plugin("member-check-example", "example-yield-euler");
    let plans = plate_plans(&text, &library(), &Default::default()).unwrap();
    assert!(plans.plugin.is_some());
    let forces = axial();
    let before = ohl_member_check::joints::joint_calls();
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &forces), ("half", 0.5, &forces)]);
    assert_eq!(
        ohl_member_check::joints::joint_calls() - before,
        1,
        "one call for both cases"
    );
    assert_eq!(rows.len(), 2);
    let weld = |n: usize| {
        rows[n]
            .iter()
            .find(|p| matches!(&p.part, PlatePart::Weld(_)))
            .unwrap()
    };
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let seam = d.joint("N2").unwrap().plates[0].welds[0].length();
    let w = weld(0);
    assert!((w.force - 40e3).abs() < 1e-6, "{w:?}");
    assert!(
        w.governs.starts_with("throat × length × f_y (example)")
            && w.governs.contains("[not on the safe side: EXAMPLE METHOD"),
        "{w:?}"
    );
    // The fixture's plates take the members' steel: its yield strength
    // from the usage, the same for both cases.
    let fy = 40e3 / (w.usage.unwrap() * 0.006 * seam);
    assert!(fy > 200e6 && fy < 500e6, "{fy}");
    assert!((weld(1).usage.unwrap() - 2.0 * w.usage.unwrap()).abs() < 1e-12);
    for row in rows.iter().flatten() {
        assert!(
            !row.governs.contains("ASCE") && row.checks.iter().all(|c| !c.0.contains("ASCE")),
            "{row:?}"
        );
        // Checked by the example, or said not checked by it.
        assert!(
            (row.usage.is_some() && row.governs.contains("(example)"))
                || row
                    .governs
                    .starts_with("NOT CHECKED by member-check-example, NOT ON THE SAFE SIDE"),
            "{row:?}"
        );
    }
    let bolts = rows[0]
        .iter()
        .find(|p| p.part == PlatePart::Member("D1".into()))
        .unwrap();
    assert!(bolts.usage.is_some_and(|u| u > 0.0), "{bolts:?}");
    // The bolt tools' rules: the plugin's.
    let r = crate::bolt_rules(&text, &library(), &Default::default(), "D1", "N2", None)
        .expect("the plugin's minimum distances");
    assert!(r.code.contains("member-check-example"), "{}", r.code);
    let dia = r.diameter;
    assert!((r.end - 2.0 * dia).abs() < 1e-12 && (r.spacing - 4.0 * dia).abs() < 1e-12);
    assert!((r.edge - 2.0 * dia).abs() < 1e-12);
    assert_eq!(r.clauses[1], "example: 4 × diameter");
    assert!(crate::no_bolt_rules(&text).is_none());
    let close = crate::breaches(&r, &[[0.1, 0.03], [0.1 + 3.0 * dia, 0.03]], 0.0);
    assert!(
        close
            .iter()
            .any(|b| b.what.contains("example: 4 × diameter")),
        "{close:?}"
    );
    // Member details: the plugin's own capacities, worked out live
    // (2026-11-18), the code named — never another code's.
    let c = crate::member_capacity(&text, &library(), &Default::default(), "D1").unwrap();
    assert_eq!(
        c.code,
        "method example-yield-euler of the plugin member-check-example"
    );
    assert!(
        c.compression
            .as_ref()
            .is_some_and(|x| x.1.contains("(example)")),
        "{c:?}"
    );
    assert!(c.tension.is_some(), "{c:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A plugin with the member check alone (no `joint-check`, no
/// `bolt-distances` export): every plate part and the weld read "NOT
/// CHECKED by the plugin, NOT ON THE SAFE SIDE" without a usage — no silent
/// ASCE 10-97 —, the bolt tools have no rules and say "no minimum
/// distances from the plugin". With a method of the core the same file is
/// checked as before.
#[test]
fn a_plugin_without_a_joint_check_leaves_the_joints_not_checked() {
    let _guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(dir) = with_plugin("tests/member-check-only", "member-check-only") else {
        return;
    };
    let text = by_plugin("member-check-only", "members-only");
    let plans = plate_plans(&text, &library(), &Default::default()).unwrap();
    let forces = axial();
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &forces)]);
    assert!(rows[0].len() >= 3, "{rows:?}");
    for row in &rows[0] {
        assert!(row.usage.is_none() && row.checks.is_empty(), "{row:?}");
        assert!(
            row.governs.starts_with(
                "NOT CHECKED by member-check-only, NOT ON THE SAFE SIDE: the plugin \
                 member-check-only has no joint check"
            ),
            "{row:?}"
        );
    }
    // The bolt tools: the structure's default minimums (third round,
    // 2026-11-18), said as not a code's, with why.
    let r = crate::bolt_rules(&text, &library(), &Default::default(), "D1", "N2", None)
        .expect("the default minimums");
    assert_eq!(r.code, ohl_structures::bolts::DEFAULT_MINIMUM_WORDS);
    assert!((r.end - 1.5 * r.diameter).abs() < 1e-12);
    assert!((r.edge - 1.5 * r.diameter).abs() < 1e-12);
    assert!((r.spacing - 3.0 * r.diameter).abs() < 1e-12);
    let words = r.words();
    assert!(
        words.starts_with("default minimum, assumed, not a code's: end ≥")
            && words.contains("no minimum distances from member-check-only")
            && words.contains("NOT ON THE SAFE SIDE"),
        "{words}"
    );
    assert_eq!(
        crate::no_bolt_rules(&text).as_deref(),
        Some(
            "no minimum distances from member-check-only: the plugin has no bolt-distances export"
        )
    );
    std::fs::remove_dir_all(&dir).ok();
    // The default, the bundled plugin asce10: its rules, no such note.
    assert!(crate::no_bolt_rules(&welded()).is_none());
    assert!(
        crate::bolt_rules(&welded(), &library(), &Default::default(), "D1", "N2", None)
            .is_some_and(|r| r.code == "asce10-97")
    );
}
