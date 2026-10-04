# Contributing to OLS CAD

Thanks for your interest! The project is in its foundation phase — the most valuable contributions right now are reviews of the [roadmap](ROADMAP.md) and [architecture](docs/ARCHITECTURE.md), and validation cases with known reference results.

## Clean-room policy (important)

OLS CAD is an independent implementation. To protect the project:

- **Do not** decompile, disassemble or reverse-engineer any commercial software, or copy its code, file layouts, documentation text or UI assets.
- **Do not** contribute material covered by an NDA or a license agreement you are bound by.
- **Do** base implementations on public sources: standards, peer-reviewed papers, CIGRE/IEEE brochures, textbooks, and your own engineering knowledge. Cite them in the code docstring and the theory manual.
- Comparing *results* with other software for validation is fine; report the inputs and outputs, not the software internals.

## Engineering contributions

Every calculation PR must include:

1. A reference (standard clause, paper, textbook page).
2. Unit tests, and at least one validation case in `benchmarks/` with the expected result and tolerance.
3. A short theory note in `docs/theory/`.

## Development setup

1. Install Rust with [rustup](https://rustup.rs) (the repository pins the `stable` channel in `rust-toolchain.toml`; minimum supported version 1.96).
2. Build and test:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo run -p ols-cli -- info
   ```

3. Before pushing, run the same checks as CI:

   ```bash
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   ```

4. Optional parts:

   ```bash
   # WebAssembly plugins (needed for the plugin host tests)
   rustup target add wasm32-wasip2
   (cd plugins/examples/hello-wasm && cargo build --release --target wasm32-wasip2)
   (cd plugins/examples/geojson && cargo build --release --target wasm32-wasip2)
   (cd plugins/tests/runaway && cargo build --release --target wasm32-wasip2)
   # Python bindings
   pip install maturin pytest && pip install ./crates/ols-py && pytest crates/ols-py/tests
   # Documentation book
   cargo install mdbook && mdbook build   # output in target/book
   ```

Every source file starts with the MPL-2.0 header (copy it from an existing file). Iterative or long-running code must have exit clauses (see docs/development/performance.md).

## Workflow

- Open an issue first for anything non-trivial.
- Branch from `main`, keep PRs focused on one module.
- Labels: `module:*` for the area, `phase:*` for the roadmap phase, `type:*` for the kind of work.
- Architecture changes go through an ADR in `docs/adr/`.
- Desktop app changes follow [the app conventions](docs/development/ui.md) (e.g. menus of options stay open while boxes are ticked).
