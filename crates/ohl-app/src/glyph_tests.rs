// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Every character the program writes has a glyph in the fonts it uses
//! (egui's default fonts): no □ in a tool icon, an arrow or a symbol.

use ohl_ui::glyphs::{literal_chars, missing_glyphs};

/// The tool icons, the arrows and every other character of the program's
/// texts have a glyph.
#[test]
fn every_text_character_has_a_glyph() {
    let mut chars: Vec<(String, char)> = crate::tools::Tool::ALL
        .into_iter()
        .flat_map(|t| {
            t.icon()
                .chars()
                .map(move |c| (format!("icon of {}", t.name()), c))
        })
        .collect();
    // The arrow of the texts ("from ➡ to").
    chars.push(("arrow".into(), '➡'));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    chars.extend(literal_chars(&dir));
    let missing = missing_glyphs(&chars);
    assert!(missing.is_empty(), "no glyph:\n{}", missing.join("\n"));
}
