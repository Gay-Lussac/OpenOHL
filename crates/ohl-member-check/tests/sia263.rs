// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! SIA 263:2013 and ASCE 10-97 as the bundled plugins `sia263` and
//! `asce10` (moved out of the core 2026-11-18, ASCE 10-97 in the second
//! round): found in OpenOHL's own folder, the old names aliases of their
//! methods (ASCE 10-97 also the default, no name given), **the same
//! numbers and words as the core gave before the move** (recorded from the
//! core on the shipped examples), the clauses named, the foot movements'
//! larger usage, the solve's cap by the plugin's own compression capacity.
//! The plugins are built for wasm32-wasip2 by the full check; without them
//! these tests skip (fail with `OHL_REQUIRE_PLUGIN_TESTS`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ohl_structures_fe::{Loads, Model};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether the bundled plugins are built (else skip, or fail when the
/// plugin tests are required).
fn built() -> bool {
    let missing: Vec<&str> = ["sia263", "asce10"]
        .into_iter()
        .filter(|p| {
            !repo()
                .join(format!(
                    "plugins/codes/{p}/target/wasm32-wasip2/release/{p}.wasm"
                ))
                .is_file()
        })
        .collect();
    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "plugins/codes/{missing:?} not built"
    );
    eprintln!("skipping: plugins/codes/{missing:?} not built");
    false
}

#[test]
fn the_old_name_is_the_bundled_plugins_method() {
    let m = Model::parse(
        "[analysis]\ncapacity = \"sia263-2013\"\n[[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\n\
         [[section]]\nid = \"a\"\nmaterial = \"s\"\narea = \"1 cm2\"\niy = \"1 cm4\"\niz = \"1 cm4\"\n\
         [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
         [[node]]\nid = \"B\"\npoint = [1, 0, 0]\nsupport = \"pinned\"\n\
         [[member]]\nid = \"m\"\nnodes = [\"A\", \"B\"]\nsection = \"a\"\n",
    )
    .unwrap();
    let p = m.capacity_plugin.as_ref().expect("a plugin's method");
    assert_eq!(
        (p.plugin.as_str(), p.method.as_str()),
        ("sia263", "sia263-2013")
    );
    // Written back by its old name: no file changes.
    assert_eq!(p.name(), "sia263-2013");
    assert!(ohl_member_check::is_bundled("sia263"));
    assert_eq!(m.analysis.capacity, ohl_structures_fe::CapacityMethod::Code);
    // ASCE 10-97: the default (no name given) and its old name, both the
    // bundled plugin asce10's method; the file keeps what it says.
    for (name, written) in [
        ("", "asce10-97"),
        ("[analysis]\ncapacity = \"asce10-97\"\n", "asce10-97"),
    ] {
        let m = Model::parse(&format!(
            "{name}[[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\n\
             [[section]]\nid = \"a\"\nmaterial = \"s\"\narea = \"1 cm2\"\niy = \"1 cm4\"\n\
             iz = \"1 cm4\"\n[[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
             [[node]]\nid = \"B\"\npoint = [1, 0, 0]\nsupport = \"pinned\"\n\
             [[member]]\nid = \"m\"\nnodes = [\"A\", \"B\"]\nsection = \"a\"\n"
        ))
        .unwrap();
        let p = m.capacity_plugin.as_ref().expect("the bundled asce10");
        assert_eq!(
            (p.plugin.as_str(), p.method.as_str()),
            ("asce10", "asce10-97")
        );
        assert_eq!(p.name(), written);
        assert_eq!(m.analysis.capacity, ohl_structures_fe::CapacityMethod::Code);
    }
    assert!(ohl_member_check::is_bundled("asce10"));
    // The simple check stays the core's: no plugin.
    let m = Model::parse(
        "[analysis]\ncapacity = \"simple\"\n[[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\n\
         [[section]]\nid = \"a\"\nmaterial = \"s\"\narea = \"1 cm2\"\niy = \"1 cm4\"\niz = \"1 cm4\"\n\
         [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
         [[node]]\nid = \"B\"\npoint = [1, 0, 0]\nsupport = \"pinned\"\n\
         [[member]]\nid = \"m\"\nnodes = [\"A\", \"B\"]\nsection = \"a\"\n",
    )
    .unwrap();
    assert!(m.capacity_plugin.is_none());
    assert_eq!(
        m.analysis.capacity,
        ohl_structures_fe::CapacityMethod::Simple
    );
}

