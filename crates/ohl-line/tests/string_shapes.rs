// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A string's own volumes (line file version 8, roadmap O6): a line
//! without them is written as version 7 (an older program still reads
//! it); with them as version 8, read back the same; files of version 7 and
//! before read without them. Sizes INVENTED.

use ohl_line::{LINE_FILE_VERSION, Line};
use ohl_structures::{Along, ElectricalState, ShapeKind, StringShape};

fn example() -> Line {
    Line::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/synthetic-line/synthetic-line.ohl.json"
    )))
    .unwrap()
}

#[test]
fn own_shapes_raise_the_version_only_when_given() {
    let mut line = example();
    assert_eq!(line.version, LINE_FILE_VERSION);
    let text = line.to_json();
    assert!(text.contains("\"version\": 7,"), "{}", &text[..80]);
    assert!(!text.contains("\"shapes\""));
    let back = Line::from_json(&text).unwrap();
    assert_eq!(back.version, LINE_FILE_VERSION);
    let st = line
        .structures
        .iter_mut()
        .find(|s| !s.strings.is_empty())
        .unwrap();
    let mut horn = StringShape::new("horn", ShapeKind::Cylinder, ElectricalState::Grounded);
    horn.at = Along::Metres(0.1);
    horn.offset = [0.2, 0.0];
    st.strings[0].shapes.push(horn.clone());
    let text = line.to_json();
    assert!(text.contains("\"version\": 8,"), "{}", &text[..80]);
    let back = Line::from_json(&text).unwrap();
    let again = back
        .structures
        .iter()
        .flat_map(|s| &s.strings)
        .find(|x| !x.shapes.is_empty())
        .unwrap();
    assert_eq!(again.shapes, vec![horn]);
}
