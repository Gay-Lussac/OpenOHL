# Contributing to OpenOHL

Thanks for your interest! OpenOHL is **pre-alpha**: everything in it is
built as a base and in active development, nothing is approved for
engineering use yet. The most valuable contributions right now are:

- **validation cases** with known reference results (`benchmarks/`, see
  [Validation cases](docs/development/validation.md));
- **reviews** of the theory manual (`docs/theory/`), the
  [roadmap](ROADMAP.md) and the [architecture](docs/ARCHITECTURE.md);
- **reports from trying it** on lines you know (the results, not the
  line's data: see *Data* below).

> Until 2027-04-21 the repository takes issues, comments and pull
> requests from collaborators only (a GitHub interaction limit while the
> project settles). To take part, ask the maintainer to add you.

## Ground rules

- **Language.** The program is Rust. Python is for the bindings
  (`crates/ohl-py`), Python plugins and the scripts in `tools/` (standard
  library only, run with `python3 -I`). The web client and the docs need
  no JavaScript framework; nothing is fetched from a third-party server.
- **Licence.** Contributions to the program are AGPL-3.0-or-later; the
  plugin interface (`crates/ohl-plugins/wit/`) and the plugins under
  `plugins/` are MPL-2.0. Every source file starts with its licence header
  (copy it from a neighbouring file).
- **Clean-room** (important). OpenOHL is an independent implementation:
  - **do not** decompile, disassemble or reverse-engineer commercial
    software, or copy its code, file layouts, documentation text, icons or
    other assets;
  - **do not** contribute material covered by an NDA or a licence
    agreement you are bound by;
  - **do** work from public sources — standards, peer-reviewed papers,
    CIGRE/IEEE brochures, textbooks, your own engineering knowledge — and
    cite them in the code and the theory manual;
  - comparing *results* with other software for validation is fine:
    report the inputs and outputs, not the software's internals. New
    public texts do not name other programs.
- **No engineering tables from memory.** Every coefficient, factor and
  table value comes from a cited source or from the user's data (a
  design-code plugin reads them from the user's parameter file; the public
  plugins ship none). Example values are invented and say so.
- **Data.** Never commit a real line's data, names, places, coordinates,
  survey or photos, nor anyone's personal details. Examples are invented
  (`examples/`). Screenshots for the docs are made as the user `tester`
  (the status bar shows the login name; the screenshot helper refuses
  otherwise): `USER=tester OHL_RENDER_PNG=DIR cargo test …`.
- **Quality before speed.** Speed work may make what is *drawn* coarser,
  never a computed number. A change that makes something faster keeps its
  results the same, and a test says so.
- **Never hang, never guess.** Iterative or long-running code has exit
  clauses (`ohl_core::budget`: an iteration limit, a time limit,
  cancellation, and an error that says why it stopped) and a documented
  expected duration ([Performance](docs/development/performance.md)). A
  calculation that does not converge says so; it never returns a number
  it is not sure of. A result computed from incomplete input (a wire that
  did not solve) is marked incomplete, never a pass.
- **Every edit is a command.** The line is changed only through the
  project session's commands (`crates/ohl-session`), so every edit can be
  undone, logged, shared through the server and solved again. The
  windows build commands; they never change the model themselves.
- **Layers.** The engine comes first and works headless: every
  calculation is usable from the command line (`ohl …`) and covered by
  validation cases before it gets a window. OHL-Structure's crates
  (`apps/structure/`) stay apart from the core; `ohl-structure-link` is the
  one crate that joins them. Plugins stay independent of the core: what a
  plugin's calculation needs from the engine is agreed first, never added
  as a special case.
- **Honest about approximations.** Every simplification, conservative
  assumption or unsafe-side choice is marked as such in the theory page
  and, where a user meets it, in the app's notes.
- **Style.** `cargo fmt`; `cargo clippy -- -D warnings`; code that reads
  like the code around it (comment density, naming, idiom). Comments say
  *why*. Docs in British English, plain words.
- **Commits.** Small and focused, a subject line that says what changed
  for the user.

## Checklists

### A calculation

1. **Reference first**: the standard clause, paper or textbook page it
   follows, in the code's doc comment.
2. **The engine**: in the lowest crate that fits, headless, deterministic,
   with exit clauses; its inputs and outputs plain data.
3. **Unit tests**, and at least one **validation case** in
   `benchmarks/<module>/` with the expected result, its tolerance and its
   source (a hand derivation is a source when it is written out).
4. **A theory note** in `docs/theory/`: what is computed, the method, the
   assumptions (conservative ones marked), what is left out.
