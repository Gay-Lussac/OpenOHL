// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The structure application's formats refuse a key they do not know,
//! naming it, as every library format does (`crates/ols-cli/tests/
//! format_typos.rs` covers the core's): the example structure load-case
//! file, and the `[[load_case]]` tables of an example structure file
//! (which the core keeps unread).

use std::path::{Path, PathBuf};

use ols_structure_loads::StructureLoadCases;
use ols_structures::{LibraryItem, StructureType};

fn ex() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line")
}

/// `text` with `typo_key = 1` added after the line starting with
/// `after` (its first one).
fn with_typo(text: &str, after: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.trim_start().starts_with(after))
        .unwrap_or_else(|| panic!("no line {after}"));
    lines.insert(at + 1, "typo_key = 1");
    lines.join("\n")
}

type Parse = fn(&str) -> Result<(), String>;

fn in_structure(text: &str) -> Result<(), String> {
    let s = StructureType::parse(Path::new("x.ohls"), text).map_err(|e| e.to_string())?;
    StructureLoadCases::from_structure(&s).map(drop)
}

#[test]
fn structure_load_cases_refuse_a_mistyped_key() {
    let file = std::fs::read_to_string(ex().join("structure-load-cases.ohlslc")).unwrap();
    StructureLoadCases::parse(&file).unwrap();
    let structure = std::fs::read_to_string(ex().join("Structures/type-02.ohls")).unwrap();
    in_structure(&structure).unwrap();
    let cases: [(String, Parse); 5] = [
        (with_typo(&file, "version"), |t| {
            StructureLoadCases::parse(t).map(drop)
        }),
        (with_typo(&file, "[[load_case]]"), |t| {
            StructureLoadCases::parse(t).map(drop)
        }),
        (with_typo(&file, "[[load_case.load]]"), |t| {
            StructureLoadCases::parse(t).map(drop)
        }),
        (with_typo(&structure, "[[load_case]]"), in_structure),
        (with_typo(&structure, "[[load_case.load]]"), in_structure),
    ];
    for (k, (text, parse)) in cases.iter().enumerate() {
        let e = parse(text).expect_err(&format!("case {k}: a typo read"));
        assert!(e.contains("typo_key"), "case {k}: {e}");
    }
}
