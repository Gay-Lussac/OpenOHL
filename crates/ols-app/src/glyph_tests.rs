// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Every character the program writes has a glyph in the fonts it uses
//! (egui's default fonts): no □ in a tool icon, an arrow or a symbol.

use eframe::egui;

/// The characters beyond ASCII in the string and character literals of
/// the Rust files in `dir` (comments and test files left out): (file:line,
/// character).
pub fn literal_chars(dir: &std::path::Path) -> Vec<(String, char)> {
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            !name.ends_with("tests.rs") && !name.starts_with("bench")
        })
        .collect();
    files.sort();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut in_string = false;
        for (k, line) in text.lines().enumerate() {
            if !in_string && line.trim_start().starts_with("//") {
                continue;
            }
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if !in_string && c == '/' && chars.get(i + 1) == Some(&'/') {
                    break;
                }
                if !in_string && c == '\'' && chars.get(i + 2) == Some(&'\'') {
                    if !chars[i + 1].is_ascii() {
                        out.push((format!("{name}:{}", k + 1), chars[i + 1]));
                    }
                    i += 3;
                    continue;
                }
                if c == '\\' && in_string {
                    i += 2;
                    continue;
                }
                if c == '"' {
                    in_string = !in_string;
                } else if in_string && !c.is_ascii() {
                    out.push((format!("{name}:{}", k + 1), c));
                }
                i += 1;
            }
        }
    }
    out
}

/// The characters of `chars` without a glyph in egui's default fonts:
/// those drawn as the replacement glyph (as a private-use character, which
/// no font has). (`has_glyph` says no for every character of the first
/// font: it is the one giving the replacement glyph.)
pub fn missing_glyphs(chars: &[(String, char)]) -> Vec<String> {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
    let font = egui::FontId::proportional(14.0);
    let mut out: Vec<String> = Vec::new();
    ctx.fonts_mut(|f| {
        let mut uv = |c: char| {
            let g = f.layout_no_wrap(c.to_string(), font.clone(), egui::Color32::WHITE);
            g.rows
                .first()
                .and_then(|r| r.glyphs.first())
                .map(|g| g.uv_rect)
        };
        let replacement = uv('\u{E000}');
        for (at, c) in chars {
            if uv(*c) == replacement {
                out.push(format!("{at}: {c} (U+{:04X})", *c as u32));
            }
        }
    });
    ctx.tex_manager().write().take_delta().clear();
    out
}

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
    chars.push(("arrow".into(), crate::menu::ARROW.chars().next().unwrap()));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    chars.extend(literal_chars(&dir));
    let missing = missing_glyphs(&chars);
    assert!(missing.is_empty(), "no glyph:\n{}", missing.join("\n"));
}
