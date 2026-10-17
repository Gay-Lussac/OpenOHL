# Structure application crates

The structure application (decision 4 of [Building
structures](structure-building.md)) is developed apart from the core, so
that it and the way it talks to the core can change on their own. Its
crates live in `apps/structure/` (workspace members `apps/structure/*`);
their tests run separately from the core's.

| Crate | What | Depends on |
|---|---|---|
| `ols-structure-loads` | Load-case recipes (`[[load_case]]` in a `.olss`, `.olsslc` on their own), `LineValues` and `LoadTable` (plain, serde-able data), `evaluate`, CSV of the table and the recipes | `ols-core`, `ols-structures` only: no line, no `ols-loads` |
| `ols-structure-link` | **The boundary**: the one crate where the application meets the core. `line_values` (a placed structure's loads from `ols_loads::structure_loads` → `LineValues`), `fe_loads` (a table case → `ols_structures_fe::Loads`; a point on a member shared to its ends), `analyse_cases` (table cases solved on the structure's model → `CaseResult`, plain data: displacements, member forces and usage, reactions, Σ loads and Σ reactions), `export_structure` (a level 3 structure with its section library and load table as plain `ExportStructure` for the exports). `placed_values` (a placed structure's line values with the other places of its type, written for the application, below), `SectionAreas` (member areas from the section library for surface loads) | core crates + `ols-structure-loads` |
| `ols-structure-edit` | The edits of a structure file, each a function on its text (`toml_edit`: comments and order kept); `StructureFile` with undo and redo of whole edits (a drag is one); `Geometry` (standing on an extension: `geometry_with`; the pole's wireframe), picking; `missing(level)` (what the next level lacks — also the pole's outline and design, extension members without sections — and advice: points on no joint, joints on one member, members crossing without a joint, an extension without members at level 3); the pole's edits (`pole`: outline rows, shape, fields, a `.olsp` brought in) and the extensions' (`extension_edits`: joints, members, the piece in one go, legs); the edits of the load-case recipes (`load_cases.rs`: by place, `merge_load_cases` for imports); the body tools: generators (`generators`, pure), apply and rebuild (`rebuild`), the `[[generator]]` and `[[symmetry]]` tables (`body`), mirrored edits (`symmetry`), member groups (`groups`). No UI | `ols-core`, `ols-structures` (the file format only). `ols-app`'s structure screen depends on it, so there is one implementation of the edits |
| `ols-structure-export` | The exports of a level 3 structure ([Structure exports](structure-exports.md)): IFC4 (members with profile solids and the structural analysis model with load cases), IFC2x3 (members, for Tekla Structures), SAF (xlsx, its own small writer on `zip`), CSV tables; the tool `ols-structure-export` (not `ols structures export`: `ols-cli` is a core crate) | `ols-structure-link` (`export_structure`, the plain `ExportStructure`), `ols-structure-loads`, `ols-sections`, `ols-structures` (parsing the file only) |
| `ols-structure-app` | The binary `ols-structure` (eframe): the 3D view in local axes, the tabs, the header with the level and what the next one lacks, Save / Save as, undo / redo. The line application's **Edit…** starts it on the structure's file and reads the file again when it is saved. Tabs in modules of their own: `tables.rs` (Geometry, Body, Design), `in_line_tab.rs` (In the line), `body_tab.rs` and `generators_ui.rs` (the Body tab's tools, Design › Groups), `load_cases_tab.rs`, `results_tab.rs`, `export_menu.rs` (File › Export), `pole_panel.rs` (Body › Pole, Design › Pole), `extensions_panel.rs` (Body › Extensions) | `ols-structure-edit`, `ols-structure-loads`, `ols-structure-link` (analysis only through it), `ols-structure-export`, `ols-structures`, `ols-render` (drawing only) |
| `ols-structure-tests` | Test-only (`publish = false`, empty library, tests in `tests/`): the synthetic example line solved by the core, through the link, against hand values and the line run's own structure check; the application's formats refusing typos; a body built by the generators solved by the FE model (`body_fe.rs`) | everything above, as dev-dependencies |

Rules:

- **Core crates never depend on `apps/structure` crates** (also not as
  dev-dependencies). The core keeps the application's tables unread where
  it must hold them: `StructureType::load_cases` is the raw
  `[[load_case]]` tables (and `generators`, `symmetry` the raw
  `[[generator]]`, `[[symmetry]]` tables, read by `ols-structure-edit`), as `[[section]]` / `[[material]]` are for
  `ols-structures-fe`. For that reason the typo test of the application's
  formats is in `ols-structure-tests`, not in
  `crates/ols-cli/tests/format_typos.rs`.
- Application crates other than the link take and give plain data; only
  the link reads core results.
- Plugins are not involved (design codes stay `code-plugin`s giving the
  line's weather and load cases).

## Started for a placed structure

```text
line app: Properties › Edit…  (Action::EditInStructureApp { id, structure })
  ├─ writes <stem>.<id>.line-values.json   ("being computed" first, then
  │     ols_structure_link::placed_values on a thread; again when the
  │     line changes and stays unchanged for 1 s; one run at a time)
  └─ starts  ols-structure <file> --structure <id> --line-values <json> --sections <dir>
structure app: reads the JSON when its mtime changes (twice a second):
  header "loads of <id> in <line>", the In the line tab, the load table
  typed values → <stem>.<id>.loads.csv → the line app stores them (kind `loads`)
```

- The JSON is `ols_structure_loads::PlacedValues` (format
  `ols-structure-line-values`, version 1): the structure, the line, its
  type, the line's load cases, the values (`LineValues`), the other
  places of the same type with theirs, and a note (no values yet, no
  load case file, sections a case did not solve).
- One structure application per structure file: Edit… on another place
  of the same type while one is open is refused with a message.
- Outside a project (a plain line file) both files are written next to
  the structure files.
- Member areas for surface loads (`SectionAreas`): profile width for the
  load's direction × length × sin θ (`Model::projected_area`, shared with
  member wind); a member whose section is not in the library gets "no
  area" with the reason.

## Load cases and Results tabs

- **Recipes** are edited in the file (`ols-structure-edit::load_cases`):
  each change one undo step; the tab reads them back with
  `StructureLoadCases::from_structure` and shows why they do not read.
- **The load table** is evaluated again whenever the file text or the
  typed values change (`LoadCasesTab::refresh`, keyed by a hash).
  Values typed over it are `Override`s of the session, applied after
  each evaluation. They are kept beside the structure file as
  `<stem>.loads.csv` (the table's CSV with the typed rows only), written
  on Save (also Save as, beside the new file) and read on open; unsaved
  typed values count as unsaved changes. Started for a placed structure
  (`--structure T2`), they are `<stem>.<id>.loads.csv` instead, and the
  line application keeps them in the project (library kind `loads`,
  per project line; spec §4.1). A table
  imported as CSV becomes the overrides of what it has beyond the
  evaluation (`LoadTable::overrides_over`), so export → import gives the
  same table.
- **Results** need level 3 (`Detail::Design`, which includes `model =`).
  The run goes to a thread of its own through
  `ols_structure_link::analyse_cases`; the window polls it and stays
  responsive. Exit clauses ([Performance](performance.md)): each case at
  most 200 iterations and 30 s, a **Cancel** button (a `CancelToken`);
  a case that does not solve is listed with the solver's reason
  (mechanism, unstable, stopped by a limit with the residual). Results
  computed from another table or file are marked as such.
- **The view** draws a tab's overlay (`ViewInput::overlay`, a `Scene`
  with a key; uploaded again only when the key changes): load arrows of
  the case shown, or the deformed shape coloured by usage (red above
  100 %, orange from 90 %, as the line application's Usage page) and
  the reactions.

## Checks

Before a commit that touches the application's crates (expected about
1–3 minutes with a warm build; each step under a time limit):

```sh
cargo fmt --all --check
cargo clippy -p ols-structure-loads -p ols-structure-link -p ols-structure-export -p ols-structure-tests --all-targets -- -D warnings
cargo test -p ols-structure-loads -p ols-structure-link -p ols-structure-export -p ols-structure-tests
```

The editor and the window on their own (seconds for the edits, under a
minute for the window's egui_kittest tests with a warm build):

```sh
cargo clippy -p ols-structure-edit -p ols-structure-app --all-targets -- -D warnings
cargo test -p ols-structure-edit -p ols-structure-app
```

`ols-app` depends on `ols-structure-edit` (its structure screen), so a
change there also runs `cargo test -p ols-app`.

The workspace-wide checks (and CI, which runs `--workspace`) include
these crates too; a change to a core crate the application uses also
runs the core's tests.

## Formats

Structure load-case recipes and the load table: [Structure load
cases](../user-guide/structure-load-cases.md); map: [Data
model](data-model.md#structure-load-cases-load_case-olsslc). CSV works
today; XLSX import and export come later.
