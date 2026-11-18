// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! SIA 263:2013 as the bundled plugin `sia263` (moved out of the core
//! 2026-11-18): found in OpenOHL's own folder, the old name an alias of
//! its method, **the same numbers and words as the core gave before the
//! move** (recorded from the core on the shipped examples), its clauses
//! named, the foot movements' larger usage, the solve's cap by its own
//! compression capacity. The plugin is built for wasm32-wasip2 by the full
//! check; without it these tests skip (fail with
//! `OHL_REQUIRE_PLUGIN_TESTS`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ohl_structures_fe::{Loads, Model};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Whether the bundled plugin is built (else skip, or fail when the
/// plugin tests are required).
fn built() -> bool {
    let wasm = repo().join("plugins/codes/sia263/target/wasm32-wasip2/release/sia263.wasm");
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
    assert_eq!(
        m.analysis.capacity,
        ohl_structures_fe::CapacityMethod::Asce10
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
/// member|usage bits|governs`.
fn lines(path: &Path, sections: &ohl_sections::SectionLibrary, out: &mut String) {
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
            ohl_structures_fe::model::PluginMethod::parse("sia263-2013")
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
        let c = ohl_member_check::check(&model, &list)
            .expect("a plugin")
            .expect("checked");
        for ((name, _, _), usage) in solved.iter().zip(&c.usage) {
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
/// lines).
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
        lines(&repo().join(f), &sections, &mut out);
    }
    assert_eq!(out.lines().count(), 2256);
    assert_eq!(fnv(&out), 0x8b2d_471e_2dd6_7097, "{out}");
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
