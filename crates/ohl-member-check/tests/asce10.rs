// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! ASCE 10-97, the default member check, as the bundled plugin `asce10`
//! (moved out of the core 2026-11-18): the core's tests of what its ASCE
//! 10-97 check said, moved with it and run through the plugin — the
//! clauses not covered named on the members they apply to, single angles
//! checked axially by default; and the plugin's settings (none: a
//! parameter file with a value refused, named). The plugins are built for
//! wasm32-wasip2 by the full check; without them these tests skip (fail
//! with `OHL_REQUIRE_PLUGIN_TESTS`).

use std::path::{Path, PathBuf};

use ohl_structures_fe::{Loads, MemberUsage, Model};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether the bundled plugin is built (else skip, or fail when the plugin
/// tests are required).
fn built() -> bool {
    if repo()
        .join("plugins/codes/asce10/target/wasm32-wasip2/release/asce10.wasm")
        .is_file()
    {
        return true;
    }
    assert!(
        std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugins/codes/asce10 not built"
    );
    eprintln!("skipping: plugins/codes/asce10 not built");
    false
}

const LIBRARY: &str = "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\n\
    shear_modulus = \"81 GPa\"\ndensity = \"7850 kg/m3\"\nyield = \"355 MPa\"\n\
    [[section]]\nid = \"I200\"\nshape = \"i_section\"\nmaterial = \"steel\"\n\
    h = \"200 mm\"\nb = \"100 mm\"\ntw = \"5.6 mm\"\ntf = \"8.5 mm\"\nr = \"12 mm\"\n\
    [[section]]\nid = \"2L\"\nshape = \"double_angle\"\nmaterial = \"steel\"\n\
    h = \"60 mm\"\nb = \"60 mm\"\nt = \"6 mm\"\nr = \"8 mm\"\ngap = \"10 mm\"\n\
    [[section]]\nid = \"L100\"\nshape = \"equal_angle\"\nmaterial = \"steel\"\n\
    h = \"200 mm\"\nt = \"20 mm\"\n\
    [[section]]\nid = \"T200\"\nshape = \"circular_hollow\"\nmaterial = \"steel\"\n\
    d = \"200 mm\"\nt = \"8 mm\"\n";

/// A frame portal of `section` (two columns and a beam, fixed feet),
/// loaded at a top corner; `extra`: more of `[analysis]`.
fn portal(section: &str, extra: &str) -> String {
    format!(
        "[analysis]\nnonlinear = true\n{extra}\
         [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
         [[node]]\nid = \"B\"\npoint = [0, 0, 6]\n\
         [[node]]\nid = \"C\"\npoint = [5, 0, 6]\n\
         [[node]]\nid = \"D\"\npoint = [5, 0, 0]\nsupport = \"fixed\"\n\
         [[member]]\nid = \"c1\"\nnodes = [\"A\", \"B\"]\nsection = \"{section}\"\nkind = \"frame\"\n\
         [[member]]\nid = \"beam\"\nnodes = [\"B\", \"C\"]\nsection = \"{section}\"\nkind = \"frame\"\n\
         [[member]]\nid = \"c2\"\nnodes = [\"D\", \"C\"]\nsection = \"{section}\"\nkind = \"frame\"\n\
         [[load]]\nnode = \"B\"\nforce = [\"20 kN\", \"5 kN\", \"-30 kN\"]\n"
    )
}

/// `text` solved under its own loads (`weight`: the self-weight factor),
/// its members checked by its code's plugin (found as a run finds it,
/// `dir` the structure's folder).
fn run(text: &str, weight: f64, dir: Option<&Path>) -> (Model, Vec<MemberUsage>) {
    let lib = ohl_sections::SectionLibrary::parse(LIBRARY).unwrap();
    let m = Model::parse_with(text, &lib).unwrap();
    let m = ohl_member_check::find(m, dir).unwrap();
    let loads = Loads {
        nodal: m.loads.clone(),
        self_weight: weight,
        ..Default::default()
    };
    let (s, _) =
        ohl_structures_fe::analyse(&m, &loads, 1.0, &ohl_structures_fe::default_budget()).unwrap();
    let c = ohl_member_check::check(
        &m,
        &[ohl_member_check::Solved {
            name: "own loads",
            strength: 1.0,
            self_weight: weight,
            solution: &s,
            moved: None,
        }],
    )
    .expect("a plugin")
    .expect("checked");
    (m, c.usage.into_iter().next().unwrap())
}

fn governs(m: &Model, u: &[MemberUsage], id: &str) -> String {
    let k = m.members.iter().position(|x| x.id == id).unwrap();
    u.iter().find(|x| x.member == k).unwrap().governs.clone()
}

/// The clauses not covered, named on the members they apply to
/// (2026-11-08; the core's test, moved with the code): the portal's
/// columns (compressed frames) of an I section name the effective widths,
/// the amplification, the lateral-torsional buckling and the shear; two
/// angles back to back as trusses name their stitch bolts; a compression
/// resistance given on the member covers the compression clauses.
#[test]
fn clauses_not_covered_are_named_on_the_members() {
    if !built() {
        return;
    }
    let (m, u) = run(&portal("I200", ""), 1.0, None);
    let g = governs(&m, &u, "c2");
    for clause in ["3.9.3", "3.12, 3.13", "3.14", "3.15"] {
        assert!(
            g.contains(&format!("(ASCE 10-97 {clause})")),
            "{clause}: {g}"
        );
    }
    assert!(ohl_structures_fe::check::not_safe(&g));
    // Two angles back to back as trusses: a two-bar truss, one compressed.
    let truss = |given: &str| {
        "[analysis]\nnonlinear = false\nself_weight = false\n\
         [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
         [[node]]\nid = \"B\"\npoint = [4, 0, 0]\nsupport = \"pinned\"\n\
         [[node]]\nid = \"C\"\npoint = [2, 0, 1]\n\
         [[member]]\nid = \"a\"\nnodes = [\"A\", \"C\"]\nsection = \"2L\"\nGIVEN\
         [[member]]\nid = \"b\"\nnodes = [\"B\", \"C\"]\nsection = \"2L\"\nGIVEN\
         [[member]]\nid = \"tie\"\nnodes = [\"A\", \"B\"]\nsection = \"2L\"\n\
         [[load]]\nnode = \"C\"\nforce = [0, 0, \"-10 kN\"]\n"
            .replace("GIVEN", given)
    };
    let (m, u) = run(&truss(""), 0.0, None);
    assert!(
        governs(&m, &u, "a")
            .contains("stitch bolts of the built-up member not checked (ASCE 10-97 3.11)")
    );
    assert!(
        !governs(&m, &u, "tie").contains("3.11"),
        "in tension: not said"
    );
    let (m, u) = run(&truss("capacity_compression = \"50 kN\"\n"), 0.0, None);
    assert!(
        !governs(&m, &u, "a").contains("3.11"),
        "{}",
        governs(&m, &u, "a")
    );
}

/// A portal of single angles (frames): by default each member is checked
/// for its axial force only (tower practice, 2026-10-27), its end moments
/// said with what they would add — not on the safe side (2026-11-08);
/// `single_angle_moments = true` adds the end moments. A tube portal keeps
/// its interaction either way (the core's test, moved with the code).
#[test]
fn single_angles_are_checked_axially_by_default() {
    if !built() {
        return;
    }
    let (_, axial) = run(&portal("L100", ""), 0.0, None);
    let (_, both) = run(&portal("L100", "single_angle_moments = true\n"), 0.0, None);
    for (a, b) in axial.iter().zip(&both) {
        assert!(
            !a.governs.contains("+ My") && b.governs.contains("+ My"),
            "{a:?} {b:?}"
        );
        assert!(a.usage.unwrap() < b.usage.unwrap());
        assert!(
            ohl_structures_fe::check::not_safe(&a.governs) && a.governs.contains("would add"),
            "{a:?}"
        );
        assert!(!b.governs.contains("would add"), "{b:?}");
    }
    let (_, tube) = run(&portal("T200", ""), 0.0, None);
    assert!(tube.iter().all(|u| u.governs.contains("My")), "{tube:?}");
}

/// ASCE 10-97 takes no parameters: a parameter file `Plugins/asce10.toml`
/// with a value in it is refused, named (never silently left out); an
/// empty one is the method's own.
#[test]
fn a_parameter_file_with_values_is_refused() {
    if !built() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("ohl-asce10-params-{}", std::process::id()));
    let plugins = dir.join(ohl_member_check::BESIDE);
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("asce10.toml"), "# nothing to set\n").unwrap();
    let (m, u) = run(&portal("T200", ""), 0.0, Some(&dir));
    assert!(u.iter().all(|x| x.usage.is_some()), "{u:?}");
    assert!(m.capacity_plugin.as_ref().unwrap().parameters.is_some());
    std::fs::write(
        plugins.join("asce10.toml"),
        "[resistance]\ngamma_m1 = 1.1\n",
    )
    .unwrap();
    let lib = ohl_sections::SectionLibrary::parse(LIBRARY).unwrap();
    let m = Model::parse_with(&portal("T200", ""), &lib).unwrap();
    let m = ohl_member_check::find(m, Some(&dir)).unwrap();
    let s = ohl_structures_fe::analyse(
        &m,
        &Loads {
            nodal: m.loads.clone(),
            ..Default::default()
        },
        1.0,
        &ohl_structures_fe::default_budget(),
    )
    .unwrap()
    .0;
    let e = ohl_member_check::check(
        &m,
        &[ohl_member_check::Solved {
            name: "own loads",
            strength: 1.0,
            self_weight: 0.0,
            solution: &s,
            moved: None,
        }],
    )
    .expect("a plugin")
    .unwrap_err();
    assert!(
        e.contains("takes no parameters") && e.contains("resistance"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Buckling lengths from what braces a member, through the plugin
/// (2026-11-19, `member-check-bracing`): an X panel of L 100 angles whose
/// diagonals are split at their crossing joint, pushed sideways. The
/// compressed half takes its lengths from the bracing (said: the other
/// diagonal pulling, the crossing holds it, Appendix B Example 7), and
/// the core's remark on an unbraced crossing is not added. With `k = 0.5`
/// given: its own length, not from the bracing.
#[test]
fn lengths_from_the_bracing_through_the_plugin() {
    if !built() {
        return;
    }
    let panel = |extra: &str| {
        format!(
            "[analysis]\nnonlinear = false\nself_weight = false\n\
             [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
             [[node]]\nid = \"B\"\npoint = [4, 0, 0]\nsupport = \"pinned\"\n\
             [[node]]\nid = \"C\"\npoint = [0, 0, 4]\nsupport = [\"y\"]\n\
             [[node]]\nid = \"D\"\npoint = [4, 0, 4]\nsupport = [\"y\"]\n\
             [[node]]\nid = \"M\"\npoint = [2, 0, 2]\n\
             [[member]]\nid = \"c1\"\nnodes = [\"A\", \"C\"]\nsection = \"L100\"\n\
             [[member]]\nid = \"c2\"\nnodes = [\"B\", \"D\"]\nsection = \"L100\"\n\
             [[member]]\nid = \"top\"\nnodes = [\"C\", \"D\"]\nsection = \"L100\"\n\
             [[member]]\nid = \"d1a\"\nnodes = [\"A\", \"M\"]\nsection = \"L100\"\n{extra}\
             [[member]]\nid = \"d1b\"\nnodes = [\"M\", \"D\"]\nsection = \"L100\"\n{extra}\
             [[member]]\nid = \"d2a\"\nnodes = [\"B\", \"M\"]\nsection = \"L100\"\n{extra}\
             [[member]]\nid = \"d2b\"\nnodes = [\"M\", \"C\"]\nsection = \"L100\"\n{extra}\
             [[load]]\nnode = \"C\"\nforce = [\"-50 kN\", 0, 0]\n"
        )
    };
    let (m, u) = run(&panel(""), 0.0, None);
    let compressed = ["d1a", "d1b", "d2a", "d2b"]
        .into_iter()
        .find(|id| governs(&m, &u, id).starts_with("compression"))
        .expect("a compressed half");
    let g = governs(&m, &u, compressed);
    assert!(g.contains("from the bracing"), "{g}");
    assert!(!g.contains("buckling length its own"), "{g}");
    let (m, u) = run(&panel("k = 0.5\n"), 0.0, None);
    let g = governs(&m, &u, compressed);
    assert!(!g.contains("from the bracing"), "{g}");
}
