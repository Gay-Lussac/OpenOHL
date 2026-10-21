# Writing plugins

OpenOHL has three plugin tiers (ADR-0003): **WebAssembly** components (stable interface, sandboxed, any language), **Python** (scripts, reports, import/export) and native Rust inside this repository.

Every plugin has an `ohl-plugin.toml` manifest:

```toml
[plugin]
name = "hello-wasm"
version = "0.1.0"
description = "Example plugin"
sdk = "0.3"                 # plugin interface version targeted
tier = "wasm"               # "wasm" or "python"
entry = "target/wasm32-wasip2/release/hello_wasm.wasm"
extension-points = []

[permissions]               # everything denied by default
network = false
filesystem = []
```

### Table columns

A plugin may add columns to the app's tables, declared in its manifest
(any tier; nothing to implement):

```toml
[[column]]
table = "circuits"         # structures, sections, circuits (the Conductor
                           # paths table), systems, jumpers, alignments or routes
name = "insulation_level"  # unique in the plugin: letters, digits, _ -
title = "insulation level" # header (default: name)
help = "…"                 # shown on the header
values = ["low", "high"]   # a dropdown; none: typed
default = "high"           # shown, dimmed, where not set
```

The values are kept on the objects, in their `fields` under
`plugin.name` (e.g. `code.insulation_level`; a circuit's on all its
sections), saved with the project and in OHL XML (`<field key value>`).
The plugin, or the check that belongs to it, reads them there. Without
the plugin, a project keeps the values and the tables still show them
in a column marked "(unused)", typed like text. A check plugin can
declare a circuit's value its check needs this way (e.g. an insulation
level read by the distance between conductors).


