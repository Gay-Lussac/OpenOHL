# Writing plugins

OLS CAD has three plugin tiers (ADR-0003): **WebAssembly** components (stable interface, sandboxed, any language), **Python** (scripts, reports, import/export) and native Rust inside this repository.

Every plugin has an `ols-plugin.toml` manifest:

```toml
[plugin]
name = "hello-wasm"
version = "0.1.0"
description = "Example plugin"
sdk = "0.1"                 # plugin interface version targeted
tier = "wasm"               # "wasm" or "python"
entry = "target/wasm32-wasip2/release/hello_wasm.wasm"
extension-points = []

[permissions]               # everything denied by default
network = false
filesystem = []
```

## WebAssembly plugins

The interface is defined in WIT: `crates/ols-plugins/wit/plugin.wit`. A Rust example lives in `plugins/examples/hello-wasm`:

```bash
rustup target add wasm32-wasip2
cd plugins/examples/hello-wasm && cargo build --release --target wasm32-wasip2
cargo run -p ols-cli -- plugin info plugins/examples/hello-wasm
```

Plugins run sandboxed — no file system, network, environment variables or standard I/O — with **exit clauses** on every call: a time limit (default 5 s; a runaway plugin is interrupted and reported as a timeout) and a memory cap (default 256 MiB). `plugins/tests/runaway` is a deliberately endless plugin used to test this.

## Python plugins

Python plugins use the `ols` module built from `crates/ols-py`. See `plugins/examples/hello-python`. Running Python plugins from inside the application is planned; today they run as scripts.
