// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plugin's WebAssembly interface (`field-preset-plugin` world): the
//! host's records to and from the plain types of [`crate::analysis`].

use crate::{analysis, model, params, report};

wit_bindgen::generate!({
    inline: r#"
        package local:building-cuts;
        world plugin {
            include ohl:plugin/field-preset-plugin@0.1.0;
        }
    "#,
    path: "../../../crates/ohl-plugins/wit",
    world: "local:building-cuts/plugin",
    generate_all,
});

use exports::ohl::plugin::field_presets::{Cut, FieldInput, FieldPreset, OutFile, Outline, Place};

struct BuildingCuts;

impl Guest for BuildingCuts {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "building-cuts".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Buildings near the line: the field's cuts and places of sensitive use"
                .into(),
        }
    }
}

fn to_wit(p: analysis::PresetOut) -> FieldPreset {
    FieldPreset {
        name: p.name,
        description: p.description,
        cuts: p
            .cuts
            .into_iter()
            .map(|c| Cut {
                label: c.label,
                from: c.from,
                to: c.to,
                fraction: Some(c.fraction),
                width: c.width,
                above: c.above,
                step: c.step,
            })
            .collect(),
        points: p
            .points
            .into_iter()
            .map(|q| Place {
                name: q.name,
                x: q.x,
                y: q.y,
                z: None,
                height: Some(q.height),
            })
            .collect(),
        levels_ut: p.levels_ut,
        levels_kv_m: p.levels_kv_m,
        outlines: p
            .outlines
            .into_iter()
            .map(|o| Outline {
                id: o.id,
                ring: o.ring.iter().flat_map(|q| [q[0], q[1]]).collect(),
                ground: o.ground,
                top: o.top,
            })
            .collect(),
        notes: p.notes,
    }
}

fn from_wit(p: FieldPreset) -> analysis::PresetOut {
    analysis::PresetOut {
        name: p.name,
        description: p.description,
        cuts: p
            .cuts
            .into_iter()
            .map(|c| analysis::CutOut {
                label: c.label,
                from: c.from,
                to: c.to,
                fraction: c.fraction.unwrap_or(f64::NAN),
                width: c.width,
                above: c.above,
                step: c.step,
            })
            .collect(),
        points: p
            .points
            .into_iter()
            .map(|q| analysis::PlaceOut {
                name: q.name,
                x: q.x,
                y: q.y,
                height: q.height.unwrap_or(f64::NAN),
            })
            .collect(),
        levels_ut: p.levels_ut,
        levels_kv_m: p.levels_kv_m,
        outlines: p
            .outlines
            .into_iter()
            .map(|o| analysis::OutlineOut {
                id: o.id,
                ring: o.ring.as_chunks::<2>().0.to_vec(),
                ground: o.ground,
                top: o.top,
            })
            .collect(),
        notes: p.notes,
    }
}

impl exports::ohl::plugin::field_presets::Guest for BuildingCuts {
    fn parameters_template() -> String {
        params::TEMPLATE.into()
    }

    fn field_presets(input: FieldInput) -> Result<Vec<FieldPreset>, String> {
        let params = params::Params::parse(&input.parameters)?;
        let model = model::Model::parse(&input.model)?;
        let points: Vec<analysis::InPoint> = input
            .points
            .into_iter()
            .map(|p| analysis::InPoint {
                x: p.x,
                y: p.y,
                z: p.z,
                class: p.class,
                code: p.code,
                line: p.line,
            })
            .collect();
        let presets = analysis::presets(&params, &model, &points, &input.ground)?;
        Ok(presets.into_iter().map(to_wit).collect())
    }

    fn report(
        parameters: String,
        preset: FieldPreset,
        results: String,
    ) -> Result<Vec<OutFile>, String> {
        let params = params::Params::parse(&parameters)?;
        let files = report::report(&params, &from_wit(preset), &results)?;
        Ok(files
            .into_iter()
            .map(|f| OutFile {
                name: f.name,
                media_type: f.media_type,
                bytes: f.bytes,
            })
            .collect())
    }
}

export!(BuildingCuts);