**Licence.** The plugin interface (the WIT files in
`crates/ohl-plugins/wit/`) and everything under `plugins/` (the shared
crate `ohl-code-common`, the design-code, example and test plugins) are
under the [Mozilla Public License 2.0](https://github.com/Gay-Lussac/OpenOHL/blob/main/LICENSE-MPL-2.0):
a plugin built on them may keep any licence, a closed one included.
OpenOHL itself is under the AGPL-3.0-or-later
([ADR-0011](../adr/0011-license-agpl.md)).
## Where plugins are found, and the shared settings

Both desktop applications — the line application `ohl-design` (OHL-Design)
and OHL-Structure `ohl-structure` — list the plugins installed for the
user (the line application's data folder, `plugins/`:
`eframe::storage_dir("OLS CAD")`; the storage id keeps its name from
before the rename to OpenOHL, so settings, recent files and plugins stay
where they were) and
those of the folders added. The folders and the plugins turned off are
one settings file, `settings.toml` in that plugin folder
(`ohl_plugins::PluginSettings`: `folders`, `disabled`; written through
a temporary file renamed into place), read and written by both: a plugin
added or turned off in one is so in the other.

What applies in OHL-Structure: `design-code` (Load cases › Load cases
from a design code…: its load cases become recipes taking the line's
values); `check` is listed (member and joint code checks will be a
structure world later); `io` plugins take the line's export model, not a
structure (listed as not applicable); `terrain`, `project` and Python
plugins are the line application's. See
[OHL-Structure](../user-guide/structure-app.md#plugins).

## WebAssembly plugins

The interface is defined in WIT: `crates/ohl-plugins/wit/plugin.wit`. A Rust example lives in `plugins/examples/hello-wasm`:

```bash
rustup target add wasm32-wasip2
cd plugins/examples/hello-wasm && cargo build --release --target wasm32-wasip2
cargo run -p ohl-cli -- plugin info plugins/examples/hello-wasm
```

`ohl plugin info DIR` works for a plugin of any world: it loads the
plugin in the world of each extension point its manifest lists
(`design-code`: `code-plugin`, `io`, `terrain`, `project`, `check`; none
of these: the minimal `plugin` world) and prints its `describe` (a check
plugin, which has none, by its manifest), the world, the extension
points, tier, interface version and entry file, then what else it
offers: its formats (id, description, extension, importer or exporter),
its checks, or the length of a design code's parameter template.

```text
name         geojson
version      0.2.0
description  GeoJSON export (structures, wires) and import (structures)
world        io-plugin
extensions   io
tier         wasm
sdk          0.3
entry        plugins/examples/geojson/target/wasm32-wasip2/release/geojson.wasm
  format geojson: Structures and wires as GeoJSON features (line CRS) (.geojson, exporter)
  format geojson-structures: Structures from GeoJSON Point features (.geojson, importer)
```

Plugins run sandboxed — no file system, network, environment variables or standard I/O — with **exit clauses** on every call: a time limit (default 5 s; a runaway plugin is interrupted and reported as a timeout) and a memory cap (default 256 MiB). `plugins/tests/runaway` is a deliberately endless plugin used to test this.

## File-format plugins

A plugin that adds import or export formats implements the `io-plugin` world of the same WIT file and lists the extension point `io` in its manifest:

```toml
extension-points = ["io"]
```

- `formats()` lists its formats: id, description, extension and direction (exporter or importer).
- `export-model(format, model, options)` receives the solved model as JSON and returns the file's bytes. The model is `{"format": "ohl-export-model", "version": 1, "crs": …, "line": <line file>, "cases": [{"name", "summary", "wires": [{"section", "span", "circuit", "phase", "color", "system", "voltage_kv", "horizontal_tension_n", "max_tension_n", "sag_m", "points": [[x, y, z], …]}], "strings": [{"structure", "label", "by", "force_n", "swing_deg", "swing_across_deg", "swing_along_deg", "max_load_n", "max_swing_deg", "max_swing_across_deg", "max_swing_along_deg"}]}]}` (coordinates in the line's coordinate system; jumpers have section `jumper:<id>` and no horizontal tension; tensions per wire, N; sag in m). `strings` lists each structure's string as solved in the case (once; `by`: the section that reported it, `jumper:<id>` for a jumper's helping string) with its **limits** — see [String limits](#string-limits-what-a-plugin-sees).
- `import-file(format, data, options)` returns a line file (JSON), which the host checks like any line file. When the user gives a line to update, it arrives as option `into` (its JSON).
- Errors are returned as strings and shown to the user as the format's error.

`plugins/examples/geojson` is a complete example (GeoJSON export of structures and wires; structures imported from GeoJSON points). Its formats appear next to the built-in ones in `ohl formats --plugin plugins/examples/geojson`; see [Import and export](../user-guide/import-export.md).

## Terrain plugins

A plugin that reads terrain surfaces implements the `terrain-plugin` world and lists the extension point `terrain`:

```toml
extension-points = ["terrain"]

# Large files: more than the default 5 s and 256 MiB per call (granted up
# to 600 s and 4 GiB).
[limits]
seconds = 60
memory-mib = 1024
```

- `formats()` lists the formats it reads (direction `importer`).
- `import-surfaces(format, data, options)` returns surfaces: a name, the points as `x0, y0, z0, x1, …` (metres, x east, y north, z up) and the triangles as three point indices each. The host checks them like any mesh and keeps them in the project.

`plugins/examples/esri-grid` reads ESRI ASCII grids (`.asc`): a point per cell centre, two triangles per square of cells with data. Its format appears in `ohl surface formats --plugin plugins/examples/esri-grid` and in Terrain › Terrain surfaces once the plugin is installed or its folder added (Plugins › Plugin manager).

A plugin also suits formats that should not be in the open-source code — e.g. one read from a private specification — kept and installed only where it may be used.

## Project plugins

A plugin that reads another program's project files implements the `project-plugin` world and lists the extension point `project` (with `[limits]` for large files, as above):

- `formats()` lists the formats it reads.
- `import-project(format, data, options)` returns a **project**: the line as a line file (JSON), its **library files** (`kind` — `cables`, `weather`, `loadcases`, `structures`, `insulators`, `rules`, `galloping`, `ambient`, `costs`, `poles`, `femodels`, `sections` — `name` and TOML `text`, as OpenOHL writes them) and **notes**: what was read, what was left out, what was assumed. The host checks the line and creates a new project file with it and its libraries (File › New project from another program, `ohl project import-from`), and shows the notes.

`plugins/examples/project-bundle` reads a JSON bundle `{"line": …, "libraries": [{"kind", "name", "text"}]}` — the shape such a reader returns. Readers of formats that should stay out of the open-source code are installed only where they may be used.

## Design-code plugins

A plugin for a design code implements the `code-plugin` world
(`crates/ohl-plugins/wit/code.wit`) and lists the extension point
`design-code`:

```toml
extension-points = ["design-code"]
```

- `describe()` returns its name, version, a one-line description and a
  **parameters template**: a TOML parameter file with its schema in
  comments and placeholder values.
- `generate(parameters)` reads the user's parameter file (TOML text) and
  returns **generated**: `weather-cases` (an `.ohlwc` file's text),
  `load-cases` (an `.ohllc` file's text, each case naming one of those
  weather cases), `notes` (what was computed, assumed or left out) and
  `clearance-presets` (may be empty). A wrong file is an error string,
  shown to the user.
- A **clearance preset** (`clearance-preset`) fills in a clearance window
  in one click: its `name` (unique among the plugin's), its `measure`
  (`vertical` or `radial`: the window), its `rules` — a clearance rule
  file's text (`.ohlr`, format `ohl-clearance-rules`: the rules with
  their classes, voltages, distances and, vertical, `vertical_band` (the
  zone's half width of each rule); `[wind]` steps and sides; `[check]`
  circuits, calculation `method` and `search_distance`) — and its
  `weather-cases`, an `.ohlwc` file's text with the cases its rules name.

The host (`ohl_io::codes`, `WasmHost::load_code`) checks the files as
OpenOHL reads them before anything is written — each preset's rule file
read (an error names it, `<plugin>-<name>.ohlr`), a rule of its
measure, every weather case its rules name among its own, the names
unique; `ohl code generate` and Criteria › Design code write them (the
presets as `<plugin>-<name>.ohlr` and `.ohlwc`, in the project among
its rule and weather case files). Plugin interface version 0.2 (`sdk =
"0.2"`) brought the presets: a plugin built for 0.1 must be rebuilt. Version 0.3
(2026-10-18) renamed the interface `ols:plugin` → `ohl:plugin` and the
manifest `ols-plugin.toml` → `ohl-plugin.toml` (still read): a plugin
built for 0.2 must be rebuilt (its sources need the new package name
only). A plugin ships **no values of the
standard**: it implements the code's method, every number comes from the
parameter file, and the template's numbers are invented placeholders,
marked as such.

### Wind factors for structures

A design-code plugin may also implement the optional `structure-wind`
interface: the `code-wind-plugin` world (`include code-plugin; export
structure-wind;`), extension point `design-code` as before.

- `structure-wind(parameters)` reads the same parameter file and returns
  **none** (the code gives no factors) or a cross-section library's text
  (`.ohlsec`, TOML) holding **only** a `[wind]` table: `source` (free
  text, shown with the results), `drag` (a factor per profile shape:
  `equal_angle`, `unequal_angle`, `channel`, `i_section`,
  `rectangular_hollow`, `circular_hollow`, `round_bar`,
  `polygonal_hollow`, `general`), `face_drag` (`solidity`, `factor`: a
  lattice face's drag by its solidity ratio) and `shielding` (`solidity`,
  optional `spacing`, `factor`: the factor on a face behind another by the
  front face's solidity ratio and the spacing ratio), each optional — see
  [Cross-section library](../user-guide/section-library.md#wind-factors-for-structures-wind).
  An error string is shown to the user, as for `generate`.
- **OpenOHL measures, the plugin gives the numbers**: the faces of the
  lattice, their solidity ratios, which face is in front of which member
  and the spacing ratio are computed in OpenOHL ([Wind on lattice
  structures](../theory/structure-wind.md)); the plugin returns its code's
  tables, built from the parameter file (no values of a standard typed
  into the plugin).
- The host (`WasmHost::load_code`) looks for the export
  (`ohl:plugin/structure-wind@0.1.0`): a plugin built without it loads
  and runs as before and gives none (`WasmCodePlugin::structure_wind`
  returns `None`). It checks the text as the cross-section library reads it, a
  `[wind]` table and nothing else (`ohl_io::codes::check_structure_wind`),
  and `ohl code generate`, Criteria › Design code and OHL-Structure's Load
  cases from a design code write it as `<plugin>-wind.ohlsec` into the
  cross-section library.
- **Backwards compatible**: the interface version stays 0.3 (`sdk =
  "0.3"`): nothing a plugin of 0.3 implements changed, the export is only
  looked for. `plugins/codes/*` implement it as a stub returning none
  (`ohl_code_common::no_structure_wind!()`), so they say so; a plugin that
  has its code's factors replaces the stub with its own function.

### Rule presets

A design-code plugin may also implement the optional `rule-presets`
interface (added 2026-10-20, interface version still 0.3): presets of
**any clearance rule set**, not only the two clearance windows'. Worlds:
`code-rules-plugin` (`include code-wind-plugin; export rule-presets;`)
and `code-check-rules-plugin` (`include code-check-plugin; export
rule-presets;`); extension point `design-code` as before.

- `rule-presets(parameters)` returns a list of **rule-preset** records,
  or an error string. `parameters` is the user's parameter file (as
  `generate` reads it) or **empty** when the user gave none (the app's
  Load preset… without a parameter file chosen): a plugin whose presets
  come from its own data gives them anyway, one that needs the file says
  so in its error.
- A **rule-preset**: `name` (unique among the plugin's rule presets),
  `description` (what it holds and the articles of the code, shown before
  it is loaded), `rules` — a clearance rule file's text (`.ohlr`, any
  targets: `ground`, `points`, `same_line`, `other_lines`; each distance
  a distance, `{ by_voltage = [[up to kV, distance], …] }` or `{ base =
  …, per_kv = …, above_kv = … }`; `[wind]` and `[check]` optional) — and
  `weather-cases`, an `.ohlwc` file's text with the cases its rules name
  (empty when they name none).
- The host (`WasmHost::load_code`) instantiates the component once and
  looks for the export (`ohl:plugin/rule-presets@0.1.0`) apart from the
  others (its own binding, world `rule-presets-export`): a plugin built
  without it loads and generates as before, and
  `WasmCodePlugin::rule_presets` gives an empty list;
  `has_rule_presets()` says whether it is there (`ohl plugin info` lists
  the optional exports). `ohl_io::codes::rule_presets` and
  `check_rule_presets` check each preset as OpenOHL reads it (a rule file
  with at least one rule; its weather cases, when given; every case a rule
  or a case pair names among them; names unique).
- Used by: the desktop application's Clearances tab, **Edit rules › Load
  preset…** (the enabled design-code plugins, read on a worker thread with
  the Design code window's parameter file; previewed; merged or replacing
  the rules, one command); `ohl code rule-presets --plugin DIR [--params
  FILE] [--out DIR]` (lists, writes `<plugin>-rules-<name>.ohlr` / `.ohlwc`);
  `ohl code generate` (writes them next to the weather case file).
- **Backwards compatible**: nothing of 0.3 changed; plugins built before
  (any of `code-plugin`, `code-wind-plugin`, `code-check-plugin`) load
  unchanged (test plugin `plugins/tests/code-old`). `plugins/codes/*`
  are built for `code-rules-plugin` with a stub giving none
  (`ohl_code_common::no_rule_presets!()`); `plugins/tests/code-presets`
  gives one preset with invented values for the tests.

### Vegetation presets

A **preset of the vegetation and danger-tree check** is a rule preset (the
same `rule-presets` export, nothing added to the interface) whose `rules`
text carries a `[vegetation]` table — with or without `[[rule]]`s (a
preset holding only `[vegetation]` is listed for the vegetation check
only; see [Presets per check](#presets-per-check)).
Hosts before 2026-10-20 refuse a preset without rules; plugins built
before load unchanged and give none. What a plugin returns, per preset:

- `name`, `description` (the source of its values, shown in the
  dropdown's hover and the preview);
- `rules`: a rule file's text (`format = "ohl-clearance-rules"`,
  `version = 1`, a `name`) with, all keys optional (those given replace
  the panel's values, the others stay):

  ```toml
  [wind]                      # the wind steps of the check
  steps = true
  step = "100 Pa"
  sides = "both"              # both, left or right

  [vegetation]
  classes = [42, 43]          # LiDAR class numbers checked as vegetation
  clearance = { by_voltage = [[0, "2.5 m"], [20, "3 m"], [220, "7 m"], [380, "9 m"]] }
                              # or "5 m", or { base = …, per_kv = …, above_kv = … }
  fall_allowance = "2 m"      # A: the same forms
  root_ball = 0.05            # k
  max_base_ground_distance = "5 m"
  search_distance = "50 m"
  growth_per_year = "0.3 m"
  years = 3
  buffer = "0.5 m"
  min_tree_height = "2 m"
  max_tree_height = "40 m"
  fall_in = true
  swing_arc = false
  cases = ["Veg hot", "Veg wind"]   # its weather cases (2026-10-21; `*` `?`)
  ```

- `weather-cases`: an `.ohlwc` file's text with the cases the check runs
  in (e.g. a hot case and a warm windy one); empty: none. When the preset
  is applied, its cases are matched to the project's (below), those with
  no equal are created in the project's weather cases (one undoable
  change), and every case of the preset — the project's own where one is
  used — is ticked for the run.

Unknown keys are refused (as everywhere in a rule file). Distances are
lengths (`"9 m"` or a number of metres); a table's rows are
`[up to kV, distance]` with the voltages increasing. Read by
`ohl_clearance::rules::VegetationPreset` (`RuleSet::vegetation`); checked
by `ohl_io::codes::check_rule_presets`; listed by `ohl code rule-presets`
(`vegetation: classes …`). The test plugin `plugins/tests/code-presets`
returns one (`trees`, invented values).

`plugins/codes/*` are such plugins (EN 50341 with a national annex overlay,
IEC 60826, NESC, CSA C22.3, ASCE MOP 74, GO 95); they share
`plugins/codes/common` (reading parameter files, writing the case files,
writing clearance presets — `Preset`, `RuleFile` —, the `code_plugin!`
macro implementing the world). A plugin for the Swiss LeV exists
separately; it is not part of the public code. See
[Design codes](../user-guide/design-codes.md).

### Presets per check

A preset is **per check** (owner, 2026-10-21): applying one brings only
what that check needs, so one big preset may hold the rules of many
checks in a row (dozens of rules, vertical and radial, any targets, each
with its own cases) plus a `[vegetation]` table.

- **Which places list it**: the vegetation panel those with a
  `[vegetation]` table; the Vertical clearances window those with a rule
  with a vertical distance; the Radial window those with a radial one;
  the rule editor every preset. A rule file may **declare** its checks
  instead, at the top level (optional, additive; files without it are
  classified by content as above):

  ```toml
  checks = ["vertical", "radial", "vegetation"]
  ```

  (`ohl_clearance::rules::PresetCheck`, `RuleSet::checks`,
  `RuleSet::is_for`; an unknown name is refused; `ohl code rule-presets`
  prints `for: …`.)
- **What each takes** (`ohl-app` `preset_cases::Part`): the Vertical
  window its vertical rules (those with both distances included), the
  Radial window its radial ones, each with `[wind]` and `[check]`, and
  only the weather cases those rules name (`cases`, `case_pairs`, with
  `*` `?`; a rule naming none uses the project's cases and needs none of
  the preset's); the vegetation panel its `[vegetation]` and `[wind]`, no
  rule, and its own cases — `[vegetation] cases` when given (each must
  name one of the preset's cases), else the preset's cases no rule names,
  else (no rules) all; the rule editor every rule and the cases they
  name (not `[vegetation]`). **No case only another check uses is
  created.** The cases taken are then matched (below).
- The previews group the rules by target (ground, LiDAR classes, the
  line's own wires, other lines) and give per rule the cases it uses, in
  the project's names.

### Matching a preset's weather cases

Wherever the desktop application applies a preset with weather cases —
the rule editor's Load preset…, the Vertical / Radial clearance windows
(the project's preset files and the plugin presets), the vegetation
panel — its cases are matched to the project's **by physics first, then
by name** (owner, 2026-10-21; `ohl-app`'s `preset_cases`):

- **Equivalent** (`ohl_weather::WeatherCase::same_physics`): the same
  wire temperature; the same ice — none on both (a thickness or a load
  of 0, any density), or both a thickness with the same thickness and
  density, or both a load with the same load and density (a density not
  given is glaze ice, 913 kg/m³); the same wind pressure, and with wind
  above 0 the same side (`wind_from`) and air density factor (not given
  equals only not given); the same ambient temperature (likewise).
  Numbers within 1e-6 relative to the larger value (absolute below 1, SI
  units, °C). Name, description and tags never matter. A thickness and a
  load are never equivalent (the weight of a thickness depends on the
  wire's diameter). Conditions (initial / final / creep) and per-span
  cases are not part of a weather case (they are the line's), so they
  are not compared.
- A preset case equivalent to a project case **uses it**: the preset's
  rules are rewritten to name the project's case (`cases`, and each side
  of `case_pairs`; a `*` `?` pattern matching a renamed case becomes the
  names it matched; other entries stay as written, comments kept), and
  it is not created. With several equivalent, the **first in the
  project's order**; the preview says which others are the same.
- Only a case with no equivalent is **created**; one whose name the
  project gives to a non-equivalent case is created as `"<name>
  (preset)"` (then `(preset 2)`, …), the rules naming it so.
- Without a weather case file of its own (the model's still-air 15 °C
  case only), the project has nothing to match: the preset's file is used
  for the model, as before.
- The previews list per preset case "uses your case X" / "creates Y".
- The command line applies no preset to a project: `ohl code rule-presets
  --out` and `ohl code generate` write the preset files only.

## Sheet template presets

*Added 2026-10-20 (built, in active development, not approved yet).* Any
plugin may offer **sheet templates** ([Sheet
templates](../user-guide/sheet-templates.md)): the page, title block with
its logos, band rows, legend, key map, the styles of the line's kinds
and roles — e.g. an authority's plan-submission layout and colour code.
Two ways, either or both (extension point `sheet-templates` in the
manifest):

- **Files, no code**: the manifest lists `.ohlsheet` files inside the
  plugin's folder; their images are read next to them.

  ```toml
  [plugin]
  name = "my-sheets"
  version = "0.1.0"
  sdk = "0.3"
  tier = "wasm"
  entry = "my_sheets.wasm"      # need not exist for files only
  extension-points = ["sheet-templates"]

  [[sheet-template]]
  file = "templates/a1.ohlsheet"
  ```

- **Code**: the optional export `sheet-templates` of `plugin.wit`
  (world `sheet-template-plugin`: `describe` and `sheet-templates`, or
  add `export sheet-templates;` to any world), additive to
  `ohl:plugin@0.1.0` — the interface version stays **0.3**, plugins built
  before load unchanged and give none:

  ```wit
  interface sheet-templates {
      record sheet-image { name: string, media-type: string, bytes: list<u8> }
      record sheet-template {
          name: string,          // unique in the plugin, e.g. "a1-standard"
          description: string,   // shown before it is chosen
          template: string,      // the .ohlsheet text (TOML)
          images: list<sheet-image>,  // the images it names
      }
      sheet-templates: func() -> result<list<sheet-template>, string>;
  }
  ```

The host (`ohl_plugins::WasmHost::sheet_templates`, found with
`discover_sheet_templates`) looks for the export by name
(`ohl:plugin/sheet-templates@0.1.0`, world `sheet-templates-export`), as
for `rule-presets`. Each template is read with the same reader as a file
(`ohl_drawing::SheetTemplate::parse`): one that does not read is shown
with its error, the others are still offered. They are named
`PLUGIN/PRESET` (`ohl drawing --sheet-template PLUGIN/PRESET --plugin
DIR`, `ohl sheet-templates --plugin DIR`) and grouped by plugin in the
export window's template list. Every key a preset may set is in
`examples/sheet-templates/example-a1.ohlsheet`; an authority's colour
code goes in `[line_styles]` (kinds: `hv`, `lv`, `telecom`, `earth`;
`kind_field` and `construction_field` may name a column the plugin adds
to the sections table, e.g. `my-sheets.line_kind`) and `[roles]`
(`existing`, `project`, `removal`, `decommissioned`, `other`, or its
own). The test plugin `plugins/tests/sheet-presets` gives one template
from its code (an A1 with an SVG logo) and one from its manifest
(invented values).

## Check plugins

A plugin that runs a check implements the `check` interface
(`crates/ohl-plugins/wit/code.wit`: the `check-plugin` world, or
`code-check-plugin` for a design-code plugin that also runs checks) and
lists the extension point `check`. **OpenOHL measures, the plugin holds
the values and the rules**: the measuring stays in OpenOHL (it is
geometry, and fast), the code's numbers and its formulas stay out of it.

- `checks()` lists the checks it runs: the **id** of what OpenOHL
  measures for it (today `phase-distance`, the distance between
  conductors in the span), its title and **its Checks submenu** (the app
  builds the Checks menu from the plugins in use: a check's entry is
  there only while its plugin is; a check OpenOHL does not measure for is
  listed, greyed), an
  **explanation** of the computation (shown in its window), its
  **values** file as shipped (TOML) with its extension, the **choices**
  made before a run (e.g. the k diagram), and the plugin columns of the
  circuits its window shows to be set there.
- `value-tables(check, values)` shows a values file as tables (shown
  locked in the window, for transparency); `edit-value(check, values,
  table, row, column, text)` returns the file with that cell changed (an
  admin's edit, kept as the project's copy), or what is wrong.
- `evaluate(check, values, choices, pair, stations)`: for one pair of
  conductors (`conductor-pair`: same system, same span, each one's kV,
  plugin columns, diameter and mass, the mean height, the largest sags
  between spacers when spacers join them) and its stations (6 numbers
  each: distance — between the conductors' axes, as the codes define the
  phase distance, less the polylines' bounds — β, each local sag, each
  l_k), the
  numbers required at
  each station — the distance first, then the check's own (kept with the
  worst station) — and the rule applied.

The host (`ohl_io::checks`, `WasmHost::load_check`) measures
(`ohl_clearance::phase_distance`), calls the plugin once per pair, one
instance per core (`WasmHost::another_check`, compiled once), and keeps
each pair's worst station and the length below — see [Distance between
conductors: the core's part](phase-distance-check.md). A national-code
plugin kept outside the repository runs such a check. On an
existing line (930 conductors, 5593 pairs) a plugin's check takes about
as long as the same computation built into OpenOHL (0.8 s against
0.65–0.9 s; with nearby pairs, about 10 500, 0.9 s against 0.65–0.7 s), plus
about 0.3 s to load the plugin once.

## String limits: what a plugin sees

A string may carry limits that **OpenOHL does not check** — the tension it is rated for and the swings it may take ([String limits](../user-guide/structure-library.md#string-limits-not-checked)). Absent means not given: nothing checks it. Given, nothing in OpenOHL checks it either: a plugin (or a report, or a script) does, where its rules need it. What it gets:

- **Export model** (`io` plugins, `export-model`): the `line` holds every string with its own `max_load` (N), `max_swing_deg`, `max_swing_across_deg`, `max_swing_along_deg` (degrees) — structure types placed and the limits of the assembly a string names filled in where it gives none (absent keys: not given). Each case's `strings` gives, per string, the solved axial force `force_n` (whole bundle, tension positive), its swing `swing_deg` (from straight down), `swing_across_deg` and `swing_along_deg` (the angles of its projections on the structure's planes across and along the line, positive to the right and ahead), and its limits beside them as `max_load_n`, `max_swing_deg`, `max_swing_across_deg`, `max_swing_along_deg` (`null`: not given). A plugin compares, e.g. `abs(swing_across_deg) > max_swing_across_deg`. The format stays version 1: keys were only added.
- **Check plugins** (`check`): the WIT records pass no strings (a check measures conductor pairs), so the interface is unchanged (still `ohl:plugin@0.1.0`); a check that needs the limits reads them from the line as above.
- **Rust** (built-in checks, reports, the server): `ohl_line::InsulatorString::limits()` → `StringLimits`, `Line::string_limits(structure, label)`, and next to each solved string `solve::StringState::{force, limits, swing}` and `jumper::JumperStringShape::{force, limits, swing}`; `ohl_line::swing(orientation_deg, top, bottom)` for any other position. Library side: `InsulatorAssembly::limits()`, `TypeString::limits()` (`ohl_structures::InsulatorLimits`).
- **Python**: the line's JSON (the console's `ohl` module) carries the same `max_*` keys on its strings.

## Python plugins

Python plugins are scripts (`tier = "python"`, `entry` the script). Outside the application they use the `ohl` module built from `crates/ohl-py` (package `openohl`); inside it, the Python console runs them (Plugins › Python console › Plugin scripts) with the `ohl` module of the application, which also reads and edits the open project ([Python console](../user-guide/desktop-app.md#python-console)). See `plugins/examples/hello-python`.

The desktop application embeds Python through its `python` feature (on by default): building it needs a shared Python 3.10+ (`PYO3_PYTHON` chooses which; a static one, such as Anaconda's, cannot be embedded). Without the feature (`--no-default-features`), the console says so.
