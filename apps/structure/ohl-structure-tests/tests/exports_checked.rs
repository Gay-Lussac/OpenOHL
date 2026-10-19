// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The structure exports checked with free tools
//! (`docs/development/structure-exports.md`, "Checks with free tools"):
//!
//! - the sample exports in `examples/exports/` are what the exporter
//!   writes now (`OHL_UPDATE_EXPORT_SAMPLES=1` rewrites them);
//! - with `OHL_IFC_PYTHON` pointing to a Python that has IfcOpenShell,
//!   the IFC4, IFC2x3 and SAF exports of the example lattice structure,
//!   the every-profile sample and a body built by the generators are
//!   checked by `tools/check_structure_exports.py` (IfcOpenShell's schema
//!   and rule validation, content and references); skipped otherwise.

use std::path::{Path, PathBuf};
use std::process::Command;

use ohl_sections::SectionLibrary;
use ohl_structure_edit::{
    ArmShape, Bracing, Crossarm, Generator, Kind, Panel, Panels, Peak, apply, edits, groups,
    set_group_section,
};
use ohl_structure_export::Format;
use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn library() -> SectionLibrary {
    SectionLibrary::load(&root().join("libraries/sections/example-sections.ohlsec")).unwrap()
}

fn read(doc: &DocumentMut) -> StructureType {
    StructureType::parse(Path::new("g.ohls"), &doc.to_string()).unwrap()
}

/// A body made by the generators (X panels with horizontals, triangular
/// crossarms, a peak), library sections per member group, pinned feet
/// and one load at the peak.
fn generated() -> String {
    let mut doc: DocumentMut =
        "format = \"ohl-structure\"\nversion = 2\nid = \"generated\"\nname = \"Generated body (invented)\"\nkind = \"lattice\"\n"
            .parse()
            .unwrap();
    let g = |id: &str, kind| Generator {
        id: id.into(),
        kind,
    };
    let panels = Kind::Panels(Panels {
        bottom: None,
        feet: [6.0, 6.0],
        waist: [1.6, 1.6],
        panels: [6.0, 12.0, 17.0, 21.0]
            .into_iter()
            .map(|top| Panel {
                top,
                bracing: Bracing::X,
                horizontal: true,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let t = read(&doc);
    apply(&mut doc, &t, &g("body", panels)).unwrap();
    for x in [-5.0, 5.0] {
        let t = read(&doc);
        edits::add_point(&mut doc, &t, [x, 0.0, 19.0]);
    }
    let t = read(&doc);
    let labels = t.points.iter().map(|p| p.label.clone()).collect();
    let arms = Kind::Crossarm(Crossarm {
        points: labels,
        shape: ArmShape::Triangular,
        ..Default::default()
    });
    apply(&mut doc, &t, &g("arms", arms)).unwrap();
    let t = read(&doc);
    let peak = Kind::Peak(Peak {
        points: Vec::new(),
        height: 3.0,
    });
    apply(&mut doc, &t, &g("peak", peak)).unwrap();
    let t = read(&doc);
    for gr in groups(&t) {
        let s = if gr.name == "body-leg" {
            "UA-83x52x6"
        } else {
            "EA-61x7"
        };
        set_group_section(&mut doc, &t, &gr.name, Some(s));
    }
    let t = read(&doc);
    for n in t.nodes.iter().filter(|n| n.point[2].si().abs() < 1e-6) {
        edits::set_support(&mut doc, &n.id, Some("pinned"));
    }
    let t = read(&doc);
    let top = t
        .nodes
        .iter()
        .max_by(|a, b| a.point[2].si().total_cmp(&b.point[2].si()))
        .unwrap();
    format!(
        "{doc}\n[[load]]\nnode = \"{}\"\nforce = [\"2 kN\", \"5 kN\", \"-10 kN\"]\n",
        top.id
    )
}

/// The formats checked, with their file names.
const FORMATS: [(Format, &str); 4] = [
    (Format::Ifc, "ifc4.ifc"),
    (Format::IfcMembers, "ifc4-members.ifc"),
    (Format::Ifc2x3, "ifc2x3.ifc"),
    (Format::Saf, "saf.xlsx"),
];

/// The sample files for the owner (examples/exports/README.md).
const SAMPLES: [(Format, &str); 3] = [
    (Format::Ifc, "every-profile-ifc4.ifc"),
    (Format::Ifc2x3, "every-profile-ifc2x3.ifc"),
    (Format::Saf, "every-profile-saf.xlsx"),
];

#[test]
fn the_sample_exports_are_current() {
    let dir = root().join("examples/exports");
    let text = std::fs::read_to_string(dir.join("every-profile.ohls")).unwrap();
    let s = ohl_structure_export::structure(&text, &library(), None).unwrap();
    let update = std::env::var_os("OHL_UPDATE_EXPORT_SAMPLES").is_some();
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("export-samples");
    std::fs::create_dir_all(&tmp).unwrap();
    for (format, name) in SAMPLES {
        let out = if update {
            dir.join(name)
        } else {
            tmp.join(name)
        };
        ohl_structure_export::write(&s, format, &out, 0).unwrap();
        let now = std::fs::read(&out).unwrap();
        let kept = std::fs::read(dir.join(name)).unwrap();
        assert!(
            now == kept,
            "examples/exports/{name} differs from the exporter's output: \
             rerun with OHL_UPDATE_EXPORT_SAMPLES=1 and check it again"
        );
    }
}

#[test]
fn exports_pass_the_free_tools() {
    let Some(python) = std::env::var_os("OHL_IFC_PYTHON") else {
        eprintln!("skipping: OHL_IFC_PYTHON (a Python with IfcOpenShell) not set");
        return;
    };
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("exports-checked");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let examples = root().join("examples");
    let structures = [
        (
            "type-02",
            std::fs::read_to_string(examples.join("synthetic-line/Structures/type-02.ohls"))
                .unwrap(),
        ),
        (
            "every-profile",
            std::fs::read_to_string(examples.join("exports/every-profile.ohls")).unwrap(),
        ),
        ("generated", generated()),
    ];
    let library = library();
    for (id, text) in &structures {
        let s = ohl_structure_export::structure(text, &library, None).unwrap();
        for (format, suffix) in FORMATS {
            ohl_structure_export::write(&s, format, &dir.join(format!("{id}-{suffix}")), 0)
                .unwrap();
        }
    }
    let out = Command::new(python)
        .arg("-I")
        .arg(root().join("tools/check_structure_exports.py"))
        .arg(&dir)
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&out.stdout);
    eprintln!("{report}{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "the checks found errors (above)");
    assert!(report.contains(&format!(
        "{} files checked, 0 with errors",
        3 * FORMATS.len()
    )));
}
