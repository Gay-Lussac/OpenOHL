# Import and export

Every file format is an **exporter** (solved line → file) or an **importer** (file → line file). The built-in formats and those of file-format plugins look the same: list them with

```bash
ohl formats [--plugin DIR]
```

| id                   | direction | contents |
|----------------------|-----------|----------|
| `structures-csv`     | both      | `structure,x,y,z,orientation_deg,type,extension`; import: any columns ([neutral files](#neutral-files-points-and-a-route)) |
| `wires-csv`          | export    | `case,section,span,circuit,phase,index,x,y,z` for every weather case (jumpers: section `jumper:<id>`) |
| `spans-csv`          | export    | one row per span and weather case: structures, horizontal span, horizontal and largest tension, sag |
| `kml`, `kmz`         | export    | structures (points) and wires (one folder per weather case) in WGS84 for Google Earth and GIS |
| `ohl-xml`            | both      | the whole line model as [OHL XML](../schema/ohl-line.md); import also reads [LandXML](#landxml) |
| `landxml`            | export    | routes as LandXML 1.2 alignments (with a profile of the alignment points), structures as `CgPoints` |
| `geojson`            | export    | [GIS layers](#gis-layers): structures, routes, wires per weather case with tensions and sag (WGS84) |
| `structures-geojson` | import    | structures from GeoJSON points along a LineString |
| `shapefile`          | export    | the GIS layers as ESRI shapefiles, zipped (`.zip`) |
| `structures-shp`     | import    | structures from shapefile points (`.shp`, or a zip with its `.dbf`) |
| `gpkg`               | export    | the GIS layers as an OGC GeoPackage |
| `ifc`                | export    | [IFC4](#ifc) structures and wires for BIM programs |
| `xlsx`               | export    | [tables](#tables-xlsx) as an Excel workbook |

All of them are written directly by OpenOHL from the public specifications (no GDAL or other native library). Exports take time in proportion to the size of the model (milliseconds for the example line with several weather cases).

## Export

Exports are written by `ohl line` after solving, so they use the same libraries, structure files and weather cases (`--weather`, `--case`, `--tag`):

```bash
ohl line examples/synthetic-line/synthetic-line.ohl.json \
    --export line.kmz --export wires-csv=wires.csv --option crs=EPSG:2056
```

`--export FILE` picks the format from the extension (the first exporter for it: `.csv` → `structures-csv`, `.xml` → `ohl-xml`, `.zip` → `shapefile`); `--export FORMAT=FILE` names it, e.g. `landxml=line.xml`. `--option KEY=VALUE` passes options to the formats (repeatable). In the application, File › Export › The line… lists the same formats.

KML/KMZ options:

- `crs` — the line's coordinate system, when the line file has none (or an informative one such as `local (invented)`). Coordinates go to WGS84 through [ohl-crs](coordinates.md); heights are kept (`altitudeMode` absolute: the line's heights above sea level).
- `cases` — `first` (default) or `all` weather cases.

Wire colours come from the sections ([colours](line-file.md#colours)).

### GIS layers

GeoJSON, Shapefile and GeoPackage hold three layers:

| layer        | geometry       | attributes |
|--------------|----------------|------------|
| `structures` | point (x, y, z) | `id`, `type`, `extension`, `orient_deg`, `routes` |
| `routes`     | line           | `id`, `name`, `structures` (ids in order) |
| `wires`      | line, one per span (or jumper) and weather case | `case`, `section`, `span` (from 1), `circuit`, `phase`, `system`, `voltage_kv`, `color`, `h_tens_n` (horizontal tension, N), `max_tens_n` (largest tension, N), `sag_m` |

Options:

- `coordinates` — `wgs84` (longitude, latitude; heights kept) or `line` (the line's coordinate system). Default: `wgs84` for GeoJSON (as RFC 7946 asks; with `line`, the file names the system in a `crs` member), `line` for shapefiles and GeoPackage.
- `crs` — the line's coordinate system, when the line file has none.
- `cases` — `all` (default) or `first` weather case.

**Shapefiles** come as one zip with `structures`, `routes` and `wires` (`.shp`, `.shx`, `.dbf`, and `.cpg` saying the text is UTF-8); most GIS programs open the zip directly. Point layers are PointZ, lines PolyLineZ. A `.prj` is written for WGS84 output only: for the line's coordinates, set the layer's coordinate system in the GIS program. Text attributes are cut at 254 bytes.

**GeoPackage** (`.gpkg`, version 1.2): feature tables `structures` (POINT Z), `routes` and `wires` (LINESTRING Z). A line coordinate system `EPSG:<code>` is registered under that code (GIS programs take the definition from the code); without one the layers use the undefined Cartesian system. No spatial index is written.

### LandXML

`landxml` writes each route as an `Alignment` — `CoordGeom` straight lines between its alignment points (or its structures, for a route without an alignment), stationing from 0, and a `Profile` with the heights of those points — and every structure as a `CgPoint` named by its id (`code`: its type). Points are northing, easting, elevation, in metres; a coordinate system `EPSG:<code>` is written as `CoordinateSystem`. Terrain surfaces are written and read apart (Line › Terrain › Terrain surfaces).

### IFC

`ifc` writes an IFC4 file (STEP, `ReferenceView`): a project with metre, radian, newton and volt units and a site placed at the first structure. Each structure is an `IfcBuildingElementProxy` placed and turned like the structure, drawn as 3D lines (its stick drawing; else a shaft with arms to the attachment points) and its strings, with property set `OHL_Structure` (id, type, extension, orientation, routes). Each span or jumper is an `IfcCableSegment` (`CONDUCTORSEGMENT`) drawn as a 3D polyline, with property set `OHL_Wire` (case, section, span, cable, circuit, phase, system, nominal voltage, horizontal and largest tension, sag). Option `cases`: `first` (default) or `all`. Coordinates are the line's; no map conversion is written.

### Tables (XLSX)

`xlsx` writes a workbook with the sheets *Structures*, *Sections*, *Spans* (every span and jumper of every weather case: tensions and sag), *Stations* (stationing, offsets, line angles of every route) and, when `ohl line` computes them (`--load-cases`), *Structure loads* and *Structure usage* — the same tables as `--loads-csv` and `--usage-csv`. `spans-csv` writes the *Spans* sheet as CSV.

### Safety volumes

The safety volumes around the wires (Checks › Safety volumes, `ohl
safety-volume`) are written by their own export as DXF (with a `.prj`),
IFC4 with `IfcMapConversion` and KML/KMZ: see
[Safety volumes](analyses.md#safety-volumes-for-work-near-the-line-2026-10-20).

## Import

```bash
ohl import positions.csv --into line.ohl.json --out line-moved.ohl.json
ohl import survey.csv --format structures-csv --out new-line.ohl.json
ohl import line.xml --out line.ohl.json
```

The format comes from `--format` or the file extension. With `--into`, the file updates that line; without, it makes a new line. The result is checked like any line file before it is written. In the application, File › Import › Into the line… reads a file into the open line as one command (undone at once).

Into a project file — a new one, or one that holds a line — as one logged command:

```bash
ohl project import-file points.geojson --into new.ohl --user ann --option crs=EPSG:2056 --option type=tangent
```

`ohl-xml` reads an [OHL XML](../schema/ohl-line.md) line (it replaces the line). It recognises LandXML too: its `CgPoint`s become structures along the first alignment (option `alignment`: by name), as for the [neutral files](#neutral-files-points-and-a-route); a LandXML file with an alignment but no points updates the alignment of a route (option `route`, default the first) of the line it is imported into. Curves and spirals are read by their end points.

### Neutral files: points and a route

`structures-csv`, `structures-geojson`, `structures-shp` and LandXML points are read the same way:

- Every point becomes a structure; a point whose id is a structure of the line moves and turns it — attachments, strings and stick drawings move with it — and keeps its type unless the file gives one.
- With a polyline in the file (a GeoJSON LineString, the first shapefile polyline — a `routes` shapefile in the zip first —, a LandXML alignment), the points are ordered by their station along it, and the route `route` (default `main`) runs through them with the polyline as its alignment. A new line always gets that route (in file order without a polyline).
- New structures without an orientation are turned square to the bisector of the line at them.

| option       | meaning |
|--------------|---------|
| `type`       | structure type of new structures without one (from the [structure library](structure-library.md)) |
| `prefix`     | id prefix of points without an id (default `P`: `P1`, `P2`…) |
| `route`      | the route made or replaced (default `main`) |
| `order`      | `station` (default, along the polyline) or `file` |
| `crs`        | coordinate system of a new line (a line imported into keeps its own) |
| `source_crs` | coordinate system of the file, if it does not say |
| `z`          | height of points without one, m (default 0) |
| `name`       | name of a new line |
| `id`         | GeoJSON property or shapefile column holding the ids (default `id`, `name` or `structure`) |

Coordinates are converted from the file's system to the line's ([ohl-crs](coordinates.md)). GeoJSON is longitude, latitude (WGS84) unless it names a system in a `crs` member; a zipped shapefile with a WGS84 `.prj` is too; CSV and other shapefiles are taken in the line's coordinates (or `source_crs`). Points in longitude and latitude need the line's system: `--option crs=EPSG:<code>` for a new line.

`structures-csv` needs columns `x` and `y`; `structure`, `z`, `orientation_deg`, `type` and `extension` are optional (an empty cell keeps the current value). Other column names are given as `<column>_column` options, the separator with `delimiter` (`,`, `;` or `tab`):

```bash
ohl import survey.csv --format structures-csv --out new.ohl.json \
    --option delimiter=';' --option x_column=Easting --option y_column=Northing --option z_column=H \
    --option structure_column=Nr --option type=tangent
```

## Formats from plugins

A WebAssembly plugin with the extension point `io` adds formats (see [Writing plugins](../development/plugins.md#file-format-plugins)); `--plugin DIR` loads it for `ohl formats`, `ohl import`, `ohl project import-file` and `ohl line --export`. A plugin format with the id of a built-in one replaces it. The example plugin `plugins/examples/geojson` exports GeoJSON and imports structures from GeoJSON points:

```bash
(cd plugins/examples/geojson && cargo build --release --target wasm32-wasip2)
ohl line line.ohl.json --export line.geojson --plugin plugins/examples/geojson
ohl import points.geojson --plugin plugins/examples/geojson --format geojson-structures --into line.ohl.json --out moved.ohl.json
```

A plugin error, a crash, or a plugin exceeding its time or memory limit is reported as the format's error; the line is never half-written.

## Projects from other programs

A **project plugin** reads another program's project file into a new OpenOHL project: the line (structures and attachments, sections and their stringing, circuits as electrical systems, jumpers, point loads, alignment) and its libraries (wires, weather cases, load cases), with notes on what was read, left out or assumed.

- **In the application:** File › **New project from another program…** (also on the start page): the file, the format (default: from the extension), options, then where to save the new `.ohl`. It runs in the background; the project opens with a window of the reader's notes.
- **From the command line:**

  ```bash
  ohl project import-from FILE --into new.ohl --user ann --plugin DIR [--format F] [--option KEY=VALUE] [--clip-in]
  ```

**Clip in** (`--clip-in`, or the window's tick): sections the reader left in the blocks are clipped in at their stringing condition, so each span keeps its length — as programs that sag a section with a ruling span and then solve it with fixed lengths do.

### Into an existing project

File › Import › **Another project into this one…** (2026-10-25) reads
another program's project file the same way and brings it into the
project open — other lines of the same project, a neighbouring line, a
later state of the same line:

1. **The file**: the format (default: from the extension), options,
   *clip in*; **Read the file** (in the background; nothing changes yet).
2. **Into which project line?** The line you work on (one change,
   undoable), or **a new project line made from it** (named; you then work
   on it — to look at the import apart and bring it into another line
   later with Versions › Bring in changes).
3. **Its routes**: each of the import's routes is **this project's route
   …** (its alignments join that route) or **a new route** (renamed if
   its id is taken). A route of an id the project has is taken to be the
   same by default.
4. **What appears in both**: an object of the **same id** with other
   values, or of another id **at the same place** — an alignment point or
   a structure nearer than the tolerance in plan (1 m by default), a
   section over the same structures and sets, a jumper between the same
   ends — and a library file of the same name with another text. Each is
   listed with what differs and settled: **keep the project's** (the
   default: the imported one left out, what the import attached to it
   attached to the project's), **take the imported one** (it replaces the
   project's, under the project's id) or **keep both** (the imported one
   under an id of its own, `T12 (2)`, what refers to it following). *All:*
   settles every one alike. What is the same in both is left as it is;
   the rest is added. The ids the import's objects were given are listed.

**Import** writes the library files, then the line as one change. The
coordinate systems must be the same (else reproject one first). The
renames are made with the project's own rename (what refers to an object
follows as when it is renamed by hand).

Terrain (TIN) and point clouds are imported apart, also from File: **Import › Terrain surface (TIN)…** (Line › Terrain › Terrain surfaces) and **Import › LiDAR points (LAS/LAZ)…**.
