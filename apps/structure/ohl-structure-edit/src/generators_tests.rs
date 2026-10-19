// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The bodies the generators built before redundants, plan bracing,
//! widths per panel and one arm for several points were added: written
//! from files with every generator kind and pattern, they must stay the
//! same, character for character (`generators_reference.txt`, written
//! by the code before those options; `OHL_WRITE_REFERENCE=1` writes it
//! again).

use std::path::Path;

use ohl_structures::{LibraryItem, StructureType};
use toml_edit::DocumentMut;

use crate::rebuild::rebuild_all;

const HEAD: &str =
    "format = \"ohl-structure\"\nversion = 3\nid = \"G\"\nname = \"g\"\nkind = \"lattice\"\n";

/// Files with generator records (and points), built in file order.
fn cases() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for bracing in ["single", "x", "k", "diamond", "warren"] {
        for horizontal in [true, false] {
            out.push((
                "patterns",
                format!(
                    "[[generator]]\nid = \"body\"\nkind = \"panels\"\nbottom = \"0 m\"\n\
                     feet = [\"7 m\", \"5 m\"]\nwaist = \"2 m\"\npanel = [\n\
                     {{ top = \"4 m\", bracing = \"{bracing}\", horizontal = {horizontal} }},\n\
                     {{ top = \"9 m\", bracing = \"{bracing}\", horizontal = {horizontal} }},\n\
                     {{ top = \"13 m\", bracing = \"{bracing}\", horizontal = {horizontal} }},\n]\n"
                ),
            ));
        }
    }
    out.push((
        "mixed",
        "[[generator]]\nid = \"body\"\nkind = \"panels\"\nfeet = \"8 m\"\nwaist = \"2 m\"\n\
         panel = [\n{ top = \"6 m\", bracing = \"k\", horizontal = false },\n\
         { top = \"11 m\", bracing = \"diamond\" },\n{ top = \"15 m\", bracing = \"x\" },\n\
         { top = \"18 m\", bracing = \"warren\", horizontal = false },\n]\n"
            .to_owned(),
    ));
    for shape in ["triangular", "rectangular"] {
        out.push((
            "arms",
            format!(
                "[[point]]\nlabel = \"A1\"\npoint = [\"6 m\", \"0 m\", \"7.5 m\"]\n\
                 [[point]]\nlabel = \"A2\"\npoint = [\"-6 m\", \"0 m\", \"10 m\"]\n\
                 [[point]]\nlabel = \"A3\"\npoint = [\"4 m\", \"0 m\", \"7.5 m\"]\n\
                 [[point]]\nlabel = \"A4\"\npoint = [\"0 m\", \"5 m\", \"6 m\"]\n\
                 [[point]]\nlabel = \"E1\"\npoint = [\"0.5 m\", \"0 m\", \"13 m\"]\n\
                 [[generator]]\nid = \"body\"\nkind = \"panels\"\nbottom = \"0 m\"\n\
                 feet = \"6 m\"\nwaist = \"2 m\"\npanel = [\n\
                 {{ top = \"5 m\", bracing = \"x\" }},\n{{ top = \"10 m\", bracing = \"x\" }},\n]\n\
                 [[generator]]\nid = \"arm\"\nkind = \"crossarm\"\n\
                 points = [\"A1\", \"A2\", \"A3\", \"A4\"]\nshape = \"{shape}\"\n\
                 [[generator]]\nid = \"peak\"\nkind = \"peak\"\npoints = [\"E1\"]\n\
                 [[generator]]\nid = \"top\"\nkind = \"peak\"\nheight = \"2 m\"\n"
            ),
        ));
    }
    out.push((
        "copy",
        "[[node]]\nid = \"N1\"\npoint = [\"0 m\", \"-2 m\", \"5 m\"]\n\
         [[generator]]\nid = \"body\"\nkind = \"panels\"\nbottom = \"0 m\"\nfeet = \"6 m\"\n\
         waist = \"2 m\"\npanel = [\n{ top = \"5 m\", bracing = \"x\" },\n\
         { top = \"10 m\", bracing = \"x\" },\n]\n|\
         [[member]]\nid = \"H1\"\nnodes = [\"N1\", \"body-L0-3\"]\n\
         [[member]]\nid = \"H2\"\nnodes = [\"N1\", \"body-L0-4\"]\n\
         [[generator]]\nid = \"front\"\nkind = \"copy_face\"\nbody = \"body\"\nface = \"y-\"\n"
            .to_owned(),
    ));
    out
}

/// Every case built: the files as written.
fn listing() -> String {
    let mut text = String::new();
    for (name, records) in cases() {
        // Stages split by `|`: each added, then every generator built.
        let mut doc: DocumentMut = HEAD.parse().unwrap();
        for stage in records.split('|') {
            doc = format!("{doc}{stage}").parse().unwrap();
            let t = StructureType::parse(Path::new("g.ohls"), &doc.to_string()).unwrap();
            rebuild_all(&mut doc, &t).unwrap();
        }
        text.push_str(&format!("### {name}\n{doc}\n"));
    }
    text
}

#[test]
fn bodies_built_without_the_new_options_are_unchanged() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generators_reference.txt");
    let now = listing();
    if std::env::var_os("OHL_WRITE_REFERENCE").is_some() {
        std::fs::write(&path, &now).unwrap();
    }
    let before = std::fs::read_to_string(&path).unwrap();
    assert!(now == before, "the generated bodies changed");
}
