// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The line's values solved at calculation level 3 handed to
//! OHL-Structure (plan of 2026-11-15, phase 5): with the line's level the
//! cases the line sends are solved rigid and at level 3, each a case of
//! the line-values file with its level (`levels`, version 10); written
//! and read back, each case says the level it was solved at. Rigid, the
//! file carries no levels (every case at 2).

use std::path::{Path, PathBuf};

use ohl_line::Line;
use ohl_loads::LoadCases;
use ohl_structure_link::{placed_values_with, structure_case_loads, structure_cases};
use ohl_structure_loads::PlacedValues;
use ohl_structures::StructureLibrary;

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

#[test]
fn level_three_line_values_reach_ohl_structure_with_their_level() {
    let types = StructureLibrary::load_dir(&ex().join("Structures")).unwrap();
    let mut line = Line::load(&ex().join("synthetic-line-typed.ohl.json"))
        .unwrap()
        .resolve(&types, &ohl_structures::InsulatorLibrary::default())
        .unwrap();
    line.creep_stretch.creep_case = Some("everyday".into());
    let library = ohl_cables::Library::load(&ex().join("../../libraries/cables")).unwrap();
    let weather = ohl_weather::WeatherCases::load(&ex().join("weather-cases.ohlwc"))
        .unwrap()
        .cases;
    let cases = LoadCases::load(&ex().join("load-cases.ohllc"))
        .unwrap()
        .cases;
    let budget = ohl_wirefe::default_fe_budget();

    let rigid =
        structure_case_loads(&line, &library, structure_cases(&weather, &cases), &budget).unwrap();
    let rigid = placed_values_with(&rigid, &line, "T2", "synthetic");
    assert!(rigid.levels.is_empty(), "{:?}", rigid.levels);
    assert!(rigid.cases.iter().all(|c| rigid.level_of(c) == 2));

    let sent = structure_cases(&weather, &cases)
        .at_level_three(types, ohl_sections::SectionLibrary::default());
    let at3 = structure_case_loads(&line, &library, sent, &budget).unwrap();
    assert!(at3.library.iter().all(|c| c.level.is_some()));
    let placed = placed_values_with(&at3, &line, "T2", "synthetic");
    assert_eq!(placed.levels.len(), placed.cases.len());
    // Each case twice: the rigid solve and the level-3 solve.
    assert_eq!(
        placed.cases.len(),
        2 * rigid.cases.len(),
        "{:?}",
        placed.cases
    );
    for c in &rigid.cases {
        assert_eq!(placed.level_of(&format!("{c} (rigid)")), 2, "{c}");
        assert_eq!(placed.level_of(&format!("{c} (level 3)")), 3, "{c}");
    }

    // Through the file: version 10, the levels kept.
    let dir = std::env::temp_dir().join(format!("ohl-level3-values-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let at = dir.join("type-02.T2.line-values.json");
    placed.clone().at_needed_version().save(&at).unwrap();
    let back = PlacedValues::load(&at).unwrap();
    assert_eq!(back.levels, placed.levels);
    let first = &rigid.cases[0];
    assert_eq!(back.level_of(&format!("{first} (level 3)")), 3);
    std::fs::remove_dir_all(&dir).ok();
}
