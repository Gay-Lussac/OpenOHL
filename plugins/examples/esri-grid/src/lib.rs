// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Example OpenOHL terrain plugin (`terrain-plugin` world): an ESRI ASCII
//! grid (`.asc`: `ncols`, `nrows`, `xllcorner`/`xllcenter`,
//! `yllcorner`/`yllcenter`, `cellsize`, optional `nodata_value`, then the
//! rows from north to south) as a surface: a point per cell centre, two
//! triangles per square of four cells with data.
//!
//! Build with:
//!
//! ```bash
//! cargo build --release --target wasm32-wasip2
//! ```

wit_bindgen::generate!({
    world: "terrain-plugin",
    path: "../../../crates/ohl-plugins/wit",
});

use ohl::plugin::io_types::Direction;

struct Grid;

fn read(text: &str) -> Result<Surface, String> {
    let mut words = text.split_whitespace();
    let mut header = std::collections::HashMap::new();
    let mut first_value = None;
    // Header: key value pairs until the first number.
    while let Some(w) = words.next() {
        if w.parse::<f64>().is_ok() {
            first_value = Some(w);
            break;
        }
        let v = words
            .next()
            .ok_or_else(|| format!("header {w}: no value"))?
            .parse::<f64>()
            .map_err(|e| format!("header {w}: {e}"))?;
        header.insert(w.to_ascii_lowercase(), v);
    }
    let get = |k: &str| header.get(k).copied().ok_or_else(|| format!("header: no {k}"));
    let (ncols, nrows) = (get("ncols")? as usize, get("nrows")? as usize);
    let cell = get("cellsize")?;
    if ncols < 2 || nrows < 2 || !(cell > 0.0) {
        return Err("at least 2 × 2 cells of a positive size".into());
    }
    // Cell centres.
    let x0 = match header.get("xllcenter") {
        Some(x) => *x,
        None => get("xllcorner")? + cell / 2.0,
    };
    let y0 = match header.get("yllcenter") {
        Some(y) => *y,
        None => get("yllcorner")? + cell / 2.0,
    };
    let nodata = header.get("nodata_value").copied();
    let values: Vec<f64> = first_value
        .into_iter()
        .chain(words)
        .map(|w| w.parse::<f64>().map_err(|e| format!("value {w}: {e}")))
        .collect::<Result<_, _>>()?;
    if values.len() != ncols * nrows {
        return Err(format!(
            "{} values for {ncols} × {nrows} cells",
            values.len()
        ));
    }
    let mut index = vec![u32::MAX; ncols * nrows];
    let mut points = Vec::new();
    for r in 0..nrows {
        for c in 0..ncols {
            let z = values[r * ncols + c];
            if nodata.is_some_and(|n| z == n) {
                continue;
            }
            index[r * ncols + c] = (points.len() / 3) as u32;
            // Rows run from north to south.
            points.extend([x0 + c as f64 * cell, y0 + (nrows - 1 - r) as f64 * cell, z]);
        }
    }
    let mut triangles = Vec::new();
    for r in 0..nrows - 1 {
        for c in 0..ncols - 1 {
            let [a, b, d, e] = [
                index[r * ncols + c],
                index[r * ncols + c + 1],
                index[(r + 1) * ncols + c],
                index[(r + 1) * ncols + c + 1],
            ];
            if a != u32::MAX && d != u32::MAX && e != u32::MAX {
                triangles.extend([a, d, e]);
            }
            if a != u32::MAX && e != u32::MAX && b != u32::MAX {
                triangles.extend([a, e, b]);
            }
        }
    }
    Ok(Surface {
        name: String::new(),
        points,
        triangles,
    })
}

impl Guest for Grid {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "esri-grid".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "ESRI ASCII grid elevation models as surfaces".into(),
        }
    }

    fn formats() -> Vec<Format> {
        vec![Format {
            id: "esri-ascii-grid".into(),
            description: "ESRI ASCII grid (.asc): a point per cell centre".into(),
            extension: "asc".into(),
            direction: Direction::Importer,
        }]
    }

    fn import_surfaces(
        format: String,
        data: Vec<u8>,
        _options: Vec<(String, String)>,
    ) -> Result<Vec<Surface>, String> {
        if format != "esri-ascii-grid" {
            return Err(format!("unknown format {format}"));
        }
        let text = std::str::from_utf8(&data).map_err(|e| e.to_string())?;
        read(text).map(|s| vec![s])
    }
}

export!(Grid);
