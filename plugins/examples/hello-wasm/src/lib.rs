// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Example OLS CAD plugin compiled to a WebAssembly component.
//!
//! Build with:
//!
//! ```bash
//! cargo build --release --target wasm32-wasip2
//! ```

wit_bindgen::generate!({
    world: "plugin",
    path: "../../../crates/ols-plugins/wit",
});

struct HelloPlugin;

impl Guest for HelloPlugin {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "hello-wasm".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Example plugin: total load of a distributed load over a span".into(),
        }
    }

    fn span_load(load_per_meter: f64, span_length: f64) -> f64 {
        load_per_meter * span_length
    }
}

export!(HelloPlugin);