#[test]
fn the_bundled_plugin_loads() {
    if !built() {
        return;
    }
    assert_eq!(ohl_member_check::check_bundled(), Ok(()));
    let root = ohl_member_check::bundled_root().expect("found");
    assert!(root.ends_with("plugins/codes"), "{}", root.display());
}

/// One structure file checked as the core's comparison of 2026-11-18 did
/// it (variants and load cases alike): its lines `file|variant|case|
/// member|usage bits|governs`; checked by the bundled plugin named by
/// the old name `plugin`.
fn lines(path: &Path, sections: &ohl_sections::SectionLibrary, plugin: &str, out: &mut String) {
    for variant in ["base", "redundant", "angles", "no-compression"] {
        let mut model = Model::load_with(path, sections).unwrap();
        match variant {
            "redundant" => model.analysis.redundant_force = Some(0.025),
            "angles" => model.analysis.single_angle_moments = true,
            "no-compression" => {
                if !model
                    .members
                    .iter()
                    .any(|m| m.kind == ohl_structures_fe::MemberKind::Tension)
                {
                    continue;
                }
                model.analysis.tension_compression = ohl_structures_fe::TensionCompression::None
            }
            _ => {}
        }
        // As the comparison set it: the method by its old name.
        model.capacity_plugin = Some(
            ohl_structures_fe::model::PluginMethod::parse(plugin)
                .unwrap()
                .unwrap(),
        );
        let model = ohl_member_check::find(model, path.parent()).unwrap();
        let runs = [
            ("A", [6e3, 0.0, 12e3], 1.0),
            ("B", [0.0, 8e3, 10e3], 1.0),
            ("C", [-15e3, -3e3, 20e3], 0.9),
            ("D", [40e3, 10e3, 60e3], 1.0),
        ];
        let mut solved = Vec::new();
        for (name, [t, l, v], strength) in runs {
            let mut nodal = Vec::new();
            if model.joint_points.is_empty() {
                nodal.extend(model.loads.iter().cloned());
            }
            for p in &model.joint_points {
                if let Some(at) = model.point_load(&p.label, [t, l, -v]) {
                    nodal.extend(at.loads);
                }
            }
            let loads = Loads {
                nodal,
                member: Vec::new(),
                self_weight: 1.0,
                prescribed: Vec::new(),
            };
            if let Ok((s, _)) = ohl_structures_fe::analyse(
                &model,
                &loads,
                strength,
                &ohl_structures_fe::default_budget(),
            ) {
                solved.push((name, strength, s));
            }
        }
        let list: Vec<ohl_member_check::Solved> = solved
            .iter()
            .map(|(n, st, s)| ohl_member_check::Solved {
                name: n,
                strength: *st,
                self_weight: 1.0,
                solution: s,
                moved: None,
            })
            .collect();
        let usages = ohl_member_check::check(&model, &list)
            .expect("a plugin")
            .expect("checked")
            .usage;
        for ((name, _, _), usage) in solved.iter().zip(&usages) {
            for u in usage {
                let _ = writeln!(
                    out,
                    "{}|{variant}|{name}|{}|{}|{}",
                    path.file_name().unwrap().to_string_lossy(),
                    model.members[u.member].id,
                    u.usage
                        .map_or("none".to_owned(), |x| format!("{:016x}", x.to_bits())),
                    u.governs
                );
            }
        }
    }
}

