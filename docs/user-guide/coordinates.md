# Coordinate systems

Lines, surveys and point clouds are in a projected coordinate system in
metres (the line file's `crs`, e.g. `"EPSG:2056"`). OpenOHL knows the EPSG
registry and proj4 strings, and transforms points and whole lines between
systems — pure Rust, no native library, the same in WebAssembly plugins.

```bash
ohl crs info EPSG:2056
ohl crs transform --from EPSG:2056 --to EPSG:4326 2600000 1200000
# 7.438632421 46.951082772
ohl crs line line.ohl.json --to EPSG:32632 --out line-utm.ohl.json
```

- **Geographic** coordinates are longitude, latitude in degrees.
- **Heights are kept** by default: line, survey and LiDAR heights are
  usually orthometric (above a geoid) and no geoid model is applied.
  `--ellipsoidal` treats them as ellipsoidal heights through the datum
  shift (`--points 3` to give X Y Z).
- **A whole line** (`ohl crs line`): structure positions, attachment, leg
  and stick points, alignments and jumper supports move together; structure
  orientations follow the rotation between the two grids. Lines stay in
  projected systems. **Grid distances change** with each projection's scale
  (a span of 82.206 m in LV95 is 82.183 m in UTM 32N): check spans and
  stringing after moving a line.

## In the desktop application: Line › Terrain › Coordinate system

- **Define…** says which system the project's coordinates are in — nothing
  moves. Choose it from the **EPSG** list with the
  [coordinate-system picker](#the-coordinate-system-picker), type a
  **proj4 string**, or read a **PRJ file**: WKT that names
  its EPSG code (`AUTHORITY["EPSG","2056"]`, WKT 2 `ID["EPSG",2056]`), WKT
  whose name is that of an EPSG system (also written as `CH1903+_LV95`), or a
  proj4 string. Other WKT is not read: choose its EPSG system instead.
  Lines are designed in projected systems, so geographic ones (longitude,
  latitude) are refused. It is an edit of the line's settings: undone like
  any other. A project whose system no cartographic calculation reads
  (a local one, e.g. `local (invented)`) keeps it while nothing else is
  chosen.

  The same window takes the project's **height datum** — built, in active
  development (not approved yet): the vertical datum its heights are in,
  optional, its **name** (`LN02`, `LHN95`…) and, when it has one, the
  **EPSG code** of its vertical system (`5728`, `EPSG:5728`), either or
  both. Nothing is converted: it says what the heights are. The sheets'
  `{height_datum}` takes it first, then the vertical part of the
  coordinate system (`EPSG:2056+5728` → `EPSG:5728`), then the template's
  text for an empty field ([sheet templates](sheet-templates.md)); the
  window shows what the sheets will read. It is shown in the menu, the
  window and the project's properties (nothing selected), kept in the
  project with the coordinate system (the line settings) and written to
  the line file (`height_datum`, [line files](line-file.md)). On the
  command line it is set as the coordinate system is, with the line
  settings row: `ohl project apply project.ohl --user ann '{"op": "put",
  "kind": "meta", "id": "line", "value": {"name": "…", "crs":
  "EPSG:2056", "height_datum": {"name": "LN02", "epsg": 5728}}}'`
  ([project files](project.md)).
- **Coordinate system calculator…** converts points: a system on each side
  (the project's and WGS84 at first), one point per line, `X Y` or
  `X Y Z` (separated by spaces, commas, semicolons or tabs; empty lines and
  lines starting with `#` kept). The results follow as you type: degrees
  with 9 decimals, metres with 4. **Heights are ellipsoidal** sends heights
  through the datum shift (GNSS heights); unticked they are kept.
  **Swap** turns it around (the results become the points). **Copy** puts
  the results on the clipboard.
- **Reproject project…** moves the whole project into another (projected)
  system and writes it as a **new project file** (`<name> <system>.ohl`
  next to it by default; an existing file is never replaced): every
  project line, every terrain surface (the triangles are
  kept), every point cloud (each point moved and binned again, its
  class, intensity and returns kept; the levels of detail rebuilt) and
  every DXF drawing. Orthophotos are left out (they would need
  resampling): import them again in the new system. The
  project you are in is not changed. It runs in the background with a
  progress bar (LiDAR: about 1 µs per point, so 100 million points take a few
  minutes) and the new project opens when you close the progress window. The
  project's own system must be defined first, and only an admin of the
  project can reproject it. Heights are kept. In the new project the move
  is a log entry of its own that cannot be undone (the terrain would not
  follow): reproject back instead. The history of versions stays in the
  old system.

### The coordinate-system picker

Wherever a coordinate system is chosen in the desktop application — **Define…**,
both sides of the **calculator**, **Reproject project…**, a **new project**,
and **Import LAS/LAZ** into a new point cloud (**Choose…** beside its field) —
the **EPSG** choice is a searchable list:

- **Search** by EPSG code (`2056` or `EPSG:2056`), or by words of the name,
  the area of use or the unit, in any order, case and punctuation ignored
  (`lv95`, `UTM 32N WGS`, `CH1903+_LV95`, `us-ft`). A code typed whole is
  chosen at once; **Enter** takes the first row. An exact code is listed
  first, then the codes starting with the digits typed, then the names
  starting with the words typed, then the rest by code.
- Each row shows the **code**, the **name**, the **unit** (`degree`, `m`,
  `ft`, `us-ft`…, from the system's proj4 definition) and its **area of use**
  where the registry's definition gives one (a few hundred do; for most only
  the name tells where it is used, so a country name finds a system only
  when it is in its name or area). Hovering a row spells it out; the line
  under the list says what was chosen, with its unit.
- Only systems this program can **compute** are listed: the codes of the
  registry whose definition the projection library reads (a code of a
  projection it does not have is left out, so none is offered that would
  fail later). **Geocentric** systems (X, Y, Z from the earth's centre) are
  left out too: a line is not designed in them.

The command line (`--from`, `--to`, `ohl crs info`) takes codes and proj4
strings as before.

## Accuracy

Datum changes use the Helmert parameters of the definitions, not grid
files.

- **Switzerland, LV95 (EPSG:2056) ↔ WGS84**: compared with swisstopo's
  official transformation service REFRAME, with WGS84 taken as
  CHTRS95/ETRS89 as REFRAME does. The benchmark cases `crs-001` …
  `crs-012` (`benchmarks/crs/`) check positions to 1 cm; the tests of
  `ohl-crs` check positions and ellipsoidal heights to 1 cm.
- **Projection changes within one datum** (e.g. WGS84 ↔ UTM) follow the
  projection formulas (benchmark case `crs-013`, checked to 1 cm).
- **Datums that need distortion grids** (LV03 ↔ LV95, NAD27 ↔ NAD83, …) are
  approximated by their Helmert parameters; a PROJ-based plugin with grid
  files can provide them behind the same interface.
- With heights kept, a round trip through another datum returns within
  a few millimetres (about 1 mm in the tests; the datum shift's height
  change is dropped each way).
