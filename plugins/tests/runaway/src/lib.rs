// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Test plugin whose `span-load` never returns. Used to prove the host's
//! time limit interrupts endless loops (`ols-plugins` tests).

wit_bindgen::generate!({
    world: "plugin",
    path: "../../../crates/ols-plugins/wit",
});

struct Runaway;

impl Guest for Runaway {
    fn describe() -> PluginInfo {
        PluginInfo {
            name: "runaway".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "Never returns from span-load".into(),
        }
    }

    fn span_load(load_per_meter: f64, _span_length: f64) -> f64 {
        let mut x = load_per_meter;
        loop {
            x = core::hint::black_box(x + 1.0);
        }
    }
}

export!(Runaway);
