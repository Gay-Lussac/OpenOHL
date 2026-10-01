# ADR-0002: Tech stack

- **Status:** Accepted
- **Date:** 2026-10-01

## Context
Goal: faster and more user-friendly than existing tools (DESIGN_GOALS §1). Core calculations must be fast, multithreaded and memory-safe; the platform must run on Windows, macOS and Linux and be scriptable.

## Options considered
1. Python + NumPy/SciPy — easy to contribute, but slow in loops and hard to multithread.
2. C++ — fast, but memory-unsafe and harder for contributors.
3. **Rust** — C++-level speed, memory/thread safety, modern tooling (Cargo), good WASM and Python interop.

## Decision
- **Rust** (stable, edition 2024), single Cargo workspace, one crate per module.
- Minimum supported Rust version (MSRV): follows the MSRV of `wasmtime` (about three releases behind latest stable); currently 1.96. OLS CAD is an application, so tracking recent Rust is acceptable.
- Geometry: `glam` / `nalgebra`; large dense & sparse linear algebra (FE): `faer`.
- Parallelism: `rayon` in engines; engines are synchronous. `tokio` only for networking.
- Units: SI `f64` internally; typed quantities (`Newtons`, `NewtonsPerMeter`, …) at the public API; conversion only at I/O.
- Coordinates: `f64` in the project CRS; `proj` for transformations.
- Bindings: PyO3 + `maturin` (Python), `wasmtime` (WASM plugins).
- GUI toolkit: **deferred** to the start of Phase 3 (prototype comparison: Slint, egui, Qt/cxx-qt).
- Quality: clippy, rustfmt, `cargo test`, `proptest`, `criterion` benchmarks with CI regression gate.

## Consequences
- Contributors need Rust for core work; Python and WASM tiers keep the barrier low for plugins and scripts.
- Compile times are higher than Python; mitigated by crate splitting and CI caching.
