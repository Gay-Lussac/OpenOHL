# Import and export

Every file format is an **exporter** (solved line → file) or an **importer** (file → line file). The built-in formats and those of file-format plugins look the same: list them with

```bash
ols formats [--plugin DIR]
```

| id               | direction | contents |
|------------------|-----------|----------|
| `structures-csv` | both      | `structure,x,y,z,orientation_deg,type,extension` |
| `wires-csv`      | export    | `case,section,span,circuit,phase,index,x,y,z` for every weather case (jumpers: section `jumper:<id>`) |
| `kml`, `kmz`     | export    | structures (points) and wires (one folder per weather case) in WGS84 for Google Earth and GIS |

## Export

Exports are written by `ols line` after solving, so they use the same libraries, pylon files and weather cases (`--weather`, `--case`, `--tag`):

```bash
ols line examples/synthetic-line/synthetic-line.ols.json \
    --export line.kmz --export wires-csv=wires.csv --option crs=EPSG:2056
```

`--export FILE` picks the format from the extension (the first exporter for it: `.csv` → `structures-csv`); `--export FORMAT=FILE` names it. `--option KEY=VALUE` passes options to the formats (repeatable).

KML/KMZ options:

- `crs` — the line's coordinate system, when the line file has none (or an informative one such as `local (invented)`). Coordinates go to WGS84 through [ols-crs](coordinates.md); heights are kept (`altitudeMode` absolute: the line's heights above sea level).
- `cases` — `first` (default) or `all` weather cases.

Wire colours come from the sections ([colours](line-file.md#colours)).

## Import

```bash
ols import positions.csv --into line.ols.json --out line-moved.ols.json
ols import survey.csv --format structures-csv --out new-line.ols.json
```

The format comes from `--format` or the file extension. With `--into`, the file updates that line; without, it makes a new line. The result is checked like any line file before it is written.

`structures-csv` into a line moves and turns the structures it lists — attachments, V-string legs and stick drawings move with them — and adds the new ones (with their `type`, if given). Columns `orientation_deg`, `type` and `extension` are optional; an empty cell keeps the current value.

## Formats from plugins

A WebAssembly plugin with the extension point `io` adds formats (see [Writing plugins](../development/plugins.md#file-format-plugins)); `--plugin DIR` loads it for `ols formats`, `ols import` and `ols line --export`. The example plugin `plugins/examples/geojson` exports GeoJSON and imports structures from GeoJSON points:

```bash
(cd plugins/examples/geojson && cargo build --release --target wasm32-wasip2)
ols line line.ols.json --export line.geojson --plugin plugins/examples/geojson
ols import points.geojson --plugin plugins/examples/geojson --into line.ols.json --out moved.ols.json
```

A plugin error, a crash, or a plugin exceeding its time or memory limit is reported as the format's error; the line is never half-written.

## Projects from other programs

A **project plugin** reads another program's project file into a new OLS CAD project: the line (structures and attachments, sections and their stringing, circuits as electrical systems, jumpers, point loads, alignment) and its libraries (wires, weather cases, load cases), with notes on what was read, left out or assumed.

- **In the application:** File › **New project from another program…** (also on the start page): the file, the format (default: from the extension), options, then where to save the new `.ols`. It runs in the background; the project opens with a window of the reader's notes.
- **From the command line:**

  ```bash
  ols project import-from FILE --into new.ols --user ann --plugin DIR [--format F] [--option KEY=VALUE] [--clip-in]
  ```

**Clip in** (`--clip-in`, or the window's tick): sections the reader left in the blocks are clipped in at their stringing condition, so each span keeps its length — as programs that sag a section with a ruling span and then solve it with fixed lengths do.

Terrain (TIN) and point clouds are imported apart: Terrain › Terrain surfaces, Terrain › Survey data.