/// FNV-1a, 64 bits.
fn fnv(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The shipped examples' towers checked by the plugin give, bit for bit
/// and word for word, what the core's SIA 263 gave before the move
/// (recorded 2026-11-18 from the core at commit 01cc57ec: 2 256 member
/// results, every usage's bits and what governs; the hash of those
/// lines). Recorded again the same day (the lattice towers' safe-side
/// items): 6 results of the variant with `single_angle_moments` changed —
/// an angle's moments about its principal axes, dropped before, now taken
/// (each larger: `every-profile.ohls` m-EA-2 and m-UA in cases A, B, C;
/// the other 2 250 the same). Recorded again on 2026-11-19 (buckling
/// lengths from the bracing): every usage the same to the bit; what
/// governs a member whose lengths are now worked out says so ("L …, from
/// the bracing").
#[test]
fn the_examples_give_the_cores_numbers() {
    if !built() {
        return;
    }
    let sections = ohl_sections::SectionLibrary::load(&repo().join("libraries/sections")).unwrap();
    let mut out = String::new();
    for f in [
        "examples/synthetic-line/Structures/type-02.ohls",
        "examples/exports/every-profile.ohls",
    ] {
        lines(&repo().join(f), &sections, "sia263-2013", &mut out);
    }
    assert_eq!(out.lines().count(), 2256);
    assert_eq!(fnv(&out), 0xc897_c15d_0593_ec69, "{out}");
}

/// The same examples by ASCE 10-97 (the default), now its bundled plugin
/// `asce10` (2026-11-18): the numbers the core's own ASCE 10-97 gave,
/// recorded at commit 01cc57ec before the SIA 263 move (and unchanged by
/// it) — the same hash, every usage's bits and what governs. Recorded
/// again on 2026-11-19 (buckling lengths from the bracing): every usage
/// the same to the bit; what governs a member whose lengths are now
/// worked out says so ("KL/r … (v over …, from the bracing)").
#[test]
fn the_examples_keep_the_cores_asce10_numbers() {
    if !built() {
        return;
    }
    let sections = ohl_sections::SectionLibrary::load(&repo().join("libraries/sections")).unwrap();
    let mut out = String::new();
    for f in [
        "examples/synthetic-line/Structures/type-02.ohls",
        "examples/exports/every-profile.ohls",
    ] {
        lines(&repo().join(f), &sections, "asce10-97", &mut out);
    }
    assert_eq!(out.lines().count(), 2256);
    assert_eq!(fnv(&out), 0x7b52_6b28_e37a_9a62, "{out}");
}

fn library() -> ohl_sections::SectionLibrary {
    ohl_sections::SectionLibrary::parse(
        "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\nshear_modulus = \"81 GPa\"\n\
         yield = \"355 MPa\"\nultimate = \"510 MPa\"\ndensity = \"7850 kg/m3\"\n\
         [[section]]\nid = \"I200\"\nshape = \"i_section\"\nmaterial = \"steel\"\n\
         h = \"200 mm\"\nb = \"100 mm\"\ntw = \"5.6 mm\"\ntf = \"8.5 mm\"\nr = \"12 mm\"\n\
         [[section]]\nid = \"2L\"\nshape = \"double_angle\"\nmaterial = \"steel\"\n\
         h = \"60 mm\"\nb = \"60 mm\"\nt = \"6 mm\"\nr = \"8 mm\"\ngap = \"10 mm\"\n",
    )
    .unwrap()
}

/// Solves `text` (its own loads), the members checked by the plugin;
/// `moved`: also on its supports moved by these displacements.
fn run(text: &str, moved: Option<[f64; 3]>) -> (Model, ohl_member_check::Checked) {
    let m = Model::parse_with(text, &library()).unwrap();
    let m = ohl_member_check::find(m, None).unwrap();
    let loads = Loads {
        nodal: m.loads.clone(),
        self_weight: 1.0,
        ..Default::default()
    };
    let budget = ohl_structures_fe::default_budget();
    let (plain, _) = ohl_structures_fe::analyse(&m, &loads, 1.0, &budget).unwrap();
    let with = moved.map(|d| {
        let support = m
            .nodes
            .iter()
            .position(|n| n.fixed.iter().any(|f| *f))
            .unwrap();
        let moved = Loads {
            prescribed: vec![ohl_structures_fe::solve::Prescribed {
                node: support,
                displacement: [d[0], d[1], d[2], 0.0, 0.0, 0.0],
            }],
            ..loads.clone()
        };
        ohl_structures_fe::analyse(&m, &moved, 1.0, &budget)
            .unwrap()
            .0
    });
    let c = ohl_member_check::check(
        &m,
        &[ohl_member_check::Solved {
            name: "case",
            strength: 1.0,
            self_weight: 1.0,
            solution: &plain,
            moved: with.as_ref(),
        }],
    )
    .expect("a plugin")
    .expect("checked");
    (m, c)
}

fn portal() -> String {
    "[analysis]\ncapacity = \"sia263-2013\"\n\
     [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"fixed\"\n\
     [[node]]\nid = \"B\"\npoint = [0, 0, 4]\n\
     [[node]]\nid = \"C\"\npoint = [6, 0, 4]\n\
     [[node]]\nid = \"D\"\npoint = [6, 0, 0]\nsupport = \"fixed\"\n\
     [[member]]\nid = \"c1\"\nnodes = [\"A\", \"B\"]\nsection = \"I200\"\nkind = \"frame\"\n\
     [[member]]\nid = \"beam\"\nnodes = [\"B\", \"C\"]\nsection = \"I200\"\nkind = \"frame\"\n\
     [[member]]\nid = \"c2\"\nnodes = [\"D\", \"C\"]\nsection = \"I200\"\nkind = \"frame\"\n\
     [[load]]\nnode = \"B\"\nforce = [\"20 kN\", \"5 kN\", \"-30 kN\"]\n"
        .to_owned()
}

/// The clauses SIA 263 does not cover in the program, named on the
/// members (the core's test of 2026-11-08, moved with the code): a frame's
/// shear and torsion; a compressed built-up member's battens.
#[test]
fn clauses_not_covered_are_named_by_the_plugin() {
    if !built() {
        return;
    }
    let (m, c) = run(&portal(), None);
    let k = m.members.iter().position(|x| x.id == "c2").unwrap();
    let g = &c.usage[0][k].governs;
    assert!(
        g.contains("(SIA 263 5.1.4, 5.1.7)") && !g.contains("ASCE"),
        "{g}"
    );
    assert!(
        c.note.contains("SIA 263:2013") && c.note.contains("plugin sia263"),
        "{}",
        c.note
    );
    let truss = "[analysis]\nnonlinear = false\nself_weight = false\ncapacity = \"sia263-2013\"\n\
                 [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
                 [[node]]\nid = \"B\"\npoint = [4, 0, 0]\nsupport = \"pinned\"\n\
                 [[node]]\nid = \"C\"\npoint = [2, 0, 1]\n\
                 [[member]]\nid = \"a\"\nnodes = [\"A\", \"C\"]\nsection = \"2L\"\n\
                 [[member]]\nid = \"b\"\nnodes = [\"B\", \"C\"]\nsection = \"2L\"\n\
                 [[member]]\nid = \"tie\"\nnodes = [\"A\", \"B\"]\nsection = \"2L\"\n\
                 [[load]]\nnode = \"C\"\nforce = [0, 0, \"-10 kN\"]\n";
    let (m, c) = run(truss, None);
    let k = m.members.iter().position(|x| x.id == "a").unwrap();
    assert!(c.usage[0][k].governs.contains("(SIA 263 5.5)"));
}

/// With foot movements each member keeps the larger usage of with and
/// without them (as the core's own methods), in one call.
#[test]
fn foot_movements_keep_the_larger_usage() {
    if !built() {
        return;
    }
    let (_, plain) = run(&portal(), None);
    let (_, both) = run(&portal(), Some([0.0, 0.0, -0.02]));
    let mut larger = 0;
    for (p, b) in plain.usage[0].iter().zip(&both.usage[0]) {
        let (p, b) = (p.usage.unwrap(), b.usage.unwrap());
        assert!(b >= p, "never below the plain solve: {b} {p}");
        if b > p {
            larger += 1;
        }
    }
    assert!(larger > 0, "a settlement of 20 mm loads the portal");
}

/// A tension-only member's compression is capped in the solve by the
/// plugin's own compression capacity (asked when the plugin is found).
#[test]
fn the_solves_cap_is_the_plugins_compression() {
    if !built() {
        return;
    }
    let text = "[analysis]\ncapacity = \"sia263-2013\"\n\
                [[node]]\nid = \"A\"\npoint = [0, 0, 0]\nsupport = \"pinned\"\n\
                [[node]]\nid = \"B\"\npoint = [3, 0, 0]\nsupport = \"pinned\"\n\
                [[member]]\nid = \"t\"\nnodes = [\"A\", \"B\"]\nsection = \"2L\"\nkind = \"tension\"\n";
    let m = Model::parse_with(text, &library()).unwrap();
    assert!(m.method_compression.is_empty());
    let m = ohl_member_check::find(m, None).unwrap();
    let cap = m.method_compression[0].expect("the plugin's compression capacity");
    assert!(cap > 0.0);
    assert_eq!(
        ohl_structures_fe::check::design_compression(&m, 0),
        Some(cap)
    );
    let given = ohl_member_check::capacities(&m).unwrap();
    assert!(given[0].capacities.iter().any(|c| c.0 == "tension"));
}
