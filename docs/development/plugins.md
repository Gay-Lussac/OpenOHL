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

## File-format plugins

A plugin that adds import or export formats implements the `io-plugin` world of the same WIT file and lists the extension point `io` in its manifest:

```toml
extension-points = ["io"]
```

- `formats()` lists its formats: id, description, extension and direction (exporter or importer).
- `export-model(format, model, options)` receives the solved model as JSON and returns the file's bytes. The model is `{"format": "ols-export-model", "version": 1, "crs": …, "line": <line file>, "cases": [{"name", "summary", "wires": [{"section", "span", "circuit", "phase", "color", "system", "voltage_kv", "points": [[x, y, z], …]}]}]}` (coordinates in the line's coordinate system; jumpers have section `jumper:<id>`).
- `import-file(format, data, options)` returns a line file (JSON), which the host checks like any line file. When the user gives a line to update, it arrives as option `into` (its JSON).
- Errors are returned as strings and shown to the user as the format's error.

`plugins/examples/geojson` is a complete example (GeoJSON export of structures and wires; structures imported from GeoJSON points). Its formats appear next to the built-in ones in `ols formats --plugin plugins/examples/geojson`; see [Import and export](../user-guide/import-export.md).

## Python plugins

Python plugins are scripts (`tier = "python"`, `entry` the script). Outside the application they use the `ols` package built from `crates/ols-py`; inside it, the Python console runs them (Plugins › Python console › Plugin scripts) with the `ols` module of the application, which also reads and edits the open project ([Python console](../user-guide/desktop-app.md#python-console)). See `plugins/examples/hello-python`.

The desktop application embeds Python through its `python` feature (on by default): building it needs a shared Python 3.10+ (`PYO3_PYTHON` chooses which; a static one, such as Anaconda's, cannot be embedded). Without the feature (`--no-default-features`), the console says so.