5. **The command line** (`crates/ohl-cli`) before the window.
6. **Performance**: an expected duration; a benchmark if it is in an
   interactive path.

### An edit of the line

1. A session **command** (`crates/ohl-session`): what it changes, what
   follows (renames carry their references along), what it refuses and
   why.
2. **Undo and redo** tested; a refused command changes nothing.
3. Only the sections the edit touches are solved again, with the same
   result as a full solve (tested).
4. The window or tool builds the command; one user action is one step of
   undo.

### A file format

1. Old files keep reading. A new version number is written **only** when a
   file uses what is new, so older programs still read the rest (they say
   "newer version" otherwise).
2. Unknown keys are refused, naming them (`deny_unknown_fields`).
3. **Round-trip** and malformed-input tests.
4. The [data model map](docs/development/data-model.md) updated with the
   field, its unit, its default and since when; the schema
   (`docs/schema/`) for the line's XML.

### The desktop applications

- Follow [the app conventions](docs/development/ui.md): one name per
  object ([Glossary](docs/user-guide/glossary.md)); menus of options stay
  open while boxes are ticked; long errors and reports in scrolling boxes
  of a fixed height; windows can always be closed.
- UI text uses only characters the app's font has (the `glyph_tests`
  fail otherwise).
- Checks that depend on a structure's shape follow
  [the structure detail levels](docs/development/structure-detail.md):
  their behaviour at levels 1, 2 and 3 is decided with the maintainer
  first.
- **UI tests** with `egui_kittest`, driving the window as a user would
  (see `crates/ohl-app/src/tests/`); the tutorial's test
  (`tests/tutorial.rs`) must keep passing — change the tutorial with the
  app.
- Look at it: render it (`OHL_RENDER_PNG`) and attach before and after
  images to the pull request.

### A design-code plugin

- Every value from the user's parameter file; none shipped.
- Its presets (rules, weather cases, sheet templates) are data the user
  can see and change (Plugins › Plugin manager › Presets…).
- No change to the core for one code: what is missing in the engine is
  proposed as a general feature first.

### Documentation

- The user guide (`docs/user-guide/`) changes with the behaviour; the
  [decision log](docs/development/decisions.md) records choices made and
  why; the [roadmap](ROADMAP.md) its status.
- Public texts say "in active development", never "done" or "validated":
  only the maintainer marks a feature approved.

## Development setup

1. Install Rust with [rustup](https://rustup.rs) (the repository pins the
   `stable` channel in `rust-toolchain.toml`; minimum supported version
   1.96).
2. Build and test:

   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo run -p ohl-cli -- info
   ```

3. Before pushing, run what CI runs (CI is started by hand, so these
   local checks are what keeps `main` green):

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --exclude ohl-py --all-targets -- -D warnings
   cargo clippy -p ohl-app --no-default-features --all-targets -- -D warnings
   cargo test --workspace --exclude ohl-py
   RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --exclude ohl-py
   mdbook build
   cargo run -p ohl-cli -- validate benchmarks
   python3 -I tools/check_doc_links.py
   ```

   `OHL_REQUIRE_PLUGIN_TESTS=1` and `OHL_REQUIRE_GPU_TESTS=1` make the tests
   that need the built plugins or a GPU fail instead of being skipped.

4. Optional parts:

   ```bash
   # WebAssembly plugins (needed for the plugin host tests): every plugin
   rustup target add wasm32-wasip2
   for d in plugins/*/*/; do [ -f "$d/Cargo.toml" ] && (cd "$d" && cargo build --release --target wasm32-wasip2); done
   # Python bindings
   pip install maturin pytest && pip install ./crates/ohl-py && pytest crates/ohl-py/tests
   # The documentation book
   cargo install mdbook && mdbook build   # output in target/book
   ```

   Build folders grow large: `sh tools/prune-target.sh target` removes
   what a build no longer needs. Debug builds keep line tables only
   (backtraces with lines; `CARGO_PROFILE_DEV_DEBUG=true` for a debugger
   session with variables); for local timing runs, `--profile bench-fast`
   builds much faster than `--release` (see
   [Build profiles](docs/development/performance.md#build-profiles)).

## Workflow

- Open an issue first for anything non-trivial (bug, feature and
  validation templates in `.github/ISSUE_TEMPLATE/`).
- Branch from `main`; keep a pull request to one subject.
- Labels: `module:*` for the area, `phase:*` for the roadmap phase,
  `type:*` for the kind of work.
- Architecture changes go through an ADR in `docs/adr/`.
- The pull request template's checklist is the minimum; for a calculation
  its engineering part is required.
