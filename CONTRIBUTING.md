# Contributing to OpenOHL

Thanks for your interest! The project is pre-alpha — the most valuable contributions right now are validation cases with known reference results, reviews of the theory manual (`docs/theory/`), the [roadmap](ROADMAP.md) and the [architecture](docs/ARCHITECTURE.md), and reports from trying it on real lines.

## Clean-room policy (important)

OpenOHL is an independent implementation. To protect the project:

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
   cargo run -p ohl-cli -- info
   ```

3. Before pushing, run the same checks as CI (the CI workflow is started by hand, so these local checks are what keeps `main` green):

   ```bash
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo run -p ohl-cli -- validate   # the validation cases in benchmarks/
   ```

4. Optional parts:

   ```bash
   # WebAssembly plugins (needed for the plugin host tests)
   rustup target add wasm32-wasip2
   (cd plugins/examples/hello-wasm && cargo build --release --target wasm32-wasip2)
   (cd plugins/examples/geojson && cargo build --release --target wasm32-wasip2)
   (cd plugins/tests/runaway && cargo build --release --target wasm32-wasip2)
   (cd plugins/examples/esri-grid && cargo build --release --target wasm32-wasip2)
   (cd plugins/examples/project-bundle && cargo build --release --target wasm32-wasip2)
   for d in plugins/codes/*/; do [ -f "$d/ohl-plugin.toml" ] && (cd "$d" && cargo build --release --target wasm32-wasip2); done
   # Python bindings
   pip install maturin pytest && pip install ./crates/ohl-py && pytest crates/ohl-py/tests
   # Documentation book
   cargo install mdbook && mdbook build   # output in target/book
   ```

Every source file starts with the licence header (copy it from an existing file): AGPL-3.0-or-later for the program, MPL-2.0 for the plugin interface and the plugins under `plugins/` and `crates/ohl-plugins/wit/`. Iterative or long-running code must have exit clauses (see docs/development/performance.md).

## Workflow

- Open an issue first for anything non-trivial.
- Branch from `main`, keep PRs focused on one module.
- Labels: `module:*` for the area, `phase:*` for the roadmap phase, `type:*` for the kind of work.
- Architecture changes go through an ADR in `docs/adr/`.
- Desktop app changes follow [the app conventions](docs/development/ui.md) (e.g. menus of options stay open while boxes are ticked).
- Checks that depend on a structure's shape follow [the structure detail levels](docs/development/structure-detail.md): their behaviour for detail 1, 2 and 3 is decided with the project owner first.
