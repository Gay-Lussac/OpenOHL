# Coordinate systems

Lines, surveys and point clouds are in a projected coordinate system in
metres (the line file's `crs`, e.g. `"EPSG:2056"`). OLS CAD knows the EPSG
registry and proj4 strings, and transforms points and whole lines between
systems — pure Rust, no native library, the same in WebAssembly plugins.

```bash
ols crs info EPSG:2056
ols crs transform --from EPSG:2056 --to EPSG:4326 2600000 1200000
# 7.438632421 46.951082772
ols crs line line.ols.json --to EPSG:32632 --out line-utm.ols.json
```

- **Geographic** coordinates are longitude, latitude in degrees.
- **Heights are kept** by default: line, survey and LiDAR heights are
  usually orthometric (above a geoid) and no geoid model is applied.
  `--ellipsoidal` treats them as ellipsoidal heights through the datum
  shift (`--points 3` to give X Y Z).
- **A whole line** (`ols crs line`): structure positions, attachment, leg
  and stick points, alignments and jumper supports move together; structure
  orientations follow the rotation between the two grids. Lines stay in
  projected systems. **Grid distances change** with each projection's scale
  (a span of 82.206 m in LV95 is 82.183 m in UTM 32N): check spans and
  stringing after moving a line.

## In the desktop application: Terrain › Coordinate system

- **Define…** says which system the project's coordinates are in — nothing
  moves. Choose it from the **EPSG** list (about 6000 systems; *Show only
  systems containing* filters by name or code, e.g. `LV95`, `UTM zone 32N`,
  `2056`), type a **proj4 string**, or read a **PRJ file**: WKT that names
  its EPSG code (`AUTHORITY["EPSG","2056"]`, WKT 2 `ID["EPSG",2056]`), WKT
  whose name is that of an EPSG system (Esri's `CH1903+_LV95` too), or a
  proj4 string. Other WKT is not read: choose its EPSG system instead.
  Lines are designed in projected systems, so geographic ones (longitude,
  latitude) are refused. It is an edit of the line's settings: undone like
  any other.
- **Coordinate system calculator…** converts points: a system on each side
  (the project's and WGS84 at first), one point per line, `X Y` or
  `X Y Z` (separated by spaces, commas, semicolons or tabs; empty lines and
  lines starting with `#` kept). The results follow as you type: degrees
  with 9 decimals, metres with 4. **Heights are ellipsoidal** sends heights
  through the datum shift (GNSS heights); unticked they are kept.
  **Swap** turns it around (the results become the points). **Copy** puts
  the results on the clipboard.
- **Reproject project…** moves the whole project into another (projected)
  system and writes it as a **new project file** (`<name> <system>.ols`
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

## Accuracy

Datum changes use the Helmert parameters of the definitions, not grid
files.

- **Switzerland, LV95 (EPSG:2056) ↔ WGS84**: matches swisstopo's official
  transformation service REFRAME to under 1 mm in position and height
  (validation cases `crs-001` … `crs-012`), with WGS84 taken as
  CHTRS95/ETRS89 as REFRAME does.
- **Projection changes within one datum** (e.g. WGS84 ↔ UTM) are exact to
  the millimetre (validation case `crs-013`).
- **Datums that need distortion grids** (LV03 ↔ LV95, NAD27 ↔ NAD83, …) are
  approximated by their Helmert parameters; a PROJ-based plugin with grid
  files can provide them behind the same interface.
- With heights kept, a round trip through another datum returns within
  about 1 mm (the datum shift's height change is dropped each way).
