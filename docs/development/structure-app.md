# Structure application crates

The structure application (decision 4 of [Building
structures](structure-building.md)) is developed apart from the core, so
that it and the way it talks to the core can change on their own. Its
crates live in `apps/structure/` (workspace members `apps/structure/*`);
their tests run separately from the core's.

| Crate | What | Depends on |
|---|---|---|
| `ols-structure-loads` | Load-case recipes (`[[load_case]]` in a `.olss`, `.olsslc` on their own), `LineValues` and `LoadTable` (plain, serde-able data), `evaluate`, CSV of the table and the recipes | `ols-core`, `ols-structures` only: no line, no `ols-loads` |
| `ols-structure-link` | **The boundary**: the one crate where the application meets the core. `line_values` (a placed structure's loads from `ols_loads::structure_loads` → `LineValues`), `fe_loads` (a table case → `ols_structures_fe::Loads`). Later: the section library (member areas) and saving back into the line | core crates + `ols-structure-loads` |
| `ols-structure-edit` | The edits of a structure file, each a function on its text (`toml_edit`: comments and order kept); `StructureFile` with undo and redo of whole edits (a drag is one); `Geometry`, picking; `missing(level)` (what the next level lacks, and advice: points on no joint, joints on one member, members crossing without a joint). No UI | `ols-core`, `ols-structures` (the file format only). `ols-app`'s pylon workspace depends on it, so there is one implementation of the edits |
| `ols-structure-app` | The binary `ols-structure` (eframe): the 3D view in local axes, the tabs, the header with the level and what the next one lacks, Save / Save as, undo / redo. The line application's **Edit…** starts it on the structure's file and reads the file again when it is saved | `ols-structure-edit`, `ols-structures`, `ols-render` (drawing only); later the link for line values |
| `ols-structure-tests` | Test-only (`publish = false`, empty library, tests in `tests/`): the synthetic example line solved by the core, through the link, against hand values and the line run's own structure check; the application's formats refusing typos | everything above, as dev-dependencies |

Rules:

- **Core crates never depend on `apps/structure` crates** (also not as
  dev-dependencies). The core keeps the application's tables unread where
  it must hold them: `StructureType::load_cases` is the raw
  `[[load_case]]` tables, as `[[section]]` / `[[material]]` are for
  `ols-structures-fe`. For that reason the typo test of the application's
  formats is in `ols-structure-tests`, not in
  `crates/ols-cli/tests/format_typos.rs`.
- Application crates other than the link take and give plain data; only
  the link reads core results.
- Plugins are not involved (design codes stay `code-plugin`s giving the
  line's weather and load cases).

## Checks

Before a commit that touches the application's crates (expected about
1–3 minutes with a warm build; each step under a time limit):

```sh
cargo fmt --all --check
cargo clippy -p ols-structure-loads -p ols-structure-link -p ols-structure-tests --all-targets -- -D warnings
cargo test -p ols-structure-loads -p ols-structure-link -p ols-structure-tests
```

The editor and the window on their own (seconds for the edits, under a
minute for the window's egui_kittest tests with a warm build):

```sh
cargo clippy -p ols-structure-edit -p ols-structure-app --all-targets -- -D warnings
cargo test -p ols-structure-edit -p ols-structure-app
```

`ols-app` depends on `ols-structure-edit` (its pylon workspace), so a
change there also runs `cargo test -p ols-app`.

The workspace-wide checks (and CI, which runs `--workspace`) include
these crates too; a change to a core crate the application uses also
runs the core's tests.

## Formats

Structure load-case recipes and the load table: [Structure load
cases](../user-guide/structure-load-cases.md); map: [Data
model](data-model.md#structure-load-cases-load_case-olsslc). CSV works
today; XLSX import and export come later.
