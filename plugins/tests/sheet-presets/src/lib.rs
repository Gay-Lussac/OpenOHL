// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin (`sheet-template-plugin` world): one sheet template from
//! its code — A1 landscape, a title block with an SVG logo the plugin
//! gives, wires by kind, a legend — and one from its manifest (a file, no
//! code: `templates/test-a3.ohlsheet`). Every value is **invented** for
//! the tests (`ohl-plugins`, `ohl-cli`, `ohl-app`): no company's or
//! authority's.

wit_bindgen::generate!({
    world: "sheet-template-plugin",
    path: "../../../crates/ohl-plugins/wit",
});

use exports::ohl::plugin::sheet_templates::{SheetImage, SheetTemplate};

struct Sheets;

/// The template (invented).
const TEMPLATE: &str = r##"format = "ohl-sheet-template"
version = 1
name = "test-a1"
description = "Test: A1, a title block with a logo, wires by kind (invented)"

[page]
format = "A1"

[line_styles]
by_kind = true
[line_styles.kinds.hv]
colour = "#b00000"
label = "test high voltage"

[legend]
place = "above_title_block"
height = "30 mm"

[title_block]
width = "160 mm"
columns = ["40 mm", "*"]
rows = ["14 mm", "8 mm", "8 mm"]
grid = true

[[title_block.cell]]
at = [0, 0]
span = [3, 1]
image = "test-logo.svg"

[[title_block.cell]]
at = [0, 1]
text = "{heading}"
size = "3 mm"
bold = true

[[title_block.cell]]
at = [1, 1]
text = "Sheet {sheet} / {sheets} — order {order_no}"
size = "2.5 mm"

[[title_block.cell]]
at = [2, 1]
text = "{scales}"
size = "2 mm"

[[field]]
key = "order_no"
label = "Order number"
required = true
"##;

/// Its logo (invented).
const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect x="2" y="2" width="16" height="16" fill="#6a1b9a"/><path d="M22 6 H38 M22 14 H34" stroke="#6a1b9a" stroke-width="3"/></svg>"##;

impl Guest for Sheets {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "sheet-presets".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Test plugin: sheet templates (invented)".into(),
        }
    }
}

impl exports::ohl::plugin::sheet_templates::Guest for Sheets {
    fn sheet_templates() -> Result<Vec<SheetTemplate>, String> {
        Ok(vec![SheetTemplate {
            name: "test-a1".into(),
            description: "A1 with a logo, wires by kind (invented test values)".into(),
            template: TEMPLATE.into(),
            images: vec![SheetImage {
                name: "test-logo.svg".into(),
                media_type: "image/svg+xml".into(),
                bytes: LOGO.as_bytes().to_vec(),
            }],
        }])
    }
}

export!(Sheets);
