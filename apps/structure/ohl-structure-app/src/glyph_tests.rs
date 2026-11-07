// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Every character the program writes has a glyph in the fonts it uses
//! (egui's default fonts): no □ in an arrow or a symbol (as in
//! OHL-Design).

use ohl_ui::glyphs::{literal_chars, missing_glyphs};

/// Every character of the program's texts has a glyph (no □).
#[test]
fn every_text_character_has_a_glyph() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let missing = missing_glyphs(&literal_chars(&dir));
    assert!(missing.is_empty(), "no glyph:\n{}", missing.join("\n"));
}
