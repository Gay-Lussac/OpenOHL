# Structure application crates

The structure application (decision 4 of [Building
structures](structure-building.md)) is developed apart from the core, so
that it and the way it talks to the core can change on their own. Its
crates live in `apps/structure/` (workspace members `apps/structure/*`);
their tests run separately from the core's.

| Crate | What | Depends on |
|---|---|---|
| `ohl-structure-loads` | Load-case recipes (`[[load_case]]` in a `.ohls`, `.ohlslc` on their own), `LineValues` and `LoadTable` (plain, serde-able data), `evaluate`, CSV of the table and the recipes | `ohl-core`, `ohl-structures` only: no line, no `ohl-loads` |
| `ohl-structure-link` | **The boundary**: the one crate where the application meets the core. `line_values` (a placed structure's loads from `ohl_loads::structure_loads` → `LineValues`), `fe_loads` (a table case → `ohl_structures_fe::Loads`; a point on a member shared to its ends), `analyse_cases` (table cases solved on the structure's model → `CaseResult`, plain data: displacements, member forces and usage, reactions, Σ loads and Σ reactions), `export_structure` (a level 3 structure with its section library and load table as plain `ExportStructure` for the exports). `placed_values` (a placed structure's line values with the other places of its type, written for the application, below), `SectionAreas` (member areas from the section library for surface loads) | core crates + `ohl-structure-loads` |
| `ohl-structure-edit` | The edits of a structure file, each a function on its text (`toml_edit`: comments and order kept); `StructureFile` with undo and redo of whole edits (a drag is one); `Geometry` (standing on an extension: `geometry_with`; the pole's wireframe), picking; `missing(level)` (what the next level lacks — also the pole's outline and design, extension members without sections — and advice: points on no joint, joints on one member, members crossing without a joint, an extension without members at level 3); the pole's edits (`pole`: outline rows, shape, fields, a `.ohlp` brought in) and the extensions' (`extension_edits`: joints, members, the piece in one go, legs); the edits of the load-case recipes (`load_cases.rs`: by place, `merge_load_cases` for imports); the body tools: generators (`generators`, pure), apply and rebuild (`rebuild`), the `[[generator]]` and `[[symmetry]]` tables (`body`), mirrored edits (`symmetry`), member groups (`groups`). No UI | `ohl-core`, `ohl-structures` (the file format only). `ohl-app`'s structure screen depends on it, so there is one implementation of the edits |
| `ohl-structure-export` | The exports of a level 3 structure ([Structure exports](structure-exports.md)): IFC4 (members with profile solids and the structural analysis model with load cases), IFC2x3 (members, for steel-detailing programs that take IFC2x3 only), SAF (xlsx, its own small writer on `zip`), CSV tables; the tool `ohl-structure-export` (not `ohl structures export`: `ohl-cli` is a core crate) | `ohl-structure-link` (`export_structure`, the plain `ExportStructure`), `ohl-structure-loads`, `ohl-sections`, `ohl-structures` (parsing the file only) |
| `ohl-structure-app` | The binary `ohl-structure`, **OHL-Structure** (eframe): the menus at the top and the panels under them (below), the 3D view in local axes, the Level panel with what the next level lacks, Save / Save as, undo / redo. The line application's **Edit…** starts it on the structure's file and reads the file again when it is saved. Modules: `app.rs` (the window, the state, the frame), `menus.rs` (the menu bar and the actions it shares with the panels' tools, the windows), `layout.rs` (the panels and menus: `Panel`, `Area`, the default arrangement, `open_panel`), `tables.rs` (the panels of tables of Geometry, Body, Design and their actions), `in_line_tab.rs` (Line values), `body_tab.rs` and `generators_ui.rs` (the Joints and Members panels' tools, Generators, Groups), `load_cases_tab.rs` (Load recipes, Load table, the line's forces as rows, the status block, Store in the structure file), `results_tab.rs` (FE results), `export_menu.rs` (File › Export), `pole_panel.rs` (Pole, Pole design), `extensions_panel.rs` (Extensions), `libraries.rs` (File › Libraries and the Libraries panel: `LibKind`, `Folders`, `references`, `scan`, `import`, `export`, `sections_subset`), `start.rs` (the start page, recent files, New structure), `plugins_ui.rs` (Plugins › Plugin manager, Load cases from a design code) | `ohl-structure-edit`, `ohl-structure-loads`, `ohl-structure-link` (analysis only through it; also `pole_file_id`, `model_file_name` to check `.ohlp` / `.ohlfe` files), `ohl-structure-export`, `ohl-structures`, `ohl-sections`, `ohl-plugins` (core: manifests, discovery, the host, the shared settings), `ohl-render` (drawing only) |
| `ohl-structure-tests` | Test-only (`publish = false`, empty library, tests in `tests/`): the synthetic example line solved by the core, through the link, against hand values and the line run's own structure check; the application's formats refusing typos; a body built by the generators solved by the FE model (`body_fe.rs`) | everything above, as dev-dependencies |

Rules:

- **Core crates never depend on `apps/structure` crates** (also not as
  dev-dependencies). The core keeps the application's tables unread where
  it must hold them: `StructureType::load_cases` is the raw
  `[[load_case]]` tables (and `generators`, `symmetry` the raw
  `[[generator]]`, `[[symmetry]]` tables, read by `ohl-structure-edit`), as `[[section]]` / `[[material]]` are for
  `ohl-structures-fe`. For that reason the typo test of the application's
  formats is in `ohl-structure-tests`, not in
  `crates/ohl-cli/tests/format_typos.rs`.
- Application crates other than the link take and give plain data; only
  the link reads core results.
- Plugins reach the application only through `ohl-plugins` (a core
  crate, an allowed dependency of `ohl-structure-app` only): design codes
  stay `code-plugin`s giving the line's weather and load cases — the
  application takes their load-case names as recipes taking the line's
  values (`ohl_structure_edit::take_line_load_cases`). No plugin world is
  made for the application yet; check plugins are listed only.

## On its own: start page, libraries, plugins

- **Start page** (`start.rs`): no file argument → `StructureApp::start_page`.
  Recent files are kept in eframe storage (`RECENT_KEY`
  `ols-structure-recent`, ten, canonical paths, added by
  `StructureApp::open`). **New structure** writes `start::minimal` (one
  point, level 1) or a pylon template built with
  `PylonTemplate::build(defaults(role))` from `ohl-structures` (no
  dependency on `ohl-app`'s pylon builder); `start::create` refuses an
  existing file and checks the result opens. The launcher
  `Start OHL-Structure.command` builds `ohl-structure-app` and runs `ohl-structure`.
- **Libraries** (`libraries.rs`): `Folders::new(file, --sections,
  --insulators)` gives where each `LibKind` is read and imported into
  (sections: the folder given or `Sections/`; insulators: the folder
  given, else the file's folder and `Insulators/`; poles, models, recipes:
  the file's folder; templates: `PylonTemplates/`). `scan` reads and
  checks each file (`check_file`: `SectionLibrary::parse`,
  `InsulatorAssembly`/`PylonTemplate` `LibraryItem::parse`,
  `StructureLoadCases::parse`, the link's `pole_file_id` and
  `model_file_name`) and lists what `references` names that none has.
  `import` checks every file and the ids against the other files of the
  folder before copying anything; `export` writes the line application's
  layout (`structures/`, `sections/`, `insulators/`, `pylon-templates/`),
  sections filtered by `sections_subset` (`toml_edit`: the sections used
  and their materials, comments kept). After an import
  `StructureApp::reload_libraries` reads the insulators and sections
  again (`load_libraries`, shared with `open`). The listing is cached by
  a hash of the file text and the folders.
- **Started from a project**: the line application passes the working
  copy's folders (`--insulators`, `--sections`, the structure file in
  `structures/`), and its `StructureApps` watch hashes the library files
  there (`structure_app::library_signature`, `LIBRARY_EXTENSIONS`, twice
  a second); a change is `Change::Libraries`, handled as a library page
  saved (`library_saved`: `store_libraries` puts the working copy into
  the project, the model reads its libraries again).
- **Plugins** (`plugins_ui.rs`): `PluginsState` lists `user_dir`
  (`eframe::storage_dir("OLS CAD")/plugins`, the line application's;
  `LINE_STORAGE_ID`, the storage id kept from before OpenOHL) and
  the folders of `ohl_plugins::PluginSettings` (`settings.toml` there),
  read when the manager opens and written at each change; `applies`
  says per extension point what it does here. Details come from
  `WasmHost::describe_any` on a thread; a design code runs
  `WasmHost::load_code(..).generate` on a thread (`run_code`), polled
  each frame.

## Started for a placed structure

```text
line app: Properties › Edit…  (Action::EditInStructureApp { id, structure })
  ├─ writes <stem>.<id>.line-values.json   ("being computed" first, then
  │     ohl_structure_link::placed_values on a thread; again when the
  │     line changes and stays unchanged for 1 s; one run at a time)
  └─ starts  ohl-structure <file> --structure <id> --line-values <json> --sections <dir>
structure app: reads the JSON when its mtime changes (twice a second):
  "loads of <id> in <line>" (Level panel), the status block, Line
  values, the load table (the line's forces first; Store in the file)
  typed values → <stem>.<id>.loads.csv → the line app stores them (kind `loads`)
```

- The JSON is `ohl_structure_loads::PlacedValues` (format
  `ohl-structure-line-values`, version 3): the structure, the line, its
  type, the line's cases and their sources (version 2), the values
  (`LineValues`), the other places of the same type with theirs, a note
  (no values yet, no load case file, sections a case did not solve) and
  `missing` (version 3): per case, the sections and jumpers at the
  structure that did not solve (`MissingWire`). The application marks
  such a case incomplete in the status block, on its load-table rows
  (note `incomplete: S12 not solved`), in Line values and in FE results
  (`LoadCasesTab::incomplete`). Older versions still read.
- One structure application per structure file: Edit… on another place
  of the same type while one is open is refused with a message.
- Outside a project (a plain line file) both files are written next to
  the structure files.
- Member areas for surface loads (`SectionAreas`): profile width for the
  load's direction × length × sin θ (`Model::projected_area`, shared with
  member wind); a member whose section is not in the library gets "no
  area" with the reason.

## The window: menus and panels

The window follows the line application's design rules: every menu at
the top — File, Edit, View, then one per part of the work (Geometry,
Body, Design, Load cases, Results, In the line; `layout::Area`), Help —
and the panels under them docked with `egui_dock` (the same version as
`ohl-design`), each movable, closable and opened again from View or its
menu. View is an options menu (`layout::options_menu`, a copy of the line
application's: a tick never closes it); errors and long reports scroll in
boxes of fixed height (`app::error_label`); every window has its ×.

- `layout::Panel` lists the panels; each belongs to an `Area` (the 3D
  view and the Level panel to none) and shows one table or a few
  (`Panel::sheets`; several: chosen at its top). `default_dock` is the
  arrangement at first start and after **View › Reset layout**: the
  Level panel at the top left, a leaf with the geometry, body and design
  panels, one with the load and result panels, the 3D view on the right
  (`split_left` at 0.45).
- `open_panel` brings a panel to the front, or opens it where it belongs
  (next to a panel of its area, else of the tables; the 3D view on the
  right, the Level panel above the tables). Menus, header links
  (`StructureApp::go_to`, through `focus_request` since the dock is taken
  out of the application while it is drawn) and the level at opening all
  go through it (`StructureApp::focus`).
- The dock is drawn with the application taken as the `TabViewer`
  (`layout::Viewer`); the panel in front of the leaf last clicked sets the
  area worked in (`StructureApp::area`), which decides what the 3D view
  draws over the structure (load arrows, results). The generator's
  preview is cleared when its panel was not drawn.
- The arrangement is kept with eframe's persistence (`egui_dock`'s
  `serde` feature, key `ols-structure-dock`; `StructureApp::restore` at
  start, `eframe::App::save`); one that does not read gives the default.
- The menus' actions are the same methods as the panels' tools
  (`add_point`, `add_insulators`, `add_joint`, `add_member`,
  `point_here`, `ask_delete`, `set_symmetry`, `make_symmetric`,
  `add_case`, the import and export dialogs, `run_results`,
  `cancel_results`, `store_line_cases`); an action brings its panel to
  the front. The deletion is asked in a window.

## Load cases and Results panels

- **The line's forces** come first in the load table
  (`load_cases_tab::line_rows`): every case of the line-values file with
  values × every point, the factored transverse, longitudinal and
  vertical forces where the line factors the case (else the unfactored
  ones), source `line`, no recipe needed; a line case with the name of a
  recipe is called "NAME (line)". The recipes evaluated follow. The
  results and the exports take them like any case.
- **Store in the structure file** (`StructureApp::store_line_cases`,
  `ohl_structure_edit::store_given_cases`): the line's cases ticked
  written as `[[load_case]]` of given values (a `value` and `direction`
  per point and non-zero component, `line_case`, a description starting
  "Stored from the line" with the line, the place and the date), one
  edit; a case of the same name is replaced in its place (the window asks
  first). `take_line_load_cases` writes recipes instead (the line's
  factored values at every point, following the line).
- **The status block** (`LoadCasesTab::line_status`) turns the
  line-values file's state and note into plain sentences: no line (opened
  alone), waiting, being computed, no load case file (with what to do),
  a case without values, what the line gave; plus the cases stored from
  the line.
- **Recipes** are edited in the file (`ohl-structure-edit::load_cases`):
  each change one undo step; the panel reads them back with
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
  `ohl_structure_link::analyse_cases`; the window polls it and stays
  responsive. Exit clauses ([Performance](performance.md)): each case at
  most 200 iterations and 30 s, a **Cancel** button (a `CancelToken`);
  a case that does not solve is listed with the solver's reason
  (mechanism, unstable, stopped by a limit with the residual). Results
  computed from another table or file are marked as such.
- **The view** draws the overlay of the area worked in (`ViewInput::overlay`, a `Scene`
  with a key; uploaded again only when the key changes): load arrows of
  the case shown, or the deformed shape coloured by usage (red above
  100 %, orange from 90 %, as the line application's Usage page) and
  the reactions.

## Checks

Before a commit that touches the application's crates (expected about
1–3 minutes with a warm build; each step under a time limit):

```sh
cargo fmt --all --check
cargo clippy -p ohl-structure-loads -p ohl-structure-link -p ohl-structure-export -p ohl-structure-tests --all-targets -- -D warnings
cargo test -p ohl-structure-loads -p ohl-structure-link -p ohl-structure-export -p ohl-structure-tests
```

The editor and the window on their own (seconds for the edits, under a
minute for the window's egui_kittest tests with a warm build):

```sh
cargo clippy -p ohl-structure-edit -p ohl-structure-app --all-targets -- -D warnings
cargo test -p ohl-structure-edit -p ohl-structure-app
```

The plugin manager's test lists the in-repo plugins and runs the
`en50341` design code when it is built; with `OHL_REQUIRE_PLUGIN_TESTS`
set, an unbuilt plugin fails it (build each `plugins/*/*/` with a
`Cargo.toml`: `cargo build --release --target wasm32-wasip2`).

`ohl-app` depends on `ohl-structure-edit` (its structure screen), so a
change there also runs `cargo test -p ohl-app`.

The workspace-wide checks (and CI, which runs `--workspace`) include
these crates too; a change to a core crate the application uses also
runs the core's tests.

## Formats

Structure load-case recipes and the load table: [Structure load
cases](../user-guide/structure-load-cases.md); map: [Data
model](data-model.md#structure-load-cases-load_case-ohlslc). CSV works
today; XLSX import and export come later.
